//! M4 Regression Test Suite - Test Harness and Modules
//!
//! This module provides utilities for running semantic equivalence tests
//! across different durability modes.

use strata_core::types::RunId;
use strata_durability::wal::DurabilityMode;
use strata_engine::Database;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::{Duration, Instant};

// Test modules in priority order
pub mod aba_detection;
pub mod cross_primitive;
pub mod eventlog_semantics;
pub mod kill_switch;
pub mod kv_semantics;
pub mod m4_red_flags;
pub mod snapshot_monotonicity;
pub mod statecell_semantics;

// ============================================================================
// Test Harness Utilities
// ============================================================================

/// Create a test database with specified durability mode
pub fn create_test_db(mode: DurabilityMode) -> Arc<Database> {
    Arc::new(
        Database::builder()
            .durability(mode)
            .open_temp()
            .expect("Failed to create test database"),
    )
}

/// Create an in-memory test database (fastest)
pub fn create_inmemory_db() -> Arc<Database> {
    create_test_db(DurabilityMode::InMemory)
}

/// The three durability modes we test against
pub fn all_durability_modes() -> Vec<DurabilityMode> {
    vec![
        DurabilityMode::InMemory,
        DurabilityMode::default(), // Batched
        DurabilityMode::Strict,
    ]
}

/// Run a test workload across all durability modes and assert results are identical
///
/// # Panics
/// Panics if results differ across modes (semantic drift detected)
pub fn test_across_modes<F, T>(test_name: &str, workload: F)
where
    F: Fn(Arc<Database>) -> T,
    T: PartialEq + Debug,
{
    let modes = all_durability_modes();
    let mut results: Vec<(DurabilityMode, T)> = Vec::new();

    for mode in modes {
        let db = create_test_db(mode.clone());
        let result = workload(db);
        results.push((mode, result));
    }

    // Assert all results identical to first (InMemory)
    let (first_mode, first_result) = &results[0];
    for (mode, result) in &results[1..] {
        assert_eq!(
            first_result, result,
            "SEMANTIC DRIFT in '{}': {:?} produced {:?}, but {:?} produced {:?}",
            test_name, first_mode, first_result, mode, result
        );
    }
}

/// Run a test that should produce identical results regardless of mode,
/// but the results themselves may vary (e.g., UUIDs, timestamps)
///
/// Instead of comparing exact results, this takes a validation function.
pub fn test_across_modes_with_validation<F, T, V>(test_name: &str, workload: F, validate: V)
where
    F: Fn(Arc<Database>) -> T,
    T: Debug,
    V: Fn(&T) -> bool,
{
    let modes = all_durability_modes();

    for mode in modes {
        let db = create_test_db(mode.clone());
        let result = workload(db);
        assert!(
            validate(&result),
            "VALIDATION FAILED in '{}' with {:?}: got {:?}",
            test_name,
            mode,
            result
        );
    }
}

// ============================================================================
// Performance Measurement Utilities
// ============================================================================

/// Performance measurement result
#[derive(Debug, Clone)]
pub struct PerfResult {
    pub operation: String,
    pub iterations: usize,
    pub total_ns: u128,
    pub mean_ns: u64,
    pub p50_ns: u64,
    pub p95_ns: u64,
    pub p99_ns: u64,
    pub min_ns: u64,
    pub max_ns: u64,
}

impl PerfResult {
    /// Calculate ops per second
    pub fn ops_per_sec(&self) -> f64 {
        if self.total_ns == 0 {
            return 0.0;
        }
        (self.iterations as f64) / (self.total_ns as f64 / 1_000_000_000.0)
    }

    /// Check if p99 to mean ratio is within threshold
    pub fn tail_ratio(&self) -> f64 {
        if self.mean_ns == 0 {
            return 0.0;
        }
        self.p99_ns as f64 / self.mean_ns as f64
    }
}

/// Measure operation latency with warmup
pub fn measure_latency<F>(name: &str, iterations: usize, warmup: usize, mut f: F) -> PerfResult
where
    F: FnMut(usize),
{
    // Warmup
    for i in 0..warmup {
        f(i);
    }

    // Measure
    let mut latencies: Vec<u128> = Vec::with_capacity(iterations);
    let start = Instant::now();

    for i in 0..iterations {
        let op_start = Instant::now();
        f(i + warmup);
        latencies.push(op_start.elapsed().as_nanos());
    }

    let total_ns = start.elapsed().as_nanos();

    // Calculate statistics
    latencies.sort();
    let mean_ns = (latencies.iter().sum::<u128>() / iterations as u128) as u64;
    let p50_ns = latencies[iterations / 2] as u64;
    let p95_ns = latencies[iterations * 95 / 100] as u64;
    let p99_ns = latencies[iterations * 99 / 100] as u64;
    let min_ns = latencies[0] as u64;
    let max_ns = latencies[iterations - 1] as u64;

    PerfResult {
        operation: name.to_string(),
        iterations,
        total_ns,
        mean_ns,
        p50_ns,
        p95_ns,
        p99_ns,
        min_ns,
        max_ns,
    }
}

// ============================================================================
// Test Assertions
// ============================================================================

/// Assert latency is under threshold (microseconds)
pub fn assert_latency_under(result: &PerfResult, threshold_us: u64) {
    let threshold_ns = threshold_us * 1000;
    assert!(
        result.mean_ns <= threshold_ns,
        "LATENCY EXCEEDED: {} mean {}ns > {}ns threshold",
        result.operation,
        result.mean_ns,
        threshold_ns
    );
}

/// Assert throughput is above threshold (ops/sec)
pub fn assert_throughput_above(result: &PerfResult, min_ops_per_sec: f64) {
    let actual = result.ops_per_sec();
    assert!(
        actual >= min_ops_per_sec,
        "THROUGHPUT TOO LOW: {} got {:.0} ops/sec < {:.0} threshold",
        result.operation,
        actual,
        min_ops_per_sec
    );
}

/// Assert tail latency ratio is within threshold
pub fn assert_tail_ratio_under(result: &PerfResult, max_ratio: f64) {
    let ratio = result.tail_ratio();
    assert!(
        ratio <= max_ratio,
        "TAIL LATENCY EXPLOSION: {} p99/mean = {:.1}x > {:.1}x threshold",
        result.operation,
        ratio,
        max_ratio
    );
}

// ============================================================================
// Common Test Helpers
// ============================================================================

/// Generate a unique RunId for testing
pub fn test_run_id() -> RunId {
    RunId::new()
}

/// Generate multiple unique RunIds
pub fn test_run_ids(count: usize) -> Vec<RunId> {
    (0..count).map(|_| RunId::new()).collect()
}

/// Timeout wrapper for tests
pub fn with_timeout<F, T>(timeout: Duration, f: F) -> Option<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    use std::sync::mpsc;
    use std::thread;

    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let result = f();
        let _ = tx.send(result);
    });

    rx.recv_timeout(timeout).ok()
}

#[cfg(test)]
mod harness_tests {
    use super::*;

    #[test]
    fn test_create_inmemory_db() {
        let db = create_inmemory_db();
        assert!(db.is_open());
    }

    #[test]
    fn test_all_durability_modes() {
        let modes = all_durability_modes();
        assert_eq!(modes.len(), 3);
    }

    #[test]
    fn test_measure_latency() {
        let result = measure_latency("noop", 100, 10, |_| {});
        assert_eq!(result.iterations, 100);
        assert!(result.mean_ns < 1_000_000); // < 1ms for noop
    }
}
