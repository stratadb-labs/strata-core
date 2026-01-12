//! Concurrency layer for in-mem
//!
//! This crate implements optimistic concurrency control (OCC) with:
//! - TransactionContext: Read/write set tracking
//! - TransactionManager: Atomic commit coordination
//! - Snapshot isolation via ClonedSnapshotView
//! - Conflict detection at commit time (Story #83)
//! - Compare-and-swap (CAS) operations
//! - WAL integration for durability

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod manager;
pub mod snapshot;
pub mod transaction;
pub mod validation;
pub mod wal_writer;

pub use manager::TransactionManager;
pub use snapshot::ClonedSnapshotView;
pub use transaction::{
    ApplyResult, CASOperation, CommitError, PendingOperations, TransactionContext,
    TransactionStatus,
};
pub use validation::{
    validate_cas_set, validate_read_set, validate_transaction, validate_write_set, ConflictType,
    ValidationResult,
};
pub use wal_writer::TransactionWALWriter;

// Re-export the SnapshotView trait from core for convenience
pub use in_mem_core::traits::SnapshotView;
