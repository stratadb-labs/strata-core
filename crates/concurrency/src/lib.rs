//! Concurrency layer for Strata
//!
//! This crate implements optimistic concurrency control (OCC) with:
//! - TransactionContext: Read/write set tracking
//! - TransactionManager: Atomic commit coordination
//! - RecoveryCoordinator: Database recovery from WAL
//! - Snapshot isolation via ClonedSnapshotView
//! - Conflict detection at commit time
//! - Compare-and-swap (CAS) operations
//! - WAL integration for durability
//! - JSON region-based conflict detection

#![warn(missing_docs)]
#![warn(clippy::all)]

pub(crate) mod conflict;
pub mod manager;
pub mod payload;
pub mod recovery;
pub mod snapshot;
pub mod transaction;
pub(crate) mod validation;

pub use manager::TransactionManager;
pub use payload::TransactionPayload;
pub use recovery::{RecoveryCoordinator, RecoveryResult, RecoveryStats};
pub use snapshot::ClonedSnapshotView;
pub use transaction::{CommitError, JsonStoreExt, TransactionContext, TransactionStatus};

// Re-export the SnapshotView trait from core for convenience
pub use strata_core::traits::SnapshotView;
