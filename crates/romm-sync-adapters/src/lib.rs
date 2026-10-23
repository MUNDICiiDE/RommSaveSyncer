//! Read-only emulator save adapters and platform readiness.
//!
//! Adapters translate one game's emulator-specific save location into the
//! M1B discovery and execution vocabulary. They read directories and save
//! bytes but never write them; all mutations still flow through the guarded
//! executor. Readiness stages are reviewed source facts: only a
//! target-validated configuration produces a planner-validated adapter.

mod battery;
mod gci;
mod identity;
mod readiness;
mod resolution;

use std::{io, path::PathBuf};

pub use battery::*;
pub use gci::*;
pub use identity::*;
pub use readiness::*;
pub use resolution::*;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("invalid adapter argument: {0}")]
    InvalidArgument(String),
    #[error("{operation} failed for {path}: {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

impl AdapterError {
    pub(crate) fn io(operation: &'static str, path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            operation,
            path: path.into(),
            source,
        }
    }
}
