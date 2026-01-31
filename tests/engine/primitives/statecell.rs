//! StateCell Primitive Tests
//!
//! Tests for CAS-based versioned cells for coordination.

use crate::common::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;

// ============================================================================
// Basic Operations
// ============================================================================

#[test]
fn init_creates_new_cell() {
    let test_db = TestDb::new();
    let state = test_db.state();

    let result = state.init(&test_db.branch_id, "cell", Value::Int(42)).unwrap();
    assert!(result.version.as_u64() > 0);
}

#[test]
fn init_fails_if_exists() {
    let test_db = TestDb::new();
    let state = test_db.state();

    state.init(&test_db.branch_id, "cell", Value::Int(1)).unwrap();

    let result = state.init(&test_db.branch_id, "cell", Value::Int(2));
    assert!(result.is_err());
}

#[test]
fn read_nonexistent_returns_none() {
    let test_db = TestDb::new();
    let state = test_db.state();

    let result = state.read(&test_db.branch_id, "nonexistent").unwrap();
    assert!(result.is_none());
}

#[test]
fn read_returns_initialized_value() {
    let test_db = TestDb::new();
    let state = test_db.state();

    state.init(&test_db.branch_id, "cell", Value::Int(42)).unwrap();

    let result = state.read(&test_db.branch_id, "cell").unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap(), Value::Int(42));
}

#[test]
fn exists_returns_correct_status() {
    let test_db = TestDb::new();
    let state = test_db.state();

    // exists rewritten using read().is_some()
    assert!(state.read(&test_db.branch_id, "cell").unwrap().is_none());

    state.init(&test_db.branch_id, "cell", Value::Int(1)).unwrap();
    assert!(state.read(&test_db.branch_id, "cell").unwrap().is_some());
}

#[test]
#[ignore = "requires: StateCell::delete"]
fn delete_removes_cell() {
    let _test_db = TestDb::new();
    // delete is not in MVP
}

#[test]
#[ignore = "requires: StateCell::delete"]
fn delete_nonexistent_returns_false() {
    let _test_db = TestDb::new();
    // delete is not in MVP
}

// ============================================================================
// CAS Operations
// ============================================================================

#[test]
fn cas_succeeds_with_correct_version() {
    let test_db = TestDb::new();
    let state = test_db.state();

    state.init(&test_db.branch_id, "cell", Value::Int(1)).unwrap();

    let current = state.readv(&test_db.branch_id, "cell").unwrap().unwrap();
    let version = current.version();

    let result = state.cas(&test_db.branch_id, "cell", version, Value::Int(2));
    assert!(result.is_ok());

    let updated = state.read(&test_db.branch_id, "cell").unwrap().unwrap();
    assert_eq!(updated, Value::Int(2));
}

#[test]
fn cas_fails_with_wrong_version() {
    let test_db = TestDb::new();
    let state = test_db.state();

    state.init(&test_db.branch_id, "cell", Value::Int(1)).unwrap();

    // Use wrong version
    let result = state.cas(&test_db.branch_id, "cell", Version::counter(999999), Value::Int(2));
    assert!(result.is_err());

    // Value unchanged
    let current = state.read(&test_db.branch_id, "cell").unwrap().unwrap();
    assert_eq!(current, Value::Int(1));
}

#[test]
fn cas_fails_on_nonexistent_cell() {
    let test_db = TestDb::new();
    let state = test_db.state();

    let result = state.cas(&test_db.branch_id, "nonexistent", Version::counter(1), Value::Int(1));
    assert!(result.is_err());
}

#[test]
fn cas_version_increments() {
    let test_db = TestDb::new();
    let state = test_db.state();

    state.init(&test_db.branch_id, "cell", Value::Int(1)).unwrap();

    let v1 = state.readv(&test_db.branch_id, "cell").unwrap().unwrap().version();

    state.cas(&test_db.branch_id, "cell", v1, Value::Int(2)).unwrap();
    let v2 = state.readv(&test_db.branch_id, "cell").unwrap().unwrap().version();

    assert!(v2.as_u64() > v1.as_u64());
}

// ============================================================================
// Set (Unconditional Write)
// ============================================================================

#[test]
fn set_creates_if_not_exists() {
    let test_db = TestDb::new();
    let state = test_db.state();

    let result = state.set(&test_db.branch_id, "cell", Value::Int(42));
    assert!(result.is_ok());

    let current = state.read(&test_db.branch_id, "cell").unwrap().unwrap();
    assert_eq!(current, Value::Int(42));
}

#[test]
fn set_overwrites_without_version_check() {
    let test_db = TestDb::new();
    let state = test_db.state();

    state.init(&test_db.branch_id, "cell", Value::Int(1)).unwrap();

    // Set doesn't care about version
    state.set(&test_db.branch_id, "cell", Value::Int(100)).unwrap();

    let current = state.read(&test_db.branch_id, "cell").unwrap().unwrap();
    assert_eq!(current, Value::Int(100));
}

// ============================================================================
// Transition
// ============================================================================

#[test]
#[ignore = "requires: StateCell::transition"]
fn transition_reads_transforms_writes() {
    let _test_db = TestDb::new();
    // read-modify-write is an architectural principle
    // but transition() is not yet in the MVP API
}

#[test]
#[ignore = "requires: StateCell::transition_or_init"]
fn transition_or_init_initializes_if_missing() {
    let _test_db = TestDb::new();
    // transition_or_init is not yet in the MVP API
}

// ============================================================================
// List
// ============================================================================

#[test]
#[ignore = "requires: StateCell::list"]
fn list_returns_all_cells() {
    let _test_db = TestDb::new();
    // list() is not yet in the MVP API
}

#[test]
#[ignore = "requires: StateCell::list"]
fn list_empty_run_returns_empty() {
    let _test_db = TestDb::new();
    // list() is not yet in the MVP API
}

// ============================================================================
// Concurrency
// ============================================================================

#[test]
fn concurrent_cas_exactly_one_wins() {
    let test_db = TestDb::new_in_memory();
    let state = test_db.state();
    let branch_id = test_db.branch_id;

    state.init(&branch_id, "counter", Value::Int(0)).unwrap();

    let success_count = Arc::new(AtomicU64::new(0));
    let db = test_db.db.clone();
    let barrier = Arc::new(Barrier::new(4));

    let handles: Vec<_> = (0..4).map(|_| {
        let db = db.clone();
        let success = success_count.clone();
        let barrier = barrier.clone();

        thread::spawn(move || {
            let state = StateCell::new(db);

            // Read the current version
            let current = state.readv(&branch_id, "counter").unwrap().unwrap();
            let version = current.version();

            // Barrier: all threads have read the same version before any CAS.
            // Without this, a thread could complete its CAS before others read,
            // allowing them to see the updated version and CAS successfully.
            barrier.wait();

            if state.cas(&branch_id, "counter", version, Value::Int(1)).is_ok() {
                success.fetch_add(1, Ordering::SeqCst);
            }
        })
    }).collect();

    for h in handles {
        h.join().unwrap();
    }

    // Exactly one thread should have succeeded
    // (others see stale version after first CAS)
    let wins = success_count.load(Ordering::SeqCst);
    assert_eq!(wins, 1, "Exactly one CAS should succeed, got {}", wins);
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn empty_cell_name() {
    let test_db = TestDb::new();
    let state = test_db.state();

    state.init(&test_db.branch_id, "", Value::Int(1)).unwrap();
    // exists rewritten using read().is_some()
    assert_eq!(state.read(&test_db.branch_id, "").unwrap().unwrap(), Value::Int(1));
}

#[test]
fn special_characters_in_name() {
    let test_db = TestDb::new();
    let state = test_db.state();

    let name = "cell/with:special@chars";
    state.init(&test_db.branch_id, name, Value::Int(1)).unwrap();
    // exists rewritten using read().is_some()
    assert_eq!(state.read(&test_db.branch_id, name).unwrap().unwrap(), Value::Int(1));
}
