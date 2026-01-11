//! Transaction context for OCC
//!
//! This module implements the core transaction data structure for optimistic
//! concurrency control. TransactionContext tracks all reads, writes, deletes,
//! and CAS operations for a transaction, enabling validation at commit time.
//!
//! See `docs/architecture/M2_TRANSACTION_SEMANTICS.md` for the full specification.

use in_mem_core::error::{Error, Result};
use in_mem_core::types::{Key, RunId};
use in_mem_core::value::Value;
use std::collections::{HashMap, HashSet};

/// Status of a transaction in its lifecycle
///
/// State transitions:
/// - `Active` → `Validating` (begin commit)
/// - `Validating` → `Committed` (validation passed)
/// - `Validating` → `Aborted` (conflict detected)
/// - `Active` → `Aborted` (user abort or error)
///
/// Terminal states (no transitions allowed):
/// - `Committed`
/// - `Aborted`
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransactionStatus {
    /// Transaction is executing, can read/write
    Active,
    /// Transaction is being validated for conflicts
    Validating,
    /// Transaction committed successfully
    Committed,
    /// Transaction was aborted
    Aborted {
        /// Human-readable reason for abort
        reason: String,
    },
}

/// A compare-and-swap operation to be validated at commit
///
/// CAS operations are buffered until commit time. At commit:
/// 1. Validate that the key's current version equals `expected_version`
/// 2. If valid, write `new_value`
/// 3. If invalid, abort the transaction
///
/// Note: CAS does NOT automatically add to read_set. If you want read-set
/// protection in addition to CAS validation, explicitly read the key first.
#[derive(Debug, Clone)]
pub struct CASOperation {
    /// Key to CAS
    pub key: Key,
    /// Expected version (0 = key must not exist)
    pub expected_version: u64,
    /// New value to write if version matches
    pub new_value: Value,
}

/// Transaction context for OCC with snapshot isolation
///
/// Tracks all reads, writes, deletes, and CAS operations for a transaction.
/// Validation and commit happen at transaction end.
///
/// # Lifecycle
///
/// 1. **BEGIN**: Create with `new()`, status is `Active`
/// 2. **READ/WRITE**: Buffer operations (Stories #81/#82 add these methods)
/// 3. **VALIDATE**: Call `mark_validating()`, check for conflicts
/// 4. **COMMIT/ABORT**: Call `mark_committed()` or `mark_aborted()`
///
/// # Example (conceptual - full API in later stories)
///
/// ```ignore
/// let mut txn = TransactionContext::new(1, run_id, 100);
/// // ... perform reads/writes ...
/// txn.mark_validating()?;
/// // ... validate against storage ...
/// txn.mark_committed()?;
/// ```
pub struct TransactionContext {
    // Identity
    /// Unique transaction ID
    pub txn_id: u64,
    /// Run this transaction belongs to
    pub run_id: RunId,

    // Snapshot isolation
    /// Version at transaction start (snapshot version)
    ///
    /// All reads see data as of this version. Used for conflict detection.
    pub start_version: u64,

    // Operation tracking
    /// Keys read and their versions (for validation)
    ///
    /// At commit time, we check that each key's current version still matches
    /// the version we read. If not, there's a read-write conflict.
    pub read_set: HashMap<Key, u64>,

    /// Keys written with their new values (buffered)
    ///
    /// These writes are not visible to other transactions until commit.
    /// At commit, they are applied atomically to storage.
    pub write_set: HashMap<Key, Value>,

    /// Keys to delete (buffered)
    ///
    /// Deletes are buffered like writes. A deleted key returns None
    /// when read within this transaction (read-your-deletes).
    pub delete_set: HashSet<Key>,

    /// CAS operations to validate and apply
    ///
    /// Each CAS is validated at commit time against the current storage
    /// version, independent of the read_set.
    pub cas_set: Vec<CASOperation>,

    // State
    /// Current transaction status
    pub status: TransactionStatus,
}

impl TransactionContext {
    /// Create a new transaction context
    ///
    /// The transaction starts in `Active` state with empty operation sets.
    ///
    /// # Arguments
    /// * `txn_id` - Unique transaction identifier
    /// * `run_id` - Run this transaction belongs to
    /// * `start_version` - Snapshot version at transaction start
    ///
    /// # Example
    ///
    /// ```
    /// use in_mem_concurrency::TransactionContext;
    /// use in_mem_core::types::RunId;
    ///
    /// let run_id = RunId::new();
    /// let txn = TransactionContext::new(1, run_id, 100);
    /// assert!(txn.is_active());
    /// ```
    pub fn new(txn_id: u64, run_id: RunId, start_version: u64) -> Self {
        TransactionContext {
            txn_id,
            run_id,
            start_version,
            read_set: HashMap::new(),
            write_set: HashMap::new(),
            delete_set: HashSet::new(),
            cas_set: Vec::new(),
            status: TransactionStatus::Active,
        }
    }

    /// Check if transaction is in Active state
    ///
    /// Only active transactions can accept new read/write operations.
    pub fn is_active(&self) -> bool {
        matches!(self.status, TransactionStatus::Active)
    }

    /// Check if transaction is committed
    pub fn is_committed(&self) -> bool {
        matches!(self.status, TransactionStatus::Committed)
    }

    /// Check if transaction is aborted
    pub fn is_aborted(&self) -> bool {
        matches!(self.status, TransactionStatus::Aborted { .. })
    }

    /// Check if transaction can accept operations
    ///
    /// # Errors
    /// Returns `Error::InvalidState` if transaction is not in `Active` state.
    pub fn ensure_active(&self) -> Result<()> {
        if self.is_active() {
            Ok(())
        } else {
            Err(Error::InvalidState(format!(
                "Transaction {} is not active: {:?}",
                self.txn_id, self.status
            )))
        }
    }

    /// Transition to Validating state
    ///
    /// This is the first step of the commit process. After marking validating,
    /// the transaction should be validated against current storage state.
    ///
    /// # Errors
    /// Returns `Error::InvalidState` if not in `Active` state.
    ///
    /// # State Transition
    /// `Active` → `Validating`
    pub fn mark_validating(&mut self) -> Result<()> {
        self.ensure_active()?;
        self.status = TransactionStatus::Validating;
        Ok(())
    }

    /// Transition to Committed state
    ///
    /// Called after successful validation. The transaction's writes have been
    /// applied to storage.
    ///
    /// # Errors
    /// Returns `Error::InvalidState` if not in `Validating` state.
    ///
    /// # State Transition
    /// `Validating` → `Committed`
    pub fn mark_committed(&mut self) -> Result<()> {
        match &self.status {
            TransactionStatus::Validating => {
                self.status = TransactionStatus::Committed;
                Ok(())
            }
            _ => Err(Error::InvalidState(format!(
                "Cannot commit transaction {} from state {:?}",
                self.txn_id, self.status
            ))),
        }
    }

    /// Transition to Aborted state
    ///
    /// Can be called from `Active` (user abort) or `Validating` (conflict detected).
    /// Buffered writes are discarded - they were never applied to storage.
    ///
    /// # Arguments
    /// * `reason` - Human-readable reason for abort
    ///
    /// # Errors
    /// Returns `Error::InvalidState` if already `Committed` or `Aborted`.
    ///
    /// # State Transitions
    /// - `Active` → `Aborted`
    /// - `Validating` → `Aborted`
    pub fn mark_aborted(&mut self, reason: String) -> Result<()> {
        match &self.status {
            TransactionStatus::Committed => Err(Error::InvalidState(format!(
                "Cannot abort committed transaction {}",
                self.txn_id
            ))),
            TransactionStatus::Aborted { .. } => Err(Error::InvalidState(format!(
                "Transaction {} already aborted",
                self.txn_id
            ))),
            _ => {
                self.status = TransactionStatus::Aborted { reason };
                Ok(())
            }
        }
    }

    /// Get the number of keys in the read set
    pub fn read_count(&self) -> usize {
        self.read_set.len()
    }

    /// Get the number of keys in the write set
    pub fn write_count(&self) -> usize {
        self.write_set.len()
    }

    /// Get the number of keys in the delete set
    pub fn delete_count(&self) -> usize {
        self.delete_set.len()
    }

    /// Get the number of CAS operations
    pub fn cas_count(&self) -> usize {
        self.cas_set.len()
    }

    /// Check if transaction has any pending operations
    ///
    /// Returns true if there are buffered writes, deletes, or CAS operations
    /// that would need to be applied at commit.
    pub fn has_pending_operations(&self) -> bool {
        !self.write_set.is_empty() || !self.delete_set.is_empty() || !self.cas_set.is_empty()
    }

    /// Check if transaction is read-only
    ///
    /// A read-only transaction has reads but no writes, deletes, or CAS ops.
    /// Read-only transactions always commit successfully (no conflicts possible
    /// since they don't modify anything).
    pub fn is_read_only(&self) -> bool {
        self.write_set.is_empty() && self.delete_set.is_empty() && self.cas_set.is_empty()
    }

    /// Get the abort reason if transaction is aborted
    pub fn abort_reason(&self) -> Option<&str> {
        match &self.status {
            TransactionStatus::Aborted { reason } => Some(reason),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_txn() -> TransactionContext {
        let run_id = RunId::new();
        TransactionContext::new(1, run_id, 100)
    }

    // === Construction Tests ===

    #[test]
    fn test_new_transaction_is_active() {
        let txn = create_test_txn();
        assert!(txn.is_active());
        assert!(!txn.is_committed());
        assert!(!txn.is_aborted());
        assert_eq!(txn.txn_id, 1);
        assert_eq!(txn.start_version, 100);
    }

    #[test]
    fn test_new_transaction_has_empty_sets() {
        let txn = create_test_txn();
        assert_eq!(txn.read_count(), 0);
        assert_eq!(txn.write_count(), 0);
        assert_eq!(txn.delete_count(), 0);
        assert_eq!(txn.cas_count(), 0);
        assert!(!txn.has_pending_operations());
        assert!(txn.is_read_only());
    }

    #[test]
    fn test_new_transaction_preserves_run_id() {
        let run_id = RunId::new();
        let txn = TransactionContext::new(42, run_id.clone(), 500);
        assert_eq!(txn.run_id, run_id);
        assert_eq!(txn.txn_id, 42);
        assert_eq!(txn.start_version, 500);
    }

    // === State Transition Tests ===

    #[test]
    fn test_state_transition_active_to_validating() {
        let mut txn = create_test_txn();
        assert!(txn.mark_validating().is_ok());
        assert!(!txn.is_active());
        assert!(matches!(txn.status, TransactionStatus::Validating));
    }

    #[test]
    fn test_state_transition_validating_to_committed() {
        let mut txn = create_test_txn();
        txn.mark_validating().unwrap();
        assert!(txn.mark_committed().is_ok());
        assert!(txn.is_committed());
        assert!(matches!(txn.status, TransactionStatus::Committed));
    }

    #[test]
    fn test_state_transition_active_to_aborted() {
        let mut txn = create_test_txn();
        assert!(txn.mark_aborted("user requested abort".to_string()).is_ok());
        assert!(txn.is_aborted());
        assert!(matches!(txn.status, TransactionStatus::Aborted { .. }));
        assert_eq!(txn.abort_reason(), Some("user requested abort"));
    }

    #[test]
    fn test_state_transition_validating_to_aborted() {
        let mut txn = create_test_txn();
        txn.mark_validating().unwrap();
        assert!(txn.mark_aborted("conflict detected".to_string()).is_ok());
        assert!(txn.is_aborted());
        assert_eq!(txn.abort_reason(), Some("conflict detected"));
    }

    // === Invalid State Transition Tests ===

    #[test]
    fn test_cannot_validating_from_committed() {
        let mut txn = create_test_txn();
        txn.mark_validating().unwrap();
        txn.mark_committed().unwrap();
        let result = txn.mark_validating();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidState(_)));
    }

    #[test]
    fn test_cannot_validating_from_aborted() {
        let mut txn = create_test_txn();
        txn.mark_aborted("test".to_string()).unwrap();
        let result = txn.mark_validating();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidState(_)));
    }

    #[test]
    fn test_cannot_commit_from_active() {
        let mut txn = create_test_txn();
        // Cannot commit directly from Active, must validate first
        let result = txn.mark_committed();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidState(_)));
    }

    #[test]
    fn test_cannot_commit_from_aborted() {
        let mut txn = create_test_txn();
        txn.mark_aborted("test".to_string()).unwrap();
        let result = txn.mark_committed();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidState(_)));
    }

    #[test]
    fn test_cannot_commit_from_committed() {
        let mut txn = create_test_txn();
        txn.mark_validating().unwrap();
        txn.mark_committed().unwrap();
        let result = txn.mark_committed();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidState(_)));
    }

    #[test]
    fn test_cannot_abort_committed_transaction() {
        let mut txn = create_test_txn();
        txn.mark_validating().unwrap();
        txn.mark_committed().unwrap();
        let result = txn.mark_aborted("too late".to_string());
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidState(_)));
    }

    #[test]
    fn test_cannot_abort_already_aborted() {
        let mut txn = create_test_txn();
        txn.mark_aborted("first abort".to_string()).unwrap();
        let result = txn.mark_aborted("second abort".to_string());
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidState(_)));
    }

    // === ensure_active Tests ===

    #[test]
    fn test_ensure_active_succeeds_when_active() {
        let txn = create_test_txn();
        assert!(txn.ensure_active().is_ok());
    }

    #[test]
    fn test_ensure_active_fails_when_validating() {
        let mut txn = create_test_txn();
        txn.mark_validating().unwrap();
        let result = txn.ensure_active();
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::InvalidState(_)));
    }

    #[test]
    fn test_ensure_active_fails_when_committed() {
        let mut txn = create_test_txn();
        txn.mark_validating().unwrap();
        txn.mark_committed().unwrap();
        let result = txn.ensure_active();
        assert!(result.is_err());
    }

    #[test]
    fn test_ensure_active_fails_when_aborted() {
        let mut txn = create_test_txn();
        txn.mark_aborted("test".to_string()).unwrap();
        let result = txn.ensure_active();
        assert!(result.is_err());
    }

    // === Abort Reason Tests ===

    #[test]
    fn test_abort_reason_none_when_not_aborted() {
        let txn = create_test_txn();
        assert!(txn.abort_reason().is_none());
    }

    #[test]
    fn test_abort_reason_none_when_committed() {
        let mut txn = create_test_txn();
        txn.mark_validating().unwrap();
        txn.mark_committed().unwrap();
        assert!(txn.abort_reason().is_none());
    }

    #[test]
    fn test_abort_reason_preserves_message() {
        let mut txn = create_test_txn();
        txn.mark_aborted("read-write conflict on key X".to_string())
            .unwrap();
        assert_eq!(txn.abort_reason(), Some("read-write conflict on key X"));
    }

    // === CASOperation Tests ===

    #[test]
    fn test_cas_operation_creation() {
        use in_mem_core::types::{Namespace, TypeTag};

        let run_id = RunId::new();
        let namespace = Namespace::new("t".into(), "a".into(), "g".into(), run_id);
        let key = Key::new(namespace, TypeTag::KV, b"test".to_vec());
        let value = Value::I64(42);

        let cas_op = CASOperation {
            key: key.clone(),
            expected_version: 5,
            new_value: value.clone(),
        };

        assert_eq!(cas_op.key, key);
        assert_eq!(cas_op.expected_version, 5);
        assert_eq!(cas_op.new_value, value);
    }

    #[test]
    fn test_cas_operation_version_zero_means_not_exist() {
        use in_mem_core::types::{Namespace, TypeTag};

        let run_id = RunId::new();
        let namespace = Namespace::new("t".into(), "a".into(), "g".into(), run_id);
        let key = Key::new(namespace, TypeTag::KV, b"new_key".to_vec());

        // expected_version = 0 means "key must not exist"
        let cas_op = CASOperation {
            key,
            expected_version: 0,
            new_value: Value::String("initial".to_string()),
        };

        assert_eq!(cas_op.expected_version, 0);
    }

    // === TransactionStatus Tests ===

    #[test]
    fn test_transaction_status_equality() {
        assert_eq!(TransactionStatus::Active, TransactionStatus::Active);
        assert_eq!(TransactionStatus::Validating, TransactionStatus::Validating);
        assert_eq!(TransactionStatus::Committed, TransactionStatus::Committed);

        let aborted1 = TransactionStatus::Aborted {
            reason: "test".to_string(),
        };
        let aborted2 = TransactionStatus::Aborted {
            reason: "test".to_string(),
        };
        let aborted3 = TransactionStatus::Aborted {
            reason: "other".to_string(),
        };

        assert_eq!(aborted1, aborted2);
        assert_ne!(aborted1, aborted3);
        assert_ne!(TransactionStatus::Active, TransactionStatus::Validating);
    }

    #[test]
    fn test_transaction_status_debug() {
        let active = TransactionStatus::Active;
        let debug_str = format!("{:?}", active);
        assert!(debug_str.contains("Active"));

        let aborted = TransactionStatus::Aborted {
            reason: "conflict".to_string(),
        };
        let debug_str = format!("{:?}", aborted);
        assert!(debug_str.contains("Aborted"));
        assert!(debug_str.contains("conflict"));
    }

    #[test]
    fn test_transaction_status_clone() {
        let original = TransactionStatus::Aborted {
            reason: "test".to_string(),
        };
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }
}
