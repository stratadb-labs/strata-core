//! JsonStore Tier 1 Operations Tests
//!
//! Tests for M11B Tier 1 features:
//! - json_list: Document listing with cursor-based pagination
//! - json_cas: Compare-and-swap for optimistic concurrency
//! - json_query: Exact field matching

use crate::*;

// =============================================================================
// List Tests
// =============================================================================

/// Test basic list functionality
#[test]
fn test_json_list_returns_documents() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();

        // Create several documents
        db.json_set(&run, "doc1", "$", obj([("name", Value::String("Alice".into()))])).unwrap();
        db.json_set(&run, "doc2", "$", obj([("name", Value::String("Bob".into()))])).unwrap();
        db.json_set(&run, "doc3", "$", obj([("name", Value::String("Charlie".into()))])).unwrap();

        // List all documents
        let result = db.json_list(&run, None, None, 10).unwrap();
        assert_eq!(result.keys.len(), 3, "Should have 3 documents");
        assert!(result.next_cursor.is_none(), "Should have no more pages");
    });
}

/// Test list with limit enforces pagination
#[test]
fn test_json_list_pagination_works() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();

        // Create 5 documents
        for i in 0..5 {
            let key = format!("doc{}", i);
            db.json_set(&run, &key, "$", obj([("idx", Value::Int(i))])).unwrap();
        }

        // List with limit of 2
        let page1 = db.json_list(&run, None, None, 2).unwrap();
        assert_eq!(page1.keys.len(), 2, "First page should have 2 documents");
        assert!(page1.next_cursor.is_some(), "Should have next page cursor");

        // Get next page
        let page2 = db.json_list(&run, None, page1.next_cursor.as_deref(), 2).unwrap();
        assert_eq!(page2.keys.len(), 2, "Second page should have 2 documents");

        // Get final page
        let page3 = db.json_list(&run, None, page2.next_cursor.as_deref(), 2).unwrap();
        assert_eq!(page3.keys.len(), 1, "Final page should have 1 document");
        assert!(page3.next_cursor.is_none(), "Should have no more pages");
    });
}

/// Test list returns empty for empty store
#[test]
fn test_json_list_empty_store() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();

        let result = db.json_list(&run, None, None, 10).unwrap();
        assert!(result.keys.is_empty(), "Empty store should return empty list");
        assert!(result.next_cursor.is_none(), "Should have no cursor");
    });
}

/// Test list with run isolation
#[test]
fn test_json_list_run_isolation() {
    test_across_substrate_modes(|db| {
        let run1 = ApiRunId::default_run_id();
        let run2 = ApiRunId::new();

        // Create documents in run1
        db.json_set(&run1, "doc1", "$", Value::Int(1)).unwrap();
        db.json_set(&run1, "doc2", "$", Value::Int(2)).unwrap();

        // Create document in run2
        db.json_set(&run2, "doc3", "$", Value::Int(3)).unwrap();

        // List should be isolated per run
        let result1 = db.json_list(&run1, None, None, 10).unwrap();
        let result2 = db.json_list(&run2, None, None, 10).unwrap();

        assert_eq!(result1.keys.len(), 2, "Run1 should have 2 docs");
        assert_eq!(result2.keys.len(), 1, "Run2 should have 1 doc");
    });
}

// =============================================================================
// CAS Tests
// =============================================================================

/// Test CAS succeeds with correct version
#[test]
fn test_json_cas_succeeds_with_correct_version() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();
        let key = "cas_doc";

        // Create document (version 1)
        db.json_set(&run, key, "$", obj([("counter", Value::Int(0))])).unwrap();

        // Read current version
        let current = db.json_get(&run, key, "$").unwrap().unwrap();
        let version = match current.version {
            Version::Counter(v) => v,
            Version::Txn(v) => v,
            Version::Sequence(v) => v,
        };

        // CAS with correct version should succeed
        let new_version = db.json_cas(&run, key, version, "counter", Value::Int(1)).unwrap();
        assert!(matches!(new_version, Version::Counter(_) | Version::Txn(_)));

        // Verify value was updated
        let updated = db.json_get(&run, key, "counter").unwrap().unwrap();
        assert_eq!(updated.value, Value::Int(1));
    });
}

/// Test CAS fails with wrong version
#[test]
fn test_json_cas_fails_with_wrong_version() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();
        let key = "cas_fail_doc";

        // Create document
        db.json_set(&run, key, "$", obj([("counter", Value::Int(0))])).unwrap();

        // Try CAS with wrong version (0 when it should be 1)
        let result = db.json_cas(&run, key, 0, "counter", Value::Int(1));
        assert!(result.is_err(), "CAS with wrong version should fail");

        // Verify value was NOT updated
        let unchanged = db.json_get(&run, key, "counter").unwrap().unwrap();
        assert_eq!(unchanged.value, Value::Int(0), "Value should be unchanged");
    });
}

/// Test CAS fails on non-existent document
#[test]
fn test_json_cas_fails_on_nonexistent() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();

        let result = db.json_cas(&run, "nonexistent", 1, "field", Value::Int(1));
        assert!(result.is_err(), "CAS on non-existent doc should fail");
    });
}

/// Test concurrent CAS - exactly one wins
#[test]
fn test_concurrent_cas_exactly_one_wins() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::thread;

    let db = create_inmemory_db();
    let substrate = SubstrateImpl::new(db);
    let run = ApiRunId::default_run_id();
    let key = "concurrent_cas";

    // Create document
    substrate.json_set(&run, key, "$", obj([("counter", Value::Int(0))])).unwrap();

    // Get initial version
    let initial = substrate.json_get(&run, key, "$").unwrap().unwrap();
    let initial_version = match initial.version {
        Version::Counter(v) => v,
        Version::Txn(v) => v,
        Version::Sequence(v) => v,
    };

    let success_count = Arc::new(AtomicUsize::new(0));
    let substrate = Arc::new(substrate);

    // Spawn multiple threads trying to CAS with the same initial version
    let threads: Vec<_> = (0..5)
        .map(|i| {
            let substrate = substrate.clone();
            let success_count = success_count.clone();
            let run = run.clone();

            thread::spawn(move || {
                let result = substrate.json_cas(
                    &run,
                    key,
                    initial_version,
                    "counter",
                    Value::Int(i + 1),
                );
                if result.is_ok() {
                    success_count.fetch_add(1, Ordering::SeqCst);
                }
            })
        })
        .collect();

    for t in threads {
        t.join().unwrap();
    }

    // Exactly one CAS should have succeeded
    assert_eq!(success_count.load(Ordering::SeqCst), 1, "Exactly one CAS should win");
}

// =============================================================================
// Query Tests
// =============================================================================

/// Test query returns matching documents
#[test]
fn test_json_query_exact_match() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();

        // Create documents with different status values
        db.json_set(&run, "order1", "$", obj([("status", Value::String("pending".into()))])).unwrap();
        db.json_set(&run, "order2", "$", obj([("status", Value::String("completed".into()))])).unwrap();
        db.json_set(&run, "order3", "$", obj([("status", Value::String("pending".into()))])).unwrap();
        db.json_set(&run, "order4", "$", obj([("status", Value::String("cancelled".into()))])).unwrap();

        // Query for pending orders
        let results = db.json_query(&run, "status", Value::String("pending".into()), 10).unwrap();
        assert_eq!(results.len(), 2, "Should find 2 pending orders");

        // Query for completed orders
        let results = db.json_query(&run, "status", Value::String("completed".into()), 10).unwrap();
        assert_eq!(results.len(), 1, "Should find 1 completed order");
    });
}

/// Test query returns empty for no match
#[test]
fn test_json_query_returns_empty_for_no_match() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();

        // Create some documents
        db.json_set(&run, "doc1", "$", obj([("status", Value::String("active".into()))])).unwrap();
        db.json_set(&run, "doc2", "$", obj([("status", Value::String("active".into()))])).unwrap();

        // Query for non-existent value
        let results = db.json_query(&run, "status", Value::String("nonexistent".into()), 10).unwrap();
        assert!(results.is_empty(), "Should find no matching documents");
    });
}

/// Test query respects limit
#[test]
fn test_json_query_respects_limit() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();

        // Create many documents with same status
        for i in 0..10 {
            let key = format!("doc{}", i);
            db.json_set(&run, &key, "$", obj([("status", Value::String("active".into()))])).unwrap();
        }

        // Query with limit of 3
        let results = db.json_query(&run, "status", Value::String("active".into()), 3).unwrap();
        assert_eq!(results.len(), 3, "Should return only 3 results due to limit");
    });
}

/// Test query with nested path
#[test]
fn test_json_query_nested_path() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();

        // Create documents with nested structure
        db.json_set(&run, "user1", "$", obj([
            ("profile", obj([
                ("country", Value::String("USA".into()))
            ]))
        ])).unwrap();
        db.json_set(&run, "user2", "$", obj([
            ("profile", obj([
                ("country", Value::String("Canada".into()))
            ]))
        ])).unwrap();
        db.json_set(&run, "user3", "$", obj([
            ("profile", obj([
                ("country", Value::String("USA".into()))
            ]))
        ])).unwrap();

        // Query nested path
        let results = db.json_query(&run, "profile.country", Value::String("USA".into()), 10).unwrap();
        assert_eq!(results.len(), 2, "Should find 2 users from USA");
    });
}

/// Test query with different value types
#[test]
fn test_json_query_different_types() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();

        // Create documents with different types
        db.json_set(&run, "int_doc", "$", obj([("value", Value::Int(42))])).unwrap();
        db.json_set(&run, "float_doc", "$", obj([("value", Value::Float(42.5))])).unwrap();
        db.json_set(&run, "str_doc", "$", obj([("value", Value::String("42".into()))])).unwrap();
        db.json_set(&run, "bool_doc", "$", obj([("value", Value::Bool(true))])).unwrap();

        // Query for integer 42
        let results = db.json_query(&run, "value", Value::Int(42), 10).unwrap();
        assert_eq!(results.len(), 1, "Should find only the int document");

        // Query for boolean true
        let results = db.json_query(&run, "value", Value::Bool(true), 10).unwrap();
        assert_eq!(results.len(), 1, "Should find only the bool document");
    });
}

/// Test query with run isolation
#[test]
fn test_json_query_run_isolation() {
    test_across_substrate_modes(|db| {
        let run1 = ApiRunId::default_run_id();
        let run2 = ApiRunId::new();

        // Create documents in both runs with same status
        db.json_set(&run1, "doc1", "$", obj([("status", Value::String("active".into()))])).unwrap();
        db.json_set(&run2, "doc2", "$", obj([("status", Value::String("active".into()))])).unwrap();

        // Query should be isolated per run
        let results1 = db.json_query(&run1, "status", Value::String("active".into()), 10).unwrap();
        let results2 = db.json_query(&run2, "status", Value::String("active".into()), 10).unwrap();

        assert_eq!(results1.len(), 1, "Run1 should have 1 matching doc");
        assert_eq!(results2.len(), 1, "Run2 should have 1 matching doc");
    });
}
