# Epic 80: Value Model Stabilization

**Goal**: Finalize and freeze the canonical Value model with all 8 types, equality semantics, and size limits

**Dependencies**: M10 complete

**Milestone**: M11a (Core Contract & API)

---

## Test-Driven Development Protocol

> **CRITICAL**: This epic follows strict Test-Driven Development (TDD). Tests are written FIRST, then implementation.

### The TDD Cycle

1. **Write the test** - Define expected behavior before writing any implementation
2. **Run the test** - Verify it fails (red)
3. **Write minimal implementation** - Just enough to pass the test
4. **Run the test** - Verify it passes (green)
5. **Refactor** - Clean up while keeping tests green

### NEVER Modify Tests to Make Them Pass

> **ABSOLUTE RULE**: When a test fails, the problem is in the implementation, NOT the test.

**FORBIDDEN behaviors:**
- Changing test assertions to match buggy output
- Weakening test conditions (e.g., `==` to `!=`, exact match to contains)
- Removing test cases that expose bugs
- Adding `#[ignore]` to failing tests
- Changing expected values to match actual (wrong) values

**REQUIRED behaviors:**
- Investigate WHY the test fails
- Fix the implementation to match the specification
- If the spec is wrong, get explicit approval before changing both spec AND test
- Document any spec changes in the epic

**Example of WRONG approach:**
```rust
#[test]
fn test_int_not_equal_float() {
    // WRONG: Changed test because implementation was buggy
    assert_eq!(Value::Int(1), Value::Float(1.0)); // Was: assert_ne!
}
```

**Example of CORRECT approach:**
```rust
#[test]
fn test_int_not_equal_float() {
    // Spec says Int(1) != Float(1.0) - NO TYPE COERCION
    // If this fails, fix Value::eq(), not this test
    assert_ne!(Value::Int(1), Value::Float(1.0));
}
```

---

## Scope

- Finalize `Value` enum with all 8 types
- Float edge cases (NaN, Infinity, -0.0, subnormals)
- Value equality with IEEE-754 semantics
- No implicit type coercion
- Size limits (keys, strings, bytes, arrays, objects, nesting)
- Key validation rules

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #560 | Value Enum Finalization | FOUNDATION |
| #561 | Float Edge Case Handling | CRITICAL |
| #562 | Value Equality Semantics | CRITICAL |
| #563 | No Type Coercion Verification | CRITICAL |
| #564 | Size Limits Implementation | CRITICAL |
| #565 | Key Validation Rules | CRITICAL |

---

## Story #560: Value Enum Finalization

**File**: `crates/core/src/value.rs`

**Deliverable**: Finalized Value enum with all 8 canonical types

### Tests FIRST

```rust
#[cfg(test)]
mod value_construction_tests {
    use super::*;

    // === Null Tests ===

    #[test]
    fn test_null_construction() {
        let v = Value::Null;
        assert!(matches!(v, Value::Null));
    }

    // === Bool Tests ===

    #[test]
    fn test_bool_true_construction() {
        let v = Value::Bool(true);
        assert!(matches!(v, Value::Bool(true)));
    }

    #[test]
    fn test_bool_false_construction() {
        let v = Value::Bool(false);
        assert!(matches!(v, Value::Bool(false)));
    }

    // === Int Tests ===

    #[test]
    fn test_int_positive_construction() {
        let v = Value::Int(123);
        assert!(matches!(v, Value::Int(123)));
    }

    #[test]
    fn test_int_negative_construction() {
        let v = Value::Int(-456);
        assert!(matches!(v, Value::Int(-456)));
    }

    #[test]
    fn test_int_zero_construction() {
        let v = Value::Int(0);
        assert!(matches!(v, Value::Int(0)));
    }

    #[test]
    fn test_int_max_construction() {
        let v = Value::Int(i64::MAX);
        assert!(matches!(v, Value::Int(i64::MAX)));
    }

    #[test]
    fn test_int_min_construction() {
        let v = Value::Int(i64::MIN);
        assert!(matches!(v, Value::Int(i64::MIN)));
    }

    // === Float Tests ===

    #[test]
    fn test_float_positive_construction() {
        let v = Value::Float(1.23);
        match v {
            Value::Float(f) => assert!((f - 1.23).abs() < f64::EPSILON),
            _ => panic!("Expected Float"),
        }
    }

    #[test]
    fn test_float_negative_construction() {
        let v = Value::Float(-4.56);
        match v {
            Value::Float(f) => assert!((f - (-4.56)).abs() < f64::EPSILON),
            _ => panic!("Expected Float"),
        }
    }

    #[test]
    fn test_float_zero_construction() {
        let v = Value::Float(0.0);
        match v {
            Value::Float(f) => assert_eq!(f, 0.0),
            _ => panic!("Expected Float"),
        }
    }

    // === String Tests ===

    #[test]
    fn test_string_empty_construction() {
        let v = Value::String(String::new());
        assert!(matches!(v, Value::String(ref s) if s.is_empty()));
    }

    #[test]
    fn test_string_ascii_construction() {
        let v = Value::String("hello".to_string());
        assert!(matches!(v, Value::String(ref s) if s == "hello"));
    }

    #[test]
    fn test_string_unicode_construction() {
        let v = Value::String("こんにちは".to_string());
        assert!(matches!(v, Value::String(ref s) if s == "こんにちは"));
    }

    #[test]
    fn test_string_emoji_construction() {
        let v = Value::String("🚀🎉".to_string());
        assert!(matches!(v, Value::String(ref s) if s == "🚀🎉"));
    }

    // === Bytes Tests ===

    #[test]
    fn test_bytes_empty_construction() {
        let v = Value::Bytes(vec![]);
        assert!(matches!(v, Value::Bytes(ref b) if b.is_empty()));
    }

    #[test]
    fn test_bytes_binary_construction() {
        let v = Value::Bytes(vec![0, 255, 128]);
        assert!(matches!(v, Value::Bytes(ref b) if b == &[0, 255, 128]));
    }

    #[test]
    fn test_bytes_all_values_construction() {
        let all_bytes: Vec<u8> = (0..=255).collect();
        let v = Value::Bytes(all_bytes.clone());
        assert!(matches!(v, Value::Bytes(ref b) if b == &all_bytes));
    }

    // === Array Tests ===

    #[test]
    fn test_array_empty_construction() {
        let v = Value::Array(vec![]);
        assert!(matches!(v, Value::Array(ref a) if a.is_empty()));
    }

    #[test]
    fn test_array_single_element_construction() {
        let v = Value::Array(vec![Value::Int(1)]);
        assert!(matches!(v, Value::Array(ref a) if a.len() == 1));
    }

    #[test]
    fn test_array_mixed_types_construction() {
        let v = Value::Array(vec![
            Value::Int(1),
            Value::String("hello".to_string()),
            Value::Bool(true),
        ]);
        assert!(matches!(v, Value::Array(ref a) if a.len() == 3));
    }

    #[test]
    fn test_array_nested_construction() {
        let v = Value::Array(vec![
            Value::Array(vec![Value::Int(1)]),
        ]);
        match &v {
            Value::Array(outer) => {
                assert_eq!(outer.len(), 1);
                assert!(matches!(&outer[0], Value::Array(_)));
            }
            _ => panic!("Expected Array"),
        }
    }

    // === Object Tests ===

    #[test]
    fn test_object_empty_construction() {
        let v = Value::Object(HashMap::new());
        assert!(matches!(v, Value::Object(ref o) if o.is_empty()));
    }

    #[test]
    fn test_object_single_entry_construction() {
        let mut map = HashMap::new();
        map.insert("key".to_string(), Value::Int(42));
        let v = Value::Object(map);
        assert!(matches!(v, Value::Object(ref o) if o.len() == 1));
    }

    #[test]
    fn test_object_nested_construction() {
        let mut inner = HashMap::new();
        inner.insert("inner_key".to_string(), Value::Int(1));

        let mut outer = HashMap::new();
        outer.insert("outer_key".to_string(), Value::Object(inner));

        let v = Value::Object(outer);
        match &v {
            Value::Object(o) => {
                assert!(matches!(o.get("outer_key"), Some(Value::Object(_))));
            }
            _ => panic!("Expected Object"),
        }
    }
}
```

### Implementation

```rust
use std::collections::HashMap;

/// Canonical Strata Value type
///
/// This is the ONLY public value model. All API surfaces use this type.
/// After M11, this enum is FROZEN and cannot change without major version bump.
#[derive(Debug, Clone)]
pub enum Value {
    /// JSON null / absence of value
    Null,

    /// Boolean true or false
    Bool(bool),

    /// 64-bit signed integer
    /// Range: -9,223,372,036,854,775,808 to 9,223,372,036,854,775,807
    Int(i64),

    /// 64-bit IEEE-754 floating point
    /// Supports: NaN, +Inf, -Inf, -0.0, subnormals
    Float(f64),

    /// UTF-8 encoded string
    String(String),

    /// Arbitrary binary data
    /// NOT equivalent to String - distinct type
    Bytes(Vec<u8>),

    /// Ordered sequence of values
    Array(Vec<Value>),

    /// String-keyed map of values
    Object(HashMap<String, Value>),
}

impl Value {
    /// Returns the type name as a string (for error messages)
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Null => "Null",
            Value::Bool(_) => "Bool",
            Value::Int(_) => "Int",
            Value::Float(_) => "Float",
            Value::String(_) => "String",
            Value::Bytes(_) => "Bytes",
            Value::Array(_) => "Array",
            Value::Object(_) => "Object",
        }
    }

    /// Check if this value is null
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    /// Try to get as bool
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    /// Try to get as i64
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// Try to get as f64
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Value::Float(f) => Some(*f),
            _ => None,
        }
    }

    /// Try to get as string slice
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// Try to get as bytes slice
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Value::Bytes(b) => Some(b),
            _ => None,
        }
    }

    /// Try to get as array slice
    pub fn as_array(&self) -> Option<&[Value]> {
        match self {
            Value::Array(a) => Some(a),
            _ => None,
        }
    }

    /// Try to get as object reference
    pub fn as_object(&self) -> Option<&HashMap<String, Value>> {
        match self {
            Value::Object(o) => Some(o),
            _ => None,
        }
    }
}
```

### Acceptance Criteria

- [ ] Value enum has exactly 8 variants: Null, Bool, Int, Float, String, Bytes, Array, Object
- [ ] All construction tests pass
- [ ] Type accessor methods work correctly
- [ ] `type_name()` returns correct strings
- [ ] No additional variants or type aliases

---

## Story #561: Float Edge Case Handling

**File**: `crates/core/src/value.rs`

**Deliverable**: Correct handling of IEEE-754 special float values

### Tests FIRST

```rust
#[cfg(test)]
mod float_edge_case_tests {
    use super::*;

    // === NaN Tests ===

    #[test]
    fn test_nan_construction() {
        let v = Value::Float(f64::NAN);
        match v {
            Value::Float(f) => assert!(f.is_nan()),
            _ => panic!("Expected Float"),
        }
    }

    #[test]
    fn test_nan_is_nan() {
        let f = f64::NAN;
        assert!(f.is_nan());
    }

    // === Infinity Tests ===

    #[test]
    fn test_positive_infinity_construction() {
        let v = Value::Float(f64::INFINITY);
        match v {
            Value::Float(f) => {
                assert!(f.is_infinite());
                assert!(f.is_sign_positive());
            }
            _ => panic!("Expected Float"),
        }
    }

    #[test]
    fn test_negative_infinity_construction() {
        let v = Value::Float(f64::NEG_INFINITY);
        match v {
            Value::Float(f) => {
                assert!(f.is_infinite());
                assert!(f.is_sign_negative());
            }
            _ => panic!("Expected Float"),
        }
    }

    // === Negative Zero Tests ===

    #[test]
    fn test_negative_zero_construction() {
        let v = Value::Float(-0.0);
        match v {
            Value::Float(f) => {
                assert_eq!(f, 0.0); // -0.0 == 0.0 per IEEE-754
                assert!(f.is_sign_negative()); // But sign is preserved
            }
            _ => panic!("Expected Float"),
        }
    }

    #[test]
    fn test_negative_zero_sign_preserved() {
        let v = Value::Float(-0.0);
        match v {
            Value::Float(f) => {
                // Bit pattern must be preserved
                assert_eq!(f.to_bits(), (-0.0_f64).to_bits());
            }
            _ => panic!("Expected Float"),
        }
    }

    // === Extreme Values ===

    #[test]
    fn test_float_max_construction() {
        let v = Value::Float(f64::MAX);
        match v {
            Value::Float(f) => assert_eq!(f, f64::MAX),
            _ => panic!("Expected Float"),
        }
    }

    #[test]
    fn test_float_min_positive_construction() {
        let v = Value::Float(f64::MIN_POSITIVE);
        match v {
            Value::Float(f) => assert_eq!(f, f64::MIN_POSITIVE),
            _ => panic!("Expected Float"),
        }
    }

    #[test]
    fn test_float_subnormal_construction() {
        // Smallest subnormal
        let subnormal = f64::from_bits(1);
        assert!(subnormal.is_subnormal());

        let v = Value::Float(subnormal);
        match v {
            Value::Float(f) => {
                assert!(f.is_subnormal());
                assert_eq!(f.to_bits(), 1);
            }
            _ => panic!("Expected Float"),
        }
    }

    #[test]
    fn test_float_precision_preserved() {
        // This value cannot be exactly represented in f32
        let precise = 1.0000000000000002_f64;
        let v = Value::Float(precise);
        match v {
            Value::Float(f) => {
                assert_eq!(f.to_bits(), precise.to_bits());
            }
            _ => panic!("Expected Float"),
        }
    }

    // === Float Helper Methods ===

    #[test]
    fn test_is_special_float_nan() {
        let v = Value::Float(f64::NAN);
        assert!(v.is_special_float());
    }

    #[test]
    fn test_is_special_float_infinity() {
        let v = Value::Float(f64::INFINITY);
        assert!(v.is_special_float());
    }

    #[test]
    fn test_is_special_float_neg_zero() {
        let v = Value::Float(-0.0);
        assert!(v.is_special_float());
    }

    #[test]
    fn test_is_special_float_normal() {
        let v = Value::Float(1.5);
        assert!(!v.is_special_float());
    }
}
```

### Implementation

```rust
impl Value {
    /// Check if this is a special float value requiring wire encoding wrapper
    ///
    /// Special floats: NaN, +Inf, -Inf, -0.0
    /// These require `{"$f64": "..."}` wrapper in JSON wire encoding.
    pub fn is_special_float(&self) -> bool {
        match self {
            Value::Float(f) => {
                f.is_nan() || f.is_infinite() || (f == &0.0 && f.is_sign_negative())
            }
            _ => false,
        }
    }

    /// Get the special float kind if this is a special float
    pub fn special_float_kind(&self) -> Option<SpecialFloatKind> {
        match self {
            Value::Float(f) => {
                if f.is_nan() {
                    Some(SpecialFloatKind::NaN)
                } else if *f == f64::INFINITY {
                    Some(SpecialFloatKind::PositiveInfinity)
                } else if *f == f64::NEG_INFINITY {
                    Some(SpecialFloatKind::NegativeInfinity)
                } else if *f == 0.0 && f.is_sign_negative() {
                    Some(SpecialFloatKind::NegativeZero)
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

/// Kinds of special float values
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecialFloatKind {
    NaN,
    PositiveInfinity,
    NegativeInfinity,
    NegativeZero,
}

impl SpecialFloatKind {
    /// Convert to wire encoding string
    pub fn to_wire_string(&self) -> &'static str {
        match self {
            SpecialFloatKind::NaN => "NaN",
            SpecialFloatKind::PositiveInfinity => "+Inf",
            SpecialFloatKind::NegativeInfinity => "-Inf",
            SpecialFloatKind::NegativeZero => "-0.0",
        }
    }

    /// Parse from wire encoding string
    pub fn from_wire_string(s: &str) -> Option<Self> {
        match s {
            "NaN" => Some(SpecialFloatKind::NaN),
            "+Inf" => Some(SpecialFloatKind::PositiveInfinity),
            "-Inf" => Some(SpecialFloatKind::NegativeInfinity),
            "-0.0" => Some(SpecialFloatKind::NegativeZero),
            _ => None,
        }
    }

    /// Convert to f64 value
    pub fn to_f64(&self) -> f64 {
        match self {
            SpecialFloatKind::NaN => f64::NAN,
            SpecialFloatKind::PositiveInfinity => f64::INFINITY,
            SpecialFloatKind::NegativeInfinity => f64::NEG_INFINITY,
            SpecialFloatKind::NegativeZero => -0.0,
        }
    }
}
```

### Acceptance Criteria

- [ ] NaN constructs and is identified correctly
- [ ] +Inf and -Inf construct and are identified correctly
- [ ] -0.0 constructs with sign preserved
- [ ] Subnormal floats are not flushed to zero
- [ ] Full f64 precision is preserved
- [ ] `is_special_float()` correctly identifies special values
- [ ] `SpecialFloatKind` wire encoding strings match spec

---

## Story #562: Value Equality Semantics

**File**: `crates/core/src/value.rs`

**Deliverable**: Correct equality implementation following IEEE-754 and no-coercion rules

### Tests FIRST

```rust
#[cfg(test)]
mod equality_tests {
    use super::*;

    // === Same Type Equality ===

    #[test]
    fn test_null_equals_null() {
        assert_eq!(Value::Null, Value::Null);
    }

    #[test]
    fn test_bool_true_equals_true() {
        assert_eq!(Value::Bool(true), Value::Bool(true));
    }

    #[test]
    fn test_bool_false_equals_false() {
        assert_eq!(Value::Bool(false), Value::Bool(false));
    }

    #[test]
    fn test_bool_true_not_equals_false() {
        assert_ne!(Value::Bool(true), Value::Bool(false));
    }

    #[test]
    fn test_int_equals_same_int() {
        assert_eq!(Value::Int(42), Value::Int(42));
    }

    #[test]
    fn test_int_not_equals_different_int() {
        assert_ne!(Value::Int(42), Value::Int(43));
    }

    #[test]
    fn test_float_equals_same_float() {
        assert_eq!(Value::Float(3.14), Value::Float(3.14));
    }

    #[test]
    fn test_string_equals_same_string() {
        assert_eq!(
            Value::String("hello".to_string()),
            Value::String("hello".to_string())
        );
    }

    #[test]
    fn test_string_not_equals_different_string() {
        assert_ne!(
            Value::String("hello".to_string()),
            Value::String("world".to_string())
        );
    }

    #[test]
    fn test_bytes_equals_same_bytes() {
        assert_eq!(
            Value::Bytes(vec![1, 2, 3]),
            Value::Bytes(vec![1, 2, 3])
        );
    }

    #[test]
    fn test_array_equals_same_elements() {
        assert_eq!(
            Value::Array(vec![Value::Int(1), Value::Int(2)]),
            Value::Array(vec![Value::Int(1), Value::Int(2)])
        );
    }

    #[test]
    fn test_array_not_equals_different_order() {
        assert_ne!(
            Value::Array(vec![Value::Int(1), Value::Int(2)]),
            Value::Array(vec![Value::Int(2), Value::Int(1)])
        );
    }

    #[test]
    fn test_object_equals_same_entries() {
        let mut map1 = HashMap::new();
        map1.insert("a".to_string(), Value::Int(1));

        let mut map2 = HashMap::new();
        map2.insert("a".to_string(), Value::Int(1));

        assert_eq!(Value::Object(map1), Value::Object(map2));
    }

    #[test]
    fn test_object_equals_regardless_of_insertion_order() {
        let mut map1 = HashMap::new();
        map1.insert("a".to_string(), Value::Int(1));
        map1.insert("b".to_string(), Value::Int(2));

        let mut map2 = HashMap::new();
        map2.insert("b".to_string(), Value::Int(2));
        map2.insert("a".to_string(), Value::Int(1));

        assert_eq!(Value::Object(map1), Value::Object(map2));
    }

    // === IEEE-754 Float Equality ===

    #[test]
    fn test_nan_not_equals_nan() {
        // CRITICAL: NaN != NaN per IEEE-754
        assert_ne!(Value::Float(f64::NAN), Value::Float(f64::NAN));
    }

    #[test]
    fn test_different_nan_payloads_not_equal() {
        let nan1 = f64::from_bits(0x7ff8000000000001);
        let nan2 = f64::from_bits(0x7ff8000000000002);
        assert!(nan1.is_nan() && nan2.is_nan());
        assert_ne!(Value::Float(nan1), Value::Float(nan2));
    }

    #[test]
    fn test_positive_infinity_equals_positive_infinity() {
        assert_eq!(Value::Float(f64::INFINITY), Value::Float(f64::INFINITY));
    }

    #[test]
    fn test_negative_infinity_equals_negative_infinity() {
        assert_eq!(Value::Float(f64::NEG_INFINITY), Value::Float(f64::NEG_INFINITY));
    }

    #[test]
    fn test_positive_infinity_not_equals_negative_infinity() {
        assert_ne!(Value::Float(f64::INFINITY), Value::Float(f64::NEG_INFINITY));
    }

    #[test]
    fn test_negative_zero_equals_positive_zero() {
        // CRITICAL: -0.0 == 0.0 per IEEE-754
        assert_eq!(Value::Float(-0.0), Value::Float(0.0));
    }

    // === Cross-Type Inequality (NO COERCION) ===

    #[test]
    fn test_null_not_equals_bool() {
        assert_ne!(Value::Null, Value::Bool(false));
    }

    #[test]
    fn test_null_not_equals_int_zero() {
        assert_ne!(Value::Null, Value::Int(0));
    }

    #[test]
    fn test_null_not_equals_empty_string() {
        assert_ne!(Value::Null, Value::String(String::new()));
    }

    #[test]
    fn test_int_one_not_equals_float_one() {
        // CRITICAL: No type coercion - Int(1) != Float(1.0)
        assert_ne!(Value::Int(1), Value::Float(1.0));
    }

    #[test]
    fn test_int_zero_not_equals_float_zero() {
        // CRITICAL: No type coercion
        assert_ne!(Value::Int(0), Value::Float(0.0));
    }

    #[test]
    fn test_bool_true_not_equals_int_one() {
        // CRITICAL: No type coercion
        assert_ne!(Value::Bool(true), Value::Int(1));
    }

    #[test]
    fn test_bool_false_not_equals_int_zero() {
        // CRITICAL: No type coercion
        assert_ne!(Value::Bool(false), Value::Int(0));
    }

    #[test]
    fn test_string_not_equals_bytes() {
        // CRITICAL: String("abc") != Bytes([97, 98, 99])
        assert_ne!(
            Value::String("abc".to_string()),
            Value::Bytes(vec![97, 98, 99])
        );
    }

    #[test]
    fn test_empty_array_not_equals_null() {
        assert_ne!(Value::Array(vec![]), Value::Null);
    }

    #[test]
    fn test_empty_object_not_equals_null() {
        assert_ne!(Value::Object(HashMap::new()), Value::Null);
    }

    #[test]
    fn test_string_number_not_equals_int() {
        // "123" != Int(123)
        assert_ne!(Value::String("123".to_string()), Value::Int(123));
    }
}
```

### Implementation

```rust
impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            // Same types
            (Value::Null, Value::Null) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => {
                // IEEE-754 equality: NaN != NaN, but -0.0 == 0.0
                a == b
            }
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Bytes(a), Value::Bytes(b)) => a == b,
            (Value::Array(a), Value::Array(b)) => a == b,
            (Value::Object(a), Value::Object(b)) => a == b,

            // Different types: NEVER equal (NO TYPE COERCION)
            _ => false,
        }
    }
}

impl Eq for Value {}

// NOTE: We implement Eq even though Float doesn't satisfy reflexivity (NaN != NaN).
// This is intentional - our equality semantics follow IEEE-754.
// HashMaps of Values will work correctly because we also implement Hash consistently.

impl std::hash::Hash for Value {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Discriminant first for type distinction
        std::mem::discriminant(self).hash(state);

        match self {
            Value::Null => {}
            Value::Bool(b) => b.hash(state),
            Value::Int(i) => i.hash(state),
            Value::Float(f) => {
                // Hash the bits for consistency
                // Note: -0.0 and 0.0 have different bits but equal values
                // We normalize to 0.0 bits for hashing
                if *f == 0.0 {
                    0u64.hash(state);
                } else {
                    f.to_bits().hash(state);
                }
            }
            Value::String(s) => s.hash(state),
            Value::Bytes(b) => b.hash(state),
            Value::Array(a) => {
                a.len().hash(state);
                for v in a {
                    v.hash(state);
                }
            }
            Value::Object(o) => {
                // Hash entries in sorted order for determinism
                let mut entries: Vec<_> = o.iter().collect();
                entries.sort_by_key(|(k, _)| *k);
                entries.len().hash(state);
                for (k, v) in entries {
                    k.hash(state);
                    v.hash(state);
                }
            }
        }
    }
}
```

### Acceptance Criteria

- [ ] Same-type equality works correctly
- [ ] NaN != NaN (IEEE-754 semantics)
- [ ] -0.0 == 0.0 (IEEE-754 semantics)
- [ ] +Inf == +Inf, -Inf == -Inf
- [ ] Different types are NEVER equal (no coercion)
- [ ] Int(1) != Float(1.0) - CRITICAL
- [ ] String("abc") != Bytes([97,98,99]) - CRITICAL
- [ ] Hash implementation is consistent with equality

---

## Story #563: No Type Coercion Verification

**File**: `crates/core/src/value.rs` (tests), `crates/api/src/facade.rs` (tests)

**Deliverable**: Comprehensive tests proving no type coercion occurs anywhere

### Tests FIRST

```rust
#[cfg(test)]
mod no_coercion_tests {
    use super::*;

    /// These tests verify the NO TYPE COERCION rule.
    /// If any test fails, the implementation is WRONG.
    /// DO NOT modify these tests - fix the implementation.

    #[test]
    fn nc_001_int_one_not_float_one() {
        assert_ne!(Value::Int(1), Value::Float(1.0));
    }

    #[test]
    fn nc_002_int_zero_not_float_zero() {
        assert_ne!(Value::Int(0), Value::Float(0.0));
    }

    #[test]
    fn nc_003_int_max_not_float() {
        assert_ne!(Value::Int(i64::MAX), Value::Float(i64::MAX as f64));
    }

    #[test]
    fn nc_004_string_not_bytes() {
        // Even when the bytes are the UTF-8 encoding of the string
        let s = "abc";
        let b = s.as_bytes().to_vec();
        assert_ne!(Value::String(s.to_string()), Value::Bytes(b));
    }

    #[test]
    fn nc_005_null_not_empty_string() {
        assert_ne!(Value::Null, Value::String(String::new()));
    }

    #[test]
    fn nc_006_null_not_zero() {
        assert_ne!(Value::Null, Value::Int(0));
    }

    #[test]
    fn nc_007_null_not_false() {
        assert_ne!(Value::Null, Value::Bool(false));
    }

    #[test]
    fn nc_008_empty_array_not_null() {
        assert_ne!(Value::Array(vec![]), Value::Null);
    }

    #[test]
    fn nc_009_empty_object_not_null() {
        assert_ne!(Value::Object(HashMap::new()), Value::Null);
    }

    #[test]
    fn nc_010_bool_true_not_int_one() {
        assert_ne!(Value::Bool(true), Value::Int(1));
    }

    #[test]
    fn nc_011_bool_false_not_int_zero() {
        assert_ne!(Value::Bool(false), Value::Int(0));
    }

    #[test]
    fn nc_012_string_number_not_int() {
        assert_ne!(Value::String("123".to_string()), Value::Int(123));
    }

    #[test]
    fn nc_013_no_implicit_string_to_bytes() {
        // Cannot compare String to Bytes - they are different types
        let s = Value::String("test".to_string());
        let b = Value::Bytes(b"test".to_vec());
        assert_ne!(s, b);
    }

    #[test]
    fn nc_014_no_implicit_int_promotion() {
        // Int should never be promoted to Float
        let i = Value::Int(42);
        let f = Value::Float(42.0);
        assert_ne!(i, f);

        // Type is preserved
        assert!(matches!(i, Value::Int(42)));
        assert!(matches!(f, Value::Float(_)));
    }

    #[test]
    fn nc_015_cas_respects_type_distinction() {
        // This test verifies that CAS operations fail when types differ
        // even if the "logical value" seems the same
        //
        // Expected: CAS(key, expected=Int(1), new=X) FAILS if actual value is Float(1.0)
        //
        // This is tested at the API level in facade tests
    }
}
```

### Acceptance Criteria

- [ ] All NC-* tests pass
- [ ] CAS operations fail on type mismatch even for "same logical value"
- [ ] No implicit widening (Int → Float)
- [ ] No implicit encoding (String → Bytes)
- [ ] No truthiness coercion (Bool ↔ Int)
- [ ] No nullish coercion (Null ↔ empty/zero)

---

## Story #564: Size Limits Implementation

**File**: `crates/core/src/limits.rs` (NEW)

**Deliverable**: Configurable size limits with enforcement

### Tests FIRST

```rust
#[cfg(test)]
mod size_limit_tests {
    use super::*;

    // === Key Length Tests ===

    #[test]
    fn test_key_at_max_length() {
        let limits = Limits::default();
        let key = "x".repeat(limits.max_key_bytes);
        assert!(limits.validate_key(&key).is_ok());
    }

    #[test]
    fn test_key_exceeds_max_length() {
        let limits = Limits::default();
        let key = "x".repeat(limits.max_key_bytes + 1);
        let result = limits.validate_key(&key);
        assert!(matches!(result, Err(LimitError::KeyTooLong { .. })));
    }

    #[test]
    fn test_key_much_larger_than_max() {
        let limits = Limits::default();
        let key = "x".repeat(10 * 1024); // 10KB
        let result = limits.validate_key(&key);
        assert!(matches!(result, Err(LimitError::KeyTooLong { .. })));
    }

    // === String Length Tests ===

    #[test]
    fn test_string_at_max_length() {
        let limits = Limits::default();
        let s = "x".repeat(limits.max_string_bytes);
        let value = Value::String(s);
        assert!(limits.validate_value(&value).is_ok());
    }

    #[test]
    fn test_string_exceeds_max_length() {
        let limits = Limits::default();
        let s = "x".repeat(limits.max_string_bytes + 1);
        let value = Value::String(s);
        let result = limits.validate_value(&value);
        assert!(matches!(result, Err(LimitError::ValueTooLarge { .. })));
    }

    // === Bytes Length Tests ===

    #[test]
    fn test_bytes_at_max_length() {
        let limits = Limits::default();
        let b = vec![0u8; limits.max_bytes_len];
        let value = Value::Bytes(b);
        assert!(limits.validate_value(&value).is_ok());
    }

    #[test]
    fn test_bytes_exceeds_max_length() {
        let limits = Limits::default();
        let b = vec![0u8; limits.max_bytes_len + 1];
        let value = Value::Bytes(b);
        let result = limits.validate_value(&value);
        assert!(matches!(result, Err(LimitError::ValueTooLarge { .. })));
    }

    // === Array Length Tests ===

    #[test]
    fn test_array_at_max_length() {
        let limits = Limits::with_small_limits(); // Use small limits for test speed
        let arr = vec![Value::Null; limits.max_array_len];
        let value = Value::Array(arr);
        assert!(limits.validate_value(&value).is_ok());
    }

    #[test]
    fn test_array_exceeds_max_length() {
        let limits = Limits::with_small_limits();
        let arr = vec![Value::Null; limits.max_array_len + 1];
        let value = Value::Array(arr);
        let result = limits.validate_value(&value);
        assert!(matches!(result, Err(LimitError::ValueTooLarge { .. })));
    }

    // === Object Entries Tests ===

    #[test]
    fn test_object_at_max_entries() {
        let limits = Limits::with_small_limits();
        let mut map = HashMap::new();
        for i in 0..limits.max_object_entries {
            map.insert(format!("key{}", i), Value::Null);
        }
        let value = Value::Object(map);
        assert!(limits.validate_value(&value).is_ok());
    }

    #[test]
    fn test_object_exceeds_max_entries() {
        let limits = Limits::with_small_limits();
        let mut map = HashMap::new();
        for i in 0..=limits.max_object_entries {
            map.insert(format!("key{}", i), Value::Null);
        }
        let value = Value::Object(map);
        let result = limits.validate_value(&value);
        assert!(matches!(result, Err(LimitError::ValueTooLarge { .. })));
    }

    // === Nesting Depth Tests ===

    #[test]
    fn test_nesting_at_max_depth() {
        let limits = Limits::default();
        let value = create_nested_array(limits.max_nesting_depth);
        assert!(limits.validate_value(&value).is_ok());
    }

    #[test]
    fn test_nesting_exceeds_max_depth() {
        let limits = Limits::default();
        let value = create_nested_array(limits.max_nesting_depth + 1);
        let result = limits.validate_value(&value);
        assert!(matches!(result, Err(LimitError::NestingTooDeep { .. })));
    }

    // === Vector Dimension Tests ===

    #[test]
    fn test_vector_at_max_dim() {
        let limits = Limits::default();
        let vec = vec![0.0f32; limits.max_vector_dim];
        assert!(limits.validate_vector(&vec).is_ok());
    }

    #[test]
    fn test_vector_exceeds_max_dim() {
        let limits = Limits::default();
        let vec = vec![0.0f32; limits.max_vector_dim + 1];
        let result = limits.validate_vector(&vec);
        assert!(matches!(result, Err(LimitError::VectorDimExceeded { .. })));
    }

    // === Custom Limits Tests ===

    #[test]
    fn test_custom_limits_respected() {
        let limits = Limits {
            max_key_bytes: 100,
            ..Limits::default()
        };

        let key = "x".repeat(100);
        assert!(limits.validate_key(&key).is_ok());

        let key = "x".repeat(101);
        assert!(limits.validate_key(&key).is_err());
    }

    // Helper function
    fn create_nested_array(depth: usize) -> Value {
        let mut value = Value::Null;
        for _ in 0..depth {
            value = Value::Array(vec![value]);
        }
        value
    }
}
```

### Implementation

```rust
/// Size limits for values and keys
#[derive(Debug, Clone)]
pub struct Limits {
    /// Maximum key length in bytes (default: 1024)
    pub max_key_bytes: usize,

    /// Maximum string length in bytes (default: 16MB)
    pub max_string_bytes: usize,

    /// Maximum bytes length (default: 16MB)
    pub max_bytes_len: usize,

    /// Maximum encoded value size in bytes (default: 32MB)
    pub max_value_bytes_encoded: usize,

    /// Maximum array length (default: 1M elements)
    pub max_array_len: usize,

    /// Maximum object entries (default: 1M entries)
    pub max_object_entries: usize,

    /// Maximum nesting depth (default: 128)
    pub max_nesting_depth: usize,

    /// Maximum vector dimensions (default: 8192)
    pub max_vector_dim: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_key_bytes: 1024,
            max_string_bytes: 16 * 1024 * 1024,      // 16MB
            max_bytes_len: 16 * 1024 * 1024,         // 16MB
            max_value_bytes_encoded: 32 * 1024 * 1024, // 32MB
            max_array_len: 1_000_000,
            max_object_entries: 1_000_000,
            max_nesting_depth: 128,
            max_vector_dim: 8192,
        }
    }
}

impl Limits {
    /// Create limits with small values for testing
    pub fn with_small_limits() -> Self {
        Limits {
            max_key_bytes: 100,
            max_string_bytes: 1000,
            max_bytes_len: 1000,
            max_value_bytes_encoded: 2000,
            max_array_len: 100,
            max_object_entries: 100,
            max_nesting_depth: 10,
            max_vector_dim: 100,
        }
    }

    /// Validate a key
    pub fn validate_key(&self, key: &str) -> Result<(), LimitError> {
        let len = key.len();
        if len > self.max_key_bytes {
            return Err(LimitError::KeyTooLong {
                actual: len,
                max: self.max_key_bytes,
            });
        }
        Ok(())
    }

    /// Validate a value
    pub fn validate_value(&self, value: &Value) -> Result<(), LimitError> {
        self.validate_value_impl(value, 0)
    }

    fn validate_value_impl(&self, value: &Value, depth: usize) -> Result<(), LimitError> {
        if depth > self.max_nesting_depth {
            return Err(LimitError::NestingTooDeep {
                actual: depth,
                max: self.max_nesting_depth,
            });
        }

        match value {
            Value::Null | Value::Bool(_) | Value::Int(_) | Value::Float(_) => Ok(()),

            Value::String(s) => {
                if s.len() > self.max_string_bytes {
                    return Err(LimitError::ValueTooLarge {
                        reason: "string_too_long".to_string(),
                        actual: s.len(),
                        max: self.max_string_bytes,
                    });
                }
                Ok(())
            }

            Value::Bytes(b) => {
                if b.len() > self.max_bytes_len {
                    return Err(LimitError::ValueTooLarge {
                        reason: "bytes_too_long".to_string(),
                        actual: b.len(),
                        max: self.max_bytes_len,
                    });
                }
                Ok(())
            }

            Value::Array(arr) => {
                if arr.len() > self.max_array_len {
                    return Err(LimitError::ValueTooLarge {
                        reason: "array_too_long".to_string(),
                        actual: arr.len(),
                        max: self.max_array_len,
                    });
                }
                for v in arr {
                    self.validate_value_impl(v, depth + 1)?;
                }
                Ok(())
            }

            Value::Object(obj) => {
                if obj.len() > self.max_object_entries {
                    return Err(LimitError::ValueTooLarge {
                        reason: "object_too_many_entries".to_string(),
                        actual: obj.len(),
                        max: self.max_object_entries,
                    });
                }
                for v in obj.values() {
                    self.validate_value_impl(v, depth + 1)?;
                }
                Ok(())
            }
        }
    }

    /// Validate a vector
    pub fn validate_vector(&self, vec: &[f32]) -> Result<(), LimitError> {
        if vec.len() > self.max_vector_dim {
            return Err(LimitError::VectorDimExceeded {
                actual: vec.len(),
                max: self.max_vector_dim,
            });
        }
        Ok(())
    }
}

/// Limit validation errors
#[derive(Debug, thiserror::Error)]
pub enum LimitError {
    #[error("Key too long: {actual} bytes exceeds maximum {max}")]
    KeyTooLong { actual: usize, max: usize },

    #[error("Value too large ({reason}): {actual} exceeds maximum {max}")]
    ValueTooLarge { reason: String, actual: usize, max: usize },

    #[error("Nesting too deep: {actual} levels exceeds maximum {max}")]
    NestingTooDeep { actual: usize, max: usize },

    #[error("Vector dimension exceeded: {actual} exceeds maximum {max}")]
    VectorDimExceeded { actual: usize, max: usize },
}
```

### Acceptance Criteria

- [ ] Default limits match spec (1024 key, 16MB string, 128 nesting, etc.)
- [ ] `validate_key()` enforces key length
- [ ] `validate_value()` enforces all value limits
- [ ] Nesting depth is checked recursively
- [ ] Vector dimension is validated
- [ ] Custom limits are respected
- [ ] Error types include actual and max values

---

## Story #565: Key Validation Rules

**File**: `crates/core/src/key.rs` (NEW)

**Deliverable**: Key validation with all rules from spec

### Tests FIRST

```rust
#[cfg(test)]
mod key_validation_tests {
    use super::*;

    // === Valid Keys ===

    #[test]
    fn test_valid_simple_key() {
        assert!(validate_key("mykey").is_ok());
    }

    #[test]
    fn test_valid_unicode_key() {
        assert!(validate_key("日本語キー").is_ok());
    }

    #[test]
    fn test_valid_emoji_key() {
        assert!(validate_key("🔑key🔑").is_ok());
    }

    #[test]
    fn test_valid_numeric_string_key() {
        assert!(validate_key("12345").is_ok());
    }

    #[test]
    fn test_valid_special_chars_key() {
        assert!(validate_key("a-b_c.d:e/f").is_ok());
    }

    #[test]
    fn test_valid_single_char_key() {
        assert!(validate_key("a").is_ok());
    }

    #[test]
    fn test_valid_whitespace_key() {
        // Whitespace is allowed
        assert!(validate_key("  spaces  ").is_ok());
    }

    #[test]
    fn test_valid_newline_key() {
        // Newlines are allowed
        assert!(validate_key("line1\nline2").is_ok());
    }

    #[test]
    fn test_valid_underscore_prefix() {
        // _mykey is valid (not _strata/)
        assert!(validate_key("_mykey").is_ok());
    }

    #[test]
    fn test_valid_similar_to_reserved() {
        // _stratafoo is valid (no slash after _strata)
        assert!(validate_key("_stratafoo").is_ok());
    }

    // === Invalid Keys ===

    #[test]
    fn test_invalid_empty_key() {
        let result = validate_key("");
        assert!(matches!(result, Err(KeyError::Empty)));
    }

    #[test]
    fn test_invalid_nul_byte() {
        let result = validate_key("a\x00b");
        assert!(matches!(result, Err(KeyError::ContainsNul)));
    }

    #[test]
    fn test_invalid_nul_at_start() {
        let result = validate_key("\x00abc");
        assert!(matches!(result, Err(KeyError::ContainsNul)));
    }

    #[test]
    fn test_invalid_nul_at_end() {
        let result = validate_key("abc\x00");
        assert!(matches!(result, Err(KeyError::ContainsNul)));
    }

    #[test]
    fn test_invalid_reserved_prefix() {
        let result = validate_key("_strata/foo");
        assert!(matches!(result, Err(KeyError::ReservedPrefix)));
    }

    #[test]
    fn test_invalid_reserved_prefix_exact() {
        let result = validate_key("_strata/");
        assert!(matches!(result, Err(KeyError::ReservedPrefix)));
    }

    #[test]
    fn test_invalid_too_long() {
        let key = "x".repeat(1025);
        let result = validate_key(&key);
        assert!(matches!(result, Err(KeyError::TooLong { .. })));
    }

    // === With Custom Limits ===

    #[test]
    fn test_key_with_custom_max_length() {
        let limits = Limits { max_key_bytes: 10, ..Default::default() };

        assert!(validate_key_with_limits("short", &limits).is_ok());
        assert!(validate_key_with_limits("toolongkey!", &limits).is_err());
    }
}
```

### Implementation

```rust
/// Validate a key using default limits
pub fn validate_key(key: &str) -> Result<(), KeyError> {
    validate_key_with_limits(key, &Limits::default())
}

/// Validate a key with custom limits
pub fn validate_key_with_limits(key: &str, limits: &Limits) -> Result<(), KeyError> {
    // Rule 1: Key cannot be empty
    if key.is_empty() {
        return Err(KeyError::Empty);
    }

    // Rule 2: Key cannot contain NUL bytes
    if key.contains('\x00') {
        return Err(KeyError::ContainsNul);
    }

    // Rule 3: Key cannot use reserved prefix
    if key.starts_with("_strata/") {
        return Err(KeyError::ReservedPrefix);
    }

    // Rule 4: Key cannot exceed max length
    let len = key.len();
    if len > limits.max_key_bytes {
        return Err(KeyError::TooLong {
            actual: len,
            max: limits.max_key_bytes,
        });
    }

    // Note: UTF-8 validity is guaranteed by Rust's &str type

    Ok(())
}

/// Key validation errors
#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    #[error("Key cannot be empty")]
    Empty,

    #[error("Key cannot contain NUL bytes")]
    ContainsNul,

    #[error("Key cannot use reserved prefix '_strata/'")]
    ReservedPrefix,

    #[error("Key too long: {actual} bytes exceeds maximum {max}")]
    TooLong { actual: usize, max: usize },
}
```

### Acceptance Criteria

- [ ] Empty key rejected
- [ ] NUL bytes rejected
- [ ] `_strata/` prefix rejected
- [ ] Length limit enforced
- [ ] Unicode keys allowed
- [ ] Emoji keys allowed
- [ ] Whitespace/newlines allowed
- [ ] `_mykey` allowed (not reserved)
- [ ] `_stratafoo` allowed (no slash)

---

## Testing

All tests from the Stories above, plus integration tests:

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_value_type_completeness() {
        // Ensure exactly 8 variants
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

        // Each variant has a distinct type_name
        let type_names: std::collections::HashSet<_> =
            values.iter().map(|v| v.type_name()).collect();
        assert_eq!(type_names.len(), 8);
    }

    #[test]
    fn test_deeply_nested_value() {
        let limits = Limits::default();

        // Create value at max depth
        let mut value = Value::Int(42);
        for _ in 0..limits.max_nesting_depth {
            value = Value::Array(vec![value]);
        }

        assert!(limits.validate_value(&value).is_ok());
    }

    #[test]
    fn test_complex_object_equality() {
        let v1 = Value::Object({
            let mut m = HashMap::new();
            m.insert("array".to_string(), Value::Array(vec![
                Value::Int(1),
                Value::Float(2.5),
                Value::String("three".to_string()),
            ]));
            m.insert("nested".to_string(), Value::Object({
                let mut inner = HashMap::new();
                inner.insert("key".to_string(), Value::Bytes(vec![1, 2, 3]));
                inner
            }));
            m
        });

        let v2 = v1.clone();
        assert_eq!(v1, v2);
    }
}
```

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/core/src/value.rs` | MODIFY - Finalize Value enum |
| `crates/core/src/limits.rs` | CREATE - Size limits |
| `crates/core/src/key.rs` | CREATE - Key validation |
| `crates/core/src/lib.rs` | MODIFY - Export new modules |

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-21 | Initial epic specification |
