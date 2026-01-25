//! Strict durability mode
//!
//! WAL append + immediate fsync on every commit.
//! Maximum durability - zero data loss on crash.
//! Slowest mode - ~2ms per commit due to fsync latency.
//!
//! # Use Cases
//!
//! - Audit logs and compliance data
//! - Checkpoints and metadata
//! - Any data where loss is unacceptable
//! - backwards compatibility (this was the default)
//!
//! # Performance Contract
//!
//! - `persist()`: WAL append + fsync (~2ms)
//! - Guarantees data is on disk before returning
//! - One fsync per transaction (batching possible in future)

use super::Durability;
use strata_concurrency::{TransactionContext, TransactionWALWriter};
use strata_core::error::Result;
use parking_lot::Mutex;
use strata_durability::wal::WAL;
use std::sync::Arc;

/// Strict durability - fsync on every commit
///
/// This mode provides maximum durability by ensuring every transaction
/// is persisted to disk (via fsync) before commit returns. It matches
/// the strict behavior and is appropriate for data where any loss is
/// unacceptable.
///
/// # Thread Safety
///
/// WAL access is synchronized via parking_lot::Mutex. Multiple threads can call
/// persist() concurrently, but commits will be serialized.
/// Using parking_lot::Mutex to avoid lock poisoning cascade on panic.
///
/// # Example
///
/// ```ignore
/// use strata_engine::durability::{Durability, StrictDurability};
/// use strata_durability::wal::WAL;
/// use parking_lot::Mutex;
/// use std::sync::Arc;
///
/// let wal = Arc::new(Mutex::new(WAL::open("data/wal", DurabilityMode::Strict)?));
/// let durability = StrictDurability::new(wal);
/// assert!(durability.is_persistent());
/// ```
pub struct StrictDurability {
    /// WAL for persisting transactions
    /// Using parking_lot::Mutex to avoid lock poisoning cascade on panic
    wal: Arc<Mutex<WAL>>,
}

impl StrictDurability {
    /// Create new Strict durability mode
    ///
    /// # Arguments
    ///
    /// * `wal` - Write-ahead log instance wrapped in Arc<Mutex>
    pub fn new(wal: Arc<Mutex<WAL>>) -> Self {
        Self { wal }
    }
}

impl Durability for StrictDurability {
    /// Persist transaction to WAL with immediate fsync
    ///
    /// This method:
    /// 1. Acquires WAL lock
    /// 2. Writes BeginTxn entry
    /// 3. Writes all put/delete/CAS operations
    /// 4. Writes CommitTxn entry
    /// 5. Calls fsync() to ensure data is on disk
    ///
    /// # Performance
    ///
    /// Expect ~2ms latency per commit due to fsync. This is the
    /// tradeoff for zero data loss guarantee.
    ///
    /// # Errors
    ///
    /// Returns error if WAL write or fsync fails.
    fn persist(&self, txn: &TransactionContext, commit_version: u64) -> Result<()> {
        // Acquire WAL lock (parking_lot::Mutex doesn't poison)
        let mut wal = self.wal.lock();

        // Write transaction to WAL in a scoped block
        // This ensures wal_writer's mutable borrow ends before fsync
        {
            let mut wal_writer = TransactionWALWriter::new(&mut wal, txn.txn_id, txn.run_id);

            // Write BeginTxn
            wal_writer.write_begin()?;

            // Write all puts from write_set
            for (key, value) in &txn.write_set {
                wal_writer.write_put(key.clone(), value.clone(), commit_version)?;
            }

            // Write all deletes from delete_set
            for key in &txn.delete_set {
                wal_writer.write_delete(key.clone(), commit_version)?;
            }

            // Write CAS operations (they become puts after validation)
            for cas_op in &txn.cas_set {
                wal_writer.write_put(
                    cas_op.key.clone(),
                    cas_op.new_value.clone(),
                    commit_version,
                )?;
            }

            // Write CommitTxn (this flushes the buffer)
            wal_writer.write_commit()?;
        }

        // fsync - this is the slow part (~2ms)
        // This ensures the data is actually on disk
        wal.fsync()?;

        Ok(())
    }

    /// Ensure WAL is synced before shutdown
    ///
    /// For Strict mode, this is mostly a no-op since every commit
    /// already fsyncs. We still call fsync one more time to ensure
    /// any buffered filesystem data is persisted.
    fn shutdown(&self) -> Result<()> {
        // parking_lot::Mutex doesn't poison, so no error handling needed
        let wal = self.wal.lock();
        // Final fsync to ensure any buffered data is persisted
        wal.fsync()
    }

    /// Strict mode persists all data
    #[inline]
    fn is_persistent(&self) -> bool {
        true
    }

    /// Returns "Strict"
    #[inline]
    fn mode_name(&self) -> &'static str {
        "Strict"
    }
}

// Manual Debug impl since Mutex doesn't derive Debug nicely
impl std::fmt::Debug for StrictDurability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StrictDurability")
            .field("wal", &"Arc<parking_lot::Mutex<WAL>>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strata_durability::wal::DurabilityMode;
    use tempfile::TempDir;

    fn create_test_wal() -> (TempDir, Arc<Mutex<WAL>>) {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("test.wal");
        let wal = WAL::open(&wal_path, DurabilityMode::Strict).unwrap();
        (temp_dir, Arc::new(Mutex::new(wal)))
    }

    #[test]
    fn test_strict_is_persistent() {
        let (_temp, wal) = create_test_wal();
        let durability = StrictDurability::new(wal);
        assert!(durability.is_persistent());
    }

    #[test]
    fn test_strict_requires_wal() {
        let (_temp, wal) = create_test_wal();
        let durability = StrictDurability::new(wal);
        assert!(durability.requires_wal());
    }

    #[test]
    fn test_strict_mode_name() {
        let (_temp, wal) = create_test_wal();
        let durability = StrictDurability::new(wal);
        assert_eq!(durability.mode_name(), "Strict");
    }

    #[test]
    fn test_strict_shutdown_succeeds() {
        let (_temp, wal) = create_test_wal();
        let durability = StrictDurability::new(wal);
        assert!(durability.shutdown().is_ok());
    }

    #[test]
    fn test_strict_debug() {
        let (_temp, wal) = create_test_wal();
        let durability = StrictDurability::new(wal);
        let debug_str = format!("{:?}", durability);
        assert!(debug_str.contains("StrictDurability"));
    }
}
