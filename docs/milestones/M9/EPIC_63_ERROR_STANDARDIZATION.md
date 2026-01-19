# Epic 63: Error Standardization

**Goal**: Unified StrataError across all primitives

**Dependencies**: Epic 60 (Core Types)

---

## Scope

- Define StrataError enum with all error variants
- Implement From conversions from all primitive errors
- Include EntityRef in error messages for debugging
- Document error handling guidelines
- Ensure consistent error patterns across all primitives

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #479 | StrataError Enum Definition | FOUNDATION |
| #480 | Error Conversion from Primitive Errors | CRITICAL |
| #481 | EntityRef in Error Messages | HIGH |
| #482 | Error Documentation and Guidelines | HIGH |

---

## Story #479: StrataError Enum Definition

**File**: `crates/core/src/error.rs` (NEW or MODIFY)

**Deliverable**: Unified error type for all Strata operations

### Implementation

```rust
use crate::{EntityRef, Version, RunId};
use thiserror::Error;

/// Errors from Strata operations
///
/// This is the unified error type returned by all Strata APIs.
/// It provides consistent error handling across all primitives.
///
/// ## Error Categories
///
/// - **Not Found**: Entity doesn't exist
/// - **Conflict**: Version mismatch or concurrent modification
/// - **Transaction**: Transaction-level failures
/// - **Validation**: Invalid input or operation
/// - **Storage**: Low-level storage failures
///
/// ## Usage
///
/// ```rust
/// match result {
///     Err(StrataError::NotFound { entity_ref }) => {
///         println!("Entity not found: {}", entity_ref);
///     }
///     Err(StrataError::VersionConflict { expected, actual, .. }) => {
///         println!("Conflict: expected {:?}, got {:?}", expected, actual);
///     }
///     Err(e) => {
///         println!("Other error: {}", e);
///     }
///     Ok(value) => { /* success */ }
/// }
/// ```
#[derive(Debug, Error)]
pub enum StrataError {
    // =========================================================================
    // Not Found Errors
    // =========================================================================

    /// Entity not found
    ///
    /// The referenced entity does not exist. This could be a key, document,
    /// event, or any other entity type.
    #[error("not found: {entity_ref}")]
    NotFound {
        /// Reference to the entity that was not found
        entity_ref: EntityRef,
    },

    /// Run not found
    ///
    /// The specified run does not exist. This is separate from NotFound
    /// because runs are meta-level entities.
    #[error("run not found: {run_id}")]
    RunNotFound {
        /// ID of the run that was not found
        run_id: RunId,
    },

    // =========================================================================
    // Conflict Errors
    // =========================================================================

    /// Version conflict
    ///
    /// The operation failed because the entity's version doesn't match
    /// the expected version. This typically happens with:
    /// - Compare-and-swap (CAS) operations
    /// - Optimistic concurrency control conflicts
    #[error("version conflict on {entity_ref}: expected {expected}, got {actual}")]
    VersionConflict {
        /// Reference to the conflicted entity
        entity_ref: EntityRef,
        /// The version that was expected
        expected: Version,
        /// The actual version found
        actual: Version,
    },

    /// Write conflict
    ///
    /// Two transactions attempted to modify the same entity concurrently.
    /// The transaction should be retried.
    #[error("write conflict on {entity_ref}")]
    WriteConflict {
        /// Reference to the conflicted entity
        entity_ref: EntityRef,
    },

    // =========================================================================
    // Transaction Errors
    // =========================================================================

    /// Transaction aborted
    ///
    /// The transaction was aborted due to a conflict, timeout, or other
    /// transactional failure. The reason field provides details.
    #[error("transaction aborted: {reason}")]
    TransactionAborted {
        /// Reason for the abort
        reason: String,
    },

    /// Transaction timeout
    ///
    /// The transaction exceeded the maximum allowed duration.
    #[error("transaction timeout after {duration_ms}ms")]
    TransactionTimeout {
        /// How long the transaction ran before timing out
        duration_ms: u64,
    },

    /// Transaction not active
    ///
    /// An operation was attempted on a transaction that has already
    /// been committed or rolled back.
    #[error("transaction not active (already {state})")]
    TransactionNotActive {
        /// Current state of the transaction
        state: String,
    },

    // =========================================================================
    // Validation Errors
    // =========================================================================

    /// Invalid operation
    ///
    /// The operation is not valid for the current state of the entity.
    /// Examples: creating a document that exists, deleting a required entity.
    #[error("invalid operation on {entity_ref}: {reason}")]
    InvalidOperation {
        /// Reference to the entity
        entity_ref: EntityRef,
        /// Why the operation is invalid
        reason: String,
    },

    /// Invalid input
    ///
    /// The input parameters are invalid.
    #[error("invalid input: {message}")]
    InvalidInput {
        /// Description of what's wrong with the input
        message: String,
    },

    /// Dimension mismatch (Vector-specific)
    ///
    /// The vector dimension doesn't match the collection's configured dimension.
    #[error("dimension mismatch: expected {expected}, got {got}")]
    DimensionMismatch {
        /// Expected dimension
        expected: usize,
        /// Actual dimension provided
        got: usize,
    },

    /// Path not found (JSON-specific)
    ///
    /// The specified path doesn't exist in the JSON document.
    #[error("path not found in {entity_ref}: {path}")]
    PathNotFound {
        /// Reference to the JSON document
        entity_ref: EntityRef,
        /// The path that wasn't found
        path: String,
    },

    // =========================================================================
    // Storage Errors
    // =========================================================================

    /// Storage error
    ///
    /// Low-level storage operation failed.
    #[error("storage error: {message}")]
    Storage {
        /// Error message
        message: String,
        /// Optional underlying error
        #[source]
        source: Option<Box<dyn std::error::Error + Send + Sync>>,
    },

    /// Serialization error
    ///
    /// Failed to serialize or deserialize data.
    #[error("serialization error: {message}")]
    Serialization {
        /// What went wrong
        message: String,
    },

    /// Corruption detected
    ///
    /// Data integrity check failed. This is a serious error that may
    /// require recovery from backup.
    #[error("corruption detected: {message}")]
    Corruption {
        /// Description of the corruption
        message: String,
    },

    // =========================================================================
    // Resource Errors
    // =========================================================================

    /// Capacity exceeded
    ///
    /// A resource limit was exceeded.
    #[error("capacity exceeded: {resource} (limit: {limit}, requested: {requested})")]
    CapacityExceeded {
        /// What resource was exceeded
        resource: String,
        /// The limit
        limit: usize,
        /// What was requested
        requested: usize,
    },

    /// Budget exceeded
    ///
    /// The operation exceeded its computational budget.
    #[error("budget exceeded: {operation}")]
    BudgetExceeded {
        /// What operation exceeded its budget
        operation: String,
    },

    // =========================================================================
    // Internal Errors
    // =========================================================================

    /// Internal error
    ///
    /// An unexpected internal error occurred. This indicates a bug.
    #[error("internal error: {message}")]
    Internal {
        /// Error message
        message: String,
    },
}

impl StrataError {
    // === Constructors ===

    /// Create a NotFound error
    pub fn not_found(entity_ref: EntityRef) -> Self {
        StrataError::NotFound { entity_ref }
    }

    /// Create a RunNotFound error
    pub fn run_not_found(run_id: RunId) -> Self {
        StrataError::RunNotFound { run_id }
    }

    /// Create a VersionConflict error
    pub fn version_conflict(entity_ref: EntityRef, expected: Version, actual: Version) -> Self {
        StrataError::VersionConflict {
            entity_ref,
            expected,
            actual,
        }
    }

    /// Create a WriteConflict error
    pub fn write_conflict(entity_ref: EntityRef) -> Self {
        StrataError::WriteConflict { entity_ref }
    }

    /// Create an InvalidOperation error
    pub fn invalid_operation(entity_ref: EntityRef, reason: impl Into<String>) -> Self {
        StrataError::InvalidOperation {
            entity_ref,
            reason: reason.into(),
        }
    }

    /// Create an InvalidInput error
    pub fn invalid_input(message: impl Into<String>) -> Self {
        StrataError::InvalidInput {
            message: message.into(),
        }
    }

    /// Create a DimensionMismatch error
    pub fn dimension_mismatch(expected: usize, got: usize) -> Self {
        StrataError::DimensionMismatch { expected, got }
    }

    /// Create a Storage error
    pub fn storage(message: impl Into<String>) -> Self {
        StrataError::Storage {
            message: message.into(),
            source: None,
        }
    }

    /// Create a Storage error with source
    pub fn storage_with_source(
        message: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        StrataError::Storage {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }

    /// Create a Serialization error
    pub fn serialization(message: impl Into<String>) -> Self {
        StrataError::Serialization {
            message: message.into(),
        }
    }

    /// Create an Internal error
    pub fn internal(message: impl Into<String>) -> Self {
        StrataError::Internal {
            message: message.into(),
        }
    }

    // === Classification Methods ===

    /// Check if this is a "not found" type error
    pub fn is_not_found(&self) -> bool {
        matches!(
            self,
            StrataError::NotFound { .. }
                | StrataError::RunNotFound { .. }
                | StrataError::PathNotFound { .. }
        )
    }

    /// Check if this is a conflict error
    pub fn is_conflict(&self) -> bool {
        matches!(
            self,
            StrataError::VersionConflict { .. } | StrataError::WriteConflict { .. }
        )
    }

    /// Check if this is a transaction error
    pub fn is_transaction_error(&self) -> bool {
        matches!(
            self,
            StrataError::TransactionAborted { .. }
                | StrataError::TransactionTimeout { .. }
                | StrataError::TransactionNotActive { .. }
        )
    }

    /// Check if this is a validation error
    pub fn is_validation_error(&self) -> bool {
        matches!(
            self,
            StrataError::InvalidOperation { .. }
                | StrataError::InvalidInput { .. }
                | StrataError::DimensionMismatch { .. }
        )
    }

    /// Check if this error is retryable
    ///
    /// Retryable errors are typically conflicts that may succeed on retry.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            StrataError::VersionConflict { .. }
                | StrataError::WriteConflict { .. }
                | StrataError::TransactionAborted { .. }
        )
    }

    /// Check if this is a serious/unrecoverable error
    pub fn is_serious(&self) -> bool {
        matches!(
            self,
            StrataError::Corruption { .. } | StrataError::Internal { .. }
        )
    }

    /// Get the entity reference if this error is about a specific entity
    pub fn entity_ref(&self) -> Option<&EntityRef> {
        match self {
            StrataError::NotFound { entity_ref } => Some(entity_ref),
            StrataError::VersionConflict { entity_ref, .. } => Some(entity_ref),
            StrataError::WriteConflict { entity_ref } => Some(entity_ref),
            StrataError::InvalidOperation { entity_ref, .. } => Some(entity_ref),
            StrataError::PathNotFound { entity_ref, .. } => Some(entity_ref),
            _ => None,
        }
    }
}

/// Result type alias for Strata operations
pub type StrataResult<T> = Result<T, StrataError>;
```

### Acceptance Criteria

- [ ] StrataError enum with all variants
- [ ] Uses thiserror for Display and Error impl
- [ ] Constructors for common errors
- [ ] Classification methods: is_not_found(), is_conflict(), etc.
- [ ] is_retryable() for retry logic
- [ ] entity_ref() accessor
- [ ] StrataResult<T> type alias

---

## Story #480: Error Conversion from Primitive Errors

**File**: `crates/core/src/error.rs`

**Deliverable**: From implementations for all primitive error types

### Implementation

```rust
// === KV Error Conversion ===

impl From<KvError> for StrataError {
    fn from(e: KvError) -> Self {
        match e {
            KvError::NotFound { run_id, key } => StrataError::NotFound {
                entity_ref: EntityRef::kv(run_id, key),
            },
            KvError::Storage(msg) => StrataError::Storage {
                message: msg,
                source: None,
            },
            KvError::Serialization(msg) => StrataError::Serialization { message: msg },
        }
    }
}

// === Event Error Conversion ===

impl From<EventError> for StrataError {
    fn from(e: EventError) -> Self {
        match e {
            EventError::NotFound { run_id, sequence } => StrataError::NotFound {
                entity_ref: EntityRef::event(run_id, sequence),
            },
            EventError::SequenceGap { expected, got } => StrataError::InvalidOperation {
                entity_ref: EntityRef::event(RunId::new("unknown"), expected),
                reason: format!("Sequence gap: expected {}, got {}", expected, got),
            },
            EventError::Storage(msg) => StrataError::Storage {
                message: msg,
                source: None,
            },
        }
    }
}

// === State Error Conversion ===

impl From<StateError> for StrataError {
    fn from(e: StateError) -> Self {
        match e {
            StateError::NotFound { run_id, name } => StrataError::NotFound {
                entity_ref: EntityRef::state(run_id, name),
            },
            StateError::VersionMismatch {
                run_id,
                name,
                expected,
                actual,
            } => StrataError::VersionConflict {
                entity_ref: EntityRef::state(run_id, name),
                expected: Version::Counter(expected),
                actual: Version::Counter(actual),
            },
            StateError::AlreadyExists { run_id, name } => StrataError::InvalidOperation {
                entity_ref: EntityRef::state(run_id, name),
                reason: "State cell already exists".to_string(),
            },
            StateError::Storage(msg) => StrataError::Storage {
                message: msg,
                source: None,
            },
        }
    }
}

// === Trace Error Conversion ===

impl From<TraceError> for StrataError {
    fn from(e: TraceError) -> Self {
        match e {
            TraceError::NotFound { run_id, trace_id } => StrataError::NotFound {
                entity_ref: EntityRef::trace(run_id, trace_id),
            },
            TraceError::InvalidTraceType(msg) => StrataError::InvalidInput { message: msg },
            TraceError::Storage(msg) => StrataError::Storage {
                message: msg,
                source: None,
            },
        }
    }
}

// === Json Error Conversion ===

impl From<JsonError> for StrataError {
    fn from(e: JsonError) -> Self {
        match e {
            JsonError::NotFound { run_id, doc_id } => StrataError::NotFound {
                entity_ref: EntityRef::json(run_id, doc_id),
            },
            JsonError::PathNotFound { run_id, doc_id, path } => StrataError::PathNotFound {
                entity_ref: EntityRef::json(run_id, doc_id),
                path,
            },
            JsonError::AlreadyExists { run_id, doc_id } => StrataError::InvalidOperation {
                entity_ref: EntityRef::json(run_id, doc_id),
                reason: "Document already exists".to_string(),
            },
            JsonError::InvalidPath(msg) => StrataError::InvalidInput {
                message: format!("Invalid JSON path: {}", msg),
            },
            JsonError::InvalidPatch(msg) => StrataError::InvalidInput {
                message: format!("Invalid JSON patch: {}", msg),
            },
            JsonError::Storage(msg) => StrataError::Storage {
                message: msg,
                source: None,
            },
            JsonError::Serialization(msg) => StrataError::Serialization { message: msg },
        }
    }
}

// === Vector Error Conversion ===

impl From<VectorError> for StrataError {
    fn from(e: VectorError) -> Self {
        match e {
            VectorError::CollectionNotFound { name } => StrataError::NotFound {
                entity_ref: EntityRef::vector(
                    RunId::new("unknown"),
                    name,
                    VectorId::new(0),
                ),
            },
            VectorError::CollectionAlreadyExists { name } => StrataError::InvalidOperation {
                entity_ref: EntityRef::vector(
                    RunId::new("unknown"),
                    name,
                    VectorId::new(0),
                ),
                reason: "Collection already exists".to_string(),
            },
            VectorError::VectorNotFound { key } => StrataError::NotFound {
                entity_ref: EntityRef::vector(
                    RunId::new("unknown"),
                    "unknown",
                    VectorId::new(0),
                ),
            },
            VectorError::DimensionMismatch { expected, got } => {
                StrataError::DimensionMismatch { expected, got }
            }
            VectorError::InvalidDimension { dimension } => StrataError::InvalidInput {
                message: format!("Invalid dimension: {} (must be > 0)", dimension),
            },
            VectorError::EmptyEmbedding => StrataError::InvalidInput {
                message: "Empty embedding".to_string(),
            },
            VectorError::InvalidCollectionName { name, reason } => StrataError::InvalidInput {
                message: format!("Invalid collection name '{}': {}", name, reason),
            },
            VectorError::InvalidKey { key, reason } => StrataError::InvalidInput {
                message: format!("Invalid key '{}': {}", key, reason),
            },
            VectorError::ConfigMismatch { field } => StrataError::InvalidOperation {
                entity_ref: EntityRef::vector(
                    RunId::new("unknown"),
                    "unknown",
                    VectorId::new(0),
                ),
                reason: format!("Config field '{}' cannot be changed", field),
            },
            VectorError::SearchLimitExceeded { requested, max } => {
                StrataError::CapacityExceeded {
                    resource: "search results".to_string(),
                    limit: max,
                    requested,
                }
            }
            VectorError::Storage(e) => StrataError::Storage {
                message: e.to_string(),
                source: Some(Box::new(e)),
            },
            VectorError::Transaction(e) => StrataError::TransactionAborted {
                reason: e.to_string(),
            },
            VectorError::Serialization(msg) => StrataError::Serialization { message: msg },
            VectorError::Internal(msg) => StrataError::Internal { message: msg },
        }
    }
}

// === Run Error Conversion ===

impl From<RunError> for StrataError {
    fn from(e: RunError) -> Self {
        match e {
            RunError::NotFound { run_id } => StrataError::RunNotFound { run_id },
            RunError::AlreadyExists { run_id } => StrataError::InvalidOperation {
                entity_ref: EntityRef::run(run_id.clone()),
                reason: format!("Run '{}' already exists", run_id),
            },
            RunError::InvalidTransition { run_id, from, to } => StrataError::InvalidOperation {
                entity_ref: EntityRef::run(run_id),
                reason: format!("Invalid status transition: {:?} -> {:?}", from, to),
            },
            RunError::Storage(msg) => StrataError::Storage {
                message: msg,
                source: None,
            },
        }
    }
}

// === Storage Error Conversion ===

impl From<StorageError> for StrataError {
    fn from(e: StorageError) -> Self {
        StrataError::Storage {
            message: e.to_string(),
            source: Some(Box::new(e)),
        }
    }
}

// === Transaction Error Conversion ===

impl From<TransactionError> for StrataError {
    fn from(e: TransactionError) -> Self {
        match e {
            TransactionError::Conflict { key } => StrataError::WriteConflict {
                entity_ref: EntityRef::kv(RunId::new("unknown"), key),
            },
            TransactionError::Timeout { duration_ms } => {
                StrataError::TransactionTimeout { duration_ms }
            }
            TransactionError::NotActive { state } => {
                StrataError::TransactionNotActive { state }
            }
            TransactionError::Aborted { reason } => {
                StrataError::TransactionAborted { reason }
            }
        }
    }
}

// === Serde/JSON Error Conversion ===

impl From<serde_json::Error> for StrataError {
    fn from(e: serde_json::Error) -> Self {
        StrataError::Serialization {
            message: e.to_string(),
        }
    }
}

impl From<rmp_serde::encode::Error> for StrataError {
    fn from(e: rmp_serde::encode::Error) -> Self {
        StrataError::Serialization {
            message: format!("MessagePack encode error: {}", e),
        }
    }
}

impl From<rmp_serde::decode::Error> for StrataError {
    fn from(e: rmp_serde::decode::Error) -> Self {
        StrataError::Serialization {
            message: format!("MessagePack decode error: {}", e),
        }
    }
}

// === IO Error Conversion ===

impl From<std::io::Error> for StrataError {
    fn from(e: std::io::Error) -> Self {
        StrataError::Storage {
            message: format!("IO error: {}", e),
            source: Some(Box::new(e)),
        }
    }
}
```

### Acceptance Criteria

- [ ] From<KvError> for StrataError
- [ ] From<EventError> for StrataError
- [ ] From<StateError> for StrataError
- [ ] From<TraceError> for StrataError
- [ ] From<JsonError> for StrataError
- [ ] From<VectorError> for StrataError
- [ ] From<RunError> for StrataError
- [ ] From<StorageError> for StrataError
- [ ] From<TransactionError> for StrataError
- [ ] From<serde_json::Error> for StrataError
- [ ] From<std::io::Error> for StrataError
- [ ] All conversions preserve relevant context

---

## Story #481: EntityRef in Error Messages

**File**: `crates/core/src/error.rs`

**Deliverable**: EntityRef included in all entity-related errors

### Implementation

The EntityRef inclusion is already part of Story #479. This story ensures:

1. All error variants that relate to an entity include EntityRef
2. Error messages include the entity description
3. EntityRef provides enough context for debugging

### Error Message Examples

```rust
// NotFound
"not found: KV[my-run:config]"

// VersionConflict
"version conflict on State[my-run:counter]: expected cnt:5, got cnt:6"

// InvalidOperation
"invalid operation on Json[my-run:doc-123]: Document already exists"

// PathNotFound
"path not found in Json[my-run:doc-123]: /data/items/0/name"

// WriteConflict
"write conflict on KV[my-run:shared-key]"
```

### Acceptance Criteria

- [ ] All entity-related errors include EntityRef
- [ ] EntityRef::description() used in error messages
- [ ] Error messages are clear and actionable
- [ ] entity_ref() accessor returns the reference if present

---

## Story #482: Error Documentation and Guidelines

**File**: `crates/core/src/error.rs` (doc comments) + `docs/ERROR_HANDLING.md` (NEW)

**Deliverable**: Comprehensive error handling documentation

### Error Handling Guidelines

```markdown
# Strata Error Handling Guidelines

## Error Categories

### Not Found Errors

Returned when an entity doesn't exist.

```rust
match db.kv().get(&run_id, "key")? {
    Some(value) => { /* use value */ }
    None => { /* handle missing */ }
}

// Or check first
if !db.kv().exists(&run_id, "key")? {
    // Create it
}
```

### Conflict Errors

Returned when there's a version or write conflict. These are typically retryable.

```rust
// Compare-and-swap pattern
loop {
    let current = db.state().read(&run_id, "counter")?.unwrap();
    let new_value = current.value.as_i64().unwrap() + 1;

    match db.state().cas(&run_id, "counter", current.version, Value::from(new_value)) {
        Ok(new_version) => break,
        Err(StrataError::VersionConflict { .. }) => continue, // Retry
        Err(e) => return Err(e),
    }
}
```

### Transaction Errors

Handle transaction failures appropriately.

```rust
match db.transaction(&run_id, |txn| {
    txn.kv_put("key", value)?;
    Ok(())
}) {
    Ok(_) => { /* committed */ }
    Err(StrataError::TransactionAborted { reason }) => {
        // Log and possibly retry
    }
    Err(StrataError::WriteConflict { entity_ref }) => {
        // Concurrent modification - retry
    }
    Err(e) => {
        // Other error - don't retry
    }
}
```

### Validation Errors

Returned for invalid input. Don't retry - fix the input.

```rust
match result {
    Err(StrataError::DimensionMismatch { expected, got }) => {
        panic!("Bug: embedding has wrong dimension");
    }
    Err(StrataError::InvalidInput { message }) => {
        // User error - report to user
    }
    // ...
}
```

## Best Practices

1. **Use `?` operator**: Let errors propagate naturally
2. **Match specific variants**: When you need to handle specific cases
3. **Check is_retryable()**: Before implementing retry logic
4. **Log serious errors**: is_serious() indicates bugs or corruption
5. **Include context**: When wrapping errors, preserve the EntityRef
```

### Acceptance Criteria

- [ ] All error variants have doc comments
- [ ] Examples in doc comments
- [ ] ERROR_HANDLING.md created
- [ ] Guidelines for each error category
- [ ] Best practices documented

---

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // === Constructor Tests ===

    #[test]
    fn test_error_constructors() {
        let run_id = RunId::new("test-run");

        let e = StrataError::not_found(EntityRef::kv(run_id.clone(), "key"));
        assert!(e.is_not_found());

        let e = StrataError::version_conflict(
            EntityRef::state(run_id.clone(), "cell"),
            Version::Counter(1),
            Version::Counter(2),
        );
        assert!(e.is_conflict());

        let e = StrataError::invalid_input("bad data");
        assert!(e.is_validation_error());
    }

    // === Classification Tests ===

    #[test]
    fn test_is_retryable() {
        let run_id = RunId::new("test-run");

        // Retryable
        let e = StrataError::version_conflict(
            EntityRef::kv(run_id.clone(), "key"),
            Version::TxnId(1),
            Version::TxnId(2),
        );
        assert!(e.is_retryable());

        let e = StrataError::write_conflict(EntityRef::kv(run_id.clone(), "key"));
        assert!(e.is_retryable());

        // Not retryable
        let e = StrataError::not_found(EntityRef::kv(run_id.clone(), "key"));
        assert!(!e.is_retryable());

        let e = StrataError::invalid_input("bad");
        assert!(!e.is_retryable());
    }

    #[test]
    fn test_is_serious() {
        let e = StrataError::Corruption {
            message: "CRC mismatch".to_string(),
        };
        assert!(e.is_serious());

        let e = StrataError::internal("unexpected state");
        assert!(e.is_serious());

        let e = StrataError::not_found(EntityRef::kv(RunId::new("r"), "k"));
        assert!(!e.is_serious());
    }

    // === Conversion Tests ===

    #[test]
    fn test_kv_error_conversion() {
        let kv_error = KvError::NotFound {
            run_id: RunId::new("test-run"),
            key: "missing-key".to_string(),
        };

        let strata_error: StrataError = kv_error.into();

        assert!(strata_error.is_not_found());
        assert!(strata_error.entity_ref().is_some());
    }

    #[test]
    fn test_state_version_mismatch_conversion() {
        let state_error = StateError::VersionMismatch {
            run_id: RunId::new("test-run"),
            name: "counter".to_string(),
            expected: 5,
            actual: 6,
        };

        let strata_error: StrataError = state_error.into();

        assert!(strata_error.is_conflict());
        match strata_error {
            StrataError::VersionConflict { expected, actual, .. } => {
                assert_eq!(expected, Version::Counter(5));
                assert_eq!(actual, Version::Counter(6));
            }
            _ => panic!("Expected VersionConflict"),
        }
    }

    #[test]
    fn test_vector_dimension_mismatch_conversion() {
        let vector_error = VectorError::DimensionMismatch {
            expected: 384,
            got: 768,
        };

        let strata_error: StrataError = vector_error.into();

        assert!(strata_error.is_validation_error());
        match strata_error {
            StrataError::DimensionMismatch { expected, got } => {
                assert_eq!(expected, 384);
                assert_eq!(got, 768);
            }
            _ => panic!("Expected DimensionMismatch"),
        }
    }

    // === Display Tests ===

    #[test]
    fn test_error_display() {
        let run_id = RunId::new("my-run");

        let e = StrataError::not_found(EntityRef::kv(run_id.clone(), "config"));
        assert!(e.to_string().contains("my-run"));
        assert!(e.to_string().contains("config"));

        let e = StrataError::version_conflict(
            EntityRef::state(run_id.clone(), "counter"),
            Version::Counter(5),
            Version::Counter(6),
        );
        assert!(e.to_string().contains("version conflict"));
        assert!(e.to_string().contains("cnt:5"));
        assert!(e.to_string().contains("cnt:6"));
    }

    // === Entity Ref Accessor Tests ===

    #[test]
    fn test_entity_ref_accessor() {
        let run_id = RunId::new("test-run");
        let entity_ref = EntityRef::kv(run_id, "key");

        let e = StrataError::not_found(entity_ref.clone());
        assert_eq!(e.entity_ref(), Some(&entity_ref));

        let e = StrataError::storage("disk full");
        assert_eq!(e.entity_ref(), None);
    }
}
```

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/core/src/error.rs` | CREATE or MODIFY - StrataError enum |
| `crates/primitives/src/kv_store.rs` | MODIFY - Use StrataError |
| `crates/primitives/src/event_log.rs` | MODIFY - Use StrataError |
| `crates/primitives/src/state_cell.rs` | MODIFY - Use StrataError |
| `crates/primitives/src/trace_store.rs` | MODIFY - Use StrataError |
| `crates/primitives/src/json_store.rs` | MODIFY - Use StrataError |
| `crates/primitives/src/vector/store.rs` | MODIFY - Use StrataError |
| `crates/primitives/src/run_index.rs` | MODIFY - Use StrataError |
| `docs/ERROR_HANDLING.md` | CREATE - Error handling guidelines |
