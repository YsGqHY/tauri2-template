//! Bounded file I/O helpers with same-directory atomic replacement.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Creates a directory and its parents. An empty path is rejected explicitly.
pub fn ensure_dir(path: impl AsRef<Path>) -> io::Result<()> {
    let path = path.as_ref();
    if path.as_os_str().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "directory path must not be empty",
        ));
    }
    fs::create_dir_all(path)
}

/// Reads at most `max_bytes`; files that exceed the bound return `InvalidData`.
///
/// The read itself is bounded too, so a concurrent file growth cannot cause an
/// unbounded allocation after the initial metadata check.
pub fn read_limit(path: impl AsRef<Path>, max_bytes: u64) -> io::Result<Vec<u8>> {
    let file = File::open(path)?;
    if file.metadata()?.len() > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "file exceeds configured read limit",
        ));
    }

    let read_limit = max_bytes.saturating_add(1);
    let mut bytes = Vec::new();
    file.take(read_limit).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max_bytes {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "file exceeds configured read limit",
        ));
    }
    Ok(bytes)
}

/// Writes bytes through a unique temporary file beside `path`, syncs the file,
/// then renames it into place. Parent-directory syncing is best-effort because
/// some platforms do not permit opening directories as files.
///
/// New files use owner-only permissions on Unix. Existing destination
/// permissions are not preserved; callers that need a different mode should
/// apply it explicitly after writing.
pub fn write_atomic(path: impl AsRef<Path>, bytes: &[u8]) -> io::Result<()> {
    let path = path.as_ref();
    if path.as_os_str().is_empty() || path.file_name().is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "file path must name a file",
        ));
    }

    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    ensure_dir(parent)?;

    let (temp_path, mut temp_file) = create_temp_file(parent)?;
    let mut cleanup = TempCleanup(Some(temp_path.clone()));

    temp_file.write_all(bytes)?;
    temp_file.sync_all()?;
    drop(temp_file);

    replace_existing(&temp_path, path)?;
    cleanup.0 = None;

    if let Ok(directory) = File::open(parent) {
        let _ = directory.sync_all();
    }
    Ok(())
}

#[cfg(not(windows))]
fn replace_existing(from: &Path, to: &Path) -> io::Result<()> {
    fs::rename(from, to)
}

#[cfg(windows)]
fn replace_existing(from: &Path, to: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let from_wide: Vec<u16> = from
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let to_wide: Vec<u16> = to
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let flags = MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH;
    let success = unsafe { MoveFileExW(from_wide.as_ptr(), to_wide.as_ptr(), flags) };
    if success == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn create_temp_file(parent: &Path) -> io::Result<(PathBuf, File)> {
    for _ in 0..128 {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let name = format!(".filex-tmp-{}-{sequence}", std::process::id());
        let path = parent.join(name);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate a unique temporary file",
    ))
}

struct TempCleanup(Option<PathBuf>);

impl Drop for TempCleanup {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    fn test_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "tauri2-template-filex-{}-{}",
            std::process::id(),
            TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).expect("create test directory");
        path
    }

    #[test]
    fn atomic_write_replaces_file_and_creates_parents() {
        let dir = test_dir();
        let target = dir.join("nested").join("config.bin");
        write_atomic(&target, b"first").expect("write first file");
        write_atomic(&target, b"second").expect("replace file");
        assert_eq!(fs::read(&target).expect("read target"), b"second");
        fs::remove_dir_all(dir).expect("remove test directory");
    }

    #[test]
    fn read_limit_accepts_boundary_and_rejects_oversize() {
        let dir = test_dir();
        let path = dir.join("payload");
        fs::write(&path, b"1234").expect("write payload");
        assert_eq!(read_limit(&path, 4).expect("read at limit"), b"1234");
        assert_eq!(
            read_limit(&path, 3)
                .expect_err("reject oversized payload")
                .kind(),
            io::ErrorKind::InvalidData
        );
        fs::remove_dir_all(dir).expect("remove test directory");
    }

    #[test]
    fn rejects_empty_paths() {
        assert_eq!(
            ensure_dir("")
                .expect_err("reject empty directory path")
                .kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            write_atomic("", b"data")
                .expect_err("reject empty file path")
                .kind(),
            io::ErrorKind::InvalidInput
        );
    }
}
