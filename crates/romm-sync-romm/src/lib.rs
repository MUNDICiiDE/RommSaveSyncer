//! `RomM` 5.4 protocol boundary.
//!
//! This crate owns authentication, wire hashes, transfer acknowledgement,
//! retry timing, and sanitized tracing. Emulator-specific path and format
//! adapters remain outside this boundary.

mod backoff;
mod client;
mod credential;
mod error;
mod protocol;
mod trace;
mod transport;

pub use backoff::*;
pub use client::*;
pub use credential::*;
pub use error::*;
pub use protocol::*;
pub use trace::*;
pub use transport::*;
