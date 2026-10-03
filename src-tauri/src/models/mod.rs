use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub theme_choice: String,
    pub custom_theme: Option<Value>,
    pub locale_choice: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme_choice: "system".to_string(),
            custom_theme: None,
            locale_choice: "auto".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    pub show_logo: bool,
    pub show_tooltip: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            show_logo: true,
            show_tooltip: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageStats {
    pub path: String,
    pub is_custom: bool,
    pub default_path: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableStats {
    pub name: String,
    pub label_key: String,
    pub clearable: bool,
    pub row_count: u64,
    pub size_bytes: u64,
    pub estimated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageTableStats {
    pub total_bytes: u64,
    pub tables: Vec<TableStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageSnapshot {
    pub storage: StorageStats,
    pub table_stats: StorageTableStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub identifier: String,
    pub platform: String,
    pub arch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChildWindowOptions {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub message: Option<String>,
    pub width: Option<f64>,
    pub height: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChildWindowInfo {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChildEventPayload {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub message: Option<String>,
    pub data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChildClosedPayload {
    pub id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChildMessagePayload {
    pub from: String,
    pub data: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArgPattern {
    pub kind: String,
    pub value: Option<String>,
}

fn regex_match(pattern: &str, argument: &str) -> bool {
    regex::Regex::new(pattern)
        .map(|compiled| compiled.is_match(argument))
        .unwrap_or(false)
}

impl ArgPattern {
    pub fn matches(&self, argument: &str) -> bool {
        match self.kind.as_str() {
            "any" => true,
            "literal" => self.value.as_deref() == Some(argument),
            "prefix" => self
                .value
                .as_deref()
                .map(|prefix| argument.starts_with(prefix))
                .unwrap_or(false),
            "regex" => self
                .value
                .as_deref()
                .map(|pattern| regex_match(pattern, argument))
                .unwrap_or(false),
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandSpec {
    pub id: String,
    pub executable: String,
    pub arg_patterns: Vec<ArgPattern>,
    pub max_args: usize,
    pub cwd_root: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubprocessRequest {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubprocessInfo {
    pub id: String,
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub running: bool,
    pub exit_code: Option<i32>,
    pub success: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubprocessExitPayload {
    pub id: String,
    pub code: Option<i32>,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubprocessLinePayload {
    pub id: String,
    pub line: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubprocessSnapshot {
    pub info: SubprocessInfo,
    pub stdout: Vec<String>,
    pub stderr: Vec<String>,
    pub exit: Option<SubprocessExitPayload>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimePayload {
    pub formatted: String,
    pub unix_ms: i64,
}
