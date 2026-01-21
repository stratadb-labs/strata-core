# Epic 84: Error Model Finalization

**Goal**: Freeze all error codes, messages, and payload structures

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

- Error code enumeration (frozen after M11)
- Wire error shape (code, message, details)
- ConstraintViolation reasons
- Error details payload shapes
- Error propagation from substrate to facade

---

## Error Code Registry

| Code | Meaning | Details |
|------|---------|---------|
| `NotFound` | Key/entity does not exist | `{key: string}` |
| `WrongType` | Value has unexpected type | `{expected: string, actual: string}` |
| `InvalidKey` | Key fails validation | `{key: string, reason: string}` |
| `InvalidPath` | JSONPath is malformed | `{path: string, reason: string}` |
| `ConstraintViolation` | Value/operation violates constraint | `{reason: string, ...context}` |
| `Conflict` | CAS/optimistic lock failed | `{expected: Value, actual: Value}` |
| `RunNotFound` | Run does not exist | `{run_id: string}` |
| `RunClosed` | Run is closed (read-only) | `{run_id: string}` |
| `RunExists` | Run already exists | `{run_id: string}` |
| `HistoryTrimmed` | Requested version was compacted | `{requested: Version, earliest_retained: Version}` |
| `Overflow` | Integer overflow/underflow | `{}` |
| `Internal` | Internal error (bug) | `{message: string}` |

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #587 | Error Code Enumeration | FOUNDATION |
| #588 | Wire Error Shape | CRITICAL |
| #589 | ConstraintViolation Reasons | CRITICAL |
| #590 | Error Details Payloads | CRITICAL |
| #591 | Error Propagation | CRITICAL |

---

## Story #587: Error Code Enumeration

**File**: `crates/core/src/error.rs` (NEW)

**Deliverable**: Complete enumeration of all error codes

### Tests FIRST

```rust
#[cfg(test)]
mod error_code_tests {
    use super::*;

    #[test]
    fn test_all_error_codes_exist() {
        // This test ensures all documented error codes are defined
        let codes = vec![
            StrataError::NotFound { key: "k".into() },
            StrataError::WrongType { expected: "Int", actual: "Float" },
            StrataError::InvalidKey { key: "".into(), reason: "empty".into() },
            StrataError::InvalidPath { path: "$.".into(), reason: "syntax".into() },
            StrataError::ConstraintViolation { reason: "test".into(), details: None },
            StrataError::Conflict { expected: Value::Null, actual: Value::Null },
            StrataError::RunNotFound { run_id: "r".into() },
            StrataError::RunClosed { run_id: "r".into() },
            StrataError::RunExists { run_id: "r".into() },
            StrataError::HistoryTrimmed {
                requested: Version::Txn(1),
                earliest_retained: Version::Txn(100),
            },
            StrataError::Overflow,
            StrataError::Internal { message: "bug".into() },
        ];

        // All codes should have distinct error_code() values
        let error_codes: std::collections::HashSet<_> =
            codes.iter().map(|e| e.error_code()).collect();
        assert_eq!(error_codes.len(), 12);
    }

    #[test]
    fn test_error_code_strings() {
        assert_eq!(StrataError::NotFound { key: "k".into() }.error_code(), "NotFound");
        assert_eq!(StrataError::WrongType { expected: "a", actual: "b" }.error_code(), "WrongType");
        assert_eq!(StrataError::InvalidKey { key: "k".into(), reason: "r".into() }.error_code(), "InvalidKey");
        assert_eq!(StrataError::Overflow.error_code(), "Overflow");
    }

    #[test]
    fn test_error_messages_human_readable() {
        let err = StrataError::NotFound { key: "mykey".into() };
        let msg = err.to_string();

        // Message should include the key
        assert!(msg.contains("mykey") || msg.contains("not found"));
    }

    #[test]
    fn test_error_display_impl() {
        let errors = vec![
            StrataError::NotFound { key: "test".into() },
            StrataError::WrongType { expected: "Int", actual: "String" },
            StrataError::Overflow,
        ];

        for err in errors {
            // Display should not panic
            let _ = format!("{}", err);
        }
    }
}
```

### Implementation

```rust
use crate::value::Value;
use crate::versioned::Version;

/// All possible Strata errors
///
/// FROZEN after M11 - error codes cannot change without major version bump.
#[derive(Debug, Clone, thiserror::Error)]
pub enum StrataError {
    /// Key or entity does not exist
    #[error("Key not found: {key}")]
    NotFound { key: String },

    /// Value has unexpected type
    #[error("Wrong type: expected {expected}, got {actual}")]
    WrongType {
        expected: &'static str,
        actual: &'static str,
    },

    /// Key fails validation
    #[error("Invalid key '{key}': {reason}")]
    InvalidKey { key: String, reason: String },

    /// JSONPath is malformed
    #[error("Invalid path '{path}': {reason}")]
    InvalidPath { path: String, reason: String },

    /// Value or operation violates constraint
    #[error("Constraint violation: {reason}")]
    ConstraintViolation {
        reason: String,
        details: Option<Value>,
    },

    /// CAS/optimistic lock failed
    #[error("Conflict: expected {expected:?}, actual {actual:?}")]
    Conflict { expected: Value, actual: Value },

    /// Run does not exist
    #[error("Run not found: {run_id}")]
    RunNotFound { run_id: String },

    /// Run is closed (read-only)
    #[error("Run is closed: {run_id}")]
    RunClosed { run_id: String },

    /// Run already exists
    #[error("Run already exists: {run_id}")]
    RunExists { run_id: String },

    /// Requested version was compacted
    #[error("History trimmed: requested {requested:?}, earliest retained {earliest_retained:?}")]
    HistoryTrimmed {
        requested: Version,
        earliest_retained: Version,
    },

    /// Integer overflow/underflow
    #[error("Integer overflow")]
    Overflow,

    /// Internal error (should not happen)
    #[error("Internal error: {message}")]
    Internal { message: String },
}

impl StrataError {
    /// Get the error code string
    ///
    /// FROZEN: These strings are part of the wire protocol.
    pub fn error_code(&self) -> &'static str {
        match self {
            StrataError::NotFound { .. } => "NotFound",
            StrataError::WrongType { .. } => "WrongType",
            StrataError::InvalidKey { .. } => "InvalidKey",
            StrataError::InvalidPath { .. } => "InvalidPath",
            StrataError::ConstraintViolation { .. } => "ConstraintViolation",
            StrataError::Conflict { .. } => "Conflict",
            StrataError::RunNotFound { .. } => "RunNotFound",
            StrataError::RunClosed { .. } => "RunClosed",
            StrataError::RunExists { .. } => "RunExists",
            StrataError::HistoryTrimmed { .. } => "HistoryTrimmed",
            StrataError::Overflow => "Overflow",
            StrataError::Internal { .. } => "Internal",
        }
    }
}
```

### Acceptance Criteria

- [ ] All 12 error codes are defined
- [ ] `error_code()` returns correct string for each variant
- [ ] Error messages are human-readable
- [ ] Display trait is implemented

---

## Story #588: Wire Error Shape

**File**: `crates/core/src/error.rs`, `crates/wire/src/json/error.rs`

**Deliverable**: Consistent wire format for errors

### Tests FIRST

```rust
#[cfg(test)]
mod wire_error_tests {
    use super::*;

    #[test]
    fn test_error_wire_shape_has_code() {
        let err = StrataError::NotFound { key: "test".into() };
        let wire = err.to_wire_error();

        assert_eq!(wire.code, "NotFound");
    }

    #[test]
    fn test_error_wire_shape_has_message() {
        let err = StrataError::NotFound { key: "mykey".into() };
        let wire = err.to_wire_error();

        assert!(!wire.message.is_empty());
    }

    #[test]
    fn test_error_wire_shape_has_details() {
        let err = StrataError::NotFound { key: "mykey".into() };
        let wire = err.to_wire_error();

        // NotFound should have key in details
        match wire.details {
            Some(Value::Object(map)) => {
                assert!(map.contains_key("key"));
            }
            _ => panic!("Expected details object"),
        }
    }

    #[test]
    fn test_wrong_type_wire_details() {
        let err = StrataError::WrongType { expected: "Int", actual: "Float" };
        let wire = err.to_wire_error();

        match wire.details {
            Some(Value::Object(map)) => {
                assert_eq!(map.get("expected"), Some(&Value::String("Int".into())));
                assert_eq!(map.get("actual"), Some(&Value::String("Float".into())));
            }
            _ => panic!("Expected details object"),
        }
    }

    #[test]
    fn test_wire_error_json_encoding() {
        let err = StrataError::NotFound { key: "test".into() };
        let wire = err.to_wire_error();
        let json = encode_wire_error(&wire);

        // Should have code, message, details
        assert!(json.contains(r#""code":"NotFound""#));
        assert!(json.contains(r#""message":"#));
        assert!(json.contains(r#""details":"#));
    }

    #[test]
    fn test_wire_error_code_is_string() {
        let err = StrataError::Overflow;
        let wire = err.to_wire_error();
        let json = encode_wire_error(&wire);

        // Code must be a string, not a number
        assert!(json.contains(r#""code":"Overflow""#));
    }
}
```

### Implementation

```rust
/// Wire protocol error structure
#[derive(Debug, Clone)]
pub struct WireError {
    /// Error code (matches StrataError variant name)
    pub code: String,

    /// Human-readable message
    pub message: String,

    /// Error-specific details (may be null)
    pub details: Option<Value>,
}

impl StrataError {
    /// Convert to wire error format
    pub fn to_wire_error(&self) -> WireError {
        WireError {
            code: self.error_code().to_string(),
            message: self.to_string(),
            details: self.error_details(),
        }
    }

    /// Get error-specific details
    fn error_details(&self) -> Option<Value> {
        let mut details = std::collections::HashMap::new();

        match self {
            StrataError::NotFound { key } => {
                details.insert("key".to_string(), Value::String(key.clone()));
            }
            StrataError::WrongType { expected, actual } => {
                details.insert("expected".to_string(), Value::String(expected.to_string()));
                details.insert("actual".to_string(), Value::String(actual.to_string()));
            }
            StrataError::InvalidKey { key, reason } => {
                details.insert("key".to_string(), Value::String(key.clone()));
                details.insert("reason".to_string(), Value::String(reason.clone()));
            }
            StrataError::InvalidPath { path, reason } => {
                details.insert("path".to_string(), Value::String(path.clone()));
                details.insert("reason".to_string(), Value::String(reason.clone()));
            }
            StrataError::ConstraintViolation { reason, details: extra } => {
                details.insert("reason".to_string(), Value::String(reason.clone()));
                if let Some(extra) = extra {
                    if let Value::Object(map) = extra {
                        details.extend(map.clone());
                    }
                }
            }
            StrataError::Conflict { expected, actual } => {
                details.insert("expected".to_string(), expected.clone());
                details.insert("actual".to_string(), actual.clone());
            }
            StrataError::RunNotFound { run_id } |
            StrataError::RunClosed { run_id } |
            StrataError::RunExists { run_id } => {
                details.insert("run_id".to_string(), Value::String(run_id.clone()));
            }
            StrataError::HistoryTrimmed { requested, earliest_retained } => {
                // Version encoding handled separately
                details.insert("requested".to_string(), version_to_value(requested));
                details.insert("earliest_retained".to_string(), version_to_value(earliest_retained));
            }
            StrataError::Overflow => {
                // No additional details
            }
            StrataError::Internal { message } => {
                details.insert("message".to_string(), Value::String(message.clone()));
            }
        }

        if details.is_empty() {
            None
        } else {
            Some(Value::Object(details))
        }
    }
}

fn version_to_value(v: &Version) -> Value {
    let mut map = std::collections::HashMap::new();
    map.insert("type".to_string(), Value::String(v.type_name().to_string()));
    map.insert("value".to_string(), Value::Int(v.value() as i64));
    Value::Object(map)
}
```

### Acceptance Criteria

- [ ] Wire error has code, message, details fields
- [ ] code is a string
- [ ] message is human-readable
- [ ] details contains error-specific information
- [ ] All error types produce correct details

---

## Story #589: ConstraintViolation Reasons

**File**: `crates/core/src/error.rs`

**Deliverable**: Standardized constraint violation reasons

### Tests FIRST

```rust
#[cfg(test)]
mod constraint_violation_tests {
    use super::*;

    #[test]
    fn test_constraint_value_too_large() {
        let err = StrataError::constraint_violation("value_too_large", None);
        let wire = err.to_wire_error();

        match wire.details {
            Some(Value::Object(map)) => {
                assert_eq!(map.get("reason"), Some(&Value::String("value_too_large".into())));
            }
            _ => panic!("Expected details"),
        }
    }

    #[test]
    fn test_constraint_nesting_too_deep() {
        let err = StrataError::constraint_violation("nesting_too_deep", Some(Value::Object({
            let mut m = HashMap::new();
            m.insert("depth".to_string(), Value::Int(129));
            m.insert("max".to_string(), Value::Int(128));
            m
        })));

        let wire = err.to_wire_error();
        match wire.details {
            Some(Value::Object(map)) => {
                assert_eq!(map.get("reason"), Some(&Value::String("nesting_too_deep".into())));
                assert_eq!(map.get("depth"), Some(&Value::Int(129)));
            }
            _ => panic!("Expected details"),
        }
    }

    #[test]
    fn test_constraint_key_too_long() {
        let err = StrataError::constraint_violation("key_too_long", None);
        assert_eq!(err.error_code(), "ConstraintViolation");
    }

    #[test]
    fn test_constraint_vector_dim_exceeded() {
        let err = StrataError::constraint_violation("vector_dim_exceeded", None);
        assert!(err.to_string().contains("vector_dim_exceeded"));
    }

    #[test]
    fn test_constraint_vector_dim_mismatch() {
        let err = StrataError::constraint_violation("vector_dim_mismatch", None);
        assert_eq!(err.error_code(), "ConstraintViolation");
    }

    #[test]
    fn test_constraint_root_not_object() {
        let err = StrataError::constraint_violation("root_not_object", None);
        assert_eq!(err.error_code(), "ConstraintViolation");
    }

    #[test]
    fn test_constraint_reserved_prefix() {
        let err = StrataError::constraint_violation("reserved_prefix", None);
        assert_eq!(err.error_code(), "ConstraintViolation");
    }

    #[test]
    fn test_all_constraint_reasons() {
        // Document all valid constraint reasons
        let reasons = vec![
            "value_too_large",
            "nesting_too_deep",
            "key_too_long",
            "vector_dim_exceeded",
            "vector_dim_mismatch",
            "root_not_object",
            "reserved_prefix",
            "array_too_long",
            "object_too_many_entries",
        ];

        for reason in reasons {
            let err = StrataError::constraint_violation(reason, None);
            let wire = err.to_wire_error();

            match wire.details {
                Some(Value::Object(map)) => {
                    assert_eq!(map.get("reason"), Some(&Value::String(reason.into())));
                }
                _ => panic!("Expected details for reason: {}", reason),
            }
        }
    }
}
```

### Implementation

```rust
impl StrataError {
    /// Create a constraint violation error
    pub fn constraint_violation(reason: &str, extra_details: Option<Value>) -> Self {
        StrataError::ConstraintViolation {
            reason: reason.to_string(),
            details: extra_details,
        }
    }
}

/// Standard constraint violation reasons (FROZEN)
pub mod constraint_reasons {
    pub const VALUE_TOO_LARGE: &str = "value_too_large";
    pub const NESTING_TOO_DEEP: &str = "nesting_too_deep";
    pub const KEY_TOO_LONG: &str = "key_too_long";
    pub const VECTOR_DIM_EXCEEDED: &str = "vector_dim_exceeded";
    pub const VECTOR_DIM_MISMATCH: &str = "vector_dim_mismatch";
    pub const ROOT_NOT_OBJECT: &str = "root_not_object";
    pub const RESERVED_PREFIX: &str = "reserved_prefix";
    pub const ARRAY_TOO_LONG: &str = "array_too_long";
    pub const OBJECT_TOO_MANY_ENTRIES: &str = "object_too_many_entries";
}
```

### Acceptance Criteria

- [ ] All constraint reasons are documented
- [ ] `constraint_violation()` creates error with reason
- [ ] Extra details are merged into wire details
- [ ] Reason strings are constants (FROZEN)

---

## Story #590: Error Details Payloads

**File**: `crates/core/src/error.rs`

**Deliverable**: Consistent error details for all error types

### Tests FIRST

```rust
#[cfg(test)]
mod error_details_tests {
    use super::*;

    #[test]
    fn test_not_found_details() {
        let err = StrataError::NotFound { key: "mykey".into() };
        let wire = err.to_wire_error();

        // Must have key in details
        match wire.details {
            Some(Value::Object(map)) => {
                assert_eq!(map.get("key"), Some(&Value::String("mykey".into())));
            }
            _ => panic!("Expected details with key"),
        }
    }

    #[test]
    fn test_wrong_type_details() {
        let err = StrataError::WrongType { expected: "Int", actual: "Float" };
        let wire = err.to_wire_error();

        match wire.details {
            Some(Value::Object(map)) => {
                assert_eq!(map.get("expected"), Some(&Value::String("Int".into())));
                assert_eq!(map.get("actual"), Some(&Value::String("Float".into())));
            }
            _ => panic!("Expected details with expected/actual"),
        }
    }

    #[test]
    fn test_history_trimmed_details() {
        let err = StrataError::HistoryTrimmed {
            requested: Version::Txn(10),
            earliest_retained: Version::Txn(100),
        };
        let wire = err.to_wire_error();

        match wire.details {
            Some(Value::Object(map)) => {
                assert!(map.contains_key("requested"));
                assert!(map.contains_key("earliest_retained"));

                // Versions should be encoded as objects
                match map.get("requested") {
                    Some(Value::Object(v)) => {
                        assert_eq!(v.get("type"), Some(&Value::String("txn".into())));
                        assert_eq!(v.get("value"), Some(&Value::Int(10)));
                    }
                    _ => panic!("Expected version object"),
                }
            }
            _ => panic!("Expected details"),
        }
    }

    #[test]
    fn test_conflict_details_preserve_values() {
        let expected = Value::Int(1);
        let actual = Value::Float(1.0);

        let err = StrataError::Conflict {
            expected: expected.clone(),
            actual: actual.clone(),
        };
        let wire = err.to_wire_error();

        match wire.details {
            Some(Value::Object(map)) => {
                assert_eq!(map.get("expected"), Some(&expected));
                assert_eq!(map.get("actual"), Some(&actual));
            }
            _ => panic!("Expected details"),
        }
    }

    #[test]
    fn test_overflow_minimal_details() {
        let err = StrataError::Overflow;
        let wire = err.to_wire_error();

        // Overflow has no extra details
        assert!(wire.details.is_none() || matches!(wire.details, Some(Value::Object(ref m)) if m.is_empty()));
    }
}
```

### Acceptance Criteria

- [ ] NotFound includes key
- [ ] WrongType includes expected and actual
- [ ] InvalidKey includes key and reason
- [ ] InvalidPath includes path and reason
- [ ] HistoryTrimmed includes version objects
- [ ] Conflict includes expected and actual values
- [ ] Run errors include run_id

---

## Story #591: Error Propagation

**File**: `crates/api/src/facade/error.rs`, `crates/api/src/substrate/error.rs`

**Deliverable**: Errors propagate unchanged from substrate to facade

### Tests FIRST

```rust
#[cfg(test)]
mod error_propagation_tests {
    use super::*;

    #[test]
    fn test_facade_propagates_not_found() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        // Try to get non-existent key
        let result = facade.get("nonexistent");

        // Should be Ok(None), not an error
        assert!(matches!(result, Ok(None)));
    }

    #[test]
    fn test_facade_propagates_invalid_key() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        let result = facade.set("", Value::Int(1));

        // Should propagate InvalidKey error
        assert!(matches!(result, Err(FacadeError::InvalidKey(_))));
    }

    #[test]
    fn test_facade_propagates_wrong_type() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        facade.set("key", Value::String("hello".into())).unwrap();
        let result = facade.incr("key");

        // Should propagate WrongType error
        assert!(matches!(result, Err(FacadeError::WrongType { .. })));
    }

    #[test]
    fn test_facade_does_not_swallow_errors() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        // Reserved prefix should fail
        let result = facade.set("_strata/forbidden", Value::Int(1));

        // Error must surface, not be swallowed
        assert!(result.is_err());
    }

    #[test]
    fn test_error_conversion_preserves_code() {
        let substrate_err = SubstrateError::WrongType {
            expected: "Int",
            actual: "String",
        };

        let facade_err: FacadeError = substrate_err.into();

        // Code must be preserved
        assert_eq!(facade_err.error_code(), "WrongType");
    }

    #[test]
    fn test_error_conversion_preserves_details() {
        let substrate_err = SubstrateError::InvalidKey {
            key: "badkey".into(),
            reason: "contains null".into(),
        };

        let facade_err: FacadeError = substrate_err.into();
        let wire = facade_err.to_wire_error();

        // Details must be preserved
        match wire.details {
            Some(Value::Object(map)) => {
                assert_eq!(map.get("key"), Some(&Value::String("badkey".into())));
            }
            _ => panic!("Expected details"),
        }
    }
}
```

### Implementation

```rust
/// Facade-level errors
///
/// These are thin wrappers around StrataError that ensure
/// errors propagate unchanged.
#[derive(Debug, thiserror::Error)]
pub enum FacadeError {
    #[error(transparent)]
    Strata(#[from] StrataError),
}

impl FacadeError {
    pub fn error_code(&self) -> &'static str {
        match self {
            FacadeError::Strata(e) => e.error_code(),
        }
    }

    pub fn to_wire_error(&self) -> WireError {
        match self {
            FacadeError::Strata(e) => e.to_wire_error(),
        }
    }
}

/// Substrate-level errors
///
/// Same as StrataError - no transformation.
pub type SubstrateError = StrataError;

impl From<SubstrateError> for FacadeError {
    fn from(err: SubstrateError) -> Self {
        FacadeError::Strata(err)
    }
}
```

### Acceptance Criteria

- [ ] Facade errors wrap substrate errors transparently
- [ ] Error codes are preserved through conversion
- [ ] Error details are preserved through conversion
- [ ] Facade never swallows substrate errors
- [ ] Error messages are preserved

---

## Testing

Integration tests for error handling:

```rust
#[cfg(test)]
mod error_integration_tests {
    use super::*;

    #[test]
    fn test_error_wire_round_trip() {
        let original = StrataError::WrongType {
            expected: "Int",
            actual: "Float",
        };

        let wire = original.to_wire_error();
        let json = encode_wire_error(&wire);
        let decoded = decode_wire_error(&json).unwrap();

        assert_eq!(wire.code, decoded.code);
        assert_eq!(wire.message, decoded.message);
    }

    #[test]
    fn test_all_errors_encode_to_json() {
        let errors = vec![
            StrataError::NotFound { key: "k".into() },
            StrataError::WrongType { expected: "Int", actual: "Float" },
            StrataError::InvalidKey { key: "".into(), reason: "empty".into() },
            StrataError::Overflow,
            StrataError::Internal { message: "oops".into() },
        ];

        for err in errors {
            let wire = err.to_wire_error();
            let json = encode_wire_error(&wire);

            // Should be valid JSON
            assert!(json.starts_with('{'));
            assert!(json.ends_with('}'));
            assert!(json.contains(&format!(r#""code":"{}""#, err.error_code())));
        }
    }
}
```

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/core/src/error.rs` | CREATE - Error types |
| `crates/wire/src/json/error.rs` | CREATE - Error encoding |
| `crates/api/src/facade/error.rs` | CREATE - Facade error handling |
| `crates/api/src/substrate/error.rs` | CREATE - Substrate error handling |
| `crates/core/src/lib.rs` | MODIFY - Export error module |

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-21 | Initial epic specification |
