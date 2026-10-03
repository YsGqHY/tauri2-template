use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{params, Connection};
use serde_json::Value;

use super::data::write_settings;
use super::*;
use crate::models::{AppSettings, Preferences};
use crate::state::AppState;

fn fixture_root(label: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "tauri2-{label}-{}-{}",
        super::now_unix_ms(),
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("fixture root");
    path
}

#[test]
fn migration_creates_defaults_and_updated_at() {
    let connection = Connection::open_in_memory().expect("db");
    configure_connection(&connection).expect("configure");
    migrate(&connection).expect("migration");
    assert_eq!(
        read_settings(&connection).expect("settings"),
        AppSettings::default()
    );
    assert_eq!(
        read_preferences(&connection).expect("preferences"),
        Preferences::default()
    );
    let column: String = connection
        .query_row(
            "SELECT name FROM pragma_table_info('app_config') WHERE name='updated_at'",
            [],
            |row| row.get(0),
        )
        .expect("updated_at");
    assert_eq!(column, "updated_at");
}

#[test]
fn future_schema_is_rejected_before_writes() {
    let connection = Connection::open_in_memory().expect("db");
    connection
        .execute_batch(
            "CREATE TABLE schema_meta(key TEXT PRIMARY KEY,value TEXT NOT NULL);
         INSERT INTO schema_meta VALUES('version','999');",
        )
        .expect("fixture");
    let error = migrate(&connection).expect_err("future schema");
    assert_eq!(error.code, "SCHEMA_NEWER");
    let tables: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE name='app_config'",
            [],
            |row| row.get(0),
        )
        .expect("schema query");
    assert_eq!(tables, 0);
}

#[test]
fn settings_persist_and_legacy_locale_normalizes() {
    let connection = Connection::open_in_memory().expect("db");
    connection.execute_batch(
        "CREATE TABLE schema_meta(key TEXT PRIMARY KEY,value TEXT NOT NULL);
         INSERT INTO schema_meta VALUES('version','1');
         CREATE TABLE user_preferences(key TEXT PRIMARY KEY,value_json TEXT NOT NULL);
         CREATE TABLE app_config(id INTEGER PRIMARY KEY CHECK(id=1),theme_choice TEXT NOT NULL,custom_theme_json TEXT,locale_choice TEXT NOT NULL);
         INSERT INTO app_config VALUES(1,'dark',NULL,'system');",
    ).expect("legacy fixture");
    migrate(&connection).expect("migration");
    let settings = read_settings(&connection).expect("settings");
    assert_eq!(settings.locale_choice, "auto");
    let root = fixture_root("restart");
    let storage = initialize(root.clone()).expect("initialize");
    {
        let connection = storage.db.lock().expect("db lock");
        write_settings(
            &connection,
            &AppSettings {
                theme_choice: "custom".into(),
                custom_theme: Some(
                    serde_json::json!({"name":"Night","mode":"dark","palette":{"bg":"#000000"}}),
                ),
                locale_choice: "en-US".into(),
            },
        )
        .expect("write settings");
    }
    drop(storage);
    let restarted = initialize(root.clone()).expect("restart");
    assert_eq!(
        read_settings(&restarted.db.lock().expect("db lock"))
            .expect("read")
            .locale_choice,
        "en-US"
    );
    drop(restarted);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn storage_path_roundtrip_requires_overwrite_for_existing_target() {
    let root = fixture_root("switch");
    let state = AppState::new(initialize(root.clone()).expect("initialize"));
    {
        let storage = state.storage.read().expect("storage lock");
        let connection = storage.db.lock().expect("db lock");
        set_preference(&connection, "showLogo", &Value::Bool(false)).expect("preference");
    }
    let custom = root.join("custom").join("app.sqlite3");
    set_custom_storage_path(&state, &custom, false).expect("custom path");
    assert!(state.storage.read().expect("storage lock").is_custom);
    reset_storage_path(&state, true).expect("default path");
    let storage = state.storage.read().expect("storage lock");
    assert!(
        !read_preferences(&storage.db.lock().expect("db lock"))
            .expect("preference")
            .show_logo
    );
    drop(storage);
    drop(state);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn existing_target_is_not_modified_without_confirmation() {
    let root = fixture_root("target");
    let state = AppState::new(initialize(root.clone()).expect("initialize"));
    let target = root.join("existing.sqlite3");
    fs::write(&target, b"do-not-delete").expect("target");
    let error = set_custom_storage_path(&state, &target, false).expect_err("confirmation");
    assert_eq!(error.code, "STORAGE_TARGET_EXISTS");
    assert_eq!(fs::read(&target).expect("target bytes"), b"do-not-delete");
    drop(state);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn config_failure_restores_existing_target_bytes() {
    let root = fixture_root("rollback");
    let state = AppState::new(initialize(root.clone()).expect("initialize"));
    let target = root.join("existing.sqlite3");
    fs::write(&target, b"original-target").expect("target");
    let error = super::paths::switch_path(&state, &target, true, |_path, _config| {
        Err(crate::error::AppError::new(
            "INJECTED_CONFIG_FAILURE",
            "injected",
        ))
    })
    .expect_err("injected config failure");
    assert_eq!(error.code, "INJECTED_CONFIG_FAILURE");
    assert_eq!(
        fs::read(&target).expect("restored target"),
        b"original-target"
    );
    assert!(!state.storage.read().expect("storage lock").is_custom);
    drop(state);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn legacy_theme_choice_and_partial_palette_are_normalized() {
    let root = fixture_root("theme-compat");
    let storage = initialize(root.clone()).expect("initialize");
    let connection = storage.db.lock().expect("db lock");
    connection
        .execute(
            "UPDATE app_config SET theme_choice=?1, custom_theme_json=?2, locale_choice=?3 WHERE id=1",
            params![
                "foundation-dark",
                serde_json::to_string(&serde_json::json!({
                    "mode": "dark",
                    "palette": {"bg": "rgb(1, 2, 3)"}
                }))
                .expect("theme json"),
                "system"
            ],
        )
        .expect("legacy settings");
    let settings = read_settings(&connection).expect("settings");
    assert_eq!(settings.theme_choice, "dark");
    assert_eq!(settings.locale_choice, "auto");
    let theme = settings.custom_theme.expect("custom theme");
    assert_eq!(theme["palette"]["bg"], "rgb(1, 2, 3)");
    assert!(theme["palette"]["surface"].as_str().is_some());
    drop(connection);
    drop(storage);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn custom_theme_rejects_css_injection_values() {
    let error = super::data::normalize_custom_theme(&serde_json::json!({
        "mode": "light",
        "palette": {"bg": "#fff; color: red"}
    }))
    .expect_err("unsafe color");
    assert_eq!(error.code, "THEME_INVALID");
}

#[test]
fn verbatim_windows_paths_are_normalized_for_storage_identity() {
    assert_eq!(
        super::paths::normalize_verbatim_path(Path::new(r"\\?\C:\data\app.sqlite3")),
        PathBuf::from(r"C:\data\app.sqlite3")
    );
    assert_eq!(
        super::paths::normalize_verbatim_path(Path::new(r"\\?\UNC\server\share\app.sqlite3")),
        PathBuf::from(r"\\server\share\app.sqlite3")
    );
}
