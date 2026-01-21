//! Error types for Strata database
//!
//! This module defines all error types used throughout the system.
//! We use `thiserror` for automatic `Display` and `Error` trait implementations.
//!
//! ## M9 Error Model
//!
//! The `StrataError` type is the unified error type for all Strata APIs.
//! It provides consistent error handling across all primitives.
//!
//! ### Error Categories
//!
//! - **Not Found**: Entity doesn't exist
//! - **Conflict**: Version mismatch or concurrent modification
//! - **Transaction**: Transaction-level failures
//! - **Validation**: Invalid input or operation
//! - **Storage**: Low-level storage failures
//!
//! ### Usage
//!
//! ```ignore
//! match result {
//!     Err(StrataError::NotFound { entity_ref }) => {
//!         println!("Entity not found: {}", entity_ref);
//!     }
//!     Err(StrataError::VersionConflict { expected, actual, .. }) => {
//!         println!("Conflict: expected {:?}, got {:?}", expected, actual);
//!     }
//!     Err(e) if e.is_retryable() => {
//!         // Retry the operation
//!     }
//!     Err(e) => {
//!         println!("Other error: {}", e);
//!     }
//!     Ok(value) => { /* success */ }
//! }
//! ```

use crate::contract::{EntityRef, Version};
use crate::types::{Key, RunId};
use std::io;
use thiserror::Error;

/// Result type alias for Strata operations
pub type Result<T> = std::result::Result<T, Error>;

/// Error types for the Strata database
#[derive(Debug, Error)]
pub enum Error {
    /// I/O error (file operations, network, etc.)
    #[error("I/O error: {0}")]
    IoError(#[from] io::Error),

    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    SerializationError(String),

    /// Key not found in storage
    #[error("Key not found: {0:?}")]
    KeyNotFound(Key),

    /// Version mismatch (for CAS operations)
    #[error("Version mismatch: expected {expected}, got {actual}")]
    VersionMismatch {
        /// Expected version
        expected: u64,
        /// Actual version found
        actual: u64,
    },

    /// Data corruption detected
    #[error("Data corruption: {0}")]
    Corruption(String),

    /// Invalid operation or state
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    /// Transaction aborted due to conflict (M2)
    #[error("Transaction aborted for run {0:?}")]
    TransactionAborted(RunId),

    /// Storage layer error
    #[error("Storage error: {0}")]
    StorageError(String),

    /// Invalid state transition (M2 transactions)
    #[error("Invalid state: {0}")]
    InvalidState(String),

    /// Transaction conflict detected during commit (M2)
    #[error("Transaction conflict: {0}")]
    TransactionConflict(String),

    /// Transaction exceeded timeout (M2)
    #[error("Transaction timeout: {0}")]
    TransactionTimeout(String),
}

impl From<bincode::Error> for Error {
    fn from(e: bincode::Error) -> Self {
        Error::SerializationError(e.to_string())
    }
}

impl Error {
    /// Check if this error is a transaction conflict
    ///
    /// Used for retry logic - only conflict errors should be retried.
    pub fn is_conflict(&self) -> bool {
        matches!(self, Error::TransactionConflict(_))
    }

    /// Check if this error is a transaction timeout
    ///
    /// Used to identify when a transaction was aborted due to exceeding
    /// its time limit.
    pub fn is_timeout(&self) -> bool {
        matches!(self, Error::TransactionTimeout(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Namespace, RunId, TypeTag};

    #[test]
    fn test_error_display_io() {
        let err = Error::IoError(io::Error::new(io::ErrorKind::NotFound, "file not found"));
        let msg = err.to_string();
        assert!(msg.contains("I/O error"));
    }

    #[test]
    fn test_error_display_serialization() {
        let err = Error::SerializationError("invalid format".to_string());
        let msg = err.to_string();
        assert!(msg.contains("Serialization error"));
        assert!(msg.contains("invalid format"));
    }

    #[test]
    fn test_error_display_key_not_found() {
        let run_id = RunId::new();
        let namespace = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );
        let key = Key::new(namespace, TypeTag::KV, b"test-key".to_vec());
        let err = Error::KeyNotFound(key);
        let msg = err.to_string();
        assert!(msg.contains("Key not found"));
    }

    #[test]
    fn test_error_display_version_mismatch() {
        let err = Error::VersionMismatch {
            expected: 42,
            actual: 43,
        };
        let msg = err.to_string();
        assert!(msg.contains("Version mismatch"));
        assert!(msg.contains("42"));
        assert!(msg.contains("43"));
    }

    #[test]
    fn test_error_display_corruption() {
        let err = Error::Corruption("CRC check failed".to_string());
        let msg = err.to_string();
        assert!(msg.contains("Data corruption"));
        assert!(msg.contains("CRC check failed"));
    }

    #[test]
    fn test_error_display_invalid_operation() {
        let err = Error::InvalidOperation("cannot delete while locked".to_string());
        let msg = err.to_string();
        assert!(msg.contains("Invalid operation"));
        assert!(msg.contains("cannot delete while locked"));
    }

    #[test]
    fn test_error_display_transaction_aborted() {
        let run_id = RunId::new();
        let err = Error::TransactionAborted(run_id);
        let msg = err.to_string();
        assert!(msg.contains("Transaction aborted"));
    }

    #[test]
    fn test_error_display_storage() {
        let err = Error::StorageError("write failed".to_string());
        let msg = err.to_string();
        assert!(msg.contains("Storage error"));
        assert!(msg.contains("write failed"));
    }

    #[test]
    fn test_error_from_io() {
        let io_err = io::Error::new(io::ErrorKind::PermissionDenied, "access denied");
        let err: Error = io_err.into();
        assert!(matches!(err, Error::IoError(_)));
    }

    #[test]
    fn test_error_from_bincode() {
        // Create a serialization error by using invalid bincode data
        let invalid_data = vec![0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];

        // Try to deserialize invalid data (will fail)
        let result: Result<String> = bincode::deserialize(&invalid_data).map_err(|e| e.into());

        assert!(matches!(result, Err(Error::SerializationError(_))));
    }

    #[test]
    fn test_result_type_alias() {
        fn returns_result() -> Result<i32> {
            Ok(42)
        }

        fn returns_error() -> Result<i32> {
            Err(Error::InvalidOperation("test".to_string()))
        }

        assert_eq!(returns_result().unwrap(), 42);
        assert!(returns_error().is_err());
    }

    #[test]
    fn test_error_pattern_matching() {
        let err = Error::VersionMismatch {
            expected: 10,
            actual: 11,
        };

        match err {
            Error::VersionMismatch { expected, actual } => {
                assert_eq!(expected, 10);
                assert_eq!(actual, 11);
            }
            _ => panic!("Wrong error variant"),
        }
    }

    #[test]
    fn test_error_display_invalid_state() {
        let err = Error::InvalidState("transaction not active".to_string());
        let msg = err.to_string();
        assert!(msg.contains("Invalid state"));
        assert!(msg.contains("transaction not active"));
    }

    #[test]
    fn test_error_display_transaction_conflict() {
        let err = Error::TransactionConflict("read-write conflict on key".to_string());
        let msg = err.to_string();
        assert!(msg.contains("Transaction conflict"));
        assert!(msg.contains("read-write conflict on key"));
    }

    #[test]
    fn test_is_conflict() {
        let conflict = Error::TransactionConflict("conflict".to_string());
        let not_conflict = Error::InvalidState("state".to_string());

        assert!(conflict.is_conflict());
        assert!(!not_conflict.is_conflict());
    }

    #[test]
    fn test_error_display_transaction_timeout() {
        let err = Error::TransactionTimeout("exceeded 5s limit".to_string());
        let msg = err.to_string();
        assert!(msg.contains("Transaction timeout"));
        assert!(msg.contains("exceeded 5s limit"));
    }

    #[test]
    fn test_is_timeout() {
        let timeout = Error::TransactionTimeout("timed out".to_string());
        let not_timeout = Error::InvalidState("state".to_string());
        let conflict = Error::TransactionConflict("conflict".to_string());

        assert!(timeout.is_timeout());
        assert!(!not_timeout.is_timeout());
        assert!(!conflict.is_timeout());
    }
}

// =============================================================================
// StrataError - Unified Error Type (M9)
// =============================================================================

/// Unified error type for all Strata operations (M9)
///
/// This is the canonical error type returned by all Strata APIs.
/// It provides consistent error handling across all primitives.
///
/// ## Error Categories
///
/// - **Not Found**: Entity doesn't exist (`NotFound`, `RunNotFound`, `PathNotFound`)
/// - **Conflict**: Version mismatch or concurrent modification (`VersionConflict`, `WriteConflict`)
/// - **Transaction**: Transaction-level failures (`TransactionAborted`, `TransactionTimeout`, `TransactionNotActive`)
/// - **Validation**: Invalid input or operation (`InvalidOperation`, `InvalidInput`, `DimensionMismatch`)
/// - **Storage**: Low-level storage failures (`Storage`, `Serialization`, `Corruption`)
/// - **Resource**: Resource limits exceeded (`CapacityExceeded`, `BudgetExceeded`)
/// - **Internal**: Unexpected internal errors (`Internal`)
///
/// ## Usage
///
/// ```ignore
/// use strata_core::{StrataError, StrataResult, EntityRef, Version};
///
/// fn example_operation() -> StrataResult<String> {
///     // Use ? operator naturally
///     let value = some_db_operation()?;
///     Ok(value)
/// }
///
/// // Match on specific variants
/// match result {
///     Err(StrataError::NotFound { entity_ref }) => {
///         println!("Entity not found: {}", entity_ref);
///     }
///     Err(e) if e.is_retryable() => {
///         // Retry the operation
///     }
///     Err(e) if e.is_serious() => {
///         // Log and alert
///     }
///     Err(e) => { /* handle other errors */ }
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
    /// event, state cell, or any other entity type.
    ///
    /// ## Example
    /// ```ignore
    /// // KV key not found
    /// StrataError::not_found(EntityRef::kv(run_id, "missing-key"))
    /// ```
    #[error("not found: {entity_ref}")]
    NotFound {
        /// Reference to the entity that was not found
        entity_ref: EntityRef,
    },

    /// Run not found
    ///
    /// The specified run does not exist. This is separate from `NotFound`
    /// because runs are meta-level entities that scope all other data.
    ///
    /// ## Example
    /// ```ignore
    /// StrataError::run_not_found(run_id)
    /// ```
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
    ///
    /// This error is **retryable** - the operation can be retried after
    /// re-reading the current version.
    ///
    /// ## Example
    /// ```ignore
    /// StrataError::version_conflict(
    ///     EntityRef::state(run_id, "counter"),
    ///     Version::Counter(5),  // expected
    ///     Version::Counter(6),  // actual
    /// )
    /// ```
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
    /// This error is **retryable** - the transaction can be retried.
    ///
    /// ## Example
    /// ```ignore
    /// StrataError::write_conflict(EntityRef::kv(run_id, "shared-key"))
    /// ```
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
    /// transactional failure. This error is **retryable**.
    ///
    /// ## Example
    /// ```ignore
    /// StrataError::TransactionAborted {
    ///     reason: "Conflict on key 'counter'".to_string(),
    /// }
    /// ```
    #[error("transaction aborted: {reason}")]
    TransactionAborted {
        /// Reason for the abort
        reason: String,
    },

    /// Transaction timeout
    ///
    /// The transaction exceeded the maximum allowed duration.
    ///
    /// ## Example
    /// ```ignore
    /// StrataError::TransactionTimeout { duration_ms: 5000 }
    /// ```
    #[error("transaction timeout after {duration_ms}ms")]
    TransactionTimeout {
        /// How long the transaction ran before timing out
        duration_ms: u64,
    },

    /// Transaction not active
    ///
    /// An operation was attempted on a transaction that has already
    /// been committed or rolled back.
    ///
    /// ## Example
    /// ```ignore
    /// StrataError::TransactionNotActive {
    ///     state: "committed".to_string(),
    /// }
    /// ```
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
    /// Examples: creating a document that exists, deleting a required entity,
    /// invalid state transition.
    ///
    /// ## Example
    /// ```ignore
    /// StrataError::invalid_operation(
    ///     EntityRef::json(run_id, doc_id),
    ///     "Document already exists",
    /// )
    /// ```
    #[error("invalid operation on {entity_ref}: {reason}")]
    InvalidOperation {
        /// Reference to the entity
        entity_ref: EntityRef,
        /// Why the operation is invalid
        reason: String,
    },

    /// Invalid input
    ///
    /// The input parameters are invalid. This is a validation error that
    /// cannot be fixed by retrying - the input must be corrected.
    ///
    /// ## Example
    /// ```ignore
    /// StrataError::invalid_input("Key cannot be empty")
    /// ```
    #[error("invalid input: {message}")]
    InvalidInput {
        /// Description of what's wrong with the input
        message: String,
    },

    /// Dimension mismatch (Vector-specific)
    ///
    /// The vector dimension doesn't match the collection's configured dimension.
    ///
    /// ## Example
    /// ```ignore
    /// StrataError::dimension_mismatch(384, 768)
    /// ```
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
    ///
    /// ## Example
    /// ```ignore
    /// StrataError::PathNotFound {
    ///     entity_ref: EntityRef::json(run_id, doc_id),
    ///     path: "/data/items/0/name".to_string(),
    /// }
    /// ```
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
    ///
    /// ## Example
    /// ```ignore
    /// StrataError::storage("Disk write failed")
    /// ```
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
    ///
    /// ## Example
    /// ```ignore
    /// StrataError::serialization("Invalid UTF-8 in key")
    /// ```
    #[error("serialization error: {message}")]
    Serialization {
        /// What went wrong
        message: String,
    },

    /// Corruption detected
    ///
    /// Data integrity check failed. This is a **serious** error that may
    /// require recovery from backup.
    ///
    /// ## Example
    /// ```ignore
    /// StrataError::Corruption {
    ///     message: "CRC mismatch in event log".to_string(),
    /// }
    /// ```
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
    ///
    /// ## Example
    /// ```ignore
    /// StrataError::CapacityExceeded {
    ///     resource: "event log".to_string(),
    ///     limit: 1_000_000,
    ///     requested: 1_000_001,
    /// }
    /// ```
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
    ///
    /// ## Example
    /// ```ignore
    /// StrataError::BudgetExceeded {
    ///     operation: "vector search".to_string(),
    /// }
    /// ```
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
    /// An unexpected internal error occurred. This is a **serious** error
    /// that indicates a bug in the system.
    ///
    /// ## Example
    /// ```ignore
    /// StrataError::internal("Unexpected state in transaction manager")
    /// ```
    #[error("internal error: {message}")]
    Internal {
        /// Error message
        message: String,
    },
}

impl StrataError {
    // =========================================================================
    // Constructors
    // =========================================================================

    /// Create a NotFound error
    ///
    /// ## Example
    /// ```ignore
    /// StrataError::not_found(EntityRef::kv(run_id, "missing-key"))
    /// ```
    pub fn not_found(entity_ref: EntityRef) -> Self {
        StrataError::NotFound { entity_ref }
    }

    /// Create a RunNotFound error
    ///
    /// ## Example
    /// ```ignore
    /// StrataError::run_not_found(run_id)
    /// ```
    pub fn run_not_found(run_id: RunId) -> Self {
        StrataError::RunNotFound { run_id }
    }

    /// Create a VersionConflict error
    ///
    /// ## Example
    /// ```ignore
    /// StrataError::version_conflict(
    ///     EntityRef::state(run_id, "counter"),
    ///     Version::Counter(5),
    ///     Version::Counter(6),
    /// )
    /// ```
    pub fn version_conflict(entity_ref: EntityRef, expected: Version, actual: Version) -> Self {
        StrataError::VersionConflict {
            entity_ref,
            expected,
            actual,
        }
    }

    /// Create a WriteConflict error
    ///
    /// ## Example
    /// ```ignore
    /// StrataError::write_conflict(EntityRef::kv(run_id, "shared-key"))
    /// ```
    pub fn write_conflict(entity_ref: EntityRef) -> Self {
        StrataError::WriteConflict { entity_ref }
    }

    /// Create a TransactionAborted error
    ///
    /// ## Example
    /// ```ignore
    /// StrataError::transaction_aborted("Conflict on key 'counter'")
    /// ```
    pub fn transaction_aborted(reason: impl Into<String>) -> Self {
        StrataError::TransactionAborted {
            reason: reason.into(),
        }
    }

    /// Create a TransactionTimeout error
    ///
    /// ## Example
    /// ```ignore
    /// StrataError::transaction_timeout(5000)
    /// ```
    pub fn transaction_timeout(duration_ms: u64) -> Self {
        StrataError::TransactionTimeout { duration_ms }
    }

    /// Create a TransactionNotActive error
    ///
    /// ## Example
    /// ```ignore
    /// StrataError::transaction_not_active("committed")
    /// ```
    pub fn transaction_not_active(state: impl Into<String>) -> Self {
        StrataError::TransactionNotActive {
            state: state.into(),
        }
    }

    /// Create an InvalidOperation error
    ///
    /// ## Example
    /// ```ignore
    /// StrataError::invalid_operation(
    ///     EntityRef::json(run_id, doc_id),
    ///     "Document already exists",
    /// )
    /// ```
    pub fn invalid_operation(entity_ref: EntityRef, reason: impl Into<String>) -> Self {
        StrataError::InvalidOperation {
            entity_ref,
            reason: reason.into(),
        }
    }

    /// Create an InvalidInput error
    ///
    /// ## Example
    /// ```ignore
    /// StrataError::invalid_input("Key cannot be empty")
    /// ```
    pub fn invalid_input(message: impl Into<String>) -> Self {
        StrataError::InvalidInput {
            message: message.into(),
        }
    }

    /// Create a DimensionMismatch error
    ///
    /// ## Example
    /// ```ignore
    /// StrataError::dimension_mismatch(384, 768)
    /// ```
    pub fn dimension_mismatch(expected: usize, got: usize) -> Self {
        StrataError::DimensionMismatch { expected, got }
    }

    /// Create a PathNotFound error
    ///
    /// ## Example
    /// ```ignore
    /// StrataError::path_not_found(
    ///     EntityRef::json(run_id, doc_id),
    ///     "/data/items/0",
    /// )
    /// ```
    pub fn path_not_found(entity_ref: EntityRef, path: impl Into<String>) -> Self {
        StrataError::PathNotFound {
            entity_ref,
            path: path.into(),
        }
    }

    /// Create a Storage error
    ///
    /// ## Example
    /// ```ignore
    /// StrataError::storage("Disk write failed")
    /// ```
    pub fn storage(message: impl Into<String>) -> Self {
        StrataError::Storage {
            message: message.into(),
            source: None,
        }
    }

    /// Create a Storage error with source
    ///
    /// ## Example
    /// ```ignore
    /// StrataError::storage_with_source("Failed to write", io_error)
    /// ```
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
    ///
    /// ## Example
    /// ```ignore
    /// StrataError::serialization("Invalid UTF-8 in key")
    /// ```
    pub fn serialization(message: impl Into<String>) -> Self {
        StrataError::Serialization {
            message: message.into(),
        }
    }

    /// Create a Corruption error
    ///
    /// ## Example
    /// ```ignore
    /// StrataError::corruption("CRC mismatch")
    /// ```
    pub fn corruption(message: impl Into<String>) -> Self {
        StrataError::Corruption {
            message: message.into(),
        }
    }

    /// Create a CapacityExceeded error
    ///
    /// ## Example
    /// ```ignore
    /// StrataError::capacity_exceeded("event log", 1_000_000, 1_000_001)
    /// ```
    pub fn capacity_exceeded(resource: impl Into<String>, limit: usize, requested: usize) -> Self {
        StrataError::CapacityExceeded {
            resource: resource.into(),
            limit,
            requested,
        }
    }

    /// Create a BudgetExceeded error
    ///
    /// ## Example
    /// ```ignore
    /// StrataError::budget_exceeded("vector search")
    /// ```
    pub fn budget_exceeded(operation: impl Into<String>) -> Self {
        StrataError::BudgetExceeded {
            operation: operation.into(),
        }
    }

    /// Create an Internal error
    ///
    /// ## Example
    /// ```ignore
    /// StrataError::internal("Unexpected state")
    /// ```
    pub fn internal(message: impl Into<String>) -> Self {
        StrataError::Internal {
            message: message.into(),
        }
    }

    // =========================================================================
    // Classification Methods
    // =========================================================================

    /// Check if this is a "not found" type error
    ///
    /// Returns true for: `NotFound`, `RunNotFound`, `PathNotFound`
    ///
    /// ## Example
    /// ```ignore
    /// if error.is_not_found() {
    ///     // Handle missing entity
    /// }
    /// ```
    pub fn is_not_found(&self) -> bool {
        matches!(
            self,
            StrataError::NotFound { .. }
                | StrataError::RunNotFound { .. }
                | StrataError::PathNotFound { .. }
        )
    }

    /// Check if this is a conflict error
    ///
    /// Returns true for: `VersionConflict`, `WriteConflict`
    ///
    /// ## Example
    /// ```ignore
    /// if error.is_conflict() {
    ///     // Retry with fresh data
    /// }
    /// ```
    pub fn is_conflict(&self) -> bool {
        matches!(
            self,
            StrataError::VersionConflict { .. } | StrataError::WriteConflict { .. }
        )
    }

    /// Check if this is a transaction error
    ///
    /// Returns true for: `TransactionAborted`, `TransactionTimeout`, `TransactionNotActive`
    ///
    /// ## Example
    /// ```ignore
    /// if error.is_transaction_error() {
    ///     // Handle transaction failure
    /// }
    /// ```
    pub fn is_transaction_error(&self) -> bool {
        matches!(
            self,
            StrataError::TransactionAborted { .. }
                | StrataError::TransactionTimeout { .. }
                | StrataError::TransactionNotActive { .. }
        )
    }

    /// Check if this is a validation error
    ///
    /// Returns true for: `InvalidOperation`, `InvalidInput`, `DimensionMismatch`
    ///
    /// Validation errors indicate bad input - don't retry, fix the input.
    ///
    /// ## Example
    /// ```ignore
    /// if error.is_validation_error() {
    ///     // Report to user, don't retry
    /// }
    /// ```
    pub fn is_validation_error(&self) -> bool {
        matches!(
            self,
            StrataError::InvalidOperation { .. }
                | StrataError::InvalidInput { .. }
                | StrataError::DimensionMismatch { .. }
        )
    }

    /// Check if this is a storage error
    ///
    /// Returns true for: `Storage`, `Serialization`, `Corruption`
    ///
    /// ## Example
    /// ```ignore
    /// if error.is_storage_error() {
    ///     // Check disk/IO
    /// }
    /// ```
    pub fn is_storage_error(&self) -> bool {
        matches!(
            self,
            StrataError::Storage { .. }
                | StrataError::Serialization { .. }
                | StrataError::Corruption { .. }
        )
    }

    /// Check if this error is retryable
    ///
    /// Retryable errors may succeed on retry:
    /// - `VersionConflict`: Re-read current version and retry
    /// - `WriteConflict`: Retry the transaction
    /// - `TransactionAborted`: Retry the transaction
    ///
    /// ## Example
    /// ```ignore
    /// loop {
    ///     match operation() {
    ///         Ok(result) => return Ok(result),
    ///         Err(e) if e.is_retryable() => continue,
    ///         Err(e) => return Err(e),
    ///     }
    /// }
    /// ```
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            StrataError::VersionConflict { .. }
                | StrataError::WriteConflict { .. }
                | StrataError::TransactionAborted { .. }
        )
    }

    /// Check if this is a serious/unrecoverable error
    ///
    /// Serious errors indicate potential data corruption or bugs:
    /// - `Corruption`: Data integrity failure
    /// - `Internal`: Unexpected system state (bug)
    ///
    /// These should be logged, alerted, and investigated.
    ///
    /// ## Example
    /// ```ignore
    /// if error.is_serious() {
    ///     log::error!("SERIOUS ERROR: {}", error);
    ///     alert_oncall();
    /// }
    /// ```
    pub fn is_serious(&self) -> bool {
        matches!(
            self,
            StrataError::Corruption { .. } | StrataError::Internal { .. }
        )
    }

    /// Check if this is a resource error
    ///
    /// Returns true for: `CapacityExceeded`, `BudgetExceeded`
    ///
    /// ## Example
    /// ```ignore
    /// if error.is_resource_error() {
    ///     // Reduce batch size or implement backpressure
    /// }
    /// ```
    pub fn is_resource_error(&self) -> bool {
        matches!(
            self,
            StrataError::CapacityExceeded { .. } | StrataError::BudgetExceeded { .. }
        )
    }

    /// Get the entity reference if this error is about a specific entity
    ///
    /// Returns `Some(&EntityRef)` for errors that reference an entity:
    /// - `NotFound`
    /// - `VersionConflict`
    /// - `WriteConflict`
    /// - `InvalidOperation`
    /// - `PathNotFound`
    ///
    /// Returns `None` for errors without entity context.
    ///
    /// ## Example
    /// ```ignore
    /// if let Some(entity) = error.entity_ref() {
    ///     println!("Error on entity: {}", entity);
    /// }
    /// ```
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

    /// Get the run ID if this error is about a specific run
    ///
    /// Returns the RunId from:
    /// - `RunNotFound`: The missing run
    /// - Entity-related errors: The run from the EntityRef
    ///
    /// ## Example
    /// ```ignore
    /// if let Some(run_id) = error.run_id() {
    ///     println!("Error in run: {}", run_id);
    /// }
    /// ```
    pub fn run_id(&self) -> Option<RunId> {
        match self {
            StrataError::RunNotFound { run_id } => Some(*run_id),
            _ => self.entity_ref().map(|e| e.run_id()),
        }
    }
}

// =============================================================================
// StrataResult Type Alias
// =============================================================================

/// Result type alias for Strata operations
///
/// All Strata API methods return `StrataResult<T>`.
///
/// ## Example
/// ```ignore
/// fn get_value(run_id: RunId, key: &str) -> StrataResult<String> {
///     let value = db.kv().get(&run_id, key)?;
///     match value {
///         Some(v) => Ok(v),
///         None => Err(StrataError::not_found(EntityRef::kv(run_id, key))),
///     }
/// }
/// ```
pub type StrataResult<T> = std::result::Result<T, StrataError>;

// =============================================================================
// Conversions from Legacy Error
// =============================================================================

impl From<Error> for StrataError {
    fn from(e: Error) -> Self {
        match e {
            Error::IoError(io_err) => StrataError::Storage {
                message: io_err.to_string(),
                source: Some(Box::new(io_err)),
            },
            Error::SerializationError(msg) => StrataError::Serialization { message: msg },
            Error::KeyNotFound(key) => StrataError::NotFound {
                entity_ref: EntityRef::kv(key.namespace.run_id, format!("{:?}", key)),
            },
            Error::VersionMismatch { expected, actual } => StrataError::VersionConflict {
                entity_ref: EntityRef::kv(RunId::new(), "unknown"),
                expected: Version::TxnId(expected),
                actual: Version::TxnId(actual),
            },
            Error::Corruption(msg) => StrataError::Corruption { message: msg },
            Error::InvalidOperation(msg) => StrataError::InvalidInput { message: msg },
            Error::TransactionAborted(run_id) => StrataError::TransactionAborted {
                reason: format!("Transaction aborted for run {}", run_id),
            },
            Error::StorageError(msg) => StrataError::Storage {
                message: msg,
                source: None,
            },
            Error::InvalidState(msg) => StrataError::InvalidInput { message: msg },
            Error::TransactionConflict(msg) => StrataError::WriteConflict {
                entity_ref: EntityRef::kv(RunId::new(), msg),
            },
            Error::TransactionTimeout(_msg) => {
                // Legacy error doesn't have duration, default to 0
                StrataError::TransactionTimeout { duration_ms: 0 }
            }
        }
    }
}

// =============================================================================
// Conversions from Standard Library Types
// =============================================================================

impl From<io::Error> for StrataError {
    fn from(e: io::Error) -> Self {
        StrataError::Storage {
            message: format!("IO error: {}", e),
            source: Some(Box::new(e)),
        }
    }
}

impl From<bincode::Error> for StrataError {
    fn from(e: bincode::Error) -> Self {
        StrataError::Serialization {
            message: e.to_string(),
        }
    }
}

impl From<serde_json::Error> for StrataError {
    fn from(e: serde_json::Error) -> Self {
        StrataError::Serialization {
            message: format!("JSON error: {}", e),
        }
    }
}

// =============================================================================
// StrataError Tests
// =============================================================================

#[cfg(test)]
mod strata_error_tests {
    use super::*;

    // === Constructor Tests ===

    #[test]
    fn test_not_found_constructor() {
        let run_id = RunId::new();
        let e = StrataError::not_found(EntityRef::kv(run_id, "key"));

        assert!(e.is_not_found());
        assert!(!e.is_conflict());
        assert!(!e.is_retryable());
        assert!(!e.is_serious());
        assert!(e.entity_ref().is_some());
    }

    #[test]
    fn test_run_not_found_constructor() {
        let run_id = RunId::new();
        let e = StrataError::run_not_found(run_id);

        assert!(e.is_not_found());
        assert!(!e.is_conflict());
        assert_eq!(e.run_id(), Some(run_id));
        assert!(e.entity_ref().is_none());
    }

    #[test]
    fn test_version_conflict_constructor() {
        let run_id = RunId::new();
        let e = StrataError::version_conflict(
            EntityRef::state(run_id, "counter"),
            Version::Counter(5),
            Version::Counter(6),
        );

        assert!(e.is_conflict());
        assert!(e.is_retryable());
        assert!(!e.is_not_found());
        assert!(!e.is_serious());
        assert!(e.entity_ref().is_some());
    }

    #[test]
    fn test_write_conflict_constructor() {
        let run_id = RunId::new();
        let e = StrataError::write_conflict(EntityRef::kv(run_id, "shared-key"));

        assert!(e.is_conflict());
        assert!(e.is_retryable());
    }

    #[test]
    fn test_transaction_aborted_constructor() {
        let e = StrataError::transaction_aborted("Conflict on key");

        assert!(e.is_transaction_error());
        assert!(e.is_retryable());
        assert!(!e.is_conflict());
    }

    #[test]
    fn test_transaction_timeout_constructor() {
        let e = StrataError::transaction_timeout(5000);

        assert!(e.is_transaction_error());
        assert!(!e.is_retryable());
        match e {
            StrataError::TransactionTimeout { duration_ms } => assert_eq!(duration_ms, 5000),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_transaction_not_active_constructor() {
        let e = StrataError::transaction_not_active("committed");

        assert!(e.is_transaction_error());
        assert!(!e.is_retryable());
    }

    #[test]
    fn test_invalid_operation_constructor() {
        let run_id = RunId::new();
        let doc_id = crate::types::JsonDocId::new();
        let e = StrataError::invalid_operation(
            EntityRef::json(run_id, doc_id),
            "Document already exists",
        );

        assert!(e.is_validation_error());
        assert!(!e.is_retryable());
        assert!(e.entity_ref().is_some());
    }

    #[test]
    fn test_invalid_input_constructor() {
        let e = StrataError::invalid_input("Key cannot be empty");

        assert!(e.is_validation_error());
        assert!(!e.is_retryable());
        assert!(e.entity_ref().is_none());
    }

    #[test]
    fn test_dimension_mismatch_constructor() {
        let e = StrataError::dimension_mismatch(384, 768);

        assert!(e.is_validation_error());
        match e {
            StrataError::DimensionMismatch { expected, got } => {
                assert_eq!(expected, 384);
                assert_eq!(got, 768);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_path_not_found_constructor() {
        let run_id = RunId::new();
        let doc_id = crate::types::JsonDocId::new();
        let e = StrataError::path_not_found(EntityRef::json(run_id, doc_id), "/data/items/0");

        assert!(e.is_not_found());
        assert!(e.entity_ref().is_some());
    }

    #[test]
    fn test_storage_constructor() {
        let e = StrataError::storage("Disk write failed");

        assert!(e.is_storage_error());
        assert!(!e.is_serious());
    }

    #[test]
    fn test_storage_with_source_constructor() {
        let io_err = io::Error::new(io::ErrorKind::Other, "disk full");
        let e = StrataError::storage_with_source("Write failed", io_err);

        assert!(e.is_storage_error());
        match e {
            StrataError::Storage { source, .. } => assert!(source.is_some()),
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_serialization_constructor() {
        let e = StrataError::serialization("Invalid UTF-8");

        assert!(e.is_storage_error());
    }

    #[test]
    fn test_corruption_constructor() {
        let e = StrataError::corruption("CRC mismatch");

        assert!(e.is_storage_error());
        assert!(e.is_serious());
    }

    #[test]
    fn test_capacity_exceeded_constructor() {
        let e = StrataError::capacity_exceeded("event log", 1_000_000, 1_000_001);

        assert!(e.is_resource_error());
        match e {
            StrataError::CapacityExceeded {
                resource,
                limit,
                requested,
            } => {
                assert_eq!(resource, "event log");
                assert_eq!(limit, 1_000_000);
                assert_eq!(requested, 1_000_001);
            }
            _ => panic!("Wrong variant"),
        }
    }

    #[test]
    fn test_budget_exceeded_constructor() {
        let e = StrataError::budget_exceeded("vector search");

        assert!(e.is_resource_error());
    }

    #[test]
    fn test_internal_constructor() {
        let e = StrataError::internal("Unexpected state");

        assert!(e.is_serious());
        assert!(!e.is_retryable());
    }

    // === Classification Tests ===

    #[test]
    fn test_is_retryable() {
        let run_id = RunId::new();

        // Retryable
        assert!(StrataError::version_conflict(
            EntityRef::kv(run_id, "k"),
            Version::TxnId(1),
            Version::TxnId(2),
        )
        .is_retryable());
        assert!(StrataError::write_conflict(EntityRef::kv(run_id, "k")).is_retryable());
        assert!(StrataError::transaction_aborted("conflict").is_retryable());

        // Not retryable
        assert!(!StrataError::not_found(EntityRef::kv(run_id, "k")).is_retryable());
        assert!(!StrataError::run_not_found(run_id).is_retryable());
        assert!(!StrataError::invalid_input("bad").is_retryable());
        assert!(!StrataError::transaction_timeout(1000).is_retryable());
        assert!(!StrataError::internal("bug").is_retryable());
        assert!(!StrataError::corruption("bad").is_retryable());
    }

    #[test]
    fn test_is_serious() {
        assert!(StrataError::corruption("CRC mismatch").is_serious());
        assert!(StrataError::internal("unexpected state").is_serious());

        let run_id = RunId::new();
        assert!(!StrataError::not_found(EntityRef::kv(run_id, "k")).is_serious());
        assert!(!StrataError::storage("disk full").is_serious());
    }

    // === Display Tests ===

    #[test]
    fn test_error_display_not_found() {
        let run_id = RunId::new();
        let e = StrataError::not_found(EntityRef::kv(run_id, "config"));
        let msg = e.to_string();

        assert!(msg.contains("not found"));
        assert!(msg.contains("config"));
    }

    #[test]
    fn test_error_display_version_conflict() {
        let run_id = RunId::new();
        let e = StrataError::version_conflict(
            EntityRef::state(run_id, "counter"),
            Version::Counter(5),
            Version::Counter(6),
        );
        let msg = e.to_string();

        assert!(msg.contains("version conflict"));
        assert!(msg.contains("cnt:5"));
        assert!(msg.contains("cnt:6"));
    }

    #[test]
    fn test_error_display_transaction_timeout() {
        let e = StrataError::transaction_timeout(5000);
        let msg = e.to_string();

        assert!(msg.contains("timeout"));
        assert!(msg.contains("5000"));
    }

    // === Entity Ref Accessor Tests ===

    #[test]
    fn test_entity_ref_accessor() {
        let run_id = RunId::new();
        let entity_ref = EntityRef::kv(run_id, "key");

        let e = StrataError::not_found(entity_ref.clone());
        assert_eq!(e.entity_ref(), Some(&entity_ref));

        let e = StrataError::storage("disk full");
        assert_eq!(e.entity_ref(), None);

        let e = StrataError::run_not_found(run_id);
        assert_eq!(e.entity_ref(), None);
    }

    #[test]
    fn test_run_id_accessor() {
        let run_id = RunId::new();

        // From RunNotFound
        let e = StrataError::run_not_found(run_id);
        assert_eq!(e.run_id(), Some(run_id));

        // From entity ref
        let e = StrataError::not_found(EntityRef::kv(run_id, "key"));
        assert_eq!(e.run_id(), Some(run_id));

        // No run_id
        let e = StrataError::storage("error");
        assert_eq!(e.run_id(), None);
    }

    // === Conversion Tests ===

    #[test]
    fn test_from_io_error() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "file not found");
        let e: StrataError = io_err.into();

        assert!(e.is_storage_error());
        assert!(e.to_string().contains("IO error"));
    }

    #[test]
    fn test_from_legacy_error() {
        let legacy = Error::SerializationError("bad format".to_string());
        let e: StrataError = legacy.into();

        match e {
            StrataError::Serialization { message } => {
                assert!(message.contains("bad format"));
            }
            _ => panic!("Wrong variant"),
        }
    }
}
