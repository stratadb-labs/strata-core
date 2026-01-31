//! Scale Integration Tests
//!
//! Tests behavior at different data sizes:
//! - 1,000 records (small)
//! - 10,000 records (medium)
//! - 100,000 records (large)
//!
//! Note: 1M and 10M record tests are too slow for regular CI
//! and should be run manually or in nightly builds.

use crate::common::*;
use std::time::Instant;

// ============================================================================
// Scale Helpers
// ============================================================================

const SCALE_SMALL: usize = 1_000;
const SCALE_MEDIUM: usize = 10_000;
const SCALE_LARGE: usize = 100_000;

fn measure<F: FnOnce() -> T, T>(name: &str, f: F) -> T {
    let start = Instant::now();
    let result = f();
    let elapsed = start.elapsed();
    eprintln!("  {} completed in {:?}", name, elapsed);
    result
}

// ============================================================================
// KV Scale Tests
// ============================================================================

mod kv_scale {
    use super::*;

    fn run_kv_scale_test(count: usize) {
        let test_db = TestDb::new();
        let branch_id = test_db.branch_id;
        let kv = test_db.kv();

        // Write
        measure(&format!("Write {} KV entries", count), || {
            for i in 0..count {
                kv.put(&branch_id, &format!("key_{:08}", i), Value::Int(i as i64)).unwrap();
            }
        });

        // Read all
        measure(&format!("Read {} KV entries", count), || {
            for i in 0..count {
                let val = kv.get(&branch_id, &format!("key_{:08}", i)).unwrap();
                assert_eq!(val.unwrap(), Value::Int(i as i64));
            }
        });

        // List/scan
        measure(&format!("List {} KV keys", count), || {
            let keys = kv.list(&branch_id, Some("key_")).unwrap();
            assert_eq!(keys.len(), count);
        });

        // Random reads
        measure(&format!("Random read 1000 from {}", count), || {
            for i in (0..count).step_by(count / 1000) {
                let _ = kv.get(&branch_id, &format!("key_{:08}", i)).unwrap();
            }
        });
    }

    #[test]
    fn kv_scale_1k() {
        run_kv_scale_test(SCALE_SMALL);
    }

    #[test]
    fn kv_scale_10k() {
        run_kv_scale_test(SCALE_MEDIUM);
    }

    #[test]
    #[ignore = "slow test, run manually"]
    fn kv_scale_100k() {
        run_kv_scale_test(SCALE_LARGE);
    }
}

// ============================================================================
// Event Scale Tests
// ============================================================================

mod event_scale {
    use super::*;

    fn run_event_scale_test(count: usize) {
        let test_db = TestDb::new();
        let branch_id = test_db.branch_id;
        let event = test_db.event();

        // Append
        measure(&format!("Append {} events", count), || {
            for i in 0..count {
                event.append(&branch_id, "scale_test", int_payload(i as i64)).unwrap();
            }
        });

        // Count
        measure(&format!("Count {} events", count), || {
            let len = event.read_by_type(&branch_id, "scale_test").unwrap().len() as u64;
            assert_eq!(len, count as u64);
        });

        // Read all
        measure(&format!("Read {} events", count), || {
            let events = event.read_by_type(&branch_id, "scale_test").unwrap();
            assert_eq!(events.len(), count);
        });
    }

    #[test]
    fn event_scale_1k() {
        run_event_scale_test(SCALE_SMALL);
    }

    #[test]
    fn event_scale_10k() {
        run_event_scale_test(SCALE_MEDIUM);
    }

    #[test]
    #[ignore = "slow test, run manually"]
    fn event_scale_100k() {
        run_event_scale_test(SCALE_LARGE);
    }
}

// ============================================================================
// JSON Scale Tests
// ============================================================================

mod json_scale {
    use super::*;

    fn run_json_scale_test(count: usize) {
        let test_db = TestDb::new();
        let branch_id = test_db.branch_id;
        let json = test_db.json();

        // Create documents
        measure(&format!("Create {} JSON documents", count), || {
            for i in 0..count {
                json.create(&branch_id, &format!("doc_{:08}", i), test_json_value(i)).unwrap();
            }
        });

        // Read all
        measure(&format!("Read {} JSON documents", count), || {
            for i in 0..count {
                let doc = json.get(&branch_id, &format!("doc_{:08}", i), &root()).unwrap();
                assert!(doc.is_some());
            }
        });

        // List
        measure(&format!("List {} JSON documents", count), || {
            let list = json.list(&branch_id, None, None, count + 1).unwrap();
            assert_eq!(list.doc_ids.len(), count);
        });
    }

    #[test]
    fn json_scale_1k() {
        run_json_scale_test(SCALE_SMALL);
    }

    #[test]
    fn json_scale_10k() {
        run_json_scale_test(SCALE_MEDIUM);
    }

    #[test]
    #[ignore = "slow test, run manually"]
    fn json_scale_100k() {
        run_json_scale_test(SCALE_LARGE);
    }
}

// ============================================================================
// Vector Scale Tests
// ============================================================================

mod vector_scale {
    use super::*;

    fn run_vector_scale_test(count: usize) {
        let test_db = TestDb::new();
        let branch_id = test_db.branch_id;
        let vector = test_db.vector();

        vector.create_collection(branch_id, "scale_test", config_small()).unwrap();

        // Insert vectors
        measure(&format!("Insert {} vectors", count), || {
            for i in 0..count {
                let emb = seeded_vector(3, i as u64);
                vector.insert(branch_id, "scale_test", &format!("vec_{:08}", i), &emb, None).unwrap();
            }
        });

        // Count
        let actual_count = vector.list_collections(branch_id).unwrap().iter()
            .find(|c| c.name == "scale_test").unwrap().count;
        assert_eq!(actual_count, count);

        // Search
        measure(&format!("100 searches over {} vectors", count), || {
            for i in 0..100 {
                let query = seeded_vector(3, i);
                let results = vector.search(branch_id, "scale_test", &query, 10, None).unwrap();
                assert_eq!(results.len(), 10);
            }
        });
    }

    #[test]
    fn vector_scale_1k() {
        run_vector_scale_test(SCALE_SMALL);
    }

    #[test]
    fn vector_scale_10k() {
        run_vector_scale_test(SCALE_MEDIUM);
    }

    #[test]
    #[ignore = "slow test, run manually"]
    fn vector_scale_100k() {
        run_vector_scale_test(SCALE_LARGE);
    }
}

// ============================================================================
// Cross-Primitive Scale Tests
// ============================================================================

#[test]
fn cross_primitive_scale_1k() {
    let count = SCALE_SMALL;
    let test_db = TestDb::new();
    let branch_id = test_db.branch_id;
    let p = test_db.all_primitives();

    p.vector.create_collection(branch_id, "embeddings", config_small()).unwrap();

    measure(&format!("Cross-primitive {} records", count), || {
        for i in 0..count {
            // KV: config
            p.kv.put(&branch_id, &format!("item:{}", i), Value::Int(i as i64)).unwrap();

            // Event: audit
            p.event.append(&branch_id, "items", int_payload(i as i64)).unwrap();

            // Vector: embedding
            p.vector.insert(
                branch_id,
                "embeddings",
                &format!("item_{}", i),
                &seeded_vector(3, i as u64),
                None,
            ).unwrap();
        }
    });

    // Verify counts
    assert_eq!(p.kv.list(&branch_id, Some("item:")).unwrap().len(), count);
    assert_eq!(p.event.read_by_type(&branch_id, "items").unwrap().len() as u64, count as u64);
    assert_eq!(p.vector.list_collections(branch_id).unwrap().iter()
        .find(|c| c.name == "embeddings").unwrap().count, count);
}

// ============================================================================
// Large Value Tests
// ============================================================================

#[test]
fn large_kv_values() {
    let test_db = TestDb::new();
    let branch_id = test_db.branch_id;
    let kv = test_db.kv();

    // 1KB value
    let small = vec![0xABu8; 1024];
    kv.put(&branch_id, "1kb", Value::Bytes(small.clone())).unwrap();
    let val = kv.get(&branch_id, "1kb").unwrap().unwrap();
    if let Value::Bytes(b) = val {
        assert_eq!(b.len(), 1024);
    }

    // 1MB value
    let large = vec![0xCDu8; 1024 * 1024];
    kv.put(&branch_id, "1mb", Value::Bytes(large.clone())).unwrap();
    let val = kv.get(&branch_id, "1mb").unwrap().unwrap();
    if let Value::Bytes(b) = val {
        assert_eq!(b.len(), 1024 * 1024);
    }
}

#[test]
fn deep_json_nesting() {
    let test_db = TestDb::new();
    let branch_id = test_db.branch_id;
    let json = test_db.json();

    // 50 levels deep
    let deep = deep_json_value(50);
    json.create(&branch_id, "deep", deep).unwrap();

    let doc = json.get(&branch_id, "deep", &root()).unwrap().unwrap();

    // Navigate down
    let mut current: serde_json::Value = doc.as_inner().clone();
    let mut depth = 0;
    while let Some(child) = current.get("child") {
        current = child.clone();
        depth += 1;
    }
    assert_eq!(depth, 50);
}

#[test]
fn high_dimension_vectors() {
    let test_db = TestDb::new();
    let branch_id = test_db.branch_id;
    let vector = test_db.vector();

    // 1536 dimensions (OpenAI ada-002 size)
    let config = VectorConfig {
        dimension: 1536,
        metric: DistanceMetric::Cosine,
        storage_dtype: StorageDtype::F32,
    };

    vector.create_collection(branch_id, "high_dim", config).unwrap();

    // Insert 100 high-dimensional vectors
    for i in 0..100 {
        let emb = seeded_vector(1536, i);
        vector.insert(branch_id, "high_dim", &format!("vec_{}", i), &emb, None).unwrap();
    }

    // Search should work
    let query = seeded_vector(1536, 0);
    let results = vector.search(branch_id, "high_dim", &query, 5, None).unwrap();
    assert_eq!(results.len(), 5);
    assert_eq!(results[0].key, "vec_0"); // Should find exact match
}

// ============================================================================
// Concurrent Scale Tests
// ============================================================================

#[test]
fn concurrent_writers_scale() {
    use std::sync::{Arc, Barrier};
    use std::thread;

    let test_db = TestDb::new();
    let db = test_db.db.clone();
    let branch_id = test_db.branch_id;

    let num_threads = 4;
    let writes_per_thread = 1000;
    let barrier = Arc::new(Barrier::new(num_threads));

    let handles: Vec<_> = (0..num_threads)
        .map(|t| {
            let db = db.clone();
            let barrier = barrier.clone();
            thread::spawn(move || {
                let kv = KVStore::new(db);
                barrier.wait();
                for i in 0..writes_per_thread {
                    let key = format!("t{}_{}", t, i);
                    kv.put(&branch_id, &key, Value::Int((t * 1000 + i) as i64)).unwrap();
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    let kv = KVStore::new(test_db.db.clone());
    let all_keys = kv.list(&branch_id, Some("t")).unwrap();
    assert_eq!(all_keys.len(), num_threads * writes_per_thread);
}
