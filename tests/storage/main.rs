//! Storage Crate Integration Tests
//!
//! Tests for strata-storage: MVCC, snapshots, compaction, retention.

#[path = "../common/mod.rs"]
mod common;

mod compaction;
mod format_validation;
mod mvcc_invariants;
mod retention_policy;
mod run_isolation;
mod snapshot_isolation;
mod stress;
