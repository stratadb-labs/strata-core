# Epic 87a: Core Validation Suite

**Goal**: Comprehensive validation of M11a core contract before proceeding to M11b

**Dependencies**: Epics 80, 81, 82, 83, 84

**Milestone**: M11a (Core Contract & API) - EXIT GATE

---

## Critical Importance

> **THIS EPIC IS THE EXIT GATE FOR M11a**
>
> All tests in this epic MUST pass before any M11b work begins.
> The core contract validated here becomes FROZEN after M11a.
> Any bugs discovered in M11b that trace to core contract require
> fixing and re-running this entire validation suite.

---

## Test-Driven Development Protocol

> **CRITICAL**: This epic follows strict Test-Driven Development (TDD).

### NEVER Modify Tests to Make Them Pass

> **ABSOLUTE RULE**: When a test fails, the problem is in the implementation, NOT the test.

**FORBIDDEN behaviors:**
- Changing test assertions to match buggy output
- Weakening test conditions
- Removing test cases that expose bugs
- Adding `#[ignore]` to failing tests
- Changing expected values to match actual (wrong) values

**REQUIRED behaviors:**
- Investigate WHY the test fails
- Fix the implementation to match the specification
- If the spec is wrong, get explicit approval before changing both spec AND test
- Document any spec changes in the epic and notify all stakeholders

**Special rule for this epic:**
Since this is a validation suite, tests here are DERIVED from the contract specification.
If a test fails, it means the implementation does not match the contract.
The contract is the source of truth. Fix the implementation.

---

## Scope

- Value model invariant tests
- Wire encoding round-trip tests
- Facade-Substrate parity tests
- Error model verification
- Determinism tests
- Contract stability tests (golden files)

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #592 | Value Model Invariant Tests | CRITICAL |
| #593 | Wire Encoding Round-Trip Tests | CRITICAL |
| #594 | Facade-Substrate Parity Tests | CRITICAL |
| #595 | Determinism Verification Tests | CRITICAL |

---

## Story #592: Value Model Invariant Tests

**File**: `crates/tests/src/validation/value_model.rs` (NEW)

**Deliverable**: Exhaustive tests proving value model correctness

### Tests

```rust
//! Value Model Validation Tests
//!
//! These tests validate the core invariants of the Strata value model.
//! ALL tests must pass before M11a is complete.
//!
//! DO NOT MODIFY THESE TESTS TO MAKE THEM PASS.
//! Fix the implementation instead.

#[cfg(test)]
mod value_model_validation {
    use strata_core::value::Value;
    use std::collections::HashMap;

    // =========================================================================
    // INVARIANT: Exactly 8 value types
    // =========================================================================

    #[test]
    fn inv_001_exactly_eight_value_types() {
        // Verify all 8 types exist and are distinct
        let values = vec![
            Value::Null,
            Value::Bool(true),
            Value::Int(0),
            Value::Float(0.0),
            Value::String(String::new()),
            Value::Bytes(vec![]),
            Value::Array(vec![]),
            Value::Object(HashMap::new()),
        ];

        let type_names: std::collections::HashSet<_> =
            values.iter().map(|v| v.type_name()).collect();

        assert_eq!(type_names.len(), 8, "Must have exactly 8 distinct types");
    }

    // =========================================================================
    // INVARIANT: No implicit type coercion
    // =========================================================================

    #[test]
    fn inv_002_int_not_equal_float() {
        // CRITICAL: Int(1) != Float(1.0)
        assert_ne!(Value::Int(1), Value::Float(1.0));
        assert_ne!(Value::Int(0), Value::Float(0.0));
        assert_ne!(Value::Int(-1), Value::Float(-1.0));
        assert_ne!(Value::Int(i64::MAX), Value::Float(i64::MAX as f64));
    }

    #[test]
    fn inv_003_string_not_equal_bytes() {
        // CRITICAL: String("abc") != Bytes([97, 98, 99])
        assert_ne!(
            Value::String("abc".to_string()),
            Value::Bytes(vec![97, 98, 99])
        );
        assert_ne!(
            Value::String("".to_string()),
            Value::Bytes(vec![])
        );
    }

    #[test]
    fn inv_004_bool_not_equal_int() {
        // CRITICAL: Bool(true) != Int(1), Bool(false) != Int(0)
        assert_ne!(Value::Bool(true), Value::Int(1));
        assert_ne!(Value::Bool(false), Value::Int(0));
    }

    #[test]
    fn inv_005_null_not_equal_zero_or_empty() {
        // Null is distinct from all "zero" values
        assert_ne!(Value::Null, Value::Int(0));
        assert_ne!(Value::Null, Value::Float(0.0));
        assert_ne!(Value::Null, Value::Bool(false));
        assert_ne!(Value::Null, Value::String(String::new()));
        assert_ne!(Value::Null, Value::Bytes(vec![]));
        assert_ne!(Value::Null, Value::Array(vec![]));
        assert_ne!(Value::Null, Value::Object(HashMap::new()));
    }

    #[test]
    fn inv_006_different_types_never_equal() {
        let values = vec![
            Value::Null,
            Value::Bool(true),
            Value::Int(1),
            Value::Float(1.0),
            Value::String("1".to_string()),
            Value::Bytes(vec![1]),
            Value::Array(vec![Value::Int(1)]),
            Value::Object({
                let mut m = HashMap::new();
                m.insert("1".to_string(), Value::Int(1));
                m
            }),
        ];

        // Each pair of different types must be unequal
        for (i, v1) in values.iter().enumerate() {
            for (j, v2) in values.iter().enumerate() {
                if i != j {
                    assert_ne!(v1, v2, "Different types must not be equal: {:?} vs {:?}", v1, v2);
                }
            }
        }
    }

    // =========================================================================
    // INVARIANT: IEEE-754 float semantics
    // =========================================================================

    #[test]
    fn inv_007_nan_not_equal_nan() {
        // CRITICAL: NaN != NaN (IEEE-754)
        assert_ne!(Value::Float(f64::NAN), Value::Float(f64::NAN));
    }

    #[test]
    fn inv_008_negative_zero_equals_positive_zero() {
        // -0.0 == 0.0 (IEEE-754)
        assert_eq!(Value::Float(-0.0), Value::Float(0.0));
    }

    #[test]
    fn inv_009_infinity_equals_self() {
        assert_eq!(Value::Float(f64::INFINITY), Value::Float(f64::INFINITY));
        assert_eq!(Value::Float(f64::NEG_INFINITY), Value::Float(f64::NEG_INFINITY));
    }

    #[test]
    fn inv_010_positive_infinity_not_equal_negative_infinity() {
        assert_ne!(Value::Float(f64::INFINITY), Value::Float(f64::NEG_INFINITY));
    }

    // =========================================================================
    // INVARIANT: Float edge cases preserved
    // =========================================================================

    #[test]
    fn inv_011_negative_zero_sign_preserved() {
        let v = Value::Float(-0.0);
        match v {
            Value::Float(f) => {
                assert!(f.is_sign_negative(), "-0.0 sign must be preserved");
            }
            _ => panic!("Expected Float"),
        }
    }

    #[test]
    fn inv_012_subnormals_preserved() {
        let subnormal = f64::from_bits(1); // Smallest positive subnormal
        assert!(subnormal.is_subnormal());

        let v = Value::Float(subnormal);
        match v {
            Value::Float(f) => {
                assert!(f.is_subnormal(), "Subnormal must be preserved");
                assert_eq!(f.to_bits(), 1);
            }
            _ => panic!("Expected Float"),
        }
    }

    #[test]
    fn inv_013_float_precision_preserved() {
        let precise = 1.0000000000000002_f64;
        let v = Value::Float(precise);
        match v {
            Value::Float(f) => {
                assert_eq!(f.to_bits(), precise.to_bits(), "Full f64 precision must be preserved");
            }
            _ => panic!("Expected Float"),
        }
    }

    // =========================================================================
    // INVARIANT: Same-type equality
    // =========================================================================

    #[test]
    fn inv_014_same_type_equality() {
        // Same values of same type must be equal
        assert_eq!(Value::Null, Value::Null);
        assert_eq!(Value::Bool(true), Value::Bool(true));
        assert_eq!(Value::Bool(false), Value::Bool(false));
        assert_eq!(Value::Int(42), Value::Int(42));
        assert_eq!(Value::Float(3.14), Value::Float(3.14));
        assert_eq!(Value::String("hello".into()), Value::String("hello".into()));
        assert_eq!(Value::Bytes(vec![1, 2, 3]), Value::Bytes(vec![1, 2, 3]));
        assert_eq!(
            Value::Array(vec![Value::Int(1)]),
            Value::Array(vec![Value::Int(1)])
        );
    }

    #[test]
    fn inv_015_object_equality_ignores_insertion_order() {
        let mut m1 = HashMap::new();
        m1.insert("a".to_string(), Value::Int(1));
        m1.insert("b".to_string(), Value::Int(2));

        let mut m2 = HashMap::new();
        m2.insert("b".to_string(), Value::Int(2));
        m2.insert("a".to_string(), Value::Int(1));

        assert_eq!(Value::Object(m1), Value::Object(m2));
    }

    #[test]
    fn inv_016_array_equality_respects_order() {
        assert_ne!(
            Value::Array(vec![Value::Int(1), Value::Int(2)]),
            Value::Array(vec![Value::Int(2), Value::Int(1)])
        );
    }
}
```

### Acceptance Criteria

- [ ] All INV-* tests pass
- [ ] Tests cover all documented invariants
- [ ] No tests modified to pass
- [ ] Float edge cases all verified

---

## Story #593: Wire Encoding Round-Trip Tests

**File**: `crates/tests/src/validation/wire_encoding.rs` (NEW)

**Deliverable**: Exhaustive round-trip tests for wire encoding

### Tests

```rust
//! Wire Encoding Validation Tests
//!
//! These tests validate that all values survive wire encoding round-trip.
//! ALL tests must pass before M11a is complete.
//!
//! DO NOT MODIFY THESE TESTS TO MAKE THEM PASS.

#[cfg(test)]
mod wire_encoding_validation {
    use strata_core::value::Value;
    use strata_wire::json::{encode_json, decode_json};
    use std::collections::HashMap;

    // =========================================================================
    // INVARIANT: All value types round-trip
    // =========================================================================

    #[test]
    fn wire_001_null_round_trip() {
        let original = Value::Null;
        let json = encode_json(&original);
        let decoded = decode_json(&json).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn wire_002_bool_round_trip() {
        for b in [true, false] {
            let original = Value::Bool(b);
            let json = encode_json(&original);
            let decoded = decode_json(&json).unwrap();
            assert_eq!(original, decoded);
        }
    }

    #[test]
    fn wire_003_int_round_trip() {
        for i in [0, 1, -1, 42, -999, i64::MAX, i64::MIN] {
            let original = Value::Int(i);
            let json = encode_json(&original);
            let decoded = decode_json(&json).unwrap();
            assert_eq!(original, decoded, "Int {} must round-trip", i);
        }
    }

    #[test]
    fn wire_004_float_normal_round_trip() {
        for f in [0.0, 1.5, -2.5, 3.14159, f64::MAX, f64::MIN_POSITIVE] {
            let original = Value::Float(f);
            let json = encode_json(&original);
            let decoded = decode_json(&json).unwrap();
            assert_eq!(original, decoded, "Float {} must round-trip", f);
        }
    }

    #[test]
    fn wire_005_float_nan_round_trip() {
        let original = Value::Float(f64::NAN);
        let json = encode_json(&original);
        let decoded = decode_json(&json).unwrap();

        // NaN round-trips to NaN (but NaN != NaN, so check is_nan)
        match decoded {
            Value::Float(f) => assert!(f.is_nan(), "NaN must round-trip to NaN"),
            _ => panic!("Expected Float"),
        }
    }

    #[test]
    fn wire_006_float_infinity_round_trip() {
        for inf in [f64::INFINITY, f64::NEG_INFINITY] {
            let original = Value::Float(inf);
            let json = encode_json(&original);
            let decoded = decode_json(&json).unwrap();
            assert_eq!(original, decoded, "Infinity must round-trip");
        }
    }

    #[test]
    fn wire_007_float_negative_zero_round_trip() {
        let original = Value::Float(-0.0);
        let json = encode_json(&original);
        let decoded = decode_json(&json).unwrap();

        match decoded {
            Value::Float(f) => {
                assert!(f.is_sign_negative(), "-0.0 must preserve sign through wire");
            }
            _ => panic!("Expected Float"),
        }
    }

    #[test]
    fn wire_008_string_round_trip() {
        for s in ["", "hello", "日本語", "🚀", "a\n\t\"b\\c"] {
            let original = Value::String(s.to_string());
            let json = encode_json(&original);
            let decoded = decode_json(&json).unwrap();
            assert_eq!(original, decoded, "String {:?} must round-trip", s);
        }
    }

    #[test]
    fn wire_009_bytes_round_trip() {
        let test_cases: Vec<Vec<u8>> = vec![
            vec![],
            vec![0],
            vec![255],
            vec![0, 127, 255],
            (0..=255).collect(),
        ];

        for bytes in test_cases {
            let original = Value::Bytes(bytes.clone());
            let json = encode_json(&original);
            let decoded = decode_json(&json).unwrap();
            assert_eq!(original, decoded, "Bytes {:?} must round-trip", bytes);
        }
    }

    #[test]
    fn wire_010_array_round_trip() {
        let original = Value::Array(vec![
            Value::Int(1),
            Value::String("two".into()),
            Value::Bool(true),
            Value::Array(vec![Value::Null]),
        ]);

        let json = encode_json(&original);
        let decoded = decode_json(&json).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn wire_011_object_round_trip() {
        let mut map = HashMap::new();
        map.insert("int".to_string(), Value::Int(42));
        map.insert("string".to_string(), Value::String("hello".into()));
        map.insert("nested".to_string(), Value::Object({
            let mut inner = HashMap::new();
            inner.insert("key".to_string(), Value::Bool(true));
            inner
        }));

        let original = Value::Object(map);
        let json = encode_json(&original);
        let decoded = decode_json(&json).unwrap();
        assert_eq!(original, decoded);
    }

    // =========================================================================
    // INVARIANT: Special wrapper formats
    // =========================================================================

    #[test]
    fn wire_012_bytes_uses_wrapper() {
        let value = Value::Bytes(vec![1, 2, 3]);
        let json = encode_json(&value);

        assert!(json.contains("$bytes"), "Bytes must use $bytes wrapper");
    }

    #[test]
    fn wire_013_nan_uses_wrapper() {
        let value = Value::Float(f64::NAN);
        let json = encode_json(&value);

        assert!(json.contains("$f64"), "NaN must use $f64 wrapper");
        assert!(json.contains("NaN"));
    }

    #[test]
    fn wire_014_infinity_uses_wrapper() {
        let pos = Value::Float(f64::INFINITY);
        let neg = Value::Float(f64::NEG_INFINITY);

        assert!(encode_json(&pos).contains(r#"{"$f64":"+Inf"}"#));
        assert!(encode_json(&neg).contains(r#"{"$f64":"-Inf"}"#));
    }

    #[test]
    fn wire_015_negative_zero_uses_wrapper() {
        let value = Value::Float(-0.0);
        let json = encode_json(&value);

        assert!(json.contains("$f64"), "-0.0 must use $f64 wrapper");
        assert!(json.contains("-0.0"));
    }

    #[test]
    fn wire_016_normal_float_no_wrapper() {
        let value = Value::Float(1.5);
        let json = encode_json(&value);

        assert!(!json.contains("$f64"), "Normal floats must not use wrapper");
        assert_eq!(json, "1.5");
    }

    // =========================================================================
    // INVARIANT: Type distinction preserved
    // =========================================================================

    #[test]
    fn wire_017_int_and_float_distinguished() {
        // After round-trip, Int(1) must still be Int, not Float
        let int_val = Value::Int(1);
        let int_json = encode_json(&int_val);
        let int_decoded = decode_json(&int_json).unwrap();

        assert!(matches!(int_decoded, Value::Int(1)), "Int must decode as Int");

        // And Float(1.0) must still be Float
        let float_val = Value::Float(1.0);
        let float_json = encode_json(&float_val);
        let float_decoded = decode_json(&float_json).unwrap();

        assert!(matches!(float_decoded, Value::Float(_)), "Float must decode as Float");
    }

    #[test]
    fn wire_018_string_and_bytes_distinguished() {
        let string_val = Value::String("test".to_string());
        let bytes_val = Value::Bytes(b"test".to_vec());

        let string_json = encode_json(&string_val);
        let bytes_json = encode_json(&bytes_val);

        // Different JSON representations
        assert_ne!(string_json, bytes_json);

        // Round-trip preserves types
        assert!(matches!(decode_json(&string_json).unwrap(), Value::String(_)));
        assert!(matches!(decode_json(&bytes_json).unwrap(), Value::Bytes(_)));
    }
}
```

### Acceptance Criteria

- [ ] All WIRE-* tests pass
- [ ] All 8 value types round-trip
- [ ] Float edge cases use correct wrappers
- [ ] Type distinction preserved through wire

---

## Story #594: Facade-Substrate Parity Tests

**File**: `crates/tests/src/validation/parity.rs` (NEW)

**Deliverable**: Tests proving facade desugars correctly to substrate

### Tests

```rust
//! Facade-Substrate Parity Tests
//!
//! These tests validate that every facade operation produces identical
//! results to the equivalent substrate operation sequence.
//!
//! DO NOT MODIFY THESE TESTS TO MAKE THEM PASS.

#[cfg(test)]
mod parity_validation {
    use strata_api::{Facade, Substrate};
    use strata_core::value::Value;
    use strata_testing::TestHarness;

    // =========================================================================
    // INVARIANT: Facade desugars to substrate
    // =========================================================================

    #[test]
    fn parity_001_set_get() {
        let harness = TestHarness::new();
        let facade = harness.facade();
        let substrate = harness.substrate();

        // Facade path
        facade.set("key1", Value::Int(42)).unwrap();
        let facade_result = facade.get("key1").unwrap();

        // Substrate path (manual desugaring)
        substrate.kv_put("default", "key2", Value::Int(42)).unwrap();
        let substrate_result = substrate.kv_get("default", "key2").unwrap()
            .map(|v| v.value);

        // Results must be identical
        assert_eq!(facade_result, substrate_result);
    }

    #[test]
    fn parity_002_delete() {
        let harness = TestHarness::new();
        let facade = harness.facade();
        let substrate = harness.substrate();

        // Setup
        facade.set("fa", Value::Int(1)).unwrap();
        facade.set("fb", Value::Int(2)).unwrap();
        substrate.kv_put("default", "sa", Value::Int(1)).unwrap();
        substrate.kv_put("default", "sb", Value::Int(2)).unwrap();

        // Facade delete
        let facade_count = facade.delete(&["fa", "fb", "missing"]).unwrap();

        // Substrate delete
        let mut substrate_count = 0;
        for key in ["sa", "sb", "missing"] {
            if substrate.kv_delete("default", key).unwrap() {
                substrate_count += 1;
            }
        }

        assert_eq!(facade_count, substrate_count as u64);
    }

    #[test]
    fn parity_003_incr() {
        let harness = TestHarness::new();
        let facade = harness.facade();
        let substrate = harness.substrate();

        // Setup
        facade.set("fcounter", Value::Int(10)).unwrap();
        substrate.kv_put("default", "scounter", Value::Int(10)).unwrap();

        // Facade incr
        let facade_result = facade.incr("fcounter").unwrap();

        // Substrate incr
        let substrate_result = substrate.kv_incr("default", "scounter", 1).unwrap();

        assert_eq!(facade_result, substrate_result);
    }

    #[test]
    fn parity_004_mset_mget() {
        let harness = TestHarness::new();
        let facade = harness.facade();
        let substrate = harness.substrate();

        // Facade mset
        facade.mset(&[
            ("fa", Value::Int(1)),
            ("fb", Value::Int(2)),
        ]).unwrap();

        // Substrate mset (using transaction)
        substrate.with_transaction(|txn| {
            txn.kv_put("default", "sa", Value::Int(1))?;
            txn.kv_put("default", "sb", Value::Int(2))?;
            Ok(())
        }).unwrap();

        // mget
        let facade_results = facade.mget(&["fa", "fb"]).unwrap();
        let substrate_results: Vec<_> = ["sa", "sb"]
            .iter()
            .map(|k| substrate.kv_get("default", k).unwrap().map(|v| v.value))
            .collect();

        assert_eq!(facade_results, substrate_results);
    }

    #[test]
    fn parity_005_json_operations() {
        let harness = TestHarness::new();
        let facade = harness.facade();
        let substrate = harness.substrate();

        let doc = Value::Object({
            let mut m = std::collections::HashMap::new();
            m.insert("name".to_string(), Value::String("test".into()));
            m
        });

        // Facade json_set
        facade.json_set("fdoc", "$", doc.clone()).unwrap();

        // Substrate json_set
        substrate.json_set("default", "sdoc", "$", doc.clone()).unwrap();

        // json_get
        let facade_result = facade.json_get("fdoc", "$.name").unwrap();
        let substrate_result = substrate.json_get("default", "sdoc", "$.name").unwrap();

        assert_eq!(facade_result, substrate_result);
    }

    #[test]
    fn parity_006_cas_operations() {
        let harness = TestHarness::new();
        let facade = harness.facade();
        let substrate = harness.substrate();

        // Facade CAS create
        let facade_created = facade.cas_set("fkey", Value::Null, Value::Int(1)).unwrap();

        // Substrate CAS create
        let substrate_created = substrate.cas_set("default", "skey", Value::Null, Value::Int(1)).unwrap();

        assert_eq!(facade_created, substrate_created);

        // Both should have value 1
        assert_eq!(facade.cas_get("fkey").unwrap(), Some(Value::Int(1)));
        assert_eq!(substrate.cas_get("default", "skey").unwrap(), Some(Value::Int(1)));
    }

    // =========================================================================
    // INVARIANT: Error propagation
    // =========================================================================

    #[test]
    fn parity_007_invalid_key_error() {
        let harness = TestHarness::new();
        let facade = harness.facade();
        let substrate = harness.substrate();

        // Empty key fails in both
        let facade_result = facade.set("", Value::Int(1));
        let substrate_result = substrate.kv_put("default", "", Value::Int(1));

        assert!(facade_result.is_err());
        assert!(substrate_result.is_err());

        // Same error code
        assert_eq!(
            facade_result.unwrap_err().error_code(),
            substrate_result.unwrap_err().error_code()
        );
    }

    #[test]
    fn parity_008_wrong_type_error() {
        let harness = TestHarness::new();
        let facade = harness.facade();
        let substrate = harness.substrate();

        // Setup with String
        facade.set("fkey", Value::String("hello".into())).unwrap();
        substrate.kv_put("default", "skey", Value::String("hello".into())).unwrap();

        // incr on String fails
        let facade_result = facade.incr("fkey");
        let substrate_result = substrate.kv_incr("default", "skey", 1);

        assert!(facade_result.is_err());
        assert!(substrate_result.is_err());

        assert_eq!(
            facade_result.unwrap_err().error_code(),
            substrate_result.unwrap_err().error_code()
        );
    }

    // =========================================================================
    // INVARIANT: Facade uses default run
    // =========================================================================

    #[test]
    fn parity_009_facade_uses_default_run() {
        let harness = TestHarness::new();
        let facade = harness.facade();
        let substrate = harness.substrate();

        // Facade set
        facade.set("key", Value::Int(42)).unwrap();

        // Must be in "default" run
        let result = substrate.kv_get("default", "key").unwrap();
        assert_eq!(result.unwrap().value, Value::Int(42));

        // Must NOT be in other runs
        substrate.create_run("other").unwrap();
        let other_result = substrate.kv_get("other", "key").unwrap();
        assert!(other_result.is_none());
    }
}
```

### Acceptance Criteria

- [ ] All PARITY-* tests pass
- [ ] Every facade operation has equivalent substrate behavior
- [ ] Error codes match between layers
- [ ] Facade uses default run exclusively

---

## Story #595: Determinism Verification Tests

**File**: `crates/tests/src/validation/determinism.rs` (NEW)

**Deliverable**: Tests proving determinism guarantee

### Tests

```rust
//! Determinism Verification Tests
//!
//! These tests validate the core determinism guarantee:
//! Same operations → Same state
//!
//! DO NOT MODIFY THESE TESTS TO MAKE THEM PASS.

#[cfg(test)]
mod determinism_validation {
    use strata_api::Substrate;
    use strata_core::value::Value;
    use strata_testing::TestHarness;

    #[test]
    fn det_001_same_ops_same_state() {
        // Two independent harnesses
        let h1 = TestHarness::new();
        let h2 = TestHarness::new();

        let s1 = h1.substrate();
        let s2 = h2.substrate();

        // Same sequence of operations
        let ops = vec![
            ("set", "a", Value::Int(1)),
            ("set", "b", Value::Int(2)),
            ("set", "a", Value::Int(3)),
            ("del", "b", Value::Null),
            ("set", "c", Value::String("hello".into())),
        ];

        for (op, key, value) in &ops {
            match *op {
                "set" => {
                    s1.kv_put("default", key, value.clone()).unwrap();
                    s2.kv_put("default", key, value.clone()).unwrap();
                }
                "del" => {
                    s1.kv_delete("default", key).unwrap();
                    s2.kv_delete("default", key).unwrap();
                }
                _ => panic!("Unknown op"),
            }
        }

        // State must be identical
        for key in ["a", "b", "c"] {
            let v1 = s1.kv_get("default", key).unwrap();
            let v2 = s2.kv_get("default", key).unwrap();
            assert_eq!(v1.map(|v| v.value), v2.map(|v| v.value),
                "State for key '{}' must be identical", key);
        }
    }

    #[test]
    fn det_002_operation_order_matters() {
        let h1 = TestHarness::new();
        let h2 = TestHarness::new();

        let s1 = h1.substrate();
        let s2 = h2.substrate();

        // Different order
        s1.kv_put("default", "key", Value::Int(1)).unwrap();
        s1.kv_put("default", "key", Value::Int(2)).unwrap();

        s2.kv_put("default", "key", Value::Int(2)).unwrap();
        s2.kv_put("default", "key", Value::Int(1)).unwrap();

        // Final state differs
        let v1 = s1.kv_get("default", "key").unwrap().unwrap().value;
        let v2 = s2.kv_get("default", "key").unwrap().unwrap().value;

        assert_eq!(v1, Value::Int(2));
        assert_eq!(v2, Value::Int(1));
    }

    #[test]
    fn det_003_no_external_state() {
        // Operations should not depend on external state like
        // current time, random numbers, etc.

        let h1 = TestHarness::new();
        let h2 = TestHarness::new();

        let s1 = h1.substrate();
        let s2 = h2.substrate();

        // Same operation
        s1.kv_put("default", "key", Value::Int(42)).unwrap();
        s2.kv_put("default", "key", Value::Int(42)).unwrap();

        // Version values should be deterministic
        let v1 = s1.kv_get("default", "key").unwrap().unwrap();
        let v2 = s2.kv_get("default", "key").unwrap().unwrap();

        // Values must match
        assert_eq!(v1.value, v2.value);

        // Note: Timestamps may differ (wall clock), but that doesn't affect logical state
    }

    #[test]
    fn det_004_cas_deterministic() {
        let h1 = TestHarness::new();
        let h2 = TestHarness::new();

        let s1 = h1.substrate();
        let s2 = h2.substrate();

        // Same CAS sequence
        s1.cas_set("default", "key", Value::Null, Value::Int(1)).unwrap();
        s2.cas_set("default", "key", Value::Null, Value::Int(1)).unwrap();

        s1.cas_set("default", "key", Value::Int(1), Value::Int(2)).unwrap();
        s2.cas_set("default", "key", Value::Int(1), Value::Int(2)).unwrap();

        // Results must match
        let v1 = s1.cas_get("default", "key").unwrap();
        let v2 = s2.cas_get("default", "key").unwrap();

        assert_eq!(v1, v2);
    }
}
```

### Acceptance Criteria

- [ ] All DET-* tests pass
- [ ] Same operations produce same state
- [ ] No dependency on external state
- [ ] CAS operations are deterministic

---

## Validation Suite Summary

This epic contains the M11a exit gate tests:

| Category | Test Count | Status |
|----------|------------|--------|
| Value Model Invariants | 16 | Must pass |
| Wire Encoding Round-Trip | 18 | Must pass |
| Facade-Substrate Parity | 9 | Must pass |
| Determinism Verification | 4 | Must pass |

**Total**: 47 critical validation tests

**Exit Criteria**: ALL 47 tests must pass before M11b begins.

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/tests/src/validation/mod.rs` | CREATE - Validation module |
| `crates/tests/src/validation/value_model.rs` | CREATE - Value model tests |
| `crates/tests/src/validation/wire_encoding.rs` | CREATE - Wire encoding tests |
| `crates/tests/src/validation/parity.rs` | CREATE - Parity tests |
| `crates/tests/src/validation/determinism.rs` | CREATE - Determinism tests |
| `crates/tests/Cargo.toml` | MODIFY - Add validation tests |

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-21 | Initial epic specification |
