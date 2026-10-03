mod child_windows;
mod commands;
mod error;
mod events;
mod models;
mod state;
mod storage;
mod subprocess;
mod tray;
#[allow(dead_code)]
mod utils;

use state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let storage = storage::initialize(data_dir)
                .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })?;
            app.manage(AppState::new(storage));
            tray::init(app.handle())
                .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })?;
            events::start_time_loop(app.handle())
                .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app::greet,
            commands::app::get_app_info,
            commands::settings::get_app_settings,
            commands::settings::set_theme_choice,
            commands::settings::set_custom_theme,
            commands::settings::reset_custom_theme,
            commands::settings::set_locale_choice,
            commands::settings::get_preferences,
            commands::settings::set_preference,
            commands::settings::reset_preferences,
            commands::storage::get_storage_stats,
            commands::storage::get_table_stats,
            commands::storage::get_storage_snapshot,
            commands::storage::set_custom_storage_path,
            commands::storage::reset_storage_path,
            commands::storage::clear_table,
            commands::windows::window_minimize,
            commands::windows::window_toggle_maximize,
            commands::windows::window_close,
            commands::windows::open_child_window,
            commands::windows::close_child_window,
            commands::windows::send_child_message,
            commands::windows::broadcast_child_message,
            commands::windows::list_child_windows,
            commands::subprocess::get_available_commands,
            commands::subprocess::run_subprocess,
            commands::subprocess::stop_subprocess,
            commands::subprocess::list_subprocesses,
            commands::subprocess::get_subprocess_output,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app_handle, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            if let Some(state) = app_handle.try_state::<AppState>() {
                state.stop_and_join();
            }
        }
    });
}
