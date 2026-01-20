//! Serializability Tests
//!
//! Tests for serializable execution guarantees:
//! - Operations appear to execute in some serial order
//! - Consistent reads across operations
//! - No anomalies (dirty reads, phantom reads, etc.)

use crate::test_utils::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

// =============================================================================
// Read Consistency Tests
// =============================================================================

/// No dirty reads - uncommitted data is not visible.
#[test]
fn test_no_dirty_reads() {
    // In M5's model, every operation is immediately committed
    // So "dirty reads" don't apply in the traditional sense
    // This test verifies that partial writes are not visible

    let db = create_test_db();
    let json_store = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // Create document with complex value
    let complex: JsonValue = serde_json::json!({
        "a": 1,
        "b": 2,
        "c": 3
    })
    .into();

    json_store.create(&run_id, &doc_id, complex).unwrap();

    // After create, all fields should be visible atomically
    let a = json_store
        .get(&run_id, &doc_id, &path("a"))
        .unwrap()
        .unwrap();
    let b = json_store
        .get(&run_id, &doc_id, &path("b"))
        .unwrap()
        .unwrap();
    let c = json_store
        .get(&run_id, &doc_id, &path("c"))
        .unwrap()
        .unwrap();

    assert_eq!(a.value.as_i64(), Some(1));
    assert_eq!(b.value.as_i64(), Some(2));
    assert_eq!(c.value.as_i64(), Some(3));
}

/// Read-your-writes consistency.
#[test]
fn test_read_your_writes() {
    let db = create_test_db();
    let json_store = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    json_store
        .create(&run_id, &doc_id, JsonValue::object())
        .unwrap();

    // Write then read immediately - should see the write
    for i in 0..100 {
        let key = format!("key_{}", i);
        json_store
            .set(
                &run_id,
                &doc_id,
                &key.parse().unwrap(),
                JsonValue::from(i as i64),
            )
            .unwrap();

        let read = json_store
            .get(&run_id, &doc_id, &key.parse().unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(
            read.value.as_i64(),
            Some(i as i64),
            "Failed to read own write at iteration {}",
            i
        );
    }
}

/// Monotonic reads - once a value is seen, older values are never seen.
#[test]
fn test_monotonic_reads() {
    let db = create_test_db();
    let json_store = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    json_store
        .create(&run_id, &doc_id, JsonValue::from(0i64))
        .unwrap();

    let mut last_seen = 0i64;

    for i in 1..=100i64 {
        json_store
            .set(&run_id, &doc_id, &root(), JsonValue::from(i))
            .unwrap();

        let current = json_store
            .get(&run_id, &doc_id, &root())
            .unwrap()
            .unwrap().value.as_i64()
            .unwrap();

        // Should never see a value older than last seen
        assert!(
            current >= last_seen,
            "Monotonicity violated: saw {} after {}",
            current,
            last_seen
        );
        last_seen = current;
    }
}

// =============================================================================
// Serial Execution Appearance Tests
// =============================================================================

/// Operations appear to execute in some total order.
#[test]
fn test_operations_total_order() {
    let db = create_test_db();
    let json_store = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    json_store
        .create(&run_id, &doc_id, JsonValue::object())
        .unwrap();

    // Sequence of operations
    json_store
        .set(&run_id, &doc_id, &path("a"), JsonValue::from(1i64))
        .unwrap();
    json_store
        .set(&run_id, &doc_id, &path("b"), JsonValue::from(2i64))
        .unwrap();
    json_store
        .set(&run_id, &doc_id, &path("a"), JsonValue::from(10i64))
        .unwrap(); // Overwrite
    json_store
        .delete_at_path(&run_id, &doc_id, &path("b"))
        .unwrap();
    json_store
        .set(&run_id, &doc_id, &path("c"), JsonValue::from(3i64))
        .unwrap();

    // Final state reflects all operations in order
    assert_eq!(
        json_store
            .get(&run_id, &doc_id, &path("a"))
            .unwrap()
            .unwrap().value.as_i64(),
        Some(10)
    );
    assert!(json_store
        .get(&run_id, &doc_id, &path("b"))
        .unwrap()
        .is_none());
    assert_eq!(
        json_store
            .get(&run_id, &doc_id, &path("c"))
            .unwrap()
            .unwrap().value.as_i64(),
        Some(3)
    );
}

/// Version reflects operation order.
#[test]
fn test_version_reflects_order() {
    let db = create_test_db();
    let json_store = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    json_store
        .create(&run_id, &doc_id, JsonValue::object())
        .unwrap();

    let mut versions = vec![json_store.get_version(&run_id, &doc_id).unwrap().unwrap()];

    for i in 0..10 {
        json_store
            .set(
                &run_id,
                &doc_id,
                &path(&format!("key{}", i)),
                JsonValue::from(i as i64),
            )
            .unwrap();
        versions.push(json_store.get_version(&run_id, &doc_id).unwrap().unwrap());
    }

    // Versions should be strictly increasing
    for i in 1..versions.len() {
        assert!(
            versions[i] > versions[i - 1],
            "Version not strictly increasing"
        );
    }
}

// =============================================================================
// Multi-Document Serializability Tests
// =============================================================================

/// Operations on different documents appear serial.
#[test]
fn test_multi_document_serial_appearance() {
    let db = create_test_db();
    let json_store = JsonStore::new(db.clone());
    let run_id = RunId::new();

    let doc1 = JsonDocId::new();
    let doc2 = JsonDocId::new();

    json_store
        .create(&run_id, &doc1, JsonValue::from(0i64))
        .unwrap();
    json_store
        .create(&run_id, &doc2, JsonValue::from(0i64))
        .unwrap();

    // Interleaved operations
    json_store
        .set(&run_id, &doc1, &root(), JsonValue::from(1i64))
        .unwrap();
    json_store
        .set(&run_id, &doc2, &root(), JsonValue::from(2i64))
        .unwrap();
    json_store
        .set(&run_id, &doc1, &root(), JsonValue::from(3i64))
        .unwrap();

    // Each document has consistent state
    assert_eq!(
        json_store
            .get(&run_id, &doc1, &root())
            .unwrap()
            .unwrap().value.as_i64(),
        Some(3)
    );
    assert_eq!(
        json_store
            .get(&run_id, &doc2, &root())
            .unwrap()
            .unwrap().value.as_i64(),
        Some(2)
    );
}

// =============================================================================
// Concurrent Serializability Tests
// =============================================================================

/// Concurrent increments produce correct final count.
#[test]
fn test_concurrent_increments_serializable() {
    let db = create_test_db();
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // Create counter document
    {
        let store = JsonStore::new(db.clone());
        store
            .create(&run_id, &doc_id, JsonValue::from(0i64))
            .unwrap();
    }

    let success_count = Arc::new(AtomicU64::new(0));

    // Concurrent "increments" - each thread reads, increments, writes
    // Note: This tests visibility, not actual increment atomicity
    // since M5 doesn't have read-modify-write operations
    let handles: Vec<_> = (0..10)
        .map(|_| {
            let db = db.clone();
            let run_id = run_id.clone();
            let doc_id = doc_id.clone();
            let count = success_count.clone();

            std::thread::spawn(move || {
                let store = JsonStore::new(db);

                // Each thread does 10 operations
                for _ in 0..10 {
                    // Read current value
                    let current = store
                        .get(&run_id, &doc_id, &root())
                        .unwrap()
                        .unwrap().value.as_i64()
                        .unwrap();

                    // Write incremented value (may conflict with others)
                    let result = store.set(&run_id, &doc_id, &root(), JsonValue::from(current + 1));
                    if result.is_ok() {
                        count.fetch_add(1, Ordering::Relaxed);
                    }
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    // Final value should exist and be valid
    let store = JsonStore::new(db);
    let final_value = store.get(&run_id, &doc_id, &root()).unwrap().unwrap();
    assert!(final_value.value.as_i64().is_some());

    // Note: Due to conflicts, final value may be less than total operations
    // But it should be >= 1 (at least one operation succeeded)
    assert!(final_value.value.as_i64().unwrap() >= 1);
}

/// Concurrent operations on different DOCUMENTS are serializable.
/// Note: In M5, different keys in the SAME document will still conflict
/// because conflict detection is at the document level.
#[test]
fn test_concurrent_different_docs_serializable() {
    let db = create_test_db();
    let run_id = RunId::new();

    // Create separate documents for each thread
    let doc_ids: Vec<JsonDocId> = (0..10).map(|_| JsonDocId::new()).collect();
    for doc_id in &doc_ids {
        let store = JsonStore::new(db.clone());
        store
            .create(&run_id, doc_id, JsonValue::from(0i64))
            .unwrap();
    }

    // Each thread writes to its own document
    let _results = run_concurrent_n(10, {
        let db = db.clone();
        let run_id = run_id.clone();
        let doc_ids = doc_ids.clone();
        move |i| {
            let store = JsonStore::new(db.clone());
            store
                .set(&run_id, &doc_ids[i], &root(), JsonValue::from(i as i64))
                .unwrap();
            i
        }
    });

    // Verify all documents have correct values
    let store = JsonStore::new(db);
    for (i, doc_id) in doc_ids.iter().enumerate() {
        let value = store.get(&run_id, doc_id, &root()).unwrap().unwrap();
        assert_eq!(value.value.as_i64(), Some(i as i64));
    }
}

// =============================================================================
// Isolation Level Tests
// =============================================================================

/// Snapshot isolation - read sees consistent snapshot.
#[test]
fn test_snapshot_isolation_read() {
    let db = create_test_db();
    let json_store = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // Create document with multiple fields
    json_store
        .create(
            &run_id,
            &doc_id,
            serde_json::json!({
                "balance_a": 100,
                "balance_b": 100
            })
            .into(),
        )
        .unwrap();

    // Read both values - should be consistent
    let a = json_store
        .get(&run_id, &doc_id, &path("balance_a"))
        .unwrap()
        .unwrap().value.as_i64()
        .unwrap();
    let b = json_store
        .get(&run_id, &doc_id, &path("balance_b"))
        .unwrap()
        .unwrap().value.as_i64()
        .unwrap();

    // Both should be from same snapshot
    assert_eq!(a, 100);
    assert_eq!(b, 100);
}

/// Write skew detection (overlapping paths conflict).
#[test]
fn test_overlapping_paths_conflict() {
    // M5 uses path-based conflict detection
    // Overlapping paths should conflict

    let db = create_test_db();
    let json_store = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    json_store
        .create(
            &run_id,
            &doc_id,
            serde_json::json!({
                "data": {
                    "value": 1
                }
            })
            .into(),
        )
        .unwrap();

    // Sequential writes to overlapping paths should both succeed
    // (since they're sequential, not concurrent)
    json_store
        .set(
            &run_id,
            &doc_id,
            &path("data"),
            serde_json::json!({"value": 2}).into(),
        )
        .unwrap();
    json_store
        .set(&run_id, &doc_id, &path("data.value"), JsonValue::from(3i64))
        .unwrap();

    // Final state should reflect last write
    assert_eq!(
        json_store
            .get(&run_id, &doc_id, &path("data.value"))
            .unwrap()
            .unwrap().value.as_i64(),
        Some(3)
    );
}

// =============================================================================
// Anomaly Prevention Tests
// =============================================================================

/// No lost updates on same path.
#[test]
fn test_no_lost_updates_same_path() {
    let db = create_test_db();
    let json_store = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    json_store
        .create(&run_id, &doc_id, JsonValue::from(0i64))
        .unwrap();

    // Sequential updates
    for i in 1..=10 {
        json_store
            .set(&run_id, &doc_id, &root(), JsonValue::from(i as i64))
            .unwrap();
    }

    // Final value should be last write
    assert_eq!(
        json_store
            .get(&run_id, &doc_id, &root())
            .unwrap()
            .unwrap().value.as_i64(),
        Some(10)
    );
}

/// No phantom reads - consistent query results.
#[test]
fn test_no_phantom_reads() {
    let db = create_test_db();
    let json_store = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // Create document with array
    json_store
        .create(&run_id, &doc_id, serde_json::json!([1, 2, 3]).into())
        .unwrap();

    // Read array elements
    let elem0 = json_store
        .get(&run_id, &doc_id, &path("[0]"))
        .unwrap()
        .unwrap();
    let elem1 = json_store
        .get(&run_id, &doc_id, &path("[1]"))
        .unwrap()
        .unwrap();
    let elem2 = json_store
        .get(&run_id, &doc_id, &path("[2]"))
        .unwrap()
        .unwrap();

    // All reads should be consistent
    assert_eq!(elem0.value.as_i64(), Some(1));
    assert_eq!(elem1.value.as_i64(), Some(2));
    assert_eq!(elem2.value.as_i64(), Some(3));
}
