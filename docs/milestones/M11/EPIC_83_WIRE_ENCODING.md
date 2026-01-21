# Epic 83: Wire Encoding Contract

**Goal**: Implement JSON wire encoding with special wrappers for non-JSON-native values

**Dependencies**: Epic 80 (Value Model)

**Milestone**: M11a (Core Contract & API)

---

## Test-Driven Development Protocol

> **CRITICAL**: This epic follows strict Test-Driven Development (TDD). Tests are written FIRST, then implementation.

### NEVER Modify Tests to Make Them Pass

> **ABSOLUTE RULE**: When a test fails, the problem is in the implementation, NOT the test.

**FORBIDDEN behaviors:**
- Changing test assertions to match buggy output
- Weakening test conditions
- Removing test cases that expose bugs
- Adding `#[ignore]` to failing tests

**REQUIRED behaviors:**
- Investigate WHY the test fails
- Fix the implementation to match the specification
- If the spec is wrong, get explicit approval before changing both spec AND test

---

## Scope

- JSON value encoding for all 8 types
- `$bytes` wrapper for binary data (base64)
- `$f64` wrapper for special floats (NaN, Inf, -0.0)
- `$absent` wrapper for CAS expected-missing
- Request/response envelope structure
- Version encoding (tagged union)
- Versioned<T> encoding
- Round-trip property tests

---

## Wire Encoding Rules

| Value Type | JSON Encoding | Notes |
|------------|--------------|-------|
| Null | `null` | Direct |
| Bool | `true`/`false` | Direct |
| Int | number | Full i64 range |
| Float (normal) | number | Standard JSON |
| Float (special) | `{"$f64": "..."}` | NaN, ±Inf, -0.0 |
| String | `"..."` | UTF-8 |
| Bytes | `{"$bytes": "..."}` | Standard base64 |
| Array | `[...]` | Recursive |
| Object | `{...}` | Recursive |

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #581 | JSON Value Encoding | FOUNDATION |
| #582 | $bytes Wrapper (Base64) | CRITICAL |
| #583 | $f64 Wrapper (Special Floats) | CRITICAL |
| #584 | $absent Wrapper | CRITICAL |
| #585 | Request/Response Envelope | CRITICAL |
| #586 | Version and Versioned Encoding | HIGH |

---

## Story #581: JSON Value Encoding

**File**: `crates/wire/src/json/encode.rs` (NEW)

**Deliverable**: JSON encoding for all 8 value types

### Tests FIRST

```rust
#[cfg(test)]
mod json_encoding_tests {
    use super::*;
    use crate::value::Value;

    // === Null ===

    #[test]
    fn test_encode_null() {
        let value = Value::Null;
        let json = encode_json(&value);
        assert_eq!(json, "null");
    }

    #[test]
    fn test_decode_null() {
        let value = decode_json("null").unwrap();
        assert!(matches!(value, Value::Null));
    }

    // === Bool ===

    #[test]
    fn test_encode_bool_true() {
        let value = Value::Bool(true);
        let json = encode_json(&value);
        assert_eq!(json, "true");
    }

    #[test]
    fn test_encode_bool_false() {
        let value = Value::Bool(false);
        let json = encode_json(&value);
        assert_eq!(json, "false");
    }

    #[test]
    fn test_decode_bool_true() {
        let value = decode_json("true").unwrap();
        assert!(matches!(value, Value::Bool(true)));
    }

    #[test]
    fn test_decode_bool_false() {
        let value = decode_json("false").unwrap();
        assert!(matches!(value, Value::Bool(false)));
    }

    // === Int ===

    #[test]
    fn test_encode_int_positive() {
        let value = Value::Int(123);
        let json = encode_json(&value);
        assert_eq!(json, "123");
    }

    #[test]
    fn test_encode_int_negative() {
        let value = Value::Int(-456);
        let json = encode_json(&value);
        assert_eq!(json, "-456");
    }

    #[test]
    fn test_encode_int_zero() {
        let value = Value::Int(0);
        let json = encode_json(&value);
        assert_eq!(json, "0");
    }

    #[test]
    fn test_encode_int_max() {
        let value = Value::Int(i64::MAX);
        let json = encode_json(&value);
        assert_eq!(json, "9223372036854775807");
    }

    #[test]
    fn test_encode_int_min() {
        let value = Value::Int(i64::MIN);
        let json = encode_json(&value);
        assert_eq!(json, "-9223372036854775808");
    }

    #[test]
    fn test_decode_int() {
        let value = decode_json("42").unwrap();
        assert!(matches!(value, Value::Int(42)));
    }

    #[test]
    fn test_decode_int_max() {
        let value = decode_json("9223372036854775807").unwrap();
        assert!(matches!(value, Value::Int(i64::MAX)));
    }

    // === Float (normal) ===

    #[test]
    fn test_encode_float_positive() {
        let value = Value::Float(1.5);
        let json = encode_json(&value);
        assert_eq!(json, "1.5");
    }

    #[test]
    fn test_encode_float_negative() {
        let value = Value::Float(-2.5);
        let json = encode_json(&value);
        assert_eq!(json, "-2.5");
    }

    #[test]
    fn test_encode_float_zero() {
        let value = Value::Float(0.0);
        let json = encode_json(&value);
        // Positive zero is plain JSON
        assert_eq!(json, "0.0");
    }

    #[test]
    fn test_decode_float() {
        let value = decode_json("3.14").unwrap();
        match value {
            Value::Float(f) => assert!((f - 3.14).abs() < f64::EPSILON),
            _ => panic!("Expected Float"),
        }
    }

    // === String ===

    #[test]
    fn test_encode_string_simple() {
        let value = Value::String("hello".to_string());
        let json = encode_json(&value);
        assert_eq!(json, r#""hello""#);
    }

    #[test]
    fn test_encode_string_empty() {
        let value = Value::String(String::new());
        let json = encode_json(&value);
        assert_eq!(json, r#""""#);
    }

    #[test]
    fn test_encode_string_unicode() {
        let value = Value::String("日本語".to_string());
        let json = encode_json(&value);
        assert_eq!(json, r#""日本語""#);
    }

    #[test]
    fn test_encode_string_escapes() {
        let value = Value::String("a\n\t\"b".to_string());
        let json = encode_json(&value);
        assert_eq!(json, r#""a\n\t\"b""#);
    }

    #[test]
    fn test_decode_string() {
        let value = decode_json(r#""hello""#).unwrap();
        assert!(matches!(value, Value::String(s) if s == "hello"));
    }

    // === Array ===

    #[test]
    fn test_encode_array_simple() {
        let value = Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        let json = encode_json(&value);
        assert_eq!(json, "[1,2,3]");
    }

    #[test]
    fn test_encode_array_empty() {
        let value = Value::Array(vec![]);
        let json = encode_json(&value);
        assert_eq!(json, "[]");
    }

    #[test]
    fn test_encode_array_nested() {
        let value = Value::Array(vec![
            Value::Array(vec![Value::Int(1)]),
        ]);
        let json = encode_json(&value);
        assert_eq!(json, "[[1]]");
    }

    #[test]
    fn test_encode_array_mixed_types() {
        let value = Value::Array(vec![
            Value::Int(1),
            Value::String("a".to_string()),
            Value::Bool(true),
        ]);
        let json = encode_json(&value);
        assert_eq!(json, r#"[1,"a",true]"#);
    }

    #[test]
    fn test_decode_array() {
        let value = decode_json("[1,2,3]").unwrap();
        match value {
            Value::Array(arr) => assert_eq!(arr.len(), 3),
            _ => panic!("Expected Array"),
        }
    }

    // === Object ===

    #[test]
    fn test_encode_object_simple() {
        let mut map = std::collections::HashMap::new();
        map.insert("a".to_string(), Value::Int(1));
        let value = Value::Object(map);
        let json = encode_json(&value);
        assert_eq!(json, r#"{"a":1}"#);
    }

    #[test]
    fn test_encode_object_empty() {
        let value = Value::Object(std::collections::HashMap::new());
        let json = encode_json(&value);
        assert_eq!(json, "{}");
    }

    #[test]
    fn test_encode_object_nested() {
        let mut inner = std::collections::HashMap::new();
        inner.insert("b".to_string(), Value::Int(1));
        let mut outer = std::collections::HashMap::new();
        outer.insert("a".to_string(), Value::Object(inner));
        let value = Value::Object(outer);
        let json = encode_json(&value);
        assert_eq!(json, r#"{"a":{"b":1}}"#);
    }

    #[test]
    fn test_decode_object() {
        let value = decode_json(r#"{"key":"value"}"#).unwrap();
        match value {
            Value::Object(map) => {
                assert_eq!(map.get("key"), Some(&Value::String("value".to_string())));
            }
            _ => panic!("Expected Object"),
        }
    }

    // === Round-Trip Property Tests ===

    #[test]
    fn test_round_trip_null() {
        let original = Value::Null;
        let json = encode_json(&original);
        let decoded = decode_json(&json).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn test_round_trip_bool() {
        for b in [true, false] {
            let original = Value::Bool(b);
            let json = encode_json(&original);
            let decoded = decode_json(&json).unwrap();
            assert_eq!(original, decoded);
        }
    }

    #[test]
    fn test_round_trip_int() {
        for i in [0, 1, -1, i64::MAX, i64::MIN, 42, -999] {
            let original = Value::Int(i);
            let json = encode_json(&original);
            let decoded = decode_json(&json).unwrap();
            assert_eq!(original, decoded);
        }
    }
}
```

### Implementation

```rust
use crate::value::Value;
use std::collections::HashMap;

/// Encode a Value to JSON string
pub fn encode_json(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => encode_float(*f),
        Value::String(s) => encode_string(s),
        Value::Bytes(b) => encode_bytes(b),
        Value::Array(arr) => encode_array(arr),
        Value::Object(obj) => encode_object(obj),
    }
}

fn encode_float(f: f64) -> String {
    if f.is_nan() {
        r#"{"$f64":"NaN"}"#.to_string()
    } else if f == f64::INFINITY {
        r#"{"$f64":"+Inf"}"#.to_string()
    } else if f == f64::NEG_INFINITY {
        r#"{"$f64":"-Inf"}"#.to_string()
    } else if f == 0.0 && f.is_sign_negative() {
        r#"{"$f64":"-0.0"}"#.to_string()
    } else {
        // Normal float - use JSON representation
        if f.fract() == 0.0 && f.abs() < 1e15 {
            format!("{}.0", f)
        } else {
            f.to_string()
        }
    }
}

fn encode_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 2);
    result.push('"');
    for c in s.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c.is_control() => {
                result.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => result.push(c),
        }
    }
    result.push('"');
    result
}

fn encode_bytes(bytes: &[u8]) -> String {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    format!(r#"{{"$bytes":"{}"}}"#, b64)
}

fn encode_array(arr: &[Value]) -> String {
    let elements: Vec<String> = arr.iter().map(encode_json).collect();
    format!("[{}]", elements.join(","))
}

fn encode_object(obj: &HashMap<String, Value>) -> String {
    // Sort keys for deterministic output
    let mut entries: Vec<_> = obj.iter().collect();
    entries.sort_by_key(|(k, _)| *k);

    let pairs: Vec<String> = entries
        .iter()
        .map(|(k, v)| format!("{}:{}", encode_string(k), encode_json(v)))
        .collect();

    format!("{{{}}}", pairs.join(","))
}

/// Decode a JSON string to Value
pub fn decode_json(json: &str) -> Result<Value, DecodeError> {
    let trimmed = json.trim();

    // Null
    if trimmed == "null" {
        return Ok(Value::Null);
    }

    // Bool
    if trimmed == "true" {
        return Ok(Value::Bool(true));
    }
    if trimmed == "false" {
        return Ok(Value::Bool(false));
    }

    // String
    if trimmed.starts_with('"') {
        return decode_string(trimmed);
    }

    // Array
    if trimmed.starts_with('[') {
        return decode_array(trimmed);
    }

    // Object (including special wrappers)
    if trimmed.starts_with('{') {
        return decode_object_or_wrapper(trimmed);
    }

    // Number (Int or Float)
    decode_number(trimmed)
}

fn decode_number(s: &str) -> Result<Value, DecodeError> {
    // Try Int first
    if let Ok(i) = s.parse::<i64>() {
        return Ok(Value::Int(i));
    }

    // Try Float
    if let Ok(f) = s.parse::<f64>() {
        return Ok(Value::Float(f));
    }

    Err(DecodeError::InvalidNumber(s.to_string()))
}

fn decode_object_or_wrapper(s: &str) -> Result<Value, DecodeError> {
    // Parse as generic JSON object first
    let obj = parse_json_object(s)?;

    // Check for special wrappers
    if obj.len() == 1 {
        if let Some(Value::String(b64)) = obj.get("$bytes") {
            return decode_bytes_wrapper(b64);
        }
        if let Some(Value::String(f64_str)) = obj.get("$f64") {
            return decode_f64_wrapper(f64_str);
        }
        if let Some(Value::Bool(true)) = obj.get("$absent") {
            return Ok(Value::Null); // $absent is used for CAS, decoded as Null here
        }
    }

    Ok(Value::Object(obj))
}

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("Invalid JSON: {0}")]
    InvalidJson(String),

    #[error("Invalid number: {0}")]
    InvalidNumber(String),

    #[error("Invalid base64: {0}")]
    InvalidBase64(String),

    #[error("Invalid $f64 value: {0}")]
    InvalidF64Wrapper(String),
}
```

### Acceptance Criteria

- [ ] All 8 value types encode correctly
- [ ] All 8 value types decode correctly
- [ ] Round-trip preserves values exactly
- [ ] Strings are properly escaped
- [ ] Objects have deterministic key ordering
- [ ] Large integers (i64 range) are preserved

---

## Story #582: $bytes Wrapper (Base64)

**File**: `crates/wire/src/json/bytes.rs` (NEW)

**Deliverable**: Base64 encoding for Bytes values

### Tests FIRST

```rust
#[cfg(test)]
mod bytes_encoding_tests {
    use super::*;

    #[test]
    fn test_bytes_wrapper_structure() {
        let value = Value::Bytes(vec![72, 101, 108, 108, 111]); // "Hello"
        let json = encode_json(&value);

        // Must be object with single $bytes key
        assert!(json.starts_with(r#"{"$bytes":"#));
        assert!(json.ends_with(r#""}"#));
    }

    #[test]
    fn test_bytes_wrapper_base64_standard() {
        let value = Value::Bytes(vec![72, 101, 108, 108, 111]); // "Hello"
        let json = encode_json(&value);

        assert_eq!(json, r#"{"$bytes":"SGVsbG8="}"#);
    }

    #[test]
    fn test_bytes_wrapper_base64_padding() {
        // Single byte needs padding
        let value = Value::Bytes(vec![65]); // "A"
        let json = encode_json(&value);

        // Standard base64 with padding
        assert_eq!(json, r#"{"$bytes":"QQ=="}"#);
    }

    #[test]
    fn test_bytes_empty() {
        let value = Value::Bytes(vec![]);
        let json = encode_json(&value);

        assert_eq!(json, r#"{"$bytes":""}"#);
    }

    #[test]
    fn test_bytes_all_values() {
        let all: Vec<u8> = (0..=255).collect();
        let value = Value::Bytes(all.clone());

        let json = encode_json(&value);
        let decoded = decode_json(&json).unwrap();

        assert!(matches!(decoded, Value::Bytes(ref b) if b == &all));
    }

    #[test]
    fn test_bytes_round_trip() {
        let original = Value::Bytes(vec![0, 127, 255, 1, 254]);
        let json = encode_json(&original);
        let decoded = decode_json(&json).unwrap();

        assert_eq!(original, decoded);
    }

    #[test]
    fn test_bytes_in_object() {
        let mut map = HashMap::new();
        map.insert("data".to_string(), Value::Bytes(vec![1, 2, 3]));
        let value = Value::Object(map);

        let json = encode_json(&value);
        assert!(json.contains(r#""data":{"$bytes":"#));

        let decoded = decode_json(&json).unwrap();
        match decoded {
            Value::Object(m) => {
                assert!(matches!(m.get("data"), Some(Value::Bytes(_))));
            }
            _ => panic!("Expected Object"),
        }
    }

    #[test]
    fn test_bytes_in_array() {
        let value = Value::Array(vec![
            Value::Bytes(vec![1]),
            Value::Bytes(vec![2]),
        ]);

        let json = encode_json(&value);
        let decoded = decode_json(&json).unwrap();

        match decoded {
            Value::Array(arr) => {
                assert_eq!(arr.len(), 2);
                assert!(matches!(&arr[0], Value::Bytes(_)));
                assert!(matches!(&arr[1], Value::Bytes(_)));
            }
            _ => panic!("Expected Array"),
        }
    }

    #[test]
    fn test_bytes_wrapper_collision() {
        // Object with "$bytes" key but not a wrapper
        // (has additional keys or value is not a string)
        let mut map = HashMap::new();
        map.insert("$bytes".to_string(), Value::Int(123));
        let value = Value::Object(map);

        let json = encode_json(&value);
        let decoded = decode_json(&json).unwrap();

        // Should decode as Object, not Bytes
        assert!(matches!(decoded, Value::Object(_)));
    }
}
```

### Implementation

```rust
fn encode_bytes(bytes: &[u8]) -> String {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    format!(r#"{{"$bytes":"{}"}}"#, b64)
}

fn decode_bytes_wrapper(b64: &str) -> Result<Value, DecodeError> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| DecodeError::InvalidBase64(e.to_string()))?;
    Ok(Value::Bytes(bytes))
}
```

### Acceptance Criteria

- [ ] Bytes encode as `{"$bytes": "<base64>"}`
- [ ] Uses standard base64 alphabet (not URL-safe)
- [ ] Includes padding (=)
- [ ] Empty bytes encode as `{"$bytes": ""}`
- [ ] All byte values (0-255) round-trip correctly
- [ ] Wrapper collision is handled (object with $bytes key that isn't a wrapper)

---

## Story #583: $f64 Wrapper (Special Floats)

**File**: `crates/wire/src/json/float.rs` (NEW)

**Deliverable**: Wrapper encoding for NaN, Infinity, and -0.0

### Tests FIRST

```rust
#[cfg(test)]
mod f64_wrapper_tests {
    use super::*;

    #[test]
    fn test_nan_wrapper() {
        let value = Value::Float(f64::NAN);
        let json = encode_json(&value);

        assert_eq!(json, r#"{"$f64":"NaN"}"#);
    }

    #[test]
    fn test_positive_inf_wrapper() {
        let value = Value::Float(f64::INFINITY);
        let json = encode_json(&value);

        assert_eq!(json, r#"{"$f64":"+Inf"}"#);
    }

    #[test]
    fn test_negative_inf_wrapper() {
        let value = Value::Float(f64::NEG_INFINITY);
        let json = encode_json(&value);

        assert_eq!(json, r#"{"$f64":"-Inf"}"#);
    }

    #[test]
    fn test_negative_zero_wrapper() {
        let value = Value::Float(-0.0);
        let json = encode_json(&value);

        assert_eq!(json, r#"{"$f64":"-0.0"}"#);
    }

    #[test]
    fn test_positive_zero_no_wrapper() {
        let value = Value::Float(0.0);
        let json = encode_json(&value);

        // Positive zero is plain JSON, no wrapper
        assert_eq!(json, "0.0");
        assert!(!json.contains("$f64"));
    }

    #[test]
    fn test_normal_float_no_wrapper() {
        let value = Value::Float(1.5);
        let json = encode_json(&value);

        // Normal floats are plain JSON
        assert_eq!(json, "1.5");
        assert!(!json.contains("$f64"));
    }

    #[test]
    fn test_decode_nan_wrapper() {
        let value = decode_json(r#"{"$f64":"NaN"}"#).unwrap();

        match value {
            Value::Float(f) => assert!(f.is_nan()),
            _ => panic!("Expected Float"),
        }
    }

    #[test]
    fn test_decode_positive_inf_wrapper() {
        let value = decode_json(r#"{"$f64":"+Inf"}"#).unwrap();

        assert!(matches!(value, Value::Float(f) if f == f64::INFINITY));
    }

    #[test]
    fn test_decode_negative_inf_wrapper() {
        let value = decode_json(r#"{"$f64":"-Inf"}"#).unwrap();

        assert!(matches!(value, Value::Float(f) if f == f64::NEG_INFINITY));
    }

    #[test]
    fn test_decode_negative_zero_wrapper() {
        let value = decode_json(r#"{"$f64":"-0.0"}"#).unwrap();

        match value {
            Value::Float(f) => {
                assert_eq!(f, 0.0);
                assert!(f.is_sign_negative());
            }
            _ => panic!("Expected Float"),
        }
    }

    #[test]
    fn test_nan_round_trip() {
        let original = Value::Float(f64::NAN);
        let json = encode_json(&original);
        let decoded = decode_json(&json).unwrap();

        // NaN round-trips to NaN (but NaN != NaN)
        match decoded {
            Value::Float(f) => assert!(f.is_nan()),
            _ => panic!("Expected Float"),
        }
    }

    #[test]
    fn test_negative_zero_round_trip() {
        let original = Value::Float(-0.0);
        let json = encode_json(&original);
        let decoded = decode_json(&json).unwrap();

        match decoded {
            Value::Float(f) => {
                assert!(f.is_sign_negative());
            }
            _ => panic!("Expected Float"),
        }
    }

    #[test]
    fn test_f64_wrapper_invalid_value() {
        let result = decode_json(r#"{"$f64":"invalid"}"#);

        assert!(matches!(result, Err(DecodeError::InvalidF64Wrapper(_))));
    }

    #[test]
    fn test_float_precision_preserved() {
        // Value that needs full f64 precision
        let precise = 1.0000000000000002_f64;
        let value = Value::Float(precise);

        let json = encode_json(&value);
        let decoded = decode_json(&json).unwrap();

        match decoded {
            Value::Float(f) => {
                assert_eq!(f.to_bits(), precise.to_bits());
            }
            _ => panic!("Expected Float"),
        }
    }
}
```

### Implementation

```rust
fn encode_float(f: f64) -> String {
    if f.is_nan() {
        r#"{"$f64":"NaN"}"#.to_string()
    } else if f == f64::INFINITY {
        r#"{"$f64":"+Inf"}"#.to_string()
    } else if f == f64::NEG_INFINITY {
        r#"{"$f64":"-Inf"}"#.to_string()
    } else if f == 0.0 && f.is_sign_negative() {
        r#"{"$f64":"-0.0"}"#.to_string()
    } else {
        // Normal float
        format_normal_float(f)
    }
}

fn format_normal_float(f: f64) -> String {
    // Ensure we always have a decimal point for floats
    let s = f.to_string();
    if s.contains('.') || s.contains('e') || s.contains('E') {
        s
    } else {
        format!("{}.0", s)
    }
}

fn decode_f64_wrapper(value: &str) -> Result<Value, DecodeError> {
    let f = match value {
        "NaN" => f64::NAN,
        "+Inf" => f64::INFINITY,
        "-Inf" => f64::NEG_INFINITY,
        "-0.0" => -0.0_f64,
        _ => return Err(DecodeError::InvalidF64Wrapper(value.to_string())),
    };
    Ok(Value::Float(f))
}
```

### Acceptance Criteria

- [ ] NaN encodes as `{"$f64":"NaN"}`
- [ ] +Inf encodes as `{"$f64":"+Inf"}`
- [ ] -Inf encodes as `{"$f64":"-Inf"}`
- [ ] -0.0 encodes as `{"$f64":"-0.0"}`
- [ ] +0.0 encodes as plain `0.0` (no wrapper)
- [ ] Normal floats encode as plain JSON
- [ ] All special floats round-trip correctly
- [ ] Full f64 precision is preserved

---

## Story #584: $absent Wrapper

**File**: `crates/wire/src/json/absent.rs` (NEW)

**Deliverable**: Wrapper for CAS expected-missing sentinel

### Tests FIRST

```rust
#[cfg(test)]
mod absent_wrapper_tests {
    use super::*;

    #[test]
    fn test_absent_wrapper_structure() {
        let json = encode_absent();
        assert_eq!(json, r#"{"$absent":true}"#);
    }

    #[test]
    fn test_absent_wrapper_value_is_bool_true() {
        let json = encode_absent();

        // Value must be boolean true, not 1, not "true"
        assert!(json.contains("true"));
        assert!(!json.contains("\"true\""));
        assert!(!json.contains(":1"));
    }

    #[test]
    fn test_decode_absent_wrapper() {
        let result = decode_json(r#"{"$absent":true}"#).unwrap();

        // $absent decodes to special Absent marker
        assert!(is_absent(&result));
    }

    #[test]
    fn test_absent_used_in_cas() {
        // $absent is used in CAS to indicate "expected key does not exist"
        // When expected is $absent, CAS succeeds only if key is missing

        let expected_json = r#"{"expected":{"$absent":true},"new":123}"#;

        // This would be parsed by CAS operation
        // The test verifies the wire format is correct
        assert!(expected_json.contains(r#"{"$absent":true}"#));
    }

    #[test]
    fn test_absent_wrapper_collision() {
        // Object with $absent key but wrong value type
        let json = r#"{"$absent":"not_bool"}"#;
        let result = decode_json(json).unwrap();

        // Should decode as regular object, not absent marker
        assert!(matches!(result, Value::Object(_)));
        assert!(!is_absent(&result));
    }

    #[test]
    fn test_absent_wrapper_collision_false() {
        // Object with $absent: false - not the marker
        let json = r#"{"$absent":false}"#;
        let result = decode_json(json).unwrap();

        // false is not the absent marker
        assert!(matches!(result, Value::Object(_)));
        assert!(!is_absent(&result));
    }
}
```

### Implementation

```rust
/// Marker value indicating "absent" in CAS operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Absent;

/// Encode the absent marker
pub fn encode_absent() -> String {
    r#"{"$absent":true}"#.to_string()
}

/// Check if a decoded value represents absent
pub fn is_absent(value: &Value) -> bool {
    match value {
        Value::Object(map) if map.len() == 1 => {
            matches!(map.get("$absent"), Some(Value::Bool(true)))
        }
        _ => false,
    }
}

/// Decode $absent wrapper
fn decode_absent_wrapper(value: &Value) -> Option<Absent> {
    match value {
        Value::Bool(true) => Some(Absent),
        _ => None,
    }
}
```

### Acceptance Criteria

- [ ] $absent encodes as `{"$absent":true}`
- [ ] Value is boolean `true`, not `1` or `"true"`
- [ ] `is_absent()` correctly identifies absent marker
- [ ] Collision with `$absent: false` is handled
- [ ] Collision with `$absent: "string"` is handled

---

## Story #585: Request/Response Envelope

**File**: `crates/wire/src/json/envelope.rs` (NEW)

**Deliverable**: Request and response envelope structures

### Tests FIRST

```rust
#[cfg(test)]
mod envelope_tests {
    use super::*;

    // === Request Envelope ===

    #[test]
    fn test_request_envelope_structure() {
        let request = Request {
            id: "req-123".to_string(),
            op: "kv_get".to_string(),
            params: RequestParams::KvGet {
                run_id: "default".to_string(),
                key: "mykey".to_string(),
            },
        };

        let json = encode_request(&request);

        // Must have id, op, params
        assert!(json.contains(r#""id":"req-123""#));
        assert!(json.contains(r#""op":"kv_get""#));
        assert!(json.contains(r#""params":"#));
    }

    #[test]
    fn test_request_envelope_id_string() {
        let request = Request {
            id: "test-id".to_string(),
            op: "ping".to_string(),
            params: RequestParams::Ping,
        };

        let json = encode_request(&request);

        // ID must be a string, not a number
        assert!(json.contains(r#""id":"test-id""#));
    }

    // === Success Response ===

    #[test]
    fn test_success_response_structure() {
        let response = Response::success("req-123", Value::Int(42));

        let json = encode_response(&response);

        // Must have id, ok=true, result
        assert!(json.contains(r#""id":"req-123""#));
        assert!(json.contains(r#""ok":true"#));
        assert!(json.contains(r#""result":42"#));
    }

    #[test]
    fn test_success_response_ok_is_bool() {
        let response = Response::success("test", Value::Null);

        let json = encode_response(&response);

        // ok must be boolean true, not 1 or "true"
        assert!(json.contains(r#""ok":true"#));
        assert!(!json.contains(r#""ok":1"#));
        assert!(!json.contains(r#""ok":"true""#));
    }

    // === Error Response ===

    #[test]
    fn test_error_response_structure() {
        let response = Response::error(
            "req-123",
            ApiError {
                code: "NotFound".to_string(),
                message: "Key not found".to_string(),
                details: None,
            },
        );

        let json = encode_response(&response);

        // Must have id, ok=false, error
        assert!(json.contains(r#""id":"req-123""#));
        assert!(json.contains(r#""ok":false"#));
        assert!(json.contains(r#""error":"#));
    }

    #[test]
    fn test_error_response_ok_is_bool_false() {
        let response = Response::error(
            "test",
            ApiError {
                code: "Error".to_string(),
                message: "msg".to_string(),
                details: None,
            },
        );

        let json = encode_response(&response);

        // ok must be boolean false, not 0 or "false"
        assert!(json.contains(r#""ok":false"#));
        assert!(!json.contains(r#""ok":0"#));
        assert!(!json.contains(r#""ok":"false""#));
    }

    #[test]
    fn test_error_response_error_structure() {
        let response = Response::error(
            "req-123",
            ApiError {
                code: "WrongType".to_string(),
                message: "Expected Int".to_string(),
                details: Some(Value::Object({
                    let mut m = HashMap::new();
                    m.insert("expected".to_string(), Value::String("Int".into()));
                    m.insert("actual".to_string(), Value::String("Float".into()));
                    m
                })),
            },
        );

        let json = encode_response(&response);

        // error must have code, message, details
        assert!(json.contains(r#""code":"WrongType""#));
        assert!(json.contains(r#""message":"Expected Int""#));
        assert!(json.contains(r#""details":"#));
    }

    // === ID Preservation ===

    #[test]
    fn test_request_id_preserved_in_response() {
        let request_id = "unique-request-id-12345";

        let request = Request {
            id: request_id.to_string(),
            op: "ping".to_string(),
            params: RequestParams::Ping,
        };

        let response = Response::success(&request.id, Value::Null);

        assert_eq!(response.id, request_id);
    }
}
```

### Implementation

```rust
/// Wire protocol request
#[derive(Debug, Clone)]
pub struct Request {
    /// Request ID (echoed in response)
    pub id: String,

    /// Operation name
    pub op: String,

    /// Operation parameters
    pub params: RequestParams,
}

/// Wire protocol response
#[derive(Debug, Clone)]
pub struct Response {
    /// Request ID (from request)
    pub id: String,

    /// Success or failure
    pub ok: bool,

    /// Result (if ok=true)
    pub result: Option<Value>,

    /// Error (if ok=false)
    pub error: Option<ApiError>,
}

/// API error structure
#[derive(Debug, Clone)]
pub struct ApiError {
    /// Error code (e.g., "NotFound", "WrongType")
    pub code: String,

    /// Human-readable message
    pub message: String,

    /// Additional error details
    pub details: Option<Value>,
}

impl Response {
    pub fn success(id: &str, result: Value) -> Self {
        Response {
            id: id.to_string(),
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: &str, error: ApiError) -> Self {
        Response {
            id: id.to_string(),
            ok: false,
            result: None,
            error: Some(error),
        }
    }
}

pub fn encode_request(request: &Request) -> String {
    format!(
        r#"{{"id":{},"op":{},"params":{}}}"#,
        encode_string(&request.id),
        encode_string(&request.op),
        encode_params(&request.params),
    )
}

pub fn encode_response(response: &Response) -> String {
    if response.ok {
        format!(
            r#"{{"id":{},"ok":true,"result":{}}}"#,
            encode_string(&response.id),
            encode_json(response.result.as_ref().unwrap_or(&Value::Null)),
        )
    } else {
        let error = response.error.as_ref().unwrap();
        format!(
            r#"{{"id":{},"ok":false,"error":{{"code":{},"message":{},"details":{}}}}}"#,
            encode_string(&response.id),
            encode_string(&error.code),
            encode_string(&error.message),
            error.details.as_ref().map(encode_json).unwrap_or_else(|| "null".to_string()),
        )
    }
}
```

### Acceptance Criteria

- [ ] Request has id, op, params fields
- [ ] Success response has id, ok=true, result
- [ ] Error response has id, ok=false, error
- [ ] ok field is boolean (true/false), not int or string
- [ ] error has code, message, details fields
- [ ] Request ID is preserved in response

---

## Story #586: Version and Versioned Encoding

**File**: `crates/wire/src/json/version.rs` (NEW)

**Deliverable**: Encoding for Version (tagged union) and Versioned<T>

### Tests FIRST

```rust
#[cfg(test)]
mod version_encoding_tests {
    use super::*;

    // === Version Encoding ===

    #[test]
    fn test_encode_txn_version() {
        let version = Version::Txn(123);
        let json = encode_version(&version);

        assert_eq!(json, r#"{"type":"txn","value":123}"#);
    }

    #[test]
    fn test_encode_sequence_version() {
        let version = Version::Sequence(456);
        let json = encode_version(&version);

        assert_eq!(json, r#"{"type":"sequence","value":456}"#);
    }

    #[test]
    fn test_encode_counter_version() {
        let version = Version::Counter(789);
        let json = encode_version(&version);

        assert_eq!(json, r#"{"type":"counter","value":789}"#);
    }

    #[test]
    fn test_encode_version_zero() {
        let version = Version::Txn(0);
        let json = encode_version(&version);

        assert_eq!(json, r#"{"type":"txn","value":0}"#);
    }

    #[test]
    fn test_encode_version_max() {
        let version = Version::Txn(u64::MAX);
        let json = encode_version(&version);

        assert!(json.contains("18446744073709551615"));
    }

    // === Version Decoding ===

    #[test]
    fn test_decode_txn_version() {
        let version = decode_version(r#"{"type":"txn","value":123}"#).unwrap();

        assert!(matches!(version, Version::Txn(123)));
    }

    #[test]
    fn test_decode_sequence_version() {
        let version = decode_version(r#"{"type":"sequence","value":456}"#).unwrap();

        assert!(matches!(version, Version::Sequence(456)));
    }

    #[test]
    fn test_decode_counter_version() {
        let version = decode_version(r#"{"type":"counter","value":789}"#).unwrap();

        assert!(matches!(version, Version::Counter(789)));
    }

    #[test]
    fn test_decode_invalid_version_type() {
        let result = decode_version(r#"{"type":"invalid","value":1}"#);

        assert!(matches!(result, Err(DecodeError::InvalidVersionType(_))));
    }

    #[test]
    fn test_version_round_trip() {
        for version in [
            Version::Txn(0),
            Version::Txn(123),
            Version::Sequence(456),
            Version::Counter(789),
            Version::Txn(u64::MAX),
        ] {
            let json = encode_version(&version);
            let decoded = decode_version(&json).unwrap();
            assert_eq!(version, decoded);
        }
    }

    // === Versioned<T> Encoding ===

    #[test]
    fn test_versioned_structure() {
        let versioned = Versioned {
            value: Value::Int(42),
            version: Version::Txn(100),
            timestamp: 1234567890,
        };

        let json = encode_versioned(&versioned);

        // Must have value, version, timestamp
        assert!(json.contains(r#""value":42"#));
        assert!(json.contains(r#""version":{"type":"txn","value":100}"#));
        assert!(json.contains(r#""timestamp":1234567890"#));
    }

    #[test]
    fn test_versioned_timestamp_is_u64() {
        let versioned = Versioned {
            value: Value::Null,
            version: Version::Txn(1),
            timestamp: 1234567890123456, // Microseconds
        };

        let json = encode_versioned(&versioned);

        // Timestamp must be a number, not a string
        assert!(json.contains(r#""timestamp":1234567890123456"#));
        assert!(!json.contains(r#""timestamp":"1234567890123456""#));
    }

    #[test]
    fn test_versioned_with_complex_value() {
        let mut map = HashMap::new();
        map.insert("nested".to_string(), Value::Array(vec![Value::Int(1), Value::Int(2)]));

        let versioned = Versioned {
            value: Value::Object(map),
            version: Version::Txn(50),
            timestamp: 999,
        };

        let json = encode_versioned(&versioned);
        let decoded = decode_versioned(&json).unwrap();

        assert_eq!(decoded.version, versioned.version);
        assert_eq!(decoded.timestamp, versioned.timestamp);
    }

    #[test]
    fn test_versioned_round_trip() {
        let versioned = Versioned {
            value: Value::String("test".to_string()),
            version: Version::Sequence(42),
            timestamp: 1000000,
        };

        let json = encode_versioned(&versioned);
        let decoded = decode_versioned(&json).unwrap();

        assert_eq!(decoded.value, versioned.value);
        assert_eq!(decoded.version, versioned.version);
        assert_eq!(decoded.timestamp, versioned.timestamp);
    }
}
```

### Implementation

```rust
/// Version type (tagged union)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Version {
    /// Transaction version (for KV, JSON)
    Txn(u64),

    /// Sequence version (for Events)
    Sequence(u64),

    /// Counter version (for State/CAS)
    Counter(u64),
}

impl Version {
    pub fn value(&self) -> u64 {
        match self {
            Version::Txn(v) | Version::Sequence(v) | Version::Counter(v) => *v,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Version::Txn(_) => "txn",
            Version::Sequence(_) => "sequence",
            Version::Counter(_) => "counter",
        }
    }
}

/// Versioned value with metadata
#[derive(Debug, Clone)]
pub struct Versioned<T> {
    pub value: T,
    pub version: Version,
    pub timestamp: u64, // Microseconds since epoch
}

pub fn encode_version(version: &Version) -> String {
    format!(
        r#"{{"type":"{}","value":{}}}"#,
        version.type_name(),
        version.value(),
    )
}

pub fn decode_version(json: &str) -> Result<Version, DecodeError> {
    // Parse JSON object
    let obj = parse_json_object(json)?;

    let type_str = match obj.get("type") {
        Some(Value::String(s)) => s.as_str(),
        _ => return Err(DecodeError::InvalidVersionType("missing type".to_string())),
    };

    let value = match obj.get("value") {
        Some(Value::Int(v)) => *v as u64,
        _ => return Err(DecodeError::InvalidVersionType("missing value".to_string())),
    };

    match type_str {
        "txn" => Ok(Version::Txn(value)),
        "sequence" => Ok(Version::Sequence(value)),
        "counter" => Ok(Version::Counter(value)),
        _ => Err(DecodeError::InvalidVersionType(type_str.to_string())),
    }
}

pub fn encode_versioned<T: Into<Value> + Clone>(versioned: &Versioned<T>) -> String {
    format!(
        r#"{{"value":{},"version":{},"timestamp":{}}}"#,
        encode_json(&versioned.value.clone().into()),
        encode_version(&versioned.version),
        versioned.timestamp,
    )
}
```

### Acceptance Criteria

- [ ] Version::Txn encodes as `{"type":"txn","value":N}`
- [ ] Version::Sequence encodes as `{"type":"sequence","value":N}`
- [ ] Version::Counter encodes as `{"type":"counter","value":N}`
- [ ] Invalid version type returns error
- [ ] Versioned<T> has value, version, timestamp fields
- [ ] Timestamp is u64 (microseconds)
- [ ] All versions round-trip correctly

---

## Testing

Property-based tests for encoding:

```rust
#[cfg(test)]
mod property_tests {
    use super::*;
    use quickcheck::{quickcheck, TestResult};

    quickcheck! {
        fn prop_value_round_trip(v: Value) -> bool {
            let encoded = encode_json(&v);
            let decoded = decode_json(&encoded);

            match decoded {
                Ok(d) => values_equal(&v, &d),
                Err(_) => false,
            }
        }

        fn prop_version_round_trip(v: Version) -> bool {
            let encoded = encode_version(&v);
            let decoded = decode_version(&encoded);

            match decoded {
                Ok(d) => v == d,
                Err(_) => false,
            }
        }

        fn prop_float_round_trip(f: f64) -> bool {
            let value = Value::Float(f);
            let encoded = encode_json(&value);
            let decoded = decode_json(&encoded);

            match decoded {
                Ok(Value::Float(d)) => {
                    if f.is_nan() {
                        d.is_nan()
                    } else {
                        f == d
                    }
                }
                _ => false,
            }
        }
    }

    fn values_equal(a: &Value, b: &Value) -> bool {
        match (a, b) {
            (Value::Float(f1), Value::Float(f2)) => {
                if f1.is_nan() && f2.is_nan() {
                    true
                } else {
                    f1 == f2
                }
            }
            _ => a == b,
        }
    }
}
```

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/wire/src/lib.rs` | CREATE - Wire crate entry |
| `crates/wire/src/json/mod.rs` | CREATE - JSON module |
| `crates/wire/src/json/encode.rs` | CREATE - Value encoding |
| `crates/wire/src/json/decode.rs` | CREATE - Value decoding |
| `crates/wire/src/json/bytes.rs` | CREATE - $bytes wrapper |
| `crates/wire/src/json/float.rs` | CREATE - $f64 wrapper |
| `crates/wire/src/json/absent.rs` | CREATE - $absent wrapper |
| `crates/wire/src/json/envelope.rs` | CREATE - Request/response |
| `crates/wire/src/json/version.rs` | CREATE - Version encoding |
| `Cargo.toml` | MODIFY - Add wire crate |

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-21 | Initial epic specification |
