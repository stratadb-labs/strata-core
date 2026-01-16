//! Concurrency layer for in-mem
//!
//! This crate implements optimistic concurrency control (OCC) with:
//! - TransactionContext: Read/write set tracking
//! - TransactionManager: Atomic commit coordination
//! - RecoveryCoordinator: Database recovery from WAL
//! - Snapshot isolation via ClonedSnapshotView
//! - Conflict detection at commit time (Story #83)
//! - Compare-and-swap (CAS) operations
//! - WAL integration for durability
//! - JSON region-based conflict detection (M5)

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod conflict;
pub mod manager;
pub mod recovery;
pub mod snapshot;
pub mod transaction;
pub mod validation;
pub mod wal_writer;

pub use manager::TransactionManager;
pub use recovery::{RecoveryCoordinator, RecoveryResult, RecoveryStats};
pub use snapshot::ClonedSnapshotView;
pub use transaction::{
    ApplyResult, CASOperation, CommitError, JsonPatchEntry, JsonPathRead, JsonStoreExt,
    PendingOperations, TransactionContext, TransactionStatus,
};
pub use validation::{
    validate_cas_set, validate_json_paths, validate_json_set, validate_read_set,
    validate_transaction, validate_write_set, ConflictType, ValidationResult,
};
pub use wal_writer::TransactionWALWriter;

// JSON conflict detection (M5)
pub use conflict::{
    check_all_conflicts, check_read_write_conflicts, check_version_conflicts,
    check_write_write_conflicts, find_first_read_write_conflict, find_first_version_conflict,
    find_first_write_write_conflict, ConflictResult, JsonConflictError,
};

// Re-export the SnapshotView trait from core for convenience
pub use in_mem_core::traits::SnapshotView;
