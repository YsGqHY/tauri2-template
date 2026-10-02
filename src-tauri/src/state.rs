use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex, RwLock};
use std::thread::JoinHandle;

use rusqlite::Connection;

struct LifecycleState {
    shutting_down: bool,
}

use crate::models::{
    ChildWindowInfo, CommandSpec, SubprocessExitPayload, SubprocessInfo, SubprocessSnapshot,
};

pub const SUBPROCESS_HISTORY_CAPACITY: usize = 64;

pub type SharedConnection = Arc<Mutex<Connection>>;

pub struct StorageState {
    pub default_path: PathBuf,
    pub current_path: PathBuf,
    pub config_path: PathBuf,
    pub is_custom: bool,
    pub db: SharedConnection,
}

pub struct ChildWindowRecord {
    pub info: ChildWindowInfo,
}

pub struct SubprocessBuffer {
    pub stdout: VecDeque<String>,
    pub stderr: VecDeque<String>,
}

impl SubprocessBuffer {
    pub const CAPACITY: usize = 256;

    pub fn push(target: &mut VecDeque<String>, line: String) {
        if target.len() >= Self::CAPACITY {
            target.pop_front();
        }
        target.push_back(line);
    }

    pub fn snapshot(&self) -> (Vec<String>, Vec<String>) {
        (
            self.stdout.iter().cloned().collect(),
            self.stderr.iter().cloned().collect(),
        )
    }
}

pub struct ManagedSubprocess {
    pub info: SubprocessInfo,
    /// Per-process cancellation signal, equivalent to a Go context cancellation channel.
    pub cancel_requested: Arc<AtomicBool>,
    pub buffer: Arc<Mutex<SubprocessBuffer>>,
}

pub struct AppState {
    pub storage: RwLock<StorageState>,
    pub child_windows: Mutex<HashMap<String, ChildWindowRecord>>,
    pub subprocesses: Mutex<HashMap<String, ManagedSubprocess>>,
    pub subprocess_history: Mutex<HashMap<String, SubprocessSnapshot>>,
    pub command_whitelist: RwLock<Vec<CommandSpec>>,
    pub stop_requested: Arc<AtomicBool>,
    pub next_subprocess_id: AtomicU64,
    pub background_tasks: Mutex<Vec<JoinHandle<()>>>,
    /// Serializes shutdown acceptance with worker registration. It is never held
    /// during process termination or thread joins.
    lifecycle: Mutex<LifecycleState>,
}

impl AppState {
    pub fn new(storage: StorageState) -> Self {
        Self {
            storage: RwLock::new(storage),
            child_windows: Mutex::new(HashMap::new()),
            subprocesses: Mutex::new(HashMap::new()),
            subprocess_history: Mutex::new(HashMap::new()),
            command_whitelist: RwLock::new(Vec::new()),
            stop_requested: Arc::new(AtomicBool::new(false)),
            next_subprocess_id: AtomicU64::new(1),
            background_tasks: Mutex::new(Vec::new()),
            lifecycle: Mutex::new(LifecycleState {
                shutting_down: false,
            }),
        }
    }

    pub fn is_shutting_down(&self) -> bool {
        self.lifecycle
            .lock()
            .map(|lifecycle| lifecycle.shutting_down)
            .unwrap_or(true)
    }

    /// Register a worker atomically with respect to application shutdown.
    ///
    /// The lifecycle gate is held only while checking shutdown and moving the handle
    /// into the registry. Completed handles are removed first and joined after all
    /// mutexes are released. An `Err` returns ownership of the worker to the caller.
    pub fn register_background_task(&self, task: JoinHandle<()>) -> Result<(), JoinHandle<()>> {
        let lifecycle = match self.lifecycle.lock() {
            Ok(lifecycle) => lifecycle,
            Err(_) => return Err(task),
        };
        if lifecycle.shutting_down {
            return Err(task);
        }

        let mut completed = Vec::new();
        let mut task = Some(task);
        let registry_result = self.background_tasks.lock();
        let Ok(mut registry) = registry_result else {
            return Err(task.take().expect("worker handle must be present"));
        };
        let mut active = Vec::with_capacity(registry.len() + 1);
        for existing in registry.drain(..) {
            if existing.is_finished() {
                completed.push(existing);
            } else {
                active.push(existing);
            }
        }
        if let Some(task) = task.take() {
            if task.is_finished() {
                completed.push(task);
            } else {
                active.push(task);
            }
        }
        *registry = active;
        drop(registry);
        drop(lifecycle);

        for completed in completed {
            let _ = completed.join();
        }
        Ok(())
    }

    pub fn next_subprocess_id(&self) -> String {
        format!(
            "process-{}",
            self.next_subprocess_id
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        )
    }

    /// Request application-wide cancellation and join every registered worker.
    ///
    /// `stop_requested` is the shutdown equivalent of a Go context cancellation signal.
    /// The lifecycle gate closes acceptance before controls/JoinHandles are copied/taken.
    /// Registry, buffer/history, and background-task mutexes are never held across OS
    /// termination or thread joins. Calling this more than once is intentionally idempotent.
    pub fn stop_and_join(&self) {
        if let Ok(mut lifecycle) = self.lifecycle.lock() {
            lifecycle.shutting_down = true;
        }
        self.stop_requested
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let controls = self
            .subprocesses
            .lock()
            .map(|processes| {
                processes
                    .values()
                    .map(|process| process.cancel_requested.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        for control in controls {
            control.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        let tasks = self
            .background_tasks
            .lock()
            .map(|mut tasks| std::mem::take(&mut *tasks))
            .unwrap_or_default();
        for task in tasks {
            let _ = task.join();
        }
    }
}

pub fn snapshot_from_active(process: &ManagedSubprocess) -> SubprocessSnapshot {
    let (stdout, stderr) = process
        .buffer
        .lock()
        .map(|buffer| buffer.snapshot())
        .unwrap_or_default();
    SubprocessSnapshot {
        info: process.info.clone(),
        stdout,
        stderr,
        exit: None,
    }
}

pub fn snapshot_from_exit(
    mut info: SubprocessInfo,
    buffer: &Arc<Mutex<SubprocessBuffer>>,
    exit: SubprocessExitPayload,
) -> SubprocessSnapshot {
    info.running = false;
    info.exit_code = exit.code;
    info.success = Some(exit.success);
    let (stdout, stderr) = buffer
        .lock()
        .map(|buffer| buffer.snapshot())
        .unwrap_or_default();
    SubprocessSnapshot {
        info,
        stdout,
        stderr,
        exit: Some(exit),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::SubprocessExitPayload;
    use std::sync::atomic::Ordering;

    fn info() -> SubprocessInfo {
        SubprocessInfo {
            id: "process-1".into(),
            command: "test".into(),
            args: Vec::new(),
            cwd: None,
            running: true,
            exit_code: None,
            success: None,
        }
    }

    #[test]
    fn snapshot_from_exit_preserves_bounded_streams_and_exit() {
        let mut buffer = SubprocessBuffer {
            stdout: VecDeque::new(),
            stderr: VecDeque::new(),
        };
        for index in 0..(SubprocessBuffer::CAPACITY + 3) {
            SubprocessBuffer::push(&mut buffer.stdout, format!("out-{index}"));
            SubprocessBuffer::push(&mut buffer.stderr, format!("err-{index}"));
        }
        let snapshot = snapshot_from_exit(
            info(),
            &Arc::new(Mutex::new(buffer)),
            SubprocessExitPayload {
                id: "process-1".into(),
                code: Some(7),
                success: false,
            },
        );
        assert!(!snapshot.info.running);
        assert_eq!(snapshot.info.exit_code, Some(7));
        assert_eq!(snapshot.stdout.len(), SubprocessBuffer::CAPACITY);
        assert_eq!(snapshot.stderr.len(), SubprocessBuffer::CAPACITY);
        assert_eq!(snapshot.stdout.first().map(String::as_str), Some("out-3"));
        assert_eq!(snapshot.exit.unwrap().code, Some(7));
    }

    #[test]
    fn stop_and_join_is_idempotent_without_holding_task_mutex() {
        let root = tempfile::tempdir().expect("tempdir");
        let state =
            AppState::new(crate::storage::initialize(root.path().to_path_buf()).expect("storage"));
        let stop = state.stop_requested.clone();
        let handle = std::thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                std::thread::yield_now();
            }
        });
        state.background_tasks.lock().unwrap().push(handle);
        state.stop_and_join();
        state.stop_and_join();
        assert!(state.stop_requested.load(Ordering::SeqCst));
        assert!(state.background_tasks.lock().unwrap().is_empty());
    }

    #[test]
    fn shutdown_gate_rejects_late_worker_registration() {
        let root = tempfile::tempdir().expect("tempdir");
        let state =
            AppState::new(crate::storage::initialize(root.path().to_path_buf()).expect("storage"));
        state.stop_and_join();
        let worker = std::thread::spawn(|| {});
        let worker = state
            .register_background_task(worker)
            .expect_err("shutdown must reject new workers");
        worker.join().unwrap();
        assert!(state.background_tasks.lock().unwrap().is_empty());
    }

    #[test]
    fn registration_prunes_finished_workers_without_retaining_handles() {
        let root = tempfile::tempdir().expect("tempdir");
        let state =
            AppState::new(crate::storage::initialize(root.path().to_path_buf()).expect("storage"));
        let finished = std::thread::spawn(|| {});
        while !finished.is_finished() {
            std::thread::yield_now();
        }
        state.background_tasks.lock().unwrap().push(finished);
        let stop = state.stop_requested.clone();
        let active = std::thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                std::thread::yield_now();
            }
        });
        state.register_background_task(active).unwrap();
        assert_eq!(state.background_tasks.lock().unwrap().len(), 1);
        state.stop_and_join();
        assert!(state.background_tasks.lock().unwrap().is_empty());
    }
}
