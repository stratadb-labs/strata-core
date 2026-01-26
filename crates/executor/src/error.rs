//! Error types for command execution.
//!
//! All errors from command execution are represented by the [`Error`] enum.
//! These errors are:
//! - **Structured**: Each variant has typed fields for error details
//! - **Serializable**: Can be converted to/from JSON
//! - **Lossless**: No error information is lost in conversion from internal errors

use serde::{Deserialize, Serialize};

/// Command execution errors.
///
/// All errors that can occur during command execution are represented here.
/// Errors are structured to preserve details for client-side handling.
///
/// # Categories
///
/// | Category | Variants | Description |
/// |----------|----------|-------------|
/// | Not Found | `KeyNotFound`, `RunNotFound`, etc. | Entity doesn't exist |
/// | Type | `WrongType` | Type mismatch |
/// | Validation | `InvalidKey`, `InvalidPath`, `InvalidInput` | Bad input |
/// | Concurrency | `VersionConflict`, `TransitionFailed`, `Conflict` | Race conditions |
/// | State | `RunClosed`, `RunExists`, `CollectionExists` | Invalid state transition |
/// | Constraint | `DimensionMismatch`, `ConstraintViolation`, etc. | Limits exceeded |
/// | Transaction | `TransactionNotActive`, `TransactionAlreadyActive` | Transaction state |
/// | System | `Io`, `Serialization`, `Internal` | Infrastructure errors |
///
/// # Example
///
/// ```ignore
/// use strata_executor::{Command, Error, Executor};
///
/// match executor.execute(cmd) {
///     Ok(output) => { /* handle success */ }
///     Err(Error::KeyNotFound { key }) => {
///         println!("Key '{}' not found", key);
///     }
///     Err(e) => {
///         println!("Error: {}", e);
///     }
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, thiserror::Error)]
pub enum Error {
    // ==================== Not Found ====================
    /// Key not found in KV store
    #[error("key not found: {key}")]
    KeyNotFound { key: String },

    /// Run not found
    #[error("run not found: {run}")]
    RunNotFound { run: String },

    /// Vector collection not found
    #[error("collection not found: {collection}")]
    CollectionNotFound { collection: String },

    /// Event stream not found
    #[error("stream not found: {stream}")]
    StreamNotFound { stream: String },

    /// State cell not found
    #[error("cell not found: {cell}")]
    CellNotFound { cell: String },

    /// JSON document not found
    #[error("document not found: {key}")]
    DocumentNotFound { key: String },

    // ==================== Type Errors ====================
    /// Wrong type for operation
    #[error("wrong type: expected {expected}, got {actual}")]
    WrongType { expected: String, actual: String },

    // ==================== Validation Errors ====================
    /// Invalid key format
    #[error("invalid key: {reason}")]
    InvalidKey { reason: String },

    /// Invalid JSON path
    #[error("invalid path: {reason}")]
    InvalidPath { reason: String },

    /// Invalid input
    #[error("invalid input: {reason}")]
    InvalidInput { reason: String },

    // ==================== Concurrency Errors ====================
    /// Version conflict (CAS failure)
    #[error("version conflict: expected {expected}, got {actual}")]
    VersionConflict { expected: u64, actual: u64 },

    /// State transition failed (expected value mismatch)
    #[error("transition failed: expected {expected}, got {actual}")]
    TransitionFailed { expected: String, actual: String },

    /// Generic conflict
    #[error("conflict: {reason}")]
    Conflict { reason: String },

    // ==================== State Errors ====================
    /// Run is closed
    #[error("run closed: {run}")]
    RunClosed { run: String },

    /// Run already exists
    #[error("run already exists: {run}")]
    RunExists { run: String },

    /// Collection already exists
    #[error("collection already exists: {collection}")]
    CollectionExists { collection: String },

    // ==================== Constraint Errors ====================
    /// Vector dimension mismatch
    #[error("dimension mismatch: expected {expected}, got {actual}")]
    DimensionMismatch { expected: usize, actual: usize },

    /// Constraint violation
    #[error("constraint violation: {reason}")]
    ConstraintViolation { reason: String },

    /// Requested version was trimmed by retention policy
    #[error("history trimmed: requested version {requested}, earliest is {earliest}")]
    HistoryTrimmed { requested: u64, earliest: u64 },

    /// Numeric overflow
    #[error("overflow: {reason}")]
    Overflow { reason: String },

    // ==================== Transaction Errors ====================
    /// No active transaction
    #[error("no active transaction")]
    TransactionNotActive,

    /// Transaction already active
    #[error("transaction already active")]
    TransactionAlreadyActive,

    // ==================== System Errors ====================
    /// I/O error
    #[error("I/O error: {reason}")]
    Io { reason: String },

    /// Serialization error
    #[error("serialization error: {reason}")]
    Serialization { reason: String },

    /// Internal error (bug or invariant violation)
    #[error("internal error: {reason}")]
    Internal { reason: String },
}
