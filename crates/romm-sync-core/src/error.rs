use std::{io, path::PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("{operation} failed for {path}: {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("invalid discovery manifest: {0}")]
    InvalidManifest(String),
    #[error("unsafe relative path: {0}")]
    UnsafePath(String),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("SQLite error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("state database integrity check failed: {0}")]
    DatabaseIntegrity(String),
    #[error("executor rejected the operation: {0}")]
    Rejected(String),
    #[error("remote transport failed: {0}")]
    Remote(String),
    #[error("fault injected during test: {0}")]
    Injected(String),
}

pub(crate) fn io_error(
    operation: &'static str,
    path: impl Into<PathBuf>,
    source: io::Error,
) -> CoreError {
    CoreError::Io {
        operation,
        path: path.into(),
        source,
    }
}
