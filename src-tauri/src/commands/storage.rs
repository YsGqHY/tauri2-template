use std::path::PathBuf;

use tauri::{State, WebviewWindow};

use crate::error::{AppError, AppResult};
use crate::models::{StorageSnapshot, StorageStats, StorageTableStats};
use crate::state::AppState;
use crate::storage;

#[tauri::command(async)]
pub fn get_storage_stats(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> AppResult<StorageStats> {
    super::require_main(&window)?;
    storage::get_storage_stats(&state)
}

#[tauri::command(async)]
pub fn get_table_stats(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> AppResult<StorageTableStats> {
    super::require_main(&window)?;
    storage::get_table_stats(&state)
}

#[tauri::command(async)]
pub fn get_storage_snapshot(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> AppResult<StorageSnapshot> {
    super::require_main(&window)?;
    storage::get_storage_snapshot(&state)
}

#[tauri::command(async)]
pub fn set_custom_storage_path(
    window: WebviewWindow,
    state: State<'_, AppState>,
    path: String,
    overwrite: Option<bool>,
) -> AppResult<StorageStats> {
    super::require_main(&window)?;
    let path = path.trim();
    if path.is_empty() {
        return Err(AppError::new("PATH_INVALID", "storage path 不能为空"));
    }
    storage::set_custom_storage_path(&state, &PathBuf::from(path), overwrite.unwrap_or(false))
}

#[tauri::command(async)]
pub fn reset_storage_path(
    window: WebviewWindow,
    state: State<'_, AppState>,
    overwrite: Option<bool>,
) -> AppResult<StorageStats> {
    super::require_main(&window)?;
    storage::reset_storage_path(&state, overwrite.unwrap_or(false))
}

#[tauri::command(async)]
pub fn clear_table(
    window: WebviewWindow,
    state: State<'_, AppState>,
    table: String,
) -> AppResult<StorageTableStats> {
    super::require_main(&window)?;
    let storage_state = state
        .storage
        .read()
        .map_err(|_| AppError::new("STATE_LOCK", "存储状态锁定失败"))?;
    let connection = storage_state
        .db
        .lock()
        .map_err(|_| AppError::new("STORAGE_LOCK", "数据库锁定失败"))?;
    storage::clear_table(&connection, &table)?;
    drop(connection);
    drop(storage_state);
    storage::get_table_stats(&state)
}
