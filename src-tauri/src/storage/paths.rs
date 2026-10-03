use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::{backup::Backup, Connection};
use serde::{Deserialize, Serialize};
use tempfile::{Builder, NamedTempFile};

use crate::error::{AppError, AppResult};
use crate::models::StorageStats;
use crate::state::{AppState, StorageState};

use super::{integrity_check, open_database, DATABASE_FILE_NAME};

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StorageConfig {
    pub custom_path: Option<String>,
}

pub(crate) fn read_config(path: &Path) -> AppResult<StorageConfig> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|error| {
            AppError::with_detail(
                "STORAGE_CONFIG_INVALID",
                "Invalid storage config",
                error.to_string(),
            )
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(StorageConfig::default()),
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn persist_config(path: &Path, config: &StorageConfig) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::new("PATH_INVALID", "Config has no parent"))?;
    let mut temporary = Builder::new()
        .prefix(".storage-config-")
        .tempfile_in(parent)?;
    temporary.write_all(&serde_json::to_vec_pretty(config)?)?;
    temporary.as_file().sync_all()?;
    let backup = if path.exists() {
        let backup = Builder::new()
            .prefix(".storage-config-backup-")
            .tempfile_in(parent)?;
        let backup_path = backup.path().to_path_buf();
        backup.close()?;
        fs::rename(path, &backup_path)?;
        Some(backup_path)
    } else {
        None
    };
    if let Err(error) = temporary.persist_noclobber(path) {
        if let Some(backup) = &backup {
            let _ = fs::rename(backup, path);
        }
        return Err(AppError::from(error.error));
    }
    if let Some(backup) = backup {
        let _ = fs::remove_file(backup);
    }
    Ok(())
}

pub(crate) fn normalize_verbatim_path(path: &Path) -> PathBuf {
    let value = path.to_string_lossy();
    let normalized = if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = value.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        value.into_owned()
    };
    PathBuf::from(normalized)
}

pub(crate) fn absolute_database_path(path: &Path) -> AppResult<PathBuf> {
    if path.as_os_str().is_empty() {
        return Err(AppError::new("PATH_INVALID", "Empty storage path"));
    }
    let path = normalize_verbatim_path(path);
    let path = if path.is_dir() {
        path.join(DATABASE_FILE_NAME)
    } else {
        path
    };
    let absolute = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()?.join(path)
    };
    if absolute.exists() {
        return Ok(normalize_verbatim_path(&absolute.canonicalize()?));
    }
    let parent = absolute
        .parent()
        .ok_or_else(|| AppError::new("PATH_INVALID", "Storage path has no parent"))?;
    fs::create_dir_all(parent)?;
    let name = absolute
        .file_name()
        .ok_or_else(|| AppError::new("PATH_INVALID", "Storage path has no filename"))?;
    Ok(normalize_verbatim_path(&parent.canonicalize()?.join(name)))
}

fn temporary_database(parent: &Path) -> AppResult<NamedTempFile> {
    Ok(Builder::new()
        .prefix(".storage-snapshot-")
        .suffix(".sqlite3")
        .tempfile_in(parent)?)
}

fn snapshot(source: &Connection, file: &NamedTempFile) -> AppResult<()> {
    let mut destination = Connection::open(file.path())?;
    {
        let backup = Backup::new(source, &mut destination)?;
        backup.run_to_completion(128, Duration::from_millis(5), None)?;
    }
    destination.pragma_update(None, "journal_mode", "DELETE")?;
    integrity_check(&destination)?;
    drop(destination);
    file.as_file().sync_all()?;
    Ok(())
}

fn checkpoint(connection: &Connection) -> AppResult<()> {
    let busy: i64 =
        connection.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| row.get(0))?;
    if busy != 0 {
        return Err(AppError::new("STORAGE_BUSY", "Database is in use"));
    }
    Ok(())
}

pub fn set_custom_storage_path(
    state: &AppState,
    path: &Path,
    overwrite: bool,
) -> AppResult<StorageStats> {
    switch_path(state, path, overwrite, persist_config)
}

pub fn reset_storage_path(state: &AppState, overwrite: bool) -> AppResult<StorageStats> {
    let path = state
        .storage
        .read()
        .map_err(|_| AppError::new("STATE_LOCK", "Storage lock failed"))?
        .default_path
        .clone();
    switch_path(state, &path, overwrite, persist_config)
}

pub(crate) fn switch_path(
    state: &AppState,
    path: &Path,
    overwrite: bool,
    persist: impl FnOnce(&Path, &StorageConfig) -> AppResult<()>,
) -> AppResult<StorageStats> {
    let mut storage = state
        .storage
        .write()
        .map_err(|_| AppError::new("STATE_LOCK", "Storage lock failed"))?;
    let target = absolute_database_path(path)?;
    let is_custom = target != storage.default_path;
    let config = StorageConfig {
        custom_path: if is_custom {
            Some(target.to_string_lossy().into_owned())
        } else {
            None
        },
    };
    if target == storage.current_path {
        persist(&storage.config_path, &config)?;
        storage.is_custom = is_custom;
        return Ok(stats(&storage));
    }
    if target.exists() && !overwrite {
        return Err(AppError::new(
            "STORAGE_TARGET_EXISTS",
            "Explicit overwrite confirmation is required",
        ));
    }
    // Validate the old config before modifying any destination file.
    read_config(&storage.config_path)?;
    let parent = target
        .parent()
        .ok_or_else(|| AppError::new("PATH_INVALID", "Target has no parent"))?;
    let source = storage
        .db
        .lock()
        .map_err(|_| AppError::new("STORAGE_LOCK", "Database lock failed"))?;
    checkpoint(&source)?;
    let staging = temporary_database(parent)?;
    snapshot(&source, &staging)?;

    let original = if target.exists() {
        for suffix in ["-wal", "-shm"] {
            if PathBuf::from(format!("{}{suffix}", target.display())).exists() {
                return Err(AppError::new("STORAGE_BUSY", "Target database is in use"));
            }
        }
        // The target may be corrupt; preserve its raw bytes for rollback without
        // opening it. The new snapshot still goes through SQLite integrity_check.
        let backup = temporary_database(parent)?;
        fs::copy(&target, backup.path())?;
        backup.as_file().sync_all()?;
        Some(backup)
    } else {
        None
    };
    let mut original = original;
    let install_result = if original.is_some() {
        match fs::remove_file(&target) {
            Ok(()) => staging
                .persist_noclobber(&target)
                .map(|_| ())
                .map_err(|error| AppError::from(error.error)),
            Err(error) => Err(error.into()),
        }
    } else {
        staging
            .persist_noclobber(&target)
            .map(|_| ())
            .map_err(|error| {
                if error.error.kind() == std::io::ErrorKind::AlreadyExists {
                    AppError::new(
                        "STORAGE_TARGET_EXISTS",
                        "Target database appeared during switching",
                    )
                } else {
                    AppError::from(error.error)
                }
            })
    };
    if let Err(install_error) = install_result {
        if let Err(rollback_error) = rollback(original.take(), &target, false) {
            return Err(AppError::with_detail(
                "STORAGE_ROLLBACK_FAILED",
                "Storage snapshot install and rollback failed",
                format!("install={install_error}; rollback={rollback_error}"),
            ));
        }
        return Err(install_error);
    }
    let new_connection = match open_database(&target) {
        Ok(connection) => connection,
        Err(error) => {
            rollback(original, &target, true)?;
            return Err(error);
        }
    };
    if let Err(error) = persist(&storage.config_path, &config) {
        drop(new_connection); // essential before removing or replacing on Windows
        rollback(original, &target, true)?;
        return Err(error);
    }
    drop(source);
    storage.db = Arc::new(Mutex::new(new_connection));
    storage.current_path = target;
    storage.is_custom = is_custom;
    Ok(stats(&storage))
}

fn rollback(
    original: Option<NamedTempFile>,
    target: &Path,
    installed_by_us: bool,
) -> AppResult<()> {
    // target is exclusively our installed snapshot; never delete arbitrary fixed
    // .switching/.backup paths, and never copy a live database's WAL/SHM.
    if let Some(original) = original {
        if target.exists() {
            fs::remove_file(target)?;
        }
        original.persist_noclobber(target).map_err(|error| {
            AppError::with_detail(
                "STORAGE_ROLLBACK_FAILED",
                "Target rollback failed",
                error.error.to_string(),
            )
        })?;
    } else if installed_by_us && target.exists() {
        // We only remove a target without an original backup when this operation
        // successfully installed the snapshot. A failed persist_noclobber must
        // never delete a concurrently created database.
        fs::remove_file(target)?;
    }
    Ok(())
}

pub(crate) fn main_file_size(path: &Path) -> u64 {
    fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

pub(crate) fn size(path: &Path) -> u64 {
    let mut bytes = main_file_size(path);
    for suffix in ["-wal", "-shm"] {
        bytes = bytes.saturating_add(
            fs::metadata(format!("{}{suffix}", path.display()))
                .map(|meta| meta.len())
                .unwrap_or(0),
        );
    }
    bytes
}

pub(crate) fn stats(storage: &StorageState) -> StorageStats {
    StorageStats {
        path: normalize_verbatim_path(&storage.current_path)
            .to_string_lossy()
            .into_owned(),
        is_custom: storage.is_custom,
        default_path: normalize_verbatim_path(&storage.default_path)
            .to_string_lossy()
            .into_owned(),
        size_bytes: size(&storage.current_path),
    }
}
