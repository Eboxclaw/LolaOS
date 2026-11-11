//! Harness error type.

use thiserror::Error;

pub type Result<T> = std::result::Result<T, LolaError>;

#[derive(Debug, Error)]
pub enum LolaError {
    #[error("memory layer error: {0}")]
    Memory(#[from] lola_memory::LolaMemoryError),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_yaml::Error),
}
