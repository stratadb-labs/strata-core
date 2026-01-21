# Epic 84: Error Model Finalization - Implementation Prompts

**Epic Goal**: Freeze all error codes, messages, and payload structures

**GitHub Issue**: [#580](https://github.com/anibjoshi/in-mem/issues/580)
**Status**: Ready after Epic 80
**Dependencies**: Epic 80 (Value Model)
**Phase**: 2 (Error Model)

---

## NAMING CONVENTION - CRITICAL

> **NEVER use "M11" in the actual codebase or comments.**
>
> - "Strata" IS allowed (e.g., `StrataError`, `strata_error`)
>
> **CORRECT**: `pub enum StrataError { ... }`
> **WRONG**: `//! M11 error codes`

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

Before starting ANY story in this epic, read:
1. **Contract Spec**: `docs/milestones/M11/M11_CONTRACT.md`
2. **Epic Spec**: `docs/milestones/M11/EPIC_84_ERROR_MODEL.md`
3. **Prompt Header**: `docs/prompts/M11/M11_PROMPT_HEADER.md`

---

## Epic 84 Overview

### Scope
- Error code enumeration (12 codes, FROZEN after M11)
- Wire error shape (code, message, details)
- ConstraintViolation reason codes
- Error details payload shapes
- Error propagation from substrate to facade

### Error Code Registry (FROZEN)

| Code | Meaning | Typical Details |
|------|---------|-----------------|
| `NotFound` | Key/entity does not exist | `{key: string}` |
| `WrongType` | Value has unexpected type | `{expected: string, actual: string}` |
| `InvalidKey` | Key fails validation | `{key: string, reason: string}` |
| `InvalidPath` | JSONPath is malformed | `{path: string, reason: string}` |
| `ConstraintViolation` | Structural limit violated | `{reason: string, ...context}` |
| `Conflict` | CAS/optimistic lock failed | `{expected: Value, actual: Value}` |
| `RunNotFound` | Run does not exist | `{run_id: string}` |
| `RunClosed` | Run is closed (read-only) | `{run_id: string}` |
| `RunExists` | Run already exists | `{run_id: string}` |
| `HistoryTrimmed` | Version was compacted | `{requested: Version, earliest_retained: Version}` |
| `Overflow` | Integer overflow | `{}` |
| `Internal` | Bug/internal error | `{message: string}` |

### Success Criteria
- [ ] All 12 error codes implemented
- [ ] Wire shape: `{code, message, details}`
- [ ] ConstraintViolation reasons defined
- [ ] Error propagation verified
- [ ] All error-producing conditions documented

### Component Breakdown
- **Story #581**: Error Code Enumeration
- **Story #582**: Wire Error Shape Implementation
- **Story #583**: ConstraintViolation Reason Codes
- **Story #584**: Error Details Payload Shapes
- **Story #585**: Error-Producing Condition Coverage

---

## Story #581: Error Code Enumeration

**GitHub Issue**: [#581](https://github.com/anibjoshi/in-mem/issues/581)
**Dependencies**: Epic 80
**Blocks**: All other stories

### Start Story

```bash
./scripts/start-story.sh 84 581 error-code-enumeration
```

### Key Implementation Points

```rust
use crate::value::Value;
use crate::version::Version;

/// All Strata errors - FROZEN after M11
#[derive(Debug, Clone, thiserror::Error)]
pub enum StrataError {
    #[error("Key not found: {key}")]
    NotFound { key: String },

    #[error("Wrong type: expected {expected}, got {actual}")]
    WrongType {
        expected: &'static str,
        actual: &'static str,
    },

    #[error("Invalid key '{key}': {reason}")]
    InvalidKey { key: String, reason: String },

    #[error("Invalid path '{path}': {reason}")]
    InvalidPath { path: String, reason: String },

    #[error("Constraint violation: {reason}")]
    ConstraintViolation {
        reason: String,
        details: Option<Value>,
    },

    #[error("Conflict: expected {expected:?}, actual {actual:?}")]
    Conflict { expected: Value, actual: Value },

    #[error("Run not found: {run_id}")]
    RunNotFound { run_id: String },

    #[error("Run is closed: {run_id}")]
    RunClosed { run_id: String },

    #[error("Run already exists: {run_id}")]
    RunExists { run_id: String },

    #[error("History trimmed: requested {requested:?}, earliest {earliest_retained:?}")]
    HistoryTrimmed {
        requested: Version,
        earliest_retained: Version,
    },

    #[error("Integer overflow")]
    Overflow,

    #[error("Internal error: {message}")]
    Internal { message: String },
}

impl StrataError {
    /// Get the error code string (frozen)
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

### CRITICAL Tests

```rust
#[test]
fn test_exactly_12_error_codes() {
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

    let error_codes: std::collections::HashSet<_> =
        codes.iter().map(|e| e.error_code()).collect();
    assert_eq!(error_codes.len(), 12);
}

#[test]
fn test_error_code_strings_are_correct() {
    assert_eq!(StrataError::NotFound { key: "k".into() }.error_code(), "NotFound");
    assert_eq!(StrataError::Overflow.error_code(), "Overflow");
    assert_eq!(StrataError::Internal { message: "".into() }.error_code(), "Internal");
}
```

### Acceptance Criteria

- [ ] Exactly 12 error codes defined
- [ ] `error_code()` returns correct string for each
- [ ] All codes implement Display
- [ ] No additional error codes

---

## Story #582: Wire Error Shape Implementation

**GitHub Issue**: [#582](https://github.com/anibjoshi/in-mem/issues/582)

### Key Implementation Points

```rust
/// Wire format for errors
#[derive(Debug, Clone, Serialize)]
pub struct WireError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl From<&StrataError> for WireError {
    fn from(err: &StrataError) -> Self {
        WireError {
            code: err.error_code().to_string(),
            message: err.to_string(),
            details: err.wire_details(),
        }
    }
}

impl StrataError {
    /// Get details payload for wire encoding
    pub fn wire_details(&self) -> Option<serde_json::Value> {
        match self {
            StrataError::NotFound { key } => Some(json!({ "key": key })),
            StrataError::WrongType { expected, actual } => {
                Some(json!({ "expected": expected, "actual": actual }))
            }
            StrataError::InvalidKey { key, reason } => {
                Some(json!({ "key": key, "reason": reason }))
            }
            StrataError::Conflict { expected, actual } => {
                Some(json!({ "expected": expected, "actual": actual }))
            }
            // ... etc
            StrataError::Overflow => None,
            StrataError::Internal { message } => Some(json!({ "message": message })),
        }
    }
}
```

### Wire Format Example

```json
{
  "code": "NotFound",
  "message": "Key not found: mykey",
  "details": {
    "key": "mykey"
  }
}
```

### Acceptance Criteria

- [ ] Wire shape: `{code, message, details}`
- [ ] `code` is frozen string
- [ ] `message` is human-readable
- [ ] `details` is optional, type-specific

---

## Story #583: ConstraintViolation Reason Codes

**GitHub Issue**: [#583](https://github.com/anibjoshi/in-mem/issues/583)

### Key Implementation Points

```rust
/// Constraint violation reasons (frozen)
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

### Usage

```rust
StrataError::ConstraintViolation {
    reason: constraint_reasons::VALUE_TOO_LARGE.to_string(),
    details: Some(json!({
        "actual_bytes": 20_000_000,
        "max_bytes": 16_777_216,
    })),
}
```

### Acceptance Criteria

- [ ] All reason codes defined
- [ ] Reason codes are frozen strings
- [ ] Used consistently in validation

---

## Story #584: Error Details Payload Shapes

**GitHub Issue**: [#584](https://github.com/anibjoshi/in-mem/issues/584)

### Details by Error Code

| Error Code | Details Shape |
|------------|--------------|
| `NotFound` | `{key: string}` |
| `WrongType` | `{expected: string, actual: string}` |
| `InvalidKey` | `{key: string, reason: string}` |
| `InvalidPath` | `{path: string, reason: string}` |
| `ConstraintViolation` | `{reason: string, ...context}` |
| `Conflict` | `{expected: Value, actual: Value}` |
| `RunNotFound` | `{run_id: string}` |
| `RunClosed` | `{run_id: string}` |
| `RunExists` | `{run_id: string}` |
| `HistoryTrimmed` | `{requested: Version, earliest_retained: Version}` |
| `Overflow` | `{}` (no details) |
| `Internal` | `{message: string}` |

### Acceptance Criteria

- [ ] Each error code has defined details shape
- [ ] Shapes are validated in tests
- [ ] Details are JSON-serializable

---

## Story #585: Error-Producing Condition Coverage

**GitHub Issue**: [#585](https://github.com/anibjoshi/in-mem/issues/585)

### Error-Producing Conditions

| Condition | Error Code |
|-----------|------------|
| Get non-existent key | `NotFound` |
| Incr on non-integer | `WrongType` |
| Empty key | `InvalidKey` |
| NUL in key | `InvalidKey` |
| `_strata/` prefix | `InvalidKey` (reserved_prefix) |
| Key too long | `ConstraintViolation` (key_too_long) |
| Invalid JSONPath | `InvalidPath` |
| Value too large | `ConstraintViolation` (value_too_large) |
| Nesting too deep | `ConstraintViolation` (nesting_too_deep) |
| CAS mismatch | `Conflict` |
| Unknown run_id | `RunNotFound` |
| Write to closed run | `RunClosed` |
| Create existing run | `RunExists` |
| Get trimmed version | `HistoryTrimmed` |
| Incr overflow | `Overflow` |

### Acceptance Criteria

- [ ] Every condition mapped to error code
- [ ] Test for each condition
- [ ] No undefined behavior

---

## Epic 84 Completion Checklist

### 1. Final Validation

```bash
~/.cargo/bin/cargo test error_ -- --nocapture
~/.cargo/bin/cargo test --test m11_comprehensive error_model
```

### 2. Verify Error Codes

```bash
# Verify exactly 12 error codes
~/.cargo/bin/cargo test test_exactly_12_error_codes
```

### 3. Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-84-error-model -m "Epic 84: Error Model Finalization complete

Delivered:
- 12 error codes (NotFound, WrongType, InvalidKey, InvalidPath,
  ConstraintViolation, Conflict, RunNotFound, RunClosed, RunExists,
  HistoryTrimmed, Overflow, Internal)
- Wire error shape: {code, message, details}
- ConstraintViolation reason codes
- Details payloads for all error types
- Error-producing condition coverage

Stories: #581, #582, #583, #584, #585
"
git push origin develop
gh issue close 580 --comment "Epic 84: Error Model Finalization - COMPLETE"
```

---

## Summary

Epic 84 establishes the ERROR MODEL:

- **12 error codes**: Complete, frozen set
- **Wire shape**: `{code, message, details}`
- **Structured details**: Type-specific payloads
- **Semantic distinction**: Conflict (temporal) vs ConstraintViolation (structural)
- **No undefined behavior**: Every invalid input has defined error
