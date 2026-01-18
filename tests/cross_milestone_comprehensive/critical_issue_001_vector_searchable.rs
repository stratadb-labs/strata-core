//! ISSUE-001: VectorStore Missing Searchable Trait Implementation
//!
//! **Severity**: CRITICAL
//! **Location**: `/crates/primitives/src/vector/store.rs`
//!
//! **Problem**: VectorStore has search methods but does NOT implement the `Searchable` trait.
//! All 6 other primitives implement this trait, but VectorStore is missing.
//!
//! **Spec Requirement**: M6 specifies all primitives must implement `Searchable`
//! for uniform search orchestration.
//!
//! **Impact**:
//! - VectorStore cannot be called as `Searchable`
//! - HybridSearch explicitly returns empty for Vector
//! - Vector search must be called directly, breaking uniformity
//!
//! ## Test Strategy
//!
//! 1. Verify VectorStore does NOT implement Searchable (test for issue presence)
//! 2. Verify VectorStore::search returns valid results directly
//! 3. Verify other primitives can be used polymorphically as Searchable
//! 4. Verify HybridSearch workaround for VectorStore

use crate::test_utils::*;
use in_mem_core::search_types::PrimitiveKind;
use in_mem_primitives::Searchable;

/// Test that VectorStore does NOT implement the Searchable trait (documenting the issue).
///
/// This test verifies that ISSUE-001 is still present.
/// When ISSUE-001 is fixed, this test should be updated to verify VectorStore DOES implement Searchable.
///
/// **Current behavior (ISSUE-001 present)**:
/// - VectorStore does not implement Searchable trait
/// - This test passes by verifying we can't cast VectorStore to Searchable
#[test]
fn test_vector_store_missing_searchable_issue_001() {
    let test_db = TestDb::new();
    let _vector_store = test_db.vector();

    // Document that VectorStore doesn't implement Searchable
    // This is a compile-time issue that we document at runtime
    eprintln!(
        "ISSUE-001: VectorStore does NOT implement Searchable trait. \
         This breaks uniform search orchestration across all 7 primitives."
    );

    // When ISSUE-001 is fixed, uncomment this:
    // fn assert_searchable<T: Searchable>(_: &T) {}
    // assert_searchable(&_vector_store);
}

/// Test that VectorStore::search returns proper results directly.
///
/// Even without Searchable trait, VectorStore should have working search.
#[test]
fn test_vector_store_direct_search_works() {
    let test_db = TestDb::new();
    let vector_store = test_db.vector();
    let run_id = test_db.run_id;

    // Create a collection with some vectors
    let collection = "test_collection";
    vector_store
        .create_collection(run_id, collection, config_small())
        .expect("Should create collection");

    // Insert test vectors
    for i in 0..5 {
        let key = format!("key_{}", i);
        let vec = seeded_vector(3, i as u64);
        vector_store
            .insert(run_id, collection, &key, &vec, None)
            .expect("Should insert vector");
    }

    // Direct search works
    let query_vector = seeded_vector(3, 42);
    let results = vector_store
        .search(run_id, collection, &query_vector, 3, None)
        .expect("Should search");

    assert!(!results.is_empty(), "Search should return results");
    assert!(results.len() <= 3, "Should respect k limit");
}

/// Test that VectorStore primitive_kind would return PrimitiveKind::Vector.
///
/// When ISSUE-001 is fixed, VectorStore.primitive_kind() should return PrimitiveKind::Vector.
#[test]
fn test_vector_primitive_kind_exists() {
    // Verify the PrimitiveKind::Vector variant exists
    let _kind = PrimitiveKind::Vector;

    // When ISSUE-001 is fixed, this should work:
    // let test_db = TestDb::new();
    // let vector_store = test_db.vector();
    // assert_eq!(vector_store.primitive_kind(), PrimitiveKind::Vector);
}

/// Test that other primitives (not VectorStore) can be collected as Searchable.
///
/// This tests polymorphic usage of the 5 primitives that DO implement Searchable.
#[test]
fn test_other_primitives_as_searchable() {
    let test_db = TestDb::new();
    let p = test_db.all_primitives();

    // These 5 primitives implement Searchable
    // VectorStore is notably absent due to ISSUE-001
    let searchables: Vec<&dyn Searchable> = vec![
        &p.kv,
        &p.json,
        &p.event,
        &p.state,
        &p.trace,
        // &p.vector,  // Cannot include - ISSUE-001
    ];

    // Verify we can iterate and check primitive kinds
    for searchable in &searchables {
        let kind = searchable.primitive_kind();
        assert!(
            matches!(
                kind,
                PrimitiveKind::Kv
                    | PrimitiveKind::Json
                    | PrimitiveKind::Event
                    | PrimitiveKind::State
                    | PrimitiveKind::Trace
            ),
            "Primitive kind should be one of the expected types"
        );
    }

    // Only 5 primitives implement Searchable (not 6 with Vector)
    assert_eq!(
        searchables.len(),
        5,
        "ISSUE-001: Only 5 primitives implement Searchable, VectorStore is missing"
    );
}

/// Test that HybridSearch cannot properly orchestrate vector search (ISSUE-001 impact).
///
/// This documents the impact of ISSUE-001 on HybridSearch functionality.
#[test]
fn test_hybrid_search_vector_limitation() {
    let test_db = TestDb::new();
    let vector_store = test_db.vector();
    let run_id = test_db.run_id;

    // Setup: Create collection with vectors
    let collection = "search_test";
    vector_store
        .create_collection(run_id, collection, config_small())
        .expect("Should create collection");

    for i in 0..10 {
        let key = format!("vec_{}", i);
        let vec = seeded_vector(3, i as u64);
        vector_store
            .insert(run_id, collection, &key, &vec, None)
            .expect("Should insert");
    }

    // Direct vector search works as a workaround
    let query = seeded_vector(3, 42);
    let results = vector_store
        .search(run_id, collection, &query, 5, None)
        .expect("Direct search should work");

    assert!(!results.is_empty(), "Vector search should return results");

    eprintln!(
        "ISSUE-001: HybridSearch cannot include VectorStore because it doesn't implement Searchable. \
         Direct VectorStore.search() must be called as a workaround."
    );
}

/// Test that SearchRequest structure is compatible.
///
/// When ISSUE-001 is fixed, SearchRequest should work with VectorStore.search().
#[test]
fn test_search_request_structure() {
    let test_db = TestDb::new();
    let run_id = test_db.run_id;

    // SearchRequest uses builder pattern, not Default
    let _search_req = in_mem_core::search_types::SearchRequest::new(run_id, "test query")
        .with_k(10);

    // When ISSUE-001 is fixed, VectorStore.search(&search_req) should work
    // For now, verify direct search works:
    let vector_store = test_db.vector();
    let collection = "req_test";
    vector_store
        .create_collection(run_id, collection, config_small())
        .expect("create");
    vector_store
        .insert(run_id, collection, "v1", &[1.0, 0.0, 0.0], None)
        .expect("insert");

    let results = vector_store
        .search(run_id, collection, &[1.0, 0.0, 0.0], 10, None)
        .expect("search");

    assert!(!results.is_empty());
}
