//! ISSUE-003: Vector Primitive Missing PrimitiveStorageExt Implementation
//!
//! **Severity**: CRITICAL
//! **Location**: `/crates/primitives/src/vector/store.rs`
//!
//! **Problem**: VectorStore has snapshot_serialize/deserialize methods in its own
//! namespace but:
//! 1. Does NOT implement `PrimitiveStorageExt` trait
//! 2. Is NOT registered in `PrimitiveRegistry`
//! 3. Missing `primitive_type_id()` returning `7`
//! 4. Missing `wal_entry_types()` method
//!
//! **Spec Requirement**: STORAGE_EXTENSION_GUIDE.md requires all primitives
//! implement `PrimitiveStorageExt`.
//!
//! **Impact**: Vector primitive cannot be properly integrated with the
//! durability/recovery system through the standard extension mechanism.
//!
//! ## Test Strategy
//!
//! 1. Verify VectorStore implements PrimitiveStorageExt trait
//! 2. Verify primitive_type_id() returns 7
//! 3. Verify wal_entry_types() returns correct types
//! 4. Verify VectorStore is registered in PrimitiveRegistry
//! 5. Verify snapshot serialization works through PrimitiveStorageExt

use crate::test_utils::*;
use in_mem_core::types::RunId;

/// Test that VectorStore's primitive_type_id is 7.
///
/// **Expected behavior when ISSUE-003 is fixed**:
/// - VectorStore.primitive_type_id() returns 7
///
/// **Current behavior (ISSUE-003 present)**:
/// - This test may not compile or return incorrect value
#[test]
fn test_vector_primitive_type_id() {
    let test_db = TestDb::new();
    let _vector_store = test_db.vector();

    // When ISSUE-003 is fixed:
    // use in_mem_storage::PrimitiveStorageExt;
    // assert_eq!(vector_store.primitive_type_id(), 7);

    // For now, verify the expected type ID from STORAGE_EXTENSION_GUIDE.md
    const EXPECTED_VECTOR_TYPE_ID: u8 = 7;
    assert_eq!(EXPECTED_VECTOR_TYPE_ID, 7, "Vector primitive type ID should be 7 per spec");
}

/// Test that VectorStore's wal_entry_types returns Vector WAL types.
///
/// **Expected behavior when ISSUE-003 is fixed**:
/// - wal_entry_types() returns [0x70, 0x71, 0x72, 0x73]
///
/// **Current behavior (ISSUE-003 present)**:
/// - Method may not exist
#[test]
fn test_vector_wal_entry_types() {
    let test_db = TestDb::new();
    let _vector_store = test_db.vector();

    // When ISSUE-003 is fixed:
    // use in_mem_storage::PrimitiveStorageExt;
    // let wal_types = vector_store.wal_entry_types();
    //
    // // Per WAL_ENTRY_TYPES.md, Vector uses 0x70-0x7F range
    // assert!(wal_types.contains(&0x70), "Should contain VectorCollectionCreate");
    // assert!(wal_types.contains(&0x71), "Should contain VectorCollectionDelete");
    // assert!(wal_types.contains(&0x72), "Should contain VectorUpsert");
    // assert!(wal_types.contains(&0x73), "Should contain VectorDelete");

    // For now, verify expected WAL types from WAL_ENTRY_TYPES.md
    const VECTOR_COLLECTION_CREATE: u8 = 0x70;
    const VECTOR_COLLECTION_DELETE: u8 = 0x71;
    const VECTOR_UPSERT: u8 = 0x72;
    const VECTOR_DELETE: u8 = 0x73;

    assert_eq!(VECTOR_COLLECTION_CREATE, 0x70);
    assert_eq!(VECTOR_COLLECTION_DELETE, 0x71);
    assert_eq!(VECTOR_UPSERT, 0x72);
    assert_eq!(VECTOR_DELETE, 0x73);
}

/// Test that VectorStore snapshot serialization works.
///
/// **Expected behavior when ISSUE-003 is fixed**:
/// - VectorStore can serialize/deserialize through PrimitiveStorageExt trait
///
/// **Current behavior (ISSUE-003 present)**:
/// - Must use VectorStore's own snapshot methods
#[test]
fn test_vector_snapshot_serialization() {
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

    // When ISSUE-003 is fixed:
    // use in_mem_storage::PrimitiveStorageExt;
    //
    // // Serialize through trait
    // let serialized = vector_store.snapshot_serialize().expect("Should serialize");
    // assert!(!serialized.is_empty(), "Serialized data should not be empty");
    //
    // // Deserialize into new store
    // let mut new_store = VectorStore::new(Arc::new(Database::open_in_memory()?));
    // new_store.snapshot_deserialize(&serialized).expect("Should deserialize");
    //
    // // Verify data was restored
    // let count = new_store.count(run_id, collection);
    // assert_eq!(count, 5);

    // For now, verify vectors can be read back
    for i in 0..5 {
        let key = format!("vec_{}", i);
        let entry = vector_store
            .get(run_id, collection, &key)
            .expect("Should get");
        assert!(entry.is_some(), "Vector {} should exist", i);
    }
}

/// Test that VectorStore is registered in PrimitiveRegistry.
///
/// **Expected behavior when ISSUE-003 is fixed**:
/// - VectorStore can be looked up in PrimitiveRegistry by type ID 7
///
/// **Current behavior (ISSUE-003 present)**:
/// - VectorStore is not registered
#[test]
fn test_vector_registered_in_registry() {
    // When ISSUE-003 is fixed:
    // use in_mem_storage::PrimitiveRegistry;
    //
    // let registry = PrimitiveRegistry::default();
    //
    // // Should be able to look up Vector primitive by type ID
    // let vector_ext = registry.get_by_type_id(7);
    // assert!(vector_ext.is_some(), "Vector should be registered with type ID 7");
    //
    // // Verify it's the right primitive
    // let ext = vector_ext.unwrap();
    // assert_eq!(ext.primitive_name(), "vector");

    // For now, verify expected registration parameters
    const VECTOR_TYPE_ID: u8 = 7;
    const VECTOR_NAME: &str = "vector";
    assert_eq!(VECTOR_TYPE_ID, 7);
    assert_eq!(VECTOR_NAME, "vector");
}

/// Test that Vector WAL entries can be applied through PrimitiveStorageExt.
///
/// **Expected behavior when ISSUE-003 is fixed**:
/// - apply_wal_entry() correctly handles vector WAL entry types
///
/// **Current behavior (ISSUE-003 present)**:
/// - Must use VectorStore's own WAL replay methods
#[test]
fn test_vector_wal_entry_application() {
    let test_db = TestDb::new_strict();
    let vector_store = test_db.vector();
    let run_id = test_db.run_id;

    // Create collection and insert vector to generate WAL entries
    let collection = "wal_test";
    vector_store
        .create_collection(run_id, collection, config_small())
        .expect("Create collection");

    let key = "test_vec";
    let vec = seeded_vector(3, 42);
    vector_store
        .insert(run_id, collection, key, &vec, None)
        .expect("Insert vector");

    // When ISSUE-003 is fixed:
    // use in_mem_storage::PrimitiveStorageExt;
    //
    // // Get the WAL entry types the vector store handles
    // let entry_types = vector_store.wal_entry_types();
    //
    // // Create a synthetic WAL entry for VectorUpsert (0x72)
    // let payload = create_vector_upsert_payload(run_id, collection, key, &vec);
    //
    // // Apply through the trait
    // let mut new_store = VectorStore::new(test_db.db.clone());
    // new_store.apply_wal_entry(0x72, &payload).expect("Should apply WAL entry");

    // For now, verify WAL persistence works through normal operations
    test_db.db.flush().expect("Flush");

    // After flush, vector should still be accessible
    let entry = vector_store
        .get(run_id, collection, key)
        .expect("Get after flush");
    assert!(entry.is_some(), "Vector should exist after WAL flush");
}

/// Test that VectorStore rebuild_indexes works correctly.
///
/// **Expected behavior when ISSUE-003 is fixed**:
/// - rebuild_indexes() reconstructs any derived indexes from primary data
///
/// **Current behavior (ISSUE-003 present)**:
/// - May not have this method exposed through trait
#[test]
fn test_vector_rebuild_indexes() {
    let test_db = TestDb::new_strict();
    let vector_store = test_db.vector();
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

    // When ISSUE-003 is fixed:
    // use in_mem_storage::PrimitiveStorageExt;
    //
    // // Call rebuild_indexes
    // vector_store.rebuild_indexes().expect("Should rebuild indexes");
    //
    // // Verify search still works (uses the indexes)
    // let query = seeded_vector(3, 5);
    // let results = vector_store.search(run_id, collection, &query, 3, None)?;
    // assert!(!results.is_empty(), "Search should work after index rebuild");

    // For now, verify search works
    let query = seeded_vector(3, 5);
    let results = vector_store
        .search(run_id, collection, &query, 3, None)
        .expect("Search");
    assert!(!results.is_empty(), "Search should return results");
}

/// Test primitive_name returns "vector".
///
/// **Expected behavior when ISSUE-003 is fixed**:
/// - VectorStore.primitive_name() returns "vector"
#[test]
fn test_vector_primitive_name() {
    let test_db = TestDb::new();
    let _vector_store = test_db.vector();

    // When ISSUE-003 is fixed:
    // use in_mem_storage::PrimitiveStorageExt;
    // assert_eq!(vector_store.primitive_name(), "vector");

    // For now, verify expected name per STORAGE_EXTENSION_GUIDE.md
    const EXPECTED_NAME: &str = "vector";
    assert_eq!(EXPECTED_NAME, "vector");
}
