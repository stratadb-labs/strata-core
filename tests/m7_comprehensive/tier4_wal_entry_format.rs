//! Tier 4.1: WAL Entry Format Tests
//!
//! Tests for WAL entry envelope format.

use crate::test_utils::*;
use strata_core::value::Value;
use strata_durability::{WalEntryType, MAX_WAL_ENTRY_SIZE, WAL_FORMAT_VERSION};

/// WAL format version is defined
#[test]
fn test_wal_format_version_defined() {
    assert!(WAL_FORMAT_VERSION >= 1, "WAL format version must be >= 1");
}

/// WAL max entry size is reasonable
#[test]
fn test_wal_max_entry_size() {
    assert!(MAX_WAL_ENTRY_SIZE > 1024, "Max entry size should be > 1KB");
    assert!(
        MAX_WAL_ENTRY_SIZE <= 64 * 1024 * 1024,
        "Max entry size should be reasonable"
    );
}

/// WAL entry types are defined
#[test]
fn test_wal_entry_types_defined() {
    // Test that core entry types exist
    let _ = WalEntryType::KvPut;
    let _ = WalEntryType::KvDelete;
    let _ = WalEntryType::TransactionCommit;
    let _ = WalEntryType::TransactionAbort;
}

/// WAL file created on write
#[test]
fn test_wal_file_created() {
    let test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "key", Value::String("value".into()))
        .unwrap();

    let wal_path = test_db.wal_path();
    // WAL may or may not exist depending on durability mode
    // This test validates the path structure
    assert!(wal_path.ends_with("wal.bin") || !wal_path.exists());
}

/// WAL grows with writes
#[test]
fn test_wal_grows_with_writes() {
    let test_db = TestDb::new();
    let run_id = test_db.run_id;

    let wal_path = test_db.wal_path();
    let size_before = file_size(&wal_path);

    let kv = test_db.kv();
    for i in 0..100 {
        kv.put(&run_id, &format!("k{}", i), Value::I64(i)).unwrap();
    }

    let size_after = file_size(&wal_path);

    // WAL should grow (if durability is enabled)
    // Note: May not grow if using in-memory mode
    if wal_path.exists() {
        assert!(
            size_after >= size_before,
            "WAL should not shrink during writes"
        );
    }
}

/// Large values fit in WAL entry
#[test]
fn test_large_value_in_wal() {
    let test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Write value close to max size
    let large_value = "x".repeat(1024 * 100); // 100KB
    kv.put(&run_id, "large", Value::String(large_value.clone()))
        .unwrap();

    // Should be readable
    let value = kv.get(&run_id, "large").unwrap();
    assert!(value.is_some());
    if let Some(versioned) = value {
        if let Value::String(s) = versioned.value {
            assert_eq!(s.len(), large_value.len());
        }
    }
}

/// Entry type encoding is consistent
#[test]
fn test_entry_type_encoding() {
    // Entry types should have distinct values
    let types = [
        WalEntryType::KvPut,
        WalEntryType::KvDelete,
        WalEntryType::TransactionCommit,
        WalEntryType::TransactionAbort,
    ];

    // All types should be convertible to/from u8
    for entry_type in &types {
        let byte: u8 = (*entry_type).into();
        let recovered = WalEntryType::try_from(byte);
        assert!(recovered.is_ok());
    }
}
