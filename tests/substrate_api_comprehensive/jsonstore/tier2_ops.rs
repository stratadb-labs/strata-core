//! JsonStore Tier 2 Operations Tests
//!
//! Tests for M11B Tier 2 features:
//! - json_count: Document count
//! - json_batch_get: Batch document retrieval
//! - json_batch_create: Atomic batch document creation

use crate::*;

// =============================================================================
// Count Tests
// =============================================================================

/// Test count on empty run
#[test]
fn test_json_count_empty_run() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();

        let count = db.json_count(&run).unwrap();
        assert_eq!(count, 0, "Empty run should have 0 documents");
    });
}

/// Test count increases with creates
#[test]
fn test_json_count_increases_with_creates() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();

        assert_eq!(db.json_count(&run).unwrap(), 0);

        db.json_set(&run, "doc1", "$", Value::Int(1)).unwrap();
        assert_eq!(db.json_count(&run).unwrap(), 1);

        db.json_set(&run, "doc2", "$", Value::Int(2)).unwrap();
        assert_eq!(db.json_count(&run).unwrap(), 2);

        db.json_set(&run, "doc3", "$", Value::Int(3)).unwrap();
        assert_eq!(db.json_count(&run).unwrap(), 3);
    });
}

/// Test count after creates and deletes
#[test]
fn test_json_count_after_creates_and_deletes() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();

        // Create 5 documents
        for i in 0..5 {
            let key = format!("doc{}", i);
            db.json_set(&run, &key, "$", Value::Int(i)).unwrap();
        }
        assert_eq!(db.json_count(&run).unwrap(), 5);

        // Delete 2 documents
        db.json_delete(&run, "doc1", "$").unwrap();
        db.json_delete(&run, "doc3", "$").unwrap();
        assert_eq!(db.json_count(&run).unwrap(), 3);

        // Add one more
        db.json_set(&run, "doc5", "$", Value::Int(5)).unwrap();
        assert_eq!(db.json_count(&run).unwrap(), 4);
    });
}

/// Test count with run isolation
#[test]
fn test_json_count_run_isolation() {
    test_across_substrate_modes(|db| {
        let run1 = ApiRunId::default_run_id();
        let run2 = ApiRunId::new();

        // Create documents in run1
        db.json_set(&run1, "doc1", "$", Value::Int(1)).unwrap();
        db.json_set(&run1, "doc2", "$", Value::Int(2)).unwrap();

        // Create document in run2
        db.json_set(&run2, "doc3", "$", Value::Int(3)).unwrap();

        assert_eq!(db.json_count(&run1).unwrap(), 2, "Run1 should have 2 docs");
        assert_eq!(db.json_count(&run2).unwrap(), 1, "Run2 should have 1 doc");
    });
}

// =============================================================================
// Batch Get Tests
// =============================================================================

/// Test batch get returns documents
#[test]
fn test_json_batch_get_returns_documents() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();

        // Create documents
        db.json_set(&run, "doc1", "$", Value::Int(1)).unwrap();
        db.json_set(&run, "doc2", "$", Value::Int(2)).unwrap();
        db.json_set(&run, "doc3", "$", Value::Int(3)).unwrap();

        // Batch get
        let results = db.json_batch_get(&run, &["doc1", "doc2", "doc3"]).unwrap();
        assert_eq!(results.len(), 3);

        assert!(results[0].is_some());
        assert!(results[1].is_some());
        assert!(results[2].is_some());
    });
}

/// Test batch get returns None for missing
#[test]
fn test_json_batch_get_returns_none_for_missing() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();

        // Create only one document
        db.json_set(&run, "exists", "$", Value::Int(1)).unwrap();

        // Batch get with mix of existing and missing
        let results = db.json_batch_get(&run, &["exists", "missing1", "missing2"]).unwrap();
        assert_eq!(results.len(), 3);

        assert!(results[0].is_some(), "First should exist");
        assert!(results[1].is_none(), "Second should be missing");
        assert!(results[2].is_none(), "Third should be missing");
    });
}

/// Test batch get preserves order
#[test]
fn test_json_batch_get_preserves_order() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();

        // Create documents
        db.json_set(&run, "a", "$", Value::String("alpha".into())).unwrap();
        db.json_set(&run, "b", "$", Value::String("beta".into())).unwrap();
        db.json_set(&run, "c", "$", Value::String("gamma".into())).unwrap();

        // Batch get in different order
        let results = db.json_batch_get(&run, &["c", "a", "b"]).unwrap();
        assert_eq!(results.len(), 3);

        assert_eq!(results[0].as_ref().unwrap().value, Value::String("gamma".into()));
        assert_eq!(results[1].as_ref().unwrap().value, Value::String("alpha".into()));
        assert_eq!(results[2].as_ref().unwrap().value, Value::String("beta".into()));
    });
}

/// Test batch get with empty keys
#[test]
fn test_json_batch_get_empty_keys() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();

        let results = db.json_batch_get(&run, &[]).unwrap();
        assert!(results.is_empty(), "Empty input should return empty output");
    });
}

/// Test batch get with duplicate keys
#[test]
fn test_json_batch_get_duplicate_keys() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();

        db.json_set(&run, "doc", "$", Value::Int(42)).unwrap();

        let results = db.json_batch_get(&run, &["doc", "doc", "doc"]).unwrap();
        assert_eq!(results.len(), 3);

        // All should be the same document
        for result in &results {
            assert!(result.is_some());
            assert_eq!(result.as_ref().unwrap().value, Value::Int(42));
        }
    });
}

// =============================================================================
// Batch Create Tests
// =============================================================================

/// Test batch create creates documents
#[test]
fn test_json_batch_create_creates_documents() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();

        let docs = vec![
            ("doc1", Value::Int(1)),
            ("doc2", Value::Int(2)),
            ("doc3", Value::Int(3)),
        ];

        let versions = db.json_batch_create(&run, docs).unwrap();
        assert_eq!(versions.len(), 3);

        // Verify all documents exist
        assert!(db.json_exists(&run, "doc1").unwrap());
        assert!(db.json_exists(&run, "doc2").unwrap());
        assert!(db.json_exists(&run, "doc3").unwrap());

        // Verify values
        assert_eq!(db.json_get(&run, "doc1", "$").unwrap().unwrap().value, Value::Int(1));
        assert_eq!(db.json_get(&run, "doc2", "$").unwrap().unwrap().value, Value::Int(2));
        assert_eq!(db.json_get(&run, "doc3", "$").unwrap().unwrap().value, Value::Int(3));
    });
}

/// Test batch create is atomic - fails if any exists
#[test]
fn test_json_batch_create_fails_if_any_exists() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();

        // Pre-create one document
        db.json_set(&run, "existing", "$", Value::Int(0)).unwrap();

        // Try to batch create with one existing
        let docs = vec![
            ("new1", Value::Int(1)),
            ("existing", Value::Int(2)), // This one exists
            ("new2", Value::Int(3)),
        ];

        let result = db.json_batch_create(&run, docs);
        assert!(result.is_err(), "Should fail because 'existing' already exists");

        // Verify none of the new documents were created (atomicity)
        assert!(!db.json_exists(&run, "new1").unwrap(), "new1 should not exist");
        assert!(!db.json_exists(&run, "new2").unwrap(), "new2 should not exist");
    });
}

/// Test batch create is atomic - all or nothing
#[test]
fn test_json_batch_create_is_atomic() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();

        // Create documents with batch
        let docs = vec![
            ("atomic1", Value::String("first".into())),
            ("atomic2", Value::String("second".into())),
        ];

        db.json_batch_create(&run, docs).unwrap();

        // Both should exist
        assert!(db.json_exists(&run, "atomic1").unwrap());
        assert!(db.json_exists(&run, "atomic2").unwrap());
    });
}

/// Test batch create with empty input
#[test]
fn test_json_batch_create_empty() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();

        let versions = db.json_batch_create(&run, vec![]).unwrap();
        assert!(versions.is_empty(), "Empty input should return empty versions");
    });
}

/// Test batch create with complex values
#[test]
fn test_json_batch_create_complex_values() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();

        let docs = vec![
            ("user1", obj([
                ("name", Value::String("Alice".into())),
                ("age", Value::Int(30)),
            ])),
            ("user2", obj([
                ("name", Value::String("Bob".into())),
                ("age", Value::Int(25)),
            ])),
        ];

        db.json_batch_create(&run, docs).unwrap();

        // Verify nested data
        let alice_name = db.json_get(&run, "user1", "name").unwrap().unwrap().value;
        assert_eq!(alice_name, Value::String("Alice".into()));

        let bob_age = db.json_get(&run, "user2", "age").unwrap().unwrap().value;
        assert_eq!(bob_age, Value::Int(25));
    });
}
