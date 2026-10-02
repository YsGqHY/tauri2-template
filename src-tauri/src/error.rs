use std::fmt::{Display, Formatter};
use std::io;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl AppError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            detail: None,
        }
    }

    pub fn with_detail(
        code: impl Into<String>,
        message: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            detail: Some(detail.into()),
        }
    }
}

impl Display for AppError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for AppError {}

impl From<io::Error> for AppError {
    fn from(value: io::Error) -> Self {
        Self::with_detail("IO_ERROR", "文件操作失败", value.to_string())
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(value: rusqlite::Error) -> Self {
        Self::with_detail("STORAGE_ERROR", "SQLite 操作失败", value.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(value: serde_json::Error) -> Self {
        Self::with_detail("JSON_ERROR", "JSON 数据无效", value.to_string())
    }
}

impl From<tauri::Error> for AppError {
    fn from(value: tauri::Error) -> Self {
        Self::with_detail("TAURI_ERROR", "Tauri 操作失败", value.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_stable_error_shape() {
        let value = AppError::with_detail("TEST_ERROR", "测试失败", "detail");
        let json = serde_json::to_value(value).expect("error should serialize");
        assert_eq!(json["code"], "TEST_ERROR");
        assert_eq!(json["message"], "测试失败");
        assert_eq!(json["detail"], "detail");
    }

    #[test]
    fn omits_missing_detail() {
        let value = AppError::new("TEST_ERROR", "测试失败");
        let json = serde_json::to_value(value).expect("error should serialize");
        assert!(json.get("detail").is_none());
    }
}
