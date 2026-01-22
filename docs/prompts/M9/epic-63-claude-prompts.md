# Epic 63: Error Standardization - Implementation Prompts

**Epic Goal**: Unified StrataError across all primitives

**GitHub Issue**: [#467](https://github.com/anibjoshi/in-mem/issues/467)
**Status**: Ready to begin (can run in parallel with Epic 60)
**Dependencies**: Epic 60 (EntityRef)
**Phase**: 1 (Foundation)

---

## NAMING CONVENTION - CRITICAL

> **NEVER use "M9" or "Strata" in the actual codebase or comments.**
>
> - "M9" is an internal milestone tracker only - do not use it in code, comments, or user-facing text
> - All existing crates refer to the database as "in-mem" - use this name consistently
> - Do not use "Strata" anywhere in the codebase
> - This applies to: code, comments, docstrings, error messages, log messages, test names
>
> **CORRECT**: `//! Universal entity reference for any in-mem entity`
> **WRONG**: `//! Universal entity reference for any Strata entity`

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M9_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M9_ARCHITECTURE.md`
2. **Epic Spec**: `docs/milestones/M9/EPIC_63_ERROR_STANDARDIZATION.md`
3. **Prompt Header**: `docs/prompts/M9/M9_PROMPT_HEADER.md`

---

## Epic 63 Overview

### Scope
- Define StrataError enum with all variants
- Implement From conversions from all primitive errors
- Include EntityRef in error messages for context
- Document error handling guidelines

### Key Principle: Errors Have Context

> Every error includes enough context to understand what went wrong and where.

```rust
// CORRECT: Error includes EntityRef
StrataError::NotFound {
    entity_ref: EntityRef::kv(run_id, "missing-key"),
}

// WRONG: Error without context
StrataError::NotFound  // What wasn't found?
```

### Component Breakdown

| Story | Description | Priority |
|-------|-------------|----------|
| #488 | StrataError Enum Definition | FOUNDATION |
| #489 | Error Conversion from Primitive Errors | CRITICAL |
| #490 | EntityRef in Error Messages | HIGH |
| #491 | Error Documentation and Guidelines | HIGH |

---

## Story #488: StrataError Enum Definition

**GitHub Issue**: [#488](https://github.com/anibjoshi/in-mem/issues/488)
**Estimated Time**: 2 hours
**Dependencies**: Story #469 (EntityRef)
**Blocks**: Stories #489, #490

### Start Story

```bash
gh issue view 488
./scripts/start-story.sh 63 488 strata-error
```

### Implementation

Create or modify `crates/core/src/error.rs`:

```rust
//! Unified error type for all Strata operations

use crate::{EntityRef, Version, RunId};
use std::error::Error;
use std::fmt;

/// Unified error type for all Strata operations
///
/// StrataError provides consistent error handling across all primitives.
/// Every error includes context about what entity was involved.
#[derive(Debug)]
pub enum StrataError {
    /// Entity not found
    NotFound {
        entity_ref: EntityRef,
    },

    /// Version conflict (CAS failure)
    VersionConflict {
        entity_ref: EntityRef,
        expected: Version,
        actual: Version,
    },

    /// Write conflict (concurrent modification)
    WriteConflict {
        entity_ref: EntityRef,
    },

    /// Transaction was aborted
    TransactionAborted {
        reason: String,
    },

    /// Run does not exist
    RunNotFound {
        run_id: RunId,
    },

    /// Invalid operation for this entity
    InvalidOperation {
        entity_ref: EntityRef,
        reason: String,
    },

    /// Vector dimension mismatch
    DimensionMismatch {
        expected: usize,
        got: usize,
    },

    /// Collection not found
    CollectionNotFound {
        run_id: RunId,
        collection: String,
    },

    /// Storage layer error
    Storage {
        message: String,
        source: Option<Box<dyn Error + Send + Sync>>,
    },

    /// Serialization/deserialization error
    Serialization {
        message: String,
    },
}

impl fmt::Display for StrataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StrataError::NotFound { entity_ref } => {
                write!(f, "Entity not found: {}", entity_ref)
            }
            StrataError::VersionConflict { entity_ref, expected, actual } => {
                write!(
                    f,
                    "Version conflict for {}: expected {}, got {}",
                    entity_ref, expected, actual
                )
            }
            StrataError::WriteConflict { entity_ref } => {
                write!(f, "Write conflict for {}", entity_ref)
            }
            StrataError::TransactionAborted { reason } => {
                write!(f, "Transaction aborted: {}", reason)
            }
            StrataError::RunNotFound { run_id } => {
                write!(f, "Run not found: {}", run_id)
            }
            StrataError::InvalidOperation { entity_ref, reason } => {
                write!(f, "Invalid operation on {}: {}", entity_ref, reason)
            }
            StrataError::DimensionMismatch { expected, got } => {
                write!(f, "Dimension mismatch: expected {}, got {}", expected, got)
            }
            StrataError::CollectionNotFound { run_id, collection } => {
                write!(f, "Collection '{}' not found in run {}", collection, run_id)
            }
            StrataError::Storage { message, .. } => {
                write!(f, "Storage error: {}", message)
            }
            StrataError::Serialization { message } => {
                write!(f, "Serialization error: {}", message)
            }
        }
    }
}

impl Error for StrataError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            StrataError::Storage { source, .. } => {
                source.as_ref().map(|s| s.as_ref() as &(dyn Error + 'static))
            }
            _ => None,
        }
    }
}

impl StrataError {
    /// Check if this is a not-found error
    pub fn is_not_found(&self) -> bool {
        matches!(self, StrataError::NotFound { .. })
    }

    /// Check if this is a version conflict
    pub fn is_version_conflict(&self) -> bool {
        matches!(self, StrataError::VersionConflict { .. })
    }

    /// Check if this is a transaction abort
    pub fn is_transaction_aborted(&self) -> bool {
        matches!(self, StrataError::TransactionAborted { .. })
    }
}
```

### Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_not_found_display() {
        let run_id = RunId::new("test");
        let error = StrataError::NotFound {
            entity_ref: EntityRef::kv(run_id, "missing"),
        };

        let msg = format!("{}", error);
        assert!(msg.contains("not found"));
        assert!(msg.contains("missing"));
    }

    #[test]
    fn test_version_conflict_display() {
        let run_id = RunId::new("test");
        let error = StrataError::VersionConflict {
            entity_ref: EntityRef::state(run_id, "counter"),
            expected: Version::Counter(5),
            actual: Version::Counter(7),
        };

        let msg = format!("{}", error);
        assert!(msg.contains("Version conflict"));
        assert!(msg.contains("counter"));
    }

    #[test]
    fn test_error_is_methods() {
        let run_id = RunId::new("test");

        let not_found = StrataError::NotFound {
            entity_ref: EntityRef::kv(run_id.clone(), "key"),
        };
        assert!(not_found.is_not_found());
        assert!(!not_found.is_version_conflict());

        let aborted = StrataError::TransactionAborted {
            reason: "test".into(),
        };
        assert!(aborted.is_transaction_aborted());
    }

    #[test]
    fn test_storage_error_source() {
        let source_error = std::io::Error::new(std::io::ErrorKind::Other, "disk full");
        let error = StrataError::Storage {
            message: "write failed".into(),
            source: Some(Box::new(source_error)),
        };

        assert!(error.source().is_some());
    }
}
```

### Validation

```bash
~/.cargo/bin/cargo test -p in-mem-core -- error
~/.cargo/bin/cargo clippy -p in-mem-core -- -D warnings
```

### Complete Story

```bash
./scripts/complete-story.sh 488
```

---

## Story #489: Error Conversion from Primitive Errors

**GitHub Issue**: [#489](https://github.com/anibjoshi/in-mem/issues/489)
**Estimated Time**: 3 hours
**Dependencies**: Story #488
**Blocks**: Epic 61, 62 error handling

### Start Story

```bash
gh issue view 489
./scripts/start-story.sh 63 489 error-conversion
```

### Implementation

Add to `crates/core/src/error.rs`:

```rust
// ============================================================================
// From implementations for primitive errors
// ============================================================================

impl From<KvError> for StrataError {
    fn from(err: KvError) -> Self {
        match err {
            KvError::NotFound { run_id, key } => StrataError::NotFound {
                entity_ref: EntityRef::kv(run_id, key),
            },
            KvError::Storage(msg) => StrataError::Storage {
                message: msg,
                source: None,
            },
            KvError::Serialization(msg) => StrataError::Serialization {
                message: msg,
            },
        }
    }
}

impl From<EventError> for StrataError {
    fn from(err: EventError) -> Self {
        match err {
            EventError::NotFound { run_id, sequence } => StrataError::NotFound {
                entity_ref: EntityRef::event(run_id, sequence),
            },
            EventError::Storage(msg) => StrataError::Storage {
                message: msg,
                source: None,
            },
        }
    }
}

impl From<StateError> for StrataError {
    fn from(err: StateError) -> Self {
        match err {
            StateError::NotFound { run_id, name } => StrataError::NotFound {
                entity_ref: EntityRef::state(run_id, name),
            },
            StateError::CasMismatch { run_id, name, expected, actual } => {
                StrataError::VersionConflict {
                    entity_ref: EntityRef::state(run_id, name),
                    expected: Version::Counter(expected),
                    actual: Version::Counter(actual),
                }
            }
            StateError::Storage(msg) => StrataError::Storage {
                message: msg,
                source: None,
            },
        }
    }
}

impl From<TraceError> for StrataError {
    fn from(err: TraceError) -> Self {
        match err {
            TraceError::NotFound { run_id, trace_id } => StrataError::NotFound {
                entity_ref: EntityRef::trace(run_id, trace_id),
            },
            TraceError::Storage(msg) => StrataError::Storage {
                message: msg,
                source: None,
            },
        }
    }
}

impl From<JsonError> for StrataError {
    fn from(err: JsonError) -> Self {
        match err {
            JsonError::NotFound { run_id, doc_id } => StrataError::NotFound {
                entity_ref: EntityRef::json(run_id, doc_id),
            },
            JsonError::PathNotFound { run_id, doc_id, path: _ } => StrataError::NotFound {
                entity_ref: EntityRef::json(run_id, doc_id),
            },
            JsonError::Storage(msg) => StrataError::Storage {
                message: msg,
                source: None,
            },
            JsonError::Serialization(msg) => StrataError::Serialization {
                message: msg,
            },
        }
    }
}

impl From<VectorError> for StrataError {
    fn from(err: VectorError) -> Self {
        match err {
            VectorError::NotFound { run_id, collection, vector_id } => StrataError::NotFound {
                entity_ref: EntityRef::vector(run_id, collection, vector_id),
            },
            VectorError::CollectionNotFound { run_id, collection } => {
                StrataError::CollectionNotFound { run_id, collection }
            }
            VectorError::DimensionMismatch { expected, got } => {
                StrataError::DimensionMismatch { expected, got }
            }
            VectorError::Storage(msg) => StrataError::Storage {
                message: msg,
                source: None,
            },
        }
    }
}

impl From<RunError> for StrataError {
    fn from(err: RunError) -> Self {
        match err {
            RunError::NotFound { run_id } => StrataError::RunNotFound { run_id },
            RunError::AlreadyExists { run_id } => StrataError::InvalidOperation {
                entity_ref: EntityRef::run(run_id.clone()),
                reason: format!("Run {} already exists", run_id),
            },
            RunError::Storage(msg) => StrataError::Storage {
                message: msg,
                source: None,
            },
        }
    }
}

// Also implement From for std::io::Error
impl From<std::io::Error> for StrataError {
    fn from(err: std::io::Error) -> Self {
        StrataError::Storage {
            message: err.to_string(),
            source: Some(Box::new(err)),
        }
    }
}
```

### Tests

```rust
#[test]
fn test_kv_error_conversion() {
    let kv_error = KvError::NotFound {
        run_id: RunId::new("test"),
        key: "missing".into(),
    };

    let strata_error: StrataError = kv_error.into();

    assert!(strata_error.is_not_found());
    if let StrataError::NotFound { entity_ref } = strata_error {
        assert_eq!(entity_ref.primitive_type(), PrimitiveType::Kv);
    }
}

#[test]
fn test_state_cas_error_conversion() {
    let state_error = StateError::CasMismatch {
        run_id: RunId::new("test"),
        name: "counter".into(),
        expected: 5,
        actual: 7,
    };

    let strata_error: StrataError = state_error.into();

    assert!(strata_error.is_version_conflict());
    if let StrataError::VersionConflict { expected, actual, .. } = strata_error {
        assert_eq!(expected, Version::Counter(5));
        assert_eq!(actual, Version::Counter(7));
    }
}

#[test]
fn test_vector_dimension_error_conversion() {
    let vector_error = VectorError::DimensionMismatch {
        expected: 384,
        got: 768,
    };

    let strata_error: StrataError = vector_error.into();

    if let StrataError::DimensionMismatch { expected, got } = strata_error {
        assert_eq!(expected, 384);
        assert_eq!(got, 768);
    } else {
        panic!("Expected DimensionMismatch");
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 489
```

---

## Story #490: EntityRef in Error Messages

**GitHub Issue**: [#490](https://github.com/anibjoshi/in-mem/issues/490)
**Estimated Time**: 2 hours
**Dependencies**: Story #488

### Start Story

```bash
gh issue view 490
./scripts/start-story.sh 63 490 error-entity-ref
```

### Implementation

Add helper methods to `StrataError`:

```rust
impl StrataError {
    /// Get the entity reference if this error is about a specific entity
    pub fn entity_ref(&self) -> Option<&EntityRef> {
        match self {
            StrataError::NotFound { entity_ref } => Some(entity_ref),
            StrataError::VersionConflict { entity_ref, .. } => Some(entity_ref),
            StrataError::WriteConflict { entity_ref } => Some(entity_ref),
            StrataError::InvalidOperation { entity_ref, .. } => Some(entity_ref),
            _ => None,
        }
    }

    /// Get the run ID if this error is associated with a run
    pub fn run_id(&self) -> Option<&RunId> {
        match self {
            StrataError::NotFound { entity_ref } => Some(entity_ref.run_id()),
            StrataError::VersionConflict { entity_ref, .. } => Some(entity_ref.run_id()),
            StrataError::WriteConflict { entity_ref } => Some(entity_ref.run_id()),
            StrataError::InvalidOperation { entity_ref, .. } => Some(entity_ref.run_id()),
            StrataError::RunNotFound { run_id } => Some(run_id),
            StrataError::CollectionNotFound { run_id, .. } => Some(run_id),
            _ => None,
        }
    }

    /// Get the primitive type if this error is about a specific primitive
    pub fn primitive_type(&self) -> Option<PrimitiveType> {
        self.entity_ref().map(|e| e.primitive_type())
    }

    // =========================================================================
    // Convenience Constructors
    // =========================================================================

    /// Create a NotFound error for a KV key
    pub fn kv_not_found(run_id: RunId, key: impl Into<String>) -> Self {
        StrataError::NotFound {
            entity_ref: EntityRef::kv(run_id, key.into()),
        }
    }

    /// Create a NotFound error for an event
    pub fn event_not_found(run_id: RunId, sequence: u64) -> Self {
        StrataError::NotFound {
            entity_ref: EntityRef::event(run_id, sequence),
        }
    }

    /// Create a NotFound error for a state cell
    pub fn state_not_found(run_id: RunId, name: impl Into<String>) -> Self {
        StrataError::NotFound {
            entity_ref: EntityRef::state(run_id, name.into()),
        }
    }

    /// Create a VersionConflict error for a state cell
    pub fn state_version_conflict(
        run_id: RunId,
        name: impl Into<String>,
        expected: u64,
        actual: u64,
    ) -> Self {
        StrataError::VersionConflict {
            entity_ref: EntityRef::state(run_id, name.into()),
            expected: Version::Counter(expected),
            actual: Version::Counter(actual),
        }
    }

    /// Create a NotFound error for a JSON document
    pub fn json_not_found(run_id: RunId, doc_id: JsonDocId) -> Self {
        StrataError::NotFound {
            entity_ref: EntityRef::json(run_id, doc_id),
        }
    }

    /// Create a NotFound error for a vector
    pub fn vector_not_found(run_id: RunId, collection: impl Into<String>, vector_id: VectorId) -> Self {
        StrataError::NotFound {
            entity_ref: EntityRef::vector(run_id, collection.into(), vector_id),
        }
    }
}
```

### Tests

```rust
#[test]
fn test_error_entity_ref() {
    let run_id = RunId::new("test");
    let error = StrataError::kv_not_found(run_id.clone(), "key");

    let entity_ref = error.entity_ref().unwrap();
    assert_eq!(entity_ref.run_id(), &run_id);
    assert_eq!(entity_ref.primitive_type(), PrimitiveType::Kv);
}

#[test]
fn test_error_run_id() {
    let run_id = RunId::new("test");
    let error = StrataError::RunNotFound { run_id: run_id.clone() };

    assert_eq!(error.run_id(), Some(&run_id));
}

#[test]
fn test_error_primitive_type() {
    let run_id = RunId::new("test");
    let error = StrataError::state_not_found(run_id, "counter");

    assert_eq!(error.primitive_type(), Some(PrimitiveType::State));
}

#[test]
fn test_convenience_constructors() {
    let run_id = RunId::new("test");

    let kv_error = StrataError::kv_not_found(run_id.clone(), "key");
    assert!(kv_error.is_not_found());

    let state_conflict = StrataError::state_version_conflict(run_id.clone(), "cnt", 5, 7);
    assert!(state_conflict.is_version_conflict());
}
```

### Complete Story

```bash
./scripts/complete-story.sh 490
```

---

## Story #491: Error Documentation and Guidelines

**GitHub Issue**: [#491](https://github.com/anibjoshi/in-mem/issues/491)
**Estimated Time**: 2 hours
**Dependencies**: Story #488

### Start Story

```bash
gh issue view 491
./scripts/start-story.sh 63 491 error-docs
```

### Implementation

Create `docs/architecture/ERROR_HANDLING.md`:

```markdown
# Strata Error Handling

## Error Philosophy

Errors in Strata are:
1. **Typed**: StrataError enum with specific variants
2. **Contextual**: Include EntityRef when applicable
3. **Actionable**: Messages explain what happened and what to do
4. **Consistent**: Same error type across all primitives

## StrataError Variants

### NotFound
Returned when an entity doesn't exist.
- **Contains**: EntityRef identifying the missing entity
- **Recovery**: Create the entity or handle absence

```rust
match result {
    Err(StrataError::NotFound { entity_ref }) => {
        println!("Missing: {}", entity_ref);
        // Create or handle
    }
}
```

### VersionConflict
Returned when a CAS operation fails due to version mismatch.
- **Contains**: EntityRef, expected version, actual version
- **Recovery**: Re-read, recalculate, retry

```rust
match result {
    Err(StrataError::VersionConflict { expected, actual, .. }) => {
        println!("Expected {}, got {}", expected, actual);
        // Re-read and retry
    }
}
```

### WriteConflict
Returned when concurrent writes conflict.
- **Contains**: EntityRef identifying the conflicting entity
- **Recovery**: Retry the transaction

### TransactionAborted
Returned when a transaction cannot complete.
- **Contains**: Reason string
- **Recovery**: Retry or handle failure

### RunNotFound
Returned when a run doesn't exist.
- **Contains**: RunId
- **Recovery**: Create the run first

### InvalidOperation
Returned when an operation is invalid for the entity.
- **Contains**: EntityRef, reason
- **Recovery**: Check operation validity

### DimensionMismatch
Returned when vector dimensions don't match.
- **Contains**: Expected and got dimensions
- **Recovery**: Ensure consistent dimensions

### CollectionNotFound
Returned when a vector collection doesn't exist.
- **Contains**: RunId, collection name
- **Recovery**: Create the collection first

### Storage
Returned for underlying storage failures.
- **Contains**: Message, optional source error
- **Recovery**: Retry or escalate

### Serialization
Returned for serialization/deserialization failures.
- **Contains**: Message
- **Recovery**: Check data format

## Error Handling Patterns

### Pattern 1: Match Specific Errors
```rust
match result {
    Ok(value) => use_value(value),
    Err(StrataError::NotFound { entity_ref }) => {
        // Handle missing entity
    }
    Err(StrataError::VersionConflict { .. }) => {
        // Retry with fresh data
    }
    Err(e) => return Err(e),
}
```

### Pattern 2: Use ? for Propagation
```rust
let value = txn.kv_get("key")?;
```

### Pattern 3: Extract Context
```rust
if let Err(e) = result {
    if let Some(entity_ref) = e.entity_ref() {
        log::error!("Operation failed on {}", entity_ref);
    }
    if let Some(run_id) = e.run_id() {
        log::error!("In run {}", run_id);
    }
}
```

### Pattern 4: Type-specific handling
```rust
if let Err(e) = result {
    if e.is_not_found() {
        // Handle not found
    } else if e.is_version_conflict() {
        // Handle conflict
    }
}
```

## Guidelines

1. **Always propagate errors** - Don't swallow errors silently
2. **Add context when wrapping** - If you wrap an error, add helpful context
3. **Use typed errors** - Don't use string errors
4. **Check entity existence first** - If you need an entity to exist, verify before operating
5. **Handle version conflicts gracefully** - Retry with fresh data
6. **Log with entity context** - Include entity_ref in log messages
```

### Update lib.rs with module docs

```rust
//! # Error Handling
//!
//! All Strata operations return `Result<T, StrataError>`.
//!
//! See `docs/architecture/ERROR_HANDLING.md` for comprehensive guidelines.
//!
//! ## Quick Reference
//!
//! ```rust
//! use strata::{StrataError, EntityRef};
//!
//! match result {
//!     Ok(value) => { /* use value */ }
//!     Err(StrataError::NotFound { entity_ref }) => {
//!         println!("Not found: {}", entity_ref);
//!     }
//!     Err(e) => return Err(e),
//! }
//! ```
```

### Complete Story

```bash
./scripts/complete-story.sh 491
```

---

## Epic 63 Completion Checklist

### 1. Final Validation

```bash
~/.cargo/bin/cargo test -p in-mem-core -- error
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace -- -D warnings
```

### 2. Verify Deliverables

- [ ] StrataError enum with all variants
- [ ] Display impl with human-readable messages
- [ ] Error impl with source()
- [ ] From impls for all primitive errors
- [ ] entity_ref(), run_id(), primitive_type() helpers
- [ ] Convenience constructors
- [ ] ERROR_HANDLING.md documentation

### 3. Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-63-error-standardization -m "Epic 63: Error Standardization complete

Delivered:
- StrataError unified error type
- From conversions for all primitive errors
- EntityRef context in errors
- Error handling documentation

Stories: #488, #489, #490, #491
"
git push origin develop
gh issue close 467 --comment "Epic 63: Error Standardization - COMPLETE"
```
