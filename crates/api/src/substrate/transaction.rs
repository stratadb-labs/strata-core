//! Transaction Control Substrate Operations
//!
//! The transaction layer provides ACID guarantees for operations across primitives.
//! Every write operation participates in transactions (Invariant 3).
//!
//! ## Transaction Model
//!
//! - Begin starts a new transaction context
//! - Operations within the context are batched
//! - Commit makes all changes durable atomically
//! - Rollback discards all pending changes
//!
//! ## Isolation Level
//!
//! Strata uses **Snapshot Serializable** isolation:
//! - Reads see a consistent snapshot
//! - Writes are serialized
//! - No phantom reads, no dirty reads
//!
//! ## Auto-Commit Mode
//!
//! Without explicit begin/commit:
//! - Each operation runs in its own micro-transaction
//! - Commit happens immediately after each operation
//! - This is the facade API default

use strata_core::{StrataResult, Version};
use serde::{Deserialize, Serialize};

/// Transaction identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TxnId(u64);

impl TxnId {
    /// Create a transaction ID from a raw value
    pub fn new(id: u64) -> Self {
        TxnId(id)
    }

    /// Get the raw ID value
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for TxnId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "txn:{}", self.0)
    }
}

/// Transaction options
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TxnOptions {
    /// Timeout in milliseconds (default: no timeout)
    pub timeout_ms: Option<u64>,

    /// Whether to acquire read locks (for pessimistic concurrency)
    /// Default: false (optimistic concurrency)
    pub read_locks: bool,
}

/// Transaction status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TxnStatus {
    /// Transaction is active
    Active,
    /// Transaction committed successfully
    Committed,
    /// Transaction was rolled back
    RolledBack,
    /// Transaction aborted due to conflict
    Aborted,
}

/// Transaction info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxnInfo {
    /// Transaction ID
    pub id: TxnId,
    /// Current status
    pub status: TxnStatus,
    /// Start timestamp (microseconds since epoch)
    pub started_at: u64,
    /// Commit/rollback timestamp (if completed)
    pub completed_at: Option<u64>,
    /// Number of operations in transaction
    pub operation_count: u64,
}

/// Transaction control substrate operations
///
/// This trait defines the canonical transaction control operations.
///
/// ## Contract
///
/// - Every operation executes within a transaction context
/// - Explicit transactions batch multiple operations
/// - Commit is atomic (all-or-nothing)
/// - Rollback discards all pending changes
///
/// ## Error Handling
///
/// | Condition | Error |
/// |-----------|-------|
/// | Transaction already active | `ConstraintViolation` |
/// | No active transaction | `ConstraintViolation` |
/// | Transaction conflict | `Conflict` |
/// | Transaction timeout | `Conflict` |
pub trait TransactionControl {
    /// Begin a new transaction
    ///
    /// Starts a new transaction and returns its ID.
    ///
    /// ## Semantics
    ///
    /// - Creates a new transaction context
    /// - Subsequent operations use this context
    /// - Must be followed by commit or rollback
    ///
    /// ## Errors
    ///
    /// - `ConstraintViolation`: Transaction already active in this context
    fn txn_begin(&self, options: Option<TxnOptions>) -> StrataResult<TxnId>;

    /// Commit the current transaction
    ///
    /// Makes all changes in the transaction durable.
    /// Returns the commit version.
    ///
    /// ## Semantics
    ///
    /// - Validates all changes against constraints
    /// - Checks for conflicts (optimistic concurrency)
    /// - If successful, all changes become visible atomically
    /// - If failed, transaction is aborted
    ///
    /// ## Return Value
    ///
    /// Returns `Version::Txn(n)` where `n` is the commit transaction ID.
    ///
    /// ## Errors
    ///
    /// - `ConstraintViolation`: No active transaction, or validation failed
    /// - `Conflict`: Transaction conflict detected (retry)
    fn txn_commit(&self) -> StrataResult<Version>;

    /// Rollback the current transaction
    ///
    /// Discards all pending changes in the transaction.
    ///
    /// ## Semantics
    ///
    /// - Discards all pending changes
    /// - Releases any locks
    /// - Transaction context is cleared
    ///
    /// ## Errors
    ///
    /// - `ConstraintViolation`: No active transaction
    fn txn_rollback(&self) -> StrataResult<()>;

    /// Get current transaction info
    ///
    /// Returns information about the active transaction, if any.
    ///
    /// ## Return Value
    ///
    /// - `Some(TxnInfo)`: Transaction is active
    /// - `None`: No active transaction (auto-commit mode)
    fn txn_info(&self) -> StrataResult<Option<TxnInfo>>;

    /// Check if a transaction is active
    ///
    /// Returns `true` if there's an active transaction.
    fn txn_is_active(&self) -> StrataResult<bool>;
}

/// Savepoint operations for nested transactions
///
/// Savepoints allow partial rollback within a transaction.
pub trait TransactionSavepoint: TransactionControl {
    /// Create a savepoint
    ///
    /// Creates a named savepoint in the current transaction.
    ///
    /// ## Errors
    ///
    /// - `ConstraintViolation`: No active transaction, or savepoint name invalid
    fn savepoint(&self, name: &str) -> StrataResult<()>;

    /// Rollback to a savepoint
    ///
    /// Rolls back to the named savepoint, discarding changes since then.
    ///
    /// ## Errors
    ///
    /// - `ConstraintViolation`: No active transaction
    /// - `NotFound`: Savepoint does not exist
    fn rollback_to(&self, name: &str) -> StrataResult<()>;

    /// Release a savepoint
    ///
    /// Removes a savepoint (cannot rollback to it after release).
    ///
    /// ## Errors
    ///
    /// - `ConstraintViolation`: No active transaction
    /// - `NotFound`: Savepoint does not exist
    fn release_savepoint(&self, name: &str) -> StrataResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trait_is_object_safe() {
        fn _assert_object_safe(_: &dyn TransactionControl) {}
        fn _assert_savepoint_object_safe(_: &dyn TransactionSavepoint) {}
    }

    #[test]
    fn test_txn_id_display() {
        let id = TxnId::new(42);
        assert_eq!(format!("{}", id), "txn:42");
    }

    #[test]
    fn test_txn_options_default() {
        let opts = TxnOptions::default();
        assert!(opts.timeout_ms.is_none());
        assert!(!opts.read_locks);
    }
}
