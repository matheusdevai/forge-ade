use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

/// Error type returned by every Tauri command.
///
/// Serialized to the frontend as `{ kind, message }`. Messages must never
/// contain secrets — they are shown in the UI and may end up in logs.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Git(String),
    #[error("{0}")]
    Internal(String),
}

impl AppError {
    fn kind(&self) -> &'static str {
        match self {
            AppError::Db(_) => "db",
            AppError::Io(_) => "io",
            AppError::Json(_) => "json",
            AppError::NotFound(_) => "not_found",
            AppError::Invalid(_) => "invalid",
            AppError::Forbidden(_) => "forbidden",
            AppError::Conflict(_) => "conflict",
            AppError::Git(_) => "git",
            AppError::Internal(_) => "internal",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("kind", self.kind())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;
