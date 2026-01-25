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
}
