//! ISSUE-003: Vector Primitive PrimitiveStorageExt Implementation
//!
//! **Status**: FIXED
//! **Location**: `/crates/primitives/src/vector/store.rs`
//!
//! VectorStore now implements `PrimitiveStorageExt` with:
//! 1. primitive_type_id() returning 7
//! 2. wal_entry_types() returning [0x70, 0x71, 0x72, 0x73]
//! 3. snapshot_serialize() / snapshot_deserialize()
//! 4. apply_wal_entry() for WAL replay
//! 5. primitive_name() returning "vector"
//! 6. rebuild_indexes() (no-op for M8 BruteForce)
//!
//! ## Test Strategy
//!
//! 1. Verify VectorStore implements PrimitiveStorageExt trait
//! 2. Verify primitive_type_id() returns 7
//! 3. Verify wal_entry_types() returns correct types
//! 4. Verify snapshot serialization works through PrimitiveStorageExt
//! 5. Verify primitive_name() returns "vector"

use crate::test_utils::*;
use strata_storage::PrimitiveStorageExt;

/// Test that VectorStore implements PrimitiveStorageExt trait.
#[test]
fn test_vector_implements_primitive_storage_ext() {
    let test_db = TestDb::new();
    let vector_store = test_db.vector();

    // Verify VectorStore implements PrimitiveStorageExt
    fn assert_primitive_storage_ext<T: PrimitiveStorageExt>(_: &T) {}
    assert_primitive_storage_ext(&vector_store);
}

/// Test that VectorStore's primitive_type_id is 7.
#[test]
fn test_vector_primitive_type_id() {
    let test_db = TestDb::new();
    let vector_store = test_db.vector();

    // ISSUE-003 fixed: primitive_type_id() returns 7
    assert_eq!(
        vector_store.primitive_type_id(),
        7,
        "Vector primitive type ID should be 7 per STORAGE_EXTENSION_GUIDE.md"
    );
}

/// Test that VectorStore's wal_entry_types returns Vector WAL types.
#[test]
fn test_vector_wal_entry_types() {
    let test_db = TestDb::new();
    let vector_store = test_db.vector();

    // ISSUE-003 fixed: wal_entry_types() returns correct types
    let wal_types = vector_store.wal_entry_types();

    // Per WAL_ENTRY_TYPES.md, Vector uses 0x70-0x73
    assert!(
        wal_types.contains(&0x70),
        "Should contain VectorCollectionCreate"
    );
    assert!(
        wal_types.contains(&0x71),
        "Should contain VectorCollectionDelete"
    );
    assert!(wal_types.contains(&0x72), "Should contain VectorUpsert");
    assert!(wal_types.contains(&0x73), "Should contain VectorDelete");
    assert_eq!(wal_types.len(), 4, "Should have exactly 4 WAL entry types");
}

/// Test that VectorStore snapshot serialization works through trait.
#[test]
fn test_vector_snapshot_serialization_via_trait() {
    let test_db = TestDb::new_strict();
    let vector_store = test_db.vector();
    let run_id = test_db.run_id;

    // Create a collection with vectors
    let collection = "snap_test";
    vector_store
        .create_collection(run_id, collection, config_small())
        .expect("Should create collection");

    for i in 0..5 {
        let key = format!("vec_{}", i);
        let vec = seeded_vector(3, i as u64);
        vector_store
            .insert(run_id, collection, &key, &vec, None)
            .expect("Should insert");
    }

    // ISSUE-003 fixed: Serialize through trait
    // Use fully qualified syntax to call the trait method
    let serialized = PrimitiveStorageExt::snapshot_serialize(&vector_store)
        .expect("Should serialize");
    assert!(!serialized.is_empty(), "Serialized data should not be empty");

    // Verify vectors can still be read (sanity check)
    for i in 0..5 {
        let key = format!("vec_{}", i);
        let entry = vector_store
            .get(run_id, collection, &key)
            .expect("Should get");
        assert!(entry.is_some(), "Vector {} should exist", i);
    }
}

/// Test that primitive_name returns "vector".
#[test]
fn test_vector_primitive_name() {
    let test_db = TestDb::new();
    let vector_store = test_db.vector();

    // ISSUE-003 fixed: primitive_name() returns "vector"
    assert_eq!(
        vector_store.primitive_name(),
        "vector",
        "Primitive name should be 'vector'"
    );
}

/// Test that handles_entry_type works correctly.
#[test]
fn test_vector_handles_entry_type() {
    let test_db = TestDb::new();
    let vector_store = test_db.vector();

    // Vector should handle its entry types
    assert!(
        vector_store.handles_entry_type(0x70),
        "Should handle VectorCollectionCreate"
    );
    assert!(
        vector_store.handles_entry_type(0x71),
        "Should handle VectorCollectionDelete"
    );
    assert!(
        vector_store.handles_entry_type(0x72),
        "Should handle VectorUpsert"
    );
    assert!(
        vector_store.handles_entry_type(0x73),
        "Should handle VectorDelete"
    );

    // Should NOT handle other entry types
    assert!(
        !vector_store.handles_entry_type(0x10),
        "Should NOT handle KV entry types"
    );
    assert!(
        !vector_store.handles_entry_type(0x20),
        "Should NOT handle JSON entry types"
    );
    assert!(
        !vector_store.handles_entry_type(0x00),
        "Should NOT handle core entry types"
    );
}

/// Test that rebuild_indexes works (no-op for M8 BruteForce).
#[test]
fn test_vector_rebuild_indexes() {
    let test_db = TestDb::new_strict();
    let mut vector_store = test_db.vector();
    let run_id = test_db.run_id;

    // Create collection with vectors
    let collection = "index_test";
    vector_store
        .create_collection(run_id, collection, config_small())
        .expect("Create collection");

    for i in 0..10 {
        let key = format!("v_{}", i);
        let vec = seeded_vector(3, i as u64);
        vector_store
            .insert(run_id, collection, &key, &vec, None)
            .expect("Insert");
    }

    // ISSUE-003 fixed: rebuild_indexes works (no-op for BruteForce)
    vector_store
        .rebuild_indexes()
        .expect("Should rebuild indexes");

    // Verify search still works after rebuild
    let query = seeded_vector(3, 5);
    let results = vector_store
        .search(run_id, collection, &query, 3, None)
        .expect("Search");
    assert!(!results.is_empty(), "Search should work after index rebuild");
}

/// Test VectorStore is Send + Sync (required by PrimitiveStorageExt).
#[test]
fn test_vector_store_send_sync() {
    fn assert_send_sync<T: Send + Sync + PrimitiveStorageExt>() {}
    assert_send_sync::<strata_primitives::VectorStore>();
}
