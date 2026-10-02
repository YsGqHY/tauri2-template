use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};

#[path = "process_group.rs"]
mod process_group;

const DEFAULT_STOP_GRACE: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const FORCE_KILL_WAIT: Duration = Duration::from_secs(2);
const READER_SHUTDOWN_WAIT: Duration = Duration::from_secs(2);
const MAX_OUTPUT_LINE_BYTES: usize = 1 << 20;
const OUTPUT_TRUNCATION_MARKER: &str = "… [truncated]";

use crate::error::{AppError, AppResult};
use crate::events;
use crate::models::{
    CommandSpec, SubprocessExitPayload, SubprocessInfo, SubprocessRequest, SubprocessSnapshot,
};
use crate::state::{
    snapshot_from_active, snapshot_from_exit, AppState, ManagedSubprocess, SubprocessBuffer,
    SUBPROCESS_HISTORY_CAPACITY,
};

pub fn available_commands(state: &AppState) -> AppResult<Vec<CommandSpec>> {
    let commands = state
        .command_whitelist
        .read()
        .map_err(|_| AppError::new("STATE_LOCK", "命令白名单锁定失败"))?;
    Ok(commands.clone())
}

pub fn run(
    app: &AppHandle,
    state: &AppState,
    request: SubprocessRequest,
) -> AppResult<SubprocessInfo> {
    if state.is_shutting_down() {
        return Err(AppError::new(
            "SUBPROCESS_SHUTTING_DOWN",
            "应用正在关闭，拒绝新的子进程",
        ));
    }
    let spec = {
        let commands = state
            .command_whitelist
            .read()
            .map_err(|_| AppError::new("STATE_LOCK", "命令白名单锁定失败"))?;
        validate_request(&commands, &request)?
    };
    let cwd = validate_cwd(spec.cwd_root.as_deref(), request.cwd.as_deref())?;
    let mut command = Command::new(&spec.executable);
    command
        .args(&request.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    process_group::configure_command(&mut command);
    if let Some(cwd) = &cwd {
        command.current_dir(cwd);
    }
    let mut child = command.spawn().map_err(|error| {
        AppError::with_detail("SUBPROCESS_START", "子进程启动失败", error.to_string())
    })?;
    // Child remains exclusively owned by the worker. Windows attaches a JobObject here;
    // if that cannot be done, the group helper retains a taskkill fallback and its known
    // post-spawn assignment race rather than pretending the guarantees are equivalent.
    let process_group = process_group::ProcessGroup::attach(&child)
        .unwrap_or_else(|_| process_group::ProcessGroup::fallback(child.id()));
    let process_stop = Arc::new(AtomicBool::new(false));
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let id = state.next_subprocess_id();
    let info = SubprocessInfo {
        id: id.clone(),
        command: request.command.clone(),
        args: request.args.clone(),
        cwd: cwd.as_ref().map(|path| path.to_string_lossy().into_owned()),
        running: true,
        exit_code: None,
        success: None,
    };
    let buffer = Arc::new(Mutex::new(SubprocessBuffer {
        stdout: Default::default(),
        stderr: Default::default(),
    }));
    let registry_result = state.subprocesses.lock().map(|mut processes| {
        processes.insert(
            id.clone(),
            ManagedSubprocess {
                info: info.clone(),
                cancel_requested: process_stop.clone(),
                buffer: buffer.clone(),
            },
        );
    });
    if registry_result.is_err() {
        process_stop.store(true, Ordering::SeqCst);
        let _ = process_group.kill(&mut child);
        let _ = child.wait();
        return Err(AppError::new("STATE_LOCK", "子进程状态锁定失败"));
    }

    // Start both readers only after spawn succeeds. They consume stdout/stderr in parallel
    // so a noisy child cannot deadlock on either pipe. Each task has a done channel so the
    // worker can bound shutdown instead of joining an inherited pipe forever.
    let mut readers = Vec::new();
    if let Some(stdout) = stdout {
        readers.push(spawn_reader(
            stdout,
            app.clone(),
            id.clone(),
            buffer.clone(),
            true,
        ));
    }
    if let Some(stderr) = stderr {
        readers.push(spawn_reader(
            stderr,
            app.clone(),
            id.clone(),
            buffer.clone(),
            false,
        ));
    }

    // Ready is emitted before the worker starts waiting, so an immediately exiting child
    // cannot publish subprocess:exit before the caller has observed subprocess:ready.
    let app = app.clone();
    let _ = events::emit_subprocess_ready(&app, &id);
    let state_handle = app.clone();
    let process_id = id.clone();
    let shutdown = state.stop_requested.clone();
    let worker_cancel = process_stop.clone();
    let worker = thread::spawn(move || {
        // The worker exclusively owns Child and ProcessGroup. Commands only copy an
        // AtomicBool cancel signal; no registry/buffer mutex is held during OS waits,
        // graceful termination, force kill, or reader joins.
        let status = wait_owned_child(
            &mut child,
            &process_group,
            &worker_cancel,
            &shutdown,
            DEFAULT_STOP_GRACE,
        )
        .ok();
        let detached_readers = shutdown_readers(readers, READER_SHUTDOWN_WAIT);
        if detached_readers > 0 {
            eprintln!(
                "subprocess {process_id}: detached {detached_readers} reader task(s) after bounded shutdown"
            );
        }
        let payload = SubprocessExitPayload {
            id: process_id.clone(),
            code: status.as_ref().and_then(|status| status.code()),
            success: status
                .as_ref()
                .map(|status| status.success())
                .unwrap_or(false),
        };
        if let Some(state) = state_handle.try_state::<AppState>() {
            if let Ok(mut processes) = state.subprocesses.lock() {
                if let Some(managed) = processes.get(&process_id) {
                    let snapshot =
                        snapshot_from_exit(managed.info.clone(), &managed.buffer, payload.clone());
                    if let Ok(mut history) = state.subprocess_history.lock() {
                        if history.len() >= SUBPROCESS_HISTORY_CAPACITY {
                            let oldest = history
                                .keys()
                                .min_by_key(|id| {
                                    id.trim_start_matches("process-")
                                        .parse::<u64>()
                                        .unwrap_or(0)
                                })
                                .cloned();
                            if let Some(oldest) = oldest {
                                history.remove(&oldest);
                            }
                        }
                        history.insert(process_id.clone(), snapshot);
                        processes.remove(&process_id);
                    }
                }
            }
        }
        let _ = events::emit_subprocess_exit(&app, payload);
    });
    if let Err(worker) = state.register_background_task(worker) {
        process_stop.store(true, Ordering::SeqCst);
        let _ = worker.join();
        let code = if state.is_shutting_down() {
            "SUBPROCESS_SHUTTING_DOWN"
        } else {
            "STATE_LOCK"
        };
        return Err(AppError::new(code, "应用正在关闭，拒绝新的子进程"));
    }
    Ok(info)
}

fn wait_owned_child(
    child: &mut Child,
    process_group: &process_group::ProcessGroup,
    control: &AtomicBool,
    global_stop: &AtomicBool,
    grace: Duration,
) -> io::Result<ExitStatus> {
    let grace = normalize_grace(grace);
    let mut graceful_deadline = None;

    loop {
        if let Some(status) = child.try_wait()? {
            // Always close the process group before joining readers. A descendant may
            // still hold an inherited pipe even when the direct child has exited.
            let _ = process_group.kill(child);
            let _ = child.wait();
            return Ok(status);
        }

        let cancellation_requested =
            control.load(Ordering::SeqCst) || global_stop.load(Ordering::SeqCst);
        if cancellation_requested && graceful_deadline.is_none() {
            // The cancellation flag is the Rust equivalent of Go context cancellation.
            // Only this worker touches Child/ProcessGroup, making repeated stop requests
            // naturally idempotent and avoiding lock-held waits.
            let _ = process_group.terminate(child);
            graceful_deadline = Some(Instant::now() + grace);
        }

        if let Some(deadline) = graceful_deadline {
            if Instant::now() >= deadline {
                return force_kill_and_reap(child, process_group);
            }
        }

        thread::sleep(POLL_INTERVAL);
    }
}

fn force_kill_and_reap(
    child: &mut Child,
    process_group: &process_group::ProcessGroup,
) -> io::Result<ExitStatus> {
    // First kill the whole group/job, then poll briefly for the OS to report exit.
    let _ = process_group.kill(child);
    let deadline = Instant::now() + FORCE_KILL_WAIT;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            break;
        }
        thread::sleep(POLL_INTERVAL);
    }

    // Guaranteed direct-child kill + wait is the final reap path. This wait is reached
    // only after bounded polling and prevents Unix zombies even if group termination
    // raced or a platform fallback could not observe the descendant tree.
    let _ = child.kill();
    child.wait()
}

fn normalize_grace(grace: Duration) -> Duration {
    if grace.is_zero() {
        DEFAULT_STOP_GRACE
    } else {
        grace
    }
}

struct ReaderTask {
    done: Receiver<()>,
    handle: Option<JoinHandle<()>>,
    stream: &'static str,
}

fn spawn_reader<R>(
    reader: R,
    app: AppHandle,
    id: String,
    buffer: Arc<Mutex<SubprocessBuffer>>,
    stdout: bool,
) -> ReaderTask
where
    R: Read + Send + 'static,
{
    let (done_tx, done_rx) = mpsc::sync_channel(1);
    let stream = if stdout { "stdout" } else { "stderr" };
    let handle = thread::spawn(move || {
        let mut reader = BufReader::new(reader);
        while let Ok(Some(line)) = read_bounded_line(&mut reader) {
            record_output(&app, &id, &buffer, stdout, line);
        }
        let _ = done_tx.send(());
    });
    ReaderTask {
        done: done_rx,
        handle: Some(handle),
        stream,
    }
}

fn shutdown_readers(readers: Vec<ReaderTask>, timeout: Duration) -> usize {
    let deadline = Instant::now() + timeout;
    let mut detached = 0;
    for mut reader in readers {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match reader.done.recv_timeout(remaining) {
            Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                if let Some(handle) = reader.handle.take() {
                    let _ = handle.join();
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                eprintln!(
                    "subprocess reader {} did not stop before bounded shutdown deadline",
                    reader.stream
                );
                // Dropping JoinHandle detaches only as a last resort. The process group
                // was already terminated; this prevents application shutdown from being
                // blocked forever by a broken inherited pipe.
                drop(reader.handle.take());
                detached += 1;
            }
        }
    }
    detached
}

fn read_bounded_line<R: BufRead>(reader: &mut R) -> io::Result<Option<String>> {
    let mut bytes = Vec::with_capacity(MAX_OUTPUT_LINE_BYTES.min(64 * 1024));
    let mut truncated = false;
    loop {
        let chunk = reader.fill_buf()?;
        if chunk.is_empty() {
            if bytes.is_empty() {
                return Ok(None);
            }
            break;
        }
        if let Some(newline) = chunk.iter().position(|byte| *byte == b'\n') {
            append_bounded(&mut bytes, &chunk[..newline], &mut truncated);
            reader.consume(newline + 1);
            break;
        }
        append_bounded(&mut bytes, chunk, &mut truncated);
        let consumed = chunk.len();
        reader.consume(consumed);
    }

    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    let mut line = String::from_utf8_lossy(&bytes).into_owned();
    if line.len() > MAX_OUTPUT_LINE_BYTES {
        truncated = true;
        truncate_utf8(&mut line, MAX_OUTPUT_LINE_BYTES);
    }
    if truncated {
        let keep = MAX_OUTPUT_LINE_BYTES.saturating_sub(OUTPUT_TRUNCATION_MARKER.len());
        truncate_utf8(&mut line, keep);
        line.push_str(OUTPUT_TRUNCATION_MARKER);
    }
    Ok(Some(line))
}

fn append_bounded(target: &mut Vec<u8>, source: &[u8], truncated: &mut bool) {
    if *truncated {
        return;
    }
    let remaining = MAX_OUTPUT_LINE_BYTES.saturating_sub(target.len());
    if source.len() <= remaining {
        target.extend_from_slice(source);
    } else {
        target.extend_from_slice(&source[..remaining]);
        *truncated = true;
    }
}

fn truncate_utf8(value: &mut String, max_bytes: usize) {
    if value.len() <= max_bytes {
        return;
    }
    let mut end = max_bytes;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
}

fn record_output(
    app: &AppHandle,
    id: &str,
    buffer: &Arc<Mutex<SubprocessBuffer>>,
    stdout: bool,
    line: String,
) {
    if let Ok(mut buffer) = buffer.lock() {
        if stdout {
            SubprocessBuffer::push(&mut buffer.stdout, line.clone());
        } else {
            SubprocessBuffer::push(&mut buffer.stderr, line.clone());
        }
    }
    let _ = if stdout {
        events::emit_subprocess_stdout(app, id, line)
    } else {
        events::emit_subprocess_stderr(app, id, line)
    };
}

pub fn stop(state: &AppState, id: &str) -> AppResult<()> {
    // Copy the cancellation handle and release the registry mutex before signalling.
    // The worker owns all OS operations; repeated and already-exited stops are no-ops.
    let control = state
        .subprocesses
        .lock()
        .map_err(|_| AppError::new("STATE_LOCK", "子进程状态锁定失败"))?
        .get(id)
        .map(|process| process.cancel_requested.clone());
    if let Some(control) = control {
        control.store(true, Ordering::SeqCst);
    }
    Ok(())
}

pub fn list(state: &AppState) -> AppResult<Vec<SubprocessInfo>> {
    let active = state
        .subprocesses
        .lock()
        .map_err(|_| AppError::new("STATE_LOCK", "子进程状态锁定失败"))?;
    let history = state
        .subprocess_history
        .lock()
        .map_err(|_| AppError::new("STATE_LOCK", "子进程历史锁定失败"))?;
    let mut result = active
        .values()
        .map(|process| process.info.clone())
        .collect::<Vec<_>>();
    result.extend(history.values().map(|snapshot| snapshot.info.clone()));
    Ok(result)
}

pub fn get_output(state: &AppState, id: &str) -> AppResult<SubprocessSnapshot> {
    // All registry operations take active before history, matching the worker's
    // atomic active-to-terminal transition; no disappeared-ID replay gap.
    let active = state
        .subprocesses
        .lock()
        .map_err(|_| AppError::new("STATE_LOCK", "子进程状态锁定失败"))?;
    if let Some(process) = active.get(id) {
        return Ok(snapshot_from_active(process));
    }
    let history = state
        .subprocess_history
        .lock()
        .map_err(|_| AppError::new("STATE_LOCK", "子进程历史锁定失败"))?;
    history
        .get(id)
        .cloned()
        .ok_or_else(|| AppError::new("SUBPROCESS_NOT_FOUND", "子进程不存在"))
}

fn validate_request(specs: &[CommandSpec], request: &SubprocessRequest) -> AppResult<CommandSpec> {
    let spec = specs
        .iter()
        .find(|spec| spec.id == request.command)
        .cloned()
        .ok_or_else(|| AppError::new("COMMAND_NOT_ALLOWED", "命令不在安全白名单中"))?;
    if request.args.len() > spec.max_args || request.args.len() != spec.arg_patterns.len() {
        return Err(AppError::new(
            "ARGS_NOT_ALLOWED",
            "命令参数数量不符合白名单固定模式",
        ));
    }
    for (argument, pattern) in request.args.iter().zip(spec.arg_patterns.iter()) {
        if !pattern.matches(argument) {
            return Err(AppError::new(
                "ARGS_NOT_ALLOWED",
                "命令参数不匹配白名单模式",
            ));
        }
    }
    Ok(spec)
}

fn validate_cwd(root: Option<&Path>, cwd: Option<&Path>) -> AppResult<Option<PathBuf>> {
    let candidate = cwd.or(root);
    let Some(candidate) = candidate else {
        return Ok(None);
    };
    let canonical_candidate = candidate.canonicalize().map_err(|error| {
        AppError::with_detail("CWD_INVALID", "工作目录不存在或不可访问", error.to_string())
    })?;
    if let Some(root) = root {
        let canonical_root = root.canonicalize().map_err(|error| {
            AppError::with_detail(
                "CWD_ROOT_INVALID",
                "白名单工作目录根不可访问",
                error.to_string(),
            )
        })?;
        if !canonical_candidate.starts_with(&canonical_root) {
            return Err(AppError::new("CWD_NOT_ALLOWED", "工作目录超出白名单根目录"));
        }
    }
    Ok(Some(canonical_candidate))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ArgPattern, CommandSpec};
    use std::io::Cursor;

    fn spec() -> CommandSpec {
        CommandSpec {
            id: "echo".into(),
            executable: "echo".into(),
            arg_patterns: vec![ArgPattern {
                kind: "regex".into(),
                value: Some(r"^ok-[0-9]+$".into()),
            }],
            max_args: 1,
            cwd_root: None,
        }
    }

    #[test]
    fn rejects_commands_not_in_whitelist() {
        let request = SubprocessRequest {
            command: "rm".into(),
            args: vec![],
            cwd: None,
        };
        assert_eq!(
            validate_request(&[spec()], &request).unwrap_err().code,
            "COMMAND_NOT_ALLOWED"
        );
    }

    #[test]
    fn rejects_argument_pattern_mismatch() {
        let request = SubprocessRequest {
            command: "echo".into(),
            args: vec!["no".into()],
            cwd: None,
        };
        assert_eq!(
            validate_request(&[spec()], &request).unwrap_err().code,
            "ARGS_NOT_ALLOWED"
        );
    }

    #[test]
    fn accepts_regex_pattern_and_rejects_fixed_count_mismatch() {
        let request = SubprocessRequest {
            command: "echo".into(),
            args: vec!["ok-42".into()],
            cwd: None,
        };
        assert!(validate_request(&[spec()], &request).is_ok());
        let request = SubprocessRequest {
            command: "echo".into(),
            args: vec![],
            cwd: None,
        };
        assert_eq!(
            validate_request(&[spec()], &request).unwrap_err().code,
            "ARGS_NOT_ALLOWED"
        );
    }

    #[test]
    fn output_buffer_is_bounded() {
        let mut buffer = SubprocessBuffer {
            stdout: Default::default(),
            stderr: Default::default(),
        };
        for index in 0..(SubprocessBuffer::CAPACITY + 5) {
            SubprocessBuffer::push(&mut buffer.stdout, index.to_string());
        }
        assert_eq!(buffer.stdout.len(), SubprocessBuffer::CAPACITY);
        assert_eq!(buffer.stdout.front().map(String::as_str), Some("5"));
    }

    #[test]
    fn bounded_reader_consumes_long_line_and_keeps_next_line() {
        let mut input = vec![b'x'; MAX_OUTPUT_LINE_BYTES + 128];
        input.extend_from_slice(b"\nnext\n");
        let mut reader = BufReader::new(Cursor::new(input));
        let line = read_bounded_line(&mut reader).unwrap().unwrap();
        assert!(line.len() <= MAX_OUTPUT_LINE_BYTES);
        assert!(line.ends_with(OUTPUT_TRUNCATION_MARKER));
        assert_eq!(
            read_bounded_line(&mut reader).unwrap().as_deref(),
            Some("next")
        );
    }

    #[test]
    fn bounded_reader_handles_eof_without_newline() {
        let input = vec![b'z'; MAX_OUTPUT_LINE_BYTES];
        let mut reader = BufReader::new(Cursor::new(input));
        let line = read_bounded_line(&mut reader).unwrap().unwrap();
        assert_eq!(line.len(), MAX_OUTPUT_LINE_BYTES);
        assert!(read_bounded_line(&mut reader).unwrap().is_none());
    }

    #[test]
    fn zero_grace_uses_default_but_custom_grace_is_preserved() {
        assert_eq!(normalize_grace(Duration::ZERO), DEFAULT_STOP_GRACE);
        let custom = Duration::from_millis(37);
        assert_eq!(normalize_grace(custom), custom);
    }

    #[test]
    fn stop_is_idempotent_for_missing_and_historical_processes() {
        let root = tempfile::tempdir().expect("tempdir");
        let state =
            AppState::new(crate::storage::initialize(root.path().to_path_buf()).expect("storage"));
        assert!(stop(&state, "process-missing").is_ok());
        state.subprocess_history.lock().unwrap().insert(
            "process-done".into(),
            SubprocessSnapshot {
                info: SubprocessInfo {
                    id: "process-done".into(),
                    command: "test".into(),
                    args: Vec::new(),
                    cwd: None,
                    running: false,
                    exit_code: Some(0),
                    success: Some(true),
                },
                stdout: Vec::new(),
                stderr: Vec::new(),
                exit: None,
            },
        );
        assert!(stop(&state, "process-done").is_ok());
        assert!(stop(&state, "process-done").is_ok());
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn cancellation_reaches_child_within_grace_and_force_window() {
        let mut command = test_long_running_command();
        process_group::configure_command(&mut command);
        let mut child = command.spawn().expect("spawn test child");
        let group = process_group::ProcessGroup::attach(&child)
            .unwrap_or_else(|_| process_group::ProcessGroup::fallback(child.id()));
        let cancel = AtomicBool::new(true);
        let shutdown = AtomicBool::new(false);
        let started = Instant::now();
        let status = wait_owned_child(
            &mut child,
            &group,
            &cancel,
            &shutdown,
            Duration::from_millis(50),
        )
        .expect("bounded child termination");
        assert!(started.elapsed() < DEFAULT_STOP_GRACE + FORCE_KILL_WAIT);
        assert!(!status.success());
    }

    #[cfg(unix)]
    fn test_long_running_command() -> Command {
        let mut command = Command::new("sh");
        command.args(["-c", "trap 'exit 0' TERM; while :; do sleep 1; done"]);
        command
    }

    #[cfg(windows)]
    fn test_long_running_command() -> Command {
        let mut command = Command::new("cmd");
        command.args(["/C", "ping 127.0.0.1 -n 30 >NUL"]);
        command
    }
}
