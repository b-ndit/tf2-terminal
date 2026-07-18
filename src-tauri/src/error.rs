use serde::Serialize;
use specta::Type;

/// Application-wide error type. Every IPC command returns `Result<T, AppError>`.
///
/// Each variant maps to a stable `code` so the frontend can localize/branch on
/// error kind without string-matching the message.
#[derive(Debug, thiserror::Error, Serialize, Type)]
#[serde(tag = "code", content = "message", rename_all = "snake_case")]
pub enum AppError {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("database error: {0}")]
    Database(String),

    #[error("keychain error: {0}")]
    Keychain(String),

    #[error("network error: {0}")]
    Network(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("export error: {0}")]
    Export(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        AppError::Database(err.to_string())
    }
}

impl From<sqlx::migrate::MigrateError> for AppError {
    fn from(err: sqlx::migrate::MigrateError) -> Self {
        AppError::Database(err.to_string())
    }
}

impl From<keyring_core::Error> for AppError {
    fn from(err: keyring_core::Error) -> Self {
        AppError::Keychain(err.to_string())
    }
}

impl From<reqwest::Error> for AppError {
    fn from(err: reqwest::Error) -> Self {
        AppError::Network(err.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        AppError::Internal(err.to_string())
    }
}

impl From<toml::de::Error> for AppError {
    fn from(err: toml::de::Error) -> Self {
        AppError::Config(err.to_string())
    }
}

impl From<toml::ser::Error> for AppError {
    fn from(err: toml::ser::Error) -> Self {
        AppError::Config(err.to_string())
    }
}

impl From<tauri_plugin_notification::Error> for AppError {
    fn from(err: tauri_plugin_notification::Error) -> Self {
        AppError::Internal(err.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
