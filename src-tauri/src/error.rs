use std::fmt::{Display, Formatter};

use serde::Serialize;

pub type AppResult<T> = Result<T, AppError>;

/// 可跨 Tauri Seam 返回的脱敏错误。底层错误文本可能包含 SQL 或路径，不能直接序列化。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: String,
    pub message: String,
}

impl AppError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn database() -> Self {
        Self::new("database_error", "分析数据库操作失败")
    }

    pub fn filesystem() -> Self {
        Self::new("filesystem_error", "本地文件操作失败")
    }
}

impl Display for AppError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for AppError {}

impl From<rusqlite::Error> for AppError {
    fn from(error: rusqlite::Error) -> Self {
        if let rusqlite::Error::SqliteFailure(failure, _) = error {
            return Self::new(
                format!("database_{:?}_{}", failure.code, failure.extended_code)
                    .to_ascii_lowercase(),
                "分析数据库操作失败",
            );
        }
        Self::database()
    }
}

impl From<r2d2::Error> for AppError {
    fn from(_: r2d2::Error) -> Self {
        Self::database()
    }
}

impl From<std::io::Error> for AppError {
    fn from(_: std::io::Error) -> Self {
        Self::filesystem()
    }
}
