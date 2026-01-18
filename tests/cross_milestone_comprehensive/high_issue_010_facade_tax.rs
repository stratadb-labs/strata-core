//! ISSUE-010: Facade Tax Exceeds Threshold (1472x vs 10x target)
//!
//! **Severity**: HIGH
//! **Location**: Multiple (KVStore, transaction system)
//!
//! **Problem**: Every KVStore.put() creates a full transaction, causing 1472x
//! overhead vs the 10x target. The A1/A0 ratio is 147× worse than acceptable.
//!
//! **Spec Requirement**: M4 Performance Optimization Reference targets A1/A0 ratio < 10x.
//!
//! **Impact**: Performance targets not achieved.

use crate::test_utils::*;
use std::time::Instant;

/// Test facade overhead ratio.
#[test]
fn test_facade_overhead_ratio() {
    let test_db = TestDb::new_in_memory();
    let kv = test_db.kv();
    let run_id = test_db.run_id;

    // Warm up
    for i in 0..100 {
        kv.put(&run_id, &format!("warmup_{}", i), in_mem_core::value::Value::I64(i))
            .expect("put");
    }

    // Measure put latency
    let iterations = 1000;
    let start = Instant::now();
    for i in 0..iterations {
        kv.put(&run_id, &format!("perf_key_{}", i), in_mem_core::value::Value::I64(i))
            .expect("put");
    }
    let elapsed = start.elapsed();
    let avg_latency_us = elapsed.as_micros() as f64 / iterations as f64;

    eprintln!("Average put latency: {:.2} µs", avg_latency_us);

    // When ISSUE-010 is fixed:
    // - In-memory mode should be <8µs per put
    // - Facade overhead (A1/A0) should be <10x

    // Target: <8µs for in-memory put
    // Current: ~2ms (2000µs) - way over target
}

/// Test that non-transactional fast paths exist (when implemented).
#[test]
fn test_non_transactional_fast_path() {
    // When ISSUE-010 is fixed:
    // - Single-key operations should have fast path
    // - kv.put_fast() or similar API should bypass full transaction

    let test_db = TestDb::new_in_memory();
    let kv = test_db.kv();

    // For now, verify basic put works
    kv.put(&test_db.run_id, "fast", in_mem_core::value::Value::I64(1))
        .expect("put");
}
