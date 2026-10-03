use rusqlite::{params, Connection};
use serde_json::Value;

use crate::error::{AppError, AppResult};
use crate::models::{AppSettings, Preferences};

pub fn normalize_theme_choice(choice: &str) -> AppResult<String> {
    match choice.trim() {
        "system" | "foundation-system" => Ok("system".to_string()),
        "light" | "foundation-light" => Ok("light".to_string()),
        "dark" | "foundation-dark" => Ok("dark".to_string()),
        "obsidian" | "foundation-obsidian" => Ok("obsidian".to_string()),
        "custom" | "foundation-custom" => Ok("custom".to_string()),
        _ => Err(AppError::new("THEME_INVALID", "Unsupported theme choice")),
    }
}

pub fn validate_locale(choice: &str) -> AppResult<()> {
    if matches!(choice.trim(), "auto" | "zh-CN" | "en-US") {
        Ok(())
    } else {
        Err(AppError::new("LOCALE_INVALID", "Unsupported locale choice"))
    }
}

const PALETTE_KEYS: &[&str] = &[
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

fn base_palette(mode: &str) -> serde_json::Map<String, Value> {
    let values = if mode == "dark" {
        serde_json::json!({
            "bg": "#0b0d10",
            "surface": "#22262d",
            "surfaceRaised": "#2a2f37",
            "surfaceMuted": "rgba(255, 255, 255, 0.06)",
            "text": "#f8fafc",
            "textMuted": "#cbd5e1",
            "textSubtle": "#64748b",
            "border": "rgba(255, 255, 255, 0.08)",
            "accent": "#60a5fa",
            "accentStrong": "#93c5fd",
            "accentSoft": "rgba(96, 165, 250, 0.16)",
            "success": "#4ade80",
            "warning": "#fbbf24",
            "danger": "#f87171",
            "info": "#60a5fa",
            "focus": "#93c5fd",
            "sidebar": "#15181d"
        })
    } else {
        serde_json::json!({
            "bg": "#ffffff",
            "surface": "#ffffff",
            "surfaceRaised": "#eef1f6",
            "surfaceMuted": "rgba(15, 23, 42, 0.08)",
            "text": "#0b1220",
            "textMuted": "#3f4a5a",
            "textSubtle": "#8a93a4",
            "border": "rgba(15, 23, 42, 0.10)",
            "accent": "#2563eb",
            "accentStrong": "#1d4ed8",
            "accentSoft": "rgba(37, 99, 235, 0.14)",
            "success": "#16a34a",
            "warning": "#d97706",
            "danger": "#dc2626",
            "info": "#2563eb",
            "focus": "#1d4ed8",
            "sidebar": "#f6f7fa"
        })
    };
    values
        .as_object()
        .cloned()
        .expect("theme base palette must be an object")
}

fn is_valid_css_color(value: &str) -> bool {
    let value = value.trim();
    let is_hex = matches!(value.len(), 4 | 5 | 7 | 9)
        && value.starts_with('#')
        && value.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit);
    if is_hex || value.eq_ignore_ascii_case("transparent") {
        return true;
    }
    let Some(open) = value.find('(') else {
        return false;
    };
    if open == 0 || !value.ends_with(')') {
        return false;
    }
    let name = value[..open].to_ascii_lowercase();
    if !matches!(
        name.as_str(),
        "rgb" | "rgba" | "hsl" | "hsla" | "hwb" | "lab" | "lch" | "oklab" | "oklch" | "color"
    ) {
        return false;
    }
    let body = &value[open + 1..value.len() - 1];
    !body.is_empty()
        && body.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(
                    character,
                    ' ' | '\t' | '.' | ',' | '%' | '+' | '-' | '/' | '(' | ')'
                )
        })
}

pub fn normalize_custom_theme(theme: &Value) -> AppResult<Value> {
    let object = theme
        .as_object()
        .ok_or_else(|| AppError::new("THEME_INVALID", "Theme must be an object"))?;
    if object
        .keys()
        .any(|key| !matches!(key.as_str(), "name" | "mode" | "palette"))
    {
        return Err(AppError::new("THEME_INVALID", "Unsupported theme field"));
    }
    let name = match object.get("name") {
        Some(value) => {
            let name = value
                .as_str()
                .ok_or_else(|| AppError::new("THEME_INVALID", "Theme name must be text"))?
                .trim();
            if name.is_empty() || name.chars().count() > 64 {
                return Err(AppError::new(
                    "THEME_INVALID",
                    "Theme name must contain 1 to 64 characters",
                ));
            }
            name.to_string()
        }
        None => "Foundation Custom".to_string(),
    };
    let mode = match object.get("mode") {
        Some(value) => value
            .as_str()
            .filter(|mode| matches!(*mode, "light" | "dark"))
            .ok_or_else(|| AppError::new("THEME_INVALID", "Theme mode must be light or dark"))?,
        None => "light",
    };
    let mut palette = base_palette(mode);
    if let Some(raw_palette) = object.get("palette") {
        let input_palette = raw_palette
            .as_object()
            .ok_or_else(|| AppError::new("THEME_INVALID", "Theme palette must be an object"))?;
        for (key, value) in input_palette {
            if !PALETTE_KEYS.contains(&key.as_str()) {
                return Err(AppError::new("THEME_INVALID", "Unsupported palette token"));
            }
            let color = value
                .as_str()
                .ok_or_else(|| AppError::new("THEME_INVALID", "Palette colors must be strings"))?;
            if !is_valid_css_color(color) {
                return Err(AppError::new(
                    "THEME_INVALID",
                    "Palette colors must be safe CSS colors",
                ));
            }
            palette.insert(key.clone(), Value::String(color.trim().to_string()));
        }
    }
    let mut normalized = serde_json::Map::new();
    normalized.insert("name".to_string(), Value::String(name));
    normalized.insert("mode".to_string(), Value::String(mode.to_string()));
    normalized.insert("palette".to_string(), Value::Object(palette));
    Ok(Value::Object(normalized))
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
    let custom_theme = json
        .filter(|value| !value.trim().is_empty())
        .map(|value| serde_json::from_str::<Value>(&value))
        .transpose()?
        .map(|theme| normalize_custom_theme(&theme))
        .transpose()?;
    let theme_choice = normalize_theme_choice(&theme_choice)?;
    let theme_choice = if theme_choice == "custom" && custom_theme.is_none() {
        "system".to_string()
    } else {
        theme_choice
    };
    let locale_choice = if locale_choice == "system" {
        "auto".to_string()
    } else {
        locale_choice
    };
    Ok(AppSettings {
        theme_choice,
        custom_theme,
        locale_choice,
    })
}

pub fn write_settings(connection: &Connection, settings: &AppSettings) -> AppResult<()> {
    let theme_choice = normalize_theme_choice(&settings.theme_choice)?;
    validate_locale(&settings.locale_choice)?;
    let custom_theme = settings
        .custom_theme
        .as_ref()
        .map(normalize_custom_theme)
        .transpose()?;
    if theme_choice == "custom" && custom_theme.is_none() {
        return Err(AppError::new(
            "THEME_INVALID",
            "A custom theme has not been saved",
        ));
    }
    let json = custom_theme
        .as_ref()
        .map(serde_json::to_string)
        .transpose()?;
    connection.execute(
        "UPDATE app_config SET theme_choice=?1,custom_theme_json=?2,locale_choice=?3,updated_at=?4 WHERE id=1",
        params![theme_choice, json, settings.locale_choice, super::now_unix_ms()],
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
    let descriptor = super::table_descriptor(table).ok_or_else(|| {
        AppError::new(
            "TABLE_NOT_CLEARABLE",
            "Table is not registered for clearing",
        )
    })?;
    if !descriptor.clearable {
        return Err(AppError::new(
            "TABLE_NOT_CLEARABLE",
            "Table is not clearable",
        ));
    }
    match descriptor.name {
        "user_preferences" => reset_preferences(connection),
        _ => Err(AppError::new(
            "TABLE_NOT_CLEARABLE",
            "Table is not clearable",
        )),
    }
}
