# Epic 80: Value Model Stabilization - Implementation Prompts

**Epic Goal**: Finalize and freeze the canonical Value model with all 8 types, equality semantics, and size limits

**GitHub Issue**: [#549](https://github.com/anibjoshi/in-mem/issues/549)
**Status**: Ready to begin
**Dependencies**: M10 complete
**Phase**: 1 (Data Model Foundation)

---

## NAMING CONVENTION - CRITICAL

> **NEVER use "M11" in the actual codebase or comments.**
>
> - "M11" is an internal milestone tracker only
> - "Strata" IS allowed and encouraged (e.g., `StrataError`, `strata_value`)
>
> **CORRECT**: `//! Strata value model types`
> **WRONG**: `//! M11 value type`

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

Before starting ANY story in this epic, read:
1. **Contract Spec (AUTHORITATIVE)**: `docs/milestones/M11/M11_CONTRACT.md`
2. **Implementation Plan**: `docs/milestones/M11/M11_IMPLEMENTATION_PLAN.md`
3. **Epic Spec**: `docs/milestones/M11/EPIC_80_VALUE_MODEL.md`
4. **Prompt Header**: `docs/prompts/M11/M11_PROMPT_HEADER.md`

---

## Epic 80 Overview

### Scope
- Finalize `Value` enum with all 8 types
- Float edge cases (NaN, Infinity, -0.0, subnormals)
- Value equality with IEEE-754 semantics
- **No implicit type coercion** (CRITICAL)
- Size limits (keys, strings, bytes, arrays, objects, nesting)
- Key validation rules

### Key Rules for Epic 80

1. **Eight types exactly**: Null, Bool, Int, Float, String, Bytes, Array, Object
2. **No type coercion**: `Int(1) != Float(1.0)` - CRITICAL
3. **IEEE-754 for floats**: NaN != NaN, -0.0 == 0.0
4. **Bytes != String**: Even when content matches

### Success Criteria
- [ ] Value enum with exactly 8 variants
- [ ] Float preserves NaN, +Inf, -Inf, -0.0
- [ ] `PartialEq` follows IEEE-754 and no-coercion rules
- [ ] Size limits configurable and enforced
- [ ] Key validation with NUL and reserved prefix checks
- [ ] All tests passing

### Component Breakdown
- **Story #550**: Value Enum Finalization - FOUNDATION
- **Story #551**: Float Edge Case Handling - CRITICAL
- **Story #552**: Value Equality Semantics - CRITICAL
- **Story #553**: No Type Coercion Verification - CRITICAL
- **Story #554**: Size Limits Implementation - CRITICAL
- **Story #555**: Key Validation Rules - CRITICAL

---

## Dependency Graph

```
Story #550 (Value Enum) ──┬──> Story #551 (Floats)
                          │
                          ├──> Story #552 (Equality)
                          │
                          └──> Story #553 (No Coercion)

Story #552 + #553 ────────> All other epics

Story #554 (Limits) ──────> Story #555 (Key Validation)
```

**Recommended Order**: #550 → #551 → #552 → #553 → #554 → #555

---

## Story #550: Value Enum Finalization

**GitHub Issue**: [#550](https://github.com/anibjoshi/in-mem/issues/550)
**Dependencies**: None
**Blocks**: Stories #551, #552, #553

### Start Story

```bash
gh issue view 550
./scripts/start-story.sh 80 550 value-enum-finalization
```

### Key Implementation Points

```rust
/// Canonical Strata Value type - FROZEN after M11
#[derive(Debug, Clone)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    Array(Vec<Value>),
    Object(HashMap<String, Value>),
}
```

### Acceptance Criteria

- [ ] Value enum has exactly 8 variants
- [ ] All construction tests pass
- [ ] `type_name()` returns correct strings
- [ ] No additional variants or type aliases

### Complete Story

```bash
./scripts/complete-story.sh 550
```

---

## Story #551: Float Edge Case Handling

**GitHub Issue**: [#551](https://github.com/anibjoshi/in-mem/issues/551)
**Dependencies**: Story #550
**Blocks**: Story #552

### Start Story

```bash
gh issue view 551
./scripts/start-story.sh 80 551 float-edge-cases
```

### Key Implementation Points

```rust
impl Value {
    /// Check if this is a special float value
    pub fn is_special_float(&self) -> bool {
        match self {
            Value::Float(f) => {
                f.is_nan() || f.is_infinite() || (f == &0.0 && f.is_sign_negative())
            }
            _ => false,
        }
    }
}

pub enum SpecialFloatKind {
    NaN,
    PositiveInfinity,
    NegativeInfinity,
    NegativeZero,
}
```

### Acceptance Criteria

- [ ] NaN constructs and is identified correctly
- [ ] +Inf and -Inf construct and are identified correctly
- [ ] -0.0 constructs with sign preserved
- [ ] Subnormal floats not flushed to zero
- [ ] Full f64 precision preserved

### Complete Story

```bash
./scripts/complete-story.sh 551
```

---

## Story #552: Value Equality Semantics

**GitHub Issue**: [#552](https://github.com/anibjoshi/in-mem/issues/552)
**Dependencies**: Stories #550, #551
**Blocks**: Story #553, all other epics

### Start Story

```bash
gh issue view 552
./scripts/start-story.sh 80 552 value-equality
```

### Key Implementation Points

```rust
impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Null, Value::Null) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b, // IEEE-754: NaN != NaN
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Bytes(a), Value::Bytes(b)) => a == b,
            (Value::Array(a), Value::Array(b)) => a == b,
            (Value::Object(a), Value::Object(b)) => a == b,
            // CRITICAL: Different types are NEVER equal
            _ => false,
        }
    }
}
```

### CRITICAL Tests

```rust
#[test]
fn test_nan_not_equals_nan() {
    // IEEE-754: NaN != NaN
    assert_ne!(Value::Float(f64::NAN), Value::Float(f64::NAN));
}

#[test]
fn test_negative_zero_equals_positive_zero() {
    // IEEE-754: -0.0 == 0.0
    assert_eq!(Value::Float(-0.0), Value::Float(0.0));
}

#[test]
fn test_int_not_equals_float() {
    // NO TYPE COERCION
    assert_ne!(Value::Int(1), Value::Float(1.0));
}
```

### Acceptance Criteria

- [ ] NaN != NaN (IEEE-754)
- [ ] -0.0 == 0.0 (IEEE-754)
- [ ] +Inf == +Inf, -Inf == -Inf
- [ ] Different types NEVER equal
- [ ] Hash implementation consistent with equality

### Complete Story

```bash
./scripts/complete-story.sh 552
```

---

## Story #553: No Type Coercion Verification

**GitHub Issue**: [#553](https://github.com/anibjoshi/in-mem/issues/553)
**Dependencies**: Story #552
**Blocks**: None (verification story)

### Start Story

```bash
gh issue view 553
./scripts/start-story.sh 80 553 no-type-coercion
```

### CRITICAL Tests

These tests are the CONTRACT. NEVER modify them.

```rust
#[test]
fn nc_001_int_one_not_float_one() {
    assert_ne!(Value::Int(1), Value::Float(1.0));
}

#[test]
fn nc_002_int_zero_not_float_zero() {
    assert_ne!(Value::Int(0), Value::Float(0.0));
}

#[test]
fn nc_003_string_not_bytes() {
    let s = "abc";
    let b = s.as_bytes().to_vec();
    assert_ne!(Value::String(s.to_string()), Value::Bytes(b));
}

#[test]
fn nc_004_bool_true_not_int_one() {
    assert_ne!(Value::Bool(true), Value::Int(1));
}

#[test]
fn nc_005_null_not_empty_string() {
    assert_ne!(Value::Null, Value::String(String::new()));
}
```

### Acceptance Criteria

- [ ] All NC-* tests pass
- [ ] No implicit widening (Int → Float)
- [ ] No implicit encoding (String → Bytes)
- [ ] No truthiness coercion (Bool ↔ Int)
- [ ] No nullish coercion (Null ↔ empty/zero)

### Complete Story

```bash
./scripts/complete-story.sh 553
```

---

## Story #554: Size Limits Implementation

**GitHub Issue**: [#554](https://github.com/anibjoshi/in-mem/issues/554)
**Dependencies**: Story #550
**Blocks**: Story #555

### Start Story

```bash
gh issue view 554
./scripts/start-story.sh 80 554 size-limits
```

### Key Implementation Points

```rust
pub struct Limits {
    pub max_key_bytes: usize,           // 1024
    pub max_string_bytes: usize,        // 16 MiB
    pub max_bytes_len: usize,           // 16 MiB
    pub max_value_bytes_encoded: usize, // 32 MiB
    pub max_array_len: usize,           // 1,000,000
    pub max_object_entries: usize,      // 1,000,000
    pub max_nesting_depth: usize,       // 128
    pub max_vector_dim: usize,          // 8192
}
```

### Acceptance Criteria

- [ ] Default limits match spec
- [ ] `validate_key()` enforces key length
- [ ] `validate_value()` enforces all value limits
- [ ] Nesting depth checked recursively
- [ ] Error types include actual and max values

### Complete Story

```bash
./scripts/complete-story.sh 554
```

---

## Story #555: Key Validation Rules

**GitHub Issue**: [#555](https://github.com/anibjoshi/in-mem/issues/555)
**Dependencies**: Story #554
**Blocks**: None

### Start Story

```bash
gh issue view 555
./scripts/start-story.sh 80 555 key-validation
```

### Key Implementation Points

```rust
pub fn validate_key(key: &str) -> Result<(), KeyError> {
    if key.is_empty() {
        return Err(KeyError::Empty);
    }
    if key.contains('\x00') {
        return Err(KeyError::ContainsNul);
    }
    if key.starts_with("_strata/") {
        return Err(KeyError::ReservedPrefix);
    }
    if key.len() > MAX_KEY_BYTES {
        return Err(KeyError::TooLong { actual: key.len(), max: MAX_KEY_BYTES });
    }
    Ok(())
}
```

### Acceptance Criteria

- [ ] Empty key rejected
- [ ] NUL bytes rejected
- [ ] `_strata/` prefix rejected
- [ ] Length limit enforced
- [ ] Unicode keys allowed
- [ ] `_mykey` allowed (not reserved)

### Complete Story

```bash
./scripts/complete-story.sh 555
```

---

## Epic 80 Completion Checklist

### 1. Final Validation

```bash
~/.cargo/bin/cargo build --workspace
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### 2. Verify Critical Tests

```bash
# No-coercion tests MUST pass
~/.cargo/bin/cargo test nc_ -- --nocapture

# Float edge case tests
~/.cargo/bin/cargo test float_ -- --nocapture

# Equality tests
~/.cargo/bin/cargo test equality_ -- --nocapture
```

### 3. Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-80-value-model -m "Epic 80: Value Model Stabilization complete

Delivered:
- Value enum with 8 types (Null, Bool, Int, Float, String, Bytes, Array, Object)
- IEEE-754 float semantics (NaN, Inf, -0.0)
- No type coercion (Int(1) != Float(1.0))
- Size limits implementation
- Key validation rules

Stories: #550, #551, #552, #553, #554, #555
"
git push origin develop
gh issue close 549 --comment "Epic 80: Value Model Stabilization - COMPLETE"
```

---

## Summary

Epic 80 establishes the VALUE MODEL foundation:

- **8 types only**: No additions, no aliases
- **No coercion**: Types are always distinct
- **IEEE-754 floats**: Preserve all special values
- **Size limits**: Configurable, enforced
- **Key validation**: NUL, prefix, length rules

This foundation is CRITICAL for all subsequent epics.
