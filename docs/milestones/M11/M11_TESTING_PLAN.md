# M11 Testing Plan: Public API & SDK Contract

**Status**: Active
**Version**: 2.0
**Last Updated**: 2026-01-21
**Criticality**: HIGHEST - This contract becomes permanent after M11

---

## Executive Summary

This document defines the comprehensive testing strategy for M11 (Public API & SDK Contract). Because M11 freezes the public contract that all downstream surfaces depend on, **any bug that escapes testing becomes a permanent liability**. This testing plan is designed to achieve zero-defect quality for all frozen contract elements.

### Milestone Split

M11 is split into two sub-milestones with distinct validation gates:

| Milestone | Scope | Exit Gate |
|-----------|-------|-----------|
| **M11a** | Core Contract & API (Value Model, Wire Encoding, Error Model, Facade API, Substrate API) | Core Validation Suite passes (Epic 87a) |
| **M11b** | Consumer Surfaces (CLI, SDK Foundation) | Surface Validation Suite passes (Epic 87b) |

**Critical**: M11a must be fully validated before M11b work begins. Any defects in the core contract discovered during M11b require fixing and re-validating M11a.

### Testing Philosophy

1. **Contract First**: Test the contract, not the implementation
2. **Exhaustive Edge Cases**: Every boundary condition must be tested
3. **Round-Trip Verification**: All encodings must survive round-trip
4. **Determinism Proof**: Same inputs must produce identical outputs
5. **Negative Testing**: Invalid inputs must produce correct errors
6. **Cross-Surface Parity**: Facade, Substrate, Wire, CLI must be consistent
7. **Gated Validation**: M11a complete before M11b begins

### Test Categories

| Category | Purpose | Coverage Target | Milestone |
|----------|---------|-----------------|-----------|
| Unit Tests | Individual component correctness | 100% of contract elements | M11a/M11b |
| Integration Tests | Cross-component interaction | All API flows | M11a/M11b |
| Contract Tests | Invariant verification | All documented invariants | M11a |
| Fuzz Tests | Unexpected input handling | Value model, wire encoding | M11a |
| Property Tests | Mathematical properties | Equality, encoding | M11a |
| Regression Tests | Prevent contract breakage | All frozen elements | M11a/M11b |
| Conformance Tests | SDK parity | All SDK mappings | M11b |
| CLI Tests | CLI surface coverage | All CLI commands | M11b |

---

## Table of Contents

### M11a Test Suites (Core Contract & API)

1. [Value Model Tests](#1-value-model-tests)
2. [Wire Encoding Tests](#2-wire-encoding-tests)
3. [Facade API Tests](#3-facade-api-tests)
4. [Substrate API Tests](#4-substrate-api-tests)
5. [Facade→Substrate Desugaring Tests](#5-facadesubstrate-desugaring-tests)
6. [Error Model Tests](#6-error-model-tests)
8. [Versioned<T> Tests](#8-versionedt-tests)
9. [Run Semantics Tests](#9-run-semantics-tests)
10. [Transaction Semantics Tests](#10-transaction-semantics-tests)
11. [History & Retention Tests](#11-history--retention-tests)
12. [Determinism Tests](#12-determinism-tests)
13. [Contract Stability Tests](#13-contract-stability-tests) (Core elements)
14. [Fuzz Testing Strategy](#14-fuzz-testing-strategy)
15. [Test Data Generators](#15-test-data-generators)
16. [Test Infrastructure](#16-test-infrastructure)

### M11b Test Suites (Consumer Surfaces)

7. [CLI Tests](#7-cli-tests)
17. [SDK Conformance Tests](#17-sdk-conformance-tests)
13. [Contract Stability Tests](#13-contract-stability-tests) (Surface elements)

---

# Part I: M11a Test Suites (Core Contract & API)

> **Scope**: These test suites validate the core contract that all downstream surfaces depend on. M11a tests must all pass before M11b work begins.

---

## 1. Value Model Tests

The value model is the foundation of the entire contract. Every test in this section is **critical**.

### 1.1 Type Construction Tests

**Test Suite**: `value_model::construction`

| Test ID | Test Name | Description | Priority |
|---------|-----------|-------------|----------|
| VAL-001 | `null_construction` | `Value::Null` constructs correctly | CRITICAL |
| VAL-002 | `bool_true_construction` | `Value::Bool(true)` constructs correctly | CRITICAL |
| VAL-003 | `bool_false_construction` | `Value::Bool(false)` constructs correctly | CRITICAL |
| VAL-004 | `int_positive_construction` | `Value::Int(123)` constructs correctly | CRITICAL |
| VAL-005 | `int_negative_construction` | `Value::Int(-456)` constructs correctly | CRITICAL |
| VAL-006 | `int_zero_construction` | `Value::Int(0)` constructs correctly | CRITICAL |
| VAL-007 | `int_max_construction` | `Value::Int(i64::MAX)` constructs correctly | CRITICAL |
| VAL-008 | `int_min_construction` | `Value::Int(i64::MIN)` constructs correctly | CRITICAL |
| VAL-009 | `float_positive_construction` | `Value::Float(1.23)` constructs correctly | CRITICAL |
| VAL-010 | `float_negative_construction` | `Value::Float(-4.56)` constructs correctly | CRITICAL |
| VAL-011 | `float_zero_construction` | `Value::Float(0.0)` constructs correctly | CRITICAL |
| VAL-012 | `float_negative_zero_construction` | `Value::Float(-0.0)` constructs correctly | CRITICAL |
| VAL-013 | `float_nan_construction` | `Value::Float(f64::NAN)` constructs correctly | CRITICAL |
| VAL-014 | `float_positive_infinity_construction` | `Value::Float(f64::INFINITY)` constructs correctly | CRITICAL |
| VAL-015 | `float_negative_infinity_construction` | `Value::Float(f64::NEG_INFINITY)` constructs correctly | CRITICAL |
| VAL-016 | `float_max_construction` | `Value::Float(f64::MAX)` constructs correctly | CRITICAL |
| VAL-017 | `float_min_positive_construction` | `Value::Float(f64::MIN_POSITIVE)` constructs correctly | CRITICAL |
| VAL-018 | `float_subnormal_construction` | Subnormal floats construct correctly | HIGH |
| VAL-019 | `string_empty_construction` | `Value::String("")` constructs correctly | CRITICAL |
| VAL-020 | `string_ascii_construction` | `Value::String("hello")` constructs correctly | CRITICAL |
| VAL-021 | `string_unicode_construction` | `Value::String("こんにちは")` constructs correctly | CRITICAL |
| VAL-022 | `string_emoji_construction` | `Value::String("🚀🎉")` constructs correctly | CRITICAL |
| VAL-023 | `string_surrogate_pairs_construction` | Strings with surrogate pairs construct correctly | HIGH |
| VAL-024 | `bytes_empty_construction` | `Value::Bytes(vec![])` constructs correctly | CRITICAL |
| VAL-025 | `bytes_binary_construction` | `Value::Bytes(vec![0, 255, 128])` constructs correctly | CRITICAL |
| VAL-026 | `bytes_all_values_construction` | `Value::Bytes(0..=255)` constructs correctly | CRITICAL |
| VAL-027 | `array_empty_construction` | `Value::Array(vec![])` constructs correctly | CRITICAL |
| VAL-028 | `array_single_element_construction` | `Value::Array(vec![Value::Int(1)])` constructs correctly | CRITICAL |
| VAL-029 | `array_mixed_types_construction` | Array with mixed value types constructs correctly | CRITICAL |
| VAL-030 | `array_nested_construction` | Nested arrays construct correctly | CRITICAL |
| VAL-031 | `object_empty_construction` | `Value::Object(HashMap::new())` constructs correctly | CRITICAL |
| VAL-032 | `object_single_entry_construction` | Object with single entry constructs correctly | CRITICAL |
| VAL-033 | `object_multiple_entries_construction` | Object with multiple entries constructs correctly | CRITICAL |
| VAL-034 | `object_nested_construction` | Nested objects construct correctly | CRITICAL |
| VAL-035 | `deeply_nested_construction` | Max nesting depth constructs correctly | CRITICAL |

### 1.2 Float Edge Case Tests

**Test Suite**: `value_model::float_edge_cases`

These tests are critical because float handling is a common source of bugs.

| Test ID | Test Name | Description | Assertion |
|---------|-----------|-------------|-----------|
| FLT-001 | `nan_is_nan` | NaN value is identified as NaN | `f64::NAN.is_nan() == true` |
| FLT-002 | `nan_not_equal_to_self` | NaN does not equal itself | `Value::Float(NaN) != Value::Float(NaN)` |
| FLT-003 | `nan_not_equal_to_other_nan` | Different NaN payloads are not equal | Multiple NaN values all unequal |
| FLT-004 | `positive_infinity_is_infinite` | +Inf is identified as infinite | `f64::INFINITY.is_infinite() == true` |
| FLT-005 | `negative_infinity_is_infinite` | -Inf is identified as infinite | `f64::NEG_INFINITY.is_infinite() == true` |
| FLT-006 | `positive_infinity_equals_self` | +Inf equals +Inf | `Value::Float(+Inf) == Value::Float(+Inf)` |
| FLT-007 | `negative_infinity_equals_self` | -Inf equals -Inf | `Value::Float(-Inf) == Value::Float(-Inf)` |
| FLT-008 | `positive_negative_infinity_not_equal` | +Inf != -Inf | `Value::Float(+Inf) != Value::Float(-Inf)` |
| FLT-009 | `negative_zero_equals_positive_zero` | -0.0 == 0.0 (IEEE-754) | `Value::Float(-0.0) == Value::Float(0.0)` |
| FLT-010 | `negative_zero_preserved_in_storage` | -0.0 is preserved, not normalized | Storage preserves -0.0 bit pattern |
| FLT-011 | `negative_zero_preserved_on_wire` | -0.0 uses $f64 wrapper | Wire encoding preserves -0.0 |
| FLT-012 | `subnormal_values_preserved` | Subnormal floats are not flushed to zero | Subnormals round-trip correctly |
| FLT-013 | `max_float_preserved` | f64::MAX survives round-trip | No overflow/truncation |
| FLT-014 | `min_positive_float_preserved` | f64::MIN_POSITIVE survives round-trip | No underflow |
| FLT-015 | `float_precision_preserved` | Full f64 precision preserved | `1.0000000000000002` round-trips |

**Property-Based Float Tests**:

```rust
#[quickcheck]
fn prop_float_roundtrip(f: f64) -> bool {
    let value = Value::Float(f);
    let encoded = encode_json(&value);
    let decoded = decode_json(&encoded);

    if f.is_nan() {
        // NaN should round-trip to some NaN
        matches!(decoded, Value::Float(x) if x.is_nan())
    } else {
        decoded == value
    }
}

#[quickcheck]
fn prop_float_equality_ieee754(a: f64, b: f64) -> bool {
    let va = Value::Float(a);
    let vb = Value::Float(b);

    // Value equality must match IEEE-754 equality
    (va == vb) == (a == b)
}
```

### 1.3 Value Equality Tests

**Test Suite**: `value_model::equality`

| Test ID | Test Name | Description | Expected Result |
|---------|-----------|-------------|-----------------|
| EQ-001 | `null_equals_null` | Null == Null | true |
| EQ-002 | `null_not_equals_bool` | Null != Bool | true |
| EQ-003 | `null_not_equals_int_zero` | Null != Int(0) | true |
| EQ-004 | `bool_true_equals_true` | Bool(true) == Bool(true) | true |
| EQ-005 | `bool_false_equals_false` | Bool(false) == Bool(false) | true |
| EQ-006 | `bool_true_not_equals_false` | Bool(true) != Bool(false) | true |
| EQ-007 | `bool_not_equals_int_one` | Bool(true) != Int(1) | true |
| EQ-008 | `int_equals_same_int` | Int(42) == Int(42) | true |
| EQ-009 | `int_not_equals_different_int` | Int(42) != Int(43) | true |
| EQ-010 | `int_not_equals_float` | Int(1) != Float(1.0) | **CRITICAL: true** |
| EQ-011 | `int_zero_not_equals_float_zero` | Int(0) != Float(0.0) | **CRITICAL: true** |
| EQ-012 | `float_equals_same_float` | Float(3.14) == Float(3.14) | true |
| EQ-013 | `float_nan_not_equals_nan` | Float(NaN) != Float(NaN) | **CRITICAL: true** |
| EQ-014 | `float_negative_zero_equals_zero` | Float(-0.0) == Float(0.0) | true |
| EQ-015 | `string_equals_same_string` | String("a") == String("a") | true |
| EQ-016 | `string_not_equals_different_string` | String("a") != String("b") | true |
| EQ-017 | `string_empty_equals_empty` | String("") == String("") | true |
| EQ-018 | `string_not_equals_bytes` | String("abc") != Bytes([97,98,99]) | **CRITICAL: true** |
| EQ-019 | `bytes_equals_same_bytes` | Bytes([1,2]) == Bytes([1,2]) | true |
| EQ-020 | `bytes_not_equals_different_bytes` | Bytes([1,2]) != Bytes([1,3]) | true |
| EQ-021 | `bytes_empty_equals_empty` | Bytes([]) == Bytes([]) | true |
| EQ-022 | `array_equals_same_elements` | [1,2] == [1,2] | true |
| EQ-023 | `array_not_equals_different_order` | [1,2] != [2,1] | true |
| EQ-024 | `array_not_equals_different_length` | [1,2] != [1,2,3] | true |
| EQ-025 | `array_recursive_equality` | [[1]] == [[1]] | true |
| EQ-026 | `object_equals_same_entries` | {a:1} == {a:1} | true |
| EQ-027 | `object_equals_regardless_of_insertion_order` | {a:1,b:2} == {b:2,a:1} | true |
| EQ-028 | `object_not_equals_different_keys` | {a:1} != {b:1} | true |
| EQ-029 | `object_not_equals_different_values` | {a:1} != {a:2} | true |
| EQ-030 | `object_recursive_equality` | {a:{b:1}} == {a:{b:1}} | true |

### 1.4 No Type Coercion Tests

**Test Suite**: `value_model::no_coercion`

These tests verify Rule 3: No Type Coercion.

| Test ID | Test Name | Description | Must Assert |
|---------|-----------|-------------|-------------|
| NC-001 | `int_one_not_float_one` | Int(1) != Float(1.0) | Strict inequality |
| NC-002 | `int_zero_not_float_zero` | Int(0) != Float(0.0) | Strict inequality |
| NC-003 | `int_max_not_float` | Int(i64::MAX) != Float(i64::MAX as f64) | Strict inequality |
| NC-004 | `string_not_bytes` | String("abc") != Bytes(b"abc") | Strict inequality |
| NC-005 | `null_not_empty_string` | Null != String("") | Strict inequality |
| NC-006 | `null_not_zero` | Null != Int(0) | Strict inequality |
| NC-007 | `null_not_false` | Null != Bool(false) | Strict inequality |
| NC-008 | `empty_array_not_null` | Array([]) != Null | Strict inequality |
| NC-009 | `empty_object_not_null` | Object({}) != Null | Strict inequality |
| NC-010 | `bool_true_not_int_one` | Bool(true) != Int(1) | Strict inequality |
| NC-011 | `bool_false_not_int_zero` | Bool(false) != Int(0) | Strict inequality |
| NC-012 | `string_number_not_int` | String("123") != Int(123) | Strict inequality |
| NC-013 | `no_implicit_string_to_bytes` | Cannot compare String to Bytes | Type error or false |
| NC-014 | `no_implicit_int_promotion` | No Int→Float promotion | Types preserved |
| NC-015 | `cas_respects_type_distinction` | CAS on Int(1) fails if value is Float(1.0) | CAS returns false |

### 1.5 Size Limits Tests

**Test Suite**: `value_model::size_limits`

| Test ID | Test Name | Limit | Test Cases | Expected Error |
|---------|-----------|-------|------------|----------------|
| SL-001 | `key_at_max_length` | max_key_bytes=1024 | 1024 byte key | Success |
| SL-002 | `key_exceeds_max_length` | max_key_bytes=1024 | 1025 byte key | `InvalidKey` |
| SL-003 | `key_much_larger_than_max` | max_key_bytes=1024 | 10KB key | `InvalidKey` |
| SL-004 | `string_at_max_length` | max_string_bytes=16MiB | 16MiB string | Success |
| SL-005 | `string_exceeds_max_length` | max_string_bytes=16MiB | 16MiB+1 string | `ConstraintViolation(value_too_large)` |
| SL-006 | `bytes_at_max_length` | max_bytes_len=16MiB | 16MiB bytes | Success |
| SL-007 | `bytes_exceeds_max_length` | max_bytes_len=16MiB | 16MiB+1 bytes | `ConstraintViolation(value_too_large)` |
| SL-008 | `value_encoded_at_max` | max_value_bytes_encoded=32MiB | 32MiB encoded | Success |
| SL-009 | `value_encoded_exceeds_max` | max_value_bytes_encoded=32MiB | 32MiB+1 encoded | `ConstraintViolation(value_too_large)` |
| SL-010 | `array_at_max_length` | max_array_len=1M | 1M elements | Success |
| SL-011 | `array_exceeds_max_length` | max_array_len=1M | 1M+1 elements | `ConstraintViolation(value_too_large)` |
| SL-012 | `object_at_max_entries` | max_object_entries=1M | 1M entries | Success |
| SL-013 | `object_exceeds_max_entries` | max_object_entries=1M | 1M+1 entries | `ConstraintViolation(value_too_large)` |
| SL-014 | `nesting_at_max_depth` | max_nesting_depth=128 | 128 levels | Success |
| SL-015 | `nesting_exceeds_max_depth` | max_nesting_depth=128 | 129 levels | `ConstraintViolation(nesting_too_deep)` |
| SL-016 | `vector_at_max_dim` | max_vector_dim=8192 | 8192 dimensions | Success |
| SL-017 | `vector_exceeds_max_dim` | max_vector_dim=8192 | 8193 dimensions | `ConstraintViolation(vector_dim_exceeded)` |
| SL-018 | `configurable_limits_respected` | Custom limits | Various | Limits enforced |

### 1.6 Key Validation Tests

**Test Suite**: `value_model::key_validation`

| Test ID | Test Name | Input | Expected Result |
|---------|-----------|-------|-----------------|
| KV-001 | `valid_simple_key` | `"mykey"` | Success |
| KV-002 | `valid_unicode_key` | `"日本語キー"` | Success |
| KV-003 | `valid_emoji_key` | `"🔑key🔑"` | Success |
| KV-004 | `valid_numeric_string_key` | `"12345"` | Success |
| KV-005 | `valid_special_chars_key` | `"a-b_c.d:e/f"` | Success |
| KV-006 | `invalid_empty_key` | `""` | `InvalidKey` |
| KV-007 | `invalid_nul_byte` | `"a\x00b"` | `InvalidKey` |
| KV-008 | `invalid_nul_at_start` | `"\x00abc"` | `InvalidKey` |
| KV-009 | `invalid_nul_at_end` | `"abc\x00"` | `InvalidKey` |
| KV-010 | `invalid_reserved_prefix` | `"_strata/foo"` | `InvalidKey` |
| KV-011 | `invalid_reserved_prefix_exact` | `"_strata/"` | `InvalidKey` |
| KV-012 | `valid_similar_to_reserved` | `"_stratafoo"` | Success (no slash) |
| KV-013 | `valid_underscore_prefix` | `"_mykey"` | Success |
| KV-014 | `invalid_utf8` | Invalid UTF-8 bytes | `InvalidKey` |
| KV-015 | `valid_at_max_length` | 1024 byte key | Success |
| KV-016 | `invalid_exceeds_max_length` | 1025 byte key | `InvalidKey` |
| KV-017 | `valid_single_char` | `"a"` | Success |
| KV-018 | `valid_single_byte` | `"x"` | Success |
| KV-019 | `valid_whitespace_key` | `"  spaces  "` | Success (allowed) |
| KV-020 | `valid_newline_key` | `"line1\nline2"` | Success (allowed) |

---

## 2. Wire Encoding Tests

Wire encoding must be lossless. Every value type must survive round-trip.

### 2.1 JSON Value Encoding Tests

**Test Suite**: `wire::json::value_encoding`

| Test ID | Test Name | Value | Expected JSON | Round-Trip |
|---------|-----------|-------|---------------|------------|
| JE-001 | `encode_null` | `Null` | `null` | ✓ |
| JE-002 | `encode_bool_true` | `Bool(true)` | `true` | ✓ |
| JE-003 | `encode_bool_false` | `Bool(false)` | `false` | ✓ |
| JE-004 | `encode_int_positive` | `Int(123)` | `123` | ✓ |
| JE-005 | `encode_int_negative` | `Int(-456)` | `-456` | ✓ |
| JE-006 | `encode_int_zero` | `Int(0)` | `0` | ✓ |
| JE-007 | `encode_int_max` | `Int(i64::MAX)` | `9223372036854775807` | ✓ |
| JE-008 | `encode_int_min` | `Int(i64::MIN)` | `-9223372036854775808` | ✓ |
| JE-009 | `encode_float_positive` | `Float(1.5)` | `1.5` | ✓ |
| JE-010 | `encode_float_negative` | `Float(-2.5)` | `-2.5` | ✓ |
| JE-011 | `encode_float_zero` | `Float(0.0)` | `0.0` | ✓ |
| JE-012 | `encode_float_negative_zero` | `Float(-0.0)` | `{"$f64":"-0.0"}` | ✓ |
| JE-013 | `encode_float_nan` | `Float(NaN)` | `{"$f64":"NaN"}` | ✓ |
| JE-014 | `encode_float_positive_infinity` | `Float(+Inf)` | `{"$f64":"+Inf"}` | ✓ |
| JE-015 | `encode_float_negative_infinity` | `Float(-Inf)` | `{"$f64":"-Inf"}` | ✓ |
| JE-016 | `encode_float_max` | `Float(f64::MAX)` | Scientific notation | ✓ |
| JE-017 | `encode_float_precision` | `Float(1.0000000000000002)` | Full precision | ✓ |
| JE-018 | `encode_string_simple` | `String("hello")` | `"hello"` | ✓ |
| JE-019 | `encode_string_empty` | `String("")` | `""` | ✓ |
| JE-020 | `encode_string_unicode` | `String("日本語")` | `"日本語"` | ✓ |
| JE-021 | `encode_string_escape_chars` | `String("a\n\t\"b")` | `"a\n\t\"b"` | ✓ |
| JE-022 | `encode_bytes_simple` | `Bytes([72,101,108,108,111])` | `{"$bytes":"SGVsbG8="}` | ✓ |
| JE-023 | `encode_bytes_empty` | `Bytes([])` | `{"$bytes":""}` | ✓ |
| JE-024 | `encode_bytes_all_values` | `Bytes(0..=255)` | Base64 encoded | ✓ |
| JE-025 | `encode_array_simple` | `Array([Int(1), Int(2)])` | `[1,2]` | ✓ |
| JE-026 | `encode_array_empty` | `Array([])` | `[]` | ✓ |
| JE-027 | `encode_array_nested` | `Array([Array([Int(1)])])` | `[[1]]` | ✓ |
| JE-028 | `encode_array_mixed_types` | `Array([Int(1), String("a")])` | `[1,"a"]` | ✓ |
| JE-029 | `encode_object_simple` | `Object({"a": Int(1)})` | `{"a":1}` | ✓ |
| JE-030 | `encode_object_empty` | `Object({})` | `{}` | ✓ |
| JE-031 | `encode_object_nested` | `Object({"a": Object({})})` | `{"a":{}}` | ✓ |

### 2.2 Special Wrapper Tests

**Test Suite**: `wire::json::wrappers`

| Test ID | Test Name | Description | Verification |
|---------|-----------|-------------|--------------|
| WR-001 | `bytes_wrapper_structure` | $bytes has correct structure | `{"$bytes": "<base64>"}` |
| WR-002 | `bytes_wrapper_base64_standard` | Uses standard base64 alphabet | No URL-safe chars |
| WR-003 | `bytes_wrapper_padding` | Base64 includes padding | `=` padding present |
| WR-004 | `f64_nan_wrapper` | NaN uses $f64 wrapper | `{"$f64":"NaN"}` |
| WR-005 | `f64_positive_inf_wrapper` | +Inf uses $f64 wrapper | `{"$f64":"+Inf"}` |
| WR-006 | `f64_negative_inf_wrapper` | -Inf uses $f64 wrapper | `{"$f64":"-Inf"}` |
| WR-007 | `f64_negative_zero_wrapper` | -0.0 uses $f64 wrapper | `{"$f64":"-0.0"}` |
| WR-008 | `f64_positive_zero_no_wrapper` | +0.0 is plain JSON | `0.0` (no wrapper) |
| WR-009 | `absent_wrapper_structure` | $absent has correct structure | `{"$absent":true}` |
| WR-010 | `absent_wrapper_value` | $absent value is boolean true | Not `1`, not `"true"` |
| WR-011 | `nested_bytes_in_object` | Bytes in object use wrapper | Object contains $bytes |
| WR-012 | `nested_bytes_in_array` | Bytes in array use wrapper | Array contains $bytes |
| WR-013 | `wrapper_collision_object` | Object with "$bytes" key | Distinguished from wrapper |
| WR-014 | `wrapper_collision_f64` | Object with "$f64" key | Distinguished from wrapper |
| WR-015 | `wrapper_collision_absent` | Object with "$absent" key | Distinguished from wrapper |

### 2.3 Request/Response Envelope Tests

**Test Suite**: `wire::json::envelope`

| Test ID | Test Name | Description | Expected Shape |
|---------|-----------|-------------|----------------|
| ENV-001 | `request_envelope_structure` | Request has id, op, params | All fields present |
| ENV-002 | `request_envelope_id_string` | ID is string | Type verified |
| ENV-003 | `request_envelope_op_string` | Op is string | Type verified |
| ENV-004 | `request_envelope_params_object` | Params is object | Type verified |
| ENV-005 | `success_response_structure` | Success has id, ok=true, result | All fields present |
| ENV-006 | `success_response_ok_true` | ok field is boolean true | Not 1, not "true" |
| ENV-007 | `error_response_structure` | Error has id, ok=false, error | All fields present |
| ENV-008 | `error_response_ok_false` | ok field is boolean false | Not 0, not "false" |
| ENV-009 | `error_response_error_structure` | error has code, message, details | All fields present |
| ENV-010 | `request_id_preserved` | Response ID matches request ID | ID round-trips |

### 2.4 Version Encoding Tests

**Test Suite**: `wire::json::version`

| Test ID | Test Name | Version | Expected JSON |
|---------|-----------|---------|---------------|
| VER-001 | `encode_txn_version` | `Txn(123)` | `{"type":"txn","value":123}` |
| VER-002 | `encode_sequence_version` | `Sequence(456)` | `{"type":"sequence","value":456}` |
| VER-003 | `encode_counter_version` | `Counter(789)` | `{"type":"counter","value":789}` |
| VER-004 | `encode_txn_zero` | `Txn(0)` | `{"type":"txn","value":0}` |
| VER-005 | `encode_txn_max` | `Txn(u64::MAX)` | Large number preserved |
| VER-006 | `decode_txn_version` | JSON → Txn | Correct type and value |
| VER-007 | `decode_sequence_version` | JSON → Sequence | Correct type and value |
| VER-008 | `decode_counter_version` | JSON → Counter | Correct type and value |
| VER-009 | `version_type_preserved` | Round-trip | Type tag preserved |
| VER-010 | `invalid_version_type` | `{"type":"invalid","value":1}` | Error |

### 2.5 Versioned<T> Encoding Tests

**Test Suite**: `wire::json::versioned`

| Test ID | Test Name | Description | Expected Shape |
|---------|-----------|-------------|----------------|
| VSD-001 | `versioned_structure` | Versioned has value, version, timestamp | All fields present |
| VSD-002 | `versioned_value_correct` | Value field is the actual value | Correct encoding |
| VSD-003 | `versioned_version_correct` | Version field is Version object | Tagged union |
| VSD-004 | `versioned_timestamp_microseconds` | Timestamp is microseconds | u64 value |
| VSD-005 | `versioned_with_complex_value` | Versioned<Object> | Nested correctly |
| VSD-006 | `versioned_round_trip` | Full round-trip | All fields preserved |

### 2.6 Round-Trip Property Tests

**Test Suite**: `wire::json::round_trip`

```rust
#[quickcheck]
fn prop_value_round_trip(v: Value) -> bool {
    let encoded = encode_json(&v);
    let decoded = decode_json(&encoded).unwrap();
    values_equal(&v, &decoded)  // Handles NaN specially
}

#[quickcheck]
fn prop_version_round_trip(v: Version) -> bool {
    let encoded = encode_json(&v);
    let decoded = decode_json(&encoded).unwrap();
    v == decoded
}

#[quickcheck]
fn prop_versioned_round_trip(v: Versioned<Value>) -> bool {
    let encoded = encode_json(&v);
    let decoded = decode_json(&encoded).unwrap();
    versioned_equal(&v, &decoded)
}
```

---

## 3. Facade API Tests

### 3.1 KV Operations Tests

**Test Suite**: `facade::kv`

#### 3.1.1 set() Tests

| Test ID | Test Name | Input | Expected | Notes |
|---------|-----------|-------|----------|-------|
| KV-SET-001 | `set_new_key` | set("k", 1) | Success | Creates new key |
| KV-SET-002 | `set_overwrite` | set("k", 1); set("k", 2) | get("k") = 2 | Overwrites |
| KV-SET-003 | `set_all_value_types` | All 8 types | Success for each | Type coverage |
| KV-SET-004 | `set_returns_unit` | set("k", 1) | () | No version returned |
| KV-SET-005 | `set_invalid_key_nul` | set("a\0b", 1) | InvalidKey | Key validation |
| KV-SET-006 | `set_invalid_key_reserved` | set("_strata/x", 1) | InvalidKey | Reserved prefix |
| KV-SET-007 | `set_empty_key` | set("", 1) | InvalidKey | Empty key |
| KV-SET-008 | `set_max_key_length` | set(1024_bytes, 1) | Success | At limit |
| KV-SET-009 | `set_exceeds_key_length` | set(1025_bytes, 1) | InvalidKey | Over limit |
| KV-SET-010 | `set_value_too_large` | set("k", 33MB_value) | ConstraintViolation | Size limit |
| KV-SET-011 | `set_targets_default_run` | set("k", 1) | In default run | Implicit run |

#### 3.1.2 get() Tests

| Test ID | Test Name | Setup | Call | Expected |
|---------|-----------|-------|------|----------|
| KV-GET-001 | `get_existing_key` | set("k", 123) | get("k") | Some(123) |
| KV-GET-002 | `get_missing_key` | (none) | get("missing") | None |
| KV-GET-003 | `get_returns_value_not_versioned` | set("k", 1) | get("k") | Value, not Versioned |
| KV-GET-004 | `get_null_value` | set("k", null) | get("k") | Some(Null) |
| KV-GET-005 | `get_all_value_types` | Set each type | get each | Correct type returned |
| KV-GET-006 | `get_after_overwrite` | set("k", 1); set("k", 2) | get("k") | Some(2) |
| KV-GET-007 | `get_deleted_key` | set("k", 1); delete(["k"]) | get("k") | None |
| KV-GET-008 | `get_invalid_key` | (n/a) | get("a\0b") | InvalidKey error |

#### 3.1.3 getv() Tests

| Test ID | Test Name | Setup | Expected Return |
|---------|-----------|-------|-----------------|
| KV-GETV-001 | `getv_returns_versioned` | set("k", 1) | Versioned<Value> |
| KV-GETV-002 | `getv_has_value` | set("k", 123) | .value = 123 |
| KV-GETV-003 | `getv_has_version` | set("k", 1) | .version = Txn(N) |
| KV-GETV-004 | `getv_has_timestamp` | set("k", 1) | .timestamp = microseconds |
| KV-GETV-005 | `getv_missing_key` | (none) | None |
| KV-GETV-006 | `getv_version_increments` | set("k", 1); set("k", 2) | v2.version > v1.version |
| KV-GETV-007 | `getv_timestamp_monotonic` | set("k", 1); set("k", 2) | v2.timestamp >= v1.timestamp |

#### 3.1.4 mget() Tests

| Test ID | Test Name | Setup | Call | Expected |
|---------|-----------|-------|------|----------|
| KV-MGET-001 | `mget_all_existing` | set a,b,c | mget([a,b,c]) | [Some,Some,Some] |
| KV-MGET-002 | `mget_all_missing` | (none) | mget([a,b,c]) | [None,None,None] |
| KV-MGET-003 | `mget_mixed` | set a,c | mget([a,b,c]) | [Some,None,Some] |
| KV-MGET-004 | `mget_empty_keys` | (any) | mget([]) | [] |
| KV-MGET-005 | `mget_preserves_order` | set b,a | mget([a,b]) | [a_val, b_val] |
| KV-MGET-006 | `mget_duplicate_keys` | set a | mget([a,a]) | [Some,Some] |
| KV-MGET-007 | `mget_returns_values_not_versioned` | set a,b | mget([a,b]) | Vec<Option<Value>> |

#### 3.1.5 mset() Tests

| Test ID | Test Name | Input | Verification | Notes |
|---------|-----------|-------|--------------|-------|
| KV-MSET-001 | `mset_multiple_keys` | mset([(a,1),(b,2)]) | get(a)=1, get(b)=2 | Success |
| KV-MSET-002 | `mset_empty` | mset([]) | No change | Empty is valid |
| KV-MSET-003 | `mset_overwrites` | set(a,1); mset([(a,2)]) | get(a)=2 | Overwrite |
| KV-MSET-004 | `mset_atomic_success` | mset([(a,1),(b,2)]) | Both present | Atomic |
| KV-MSET-005 | `mset_atomic_failure` | mset([(a,1),("",2)]) | Neither present | All-or-nothing |
| KV-MSET-006 | `mset_returns_unit` | mset([(a,1)]) | () | No return value |
| KV-MSET-007 | `mset_same_key_twice` | mset([(a,1),(a,2)]) | get(a)=2 | Last wins |

#### 3.1.6 delete() Tests

| Test ID | Test Name | Setup | Call | Expected Return |
|---------|-----------|-------|------|-----------------|
| KV-DEL-001 | `delete_existing_key` | set(a,1) | delete([a]) | 1 |
| KV-DEL-002 | `delete_missing_key` | (none) | delete([a]) | 0 |
| KV-DEL-003 | `delete_multiple_existing` | set a,b | delete([a,b]) | 2 |
| KV-DEL-004 | `delete_mixed` | set a | delete([a,b]) | 1 |
| KV-DEL-005 | `delete_empty_keys` | (any) | delete([]) | 0 |
| KV-DEL-006 | `delete_same_key_twice` | set a | delete([a,a]) | 1 (not 2) |
| KV-DEL-007 | `delete_verify_gone` | set a; delete([a]) | get(a) | None |
| KV-DEL-008 | `delete_returns_existed_count` | set a,b; delete([a,b,c]) | 2 | Count of existed |

#### 3.1.7 exists() Tests

| Test ID | Test Name | Setup | Call | Expected |
|---------|-----------|-------|------|----------|
| KV-EX-001 | `exists_true` | set(a,1) | exists(a) | true |
| KV-EX-002 | `exists_false` | (none) | exists(a) | false |
| KV-EX-003 | `exists_null_value` | set(a,null) | exists(a) | true |
| KV-EX-004 | `exists_after_delete` | set(a,1); delete([a]) | exists(a) | false |
| KV-EX-005 | `exists_returns_bool` | set(a,1) | exists(a) | bool type |

#### 3.1.8 exists_many() Tests

| Test ID | Test Name | Setup | Call | Expected |
|---------|-----------|-------|------|----------|
| KV-EXM-001 | `exists_many_all_exist` | set a,b,c | exists_many([a,b,c]) | 3 |
| KV-EXM-002 | `exists_many_none_exist` | (none) | exists_many([a,b,c]) | 0 |
| KV-EXM-003 | `exists_many_partial` | set a,c | exists_many([a,b,c]) | 2 |
| KV-EXM-004 | `exists_many_empty` | (any) | exists_many([]) | 0 |
| KV-EXM-005 | `exists_many_duplicates` | set a | exists_many([a,a,a]) | 3 |

#### 3.1.9 incr() Tests

| Test ID | Test Name | Setup | Call | Expected |
|---------|-----------|-------|------|----------|
| KV-INCR-001 | `incr_new_key` | (none) | incr(a, 1) | 1 |
| KV-INCR-002 | `incr_existing_int` | set(a, 10) | incr(a, 5) | 15 |
| KV-INCR-003 | `incr_negative_delta` | set(a, 10) | incr(a, -3) | 7 |
| KV-INCR-004 | `incr_zero_delta` | set(a, 10) | incr(a, 0) | 10 |
| KV-INCR-005 | `incr_wrong_type_string` | set(a, "hello") | incr(a, 1) | WrongType error |
| KV-INCR-006 | `incr_wrong_type_float` | set(a, 1.5) | incr(a, 1) | WrongType error |
| KV-INCR-007 | `incr_wrong_type_bool` | set(a, true) | incr(a, 1) | WrongType error |
| KV-INCR-008 | `incr_atomic_concurrent` | Parallel incrs | Final value | No lost updates |
| KV-INCR-009 | `incr_returns_new_value` | set(a, 5) | incr(a, 3) | Returns 8 |
| KV-INCR-010 | `incr_overflow` | set(a, i64::MAX) | incr(a, 1) | Overflow behavior |
| KV-INCR-011 | `incr_underflow` | set(a, i64::MIN) | incr(a, -1) | Underflow behavior |

### 3.2 JSON Operations Tests

**Test Suite**: `facade::json`

#### 3.2.1 json_set() Tests

| Test ID | Test Name | Key | Path | Value | Expected |
|---------|-----------|-----|------|-------|----------|
| JS-SET-001 | `json_set_root_object` | doc | $ | {a:1} | Creates doc |
| JS-SET-002 | `json_set_field` | doc | $.name | "Alice" | Sets field |
| JS-SET-003 | `json_set_nested_field` | doc | $.a.b.c | 123 | Creates path |
| JS-SET-004 | `json_set_array_index` | doc | $.items[0] | "first" | Sets element |
| JS-SET-005 | `json_set_array_append` | doc | $.items[-] | "new" | Appends |
| JS-SET-006 | `json_set_root_non_object` | doc | $ | 123 | ConstraintViolation(root_not_object) |
| JS-SET-007 | `json_set_root_array` | doc | $ | [1,2,3] | ConstraintViolation(root_not_object) |
| JS-SET-008 | `json_set_overwrite` | doc | $.x | 1; then 2 | get = 2 |
| JS-SET-009 | `json_set_invalid_path` | doc | $[invalid | (any) | InvalidPath |
| JS-SET-010 | `json_set_negative_index` | doc | $.arr[-1] | (any) | InvalidPath |

#### 3.2.2 json_get() Tests

| Test ID | Test Name | Setup | Path | Expected |
|---------|-----------|-------|------|----------|
| JS-GET-001 | `json_get_root` | {a:1,b:2} | $ | {a:1,b:2} |
| JS-GET-002 | `json_get_field` | {name:"Alice"} | $.name | "Alice" |
| JS-GET-003 | `json_get_nested` | {a:{b:{c:1}}} | $.a.b.c | 1 |
| JS-GET-004 | `json_get_array_element` | {arr:[1,2,3]} | $.arr[1] | 2 |
| JS-GET-005 | `json_get_missing_field` | {a:1} | $.b | None |
| JS-GET-006 | `json_get_missing_doc` | (none) | $ | None |
| JS-GET-007 | `json_get_returns_value` | {a:1} | $.a | Value, not Versioned |

#### 3.2.3 json_getv() Tests

| Test ID | Test Name | Setup | Path | Expected |
|---------|-----------|-------|------|----------|
| JS-GETV-001 | `json_getv_returns_versioned` | {a:1} | $.a | Versioned<Value> |
| JS-GETV-002 | `json_getv_version_is_document_level` | {a:1,b:2} | $.a | Same version as $.b |
| JS-GETV-003 | `json_getv_missing` | (none) | $ | None |

#### 3.2.4 json_del() Tests

| Test ID | Test Name | Setup | Path | Expected Return | Verification |
|---------|-----------|-------|------|-----------------|--------------|
| JS-DEL-001 | `json_del_field` | {a:1,b:2} | $.a | 1 | {b:2} remains |
| JS-DEL-002 | `json_del_missing_field` | {a:1} | $.b | 0 | Unchanged |
| JS-DEL-003 | `json_del_array_element` | {arr:[1,2,3]} | $.arr[1] | 1 | [1,3] |
| JS-DEL-004 | `json_del_nested` | {a:{b:1}} | $.a.b | 1 | {a:{}} |
| JS-DEL-005 | `json_del_root_forbidden` | {a:1} | $ | Error | Cannot delete root |

#### 3.2.5 json_merge() Tests

| Test ID | Test Name | Initial | Path | Merge Value | Result |
|---------|-----------|---------|------|-------------|--------|
| JS-MRG-001 | `json_merge_add_field` | {a:1} | $ | {b:2} | {a:1,b:2} |
| JS-MRG-002 | `json_merge_overwrite_field` | {a:1} | $ | {a:2} | {a:2} |
| JS-MRG-003 | `json_merge_null_deletes` | {a:1,b:2} | $ | {a:null} | {b:2} |
| JS-MRG-004 | `json_merge_nested` | {a:{b:1}} | $ | {a:{c:2}} | {a:{b:1,c:2}} |
| JS-MRG-005 | `json_merge_array_replaces` | {arr:[1,2]} | $ | {arr:[3]} | {arr:[3]} |
| JS-MRG-006 | `json_merge_at_path` | {x:{a:1}} | $.x | {b:2} | {x:{a:1,b:2}} |

### 3.3 Event Operations Tests

**Test Suite**: `facade::event`

| Test ID | Test Name | Description | Expected |
|---------|-----------|-------------|----------|
| EV-001 | `xadd_returns_version` | xadd(stream, payload) | Version(Sequence) |
| EV-002 | `xadd_sequence_increments` | Multiple xadds | 1, 2, 3, ... |
| EV-003 | `xadd_empty_payload` | xadd(stream, {}) | Success |
| EV-004 | `xadd_complex_payload` | xadd with nested object | Success |
| EV-005 | `xadd_bytes_in_payload` | Payload with Bytes | Success (encoded) |
| EV-006 | `xrange_all_events` | xadd x3; xrange | All 3 events |
| EV-007 | `xrange_with_limit` | xadd x3; xrange limit=2 | 2 events |
| EV-008 | `xrange_with_start_end` | xadd x5; xrange 2..4 | Events 2,3,4 |
| EV-009 | `xrange_empty_stream` | xrange on new stream | [] |
| EV-010 | `xrange_returns_versioned` | xrange | Vec<Versioned<Value>> |

### 3.4 Vector Operations Tests

**Test Suite**: `facade::vector`

| Test ID | Test Name | Description | Expected |
|---------|-----------|-------------|----------|
| VEC-001 | `vset_new_vector` | vset(k, [0.1,0.2], {}) | Success |
| VEC-002 | `vset_with_metadata` | vset(k, v, {tag:"test"}) | Success |
| VEC-003 | `vget_returns_versioned` | vget(k) | Versioned<{vector,metadata}> |
| VEC-004 | `vget_missing` | vget(missing) | None |
| VEC-005 | `vdel_existing` | vset; vdel | true |
| VEC-006 | `vdel_missing` | vdel(missing) | false |
| VEC-007 | `vset_dim_mismatch` | vset k dim=3; vset k dim=4 | ConstraintViolation(vector_dim_mismatch) |
| VEC-008 | `vset_dim_exceeds_max` | vset with 8193 dims | ConstraintViolation(vector_dim_exceeded) |
| VEC-009 | `vset_at_max_dim` | vset with 8192 dims | Success |
| VEC-010 | `vset_overwrite_same_dim` | vset k; vset k (same dim) | Success |

### 3.5 State (CAS) Operations Tests

**Test Suite**: `facade::state`

| Test ID | Test Name | Initial | expected | new | Expected Result |
|---------|-----------|---------|----------|-----|-----------------|
| CAS-001 | `cas_create_if_missing` | (none) | None | 1 | true |
| CAS-002 | `cas_create_fails_if_exists` | 1 | None | 2 | false |
| CAS-003 | `cas_update_matches` | 1 | Some(1) | 2 | true |
| CAS-004 | `cas_update_mismatch` | 1 | Some(2) | 3 | false |
| CAS-005 | `cas_null_expected` | null | Some(null) | 1 | true |
| CAS-006 | `cas_null_vs_missing` | (none) | Some(null) | 1 | false (missing != null) |
| CAS-007 | `cas_type_mismatch` | Int(1) | Some(Float(1.0)) | 2 | false |
| CAS-008 | `cas_absent_wire_encoding` | (none) | $absent | 1 | true |
| CAS-009 | `cas_structural_equality` | {a:1} | Some({a:1}) | {b:2} | true |
| CAS-010 | `cas_array_order_matters` | [1,2] | Some([2,1]) | [3] | false |
| CAS-011 | `cas_float_nan_never_matches` | NaN | Some(NaN) | 1 | false (NaN != NaN) |
| CAS-012 | `cas_get_returns_value` | 123 | cas_get(k) | | Some(123) |
| CAS-013 | `cas_get_missing` | (none) | cas_get(k) | | None |

### 3.6 History Operations Tests

**Test Suite**: `facade::history`

| Test ID | Test Name | Setup | Call | Expected |
|---------|-----------|-------|------|----------|
| HIST-001 | `history_single_version` | set k once | history(k) | 1 version |
| HIST-002 | `history_multiple_versions` | set k 3 times | history(k) | 3 versions |
| HIST-003 | `history_newest_first` | set k v1,v2,v3 | history(k) | [v3,v2,v1] order |
| HIST-004 | `history_with_limit` | set k 5 times | history(k, limit=2) | 2 versions |
| HIST-005 | `history_with_before` | set k 3 times | history(k, before=v3) | [v2,v1] |
| HIST-006 | `history_pagination` | set k 10 times | history with before | Paginate correctly |
| HIST-007 | `history_missing_key` | (none) | history(missing) | [] |
| HIST-008 | `history_kv_only` | json_set doc | history(doc) | [] (KV only) |
| HIST-009 | `get_at_existing` | set k 3 times | get_at(k, v2) | Value at v2 |
| HIST-010 | `get_at_trimmed` | (trimmed) | get_at(k, old_v) | HistoryTrimmed |
| HIST-011 | `latest_version_exists` | set k | latest_version(k) | Some(Version) |
| HIST-012 | `latest_version_missing` | (none) | latest_version(k) | None |

### 3.7 Run Operations Tests

**Test Suite**: `facade::run`

| Test ID | Test Name | Description | Expected |
|---------|-----------|-------------|----------|
| RUN-001 | `runs_includes_default` | runs() | Contains "default" |
| RUN-002 | `use_run_existing` | use_run("default") | Success |
| RUN-003 | `use_run_missing` | use_run("nonexistent") | NotFound |
| RUN-004 | `use_run_scopes_operations` | use_run(r); set(k,v) | k in run r |
| RUN-005 | `default_run_isolation` | set k in run r | get k in default | None |
| RUN-006 | `facade_targets_default` | set(k,v) | In "default" run |

### 3.8 Capability Discovery Tests

**Test Suite**: `facade::capabilities`

| Test ID | Test Name | Description | Expected |
|---------|-----------|-------------|----------|
| CAP-001 | `capabilities_returns_object` | capabilities() | Capabilities struct |
| CAP-002 | `capabilities_has_version` | capabilities().version | Version string |
| CAP-003 | `capabilities_has_operations` | capabilities().operations | Operation list |
| CAP-004 | `capabilities_has_limits` | capabilities().limits | All limits present |
| CAP-005 | `capabilities_has_encodings` | capabilities().encodings | ["json"] |
| CAP-006 | `capabilities_has_features` | capabilities().features | Feature list |
| CAP-007 | `capabilities_limits_match_config` | Configured limits | Matches actual |

---

## 4. Substrate API Tests

### 4.1 Explicit Run Parameter Tests

**Test Suite**: `substrate::run_param`

Every substrate operation must require explicit `run_id`.

| Test ID | Test Name | Operation | Verification |
|---------|-----------|-----------|--------------|
| SUB-RUN-001 | `kv_put_requires_run` | kv_put(run, k, v) | run is required |
| SUB-RUN-002 | `kv_get_requires_run` | kv_get(run, k) | run is required |
| SUB-RUN-003 | `json_set_requires_run` | json_set(run, k, p, v) | run is required |
| SUB-RUN-004 | `event_append_requires_run` | event_append(run, s, p) | run is required |
| SUB-RUN-005 | `vector_set_requires_run` | vector_set(run, k, v, m) | run is required |
| SUB-RUN-006 | `state_cas_requires_run` | state_cas(run, k, e, n) | run is required |
| SUB-RUN-007 | `cross_run_isolation` | Put in run A, get in run B | Not found |

### 4.2 Versioned Return Tests

**Test Suite**: `substrate::versioned_return`

All substrate reads must return `Versioned<T>`.

| Test ID | Test Name | Operation | Return Type |
|---------|-----------|-----------|-------------|
| SUB-VER-001 | `kv_get_returns_versioned` | kv_get(run, k) | Versioned<Value> |
| SUB-VER-002 | `json_get_returns_versioned` | json_get(run, k, p) | Versioned<Value> |
| SUB-VER-003 | `vector_get_returns_versioned` | vector_get(run, k) | Versioned<...> |
| SUB-VER-004 | `state_get_returns_versioned` | state_get(run, k) | Versioned<Value> |

### 4.3 Write Return Tests

**Test Suite**: `substrate::write_return`

All substrate writes must return `Version`.

| Test ID | Test Name | Operation | Return Type |
|---------|-----------|-----------|-------------|
| SUB-WR-001 | `kv_put_returns_version` | kv_put(run, k, v) | Version |
| SUB-WR-002 | `json_set_returns_version` | json_set(run, k, p, v) | Version |
| SUB-WR-003 | `json_merge_returns_version` | json_merge(run, k, p, v) | Version |
| SUB-WR-004 | `vector_set_returns_version` | vector_set(run, k, v, m) | Version |
| SUB-WR-005 | `event_append_returns_version` | event_append(run, s, p) | Version(Sequence) |

### 4.4 Run Lifecycle Tests

**Test Suite**: `substrate::run_lifecycle`

| Test ID | Test Name | Description | Expected |
|---------|-----------|-------------|----------|
| SUB-RL-001 | `run_create_returns_id` | run_create({}) | RunId (UUID) |
| SUB-RL-002 | `run_create_with_metadata` | run_create({name:"test"}) | Success |
| SUB-RL-003 | `run_get_existing` | run_get(id) | RunInfo |
| SUB-RL-004 | `run_get_missing` | run_get(fake_id) | None |
| SUB-RL-005 | `run_list_all` | run_list() | All runs |
| SUB-RL-006 | `run_close_custom` | run_close(id) | Success |
| SUB-RL-007 | `run_close_default_forbidden` | run_close("default") | Error |
| SUB-RL-008 | `closed_run_state` | run_get after close | state = Closed |
| SUB-RL-009 | `operations_on_closed_run` | write to closed run | ConstraintViolation |

### 4.5 Retention Tests

**Test Suite**: `substrate::retention`

| Test ID | Test Name | Description | Expected |
|---------|-----------|-------------|----------|
| SUB-RET-001 | `retention_get_default` | retention_get(run) | KeepAll |
| SUB-RET-002 | `retention_set_keep_last` | retention_set(run, KeepLast(10)) | Success |
| SUB-RET-003 | `retention_set_keep_for` | retention_set(run, KeepFor(1h)) | Success |
| SUB-RET-004 | `retention_enforced` | Set KeepLast(1); write 3 times | history has 1 |
| SUB-RET-005 | `retention_per_run` | Different policies per run | Isolated |

---

## 5. Facade→Substrate Desugaring Tests

These tests verify that the facade is a true lossless projection of the substrate.

### 5.1 KV Desugaring Parity Tests

**Test Suite**: `desugaring::kv`

| Test ID | Facade Operation | Substrate Equivalent | Verification |
|---------|------------------|---------------------|--------------|
| DS-KV-001 | `set(k, v)` | `begin(); kv_put(default, k, v); commit()` | Same state |
| DS-KV-002 | `get(k)` | `kv_get(default, k).map(\|v\| v.value)` | Same result |
| DS-KV-003 | `getv(k)` | `kv_get(default, k)` | Identical |
| DS-KV-004 | `mget([k1,k2])` | `[kv_get(default,k1), kv_get(default,k2)]` | Same results |
| DS-KV-005 | `mset([(k1,v1),(k2,v2)])` | `begin(); kv_put(..); kv_put(..); commit()` | Same state |
| DS-KV-006 | `delete([k])` | `begin(); kv_delete(default,k); commit()` | Same state |
| DS-KV-007 | `exists(k)` | `kv_get(default,k).is_some()` | Same result |
| DS-KV-008 | `incr(k, d)` | `kv_incr(default, k, d)` | Same result |

### 5.2 JSON Desugaring Parity Tests

**Test Suite**: `desugaring::json`

| Test ID | Facade Operation | Substrate Equivalent | Verification |
|---------|------------------|---------------------|--------------|
| DS-JS-001 | `json_set(k, p, v)` | `begin(); json_set(default, k, p, v); commit()` | Same state |
| DS-JS-002 | `json_get(k, p)` | `json_get(default, k, p).map(\|v\| v.value)` | Same result |
| DS-JS-003 | `json_getv(k, p)` | `json_get(default, k, p)` | Identical |
| DS-JS-004 | `json_del(k, p)` | `begin(); json_delete(default, k, p); commit()` | Same state |
| DS-JS-005 | `json_merge(k, p, v)` | `begin(); json_merge(default, k, p, v); commit()` | Same state |

### 5.3 Error Propagation Tests

**Test Suite**: `desugaring::errors`

| Test ID | Test Name | Description | Verification |
|---------|-----------|-------------|--------------|
| DS-ERR-001 | `facade_propagates_invalid_key` | set("", v) | Same InvalidKey as substrate |
| DS-ERR-002 | `facade_propagates_wrong_type` | incr on string | Same WrongType as substrate |
| DS-ERR-003 | `facade_propagates_constraint` | Set too-large value | Same ConstraintViolation |
| DS-ERR-004 | `no_error_swallowing` | Any substrate error | Surfaces unchanged |
| DS-ERR-005 | `error_details_preserved` | Error with details | Details match |

### 5.4 Invariant Verification Tests

**Test Suite**: `desugaring::invariants`

| Invariant | Test | Verification |
|-----------|------|--------------|
| FAC-1 | Every facade op maps to substrate | Desugaring produces valid substrate ops |
| FAC-2 | No new semantics | Facade result = substrate result |
| FAC-3 | No hidden errors | All errors surface |
| FAC-4 | No reordering | Operation order preserved |
| FAC-5 | Traceable behavior | No magic |

---

## 6. Error Model Tests

### 6.1 Error Code Tests

**Test Suite**: `error::codes`

| Test ID | Code | Trigger Condition | Verification |
|---------|------|-------------------|--------------|
| ERR-001 | `NotFound` | get_at missing key | Code matches |
| ERR-002 | `WrongType` | incr on string | Code matches |
| ERR-003 | `InvalidKey` | NUL in key | Code matches |
| ERR-004 | `InvalidPath` | Malformed JSON path | Code matches |
| ERR-005 | `HistoryTrimmed` | get_at trimmed version | Code matches |
| ERR-006 | `ConstraintViolation` | Value too large | Code matches |
| ERR-007 | `Conflict` | CAS failure | Code matches |
| ERR-008 | `SerializationError` | Malformed input | Code matches |
| ERR-009 | `StorageError` | IO failure | Code matches |
| ERR-010 | `InternalError` | Bug/invariant violation | Code matches |

### 6.2 Error Wire Shape Tests

**Test Suite**: `error::wire_shape`

| Test ID | Test Name | Verification |
|---------|-----------|--------------|
| ERR-WS-001 | `error_has_code` | error.code is string |
| ERR-WS-002 | `error_has_message` | error.message is string |
| ERR-WS-003 | `error_has_details` | error.details exists (may be null) |
| ERR-WS-004 | `response_ok_false` | ok field is false |
| ERR-WS-005 | `response_has_id` | Response ID matches request |

### 6.3 ConstraintViolation Reason Tests

**Test Suite**: `error::constraint_reasons`

| Test ID | Reason | Trigger | Verification |
|---------|--------|---------|--------------|
| ERR-CV-001 | `value_too_large` | 33MB value | reason = "value_too_large" |
| ERR-CV-002 | `nesting_too_deep` | 129 levels | reason = "nesting_too_deep" |
| ERR-CV-003 | `key_too_long` | 1025 byte key | reason = "key_too_long" |
| ERR-CV-004 | `vector_dim_exceeded` | 8193 dims | reason = "vector_dim_exceeded" |
| ERR-CV-005 | `vector_dim_mismatch` | Different dims | reason = "vector_dim_mismatch" |
| ERR-CV-006 | `root_not_object` | json_set $ to int | reason = "root_not_object" |
| ERR-CV-007 | `reserved_prefix` | _strata/ key | reason = "reserved_prefix" |

### 6.4 Error Details Payload Tests

**Test Suite**: `error::details`

| Test ID | Error | Expected Details |
|---------|-------|------------------|
| ERR-DT-001 | `HistoryTrimmed` | {requested: Version, earliest_retained: Version} |
| ERR-DT-002 | `ConstraintViolation` | {reason: string, ...context} |
| ERR-DT-003 | `Conflict` | {expected: ..., actual: ...} |
| ERR-DT-004 | `InvalidKey` | {key: string, reason: string} |
| ERR-DT-005 | `InvalidPath` | {path: string, reason: string} |

---

# Part II: M11b Test Suites (Consumer Surfaces)

> **Scope**: These test suites validate consumer surfaces (CLI, SDK) that build on the core contract. M11b tests can only begin after all M11a tests pass.
>
> **Prerequisite**: All M11a tests must pass before executing M11b tests.

---

## 7. CLI Tests

### 7.1 Argument Parsing Tests

**Test Suite**: `cli::parsing`

| Test ID | Input | Expected Parse |
|---------|-------|----------------|
| CLI-P-001 | `123` | Int(123) |
| CLI-P-002 | `-456` | Int(-456) |
| CLI-P-003 | `0` | Int(0) |
| CLI-P-004 | `1.23` | Float(1.23) |
| CLI-P-005 | `-4.56` | Float(-4.56) |
| CLI-P-006 | `0.0` | Float(0.0) |
| CLI-P-007 | `"hello"` | String("hello") (quotes stripped) |
| CLI-P-008 | `hello` | String("hello") (bare word) |
| CLI-P-009 | `""` | String("") (empty) |
| CLI-P-010 | `true` | Bool(true) |
| CLI-P-011 | `false` | Bool(false) |
| CLI-P-012 | `null` | Null |
| CLI-P-013 | `{"a":1}` | Object |
| CLI-P-014 | `[1,2,3]` | Array |
| CLI-P-015 | `b64:SGVsbG8=` | Bytes("Hello") |
| CLI-P-016 | `b64:` | Bytes([]) (empty) |

### 7.2 Output Formatting Tests

**Test Suite**: `cli::output`

| Test ID | Value | Expected Output |
|---------|-------|-----------------|
| CLI-O-001 | None | `(nil)` |
| CLI-O-002 | Int(42) | `(integer) 42` |
| CLI-O-003 | count=3 | `(integer) 3` |
| CLI-O-004 | Bool(true) | `(integer) 1` |
| CLI-O-005 | Bool(false) | `(integer) 0` |
| CLI-O-006 | String("hello") | `"hello"` |
| CLI-O-007 | Null | `null` |
| CLI-O-008 | Object | JSON formatted |
| CLI-O-009 | Array | JSON formatted |
| CLI-O-010 | Bytes | `{"$bytes":"..."}` |
| CLI-O-011 | Error | JSON on stderr, exit 1 |

### 7.3 Command Tests

**Test Suite**: `cli::commands`

| Test ID | Command | Expected |
|---------|---------|----------|
| CLI-C-001 | `strata set x 123` | Success |
| CLI-C-002 | `strata get x` | `123` or `(nil)` |
| CLI-C-003 | `strata mget a b c` | Array output |
| CLI-C-004 | `strata mset a 1 b 2` | Success |
| CLI-C-005 | `strata delete x y` | `(integer) N` |
| CLI-C-006 | `strata exists x` | `(integer) 0/1` |
| CLI-C-007 | `strata incr counter` | `(integer) N` |
| CLI-C-008 | `strata json.set doc $.x 1` | Success |
| CLI-C-009 | `strata json.get doc $.x` | Value |
| CLI-C-010 | `strata xadd stream '{"type":"test"}'` | Version |
| CLI-C-011 | `strata vset doc1 "[0.1,0.2]" '{}'` | Success |
| CLI-C-012 | `strata vget doc1` | Versioned output |
| CLI-C-013 | `strata vdel doc1` | `(integer) 0/1` |
| CLI-C-014 | `strata cas.set k null 123` | `(integer) 0/1` |
| CLI-C-015 | `strata cas.get k` | Value or `(nil)` |
| CLI-C-016 | `strata cas.set k 123 456` | `(integer) 0/1` |
| CLI-C-017 | `strata history mykey` | Version list |
| CLI-C-018 | `strata history mykey --limit 5` | Limited list |

### 7.4 Run Scoping Tests

**Test Suite**: `cli::run_scoping`

| Test ID | Test Name | Command | Expected |
|---------|-----------|---------|----------|
| CLI-RS-001 | `default_implicit` | `strata set x 1` | In default run |
| CLI-RS-002 | `default_explicit` | `strata --run=default set x 1` | In default run |
| CLI-RS-003 | `custom_run` | `strata --run=myrun set x 1` | In myrun |
| CLI-RS-004 | `run_isolation` | Set in run A, get in run B | Not found |
| CLI-RS-005 | `missing_run` | `strata --run=fake get x` | NotFound error |

---

## 8. Versioned<T> Tests

### 8.1 Structure Tests

**Test Suite**: `versioned::structure`

| Test ID | Test Name | Verification |
|---------|-----------|--------------|
| VS-001 | `has_value_field` | .value is present |
| VS-002 | `has_version_field` | .version is present |
| VS-003 | `has_timestamp_field` | .timestamp is present |
| VS-004 | `value_is_correct_type` | .value matches T |
| VS-005 | `version_is_tagged_union` | .version has type + value |
| VS-006 | `timestamp_is_u64` | .timestamp is u64 |

### 8.2 Version Tag Tests

**Test Suite**: `versioned::version_tags`

| Test ID | Test Name | Context | Expected Tag |
|---------|-----------|---------|--------------|
| VT-001 | `kv_uses_txn` | kv_put | type = "txn" |
| VT-002 | `json_uses_txn` | json_set | type = "txn" |
| VT-003 | `vector_uses_txn` | vector_set | type = "txn" |
| VT-004 | `event_uses_sequence` | event_append | type = "sequence" |
| VT-005 | `state_uses_counter` | state_set | type = "counter" |
| VT-006 | `run_uses_txn` | run_create | type = "txn" |

### 8.3 Timestamp Tests

**Test Suite**: `versioned::timestamp`

| Test ID | Test Name | Verification |
|---------|-----------|--------------|
| TS-001 | `timestamp_is_microseconds` | In µs range |
| TS-002 | `timestamp_monotonic` | Later op >= earlier |
| TS-003 | `timestamp_reasonable` | Within expected range |
| TS-004 | `timestamp_attached` | Always present |

### 8.4 Version Incomparability Tests

**Test Suite**: `versioned::incomparability`

| Test ID | Test Name | Comparison | Expected |
|---------|-----------|------------|----------|
| VI-001 | `txn_vs_sequence` | Txn(5) vs Sequence(5) | Cannot compare / Error |
| VI-002 | `txn_vs_counter` | Txn(5) vs Counter(5) | Cannot compare / Error |
| VI-003 | `sequence_vs_counter` | Sequence(5) vs Counter(5) | Cannot compare / Error |
| VI-004 | `same_type_comparable` | Txn(5) < Txn(10) | Comparable |

---

## 9. Run Semantics Tests

### 9.1 Default Run Tests

**Test Suite**: `run::default`

| Test ID | Test Name | Verification |
|---------|-----------|--------------|
| DR-001 | `default_run_exists` | run_list() contains "default" |
| DR-002 | `default_run_name_literal` | Name is literally "default" |
| DR-003 | `default_run_always_exists` | Never absent |
| DR-004 | `default_run_not_closeable` | run_close("default") errors |
| DR-005 | `facade_targets_default` | All facade ops go to default |
| DR-006 | `default_created_lazily` | On first write or open |

### 9.2 Run Isolation Tests

**Test Suite**: `run::isolation`

| Test ID | Test Name | Description | Verification |
|---------|-----------|-------------|--------------|
| RI-001 | `keys_isolated` | set k in run A | get k in run B = None |
| RI-002 | `json_docs_isolated` | json_set in A | json_get in B = None |
| RI-003 | `events_isolated` | xadd in A | xrange in B = [] |
| RI-004 | `vectors_isolated` | vset in A | vget in B = None |
| RI-005 | `history_isolated` | history in A | history in B = [] |

### 9.3 RunId Format Tests

**Test Suite**: `run::id_format`

| Test ID | Test Name | Verification |
|---------|-----------|--------------|
| RF-001 | `uuid_format` | Matches UUID regex |
| RF-002 | `lowercase` | All lowercase |
| RF-003 | `hyphenated` | Standard UUID hyphens |
| RF-004 | `default_is_literal` | "default" not UUID |

---

## 10. Transaction Semantics Tests

### 10.1 Isolation Tests

**Test Suite**: `txn::isolation`

| Test ID | Test Name | Description | Expected |
|---------|-----------|-------------|----------|
| TXN-I-001 | `snapshot_isolation` | Read sees snapshot | Consistent view |
| TXN-I-002 | `read_own_writes` | Write then read in txn | Sees own writes |
| TXN-I-003 | `no_dirty_reads` | Uncommitted not visible | Other txns don't see |
| TXN-I-004 | `repeatable_read` | Read twice in txn | Same value |

### 10.2 Atomicity Tests

**Test Suite**: `txn::atomicity`

| Test ID | Test Name | Description | Expected |
|---------|-----------|-------------|----------|
| TXN-A-001 | `all_or_nothing` | Multi-op commit | All visible |
| TXN-A-002 | `rollback_none` | Multi-op rollback | None visible |
| TXN-A-003 | `partial_failure` | Fail mid-txn | None visible |

### 10.3 Conflict Tests

**Test Suite**: `txn::conflict`

| Test ID | Test Name | Description | Expected |
|---------|-----------|-------------|----------|
| TXN-C-001 | `write_write_conflict` | Two txns write same key | One gets Conflict |
| TXN-C-002 | `occ_validation` | Conflict at commit | Conflict error |
| TXN-C-003 | `retry_succeeds` | Retry after conflict | Success |

### 10.4 Auto-Commit Tests

**Test Suite**: `txn::auto_commit`

| Test ID | Test Name | Description | Verification |
|---------|-----------|-------------|--------------|
| TXN-AC-001 | `facade_auto_commits` | set(k, v) | Immediately visible |
| TXN-AC-002 | `each_op_separate` | set(a,1); set(b,2) | Two txns |
| TXN-AC-003 | `mset_single_txn` | mset([(a,1),(b,2)]) | One txn |

---

## 11. History & Retention Tests

### 11.1 History Ordering Tests

**Test Suite**: `history::ordering`

| Test ID | Test Name | Description | Verification |
|---------|-----------|-------------|--------------|
| HO-001 | `newest_first` | history returns | Descending by version |
| HO-002 | `oldest_last` | Last element | Earliest version |
| HO-003 | `consistent_order` | Multiple calls | Same order |

### 11.2 History Pagination Tests

**Test Suite**: `history::pagination`

| Test ID | Test Name | Description | Verification |
|---------|-----------|-------------|--------------|
| HP-001 | `limit_works` | limit=5 | Max 5 results |
| HP-002 | `before_exclusive` | before=v5 | v4, v3, ... |
| HP-003 | `paginate_all` | Page through all | Complete coverage |
| HP-004 | `empty_page` | before oldest | Empty result |

### 11.3 Retention Policy Tests

**Test Suite**: `history::retention`

| Test ID | Test Name | Policy | Verification |
|---------|-----------|--------|--------------|
| RET-001 | `keep_all` | KeepAll | All versions retained |
| RET-002 | `keep_last_n` | KeepLast(5) | Only 5 versions |
| RET-003 | `keep_for_duration` | KeepFor(1h) | Time-based trim |
| RET-004 | `composite` | Multiple policies | Union behavior |

### 11.4 HistoryTrimmed Tests

**Test Suite**: `history::trimmed`

| Test ID | Test Name | Description | Verification |
|---------|-----------|-------------|--------------|
| HT-001 | `trimmed_error` | get_at trimmed | HistoryTrimmed error |
| HT-002 | `trimmed_details` | Error details | requested + earliest_retained |
| HT-003 | `history_excludes_trimmed` | history call | Only retained versions |

---

## 12. Determinism Tests

### 12.1 Operation Determinism Tests

**Test Suite**: `determinism::operations`

| Test ID | Test Name | Description | Verification |
|---------|-----------|-------------|--------------|
| DET-001 | `same_ops_same_state` | Replay same operations | Identical state |
| DET-002 | `order_matters` | Different order | Different state |
| DET-003 | `idempotent_replay` | Replay twice | Same result |

### 12.2 WAL Replay Tests

**Test Suite**: `determinism::wal_replay`

| Test ID | Test Name | Description | Verification |
|---------|-----------|-------------|--------------|
| WAL-001 | `replay_produces_same_state` | Replay WAL | Byte-identical state |
| WAL-002 | `replay_multiple_times` | Replay N times | Always same |
| WAL-003 | `partial_replay` | Replay prefix | Correct intermediate state |

### 12.3 Timestamp Independence Tests

**Test Suite**: `determinism::timestamp_independence`

| Test ID | Test Name | Description | Verification |
|---------|-----------|-------------|--------------|
| TI-001 | `different_timestamps_same_logic` | Replay with different time | Same logical state |
| TI-002 | `timestamp_metadata_only` | Timestamps | Don't affect operations |
| TI-003 | `timestamps_not_inputs` | State transitions | Independent of time |

---

## 13. Contract Stability Tests

### 13.1 Frozen Element Tests

**Test Suite**: `contract::frozen`

| Test ID | Element | Verification |
|---------|---------|--------------|
| FRZ-001 | Operation names | Match spec exactly |
| FRZ-002 | Parameter names | Match spec exactly |
| FRZ-003 | Return shapes | Match spec exactly |
| FRZ-004 | Error codes | All codes present |
| FRZ-005 | Wire encoding | Exact format |
| FRZ-006 | Version types | txn/sequence/counter |
| FRZ-007 | Timestamp units | Microseconds |
| FRZ-008 | Default behaviors | As documented |

### 13.2 Regression Tests

**Test Suite**: `contract::regression`

Golden file tests for all API responses:

```
tests/golden/
├── kv/
│   ├── set_response.json
│   ├── get_response.json
│   ├── getv_response.json
│   └── ...
├── json/
│   └── ...
├── errors/
│   ├── invalid_key.json
│   ├── wrong_type.json
│   └── ...
└── wire/
    ├── bytes_encoding.json
    ├── float_nan.json
    └── ...
```

---

## 14. Fuzz Testing Strategy

### 14.1 Value Model Fuzzing

```rust
// Fuzz all value types
fuzz_target!(|data: &[u8]| {
    if let Ok(value) = arbitrary_value(data) {
        // Must not panic
        let encoded = encode_json(&value);
        let decoded = decode_json(&encoded);
        // Must round-trip (handling NaN specially)
    }
});

// Fuzz key validation
fuzz_target!(|key: &str| {
    match validate_key(key) {
        Ok(_) => { /* valid key */ }
        Err(e) => assert!(matches!(e, InvalidKey))
    }
});
```

### 14.2 Wire Encoding Fuzzing

```rust
// Fuzz JSON decoding
fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        match decode_json(s) {
            Ok(value) => {
                // Valid decode must round-trip
                let reencoded = encode_json(&value);
                let redecoded = decode_json(&reencoded);
                assert_values_equal(&value, &redecoded);
            }
            Err(_) => { /* invalid input is fine */ }
        }
    }
});
```

### 14.3 Path Fuzzing

```rust
// Fuzz JSON paths
fuzz_target!(|path: &str| {
    match validate_path(path) {
        Ok(_) => { /* valid path */ }
        Err(e) => assert!(matches!(e, InvalidPath))
    }
});
```

---

## 15. Test Data Generators

### 15.1 Value Generators

```rust
fn gen_value(depth: usize) -> Value {
    if depth == 0 {
        return gen_scalar();
    }

    match rand::random::<u8>() % 10 {
        0 => Value::Null,
        1 => Value::Bool(rand::random()),
        2 => Value::Int(rand::random()),
        3 => gen_float(),
        4 => gen_string(),
        5 => gen_bytes(),
        6..=7 => Value::Array(gen_array(depth - 1)),
        8..=9 => Value::Object(gen_object(depth - 1)),
        _ => unreachable!()
    }
}

fn gen_float() -> Value {
    match rand::random::<u8>() % 10 {
        0 => Value::Float(f64::NAN),
        1 => Value::Float(f64::INFINITY),
        2 => Value::Float(f64::NEG_INFINITY),
        3 => Value::Float(-0.0),
        4 => Value::Float(0.0),
        _ => Value::Float(rand::random())
    }
}
```

### 15.2 Key Generators

```rust
fn gen_valid_key() -> String {
    let len = rand::random::<usize>() % 100 + 1;
    gen_random_utf8(len)
}

fn gen_invalid_key() -> String {
    match rand::random::<u8>() % 5 {
        0 => String::new(),  // empty
        1 => format!("a\x00b"),  // NUL
        2 => format!("_strata/foo"),  // reserved
        3 => "x".repeat(2000),  // too long
        4 => gen_invalid_utf8(),
        _ => unreachable!()
    }
}
```

### 15.3 Operation Sequence Generators

```rust
fn gen_operation_sequence(len: usize) -> Vec<Operation> {
    (0..len).map(|_| gen_operation()).collect()
}

fn gen_operation() -> Operation {
    match rand::random::<u8>() % 10 {
        0..=3 => Operation::Set(gen_valid_key(), gen_value(3)),
        4..=5 => Operation::Get(gen_valid_key()),
        6 => Operation::Delete(vec![gen_valid_key()]),
        7 => Operation::Incr(gen_valid_key(), rand::random()),
        8 => Operation::JsonSet(gen_valid_key(), gen_path(), gen_value(2)),
        9 => Operation::Xadd(gen_valid_key(), gen_object(2)),
        _ => unreachable!()
    }
}
```

---

## 16. Test Infrastructure

### 16.1 Test Harness

```rust
struct TestHarness {
    facade: Facade,
    substrate: Substrate,
    wire_encoder: WireEncoder,
}

impl TestHarness {
    fn new() -> Self {
        // Create isolated test instance
    }

    fn reset(&mut self) {
        // Reset to clean state
    }

    fn assert_facade_substrate_parity(&self, op: Operation) {
        let facade_result = self.facade.execute(&op);
        let substrate_result = self.desugar_and_execute(&op);
        assert_eq!(facade_result, substrate_result);
    }

    fn assert_round_trip(&self, value: &Value) {
        let encoded = self.wire_encoder.encode(value);
        let decoded = self.wire_encoder.decode(&encoded).unwrap();
        assert_values_equal(value, &decoded);
    }
}
```

### 16.2 Golden File Testing

```rust
#[test]
fn test_golden_files() {
    for golden_file in glob("tests/golden/**/*.json") {
        let expected = read_golden_file(&golden_file);
        let actual = execute_golden_test(&golden_file);
        assert_eq!(expected, actual, "Golden file mismatch: {}", golden_file);
    }
}
```

### 16.3 CI Integration

```yaml
# .github/workflows/m11-contract-tests.yml
name: M11 Contract Tests

on: [push, pull_request]

jobs:
  # M11a: Core Contract & API Tests
  m11a-core-contract:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Value Model Tests
        run: cargo test --package core value_model::

      - name: Wire Encoding Tests
        run: cargo test --package wire

      - name: Facade API Tests
        run: cargo test --package api facade::

      - name: Substrate API Tests
        run: cargo test --package api substrate::

      - name: Desugaring Parity Tests
        run: cargo test --package api desugaring::

      - name: Error Model Tests
        run: cargo test --package core error::

      - name: Determinism Tests
        run: cargo test --package engine determinism::

      - name: Contract Stability Tests (Core)
        run: cargo test --package tests contract::core

      - name: Fuzz Tests (limited)
        run: cargo +nightly fuzz run value_fuzz -- -max_total_time=60

  # M11b: Consumer Surfaces Tests (depends on M11a)
  m11b-consumer-surfaces:
    needs: m11a-core-contract
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: CLI Tests
        run: cargo test --package cli

      - name: SDK Rust Tests
        run: cargo test --package sdk

      - name: SDK Python Tests
        run: pytest tests/sdk/python/

      - name: SDK JavaScript Tests
        run: npm test --prefix tests/sdk/js/

      - name: Cross-SDK Interop Tests
        run: cargo test --package tests sdk_interop::

      - name: Contract Stability Tests (Surface)
        run: cargo test --package tests contract::surface
```

### 16.4 Coverage Requirements

| Module | Minimum Coverage | Milestone |
|--------|------------------|-----------|
| Value Model | 100% | M11a |
| Wire Encoding | 100% | M11a |
| Facade API | 95% | M11a |
| Substrate API | 95% | M11a |
| Error Model | 100% | M11a |
| CLI Parsing | 100% | M11b |
| CLI Output | 100% | M11b |
| SDK Type Mapping | 95% | M11b |

---

## 17. SDK Conformance Tests

> **Milestone**: M11b (Consumer Surfaces)

### 17.1 Rust SDK Type Mapping Tests

**Test Suite**: `sdk::rust::type_mapping`

| Test ID | Strata Type | Rust Type | Verification |
|---------|-------------|-----------|--------------|
| SDK-R-001 | Null | `Option<T>::None` | Correct mapping |
| SDK-R-002 | Bool | `bool` | Correct mapping |
| SDK-R-003 | Int | `i64` | Correct mapping |
| SDK-R-004 | Float | `f64` | Correct mapping |
| SDK-R-005 | String | `String` | Correct mapping |
| SDK-R-006 | Bytes | `Vec<u8>` | Correct mapping |
| SDK-R-007 | Array | `Vec<Value>` | Correct mapping |
| SDK-R-008 | Object | `HashMap<String, Value>` | Correct mapping |
| SDK-R-009 | Version | `Version` enum | Tagged union |
| SDK-R-010 | Versioned<T> | `Versioned<T>` struct | All fields |
| SDK-R-011 | Error | `StrataError` enum | All variants |

### 17.2 Python SDK Type Mapping Tests

**Test Suite**: `sdk::python::type_mapping`

| Test ID | Strata Type | Python Type | Verification |
|---------|-------------|-------------|--------------|
| SDK-PY-001 | Null | `None` | Correct mapping |
| SDK-PY-002 | Bool | `bool` | Correct mapping |
| SDK-PY-003 | Int | `int` | Correct mapping |
| SDK-PY-004 | Float | `float` | Correct mapping |
| SDK-PY-005 | String | `str` | Correct mapping |
| SDK-PY-006 | Bytes | `bytes` | Correct mapping |
| SDK-PY-007 | Array | `list` | Correct mapping |
| SDK-PY-008 | Object | `dict` | Correct mapping |
| SDK-PY-009 | Version | Version dataclass | Tagged union |
| SDK-PY-010 | Versioned<T> | Versioned dataclass | All fields |
| SDK-PY-011 | Error | `StrataError` exception | All variants |

### 17.3 JavaScript/TypeScript SDK Type Mapping Tests

**Test Suite**: `sdk::js::type_mapping`

| Test ID | Strata Type | JS/TS Type | Verification |
|---------|-------------|------------|--------------|
| SDK-JS-001 | Null | `null` | Correct mapping |
| SDK-JS-002 | Bool | `boolean` | Correct mapping |
| SDK-JS-003 | Int | `bigint` | **CRITICAL**: Not `number` |
| SDK-JS-004 | Float | `number` | Correct mapping |
| SDK-JS-005 | String | `string` | Correct mapping |
| SDK-JS-006 | Bytes | `Uint8Array` | Correct mapping |
| SDK-JS-007 | Array | `Array<Value>` | Correct mapping |
| SDK-JS-008 | Object | `Record<string, Value>` | Correct mapping |
| SDK-JS-009 | Version | `Version` type | Tagged union |
| SDK-JS-010 | Versioned<T> | `Versioned<T>` type | All fields |
| SDK-JS-011 | Error | `StrataError` class | All variants |

### 17.4 SDK Round-Trip Tests

**Test Suite**: `sdk::round_trip`

| Test ID | Test Name | Description |
|---------|-----------|-------------|
| SDK-RT-001 | `rust_value_round_trip` | Rust Value → API → Rust Value |
| SDK-RT-002 | `python_value_round_trip` | Python dict → API → Python dict |
| SDK-RT-003 | `js_value_round_trip` | JS object → API → JS object |
| SDK-RT-004 | `cross_sdk_interop_rust_python` | Rust write → Python read |
| SDK-RT-005 | `cross_sdk_interop_rust_js` | Rust write → JS read |
| SDK-RT-006 | `cross_sdk_interop_python_rust` | Python write → Rust read |
| SDK-RT-007 | `cross_sdk_interop_python_js` | Python write → JS read |
| SDK-RT-008 | `cross_sdk_interop_js_rust` | JS write → Rust read |
| SDK-RT-009 | `cross_sdk_interop_js_python` | JS write → Python read |
| SDK-RT-010 | `float_edge_cases_all_sdks` | NaN, Inf, -0.0 across all SDKs |

### 17.5 SDK Error Handling Tests

**Test Suite**: `sdk::errors`

| Test ID | Error | Rust | Python | JS |
|---------|-------|------|--------|-----|
| SDK-ERR-001 | `NotFound` | `Err(NotFound)` | `StrataNotFoundError` | Thrown error |
| SDK-ERR-002 | `WrongType` | `Err(WrongType)` | `StrataWrongTypeError` | Thrown error |
| SDK-ERR-003 | `InvalidKey` | `Err(InvalidKey)` | `StrataInvalidKeyError` | Thrown error |
| SDK-ERR-004 | `ConstraintViolation` | `Err(ConstraintViolation)` | `StrataConstraintError` | Thrown error |
| SDK-ERR-005 | `Conflict` | `Err(Conflict)` | `StrataConflictError` | Thrown error |

---

## Appendix A: Test Matrix Summary

### M11a Test Matrix (Core Contract & API)

| Test Category | Test Count | Priority | Automation |
|---------------|------------|----------|------------|
| Value Model Construction | 35 | CRITICAL | Unit |
| Float Edge Cases | 15 | CRITICAL | Unit + Property |
| Value Equality | 30 | CRITICAL | Unit |
| No Type Coercion | 15 | CRITICAL | Unit |
| Size Limits | 18 | CRITICAL | Unit |
| Key Validation | 20 | CRITICAL | Unit |
| Wire JSON Encoding | 31 | CRITICAL | Unit + Property |
| Wire Wrappers | 15 | CRITICAL | Unit |
| Wire Envelope | 10 | CRITICAL | Unit |
| Wire Version | 10 | CRITICAL | Unit |
| Facade KV | 50+ | CRITICAL | Integration |
| Facade JSON | 25+ | CRITICAL | Integration |
| Facade Event | 10 | CRITICAL | Integration |
| Facade Vector | 10 | CRITICAL | Integration |
| Facade State | 13 | CRITICAL | Integration |
| Facade History | 12 | HIGH | Integration |
| Facade Run | 6 | HIGH | Integration |
| Facade Capabilities | 7 | HIGH | Integration |
| Substrate API | 30+ | CRITICAL | Integration |
| Desugaring Parity | 30+ | CRITICAL | Contract |
| Error Model | 30+ | CRITICAL | Unit |
| Versioned<T> | 20+ | CRITICAL | Unit |
| Run Semantics | 15+ | CRITICAL | Integration |
| Transaction Semantics | 15+ | CRITICAL | Integration |
| History & Retention | 15+ | HIGH | Integration |
| Determinism | 10+ | CRITICAL | Integration |
| Contract Stability (Core) | 10+ | CRITICAL | Regression |
| Fuzz Tests | N/A | HIGH | Continuous |

**M11a Total**: ~400 tests

### M11b Test Matrix (Consumer Surfaces)

| Test Category | Test Count | Priority | Automation |
|---------------|------------|----------|------------|
| CLI Parsing | 16 | CRITICAL | Unit |
| CLI Output | 11 | CRITICAL | Unit |
| CLI Commands | 18 | CRITICAL | Integration |
| CLI Run Scoping | 5 | HIGH | Integration |
| SDK Rust Type Mapping | 11 | CRITICAL | Unit |
| SDK Python Type Mapping | 11 | CRITICAL | Unit |
| SDK JS Type Mapping | 11 | CRITICAL | Unit |
| SDK Round-Trip | 10 | CRITICAL | Integration |
| SDK Error Handling | 5 | CRITICAL | Integration |
| Contract Stability (Surface) | 5+ | CRITICAL | Regression |

**M11b Total**: ~100 tests

---

**Combined Total**: 500+ tests

---

## Appendix B: Critical Path Tests

### M11a Critical Path (Must pass before M11b begins)

These tests MUST pass before M11a can be considered complete:

1. **All 8 value types construct and round-trip** (VAL-001 to VAL-035, JE-001 to JE-031)
2. **Float edge cases work** (FLT-001 to FLT-015)
3. **No type coercion** (NC-001 to NC-015)
4. **Int(1) != Float(1.0)** (EQ-010)
5. **NaN != NaN** (EQ-013, FLT-002)
6. **Bytes vs String distinction** (EQ-018)
7. **$bytes wrapper** (WR-001 to WR-004)
8. **$f64 wrapper** (WR-004 to WR-008)
9. **$absent wrapper** (WR-009, WR-010, CAS-008)
10. **All error codes work** (ERR-001 to ERR-010)
11. **Facade-Substrate parity** (DS-KV-001 to DS-KV-008)
12. **Error propagation** (DS-ERR-001 to DS-ERR-005)
13. **Determinism** (DET-001 to DET-003, WAL-001 to WAL-003)
14. **Contract stability (core)** (FRZ-001 to FRZ-008)

### M11b Critical Path (Must pass for M11 completion)

These tests MUST pass before M11b can be considered complete:

1. **CLI argument parsing** (CLI-P-001 to CLI-P-016)
2. **CLI output formatting** (CLI-O-001 to CLI-O-011)
3. **CLI commands for all primitives** (CLI-C-001 to CLI-C-018)
4. **CLI run scoping** (CLI-RS-001 to CLI-RS-005)
5. **Rust SDK type mapping** (SDK-R-001 to SDK-R-011)
6. **Python SDK type mapping** (SDK-PY-001 to SDK-PY-011)
7. **JS SDK type mapping** (SDK-JS-001 to SDK-JS-011)
8. **Cross-SDK interoperability** (SDK-RT-004 to SDK-RT-009)
9. **Float edge cases across all SDKs** (SDK-RT-010)
10. **SDK error handling** (SDK-ERR-001 to SDK-ERR-005)

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-21 | Initial comprehensive testing plan |
| 2.0 | 2026-01-21 | Split into M11a/M11b test suites, added SDK Conformance Tests (Section 17), updated critical paths |

---

**This testing plan is the quality gate for M11. M11a must pass before M11b begins. No M11 release without all critical path tests passing.**
