use thiserror::Error;

/// Errors that can occur in the cache system.
#[derive(Debug, Error)]
pub enum CacheError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("RON serialization error: {0}")]
    RonSerialize(#[from] ron::Error),

    #[error("RON deserialization error: {0}")]
    RonDeserialize(#[from] ron::error::SpannedError),

    #[error("Cache entry not found: {0}")]
    NotFound(String),

    #[error("Invalid cache key (must be a non-empty relative path using '/' separators): {0}")]
    InvalidKey(String),
}
