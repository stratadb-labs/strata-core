//! StateCell Invariants Tests
//!
//! Tests for all 7 invariants from PRIMITIVE_CONTRACT.md as they apply to StateCell:
//!
//! - I1: Everything is Addressable (run_id + cell_name)
//! - I2: Everything is Versioned (Counter-based versioning)
//! - I3: Everything is Transactional (CAS is atomic)
//! - I4: Everything Has a Lifecycle (CRUD: Create, Read, Update, Delete)
//! - I5: Everything Exists Within a Run (run isolation)
//! - I6: Everything is Introspectable (exists, get, list)
//! - I7: Reads and Writes Have Consistent Semantics
//!
//! Test data loaded from testdata/statecell_test_data.jsonl

use crate::test_data::load_statecell_test_data;
use crate::*;
use strata_core::Version;

// =============================================================================
// I1: EVERYTHING IS ADDRESSABLE
// StateCell: (run_id, cell_name) forms the unique address
// =============================================================================

#[test]
fn test_i1_cell_has_stable_identity() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_statecell_test_data();
    let entry = &test_data.entries[0];

    // Set a value
    substrate
        .state_set(&run, &entry.cell_name, entry.value.clone())
        .expect("set should succeed");

    // Get it back multiple times - identity should be stable
    for _ in 0..3 {
        let result = substrate
            .state_get(&run, &entry.cell_name)
            .expect("get should succeed")
            .expect("cell should exist");

        assert_eq!(result.value, entry.value, "Identity should be stable across reads");
    }
}

#[test]
fn test_i1_address_requires_both_run_and_cell_name() {
    let (_, substrate) = quick_setup();
    let run1 = ApiRunId::default();
    let run2 = ApiRunId::new();
    let test_data = load_statecell_test_data();

    // Ensure run2 exists
    substrate.run_create(Some(&run2), None).expect("run create should succeed");

    let entry = &test_data.entries[0];

    // Set in run1
    substrate
        .state_set(&run1, &entry.cell_name, Value::Int(100))
        .expect("set should succeed");

    // Same cell name in run2 is a different address
    substrate
        .state_set(&run2, &entry.cell_name, Value::Int(200))
        .expect("set should succeed");

    // Verify they are independent
    let v1 = substrate.state_get(&run1, &entry.cell_name).unwrap().unwrap();
    let v2 = substrate.state_get(&run2, &entry.cell_name).unwrap().unwrap();

    assert_eq!(v1.value, Value::Int(100));
    assert_eq!(v2.value, Value::Int(200));
}

#[test]
fn test_i1_different_cell_names_are_independent() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Set different cells
    substrate.state_set(&run, "cell_a", Value::Int(1)).unwrap();
    substrate.state_set(&run, "cell_b", Value::Int(2)).unwrap();
    substrate.state_set(&run, "cell_c", Value::Int(3)).unwrap();

    // Verify independence
    assert_eq!(substrate.state_get(&run, "cell_a").unwrap().unwrap().value, Value::Int(1));
    assert_eq!(substrate.state_get(&run, "cell_b").unwrap().unwrap().value, Value::Int(2));
    assert_eq!(substrate.state_get(&run, "cell_c").unwrap().unwrap().value, Value::Int(3));
}

// =============================================================================
// I2: EVERYTHING IS VERSIONED
// StateCell uses Counter-based versioning
// =============================================================================

#[test]
fn test_i2_set_returns_version() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_statecell_test_data();
    let entry = &test_data.entries[0];

    let version = substrate
        .state_set(&run, &entry.cell_name, entry.value.clone())
        .expect("set should succeed");

    // Must return Version::Counter
    match version {
        Version::Counter(_) => {}
        _ => panic!("StateCell must return Version::Counter, got {:?}", version),
    }
}

#[test]
fn test_i2_versions_increment_on_write() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    let v1 = substrate.state_set(&run, "counter_test", Value::Int(1)).unwrap();
    let v2 = substrate.state_set(&run, "counter_test", Value::Int(2)).unwrap();
    let v3 = substrate.state_set(&run, "counter_test", Value::Int(3)).unwrap();

    if let (Version::Counter(c1), Version::Counter(c2), Version::Counter(c3)) = (v1, v2, v3) {
        assert!(c2 > c1, "Counter should increment");
        assert!(c3 > c2, "Counter should increment");
    } else {
        panic!("Expected Version::Counter");
    }
}

#[test]
fn test_i2_read_includes_version() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_statecell_test_data();
    let entry = &test_data.entries[0];

    let write_version = substrate
        .state_set(&run, &entry.cell_name, entry.value.clone())
        .expect("set should succeed");

    let read_result = substrate
        .state_get(&run, &entry.cell_name)
        .expect("get should succeed")
        .expect("cell should exist");

    // Read includes version in Versioned<Value>
    assert_eq!(read_result.version, write_version, "Read version should match write version");
}

#[test]
fn test_i2_cas_uses_version_for_coordination() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Create cell with CAS (expected_counter = None means create-if-not-exists)
    let v1 = substrate
        .state_cas(&run, "cas_version_test", None, Value::Int(1))
        .expect("cas should succeed")
        .expect("cas should return version");

    let counter1 = match v1 {
        Version::Counter(c) => c,
        _ => panic!("Expected Counter"),
    };

    // CAS with correct counter succeeds
    let v2 = substrate
        .state_cas(&run, "cas_version_test", Some(counter1), Value::Int(2))
        .expect("cas should succeed")
        .expect("cas should succeed with correct counter");

    // CAS with wrong counter fails (returns None, not error)
    let v3 = substrate
        .state_cas(&run, "cas_version_test", Some(counter1), Value::Int(3))
        .expect("cas should not error");

    assert!(v3.is_none(), "CAS with stale counter should fail");

    // Verify value is still from v2
    let current = substrate.state_get(&run, "cas_version_test").unwrap().unwrap();
    assert_eq!(current.value, Value::Int(2));
    assert_eq!(current.version, v2);
}

// =============================================================================
// I3: EVERYTHING IS TRANSACTIONAL
// CAS provides atomic compare-and-swap semantics
// =============================================================================

#[test]
fn test_i3_cas_is_atomic() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Create cell
    let v1 = substrate
        .state_cas(&run, "atomic_test", None, Value::Int(0))
        .unwrap()
        .unwrap();

    let counter = match v1 {
        Version::Counter(c) => c,
        _ => panic!("Expected Counter"),
    };

    // CAS either fully succeeds or fully fails
    let success = substrate
        .state_cas(&run, "atomic_test", Some(counter), Value::Int(1))
        .unwrap();

    assert!(success.is_some(), "CAS should succeed");

    // Cell is now at new value, not in intermediate state
    let current = substrate.state_get(&run, "atomic_test").unwrap().unwrap();
    assert_eq!(current.value, Value::Int(1));
}

#[test]
fn test_i3_set_is_atomic() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_statecell_test_data();

    // Multiple sets are each atomic
    for entry in test_data.take(10) {
        substrate
            .state_set(&run, &entry.cell_name, entry.value.clone())
            .expect("set should succeed atomically");

        // Immediately readable after set
        let read = substrate
            .state_get(&run, &entry.cell_name)
            .expect("get should succeed")
            .expect("cell should exist");

        assert_eq!(read.value, entry.value, "Value should be immediately visible");
    }
}

// =============================================================================
// I4: EVERYTHING HAS A LIFECYCLE
// StateCell: Create (set/init), Read (get), Update (set/cas), Delete
// =============================================================================

#[test]
fn test_i4_lifecycle_create_via_set() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Cell doesn't exist
    assert!(!substrate.state_exists(&run, "lifecycle_cell").unwrap());

    // Create via set
    substrate.state_set(&run, "lifecycle_cell", Value::Int(1)).unwrap();

    // Now exists
    assert!(substrate.state_exists(&run, "lifecycle_cell").unwrap());
}

#[test]
fn test_i4_lifecycle_create_via_init() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Init creates if not exists
    let version = substrate
        .state_init(&run, "init_lifecycle", Value::Int(100))
        .expect("init should succeed");

    assert!(matches!(version, Version::Counter(1)), "First version should be 1");

    // Init fails if already exists
    let result = substrate.state_init(&run, "init_lifecycle", Value::Int(200));
    assert!(result.is_err(), "Init should fail if cell exists");

    // Value unchanged
    let current = substrate.state_get(&run, "init_lifecycle").unwrap().unwrap();
    assert_eq!(current.value, Value::Int(100));
}

#[test]
fn test_i4_lifecycle_read_via_get() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_statecell_test_data();
    let entry = &test_data.entries[0];

    // Set value
    substrate.state_set(&run, &entry.cell_name, entry.value.clone()).unwrap();

    // Read back
    let result = substrate.state_get(&run, &entry.cell_name).unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().value, entry.value);
}

#[test]
fn test_i4_lifecycle_update_via_set() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Create
    substrate.state_set(&run, "update_test", Value::Int(1)).unwrap();

    // Update
    substrate.state_set(&run, "update_test", Value::Int(2)).unwrap();
    assert_eq!(substrate.state_get(&run, "update_test").unwrap().unwrap().value, Value::Int(2));

    // Update again with different type
    substrate.state_set(&run, "update_test", Value::String("changed".into())).unwrap();
    assert_eq!(
        substrate.state_get(&run, "update_test").unwrap().unwrap().value,
        Value::String("changed".into())
    );
}

#[test]
fn test_i4_lifecycle_delete() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Create
    substrate.state_set(&run, "delete_test", Value::Int(1)).unwrap();
    assert!(substrate.state_exists(&run, "delete_test").unwrap());

    // Delete
    let deleted = substrate.state_delete(&run, "delete_test").unwrap();
    assert!(deleted, "Delete should return true");
    assert!(!substrate.state_exists(&run, "delete_test").unwrap());

    // Get returns None after delete
    assert!(substrate.state_get(&run, "delete_test").unwrap().is_none());
}

// =============================================================================
// I5: EVERYTHING EXISTS WITHIN A RUN
// All StateCell data is scoped to explicit run_id
// =============================================================================

#[test]
fn test_i5_run_is_always_explicit() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Every operation requires explicit run_id
    substrate.state_set(&run, "explicit_run", Value::Int(1)).unwrap();
    substrate.state_get(&run, "explicit_run").unwrap();
    substrate.state_exists(&run, "explicit_run").unwrap();
    substrate.state_delete(&run, "explicit_run").unwrap();

    // No implicit run context
}

#[test]
fn test_i5_run_is_unit_of_isolation() {
    let (_, substrate) = quick_setup();
    let run1 = ApiRunId::default();
    let run2 = ApiRunId::new();

    substrate.run_create(Some(&run2), None).unwrap();

    // Data in run1 is invisible in run2
    substrate.state_set(&run1, "isolated_cell", Value::Int(100)).unwrap();

    assert!(substrate.state_get(&run2, "isolated_cell").unwrap().is_none());
    assert!(!substrate.state_exists(&run2, "isolated_cell").unwrap());
}

#[test]
fn test_i5_same_cell_name_different_runs_independent() {
    let (_, substrate) = quick_setup();
    let run1 = ApiRunId::default();
    let run2 = ApiRunId::new();
    let test_data = load_statecell_test_data();

    substrate.run_create(Some(&run2), None).unwrap();

    // Same cell name in different runs
    for (i, entry) in test_data.take(5).iter().enumerate() {
        substrate
            .state_set(&run1, &entry.cell_name, Value::Int(i as i64))
            .unwrap();
        substrate
            .state_set(&run2, &entry.cell_name, Value::Int((i + 100) as i64))
            .unwrap();
    }

    // Verify isolation
    for (i, entry) in test_data.take(5).iter().enumerate() {
        let v1 = substrate.state_get(&run1, &entry.cell_name).unwrap().unwrap();
        let v2 = substrate.state_get(&run2, &entry.cell_name).unwrap().unwrap();

        assert_eq!(v1.value, Value::Int(i as i64));
        assert_eq!(v2.value, Value::Int((i + 100) as i64));
    }
}

// =============================================================================
// I6: EVERYTHING IS INTROSPECTABLE
// Can check existence, state, version, and list cells
// =============================================================================

#[test]
fn test_i6_can_check_existence_via_exists() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    assert!(!substrate.state_exists(&run, "introspect_cell").unwrap());

    substrate.state_set(&run, "introspect_cell", Value::Int(1)).unwrap();

    assert!(substrate.state_exists(&run, "introspect_cell").unwrap());
}

#[test]
fn test_i6_can_check_existence_via_get() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // None means doesn't exist
    assert!(substrate.state_get(&run, "get_introspect").unwrap().is_none());

    substrate.state_set(&run, "get_introspect", Value::Int(1)).unwrap();

    // Some means exists
    assert!(substrate.state_get(&run, "get_introspect").unwrap().is_some());
}

#[test]
fn test_i6_can_read_current_state() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_statecell_test_data();

    for entry in test_data.take(10) {
        substrate.state_set(&run, &entry.cell_name, entry.value.clone()).unwrap();

        let state = substrate.state_get(&run, &entry.cell_name).unwrap().unwrap();

        // Can introspect value
        assert_eq!(state.value, entry.value);

        // Can introspect version
        assert!(matches!(state.version, Version::Counter(_)));

        // Can introspect timestamp
        // (timestamp is part of Versioned<T>)
    }
}

#[test]
fn test_i6_can_list_all_cells() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // Create several cells
    let cell_names = vec!["list_a", "list_b", "list_c", "list_d"];
    for name in &cell_names {
        substrate.state_set(&run, name, Value::Int(1)).unwrap();
    }

    // List all cells
    let listed = substrate.state_list(&run).unwrap();

    // All created cells should be listed
    for name in &cell_names {
        assert!(listed.contains(&name.to_string()), "Cell '{}' should be in list", name);
    }
}

// =============================================================================
// I7: READS AND WRITES HAVE CONSISTENT SEMANTICS
// =============================================================================

#[test]
fn test_i7_reads_never_modify_state() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    substrate.state_set(&run, "read_only_test", Value::Int(42)).unwrap();

    let v1 = substrate.state_get(&run, "read_only_test").unwrap().unwrap();

    // Multiple reads don't change anything
    for _ in 0..10 {
        let v = substrate.state_get(&run, "read_only_test").unwrap().unwrap();
        assert_eq!(v.value, v1.value);
        assert_eq!(v.version, v1.version);
    }

    // exists() is also a read
    for _ in 0..10 {
        substrate.state_exists(&run, "read_only_test").unwrap();
    }

    // Version unchanged
    let v2 = substrate.state_get(&run, "read_only_test").unwrap().unwrap();
    assert_eq!(v2.version, v1.version);
}

#[test]
fn test_i7_writes_always_produce_versions() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    // set() returns version
    let v1 = substrate.state_set(&run, "write_version", Value::Int(1)).unwrap();
    assert!(matches!(v1, Version::Counter(_)));

    // cas() returns version on success
    let counter = match v1 {
        Version::Counter(c) => c,
        _ => panic!("Expected Counter"),
    };
    let v2 = substrate
        .state_cas(&run, "write_version", Some(counter), Value::Int(2))
        .unwrap()
        .unwrap();
    assert!(matches!(v2, Version::Counter(_)));

    // init() returns version
    let v3 = substrate.state_init(&run, "new_init_cell", Value::Int(0)).unwrap();
    assert!(matches!(v3, Version::Counter(_)));
}

#[test]
fn test_i7_read_write_separation() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_statecell_test_data();
    let entry = &test_data.entries[0];

    // Write
    substrate.state_set(&run, &entry.cell_name, entry.value.clone()).unwrap();

    // Read operations
    let get_result = substrate.state_get(&run, &entry.cell_name).unwrap();
    let exists_result = substrate.state_exists(&run, &entry.cell_name).unwrap();
    let list_result = substrate.state_list(&run).unwrap();

    // All reads succeeded
    assert!(get_result.is_some());
    assert!(exists_result);
    assert!(!list_result.is_empty());
}

// =============================================================================
// CORE_API_SHAPE REQUIREMENTS
// =============================================================================

#[test]
fn test_api_shape_version_counter_type() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    let version = substrate.state_set(&run, "api_shape_test", Value::Int(1)).unwrap();

    // StateCell must return Version::Counter
    assert!(
        matches!(version, Version::Counter(_)),
        "StateCell must use Version::Counter, got {:?}",
        version
    );
}

#[test]
fn test_api_shape_versioned_wrapper() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();

    substrate.state_set(&run, "versioned_test", Value::Int(42)).unwrap();

    let result = substrate.state_get(&run, "versioned_test").unwrap().unwrap();

    // Result is Versioned<Value> with value, version, and timestamp
    let _value: &Value = &result.value;
    let _version: &Version = &result.version;
    let _timestamp = result.timestamp;
}

// =============================================================================
// CROSS-MODE CONSISTENCY
// =============================================================================

#[test]
fn test_invariants_hold_across_modes() {
    test_across_modes("statecell_invariants", |db| {
        let substrate = create_substrate(db);
        let run = ApiRunId::default();

        // I1: Addressable
        substrate.state_set(&run, "inv_cell", Value::Int(1)).unwrap();

        // I2: Versioned
        let v = substrate.state_set(&run, "inv_cell", Value::Int(2)).unwrap();
        assert!(matches!(v, Version::Counter(_)));

        // I4: Lifecycle
        substrate.state_delete(&run, "inv_cell").unwrap();
        assert!(!substrate.state_exists(&run, "inv_cell").unwrap());

        // I6: Introspectable
        substrate.state_set(&run, "inv_cell2", Value::Int(3)).unwrap();
        let list = substrate.state_list(&run).unwrap();
        assert!(list.contains(&"inv_cell2".to_string()));

        true
    });
}

// =============================================================================
// TEST DATA INTEGRATION
// =============================================================================

#[test]
fn test_testdata_values_roundtrip() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_statecell_test_data();

    // Test a sample of entries
    for entry in test_data.take(100) {
        substrate
            .state_set(&run, &entry.cell_name, entry.value.clone())
            .expect("set should succeed");

        let result = substrate
            .state_get(&run, &entry.cell_name)
            .expect("get should succeed")
            .expect("cell should exist");

        assert_eq!(result.value, entry.value, "Value should roundtrip for {}", entry.cell_name);
    }
}

#[test]
fn test_init_test_entries() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_statecell_test_data();

    for init_test in test_data.get_init_tests() {
        // First init should succeed
        let result = substrate.state_init(&run, &init_test.cell_name, Value::Int(init_test.initial_value));
        assert!(result.is_ok(), "First init should succeed for {}", init_test.cell_name);

        // Second init should fail (cell already exists)
        let result2 = substrate.state_init(&run, &init_test.cell_name, Value::Int(999));
        assert!(result2.is_err(), "Second init should fail for {}", init_test.cell_name);
    }
}

#[test]
fn test_cas_test_sequences() {
    let (_, substrate) = quick_setup();
    let run = ApiRunId::default();
    let test_data = load_statecell_test_data();

    for cas_test in test_data.get_cas_tests() {
        for step in &cas_test.sequence {
            let result = substrate
                .state_cas(&run, &cas_test.cell_name, step.expected_counter, Value::Int(step.value))
                .expect("cas should not error");

            assert!(
                result.is_some(),
                "CAS should succeed for {} with expected_counter {:?}",
                cas_test.cell_name,
                step.expected_counter
            );
        }
    }
}
