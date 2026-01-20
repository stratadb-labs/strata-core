//! Stress Tests
//!
//! High-load and stress tests for the JSON primitive:
//! - Large documents
//! - Many documents
//! - High concurrency
//! - Deep nesting
//!
//! Many of these tests use #[ignore] for CI since they're slow.

use crate::test_utils::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

// =============================================================================
// Large Document Tests
// =============================================================================

/// Large document with many top-level keys.
#[test]
fn test_large_document_many_keys() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

    // Add 1000 keys
    for i in 0..1000 {
        let key = format!("key_{}", i);
        store
            .set(
                &run_id,
                &doc_id,
                &key.parse().unwrap(),
                JsonValue::from(i as i64),
            )
            .unwrap();
    }

    // Verify all keys
    for i in 0..1000 {
        let key = format!("key_{}", i);
        let value = store
            .get(&run_id, &doc_id, &key.parse().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(value.value.as_i64(), Some(i as i64));
    }

    // Version should be 1 (create) + 1000 (sets)
    assert_version(&store, &run_id, &doc_id, 1001);
}

/// Document with large string values.
#[test]
fn test_large_string_values() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

    // Create large strings
    let large_string: String = "x".repeat(10_000);

    for i in 0..10 {
        let key = format!("key_{}", i);
        store
            .set(
                &run_id,
                &doc_id,
                &key.parse().unwrap(),
                JsonValue::from(large_string.clone()),
            )
            .unwrap();
    }

    // Verify
    for i in 0..10 {
        let key = format!("key_{}", i);
        let value = store
            .get(&run_id, &doc_id, &key.parse().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(value.value.as_str().unwrap().len(), 10_000);
    }
}

/// Large array document.
#[test]
fn test_large_array() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::array());

    // Build large array
    let large_array: serde_json::Value = (0..1000).map(|i| serde_json::json!(i)).collect();
    store
        .set(&run_id, &doc_id, &root(), large_array.into())
        .unwrap();

    // Access various indices
    assert_eq!(
        store
            .get(&run_id, &doc_id, &path("[0]"))
            .unwrap()
            .unwrap().value.as_i64(),
        Some(0)
    );
    assert_eq!(
        store
            .get(&run_id, &doc_id, &path("[500]"))
            .unwrap()
            .unwrap().value.as_i64(),
        Some(500)
    );
    assert_eq!(
        store
            .get(&run_id, &doc_id, &path("[999]"))
            .unwrap()
            .unwrap().value.as_i64(),
        Some(999)
    );
}

// =============================================================================
// Deep Nesting Tests
// =============================================================================

/// Deeply nested object.
#[test]
fn test_deep_nesting() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

    // Build deeply nested structure
    let depth = 50;
    let mut path_str = String::new();
    for i in 0..depth {
        if i > 0 {
            path_str.push('.');
        }
        path_str.push_str(&format!("level{}", i));
    }

    // Set value at deep path
    store
        .set(
            &run_id,
            &doc_id,
            &path_str.parse().unwrap(),
            JsonValue::from("deep"),
        )
        .unwrap();

    // Read back
    let value = store
        .get(&run_id, &doc_id, &path_str.parse().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(value.value.as_str(), Some("deep"));
}

/// Deeply nested array access.
#[test]
fn test_deep_array_nesting() {
    // Build nested arrays: [[[[...]]]]
    let mut value = serde_json::json!(42);
    for _ in 0..20 {
        value = serde_json::json!([value]);
    }

    let (_, store, run_id, doc_id) = setup_doc(value.into());

    // Build path to innermost value
    let path_str = "[0]".repeat(20);
    let inner = store
        .get(&run_id, &doc_id, &path_str.parse().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(inner.value.as_i64(), Some(42));
}

// =============================================================================
// Many Documents Tests
// =============================================================================

/// Create and access many documents.
#[test]
fn test_many_documents() {
    let db = create_test_db();
    let store = JsonStore::new(db);
    let run_id = RunId::new();

    let doc_count = 100;
    let doc_ids: Vec<JsonDocId> = (0..doc_count).map(|_| JsonDocId::new()).collect();

    // Create all documents
    for (i, doc_id) in doc_ids.iter().enumerate() {
        store
            .create(&run_id, doc_id, JsonValue::from(i as i64))
            .unwrap();
    }

    // Verify all documents
    for (i, doc_id) in doc_ids.iter().enumerate() {
        assert!(store.exists(&run_id, doc_id).unwrap());
        assert_eq!(
            store
                .get(&run_id, doc_id, &root())
                .unwrap()
                .unwrap().value.as_i64(),
            Some(i as i64)
        );
    }
}

/// Many operations across many documents.
#[test]
fn test_many_documents_many_operations() {
    let db = create_test_db();
    let store = JsonStore::new(db);
    let run_id = RunId::new();

    let doc_count = 20;
    let ops_per_doc = 50;

    let doc_ids: Vec<JsonDocId> = (0..doc_count).map(|_| JsonDocId::new()).collect();

    // Create documents
    for doc_id in &doc_ids {
        store.create(&run_id, doc_id, JsonValue::object()).unwrap();
    }

    // Interleaved operations
    for op in 0..ops_per_doc {
        for (doc_idx, doc_id) in doc_ids.iter().enumerate() {
            let key = format!("key_{}", op);
            let value = doc_idx * 1000 + op;
            store
                .set(
                    &run_id,
                    doc_id,
                    &key.parse().unwrap(),
                    JsonValue::from(value as i64),
                )
                .unwrap();
        }
    }

    // Verify
    for (doc_idx, doc_id) in doc_ids.iter().enumerate() {
        for op in 0..ops_per_doc {
            let key = format!("key_{}", op);
            let expected = doc_idx * 1000 + op;
            let actual = store
                .get(&run_id, doc_id, &key.parse().unwrap())
                .unwrap()
                .unwrap();
            assert_eq!(actual.value.as_i64(), Some(expected as i64));
        }
    }
}

// =============================================================================
// High Concurrency Tests (use #[ignore] for CI)
// =============================================================================

/// Many concurrent readers.
#[test]
fn test_concurrent_readers_stress() {
    let db = create_test_db();
    let store = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // Setup document with data
    store
        .create(
            &run_id,
            &doc_id,
            serde_json::json!({
                "counter": 42,
                "data": "test"
            })
            .into(),
        )
        .unwrap();

    // Spawn many concurrent readers
    let results = run_concurrent_n(50, {
        let db = db.clone();
        let run_id = run_id.clone();
        let doc_id = doc_id.clone();
        move |_| {
            let store = JsonStore::new(db.clone());
            let mut read_count = 0;
            for _ in 0..100 {
                let _ = store.get(&run_id, &doc_id, &path("counter")).unwrap();
                let _ = store.get(&run_id, &doc_id, &path("data")).unwrap();
                read_count += 2;
            }
            read_count
        }
    });

    // All readers completed
    let total_reads: i32 = results.iter().sum();
    assert_eq!(total_reads, 50 * 200);
}

/// Concurrent writers to different documents.
#[test]
fn test_concurrent_writers_different_docs_stress() {
    let db = create_test_db();
    let run_id = RunId::new();

    // Create documents
    let doc_ids: Vec<JsonDocId> = (0..20).map(|_| JsonDocId::new()).collect();
    for doc_id in &doc_ids {
        let store = JsonStore::new(db.clone());
        store
            .create(&run_id, doc_id, JsonValue::from(0i64))
            .unwrap();
    }

    // Concurrent writes
    let write_count = Arc::new(AtomicU64::new(0));

    let handles: Vec<_> = (0..20)
        .map(|i| {
            let db = db.clone();
            let run_id = run_id.clone();
            let doc_id = doc_ids[i].clone();
            let count = write_count.clone();

            std::thread::spawn(move || {
                let store = JsonStore::new(db);
                for j in 0..50 {
                    store
                        .set(&run_id, &doc_id, &root(), JsonValue::from(j as i64))
                        .unwrap();
                    count.fetch_add(1, Ordering::Relaxed);
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    // All writes completed
    assert_eq!(write_count.load(Ordering::Relaxed), 20 * 50);

    // Verify each document has some value
    for doc_id in &doc_ids {
        let store = JsonStore::new(db.clone());
        let value = store.get(&run_id, doc_id, &root()).unwrap().unwrap();
        assert!(value.value.as_i64().is_some());
    }
}

/// Mixed read/write stress.
#[test]
fn test_mixed_read_write_stress() {
    let db = create_test_db();
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // Create document
    {
        let store = JsonStore::new(db.clone());
        store
            .create(&run_id, &doc_id, JsonValue::from(0i64))
            .unwrap();
    }

    let read_count = Arc::new(AtomicU64::new(0));
    let write_count = Arc::new(AtomicU64::new(0));

    let mut handles = Vec::new();

    // Spawn readers
    for _ in 0..10 {
        let db = db.clone();
        let run_id = run_id.clone();
        let doc_id = doc_id.clone();
        let count = read_count.clone();

        handles.push(std::thread::spawn(move || {
            let store = JsonStore::new(db);
            for _ in 0..100 {
                let _ = store.get(&run_id, &doc_id, &root());
                count.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    // Spawn writers
    for _ in 0..5 {
        let db = db.clone();
        let run_id = run_id.clone();
        let doc_id = doc_id.clone();
        let count = write_count.clone();

        handles.push(std::thread::spawn(move || {
            let store = JsonStore::new(db);
            for i in 0..20 {
                let _ = store.set(&run_id, &doc_id, &root(), JsonValue::from(i as i64));
                count.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    assert_eq!(read_count.load(Ordering::Relaxed), 10 * 100);
    assert_eq!(write_count.load(Ordering::Relaxed), 5 * 20);
}

// =============================================================================
// Throughput Tests (use #[ignore] for CI)
// =============================================================================

/// Measure write throughput.
#[test]
#[ignore]
fn test_write_throughput() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

    let iterations = 10_000;
    let start = Instant::now();

    for i in 0..iterations {
        let key = format!("key_{}", i % 100); // Reuse keys
        store
            .set(
                &run_id,
                &doc_id,
                &key.parse().unwrap(),
                JsonValue::from(i as i64),
            )
            .unwrap();
    }

    let elapsed = start.elapsed();
    let ops_per_sec = iterations as f64 / elapsed.as_secs_f64();

    println!(
        "Write throughput: {:.0} ops/sec ({} ops in {:?})",
        ops_per_sec, iterations, elapsed
    );
    assert!(ops_per_sec > 100.0, "Write throughput too low");
}

/// Measure read throughput.
#[test]
#[ignore]
fn test_read_throughput() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

    // Setup some data
    for i in 0..100 {
        let key = format!("key_{}", i);
        store
            .set(
                &run_id,
                &doc_id,
                &key.parse().unwrap(),
                JsonValue::from(i as i64),
            )
            .unwrap();
    }

    let iterations = 100_000;
    let start = Instant::now();

    for i in 0..iterations {
        let key = format!("key_{}", i % 100);
        let _ = store.get(&run_id, &doc_id, &key.parse().unwrap()).unwrap();
    }

    let elapsed = start.elapsed();
    let ops_per_sec = iterations as f64 / elapsed.as_secs_f64();

    println!(
        "Read throughput: {:.0} ops/sec ({} ops in {:?})",
        ops_per_sec, iterations, elapsed
    );
    assert!(ops_per_sec > 1000.0, "Read throughput too low");
}

// =============================================================================
// Memory Stress Tests (use #[ignore] for CI)
// =============================================================================

/// Create and destroy many documents to test cleanup.
#[test]
#[ignore]
fn test_create_destroy_cycle_stress() {
    let db = create_test_db();
    let store = JsonStore::new(db);
    let run_id = RunId::new();

    for cycle in 0..100 {
        let doc_ids: Vec<JsonDocId> = (0..100).map(|_| JsonDocId::new()).collect();

        // Create
        for doc_id in &doc_ids {
            store
                .create(&run_id, doc_id, JsonValue::from(cycle as i64))
                .unwrap();
        }

        // Destroy
        for doc_id in &doc_ids {
            store.destroy(&run_id, doc_id).unwrap();
        }
    }

    // Should complete without running out of memory
}

/// Large JSON tree stress test.
#[test]
#[ignore]
fn test_large_json_tree_stress() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

    // Generate random tree
    let large_tree = random_json_tree(5, 12345);
    store.set(&run_id, &doc_id, &root(), large_tree).unwrap();

    // Should be able to read back
    let result = store.get(&run_id, &doc_id, &root()).unwrap();
    assert!(result.is_some());
}
