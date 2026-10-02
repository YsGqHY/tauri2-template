use rusqlite::{params, Connection};
use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::models::{AppSettings, Preferences};

pub fn validate_theme_choice(choice: &str) -> AppResult<()> {
    if matches!(choice, "system" | "light" | "dark" | "obsidian" | "custom") {
        Ok(())
    } else {
        Err(AppError::new("THEME_INVALID", "Unsupported theme choice"))
    }
}

pub fn validate_locale(choice: &str) -> AppResult<()> {
    if matches!(choice, "auto" | "zh-CN" | "en-US") {
        Ok(())
    } else {
        Err(AppError::new("LOCALE_INVALID", "Unsupported locale choice"))
    }
}

pub fn validate_custom_theme(theme: &Value) -> AppResult<()> {
    let object = theme
        .as_object()
        .ok_or_else(|| AppError::new("THEME_INVALID", "Theme must be an object"))?;
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "name" | "mode" | "palette"))
    {
        return Err(AppError::new("THEME_INVALID", "Unsupported theme field"));
    }
    if let Some(name) = object.get("name") {
        let name = name
            .as_str()
            .ok_or_else(|| AppError::new("THEME_INVALID", "Theme name must be text"))?;
        if name.trim().is_empty() || name.chars().count() > 64 {
            return Err(AppError::new(
                "THEME_INVALID",
                "Theme name must contain 1 to 64 characters",
            ));
        }
    }
    if let Some(mode) = object.get("mode") {
        if !matches!(mode.as_str(), Some("light" | "dark")) {
            return Err(AppError::new(
                "THEME_INVALID",
                "Theme mode must be light or dark",
            ));
        }
    }
    let palette = object
        .get("palette")
        .and_then(Value::as_object)
        .ok_or_else(|| AppError::new("THEME_INVALID", "Theme palette must be an object"))?;
    const KEYS: &[&str] = &[
        "bg",
        "surface",
        "surfaceRaised",
        "surfaceMuted",
        "text",
        "textMuted",
        "textSubtle",
        "border",
        "accent",
        "accentStrong",
        "accentSoft",
        "success",
        "warning",
        "danger",
        "info",
        "focus",
        "sidebar",
    ];
    for (key, value) in palette {
        if !KEYS.contains(&key.as_str()) {
            return Err(AppError::new("THEME_INVALID", "Unsupported palette token"));
        }
        let color = value
            .as_str()
            .ok_or_else(|| AppError::new("THEME_INVALID", "Palette colors must be strings"))?;
        if !matches!(color.len(), 4 | 7 | 9)
            || !color.starts_with('#')
            || !color.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
        {
            return Err(AppError::new(
                "THEME_INVALID",
                "Palette colors must be hex colors",
            ));
        }
    }
    Ok(())
}

pub fn read_settings(connection: &Connection) -> AppResult<AppSettings> {
    let (theme_choice, json, locale_choice) = connection.query_row(
        "SELECT theme_choice,custom_theme_json,locale_choice FROM app_config WHERE id=1",
        [],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
            ))
        },
    )?;
    let settings = AppSettings {
        theme_choice,
        custom_theme: json.map(|value| serde_json::from_str(&value)).transpose()?,
        locale_choice,
    };
    Ok(settings)
}

pub fn write_settings(connection: &Connection, settings: &AppSettings) -> AppResult<()> {
    validate_theme_choice(&settings.theme_choice)?;
    validate_locale(&settings.locale_choice)?;
    if let Some(theme) = &settings.custom_theme {
        validate_custom_theme(theme)?;
    }
    if settings.theme_choice == "custom" && settings.custom_theme.is_none() {
        return Err(AppError::new(
            "THEME_INVALID",
            "A custom theme has not been saved",
        ));
    }
    let json = settings
        .custom_theme
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;
    connection.execute(
        "UPDATE app_config SET theme_choice=?1,custom_theme_json=?2,locale_choice=?3,updated_at=?4 WHERE id=1",
        params![settings.theme_choice, json, settings.locale_choice, super::now_unix_ms()],
    )?;
    Ok(())
}

pub fn update_settings(
    connection: &Connection,
    update: impl FnOnce(&mut AppSettings) -> AppResult<()>,
) -> AppResult<AppSettings> {
    let transaction = connection.unchecked_transaction()?;
    let mut settings = read_settings(&transaction)?;
    update(&mut settings)?;
    write_settings(&transaction, &settings)?;
    let saved = read_settings(&transaction)?;
    transaction.commit()?;
    Ok(saved)
}

pub fn read_preferences(connection: &Connection) -> AppResult<Preferences> {
    let mut preferences = Preferences::default();
    let mut statement = connection.prepare("SELECT key,value_json FROM user_preferences")?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in rows {
        let (key, json) = row?;
        let value: Value = serde_json::from_str(&json)?;
        let value = value
            .as_bool()
            .ok_or_else(|| AppError::new("PREFERENCE_INVALID", "Preference must be boolean"))?;
        match key.as_str() {
            "showLogo" => preferences.show_logo = value,
            "showTooltip" => preferences.show_tooltip = value,
            _ => {}
        }
    }
    Ok(preferences)
}

pub fn set_preference(connection: &Connection, key: &str, value: &Value) -> AppResult<()> {
    if !matches!(key, "showLogo" | "showTooltip") {
        return Err(AppError::new("PREFERENCE_UNKNOWN", "Unknown preference"));
    }
    if !value.is_boolean() {
        return Err(AppError::new(
            "PREFERENCE_INVALID",
            "Preference must be boolean",
        ));
    }
    connection.execute(
        "INSERT INTO user_preferences(key,value_json,updated_at) VALUES(?1,?2,?3)
         ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,updated_at=excluded.updated_at",
        params![key, serde_json::to_string(value)?, super::now_unix_ms()],
    )?;
    Ok(())
}

pub fn reset_preferences(connection: &Connection) -> AppResult<()> {
    connection.execute("DELETE FROM user_preferences", [])?;
    Ok(())
}

pub fn clear_table(connection: &Connection, table: &str) -> AppResult<()> {
    if !super::CLEARABLE_TABLES.contains(&table) {
        return Err(AppError::new(
            "TABLE_NOT_CLEARABLE",
            "Table is not clearable",
        ));
    }
    reset_preferences(connection)
}
