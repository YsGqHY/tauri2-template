use tauri::{State, WebviewWindow};

use crate::error::AppResult;
use crate::models::{SubprocessInfo, SubprocessRequest};
use crate::state::AppState;
use crate::subprocess;

#[tauri::command]
pub fn get_available_commands(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> AppResult<Vec<crate::models::CommandSpec>> {
    super::require_main(&window)?;
    subprocess::available_commands(&state)
}

#[tauri::command]
pub fn run_subprocess(
    app: tauri::AppHandle,
    window: WebviewWindow,
    state: State<'_, AppState>,
    request: SubprocessRequest,
) -> AppResult<SubprocessInfo> {
    super::require_main(&window)?;
    subprocess::run(&app, &state, request)
}

#[tauri::command]
pub fn stop_subprocess(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
) -> AppResult<()> {
    super::require_main(&window)?;
    subprocess::stop(&state, &id)
}

#[tauri::command]
pub fn list_subprocesses(
    window: WebviewWindow,
    state: State<'_, AppState>,
) -> AppResult<Vec<SubprocessInfo>> {
    super::require_main(&window)?;
    subprocess::list(&state)
}

#[tauri::command]
pub fn get_subprocess_output(
    window: WebviewWindow,
    state: State<'_, AppState>,
    id: String,
) -> AppResult<crate::models::SubprocessSnapshot> {
    super::require_main(&window)?;
    subprocess::get_output(&state, &id)
}
