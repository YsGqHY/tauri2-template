//! Structured JSON logging with a standard-library rotating file sink.
//!
//! `init` only constructs a sink; it does not install a global logger or alter
//! the application's main thread. Callers can fall back to [`StderrSink`] when
//! a log directory cannot be opened. Records redact common secret labels before
//! any sink sees them.

use serde::Serialize;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

pub const DEFAULT_FILENAME: &str = "app.log";
pub const DEFAULT_MAX_SIZE_BYTES: u64 = 8 * 1024 * 1024;
pub const DEFAULT_BACKUPS: usize = 3;

/// Severity for a structured log record.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// A sanitized JSON log record.
#[derive(Clone, Debug, Serialize)]
pub struct LogRecord {
    #[serde(rename = "unixMs")]
    unix_ms: u64,
    level: Level,
    component: String,
    message: String,
}

impl LogRecord {
    pub fn new(level: Level, component: impl AsRef<str>, message: impl AsRef<str>) -> Self {
        Self {
            unix_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
                .min(u64::MAX as u128) as u64,
            level,
            component: redact_labeled_secret(component.as_ref()),
            message: redact_labeled_secret(message.as_ref()),
        }
    }

    pub fn unix_ms(&self) -> u64 {
        self.unix_ms
    }

    pub fn level(&self) -> Level {
        self.level
    }

    pub fn component(&self) -> &str {
        &self.component
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

/// Sink interface independent of global logger initialization.
pub trait LogSink: Send + Sync {
    fn emit(&self, record: &LogRecord) -> io::Result<()>;
}

/// Stderr fallback for startup and degraded logging paths.
pub struct StderrSink;

impl LogSink for StderrSink {
    fn emit(&self, record: &LogRecord) -> io::Result<()> {
        let stderr = io::stderr();
        let mut output = stderr.lock();
        serde_json::to_writer(&mut output, record).map_err(io::Error::other)?;
        output.write_all(b"\n")
    }
}

/// The default application sink: `data_dir/logs/app.log`.
pub fn init(data_dir: impl AsRef<Path>) -> io::Result<JsonFileSink> {
    let path = data_dir.as_ref().join("logs").join(DEFAULT_FILENAME);
    JsonFileSink::new(path)
}

/// Emits to the configured sink and falls back to stderr if the file sink fails.
pub fn emit_or_stderr(sink: &dyn LogSink, record: &LogRecord) {
    if sink.emit(record).is_err() {
        let _ = StderrSink.emit(record);
    }
}

/// JSON sink with the default 8 MiB / 3-backup rotation policy.
pub struct JsonFileSink {
    rotating: RotatingFileSink,
}

impl JsonFileSink {
    pub fn new(path: impl Into<PathBuf>) -> io::Result<Self> {
        Ok(Self {
            rotating: RotatingFileSink::new(path, DEFAULT_MAX_SIZE_BYTES, DEFAULT_BACKUPS)?,
        })
    }

    pub fn path(&self) -> &Path {
        self.rotating.path()
    }
}

impl LogSink for JsonFileSink {
    fn emit(&self, record: &LogRecord) -> io::Result<()> {
        self.rotating.emit(record)
    }
}

/// Thread-safe JSON-lines sink with size-based rotation.
pub struct RotatingFileSink {
    path: PathBuf,
    max_size_bytes: u64,
    backups: usize,
    state: Mutex<RotatingFileState>,
}

struct RotatingFileState {
    file: Option<File>,
    size: u64,
}

impl RotatingFileSink {
    pub fn new(path: impl Into<PathBuf>, max_size_bytes: u64, backups: usize) -> io::Result<Self> {
        let path = path.into();
        if path.as_os_str().is_empty() || path.file_name().is_none() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "log path must name a file",
            ));
        }
        let parent = path.parent().filter(|value| !value.as_os_str().is_empty());
        if let Some(parent) = parent {
            fs::create_dir_all(parent)?;
        }
        let (file, size) = open_current(&path)?;
        Ok(Self {
            path,
            max_size_bytes: if max_size_bytes == 0 {
                DEFAULT_MAX_SIZE_BYTES
            } else {
                max_size_bytes
            },
            backups,
            state: Mutex::new(RotatingFileState {
                file: Some(file),
                size,
            }),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn emit(&self, record: &LogRecord) -> io::Result<()> {
        let mut line = serde_json::to_vec(record).map_err(io::Error::other)?;
        line.push(b'\n');
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("log sink lock poisoned"))?;
        ensure_open(&self.path, &mut state)?;
        if state.size > 0 && state.size.saturating_add(line.len() as u64) > self.max_size_bytes {
            rotate_locked(&self.path, self.backups, &mut state)?;
        }
        ensure_open(&self.path, &mut state)?;
        let file = state
            .file
            .as_mut()
            .ok_or_else(|| io::Error::other("log sink is closed"))?;
        file.write_all(&line)?;
        file.flush()?;
        state.size = state.size.saturating_add(line.len() as u64);
        Ok(())
    }
}

impl LogSink for RotatingFileSink {
    fn emit(&self, record: &LogRecord) -> io::Result<()> {
        RotatingFileSink::emit(self, record)
    }
}

fn open_current(path: &Path) -> io::Result<(File, u64)> {
    let mut options = OpenOptions::new();
    options.create(true).append(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    let size = file.metadata()?.len();
    Ok((file, size))
}

fn ensure_open(path: &Path, state: &mut RotatingFileState) -> io::Result<()> {
    if state.file.is_none() {
        let (file, size) = open_current(path)?;
        state.file = Some(file);
        state.size = size;
    }
    Ok(())
}

fn rotate_locked(path: &Path, backups: usize, state: &mut RotatingFileState) -> io::Result<()> {
    if let Some(mut file) = state.file.take() {
        file.flush()?;
        file.sync_data()?;
    }

    if backups == 0 {
        let (file, size) = open_truncated(path)?;
        state.file = Some(file);
        state.size = size;
        return Ok(());
    }

    let expired = backup_path(path, backups + 1);
    remove_if_exists(&expired)?;
    for index in (2..=backups).rev() {
        let from = backup_path(path, index - 1);
        let to = backup_path(path, index);
        rename_if_exists(&from, &to)?;
    }
    let newest = backup_path(path, 1);
    rename_if_exists(path, &newest)?;
    let (file, size) = open_truncated(path)?;
    state.file = Some(file);
    state.size = size;
    Ok(())
}

fn open_truncated(path: &Path) -> io::Result<(File, u64)> {
    let mut options = OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    Ok((options.open(path)?, 0))
}

fn backup_path(path: &Path, index: usize) -> PathBuf {
    PathBuf::from(format!("{}.{}", path.display(), index))
}

fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn rename_if_exists(from: &Path, to: &Path) -> io::Result<()> {
    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn redact_labeled_secret(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    const SENSITIVE_LABELS: &[&str] = &[
        "api_key",
        "api-key",
        "apikey",
        "access_token",
        "access-token",
        "refresh_token",
        "refresh-token",
        "authorization",
        "password",
        "passwd",
        "credential",
        "secret",
        "private_key",
        "private-key",
        ".master.key",
        "cookie",
        "token",
    ];
    if SENSITIVE_LABELS.iter().any(|label| lower.contains(label)) {
        "[REDACTED]".to_owned()
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::fs;
    use std::sync::Arc;
    use std::thread;
    use tempfile::tempdir;

    #[test]
    fn records_have_required_json_shape_and_redact_secrets() {
        let record = LogRecord::new(Level::Info, "worker", "api_key=raw-secret-value");
        let json = serde_json::to_value(&record).expect("serialize record");
        assert!(json.get("unixMs").and_then(Value::as_u64).is_some());
        assert_eq!(json["level"], "INFO");
        assert_eq!(json["message"], "[REDACTED]");
        assert!(!json.to_string().contains("raw-secret-value"));
    }

    #[test]
    fn rotates_and_keeps_configured_backup_count() {
        let directory = tempdir().expect("create temp directory");
        let path = directory.path().join("app.log");
        let sink = RotatingFileSink::new(&path, 128, 3).expect("create sink");
        for index in 0..40 {
            sink.emit(&LogRecord::new(
                Level::Info,
                "test",
                format!("message-{index}"),
            ))
            .expect("write record");
        }
        for index in 1..=3 {
            assert!(path.with_extension(format!("log.{index}")).exists());
        }
        assert!(!path.with_extension("log.4").exists());
        for candidate in fs::read_dir(directory.path()).expect("list logs") {
            let candidate = candidate.expect("read directory entry").path();
            if candidate.is_file() {
                for line in fs::read_to_string(candidate).expect("read log").lines() {
                    let _: Value = serde_json::from_str(line).expect("valid JSON log line");
                }
            }
        }
    }

    #[test]
    fn concurrent_writes_are_serialized_and_json_lines_remain_valid() {
        let directory = tempdir().expect("create temp directory");
        let path = directory.path().join("threaded.log");
        let sink = Arc::new(RotatingFileSink::new(&path, 8 * 1024 * 1024, 3).expect("create sink"));
        let mut threads = Vec::new();
        for thread_id in 0..8 {
            let sink = Arc::clone(&sink);
            threads.push(thread::spawn(move || {
                for index in 0..25 {
                    sink.emit(&LogRecord::new(
                        Level::Debug,
                        format!("thread-{thread_id}"),
                        format!("message-{index}"),
                    ))
                    .expect("write threaded record");
                }
            }));
        }
        for thread in threads {
            thread.join().expect("join logging thread");
        }
        for line in fs::read_to_string(path).expect("read threaded log").lines() {
            let _: Value = serde_json::from_str(line).expect("valid threaded JSON");
        }
    }

    #[test]
    fn long_message_is_written_as_one_valid_json_record() {
        let directory = tempdir().expect("create temp directory");
        let path = directory.path().join("long.log");
        let sink = RotatingFileSink::new(&path, 64, 3).expect("create sink");
        sink.emit(&LogRecord::new(Level::Warn, "test", "x".repeat(1024)))
            .expect("write long record");
        let lines = fs::read_to_string(path).expect("read long log");
        let value: Value = serde_json::from_str(lines.trim()).expect("valid long JSON record");
        assert_eq!(
            value["message"].as_str().expect("message string").len(),
            1024
        );
    }
}
