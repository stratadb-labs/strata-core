//! Transaction coordinator for managing transaction lifecycle
//!
//! Per spec Section 6.1:
//! - Single monotonic counter for the entire database
//! - Incremented on each COMMIT (not each write)
//!
//! The TransactionCoordinator wraps TransactionManager and adds:
//! - Active transaction tracking
//! - Transaction metrics (started, committed, aborted)
//! - Commit rate calculation

use strata_concurrency::{RecoveryResult, TransactionContext, TransactionManager};
use strata_core::types::RunId;
use strata_storage::ShardedStore;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Transaction coordinator for the database
///
/// Manages transaction lifecycle, ID allocation, version tracking, and metrics.
/// Per spec Section 6.1: Single monotonic counter for the entire database.
pub struct TransactionCoordinator {
    /// Transaction manager for ID/version allocation
    manager: TransactionManager,
    /// Active transaction count (for metrics)
    active_count: AtomicU64,
    /// Total transactions started
    total_started: AtomicU64,
    /// Total transactions committed
    total_committed: AtomicU64,
    /// Total transactions aborted
    total_aborted: AtomicU64,
}

impl TransactionCoordinator {
    /// Create new coordinator with initial version
    ///
    /// # Arguments
    /// * `initial_version` - Starting version (typically from storage or recovery)
    pub fn new(initial_version: u64) -> Self {
        Self {
            manager: TransactionManager::new(initial_version),
            active_count: AtomicU64::new(0),
            total_started: AtomicU64::new(0),
            total_committed: AtomicU64::new(0),
            total_aborted: AtomicU64::new(0),
        }
    }

    /// Create coordinator from recovery result
    ///
    /// Initializes the coordinator with the version AND max_txn_id from recovery,
    /// ensuring new transactions get monotonically increasing versions and IDs.
    ///
    /// CRITICAL: Both final_version AND max_txn_id must be restored to ensure:
    /// - Versions are monotonically increasing (final_version)
    /// - Transaction IDs are unique across sessions (max_txn_id)
    ///
    /// # Arguments
    /// * `result` - Recovery result containing final version and max_txn_id
    pub fn from_recovery(result: &RecoveryResult) -> Self {
        Self {
            manager: TransactionManager::with_txn_id(
                result.stats.final_version,
                result.stats.max_txn_id,
            ),
            active_count: AtomicU64::new(0),
            total_started: AtomicU64::new(0),
            total_committed: AtomicU64::new(0),
            total_aborted: AtomicU64::new(0),
        }
    }

    /// Start a new transaction
    ///
    /// Creates a TransactionContext with a snapshot of the current storage state.
    /// Increments active count and total started metrics.
    ///
    /// # Arguments
    /// * `run_id` - RunId for namespace isolation
    /// * `storage` - Storage to create snapshot from
    ///
    /// # Returns
    /// * `TransactionContext` - Active transaction ready for operations
    pub fn start_transaction(
        &self,
        run_id: RunId,
        storage: &Arc<ShardedStore>,
    ) -> TransactionContext {
        let txn_id = self.manager.next_txn_id();
        let snapshot = storage.create_snapshot();

        self.active_count.fetch_add(1, Ordering::Relaxed);
        self.total_started.fetch_add(1, Ordering::Relaxed);

        TransactionContext::with_snapshot(txn_id, run_id, Box::new(snapshot))
    }

    /// Allocate commit version
    ///
    /// Per spec Section 6.1: Version incremented ONCE for the whole transaction.
    /// All keys in a transaction get the same commit version.
    pub fn allocate_commit_version(&self) -> u64 {
        self.manager.allocate_version()
    }

    /// Record transaction start
    ///
    /// Increments active count and total started count.
    /// Used by pooled transaction API that manages context creation separately.
    pub fn record_start(&self) {
        self.active_count.fetch_add(1, Ordering::Relaxed);
        self.total_started.fetch_add(1, Ordering::Relaxed);
    }

    /// Record transaction commit
    ///
    /// Decrements active count and increments committed count.
    pub fn record_commit(&self) {
        self.active_count.fetch_sub(1, Ordering::Relaxed);
        self.total_committed.fetch_add(1, Ordering::Relaxed);
    }

    /// Record transaction abort
    ///
    /// Decrements active count and increments aborted count.
    pub fn record_abort(&self) {
        self.active_count.fetch_sub(1, Ordering::Relaxed);
        self.total_aborted.fetch_add(1, Ordering::Relaxed);
    }

    /// Get current global version
    pub fn current_version(&self) -> u64 {
        self.manager.current_version()
    }

    /// Get next transaction ID (for internal use)
    pub fn next_txn_id(&self) -> u64 {
        self.manager.next_txn_id()
    }

    /// Get transaction metrics
    ///
    /// Returns current snapshot of transaction statistics.
    pub fn metrics(&self) -> TransactionMetrics {
        let started = self.total_started.load(Ordering::Relaxed);
        let committed = self.total_committed.load(Ordering::Relaxed);

        TransactionMetrics {
            active_count: self.active_count.load(Ordering::Relaxed),
            total_started: started,
            total_committed: committed,
            total_aborted: self.total_aborted.load(Ordering::Relaxed),
            commit_rate: if started > 0 {
                committed as f64 / started as f64
            } else {
                0.0
            },
        }
    }
}

/// Transaction metrics
///
/// Provides statistics about transaction lifecycle.
#[derive(Debug, Clone)]
pub struct TransactionMetrics {
    /// Number of currently active transactions
    pub active_count: u64,
    /// Total number of transactions started
    pub total_started: u64,
    /// Total number of transactions committed
    pub total_committed: u64,
    /// Total number of transactions aborted
    pub total_aborted: u64,
    /// Commit success rate (committed / started)
    pub commit_rate: f64,
}

impl TransactionMetrics {
    /// Total transactions that completed (committed + aborted)
    pub fn total_completed(&self) -> u64 {
        self.total_committed + self.total_aborted
    }

    /// Abort rate (aborted / started)
    pub fn abort_rate(&self) -> f64 {
        if self.total_started > 0 {
            self.total_aborted as f64 / self.total_started as f64
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_storage() -> Arc<ShardedStore> {
        Arc::new(ShardedStore::new())
    }

    #[test]
    fn test_coordinator_new() {
        let coordinator = TransactionCoordinator::new(0);
        assert_eq!(coordinator.current_version(), 0);

        let metrics = coordinator.metrics();
        assert_eq!(metrics.active_count, 0);
        assert_eq!(metrics.total_started, 0);
        assert_eq!(metrics.total_committed, 0);
        assert_eq!(metrics.total_aborted, 0);
    }

    #[test]
    fn test_coordinator_from_recovery() {
        use strata_concurrency::RecoveryStats;

        let stats = RecoveryStats {
            txns_replayed: 5,
            incomplete_txns: 1,
            aborted_txns: 0,
            writes_applied: 10,
            deletes_applied: 2,
            final_version: 100,
            max_txn_id: 6,
            from_checkpoint: false,
        };

        let result = RecoveryResult {
            storage: ShardedStore::new(),
            txn_manager: TransactionManager::new(100),
            stats,
        };

        let coordinator = TransactionCoordinator::from_recovery(&result);
        assert_eq!(coordinator.current_version(), 100);
    }

    #[test]
    fn test_start_transaction_updates_metrics() {
        let coordinator = TransactionCoordinator::new(0);
        let storage = create_test_storage();
        let run_id = RunId::new();

        let _txn1 = coordinator.start_transaction(run_id, &storage);
        let _txn2 = coordinator.start_transaction(run_id, &storage);

        let metrics = coordinator.metrics();
        assert_eq!(metrics.total_started, 2);
        assert_eq!(metrics.active_count, 2);
    }

    #[test]
    fn test_record_commit_updates_metrics() {
        let coordinator = TransactionCoordinator::new(0);
        let storage = create_test_storage();
        let run_id = RunId::new();

        let _txn = coordinator.start_transaction(run_id, &storage);
        coordinator.record_commit();

        let metrics = coordinator.metrics();
        assert_eq!(metrics.total_started, 1);
        assert_eq!(metrics.total_committed, 1);
        assert_eq!(metrics.active_count, 0);
        assert_eq!(metrics.commit_rate, 1.0);
    }

    #[test]
    fn test_record_abort_updates_metrics() {
        let coordinator = TransactionCoordinator::new(0);
        let storage = create_test_storage();
        let run_id = RunId::new();

        let _txn = coordinator.start_transaction(run_id, &storage);
        coordinator.record_abort();

        let metrics = coordinator.metrics();
        assert_eq!(metrics.total_started, 1);
        assert_eq!(metrics.total_aborted, 1);
        assert_eq!(metrics.active_count, 0);
        assert_eq!(metrics.commit_rate, 0.0);
    }

    #[test]
    fn test_version_monotonic() {
        let coordinator = TransactionCoordinator::new(100);

        let v1 = coordinator.allocate_commit_version();
        let v2 = coordinator.allocate_commit_version();
        let v3 = coordinator.allocate_commit_version();

        assert!(v1 < v2);
        assert!(v2 < v3);
        assert_eq!(v1, 101);
        assert_eq!(v2, 102);
        assert_eq!(v3, 103);
    }

    #[test]
    fn test_metrics_helpers() {
        let coordinator = TransactionCoordinator::new(0);
        let storage = create_test_storage();
        let run_id = RunId::new();

        // Start 4 transactions
        for _ in 0..4 {
            let _txn = coordinator.start_transaction(run_id, &storage);
        }

        // 3 commit, 1 abort
        coordinator.record_commit();
        coordinator.record_commit();
        coordinator.record_commit();
        coordinator.record_abort();

        let metrics = coordinator.metrics();
        assert_eq!(metrics.total_completed(), 4);
        assert_eq!(metrics.abort_rate(), 0.25);
        assert_eq!(metrics.commit_rate, 0.75);
    }

    #[test]
    fn test_mixed_transactions() {
        let coordinator = TransactionCoordinator::new(0);
        let storage = create_test_storage();
        let run_id = RunId::new();

        // Simulate realistic usage
        let _txn1 = coordinator.start_transaction(run_id, &storage);
        let _txn2 = coordinator.start_transaction(run_id, &storage);

        assert_eq!(coordinator.metrics().active_count, 2);

        coordinator.record_commit(); // txn1 commits

        assert_eq!(coordinator.metrics().active_count, 1);
        assert_eq!(coordinator.metrics().total_committed, 1);

        let _txn3 = coordinator.start_transaction(run_id, &storage);

        assert_eq!(coordinator.metrics().active_count, 2);
        assert_eq!(coordinator.metrics().total_started, 3);

        coordinator.record_abort(); // txn2 aborts
        coordinator.record_commit(); // txn3 commits

        let metrics = coordinator.metrics();
        assert_eq!(metrics.active_count, 0);
        assert_eq!(metrics.total_started, 3);
        assert_eq!(metrics.total_committed, 2);
        assert_eq!(metrics.total_aborted, 1);
    }
}
