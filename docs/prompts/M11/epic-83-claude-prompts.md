# Epic 83: Wire Encoding Contract - Implementation Prompts

**Epic Goal**: Implement JSON wire encoding with special wrappers for non-JSON-native values

**GitHub Issue**: [#573](https://github.com/anibjoshi/in-mem/issues/573)
**Status**: Ready after Epic 80
**Dependencies**: Epic 80 (Value Model)
**Phase**: 1 (Data Model Foundation)

---

## NAMING CONVENTION - CRITICAL

> **NEVER use "M11" in the actual codebase or comments.**
>
> - "Strata" IS allowed (e.g., `strata_wire`, `StrataEncoder`)
>
> **CORRECT**: `//! Strata wire encoding for JSON`
> **WRONG**: `//! M11 wire format`

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

Before starting ANY story in this epic, read:
1. **Contract Spec**: `docs/milestones/M11/M11_CONTRACT.md`
2. **Epic Spec**: `docs/milestones/M11/EPIC_83_WIRE_ENCODING.md`
3. **Prompt Header**: `docs/prompts/M11/M11_PROMPT_HEADER.md`

---

## Epic 83 Overview

### Scope
- JSON value encoding for all 8 types
- `$bytes` wrapper for binary data (base64)
- `$f64` wrapper for special floats (NaN, Inf, -0.0)
- `$absent` wrapper for CAS expected-missing
- Request/response envelope structure
- Version encoding (tagged union)
- Versioned<T> encoding
- Round-trip property: encode(decode(x)) == x

### Wire Encoding Rules (FROZEN)

| Value Type | JSON Encoding | Notes |
|------------|--------------|-------|
| Null | `null` | Direct |
| Bool | `true`/`false` | Direct |
| Int | number | Full i64 range |
| Float (normal) | number | Standard JSON |
| Float (NaN) | `{"$f64": "NaN"}` | Special wrapper |
| Float (+Inf) | `{"$f64": "+Inf"}` | Special wrapper |
| Float (-Inf) | `{"$f64": "-Inf"}` | Special wrapper |
| Float (-0.0) | `{"$f64": "-0.0"}` | Special wrapper |
| String | `"..."` | UTF-8 escaped |
| Bytes | `{"$bytes": "..."}` | Standard base64 |
| Array | `[...]` | Recursive |
| Object | `{...}` | Recursive |

### Success Criteria
- [ ] All 8 value types encode correctly
- [ ] All special floats use $f64 wrapper
- [ ] Bytes use $bytes wrapper with base64
- [ ] $absent for CAS expected-missing
- [ ] Request/response envelopes work
- [ ] Round-trip tests pass for all types

### Component Breakdown
- **Story #574**: Request/Response Envelope Implementation
- **Story #575**: Value Type JSON Mapping
- **Story #576**: $bytes Wrapper Implementation
- **Story #577**: $f64 Wrapper Implementation
- **Story #578**: $absent Wrapper Implementation
- **Story #579**: Versioned<T> Wire Encoding

---

## Story #575: Value Type JSON Mapping

**GitHub Issue**: [#575](https://github.com/anibjoshi/in-mem/issues/575)
**Dependencies**: Epic 80
**Blocks**: Stories #576, #577, #578

### Start Story

```bash
./scripts/start-story.sh 83 575 value-json-encoding
```

### Key Implementation Points

```rust
pub fn encode_json(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(true) => "true".to_string(),
        Value::Bool(false) => "false".to_string(),
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
    } else if f.to_bits() == (-0.0_f64).to_bits() {
        r#"{"$f64":"-0.0"}"#.to_string()
    } else {
        // Normal float
        format!("{}", f)
    }
}
```

### CRITICAL Tests

```rust
#[test]
fn test_int_encodes_as_number() {
    assert_eq!(encode_json(&Value::Int(42)), "42");
    assert_eq!(encode_json(&Value::Int(-123)), "-123");
    assert_eq!(encode_json(&Value::Int(i64::MAX)), "9223372036854775807");
}

#[test]
fn test_float_normal_encodes_as_number() {
    assert_eq!(encode_json(&Value::Float(1.5)), "1.5");
}

#[test]
fn test_float_nan_uses_wrapper() {
    assert_eq!(encode_json(&Value::Float(f64::NAN)), r#"{"$f64":"NaN"}"#);
}

#[test]
fn test_float_infinity_uses_wrapper() {
    assert_eq!(encode_json(&Value::Float(f64::INFINITY)), r#"{"$f64":"+Inf"}"#);
    assert_eq!(encode_json(&Value::Float(f64::NEG_INFINITY)), r#"{"$f64":"-Inf"}"#);
}

#[test]
fn test_float_negative_zero_uses_wrapper() {
    assert_eq!(encode_json(&Value::Float(-0.0)), r#"{"$f64":"-0.0"}"#);
}
```

### Acceptance Criteria

- [ ] Null → `null`
- [ ] Bool → `true`/`false`
- [ ] Int → JSON number (full i64 range)
- [ ] Float (normal) → JSON number
- [ ] Float (special) → `{"$f64": "..."}`
- [ ] String → Escaped JSON string
- [ ] Bytes → `{"$bytes": "..."}`
- [ ] Array → JSON array (recursive)
- [ ] Object → JSON object (recursive)

---

## Story #576: $bytes Wrapper Implementation

**GitHub Issue**: [#576](https://github.com/anibjoshi/in-mem/issues/576)

### Key Implementation Points

```rust
fn encode_bytes(bytes: &[u8]) -> String {
    use base64::{Engine, engine::general_purpose::STANDARD};
    format!(r#"{{"$bytes":"{}"}}"#, STANDARD.encode(bytes))
}

fn decode_bytes_wrapper(json: &str) -> Result<Vec<u8>, DecodeError> {
    // Parse as JSON object
    // Check for "$bytes" key
    // Decode base64 value
}
```

### CRITICAL Tests

```rust
#[test]
fn test_bytes_roundtrip() {
    let original = Value::Bytes(vec![0, 1, 255, 128]);
    let json = encode_json(&original);
    let decoded = decode_json(&json).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_bytes_not_confused_with_array() {
    let bytes = Value::Bytes(vec![1, 2, 3]);
    let array = Value::Array(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);

    let bytes_json = encode_json(&bytes);
    let array_json = encode_json(&array);

    // They should produce different JSON
    assert_ne!(bytes_json, array_json);

    // And decode back to correct types
    assert!(matches!(decode_json(&bytes_json).unwrap(), Value::Bytes(_)));
    assert!(matches!(decode_json(&array_json).unwrap(), Value::Array(_)));
}
```

### Acceptance Criteria

- [ ] Empty bytes encode as `{"$bytes":""}`
- [ ] Standard base64 encoding (not URL-safe)
- [ ] Round-trip preserves exact bytes
- [ ] Bytes never confused with Array

---

## Story #577: $f64 Wrapper Implementation

**GitHub Issue**: [#577](https://github.com/anibjoshi/in-mem/issues/577)

### Key Implementation Points

```rust
fn encode_special_float(kind: SpecialFloatKind) -> String {
    match kind {
        SpecialFloatKind::NaN => r#"{"$f64":"NaN"}"#.to_string(),
        SpecialFloatKind::PositiveInfinity => r#"{"$f64":"+Inf"}"#.to_string(),
        SpecialFloatKind::NegativeInfinity => r#"{"$f64":"-Inf"}"#.to_string(),
        SpecialFloatKind::NegativeZero => r#"{"$f64":"-0.0"}"#.to_string(),
    }
}

fn decode_f64_wrapper(s: &str) -> Result<f64, DecodeError> {
    match s {
        "NaN" => Ok(f64::NAN),
        "+Inf" => Ok(f64::INFINITY),
        "-Inf" => Ok(f64::NEG_INFINITY),
        "-0.0" => Ok(-0.0),
        _ => Err(DecodeError::InvalidF64Wrapper(s.to_string())),
    }
}
```

### CRITICAL Tests

```rust
#[test]
fn test_nan_roundtrip() {
    let original = Value::Float(f64::NAN);
    let json = encode_json(&original);
    let decoded = decode_json(&json).unwrap();
    match decoded {
        Value::Float(f) => assert!(f.is_nan()),
        _ => panic!("Expected Float"),
    }
}

#[test]
fn test_negative_zero_roundtrip() {
    let original = Value::Float(-0.0);
    let json = encode_json(&original);
    let decoded = decode_json(&json).unwrap();
    match decoded {
        Value::Float(f) => {
            assert_eq!(f, 0.0); // IEEE-754: -0.0 == 0.0
            assert!(f.is_sign_negative()); // But sign preserved
        }
        _ => panic!("Expected Float"),
    }
}
```

### Acceptance Criteria

- [ ] NaN → `{"$f64":"NaN"}`
- [ ] +Inf → `{"$f64":"+Inf"}`
- [ ] -Inf → `{"$f64":"-Inf"}`
- [ ] -0.0 → `{"$f64":"-0.0"}`
- [ ] Normal floats do NOT use wrapper
- [ ] Round-trip preserves special values

---

## Story #578: $absent Wrapper Implementation

**GitHub Issue**: [#578](https://github.com/anibjoshi/in-mem/issues/578)

### Key Implementation Points

```rust
/// Special value for CAS "expected not to exist"
pub fn encode_absent() -> String {
    r#"{"$absent":true}"#.to_string()
}

pub fn is_absent_wrapper(json: &serde_json::Value) -> bool {
    json.get("$absent").map(|v| v.as_bool() == Some(true)).unwrap_or(false)
}
```

### Purpose

- `$absent` represents "expected to not exist" in CAS operations
- Different from `null` (which is a valid value)
- Used for create-if-missing semantics

### CRITICAL Tests

```rust
#[test]
fn test_absent_different_from_null() {
    let absent = encode_absent();
    let null = encode_json(&Value::Null);

    assert_ne!(absent, null);
    assert_eq!(absent, r#"{"$absent":true}"#);
    assert_eq!(null, "null");
}

#[test]
fn test_absent_for_cas_create() {
    // In CAS: expected=$absent means "create if not exists"
    // This is different from expected=null
}
```

### Acceptance Criteria

- [ ] `$absent` encodes as `{"$absent":true}`
- [ ] Distinct from `null`
- [ ] Used for CAS create-if-missing
- [ ] Decodes correctly

---

## Story #579: Versioned<T> Wire Encoding

**GitHub Issue**: [#579](https://github.com/anibjoshi/in-mem/issues/579)

### Key Implementation Points

```rust
/// Version tagged union
pub fn encode_version(version: &Version) -> String {
    match version {
        Version::Txn(n) => format!(r#"{{"type":"txn","value":{}}}"#, n),
        Version::Sequence(n) => format!(r#"{{"type":"sequence","value":{}}}"#, n),
        Version::Counter(n) => format!(r#"{{"type":"counter","value":{}}}"#, n),
    }
}

/// Versioned<T> wrapper
pub fn encode_versioned<T: Serialize>(versioned: &Versioned<T>) -> String {
    format!(
        r#"{{"value":{},"version":{},"timestamp":{}}}"#,
        serde_json::to_string(&versioned.value).unwrap(),
        encode_version(&versioned.version),
        versioned.timestamp
    )
}
```

### Acceptance Criteria

- [ ] Version has `type` and `value` fields
- [ ] Version types: `txn`, `sequence`, `counter`
- [ ] Versioned<T> has `value`, `version`, `timestamp`
- [ ] Timestamp is microseconds since Unix epoch

---

## Story #574: Request/Response Envelope

**GitHub Issue**: [#574](https://github.com/anibjoshi/in-mem/issues/574)

### Key Implementation Points

```rust
/// Request envelope
pub struct Request {
    pub id: String,
    pub op: String,
    pub params: serde_json::Value,
}

/// Success response
pub struct SuccessResponse {
    pub id: String,
    pub ok: bool, // always true
    pub result: serde_json::Value,
}

/// Error response
pub struct ErrorResponse {
    pub id: String,
    pub ok: bool, // always false
    pub error: ErrorPayload,
}

pub struct ErrorPayload {
    pub code: String,
    pub message: String,
    pub details: Option<serde_json::Value>,
}
```

### Wire Format

Request:
```json
{"id": "123", "op": "kv.set", "params": {"key": "foo", "value": 42}}
```

Success:
```json
{"id": "123", "ok": true, "result": null}
```

Error:
```json
{"id": "123", "ok": false, "error": {"code": "NotFound", "message": "Key not found", "details": {"key": "foo"}}}
```

### Acceptance Criteria

- [ ] Request has `id`, `op`, `params`
- [ ] Success response has `ok: true`, `result`
- [ ] Error response has `ok: false`, `error`
- [ ] Error payload has `code`, `message`, `details`
- [ ] Operation names are frozen (e.g., `kv.set`, `json.get`)

---

## Epic 83 Completion Checklist

### 1. Final Validation

```bash
~/.cargo/bin/cargo test wire_ -- --nocapture
~/.cargo/bin/cargo test --test m11_comprehensive wire_encoding
```

### 2. Verify Round-Trip

```bash
# All value types should round-trip correctly
~/.cargo/bin/cargo test roundtrip_ -- --nocapture
```

### 3. Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-83-wire-encoding -m "Epic 83: Wire Encoding Contract complete

Delivered:
- JSON encoding for all 8 value types
- \$bytes wrapper (base64)
- \$f64 wrapper (NaN, Inf, -0.0)
- \$absent wrapper (CAS create-if-missing)
- Request/response envelopes
- Version/Versioned encoding
- Round-trip tests passing

Stories: #574, #575, #576, #577, #578, #579
"
git push origin develop
gh issue close 573 --comment "Epic 83: Wire Encoding Contract - COMPLETE"
```

---

## Summary

Epic 83 establishes the WIRE ENCODING:

- **JSON mandatory**: All values encode to JSON
- **Special wrappers**: $bytes, $f64, $absent
- **Round-trip property**: encode(decode(x)) == x
- **Type preservation**: Bytes != Array, Float != Int
- **Frozen format**: Cannot change without major version
