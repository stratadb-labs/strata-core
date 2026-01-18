//! ISSUE-011: Lock Sharding Insufficient for Scaling
//!
//! **Severity**: HIGH
//! **Location**: `/crates/storage/src/sharded.rs`
//!
//! **Problem**: Despite lock sharding, 4-thread disjoint key scaling is 0.20x
//! (should be ≥2.5x). Heavy lock contention persists.
//!
//! **Spec Requirement**: M4 targets ≥3.2x scaling at 4 threads.
//!
//! **Impact**: Performance doesn't scale with concurrent access.

use crate::test_utils::*;
use std::sync::Arc;
use std::thread;
use std::time::Instant;

/// Test concurrent disjoint key scaling.
#[test]
fn test_disjoint_key_scaling() {
    let test_db = TestDb::new_in_memory();
    let db = test_db.db.clone();

    // Single-threaded baseline
    let iterations = 1000;
    let start = Instant::now();
    {
        let kv = in_mem_primitives::KVStore::new(db.clone());
        let run_id = in_mem_core::types::RunId::new();
        for i in 0..iterations {
            kv.put(&run_id, &format!("single_key_{}", i), in_mem_core::value::Value::I64(i))
                .expect("put");
        }
    }
    let single_thread_time = start.elapsed();

    // 4-thread with disjoint keys
    let start = Instant::now();
    let mut handles = vec![];
    for t in 0..4 {
        let db = db.clone();
        let handle = thread::spawn(move || {
            let kv = in_mem_primitives::KVStore::new(db);
            let run_id = in_mem_core::types::RunId::new();
            for i in 0..(iterations / 4) {
                let key = format!("thread{}_key_{}", t, i);
                kv.put(&run_id, &key, in_mem_core::value::Value::I64(i as i64))
                    .expect("put");
            }
        });
        handles.push(handle);
    }
    for h in handles {
        h.join().expect("thread join");
    }
    let multi_thread_time = start.elapsed();

    let scaling = single_thread_time.as_secs_f64() / multi_thread_time.as_secs_f64();
    eprintln!(
        "Single: {:?}, 4-thread: {:?}, Scaling: {:.2}x",
        single_thread_time, multi_thread_time, scaling
    );

    // When ISSUE-011 is fixed:
    // assert!(scaling >= 2.5, "4-thread disjoint key scaling should be ≥2.5x, got {:.2}x", scaling);
}
