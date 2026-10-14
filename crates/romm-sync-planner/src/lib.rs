//! Pure, deterministic reconciliation planning for `RomM` Save Syncer.
//!
//! This crate intentionally has no filesystem, process, network, database,
//! clock, randomness, or global-state dependencies. Callers collect evidence,
//! normalize it into [`ReconciliationInput`] values, and pass those immutable
//! values to [`plan`].

mod contracts;
mod domain;
mod planner;
mod policy;

pub use contracts::*;
pub use domain::*;
pub use planner::*;
pub use policy::*;
