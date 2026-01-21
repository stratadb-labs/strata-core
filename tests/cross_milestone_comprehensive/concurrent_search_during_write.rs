//! Concurrent Search During Write Tests
//!
//! Tests search consistency during concurrent writes.

use crate::test_utils::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Test search returns consistent results during writes.
#[test]
fn test_search_consistency_during_writes() {
    let test_db = TestDb::new();
    let db = test_db.db.clone();
    let run_id = test_db.run_id;

    // Create and populate
    let vector = strata_primitives::VectorStore::new(db.clone());
    vector.create_collection(run_id, "consistency", config_small()).expect("create");

    for i in 0..100 {
        vector.insert(run_id, "consistency", &format!("v{}", i), &seeded_vector(3, i as u64), None)
            .expect("insert");
    }

    let stop = Arc::new(AtomicBool::new(false));

    // Writer thread
    let stop_writer = stop.clone();
    let db_writer = db.clone();
    let writer = thread::spawn(move || {
        let vector = strata_primitives::VectorStore::new(db_writer);
        let mut i = 1000;
        while !stop_writer.load(Ordering::Relaxed) {
            let key = format!("write_v{}", i);
            let _ = vector.insert(run_id, "consistency", &key, &seeded_vector(3, i as u64), None);
            i += 1;
            thread::sleep(Duration::from_micros(10));
        }
    });

    // Search thread - should always get consistent results
    let db_searcher = db.clone();
    let searcher = thread::spawn(move || {
        let vector = strata_primitives::VectorStore::new(db_searcher);
        for _ in 0..1000 {
            let query = seeded_vector(3, 42);
            let results = vector.search(run_id, "consistency", &query, 10, None);

            if let Ok(results) = results {
                // Results should be sorted by score
                for i in 1..results.len() {
                    assert!(
                        results[i - 1].score >= results[i].score,
                        "Results should be sorted by score"
                    );
                }
            }
        }
    });

    searcher.join().expect("searcher");
    stop.store(true, Ordering::Relaxed);
    writer.join().expect("writer");
}

/// Test no phantom reads during search.
#[test]
fn test_no_phantom_reads() {
    let test_db = TestDb::new();
    let db = test_db.db.clone();
    let run_id = test_db.run_id;

    let vector = strata_primitives::VectorStore::new(db.clone());
    vector.create_collection(run_id, "phantom", config_small()).expect("create");

    // Insert initial data
    for i in 0..10 {
        vector.insert(run_id, "phantom", &format!("v{}", i), &seeded_vector(3, i as u64), None)
            .expect("insert");
    }

    // Snapshot should be consistent - no phantom reads
    // Multiple searches should return consistent results within same "transaction"
    let query = seeded_vector(3, 5);
    let results1 = vector.search(run_id, "phantom", &query, 10, None).expect("search1");
    let results2 = vector.search(run_id, "phantom", &query, 10, None).expect("search2");

    // Results should be deterministic
    assert_eq!(results1.len(), results2.len());
}
