use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

use crate::error::{AppError, AppResult};
use crate::events;
use crate::models::{ChildEventPayload, ChildMessagePayload, ChildWindowInfo, ChildWindowOptions};
use crate::state::{AppState, ChildWindowRecord};

fn label_for_id(id: &str) -> AppResult<String> {
    if id.is_empty()
        || id.len() > 64
        || !id.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        })
    {
        return Err(AppError::new(
            "CHILD_INVALID",
            "子窗口 id 只能包含字母、数字、- 和 _，长度不超过 64",
        ));
    }
    Ok(format!("child-{id}"))
}

fn validate_kind(kind: &str) -> AppResult<()> {
    if matches!(kind, "confirm" | "message" | "blank") {
        Ok(())
    } else {
        Err(AppError::new(
            "CHILD_INVALID",
            "子窗口 kind 必须是 confirm、message 或 blank",
        ))
    }
}

fn validate_dimension(value: Option<f64>, default: f64, min: f64, max: f64) -> AppResult<f64> {
    let value = value.unwrap_or(default);
    if !value.is_finite() || value < min || value > max {
        return Err(AppError::new("CHILD_INVALID", "子窗口尺寸超出允许范围"));
    }
    Ok(value)
}

fn encode_query(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push_str(&format!("{byte:02X}"));
        }
    }
    encoded
}

pub fn open(
    app: &AppHandle,
    state: &AppState,
    options: ChildWindowOptions,
) -> AppResult<ChildWindowInfo> {
    let label = label_for_id(&options.id)?;
    validate_kind(&options.kind)?;
    let width = validate_dimension(options.width, 480.0, 320.0, 1600.0)?;
    let height = validate_dimension(options.height, 320.0, 200.0, 1200.0)?;
    if let Some(message) = options.message.as_deref() {
        if message.chars().count() > 4096 {
            return Err(AppError::new(
                "CHILD_INVALID",
                "子窗口 message 长度不能超过 4096",
            ));
        }
    }
    if let Some(existing) = app.get_webview_window(&label) {
        existing.show()?;
        existing.set_focus()?;
        return Ok(ChildWindowInfo {
            id: options.id,
            label,
            kind: options.kind,
            title: options.title,
        });
    }

    let message = options.message.as_deref().unwrap_or("");
    let route = format!(
        "index.html?child={}&id={}&message={}",
        encode_query(&options.kind),
        encode_query(&options.id),
        encode_query(message),
    );
    let window = WebviewWindowBuilder::new(app, &label, WebviewUrl::App(route.into()))
        .title(options.title.clone())
        .inner_size(width, height)
        .visible(true)
        .resizable(true)
        .build()?;
    let info = ChildWindowInfo {
        id: options.id.clone(),
        label: label.clone(),
        kind: options.kind.clone(),
        title: options.title.clone(),
    };
    state
        .child_windows
        .lock()
        .map_err(|_| AppError::new("STATE_LOCK", "子窗口状态锁定失败"))?
        .insert(options.id.clone(), ChildWindowRecord { info: info.clone() });

    let app_for_close = app.clone();
    let id_for_close = options.id.clone();
    window.on_window_event(move |event| {
        if matches!(event, WindowEvent::Destroyed) {
            if let Some(state) = app_for_close.try_state::<AppState>() {
                if let Ok(mut windows) = state.child_windows.lock() {
                    windows.remove(&id_for_close);
                }
                let _ = events::emit_child_closed(&app_for_close, &id_for_close);
            }
        }
    });

    let payload = ChildEventPayload {
        id: options.id,
        kind: options.kind,
        status: "opened".to_string(),
        message: options.message,
        data: None,
        from: Some("main".to_string()),
    };
    let event_id = payload.id.clone();
    events::emit_child_result(app, &event_id, payload)?;
    Ok(info)
}

pub fn close(
    app: &AppHandle,
    state: &AppState,
    id: &str,
    result: Option<serde_json::Value>,
    sender_label: &str,
) -> AppResult<()> {
    let label = label_for_id(id)?;
    let record = state
        .child_windows
        .lock()
        .map_err(|_| AppError::new("STATE_LOCK", "子窗口状态锁定失败"))?
        .remove(id)
        .ok_or_else(|| AppError::new("CHILD_NOT_FOUND", "子窗口不存在"))?;

    events::emit_child_result(
        app,
        id,
        ChildEventPayload {
            id: id.to_string(),
            kind: record.info.kind,
            status: "result".to_string(),
            message: None,
            data: result,
            from: Some(sender_label.to_string()),
        },
    )?;
    if let Some(window) = app.get_webview_window(&label) {
        window.close()?;
    }
    Ok(())
}

pub fn list(state: &AppState) -> AppResult<Vec<ChildWindowInfo>> {
    let windows = state
        .child_windows
        .lock()
        .map_err(|_| AppError::new("STATE_LOCK", "子窗口状态锁定失败"))?;
    Ok(windows.values().map(|record| record.info.clone()).collect())
}

pub fn send_message(
    app: &AppHandle,
    state: &AppState,
    sender_label: &str,
    target_id: &str,
    data: serde_json::Value,
) -> AppResult<()> {
    let sender_id = sender_label.strip_prefix("child-");
    if let Some(sender_id) = sender_id {
        if sender_id != target_id {
            return Err(AppError::new(
                "CHILD_FORBIDDEN",
                "子窗口不能向其他子窗口发送消息",
            ));
        }
    } else if sender_label != "main" {
        return Err(AppError::new(
            "CHILD_FORBIDDEN",
            "当前窗口不能发送子窗口消息",
        ));
    }
    let windows = state
        .child_windows
        .lock()
        .map_err(|_| AppError::new("STATE_LOCK", "子窗口状态锁定失败"))?;
    let record = windows
        .get(target_id)
        .ok_or_else(|| AppError::new("CHILD_NOT_FOUND", "目标子窗口不存在"))?;
    if record.info.kind != "blank" {
        return Err(AppError::new(
            "CHILD_FORBIDDEN",
            "消息只能发送到 blank 子窗口",
        ));
    }
    drop(windows);
    events::emit_child_send(
        app,
        target_id,
        crate::models::ChildMessagePayload {
            from: sender_label.to_string(),
            data,
        },
    )
}

pub fn broadcast_message(
    app: &AppHandle,
    window: &tauri::WebviewWindow,
    data: serde_json::Value,
) -> AppResult<()> {
    let sender = window.label().to_string();
    events::emit_child_broadcast(app, ChildMessagePayload { from: sender, data })
}
