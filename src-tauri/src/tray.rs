use serde::Serialize;
use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager};

use crate::error::AppResult;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TrayAction {
    action: String,
}

pub fn init(app: &AppHandle) -> AppResult<()> {
    let show = MenuItemBuilder::with_id("show", "显示主窗口").build(app)?;
    let home = MenuItemBuilder::with_id("home", "Home").build(app)?;
    let settings = MenuItemBuilder::with_id("settings", "Settings").build(app)?;
    let quit = PredefinedMenuItem::quit(app, Some("退出"))?;
    let menu = MenuBuilder::new(app)
        .items(&[&show, &home, &settings, &quit])
        .build()?;

    TrayIconBuilder::new()
        .menu(&menu)
        .on_menu_event(|app, event| {
            let action = event.id().as_ref();
            match action {
                "show" | "home" | "settings" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                    let _ = app.emit(
                        "tray:action",
                        TrayAction {
                            action: action.to_string(),
                        },
                    );
                }
                "quit" => app.exit(0),
                _ => {}
            }
        })
        .build(app)?;
    Ok(())
}
