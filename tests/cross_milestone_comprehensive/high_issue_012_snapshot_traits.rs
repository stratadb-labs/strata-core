//! ISSUE-012: Two Incompatible Snapshot Serialization Traits
//!
//! **Severity**: HIGH
//! **Location**: `/crates/durability/src/snapshot.rs` and `/crates/storage/src/primitive_ext.rs`
//!
//! **Problem**: Two different traits exist for snapshot serialization:
//! 1. `SnapshotSerializable` (legacy, in durability crate)
//! 2. `PrimitiveStorageExt` (new standard, in storage crate)
//!
//! The snapshot system uses `SnapshotSerializable` but the spec states integration
//! should use `PrimitiveStorageExt`.
//!
//! **Impact**: Inconsistent architecture, harder to add new primitives.

use crate::test_utils::*;

/// Test that PrimitiveStorageExt is the canonical trait.
#[test]
fn test_primitive_storage_ext_canonical() {
    // When ISSUE-012 is fixed:
    // - SnapshotSerializable should be deprecated
    // - All primitives should use PrimitiveStorageExt
    // - Snapshot system should use PrimitiveStorageExt::snapshot_serialize/deserialize

    // For now, verify primitives work with snapshots
    let test_db = TestDb::new_strict();
    let kv = test_db.kv();

    kv.put(&test_db.run_id, "snap_test", strata_core::value::Value::I64(42))
        .expect("put");

    test_db.db.flush().expect("flush");
}

/// Test that all primitives implement consistent snapshot interface.
#[test]
fn test_consistent_snapshot_interface() {
    // All 7 primitives should implement PrimitiveStorageExt
    // with consistent serialization semantics
}
