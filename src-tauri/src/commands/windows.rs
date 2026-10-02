use serde_json::Value;
use tauri::{State, WebviewWindow};

use crate::child_windows;
use crate::error::{AppError, AppResult};
use crate::models::{ChildWindowInfo, ChildWindowOptions};
use crate::state::AppState;

#[tauri::command(async)]
pub async fn window_minimize(window: WebviewWindow) -> AppResult<()> {
    window.minimize()?;
    Ok(())
}

#[tauri::command(async)]
pub async fn window_toggle_maximize(window: WebviewWindow) -> AppResult<()> {
    if window.is_maximized()? {
        window.unmaximize()?;
    } else {
        window.maximize()?;
    }
    Ok(())
}

#[tauri::command(async)]
pub async fn window_close(window: WebviewWindow) -> AppResult<()> {
    window.close()?;
    Ok(())
}

#[tauri::command(async)]
pub async fn open_child_window(
    app: tauri::AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    options: ChildWindowOptions,
) -> AppResult<ChildWindowInfo> {
    if window.label() != "main" {
        return Err(AppError::new(
            "CHILD_FORBIDDEN",
            "Only the main window can open child windows",
        ));
    }
    if options.id.trim().is_empty() || options.title.trim().is_empty() {
        return Err(AppError::new(
            "CHILD_INVALID",
            "子窗口 id 和 title 不能为空",
        ));
    }
    child_windows::open(&app, &state, options)
}

#[tauri::command(async)]
pub async fn close_child_window(
    app: tauri::AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
    result: Option<Value>,
) -> AppResult<()> {
    if let Some(sender_id) = window.label().strip_prefix("child-") {
        if sender_id != id {
            return Err(AppError::new(
                "CHILD_FORBIDDEN",
                "A child window can only close itself",
            ));
        }
    } else if window.label() != "main" {
        return Err(AppError::new(
            "CHILD_FORBIDDEN",
            "Current window cannot close child windows",
        ));
    }
    child_windows::close(&app, &state, &id, result, window.label())
}

#[tauri::command(async)]
pub async fn send_child_message(
    app: tauri::AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    target_id: String,
    data: Value,
) -> AppResult<()> {
    child_windows::send_message(&app, &state, window.label(), &target_id, data)
}

#[tauri::command(async)]
pub async fn broadcast_child_message(
    app: tauri::AppHandle,
    window: WebviewWindow,
    data: Value,
) -> AppResult<()> {
    child_windows::broadcast_message(&app, &window, data)
}

#[tauri::command(async)]
pub async fn list_child_windows(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> AppResult<Vec<ChildWindowInfo>> {
    if window.label() != "main" {
        return Err(AppError::new(
            "CHILD_FORBIDDEN",
            "Only the main window can list child windows",
        ));
    }
    child_windows::list(&state)
}
