//! Tier 10: Stress & Scale Testing
//!
//! These tests are marked #[ignore] and run manually with --ignored flag.

use crate::common::*;
use strata_core::search_types::{SearchBudget, SearchRequest};
use strata_core::value::Value;
use strata_engine::{KVStore, RunIndex};
use strata_intelligence::DatabaseSearchExt;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

// ============================================================================
// Large Dataset Tests
// ============================================================================

/// Search works with large dataset
#[test]
#[ignore]
fn test_tier10_large_dataset() {
    let db = create_test_db();
    let run_id = test_run_id();

    populate_large_dataset(&db, &run_id, 10_000);

    let kv = KVStore::new(db.clone());
    let req = SearchRequest::new(run_id, "searchable").with_k(100);

    let start = Instant::now();
    let response = kv.search(&req).unwrap();
    let elapsed = start.elapsed();

    assert!(!response.hits.is_empty());
    assert!(
        elapsed < Duration::from_secs(5),
        "Search should complete in under 5s"
    );
}

/// Hybrid search works with large dataset
///
/// Note: HybridSearch divides the search budget among all 7 primitives,
/// so we need to provide a budget large enough that KV gets sufficient time.
/// Default budget is 100ms; with 7 primitives, each gets ~14ms.
/// For 10,000 records, we need at least 700ms total (7 primitives × 100ms each).
#[test]
#[ignore]
fn test_tier10_hybrid_large_dataset() {
    let db = create_test_db();
    let run_id = test_run_id();

    populate_large_dataset(&db, &run_id, 10_000);

    let hybrid = db.hybrid();
    // Provide sufficient budget for hybrid search across 7 primitives
    // 1 second total = ~143ms per primitive, enough for 10k records
    let budget = SearchBudget::default().with_time(1_000_000); // 1 second
    let req = SearchRequest::new(run_id, "searchable")
        .with_k(100)
        .with_budget(budget);

    let start = Instant::now();
    let response = hybrid.search(&req).unwrap();
    let elapsed = start.elapsed();

    assert!(!response.hits.is_empty());
    assert!(
        elapsed < Duration::from_secs(10),
        "Hybrid search should complete in under 10s"
    );
}

// ============================================================================
// Concurrent Search Tests
// ============================================================================

/// Concurrent searches don't interfere
#[test]
#[ignore]
fn test_tier10_concurrent_searches() {
    let db = create_test_db();
    let run_id = test_run_id();

    populate_large_dataset(&db, &run_id, 1000);

    let handles: Vec<_> = (0..10)
        .map(|_| {
            let db = Arc::clone(&db);

            thread::spawn(move || {
                let hybrid = db.hybrid();
                let req = SearchRequest::new(run_id, "searchable").with_k(50);

                for _ in 0..100 {
                    let response = hybrid.search(&req).unwrap();
                    assert!(!response.hits.is_empty());
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("Thread should complete");
    }
}

/// Concurrent reads and writes
#[test]
#[ignore]
fn test_tier10_concurrent_read_write() {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let db = create_test_db();
    let run_id = test_run_id();

    let kv = KVStore::new(db.clone());
    let run_index = RunIndex::new(db.clone());
    run_index.create_run(&run_id.to_string()).unwrap();

    // Add some initial data
    for i in 0..100 {
        kv.put(
            &run_id,
            &format!("initial_{}", i),
            Value::String("searchable content".into()),
        )
        .unwrap();
    }

    let stop = Arc::new(AtomicBool::new(false));

    // Writer thread
    let writer_db = Arc::clone(&db);
    let writer_run_id = run_id;
    let writer_stop = Arc::clone(&stop);
    let writer = thread::spawn(move || {
        let kv = KVStore::new(writer_db);
        let mut i = 0;
        while !writer_stop.load(Ordering::Relaxed) {
            kv.put(
                &writer_run_id,
                &format!("new_{}", i),
                Value::String("new searchable content".into()),
            )
            .unwrap();
            i += 1;
            thread::sleep(Duration::from_micros(100));
        }
    });

    // Reader threads
    let readers: Vec<_> = (0..5)
        .map(|_| {
            let db = Arc::clone(&db);
            let stop = Arc::clone(&stop);

            thread::spawn(move || {
                let hybrid = db.hybrid();
                let req = SearchRequest::new(run_id, "searchable").with_k(50);

                while !stop.load(Ordering::Relaxed) {
                    let response = hybrid.search(&req).unwrap();
                    // Should always get valid results
                    verify_scores_decreasing(&response);
                    verify_ranks_sequential(&response);
                }
            })
        })
        .collect();

    // Run for 1 second
    thread::sleep(Duration::from_secs(1));
    stop.store(true, Ordering::Relaxed);

    writer.join().expect("Writer should complete");
    for reader in readers {
        reader.join().expect("Reader should complete");
    }
}

// ============================================================================
// Multiple Run Tests
// ============================================================================

/// Search works with many runs
#[test]
#[ignore]
fn test_tier10_many_runs() {
    let db = create_test_db();

    let kv = KVStore::new(db.clone());
    let run_index = RunIndex::new(db.clone());

    // Create 100 runs with data
    let mut run_ids = Vec::new();
    for i in 0..100 {
        let run_id = test_run_id();
        run_index.create_run(&run_id.to_string()).unwrap();

        for j in 0..10 {
            kv.put(
                &run_id,
                &format!("key_{}_{}", i, j),
                Value::String(format!("searchable content {}", i)),
            )
            .unwrap();
        }

        run_ids.push(run_id);
    }

    // Search each run
    for run_id in &run_ids {
        let req = SearchRequest::new(*run_id, "searchable").with_k(20);
        let response = kv.search(&req).unwrap();

        assert!(!response.hits.is_empty());
        assert_all_from_run(&response, *run_id);
    }
}

// ============================================================================
// Memory Stability Tests
// ============================================================================

/// Repeated searches don't leak memory
#[test]
#[ignore]
fn test_tier10_no_memory_leak() {
    let db = create_test_db();
    let run_id = test_run_id();

    populate_large_dataset(&db, &run_id, 1000);

    let hybrid = db.hybrid();
    let req = SearchRequest::new(run_id, "searchable").with_k(100);

    // Run many iterations
    for _ in 0..10_000 {
        let response = hybrid.search(&req).unwrap();
        assert!(!response.hits.is_empty());
    }

    // If we get here without OOM, we're good
}

// ============================================================================
// Edge Case Tests
// ============================================================================

/// Empty query returns empty results
#[test]
fn test_tier10_empty_query() {
    let db = create_test_db();
    let run_id = test_run_id();
    populate_test_data(&db, &run_id);

    let hybrid = db.hybrid();
    let req = SearchRequest::new(run_id, "");
    let response = hybrid.search(&req).unwrap();

    assert!(response.hits.is_empty());
}

/// Very long query works
#[test]
fn test_tier10_long_query() {
    let db = create_test_db();
    let run_id = test_run_id();
    populate_test_data(&db, &run_id);

    let hybrid = db.hybrid();
    let long_query = "test ".repeat(100);
    let req = SearchRequest::new(run_id, &long_query);
    let response = hybrid.search(&req).unwrap();

    // Should complete without error
    let _ = response.hits.len();
}

/// Unicode query works
#[test]
fn test_tier10_unicode_query() {
    let db = create_test_db();
    let run_id = test_run_id();

    let kv = KVStore::new(db.clone());
    let run_index = RunIndex::new(db.clone());
    run_index.create_run(&run_id.to_string()).unwrap();

    kv.put(
        &run_id,
        "unicode",
        Value::String("日本語 中文 한국어".into()),
    )
    .unwrap();

    let req = SearchRequest::new(run_id, "日本語");
    let response = kv.search(&req).unwrap();

    // Should complete without error
    let _ = response.hits.len();
}

/// Special characters in query work
#[test]
fn test_tier10_special_chars_query() {
    let db = create_test_db();
    let run_id = test_run_id();
    populate_test_data(&db, &run_id);

    let hybrid = db.hybrid();
    let req = SearchRequest::new(run_id, "test!@#$%^&*()");
    let response = hybrid.search(&req).unwrap();

    // Should complete without error
    let _ = response.hits.len();
}
