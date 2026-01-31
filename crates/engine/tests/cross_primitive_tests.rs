//! Cross-Primitive Transaction Tests
//!
//! NOTE: These tests are disabled because they access internal implementation
//! details (db.storage()) which is intentionally pub(crate) only.
//! Cross-primitive transaction functionality should be tested via the public
//! primitive APIs (KVStore, EventLog, etc.) in run_isolation_tests.rs
//! and primitives_cross_tests.rs.
//!
//! Per architecture documentation GitHub Issue #99:
//! Validates that transactions atomically operate across different
//! Key types (KV and Event) in a single transaction.

// Tests below are commented out because they access db.storage() which is pub(crate)
// See run_isolation_tests.rs and primitives_cross_tests.rs for equivalent tests
// using the public primitive APIs.

/*
use strata_core::Storage;
use strata_core::StrataError;
use strata_core::types::{Key, Namespace, BranchId};
use strata_core::value::Value;
use strata_engine::Database;
use tempfile::TempDir;

// ... tests that use db.storage() ...
*/
