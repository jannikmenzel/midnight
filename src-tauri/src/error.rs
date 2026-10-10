use serde::{Serialize, Serializer};

use crate::i18n::tr;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{}", tr!("Database error: {}", "Datenbankfehler: {}", .0))]
    Db(#[from] rusqlite::Error),
    #[error("{}", tr!("File error: {}", "Dateifehler: {}", .0))]
    Io(#[from] std::io::Error),
    #[error("{}", tr!("JSON error: {}", "JSON-Fehler: {}", .0))]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Msg(String),
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

pub fn msg(m: impl Into<String>) -> AppError {
    AppError::Msg(m.into())
}

pub trait Context<T> {
    fn ctx(self, what: &str) -> AppResult<T>;
}

impl<T, E: std::fmt::Display> Context<T> for Result<T, E> {
    fn ctx(self, what: &str) -> AppResult<T> {
        self.map_err(|e| AppError::Msg(format!("{what}: {e}")))
    }
}
