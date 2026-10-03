use serde_json::Value;
use tauri::{State, WebviewWindow};

use crate::error::{AppError, AppResult};
use crate::models::{AppSettings, Preferences};
use crate::state::AppState;
use crate::storage;

use super::require_main;

fn with_db<T>(
    window: &WebviewWindow,
    state: &State<'_, AppState>,
    operation: impl FnOnce(&rusqlite::Connection) -> AppResult<T>,
) -> AppResult<T> {
    require_main(window)?;
    let storage = state
        .storage
        .read()
        .map_err(|_| AppError::new("STATE_LOCK", "存储状态锁定失败"))?;
    let connection = storage
        .db
        .lock()
        .map_err(|_| AppError::new("STORAGE_LOCK", "数据库锁定失败"))?;
    operation(&connection)
}

fn update_settings(
    window: &WebviewWindow,
    state: &State<'_, AppState>,
    update: impl FnOnce(&mut AppSettings) -> AppResult<()>,
) -> AppResult<AppSettings> {
    require_main(window)?;
    let storage = state
        .storage
        .read()
        .map_err(|_| AppError::new("STATE_LOCK", "存储状态锁定失败"))?;
    let connection = storage
        .db
        .lock()
        .map_err(|_| AppError::new("STORAGE_LOCK", "数据库锁定失败"))?;
    storage::update_settings(&connection, update)
}

#[tauri::command(async)]
pub fn get_app_settings(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> AppResult<AppSettings> {
    with_db(&window, &state, storage::read_settings)
}

#[tauri::command(async)]
pub fn set_theme_choice(
    window: WebviewWindow,
    state: State<'_, AppState>,
    theme_choice: String,
) -> AppResult<AppSettings> {
    update_settings(&window, &state, |settings| {
        let normalized = storage::normalize_theme_choice(&theme_choice)?;
        if normalized == "custom" && settings.custom_theme.is_none() {
            return Err(AppError::new(
                "THEME_INVALID",
                "A custom theme has not been saved",
            ));
        }
        settings.theme_choice = normalized;
        Ok(())
    })
}

#[tauri::command(async)]
pub fn set_custom_theme(
    window: WebviewWindow,
    state: State<'_, AppState>,
    custom_theme: Option<Value>,
) -> AppResult<AppSettings> {
    update_settings(&window, &state, |settings| {
        settings.custom_theme = custom_theme
            .as_ref()
            .map(storage::normalize_custom_theme)
            .transpose()?;
        settings.theme_choice = if settings.custom_theme.is_some() {
            "custom".to_string()
        } else if settings.theme_choice == "custom" {
            "system".to_string()
        } else {
            settings.theme_choice.clone()
        };
        Ok(())
    })
}

#[tauri::command(async)]
pub fn reset_custom_theme(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> AppResult<AppSettings> {
    update_settings(&window, &state, |settings| {
        settings.custom_theme = None;
        if settings.theme_choice == "custom" {
            settings.theme_choice = "system".to_string();
        }
        Ok(())
    })
}

#[tauri::command(async)]
pub fn set_locale_choice(
    window: WebviewWindow,
    state: State<'_, AppState>,
    locale_choice: String,
) -> AppResult<AppSettings> {
    update_settings(&window, &state, |settings| {
        let locale_choice = locale_choice.trim();
        if !matches!(locale_choice, "auto" | "zh-CN" | "en-US") {
            return Err(AppError::new(
                "LOCALE_INVALID",
                "localeChoice 必须是 auto、zh-CN 或 en-US",
            ));
        }
        settings.locale_choice = locale_choice.to_string();
        Ok(())
    })
}

#[tauri::command(async)]
pub fn get_preferences(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> AppResult<Preferences> {
    with_db(&window, &state, storage::read_preferences)
}

#[tauri::command(async)]
pub fn set_preference(
    window: WebviewWindow,
    state: State<'_, AppState>,
    key: String,
    value: Value,
) -> AppResult<Preferences> {
    require_main(&window)?;
    let storage = state
        .storage
        .read()
        .map_err(|_| AppError::new("STATE_LOCK", "存储状态锁定失败"))?;
    let connection = storage
        .db
        .lock()
        .map_err(|_| AppError::new("STORAGE_LOCK", "数据库锁定失败"))?;
    storage::set_preference(&connection, &key, &value)?;
    storage::read_preferences(&connection)
}

#[tauri::command(async)]
pub fn reset_preferences(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> AppResult<Preferences> {
    require_main(&window)?;
    let storage = state
        .storage
        .read()
        .map_err(|_| AppError::new("STATE_LOCK", "存储状态锁定失败"))?;
    let connection = storage
        .db
        .lock()
        .map_err(|_| AppError::new("STORAGE_LOCK", "数据库锁定失败"))?;
    storage::reset_preferences(&connection)?;
    storage::read_preferences(&connection)
}
