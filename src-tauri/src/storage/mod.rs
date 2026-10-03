mod data;
mod paths;
mod schema;
mod stats;
#[cfg(test)]
mod tests;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::Connection;

use crate::error::{AppError, AppResult};
use crate::state::{AppState, StorageState};

#[derive(Debug, Clone, Copy)]
pub(crate) struct TableDescriptor {
    pub name: &'static str,
    pub label_key: &'static str,
    pub clearable: bool,
}

pub(crate) const TABLE_DESCRIPTORS: &[TableDescriptor] = &[
    TableDescriptor {
        name: "app_config",
        label_key: "settings.table.appConfig",
        clearable: false,
    },
    TableDescriptor {
        name: "user_preferences",
        label_key: "settings.table.userPreferences",
        clearable: true,
    },
];

pub(crate) fn table_descriptor(name: &str) -> Option<TableDescriptor> {
    TABLE_DESCRIPTORS
        .iter()
        .copied()
        .find(|descriptor| descriptor.name == name)
}

pub use data::{
    clear_table, normalize_custom_theme, normalize_theme_choice, read_preferences, read_settings,
    reset_preferences, set_preference, update_settings,
};
pub use paths::{reset_storage_path, set_custom_storage_path};
pub use schema::migrate;
pub use stats::{get_storage_snapshot, get_storage_stats, get_table_stats};

const DATABASE_FILE_NAME: &str = "app.sqlite3";
const STORAGE_FILE_NAME: &str = "storage.json";

pub fn initialize(default_dir: PathBuf) -> AppResult<StorageState> {
    fs::create_dir_all(&default_dir)?;
    let default_dir = paths::normalize_verbatim_path(&default_dir.canonicalize()?);
    let default_path = paths::normalize_verbatim_path(&default_dir.join(DATABASE_FILE_NAME));
    let config_path = default_dir.join(STORAGE_FILE_NAME);
    let config = paths::read_config(&config_path)?;
    let current_path = match config.custom_path {
        Some(path) => paths::absolute_database_path(Path::new(&path))?,
        None => default_path.clone(),
    };
    let is_custom = current_path != default_path;
    let db = open_database(&current_path)?;
    Ok(StorageState {
        default_path,
        current_path,
        config_path,
        is_custom,
        db: Arc::new(Mutex::new(db)),
    })
}

pub fn open_database(path: &Path) -> AppResult<Connection> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let connection = Connection::open(path)?;
    // Reject newer schemas before writing journal-mode or creating any tables.
    schema::check_version(&connection)?;
    integrity_check(&connection)?;
    configure_connection(&connection)?;
    migrate(&connection)?;
    Ok(connection)
}

pub fn configure_connection(connection: &Connection) -> AppResult<()> {
    connection.busy_timeout(Duration::from_secs(5))?;
    connection.pragma_update(None, "journal_mode", "WAL")?;
    connection.pragma_update(None, "foreign_keys", "ON")?;
    connection.pragma_update(None, "synchronous", "NORMAL")?;
    Ok(())
}

pub fn integrity_check(connection: &Connection) -> AppResult<()> {
    let result: String = connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if result == "ok" {
        Ok(())
    } else {
        Err(AppError::with_detail(
            "STORAGE_CORRUPT",
            "Database integrity check failed",
            result,
        ))
    }
}

#[allow(dead_code)]
pub fn with_connection<T>(
    state: &AppState,
    operation: impl FnOnce(&Connection) -> AppResult<T>,
) -> AppResult<T> {
    let storage = state
        .storage
        .read()
        .map_err(|_| AppError::new("STATE_LOCK", "Storage lock failed"))?;
    let connection = storage
        .db
        .lock()
        .map_err(|_| AppError::new("STORAGE_LOCK", "Database lock failed"))?;
    operation(&connection)
}

pub(crate) fn now_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}
