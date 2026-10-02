use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter, Manager};

use crate::error::{AppError, AppResult};
use crate::models::{
    ChildClosedPayload, ChildEventPayload, ChildMessagePayload, SubprocessExitPayload,
    SubprocessLinePayload, TimePayload,
};
use crate::state::AppState;

pub const APP_TIME: &str = "app:time";
#[allow(dead_code)]
pub const CHILD_BROADCAST: &str = "child:broadcast";

pub fn start_time_loop(app: &AppHandle) -> AppResult<()> {
    let state = app
        .try_state::<AppState>()
        .ok_or_else(|| AppError::new("STATE_MISSING", "AppState 未初始化"))?;
    let app = app.clone();
    let stop = state.stop_requested.clone();
    let handle = std::thread::spawn(move || {
        while !stop.load(std::sync::atomic::Ordering::SeqCst) {
            let _ = app.emit(APP_TIME, time_payload());
            std::thread::sleep(Duration::from_secs(1));
        }
    });
    state
        .background_tasks
        .lock()
        .map_err(|_| AppError::new("STATE_LOCK", "后台任务状态锁定失败"))?
        .push(handle);
    Ok(())
}

fn time_payload() -> TimePayload {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let unix_ms = duration.as_millis().min(i64::MAX as u128) as i64;
    let seconds = duration.as_secs() % 86_400;
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let seconds = seconds % 60;
    TimePayload {
        formatted: format!("{hours:02}:{minutes:02}:{seconds:02}"),
        unix_ms,
    }
}

pub fn emit_child_result(app: &AppHandle, id: &str, payload: ChildEventPayload) -> AppResult<()> {
    app.emit(&format!("child:result:{id}"), payload)?;
    Ok(())
}

#[allow(dead_code)]
pub fn emit_child_send(app: &AppHandle, id: &str, payload: ChildMessagePayload) -> AppResult<()> {
    app.emit(&format!("child:send:{id}"), payload)?;
    Ok(())
}

#[allow(dead_code)]
pub fn emit_child_broadcast(app: &AppHandle, payload: ChildMessagePayload) -> AppResult<()> {
    app.emit(CHILD_BROADCAST, payload)?;
    Ok(())
}

pub fn emit_child_closed(app: &AppHandle, id: &str) -> AppResult<()> {
    app.emit(
        &format!("child:closed:{id}"),
        ChildClosedPayload { id: id.to_string() },
    )?;
    Ok(())
}

pub fn emit_subprocess_stdout(app: &AppHandle, id: &str, line: String) -> AppResult<()> {
    app.emit(
        &format!("subprocess:stdout:{id}"),
        SubprocessLinePayload {
            id: id.to_string(),
            line,
        },
    )?;
    Ok(())
}

pub fn emit_subprocess_stderr(app: &AppHandle, id: &str, line: String) -> AppResult<()> {
    app.emit(
        &format!("subprocess:stderr:{id}"),
        SubprocessLinePayload {
            id: id.to_string(),
            line,
        },
    )?;
    Ok(())
}

pub fn emit_subprocess_ready(app: &AppHandle, id: &str) -> AppResult<()> {
    app.emit(
        &format!("subprocess:ready:{id}"),
        serde_json::json!({ "id": id }),
    )?;
    Ok(())
}

pub fn emit_subprocess_exit(app: &AppHandle, payload: SubprocessExitPayload) -> AppResult<()> {
    app.emit(&format!("subprocess:exit:{}", payload.id), payload)?;
    Ok(())
}
