//! WAL writer for transactions
//!
//! Writes transaction entries to WAL for durability.
//! Per spec Section 5:
//! - BeginTxn written at transaction start
//! - Write/Delete entries written during commit
//! - CommitTxn written to finalize (marks transaction durable)
//!
//! ## Usage
//!
//! ```ignore
//! let writer = TransactionWALWriter::new(&mut wal, txn_id, run_id);
//! writer.write_begin()?;
//! // ... write operations via TransactionContext.write_to_wal() ...
//! writer.write_commit()?;
//! ```

use chrono::Utc;
use in_mem_core::error::Result;
use in_mem_core::types::{Key, RunId};
use in_mem_core::value::Value;
use in_mem_durability::wal::{WALEntry, WAL};

/// Writes transaction operations to WAL
///
/// This is a convenience wrapper that handles creating properly formatted
/// WAL entries for a transaction. It tracks the transaction ID and run ID
/// so individual write operations don't need to specify them.
///
/// Per spec Section 5.3:
/// - BeginTxn: Written when transaction starts commit process
/// - Write/Delete: Written for each operation in the transaction
/// - CommitTxn: Written to mark transaction as durable (DURABILITY POINT)
pub struct TransactionWALWriter<'a> {
    wal: &'a mut WAL,
    txn_id: u64,
    run_id: RunId,
}

impl<'a> TransactionWALWriter<'a> {
    /// Create a new WAL writer for a transaction
    ///
    /// # Arguments
    /// * `wal` - WAL to write to
    /// * `txn_id` - Unique transaction ID
    /// * `run_id` - Run ID for this transaction
    pub fn new(wal: &'a mut WAL, txn_id: u64, run_id: RunId) -> Self {
        TransactionWALWriter {
            wal,
            txn_id,
            run_id,
        }
    }

    /// Write BeginTxn entry
    ///
    /// Should be called at the start of the commit process.
    pub fn write_begin(&mut self) -> Result<()> {
        let entry = WALEntry::BeginTxn {
            txn_id: self.txn_id,
            run_id: self.run_id,
            timestamp: Utc::now().timestamp(),
        };
        self.wal.append(&entry)?;
        Ok(())
    }

    /// Write a put operation
    ///
    /// # Arguments
    /// * `key` - Key being written
    /// * `value` - Value being written
    /// * `version` - Commit version for this write
    pub fn write_put(&mut self, key: Key, value: Value, version: u64) -> Result<()> {
        let entry = WALEntry::Write {
            run_id: self.run_id,
            key,
            value,
            version,
        };
        self.wal.append(&entry)?;
        Ok(())
    }

    /// Write a delete operation
    ///
    /// # Arguments
    /// * `key` - Key being deleted
    /// * `version` - Commit version for this delete
    pub fn write_delete(&mut self, key: Key, version: u64) -> Result<()> {
        let entry = WALEntry::Delete {
            run_id: self.run_id,
            key,
            version,
        };
        self.wal.append(&entry)?;
        Ok(())
    }

    /// Write CommitTxn entry (marks transaction as durable)
    ///
    /// Per spec Section 5: This is the DURABILITY POINT.
    /// Once this entry is written and flushed, the transaction is durable.
    /// If crash occurs after this, recovery will replay the transaction.
    pub fn write_commit(&mut self) -> Result<()> {
        let entry = WALEntry::CommitTxn {
            txn_id: self.txn_id,
            run_id: self.run_id,
        };
        self.wal.append(&entry)?;

        // Ensure commit marker is flushed to disk
        self.wal.flush()?;

        Ok(())
    }

    /// Write AbortTxn entry (optional for M2, but supported)
    ///
    /// Per spec Appendix A.3: Aborted transactions don't need WAL entries
    /// because they never applied anything. However, we support this for
    /// explicit abort tracking if needed.
    pub fn write_abort(&mut self) -> Result<()> {
        let entry = WALEntry::AbortTxn {
            txn_id: self.txn_id,
            run_id: self.run_id,
        };
        self.wal.append(&entry)?;
        Ok(())
    }

    /// Get the transaction ID
    pub fn txn_id(&self) -> u64 {
        self.txn_id
    }

    /// Get the run ID
    pub fn run_id(&self) -> RunId {
        self.run_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use in_mem_core::types::Namespace;
    use in_mem_durability::wal::DurabilityMode;
    use tempfile::TempDir;

    fn create_test_wal() -> (WAL, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let wal_path = temp_dir.path().join("test.wal");
        let wal = WAL::open(&wal_path, DurabilityMode::default()).unwrap();
        (wal, temp_dir)
    }

    fn create_test_namespace(run_id: RunId) -> Namespace {
        Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        )
    }

    fn create_test_key(ns: &Namespace, name: &str) -> Key {
        Key::new_kv(ns.clone(), name)
    }

    #[test]
    fn test_write_begin_creates_begin_txn_entry() {
        let (mut wal, _temp) = create_test_wal();
        let run_id = RunId::new();

        let mut writer = TransactionWALWriter::new(&mut wal, 1, run_id);
        writer.write_begin().unwrap();

        let entries = wal.read_all().unwrap();
        assert_eq!(entries.len(), 1);

        if let WALEntry::BeginTxn {
            txn_id,
            run_id: entry_run_id,
            ..
        } = &entries[0]
        {
            assert_eq!(*txn_id, 1);
            assert_eq!(*entry_run_id, run_id);
        } else {
            panic!("Expected BeginTxn entry");
        }
    }

    #[test]
    fn test_write_put_creates_write_entry() {
        let (mut wal, _temp) = create_test_wal();
        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);
        let key = create_test_key(&ns, "test_key");

        let mut writer = TransactionWALWriter::new(&mut wal, 1, run_id);
        writer.write_put(key.clone(), Value::I64(42), 100).unwrap();

        let entries = wal.read_all().unwrap();
        assert_eq!(entries.len(), 1);

        if let WALEntry::Write {
            run_id: entry_run_id,
            key: entry_key,
            value,
            version,
        } = &entries[0]
        {
            assert_eq!(*entry_run_id, run_id);
            assert_eq!(*entry_key, key);
            assert_eq!(*value, Value::I64(42));
            assert_eq!(*version, 100);
        } else {
            panic!("Expected Write entry");
        }
    }

    #[test]
    fn test_write_delete_creates_delete_entry() {
        let (mut wal, _temp) = create_test_wal();
        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);
        let key = create_test_key(&ns, "test_key");

        let mut writer = TransactionWALWriter::new(&mut wal, 1, run_id);
        writer.write_delete(key.clone(), 100).unwrap();

        let entries = wal.read_all().unwrap();
        assert_eq!(entries.len(), 1);

        if let WALEntry::Delete {
            run_id: entry_run_id,
            key: entry_key,
            version,
        } = &entries[0]
        {
            assert_eq!(*entry_run_id, run_id);
            assert_eq!(*entry_key, key);
            assert_eq!(*version, 100);
        } else {
            panic!("Expected Delete entry");
        }
    }

    #[test]
    fn test_write_commit_creates_commit_txn_entry() {
        let (mut wal, _temp) = create_test_wal();
        let run_id = RunId::new();

        let mut writer = TransactionWALWriter::new(&mut wal, 1, run_id);
        writer.write_commit().unwrap();

        let entries = wal.read_all().unwrap();
        assert_eq!(entries.len(), 1);

        if let WALEntry::CommitTxn {
            txn_id,
            run_id: entry_run_id,
        } = &entries[0]
        {
            assert_eq!(*txn_id, 1);
            assert_eq!(*entry_run_id, run_id);
        } else {
            panic!("Expected CommitTxn entry");
        }
    }

    #[test]
    fn test_write_abort_creates_abort_txn_entry() {
        let (mut wal, _temp) = create_test_wal();
        let run_id = RunId::new();

        let mut writer = TransactionWALWriter::new(&mut wal, 1, run_id);
        writer.write_abort().unwrap();

        let entries = wal.read_all().unwrap();
        assert_eq!(entries.len(), 1);

        if let WALEntry::AbortTxn {
            txn_id,
            run_id: entry_run_id,
        } = &entries[0]
        {
            assert_eq!(*txn_id, 1);
            assert_eq!(*entry_run_id, run_id);
        } else {
            panic!("Expected AbortTxn entry");
        }
    }

    #[test]
    fn test_full_transaction_lifecycle() {
        let (mut wal, _temp) = create_test_wal();
        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);
        let key1 = create_test_key(&ns, "key1");
        let key2 = create_test_key(&ns, "key2");

        let mut writer = TransactionWALWriter::new(&mut wal, 42, run_id);

        // Write transaction sequence
        writer.write_begin().unwrap();
        writer.write_put(key1.clone(), Value::I64(1), 100).unwrap();
        writer.write_put(key2.clone(), Value::I64(2), 100).unwrap();
        writer.write_delete(key1.clone(), 100).unwrap();
        writer.write_commit().unwrap();

        let entries = wal.read_all().unwrap();
        assert_eq!(entries.len(), 5);

        // Verify sequence: BeginTxn, Write, Write, Delete, CommitTxn
        assert!(matches!(entries[0], WALEntry::BeginTxn { .. }));
        assert!(matches!(entries[1], WALEntry::Write { .. }));
        assert!(matches!(entries[2], WALEntry::Write { .. }));
        assert!(matches!(entries[3], WALEntry::Delete { .. }));
        assert!(matches!(entries[4], WALEntry::CommitTxn { .. }));

        // All entries should have same run_id
        for entry in &entries {
            assert_eq!(entry.run_id(), Some(run_id));
        }

        // Verify txn_id on boundary entries
        assert_eq!(entries[0].txn_id(), Some(42));
        assert_eq!(entries[4].txn_id(), Some(42));
    }

    #[test]
    fn test_writer_accessors() {
        let (mut wal, _temp) = create_test_wal();
        let run_id = RunId::new();

        let writer = TransactionWALWriter::new(&mut wal, 123, run_id);

        assert_eq!(writer.txn_id(), 123);
        assert_eq!(writer.run_id(), run_id);
    }

    #[test]
    fn test_multiple_transactions() {
        let (mut wal, _temp) = create_test_wal();
        let run_id = RunId::new();
        let ns = create_test_namespace(run_id);
        let key = create_test_key(&ns, "key");

        // Transaction 1
        {
            let mut writer = TransactionWALWriter::new(&mut wal, 1, run_id);
            writer.write_begin().unwrap();
            writer.write_put(key.clone(), Value::I64(10), 100).unwrap();
            writer.write_commit().unwrap();
        }

        // Transaction 2
        {
            let mut writer = TransactionWALWriter::new(&mut wal, 2, run_id);
            writer.write_begin().unwrap();
            writer.write_put(key.clone(), Value::I64(20), 101).unwrap();
            writer.write_commit().unwrap();
        }

        let entries = wal.read_all().unwrap();
        assert_eq!(entries.len(), 6);

        // Verify txn_id progression
        assert_eq!(entries[0].txn_id(), Some(1));
        assert_eq!(entries[2].txn_id(), Some(1));
        assert_eq!(entries[3].txn_id(), Some(2));
        assert_eq!(entries[5].txn_id(), Some(2));
    }
}
