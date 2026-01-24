//! ISSUE-001: VectorStore Missing Searchable Trait Implementation
//!
//! **Severity**: CRITICAL
//! **Location**: `/crates/primitives/src/vector/store.rs`
//!
//! **Problem**: VectorStore has search methods but does NOT implement the `Searchable` trait.
//! All 5 other primitives implement this trait, but VectorStore is missing.
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
use strata_core::contract::PrimitiveType;
use strata_primitives::Searchable;

/// Test that VectorStore implements the Searchable trait.
///
/// ISSUE-001 has been fixed. VectorStore now implements Searchable.
///
/// Per M8_ARCHITECTURE.md Section 12.3, VectorStore returns empty results
/// for keyword search mode (it requires explicit embedding queries).
#[test]
fn test_vector_store_implements_searchable() {
    let test_db = TestDb::new();
    let vector_store = test_db.vector();

    // VectorStore now implements Searchable
    fn assert_searchable<T: Searchable>(_: &T) {}
    assert_searchable(&vector_store);

    // Verify primitive_kind returns Vector
    assert_eq!(vector_store.primitive_kind(), PrimitiveType::Vector);
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

/// Test that VectorStore primitive_kind would return PrimitiveType::Vector.
///
/// When ISSUE-001 is fixed, VectorStore.primitive_kind() should return PrimitiveType::Vector.
#[test]
fn test_vector_primitive_kind_exists() {
    // Verify the PrimitiveType::Vector variant exists
    let _kind = PrimitiveType::Vector;

    // When ISSUE-001 is fixed, this should work:
    // let test_db = TestDb::new();
    // let vector_store = test_db.vector();
    // assert_eq!(vector_store.primitive_kind(), PrimitiveType::Vector);
}

/// Test that all 5 searchable primitives can be collected as Searchable.
///
/// This tests polymorphic usage of all primitives that implement Searchable.
/// ISSUE-001 is fixed: VectorStore now implements Searchable.
#[test]
fn test_all_primitives_as_searchable() {
    let test_db = TestDb::new();
    let p = test_db.all_primitives();

    // 5 primitives implement Searchable
    // (RunIndex is excluded as it has different semantics)
    let searchables: Vec<&dyn Searchable> = vec![
        &p.kv,
        &p.json,
        &p.event,
        &p.state,
        &p.vector, // ISSUE-001 fixed: VectorStore now included
    ];

    // Verify we can iterate and check primitive kinds
    for searchable in &searchables {
        let kind = searchable.primitive_kind();
        assert!(
            matches!(
                kind,
                PrimitiveType::Kv
                    | PrimitiveType::Json
                    | PrimitiveType::Event
                    | PrimitiveType::State
                    | PrimitiveType::Vector
            ),
            "Primitive kind should be one of the expected types"
        );
    }

    // 5 primitives implement Searchable
    assert_eq!(
        searchables.len(),
        5,
        "All 5 searchable primitives should implement Searchable"
    );
}

/// Test that VectorStore returns empty for keyword search (by design).
///
/// Per M8_ARCHITECTURE.md, VectorStore does NOT do keyword search on metadata.
/// The hybrid search orchestrator must call search_by_embedding() directly
/// with an explicit embedding vector.
#[test]
fn test_vector_keyword_search_returns_empty() {
    use strata_core::search_types::SearchRequest;

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

    // Keyword search via Searchable trait returns empty (by design)
    let search_req = SearchRequest::new(run_id, "test query").with_k(5);
    let response = Searchable::search(&vector_store, &search_req).expect("Searchable::search should work");
    assert!(
        response.hits.is_empty(),
        "Keyword search should return empty for VectorStore"
    );

    // Direct vector search with embedding still works
    let query = seeded_vector(3, 42);
    let results = vector_store
        .search(run_id, collection, &query, 5, None)
        .expect("Direct search should work");

    assert!(!results.is_empty(), "Direct vector search should return results");
}

/// Test that SearchRequest structure is compatible.
///
/// When ISSUE-001 is fixed, SearchRequest should work with VectorStore.search().
#[test]
fn test_search_request_structure() {
    let test_db = TestDb::new();
    let run_id = test_db.run_id;

    // SearchRequest uses builder pattern, not Default
    let _search_req = strata_core::search_types::SearchRequest::new(run_id, "test query")
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
