//! Kernel error type.

use thiserror::Error;

pub type Result<T> = std::result::Result<T, LolaError>;

#[derive(Debug, Error)]
pub enum LolaError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_yaml::Error),
}
