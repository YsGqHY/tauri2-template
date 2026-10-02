use rusqlite::{params, Connection, OptionalExtension};

use crate::error::{AppError, AppResult};

pub const SCHEMA_VERSION: i64 = 2;

fn version(connection: &Connection) -> AppResult<i64> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='schema_meta')",
        [],
        |row| row.get(0),
    )?;
    if !exists {
        return Ok(0);
    }
    let version = connection
        .query_row(
            "SELECT value FROM schema_meta WHERE key='version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    match version {
        Some(value) => value
            .parse()
            .map_err(|_| AppError::new("SCHEMA_INVALID", "Invalid schema version")),
        None => Err(AppError::new("SCHEMA_INVALID", "Schema version is missing")),
    }
}

pub fn check_version(connection: &Connection) -> AppResult<i64> {
    let version = version(connection)?;
    if !(0..=SCHEMA_VERSION).contains(&version) {
        return Err(AppError::new("SCHEMA_NEWER", "Unsupported database schema"));
    }
    Ok(version)
}

pub fn migrate(connection: &Connection) -> AppResult<()> {
    let current = check_version(connection)?;
    let transaction = connection.unchecked_transaction()?;
    if current == 0 {
        transaction.execute_batch(
            "CREATE TABLE schema_meta (key TEXT PRIMARY KEY NOT NULL, value TEXT NOT NULL);
             CREATE TABLE user_preferences (key TEXT PRIMARY KEY NOT NULL, value_json TEXT NOT NULL, updated_at INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE app_config (
                id INTEGER PRIMARY KEY CHECK(id=1), theme_choice TEXT NOT NULL DEFAULT 'system',
                custom_theme_json TEXT, locale_choice TEXT NOT NULL DEFAULT 'auto',
                updated_at INTEGER NOT NULL DEFAULT 0
             );",
        )?;
    } else if current == 1 {
        transaction.execute_batch(
            "ALTER TABLE app_config ADD COLUMN updated_at INTEGER NOT NULL DEFAULT 0;
             ALTER TABLE user_preferences ADD COLUMN updated_at INTEGER NOT NULL DEFAULT 0;
             UPDATE app_config SET locale_choice='auto' WHERE locale_choice='system';",
        )?;
    }
    if current < SCHEMA_VERSION {
        transaction.execute(
            "INSERT INTO schema_meta(key,value) VALUES('version',?1)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![SCHEMA_VERSION.to_string()],
        )?;
    }
    transaction.execute(
        "INSERT OR IGNORE INTO app_config(id,theme_choice,custom_theme_json,locale_choice,updated_at)
         VALUES(1,'system',NULL,'auto',?1)", params![super::now_unix_ms()],
    )?;
    transaction.commit()?;
    Ok(())
}
