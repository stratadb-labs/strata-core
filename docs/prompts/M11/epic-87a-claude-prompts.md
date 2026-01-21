# Epic 87a: Core Validation Suite - Implementation Prompts

**Epic Goal**: Validate core contract guarantees (M11a Exit Gate)

**GitHub Issue**: [#586](https://github.com/anibjoshi/in-mem/issues/586)
**Status**: Ready after Epics 80-84
**Dependencies**: Epics 80, 81, 82, 83, 84
**Phase**: 4 (Core Validation - M11a Exit Gate)

---

## NAMING CONVENTION - CRITICAL

> **NEVER use "M11" in the actual codebase or comments.**
>
> - "Strata" IS allowed (e.g., `strata_validation`, `StrataContract`)
>
> **CORRECT**: `//! Core contract validation tests`
> **WRONG**: `//! M11a validation suite`

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

Before starting ANY story in this epic, read:
1. **Contract Spec**: `docs/milestones/M11/M11_CONTRACT.md`
2. **Testing Plan**: `docs/milestones/M11/M11_TESTING_PLAN.md`
3. **Prompt Header**: `docs/prompts/M11/M11_PROMPT_HEADER.md`

---

## Epic 87a Overview

### Purpose

Epic 87a is the **M11a Exit Gate**. All M11a contract guarantees must be validated before proceeding to M11b (CLI, SDK).

### Scope
- Facade-Substrate parity tests
- Value round-trip tests
- Wire encoding conformance tests
- Determinism verification tests

### What This Epic Validates

1. **Facade-Substrate Parity**: Every facade operation produces same result as desugared substrate
2. **Value Round-Trip**: All 8 types survive encode/decode
3. **Wire Encoding**: $bytes, $f64, $absent work correctly
4. **Determinism**: Same operations produce same state

### M11a Exit Criteria (ALL MUST PASS)

- [ ] Facade-Substrate parity: 100% coverage
- [ ] Value round-trip: all 8 types pass
- [ ] Float edge cases: NaN, +Inf, -Inf, -0.0 preserved
- [ ] Bytes vs String distinction preserved
- [ ] $absent distinguishes missing from null
- [ ] All 12 error codes produce correct wire shape
- [ ] Determinism verified
- [ ] No type coercion anywhere

### Component Breakdown
- **Story #587**: Facade-Substrate Parity Tests
- **Story #588**: Value Round-Trip Tests
- **Story #589**: Wire Encoding Conformance Tests
- **Story #590**: Determinism Verification Tests

---

## Story #587: Facade-Substrate Parity Tests

**GitHub Issue**: [#587](https://github.com/anibjoshi/in-mem/issues/587)
**Dependencies**: Epics 81, 82
**Blocks**: M11a completion

### Start Story

```bash
./scripts/start-story.sh 87 587 facade-substrate-parity
```

### Key Implementation Points

Every facade operation must produce the same result as calling the equivalent substrate operations directly.

```rust
#[cfg(test)]
mod parity_tests {
    use super::*;

    #[test]
    fn parity_set_get() {
        let harness = TestHarness::new();
        let facade = harness.facade();
        let substrate = harness.substrate();

        // Facade: set then get
        facade.set("key", Value::Int(42)).unwrap();
        let facade_result = facade.get("key").unwrap();

        // Reset
        let harness2 = TestHarness::new();
        let substrate2 = harness2.substrate();

        // Substrate: kv_put then kv_get
        substrate2.kv_put(&DEFAULT_RUN, "key", Value::Int(42)).unwrap();
        let substrate_result = substrate2.kv_get(&DEFAULT_RUN, "key")
            .unwrap()
            .map(|v| v.value);

        assert_eq!(facade_result, substrate_result);
    }

    #[test]
    fn parity_mset_mget() {
        // facade.mset([("a", 1), ("b", 2)])
        // == substrate.kv_put("a", 1); substrate.kv_put("b", 2)

        // Both should be atomic
    }

    #[test]
    fn parity_delete_count() {
        // facade.delete(["a", "b", "c"]) returns count
        // == loop over substrate.kv_delete, count true returns
    }

    #[test]
    fn parity_incr() {
        // facade.incr("counter", 5)
        // == substrate.kv_incr(DEFAULT_RUN, "counter", 5)
    }

    #[test]
    fn parity_json_operations() {
        // facade.json_set("doc", "$.x", 1)
        // == substrate.json_set(DEFAULT_RUN, "doc", "$.x", 1)
    }

    #[test]
    fn parity_xadd() {
        // facade.xadd("stream", payload) returns Version
        // == substrate.event_append(DEFAULT_RUN, "stream", payload)
    }

    #[test]
    fn parity_cas() {
        // facade.cas_set("k", expected, new)
        // == substrate.state_cas(DEFAULT_RUN, "k", expected, new)
    }
}
```

### Acceptance Criteria

- [ ] Every facade operation tested for parity
- [ ] Return values match
- [ ] Side effects match
- [ ] Error conditions match

---

## Story #588: Value Round-Trip Tests

**GitHub Issue**: [#588](https://github.com/anibjoshi/in-mem/issues/588)
**Dependencies**: Epics 80, 83
**Blocks**: M11a completion

### Key Implementation Points

```rust
#[cfg(test)]
mod roundtrip_tests {
    use super::*;

    fn roundtrip(value: Value) -> Value {
        let json = encode_json(&value);
        decode_json(&json).unwrap()
    }

    #[test]
    fn roundtrip_null() {
        assert_eq!(roundtrip(Value::Null), Value::Null);
    }

    #[test]
    fn roundtrip_bool() {
        assert_eq!(roundtrip(Value::Bool(true)), Value::Bool(true));
        assert_eq!(roundtrip(Value::Bool(false)), Value::Bool(false));
    }

    #[test]
    fn roundtrip_int() {
        assert_eq!(roundtrip(Value::Int(0)), Value::Int(0));
        assert_eq!(roundtrip(Value::Int(i64::MAX)), Value::Int(i64::MAX));
        assert_eq!(roundtrip(Value::Int(i64::MIN)), Value::Int(i64::MIN));
    }

    #[test]
    fn roundtrip_float_normal() {
        assert_eq!(roundtrip(Value::Float(1.5)), Value::Float(1.5));
        assert_eq!(roundtrip(Value::Float(-2.5)), Value::Float(-2.5));
    }

    #[test]
    fn roundtrip_float_nan() {
        let result = roundtrip(Value::Float(f64::NAN));
        match result {
            Value::Float(f) => assert!(f.is_nan()),
            _ => panic!("Expected Float"),
        }
    }

    #[test]
    fn roundtrip_float_infinity() {
        assert_eq!(roundtrip(Value::Float(f64::INFINITY)), Value::Float(f64::INFINITY));
        assert_eq!(roundtrip(Value::Float(f64::NEG_INFINITY)), Value::Float(f64::NEG_INFINITY));
    }

    #[test]
    fn roundtrip_float_negative_zero() {
        let result = roundtrip(Value::Float(-0.0));
        match result {
            Value::Float(f) => {
                assert_eq!(f, 0.0);
                assert!(f.is_sign_negative(), "Sign must be preserved");
            }
            _ => panic!("Expected Float"),
        }
    }

    #[test]
    fn roundtrip_string() {
        assert_eq!(
            roundtrip(Value::String("hello".into())),
            Value::String("hello".into())
        );
        assert_eq!(
            roundtrip(Value::String("こんにちは".into())),
            Value::String("こんにちは".into())
        );
    }

    #[test]
    fn roundtrip_bytes() {
        let bytes = vec![0, 1, 255, 128];
        assert_eq!(
            roundtrip(Value::Bytes(bytes.clone())),
            Value::Bytes(bytes)
        );
    }

    #[test]
    fn roundtrip_bytes_vs_string_distinct() {
        // CRITICAL: Bytes and String with same content are different
        let bytes = Value::Bytes(b"abc".to_vec());
        let string = Value::String("abc".into());

        let bytes_rt = roundtrip(bytes.clone());
        let string_rt = roundtrip(string.clone());

        assert!(matches!(bytes_rt, Value::Bytes(_)));
        assert!(matches!(string_rt, Value::String(_)));
        assert_ne!(bytes_rt, string_rt);
    }

    #[test]
    fn roundtrip_array() {
        let arr = Value::Array(vec![
            Value::Int(1),
            Value::String("two".into()),
            Value::Bool(false),
        ]);
        assert_eq!(roundtrip(arr.clone()), arr);
    }

    #[test]
    fn roundtrip_object() {
        let mut map = HashMap::new();
        map.insert("a".to_string(), Value::Int(1));
        map.insert("b".to_string(), Value::String("two".into()));
        let obj = Value::Object(map);
        assert_eq!(roundtrip(obj.clone()), obj);
    }

    #[test]
    fn roundtrip_nested() {
        let mut inner = HashMap::new();
        inner.insert("nested".to_string(), Value::Array(vec![Value::Int(1)]));
        let value = Value::Object(inner);
        assert_eq!(roundtrip(value.clone()), value);
    }
}
```

### Acceptance Criteria

- [ ] All 8 types round-trip correctly
- [ ] Float edge cases preserved (NaN, Inf, -0.0)
- [ ] Bytes vs String distinction preserved
- [ ] Nested structures work
- [ ] Full i64 range preserved

---

## Story #589: Wire Encoding Conformance Tests

**GitHub Issue**: [#589](https://github.com/anibjoshi/in-mem/issues/589)
**Dependencies**: Epic 83
**Blocks**: M11a completion

### Key Implementation Points

```rust
#[cfg(test)]
mod wire_conformance_tests {
    use super::*;

    // === $bytes wrapper ===

    #[test]
    fn wire_bytes_uses_wrapper() {
        let value = Value::Bytes(vec![72, 101, 108, 108, 111]); // "Hello"
        let json = encode_json(&value);
        assert!(json.contains("$bytes"));
        assert!(json.contains("SGVsbG8=")); // base64 of "Hello"
    }

    #[test]
    fn wire_bytes_never_plain_array() {
        let value = Value::Bytes(vec![1, 2, 3]);
        let json = encode_json(&value);
        assert!(!json.starts_with('['));
        assert!(json.contains("$bytes"));
    }

    // === $f64 wrapper ===

    #[test]
    fn wire_nan_uses_wrapper() {
        let json = encode_json(&Value::Float(f64::NAN));
        assert_eq!(json, r#"{"$f64":"NaN"}"#);
    }

    #[test]
    fn wire_infinity_uses_wrapper() {
        assert_eq!(encode_json(&Value::Float(f64::INFINITY)), r#"{"$f64":"+Inf"}"#);
        assert_eq!(encode_json(&Value::Float(f64::NEG_INFINITY)), r#"{"$f64":"-Inf"}"#);
    }

    #[test]
    fn wire_negative_zero_uses_wrapper() {
        let json = encode_json(&Value::Float(-0.0));
        assert_eq!(json, r#"{"$f64":"-0.0"}"#);
    }

    #[test]
    fn wire_normal_float_no_wrapper() {
        let json = encode_json(&Value::Float(1.5));
        assert!(!json.contains("$f64"));
        assert_eq!(json, "1.5");
    }

    // === $absent wrapper ===

    #[test]
    fn wire_absent_distinct_from_null() {
        let absent = encode_absent();
        let null = encode_json(&Value::Null);

        assert_ne!(absent, null);
        assert!(absent.contains("$absent"));
    }

    #[test]
    fn wire_absent_for_cas() {
        // When expected=$absent, CAS means "create if not exists"
        // This is different from expected=null
    }

    // === Error wire shape ===

    #[test]
    fn wire_error_has_code_message_details() {
        let err = StrataError::NotFound { key: "mykey".into() };
        let wire: WireError = (&err).into();

        assert_eq!(wire.code, "NotFound");
        assert!(wire.message.contains("mykey"));
        assert!(wire.details.is_some());
    }

    #[test]
    fn wire_all_error_codes_valid() {
        // Verify all 12 error codes produce valid wire format
    }
}
```

### Acceptance Criteria

- [ ] $bytes always used for Bytes values
- [ ] $f64 used for NaN, Inf, -0.0 only
- [ ] $absent distinct from null
- [ ] Error wire shape: {code, message, details}
- [ ] All 12 error codes encode correctly

---

## Story #590: Determinism Verification Tests

**GitHub Issue**: [#590](https://github.com/anibjoshi/in-mem/issues/590)
**Dependencies**: Epics 80-84
**Blocks**: M11a completion

### Key Implementation Points

```rust
#[cfg(test)]
mod determinism_tests {
    use super::*;

    #[test]
    fn same_ops_same_state() {
        // Two independent instances with same operations should have identical state
        let harness1 = TestHarness::new();
        let harness2 = TestHarness::new();

        let ops = vec![
            ("set", "a", Value::Int(1)),
            ("set", "b", Value::Int(2)),
            ("set", "a", Value::Int(3)),
        ];

        for (op, key, value) in &ops {
            harness1.facade().set(key, value.clone()).unwrap();
            harness2.facade().set(key, value.clone()).unwrap();
        }

        // Final states should match
        assert_eq!(harness1.facade().get("a"), harness2.facade().get("a"));
        assert_eq!(harness1.facade().get("b"), harness2.facade().get("b"));
    }

    #[test]
    fn order_matters() {
        // Different order produces different state
        let harness1 = TestHarness::new();
        let harness2 = TestHarness::new();

        harness1.facade().set("k", Value::Int(1)).unwrap();
        harness1.facade().set("k", Value::Int(2)).unwrap();

        harness2.facade().set("k", Value::Int(2)).unwrap();
        harness2.facade().set("k", Value::Int(1)).unwrap();

        // Final values differ
        assert_eq!(harness1.facade().get("k").unwrap(), Some(Value::Int(2)));
        assert_eq!(harness2.facade().get("k").unwrap(), Some(Value::Int(1)));
    }

    #[test]
    fn timestamp_independence() {
        // Operations at different times produce same logical state
        // (timestamps are metadata, not operation inputs)
    }

    #[test]
    fn value_equality_is_deterministic() {
        // Value comparison is pure function
        let v1 = Value::Int(42);
        let v2 = Value::Int(42);
        let v3 = Value::Int(42);

        // Reflexive, symmetric, transitive
        assert_eq!(v1, v1);
        assert_eq!(v1, v2);
        assert_eq!(v2, v1);
        assert_eq!(v1, v3);
    }

    #[test]
    fn float_equality_is_ieee754() {
        // IEEE-754 rules are deterministic
        assert_ne!(Value::Float(f64::NAN), Value::Float(f64::NAN));
        assert_eq!(Value::Float(-0.0), Value::Float(0.0));
    }
}
```

### Acceptance Criteria

- [ ] Same operations produce same state
- [ ] Order matters (different order = different state)
- [ ] Timestamps don't affect logical state
- [ ] Value equality is deterministic
- [ ] Float equality follows IEEE-754

---

## Epic 87a Completion = M11a Exit Gate

### Final Validation

```bash
# Run ALL M11a tests
~/.cargo/bin/cargo test --test m11_comprehensive

# Run specific validation suites
~/.cargo/bin/cargo test parity_ -- --nocapture
~/.cargo/bin/cargo test roundtrip_ -- --nocapture
~/.cargo/bin/cargo test wire_conformance_ -- --nocapture
~/.cargo/bin/cargo test determinism_ -- --nocapture

# Run no-coercion tests (CRITICAL)
~/.cargo/bin/cargo test nc_ -- --nocapture
```

### M11a Exit Gate Checklist

**ALL must pass before proceeding to M11b:**

- [ ] All facade-substrate parity tests pass
- [ ] All value round-trip tests pass
- [ ] All wire encoding conformance tests pass
- [ ] All determinism tests pass
- [ ] All no-coercion tests pass (nc_*)
- [ ] All error model tests pass
- [ ] Zero test failures

### Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-87a-core-validation -m "Epic 87a: Core Validation Suite complete - M11a EXIT GATE PASSED

Validated:
- Facade-Substrate parity (100% coverage)
- Value round-trip (all 8 types)
- Float edge cases (NaN, Inf, -0.0)
- Wire encoding conformance ($bytes, $f64, $absent)
- Error model (12 codes, wire shape)
- Determinism (same ops = same state)
- No type coercion verified

M11a Core Contract: COMPLETE AND VALIDATED

Stories: #587, #588, #589, #590
"
git push origin develop
gh issue close 586 --comment "Epic 87a: Core Validation Suite - M11a EXIT GATE PASSED"
```

---

## Summary

Epic 87a is the **M11a Exit Gate**:

- **Facade-Substrate Parity**: Every facade op equals substrate equivalent
- **Value Round-Trip**: All types survive encode/decode
- **Wire Conformance**: Wrappers work correctly
- **Determinism**: Same operations = same state
- **No Coercion**: Type distinction preserved everywhere

**After Epic 87a passes, M11a is COMPLETE. Proceed to M11b (CLI, SDK).**
