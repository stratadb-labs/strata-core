//! Durability abstraction for performance modes
//!
//! This module defines the `Durability` trait that abstracts over different
//! persistence strategies. Each mode implements this trait:
//!
//! - **InMemory**: No persistence, fastest mode
//! - **Buffered**: Async WAL append, periodic fsync
//! - **Strict**: Sync WAL append + immediate fsync
//!
//! # Architecture
//!
//! The durability layer sits between transaction commit and storage apply:
//!
//! ```text
//! Transaction Commit Flow:
//!   1. Validate transaction (OCC)
//!   2. Allocate commit version
//!   3. Durability::persist() ← MODE-SPECIFIC
//!   4. Apply to storage
//!   5. Mark committed
//! ```
//!
//! # Performance Targets
//!
//! | Mode | Target Latency | Use Case |
//! |------|----------------|----------|
//! | InMemory | <3µs | Tests, caches, ephemeral |
//! | Buffered | <30µs | Production default |
//! | Strict | ~2ms | Audit logs, checkpoints |

use strata_concurrency::TransactionContext;
use strata_core::error::Result;
use strata_core::types::RunId;

/// Durability behavior abstraction
///
/// All three durability modes implement this trait:
/// - InMemory: No persistence
/// - Buffered: Async persistence with periodic flush
/// - Strict: Sync persistence (fsync every write)
///
/// # Thread Safety
///
/// Implementations must be `Send + Sync` for use in multi-threaded contexts.
///
/// # Example
///
/// ```ignore
/// use strata_engine::durability::Durability;
///
/// fn commit_with_durability<D: Durability>(
///     durability: &D,
///     txn: &TransactionContext,
///     commit_version: u64,
/// ) -> Result<()> {
///     durability.persist(txn, commit_version)?;
///     // ... apply to storage
///     Ok(())
/// }
/// ```
pub trait Durability: Send + Sync {
    /// Persist transaction writes according to this durability mode
    ///
    /// This method is called during transaction commit, after validation
    /// passes and before storage apply. It handles WAL writes and fsync
    /// based on the durability mode.
    ///
    /// # Arguments
    ///
    /// * `txn` - Transaction context containing write_set, delete_set, cas_set
    /// * `commit_version` - Version allocated for this commit
    ///
    /// # Contract by Mode
    ///
    /// - **InMemory**: No-op, returns immediately
    /// - **Buffered**: Append to WAL buffer, trigger async flush if needed
    /// - **Strict**: Append to WAL, fsync, then return
    ///
    /// # Errors
    ///
    /// Returns an error if WAL write or fsync fails.
    fn persist(&self, txn: &TransactionContext, commit_version: u64) -> Result<()>;

    /// Graceful shutdown - flush any pending data
    ///
    /// Called when the database is closing. Ensures all buffered data
    /// is persisted before shutdown completes.
    ///
    /// # Contract by Mode
    ///
    /// - **InMemory**: No-op (nothing to flush)
    /// - **Buffered**: Flush all pending writes, fsync, stop background thread
    /// - **Strict**: No-op (already synced on every write)
    ///
    /// # Errors
    ///
    /// Returns an error if the final flush fails.
    fn shutdown(&self) -> Result<()>;

    /// Check if this durability mode persists data
    ///
    /// Returns `true` if data survives process crash.
    ///
    /// - **InMemory**: `false`
    /// - **Buffered**: `true` (eventually, after flush)
    /// - **Strict**: `true` (immediately)
    fn is_persistent(&self) -> bool;

    /// Get human-readable mode name for logging/debugging
    ///
    /// Returns one of: "InMemory", "Buffered", "Strict"
    fn mode_name(&self) -> &'static str;

    /// Check if this mode requires a WAL file
    ///
    /// InMemory mode doesn't need a WAL file at all.
    fn requires_wal(&self) -> bool {
        self.is_persistent()
    }
}

/// Extension trait for commit-time operations
///
/// Provides helper methods for common patterns during transaction commit.
pub trait DurabilityExt: Durability {
    /// Persist with optional immediate sync override
    ///
    /// For critical writes in non-strict mode, this forces immediate fsync.
    /// Useful for metadata or audit log entries in Buffered mode.
    ///
    /// # Arguments
    ///
    /// * `txn` - Transaction context
    /// * `commit_version` - Version for this commit
    /// * `force_sync` - If true, force immediate fsync regardless of mode
    fn persist_with_sync(
        &self,
        txn: &TransactionContext,
        commit_version: u64,
        force_sync: bool,
    ) -> Result<()>;
}

/// Commit data extracted from a transaction
///
/// This struct contains the data needed for persistence,
/// extracted from TransactionContext to avoid holding references.
#[derive(Debug, Clone)]
pub struct CommitData {
    /// Transaction ID
    pub txn_id: u64,
    /// Run ID for this transaction
    pub run_id: RunId,
    /// Commit version assigned to this transaction
    pub commit_version: u64,
    /// Number of puts in write_set
    pub put_count: usize,
    /// Number of deletes in delete_set
    pub delete_count: usize,
    /// Number of CAS operations
    pub cas_count: usize,
}

impl CommitData {
    /// Create from transaction context
    pub fn from_transaction(txn: &TransactionContext, commit_version: u64) -> Self {
        Self {
            txn_id: txn.txn_id,
            run_id: txn.run_id,
            commit_version,
            put_count: txn.write_set.len(),
            delete_count: txn.delete_set.len(),
            cas_count: txn.cas_set.len(),
        }
    }

    /// Total number of operations
    pub fn total_operations(&self) -> usize {
        self.put_count + self.delete_count + self.cas_count
    }

    /// Check if transaction has no writes
    pub fn is_read_only(&self) -> bool {
        self.total_operations() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock durability for testing
    struct MockDurability {
        persistent: bool,
        name: &'static str,
    }

    impl Durability for MockDurability {
        fn persist(&self, _txn: &TransactionContext, _commit_version: u64) -> Result<()> {
            Ok(())
        }

        fn shutdown(&self) -> Result<()> {
            Ok(())
        }

        fn is_persistent(&self) -> bool {
            self.persistent
        }

        fn mode_name(&self) -> &'static str {
            self.name
        }
    }

    #[test]
    fn test_requires_wal_default() {
        let persistent = MockDurability {
            persistent: true,
            name: "Strict",
        };
        assert!(persistent.requires_wal());

        let inmemory = MockDurability {
            persistent: false,
            name: "InMemory",
        };
        assert!(!inmemory.requires_wal());
    }

    #[test]
    fn test_mode_names() {
        let strict = MockDurability {
            persistent: true,
            name: "Strict",
        };
        assert_eq!(strict.mode_name(), "Strict");

        let inmemory = MockDurability {
            persistent: false,
            name: "InMemory",
        };
        assert_eq!(inmemory.mode_name(), "InMemory");
    }

    // ========== CommitData Tests ==========

    #[test]
    fn test_commit_data_from_transaction_with_writes() {
        use strata_core::types::Key;

        let run_id = RunId::new();
        let mut txn = TransactionContext::new(1, run_id, 0);

        // Add some writes
        let ns = strata_core::types::Namespace::for_run(run_id);
        txn.write_set.insert(
            Key::new_kv(ns.clone(), "key1"),
            strata_core::value::Value::Int(1),
        );
        txn.write_set.insert(
            Key::new_kv(ns.clone(), "key2"),
            strata_core::value::Value::Int(2),
        );
        txn.write_set.insert(
            Key::new_kv(ns.clone(), "key3"),
            strata_core::value::Value::Int(3),
        );

        // Add some deletes
        txn.delete_set.insert(Key::new_kv(ns.clone(), "del1"));
        txn.delete_set.insert(Key::new_kv(ns.clone(), "del2"));

        let commit_data = CommitData::from_transaction(&txn, 42);

        assert_eq!(commit_data.txn_id, 1);
        assert_eq!(commit_data.run_id, run_id);
        assert_eq!(commit_data.commit_version, 42);
        assert_eq!(commit_data.put_count, 3);
        assert_eq!(commit_data.delete_count, 2);
        assert_eq!(commit_data.cas_count, 0); // No CAS operations added
    }

    #[test]
    fn test_commit_data_total_operations() {
        let commit_data = CommitData {
            txn_id: 1,
            run_id: RunId::new(),
            commit_version: 100,
            put_count: 5,
            delete_count: 3,
            cas_count: 2,
        };

        assert_eq!(commit_data.total_operations(), 10);
    }

    #[test]
    fn test_commit_data_total_operations_zero() {
        let commit_data = CommitData {
            txn_id: 1,
            run_id: RunId::new(),
            commit_version: 100,
            put_count: 0,
            delete_count: 0,
            cas_count: 0,
        };

        assert_eq!(commit_data.total_operations(), 0);
    }

    #[test]
    fn test_commit_data_is_read_only_true() {
        let commit_data = CommitData {
            txn_id: 1,
            run_id: RunId::new(),
            commit_version: 100,
            put_count: 0,
            delete_count: 0,
            cas_count: 0,
        };

        assert!(commit_data.is_read_only());
    }

    #[test]
    fn test_commit_data_is_read_only_false_with_puts() {
        let commit_data = CommitData {
            txn_id: 1,
            run_id: RunId::new(),
            commit_version: 100,
            put_count: 1,
            delete_count: 0,
            cas_count: 0,
        };

        assert!(!commit_data.is_read_only());
    }

    #[test]
    fn test_commit_data_is_read_only_false_with_deletes() {
        let commit_data = CommitData {
            txn_id: 1,
            run_id: RunId::new(),
            commit_version: 100,
            put_count: 0,
            delete_count: 1,
            cas_count: 0,
        };

        assert!(!commit_data.is_read_only());
    }

    #[test]
    fn test_commit_data_is_read_only_false_with_cas() {
        let commit_data = CommitData {
            txn_id: 1,
            run_id: RunId::new(),
            commit_version: 100,
            put_count: 0,
            delete_count: 0,
            cas_count: 1,
        };

        assert!(!commit_data.is_read_only());
    }

    #[test]
    fn test_commit_data_clone() {
        let commit_data = CommitData {
            txn_id: 42,
            run_id: RunId::new(),
            commit_version: 100,
            put_count: 5,
            delete_count: 3,
            cas_count: 2,
        };

        let cloned = commit_data.clone();
        assert_eq!(commit_data.txn_id, cloned.txn_id);
        assert_eq!(commit_data.run_id, cloned.run_id);
        assert_eq!(commit_data.commit_version, cloned.commit_version);
        assert_eq!(commit_data.put_count, cloned.put_count);
        assert_eq!(commit_data.delete_count, cloned.delete_count);
        assert_eq!(commit_data.cas_count, cloned.cas_count);
    }

    #[test]
    fn test_commit_data_debug() {
        let commit_data = CommitData {
            txn_id: 42,
            run_id: RunId::new(),
            commit_version: 100,
            put_count: 5,
            delete_count: 3,
            cas_count: 2,
        };

        let debug_str = format!("{:?}", commit_data);
        assert!(debug_str.contains("CommitData"));
        assert!(debug_str.contains("txn_id"));
        assert!(debug_str.contains("42"));
        assert!(debug_str.contains("put_count"));
        assert!(debug_str.contains("5"));
    }

    // ========== Durability Trait Object Safety ==========

    fn accept_dyn_durability(_d: &dyn Durability) {}

    #[test]
    fn test_durability_trait_is_object_safe() {
        let mock = MockDurability {
            persistent: true,
            name: "Test",
        };
        accept_dyn_durability(&mock);
    }

    #[test]
    fn test_durability_can_be_boxed() {
        let mock: Box<dyn Durability> = Box::new(MockDurability {
            persistent: true,
            name: "Boxed",
        });

        assert!(mock.is_persistent());
        assert_eq!(mock.mode_name(), "Boxed");
        assert!(mock.requires_wal());
    }
}
