//! StateCell Basic Operations Tests
//!
//! Tests for fundamental StateCell operations:
//! - state_set / state_get
//! - state_delete
//! - state_exists

use crate::*;
use strata_core::Version;

/// Test basic set and get operations
#[test]
fn test_state_set_get() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();
        let cell = "test_cell";

        // Set a value
        let version = db.state_set(&run, cell, Value::Int(42)).unwrap();
        assert!(matches!(version, Version::Counter(_)));

        // Get the value back
        let result = db.state_get(&run, cell).unwrap();
        assert!(result.is_some());
        let versioned = result.unwrap();
        assert_eq!(versioned.value, Value::Int(42));
    });
}

/// Test that set returns incrementing versions
#[test]
fn test_state_set_incrementing_versions() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();
        let cell = "counter_cell";

        let v1 = db.state_set(&run, cell, Value::Int(1)).unwrap();
        let v2 = db.state_set(&run, cell, Value::Int(2)).unwrap();
        let v3 = db.state_set(&run, cell, Value::Int(3)).unwrap();

        // Versions should be incrementing
        if let (Version::Counter(c1), Version::Counter(c2), Version::Counter(c3)) = (v1, v2, v3) {
            assert!(c2 > c1, "v2 should be greater than v1");
            assert!(c3 > c2, "v3 should be greater than v2");
        }
    });
}

/// Test getting a non-existent cell
#[test]
fn test_state_get_nonexistent() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();

        let result = db.state_get(&run, "nonexistent_cell").unwrap();
        assert!(result.is_none());
    });
}

/// Test deleting a cell
#[test]
fn test_state_delete() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();
        let cell = "delete_cell";

        // Set a value
        db.state_set(&run, cell, Value::String("hello".to_string())).unwrap();
        assert!(db.state_get(&run, cell).unwrap().is_some());

        // Delete the cell
        let deleted = db.state_delete(&run, cell).unwrap();
        assert!(deleted);

        // Verify it's gone
        assert!(db.state_get(&run, cell).unwrap().is_none());
    });
}

/// Test deleting a non-existent cell
#[test]
fn test_state_delete_nonexistent() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();

        let deleted = db.state_delete(&run, "never_existed").unwrap();
        assert!(!deleted);
    });
}

/// Test exists operation
#[test]
fn test_state_exists() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();
        let cell = "exists_cell";

        // Should not exist initially
        assert!(!db.state_exists(&run, cell).unwrap());

        // Create the cell
        db.state_set(&run, cell, Value::Bool(true)).unwrap();
        assert!(db.state_exists(&run, cell).unwrap());

        // Delete the cell
        db.state_delete(&run, cell).unwrap();
        assert!(!db.state_exists(&run, cell).unwrap());
    });
}

/// Test all value types
#[test]
fn test_state_all_value_types() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();

        // Test each value type
        let test_cases = vec![
            ("null_cell", Value::Null),
            ("bool_cell", Value::Bool(true)),
            ("int_cell", Value::Int(-9999)),
            ("float_cell", Value::Float(3.14159)),
            ("string_cell", Value::String("hello world".to_string())),
            ("bytes_cell", Value::Bytes(vec![0xDE, 0xAD, 0xBE, 0xEF])),
            ("array_cell", Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)])),
        ];

        for (cell, value) in test_cases {
            db.state_set(&run, cell, value.clone()).unwrap();
            let result = db.state_get(&run, cell).unwrap().unwrap();
            assert_eq!(result.value, value, "Failed for cell: {}", cell);
        }
    });
}

/// Test updating a cell (overwrite)
#[test]
fn test_state_overwrite() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();
        let cell = "overwrite_cell";

        // Set initial value
        db.state_set(&run, cell, Value::Int(1)).unwrap();
        assert_eq!(db.state_get(&run, cell).unwrap().unwrap().value, Value::Int(1));

        // Overwrite with new value
        db.state_set(&run, cell, Value::Int(999)).unwrap();
        assert_eq!(db.state_get(&run, cell).unwrap().unwrap().value, Value::Int(999));

        // Overwrite with different type
        db.state_set(&run, cell, Value::String("changed".to_string())).unwrap();
        assert_eq!(
            db.state_get(&run, cell).unwrap().unwrap().value,
            Value::String("changed".to_string())
        );
    });
}

/// Test multiple cells
#[test]
fn test_state_multiple_cells() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();

        // Create multiple cells
        for i in 0..10 {
            let cell = format!("cell_{}", i);
            db.state_set(&run, &cell, Value::Int(i)).unwrap();
        }

        // Verify all cells
        for i in 0..10 {
            let cell = format!("cell_{}", i);
            let result = db.state_get(&run, &cell).unwrap().unwrap();
            assert_eq!(result.value, Value::Int(i));
        }
    });
}

/// Test cell isolation between runs
#[test]
fn test_state_run_isolation() {
    test_across_substrate_modes(|db| {
        let run1 = ApiRunId::default_run_id();
        let run2 = ApiRunId::new();
        let cell = "shared_name";

        // Ensure run2 exists
        db.run_create(Some(&run2), None).unwrap();

        // Set different values in different runs
        db.state_set(&run1, cell, Value::Int(100)).unwrap();
        db.state_set(&run2, cell, Value::Int(200)).unwrap();

        // Verify isolation
        assert_eq!(db.state_get(&run1, cell).unwrap().unwrap().value, Value::Int(100));
        assert_eq!(db.state_get(&run2, cell).unwrap().unwrap().value, Value::Int(200));
    });
}

// =============================================================================
// state_get_or_init tests
// =============================================================================

/// Test get_or_init returns existing value without calling default
#[test]
fn test_state_get_or_init_existing_value() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();
        let cell = "existing_cell";

        // Create the cell first
        db.state_set(&run, cell, Value::Int(42)).unwrap();

        // get_or_init should return existing value
        let result = db.state_get_or_init(&run, cell, || {
            panic!("Default should not be called for existing cell");
        }).unwrap();

        assert_eq!(result.value, Value::Int(42));
        assert!(matches!(result.version, Version::Counter(_)));
    });
}

/// Test get_or_init initializes non-existent cell with default
#[test]
fn test_state_get_or_init_creates_new_cell() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();
        let cell = "new_cell";

        // Cell doesn't exist, should call default and create
        let result = db.state_get_or_init(&run, cell, || Value::Int(999)).unwrap();

        assert_eq!(result.value, Value::Int(999));
        // New cells start at version 1
        assert_eq!(result.version, Version::Counter(1));

        // Verify the cell now exists
        let existing = db.state_get(&run, cell).unwrap().unwrap();
        assert_eq!(existing.value, Value::Int(999));
    });
}

/// Test lazy evaluation - default closure only called when needed
#[test]
fn test_state_get_or_init_lazy_default() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();
        let cell = "lazy_cell";
        let call_count = AtomicUsize::new(0);

        // First call - cell doesn't exist, default should be called
        let _result = db.state_get_or_init(&run, cell, || {
            call_count.fetch_add(1, Ordering::SeqCst);
            Value::Int(100)
        }).unwrap();
        assert_eq!(call_count.load(Ordering::SeqCst), 1);

        // Second call - cell exists, default should NOT be called
        let _result = db.state_get_or_init(&run, cell, || {
            call_count.fetch_add(1, Ordering::SeqCst);
            Value::Int(200)
        }).unwrap();
        assert_eq!(call_count.load(Ordering::SeqCst), 1, "Default should not be called on second access");
    });
}

/// Test get_or_init returns correct version for existing cells
#[test]
fn test_state_get_or_init_preserves_version() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();
        let cell = "version_cell";

        // Create and update the cell multiple times
        db.state_set(&run, cell, Value::Int(1)).unwrap();
        db.state_set(&run, cell, Value::Int(2)).unwrap();
        db.state_set(&run, cell, Value::Int(3)).unwrap();

        // get_or_init should return current version (3)
        let result = db.state_get_or_init(&run, cell, || Value::Int(999)).unwrap();

        assert_eq!(result.value, Value::Int(3));
        assert_eq!(result.version, Version::Counter(3));
    });
}

/// Test get_or_init with expensive default (demonstrates lazy benefit)
#[test]
fn test_state_get_or_init_expensive_default() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();
        let cell = "expensive_cell";

        // Pre-populate the cell
        db.state_set(&run, cell, Value::String("cached".to_string())).unwrap();

        // This default is "expensive" but should never be called
        let result = db.state_get_or_init(&run, cell, || {
            // Simulate expensive computation
            let mut s = String::new();
            for i in 0..10000 {
                s.push_str(&format!("{}", i));
            }
            Value::String(s)
        }).unwrap();

        // Should return existing value, not the expensive computed one
        assert_eq!(result.value, Value::String("cached".to_string()));
    });
}

/// Test get_or_init with all value types
#[test]
fn test_state_get_or_init_all_value_types() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();

        let test_cases = vec![
            ("goi_null", Value::Null),
            ("goi_bool", Value::Bool(true)),
            ("goi_int", Value::Int(-42)),
            ("goi_float", Value::Float(2.718)),
            ("goi_string", Value::String("hello".to_string())),
            ("goi_bytes", Value::Bytes(vec![1, 2, 3])),
        ];

        for (cell, expected_value) in test_cases {
            let result = db.state_get_or_init(&run, cell, || expected_value.clone()).unwrap();
            assert_eq!(result.value, expected_value, "Failed for cell: {}", cell);
        }
    });
}

/// Test get_or_init run isolation
#[test]
fn test_state_get_or_init_run_isolation() {
    test_across_substrate_modes(|db| {
        let run1 = ApiRunId::default_run_id();
        let run2 = ApiRunId::new();
        let cell = "isolated_goi";

        // Ensure run2 exists
        db.run_create(Some(&run2), None).unwrap();

        // Initialize in run1
        let r1 = db.state_get_or_init(&run1, cell, || Value::Int(100)).unwrap();
        assert_eq!(r1.value, Value::Int(100));

        // Initialize in run2 - should use its own default
        let r2 = db.state_get_or_init(&run2, cell, || Value::Int(200)).unwrap();
        assert_eq!(r2.value, Value::Int(200));

        // Verify both still have their own values
        let check1 = db.state_get_or_init(&run1, cell, || Value::Int(999)).unwrap();
        let check2 = db.state_get_or_init(&run2, cell, || Value::Int(999)).unwrap();
        assert_eq!(check1.value, Value::Int(100));
        assert_eq!(check2.value, Value::Int(200));
    });
}

/// Test get_or_init after delete
#[test]
fn test_state_get_or_init_after_delete() {
    test_across_substrate_modes(|db| {
        let run = ApiRunId::default_run_id();
        let cell = "delete_goi";

        // Create initial value
        let v1 = db.state_get_or_init(&run, cell, || Value::Int(1)).unwrap();
        assert_eq!(v1.value, Value::Int(1));

        // Delete the cell
        db.state_delete(&run, cell).unwrap();

        // get_or_init should create a new cell
        let v2 = db.state_get_or_init(&run, cell, || Value::Int(2)).unwrap();
        assert_eq!(v2.value, Value::Int(2));
        // New cell starts at version 1 again
        assert_eq!(v2.version, Version::Counter(1));
    });
}
