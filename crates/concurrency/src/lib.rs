//! Concurrency layer for in-mem
//!
//! This crate implements optimistic concurrency control (OCC) with:
//! - TransactionContext: Read/write set tracking
//! - Snapshot isolation via ClonedSnapshotView
//! - Conflict detection at commit time (Story #83)
//! - Compare-and-swap (CAS) operations

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod snapshot;
pub mod transaction;
// pub mod validation;  // Story #83

pub use snapshot::ClonedSnapshotView;
pub use transaction::{CASOperation, TransactionContext, TransactionStatus};

// Re-export the SnapshotView trait from core for convenience
pub use in_mem_core::traits::SnapshotView;
