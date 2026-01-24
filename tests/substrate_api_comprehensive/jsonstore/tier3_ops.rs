//! JsonStore Tier 3 Operations Tests
//!
//! Tests for M11B Tier 3 features:
//! - json_array_push: Atomic array append
//! - json_increment: Atomic numeric increment
//! - json_array_pop: Atomic array pop

use crate::*;
use std::sync::Arc;
use std::thread;

// =============================================================================
// Array Push Tests
// =============================================================================

/// Test array push appends to array
#[test]
fn test_json_array_push_appends_to_array() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();
        let key = "push_doc";

        // Create document with array
        db.json_set(&run, key, "$", obj([
            ("items", Value::Array(vec![Value::Int(1), Value::Int(2)]))
        ])).unwrap();

        // Push new values
        let len = db.json_array_push(&run, key, "items", vec![Value::Int(3), Value::Int(4)]).unwrap();
        assert_eq!(len, 4, "Array should have 4 items");

        // Verify array content
        let items = db.json_get(&run, key, "items").unwrap().unwrap().value;
        if let Value::Array(arr) = items {
            assert_eq!(arr.len(), 4);
            assert_eq!(arr[0], Value::Int(1));
            assert_eq!(arr[1], Value::Int(2));
            assert_eq!(arr[2], Value::Int(3));
            assert_eq!(arr[3], Value::Int(4));
        } else {
            panic!("Expected array");
        }
    });
}

/// Test array push fails on non-array
#[test]
fn test_json_array_push_fails_on_non_array() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();
        let key = "non_array_doc";

        // Create document with non-array field
        db.json_set(&run, key, "$", obj([
            ("not_array", Value::String("hello".into()))
        ])).unwrap();

        // Try to push to non-array
        let result = db.json_array_push(&run, key, "not_array", vec![Value::Int(1)]);
        assert!(result.is_err(), "Should fail on non-array");
    });
}

/// Test array push fails on non-existent path
#[test]
fn test_json_array_push_fails_on_missing_path() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();
        let key = "missing_path_doc";

        // Create document without the path
        db.json_set(&run, key, "$", obj([])).unwrap();

        // Try to push to non-existent path
        let result = db.json_array_push(&run, key, "missing", vec![Value::Int(1)]);
        assert!(result.is_err(), "Should fail on missing path");
    });
}

/// Test concurrent array push no lost items
#[test]
fn test_concurrent_array_push_no_lost_items() {
    let db = create_inmemory_db();
    let substrate = Arc::new(SubstrateImpl::new(db));
    let run = ApiRunId::default_run_id();
    let key = "concurrent_push";

    // Create document with empty array
    substrate.json_set(&run, key, "$", obj([
        ("items", Value::Array(vec![]))
    ])).unwrap();

    let num_threads = 5;
    let pushes_per_thread = 10;

    let threads: Vec<_> = (0..num_threads)
        .map(|t| {
            let substrate = substrate.clone();
            let run = run.clone();

            thread::spawn(move || {
                for i in 0..pushes_per_thread {
                    let value = Value::Int((t * 100 + i) as i64);
                    // Retry on conflict
                    loop {
                        match substrate.json_array_push(&run, key, "items", vec![value.clone()]) {
                            Ok(_) => break,
                            Err(_) => continue, // Retry on conflict
                        }
                    }
                }
            })
        })
        .collect();

    for t in threads {
        t.join().unwrap();
    }

    // Verify all items were pushed
    let items = substrate.json_get(&run, key, "items").unwrap().unwrap().value;
    if let Value::Array(arr) = items {
        assert_eq!(arr.len(), num_threads * pushes_per_thread, "All items should be present");
    } else {
        panic!("Expected array");
    }
}

// =============================================================================
// Increment Tests
// =============================================================================

/// Test increment adds to number
#[test]
fn test_json_increment_adds_to_number() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();
        let key = "incr_doc";

        // Create document with counter
        db.json_set(&run, key, "$", obj([
            ("counter", Value::Int(10))
        ])).unwrap();

        // Increment
        let new_val = db.json_increment(&run, key, "counter", 5.0).unwrap();
        assert_eq!(new_val, 15.0);

        // Decrement (negative delta)
        let new_val = db.json_increment(&run, key, "counter", -3.0).unwrap();
        assert_eq!(new_val, 12.0);

        // Verify final value
        let counter = db.json_get(&run, key, "counter").unwrap().unwrap().value;
        assert_eq!(counter, Value::Int(12));
    });
}

/// Test increment with float
#[test]
fn test_json_increment_with_float() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();
        let key = "float_doc";

        // Create document with float counter
        db.json_set(&run, key, "$", obj([
            ("value", Value::Float(1.5))
        ])).unwrap();

        // Increment
        let new_val = db.json_increment(&run, key, "value", 0.5).unwrap();
        assert!((new_val - 2.0).abs() < 0.001);
    });
}

/// Test increment fails on non-number
#[test]
fn test_json_increment_fails_on_non_number() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();
        let key = "non_num_doc";

        // Create document with string field
        db.json_set(&run, key, "$", obj([
            ("not_number", Value::String("hello".into()))
        ])).unwrap();

        // Try to increment non-number
        let result = db.json_increment(&run, key, "not_number", 1.0);
        assert!(result.is_err(), "Should fail on non-number");
    });
}

/// Test concurrent increment no lost updates
#[test]
fn test_concurrent_increment_no_lost_updates() {
    let db = create_inmemory_db();
    let substrate = Arc::new(SubstrateImpl::new(db));
    let run = ApiRunId::default_run_id();
    let key = "concurrent_incr";

    // Create document with counter
    substrate.json_set(&run, key, "$", obj([
        ("counter", Value::Int(0))
    ])).unwrap();

    let num_threads = 10;
    let increments_per_thread = 100;
    let expected_total = (num_threads * increments_per_thread) as f64;

    let threads: Vec<_> = (0..num_threads)
        .map(|_| {
            let substrate = substrate.clone();
            let run = run.clone();

            thread::spawn(move || {
                for _ in 0..increments_per_thread {
                    // Retry on conflict
                    loop {
                        match substrate.json_increment(&run, key, "counter", 1.0) {
                            Ok(_) => break,
                            Err(_) => continue, // Retry on conflict
                        }
                    }
                }
            })
        })
        .collect();

    for t in threads {
        t.join().unwrap();
    }

    // Verify final value
    let counter = substrate.json_get(&run, key, "counter").unwrap().unwrap().value;
    let final_val = match counter {
        Value::Int(i) => i as f64,
        Value::Float(f) => f,
        _ => panic!("Expected number"),
    };
    assert_eq!(final_val, expected_total, "All increments should be counted");
}

// =============================================================================
// Array Pop Tests
// =============================================================================

/// Test array pop removes last
#[test]
fn test_json_array_pop_removes_last() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();
        let key = "pop_doc";

        // Create document with array
        db.json_set(&run, key, "$", obj([
            ("items", Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)]))
        ])).unwrap();

        // Pop last element
        let popped = db.json_array_pop(&run, key, "items").unwrap();
        assert_eq!(popped, Some(Value::Int(3)));

        // Verify array
        let items = db.json_get(&run, key, "items").unwrap().unwrap().value;
        if let Value::Array(arr) = items {
            assert_eq!(arr.len(), 2);
            assert_eq!(arr[0], Value::Int(1));
            assert_eq!(arr[1], Value::Int(2));
        } else {
            panic!("Expected array");
        }

        // Pop again
        let popped = db.json_array_pop(&run, key, "items").unwrap();
        assert_eq!(popped, Some(Value::Int(2)));
    });
}

/// Test array pop empty returns none
#[test]
fn test_json_array_pop_empty_returns_none() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();
        let key = "empty_pop_doc";

        // Create document with empty array
        db.json_set(&run, key, "$", obj([
            ("items", Value::Array(vec![]))
        ])).unwrap();

        // Pop from empty
        let popped = db.json_array_pop(&run, key, "items").unwrap();
        assert_eq!(popped, None, "Pop from empty array should return None");
    });
}

/// Test array pop fails on non-array
#[test]
fn test_json_array_pop_fails_on_non_array() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();
        let key = "non_array_pop_doc";

        // Create document with non-array field
        db.json_set(&run, key, "$", obj([
            ("not_array", Value::Int(42))
        ])).unwrap();

        // Try to pop from non-array
        let result = db.json_array_pop(&run, key, "not_array");
        assert!(result.is_err(), "Should fail on non-array");
    });
}

/// Test array pop on complex values
#[test]
fn test_json_array_pop_complex_values() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();
        let key = "complex_pop_doc";

        // Create document with array of objects
        db.json_set(&run, key, "$", obj([
            ("users", Value::Array(vec![
                obj([("name", Value::String("Alice".into()))]),
                obj([("name", Value::String("Bob".into()))]),
            ]))
        ])).unwrap();

        // Pop last element (complex object)
        let popped = db.json_array_pop(&run, key, "users").unwrap();
        assert!(popped.is_some());

        if let Some(Value::Object(map)) = popped {
            assert_eq!(map.get("name"), Some(&Value::String("Bob".into())));
        } else {
            panic!("Expected object");
        }
    });
}
