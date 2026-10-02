pub mod app;
pub mod settings;
pub mod storage;
pub mod subprocess;
pub mod windows;

use tauri::WebviewWindow;

use crate::error::{AppError, AppResult};

pub fn require_main(window: &WebviewWindow) -> AppResult<()> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err(AppError::new(
            "MAIN_WINDOW_REQUIRED",
            "This operation is only available from the main window",
        ))
    }
}
