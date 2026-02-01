//! Database Lifecycle Tests
//!
//! Tests for database creation, opening, closing, and reopening.

use crate::common::*;

// ============================================================================
// Cache Database
// ============================================================================

#[test]
fn cache_database_is_functional() {
    let db = Database::cache().expect("cache database");

    let branch_id = BranchId::new();
    let kv = KVStore::new(db);

    kv.put(&branch_id, "key", Value::Int(42)).unwrap();
    let result = kv.get(&branch_id, "key").unwrap();

    assert!(result.is_some());
    assert_eq!(result.unwrap(), Value::Int(42));
}

#[test]
fn cache_database_data_is_lost_on_drop() {
    let branch_id = BranchId::new();
    let key = unique_key();

    // Write data
    {
        let db = Database::cache().expect("cache database");
        let kv = KVStore::new(db);
        kv.put(&branch_id, &key, Value::Int(42)).unwrap();
    }

    // New cache database has no data
    let db = Database::cache().expect("cache database");
    let kv = KVStore::new(db);
    let result = kv.get(&branch_id, &key).unwrap();

    assert!(result.is_none());
}

// ============================================================================
// Persistent Database
// ============================================================================

#[test]
fn persistent_database_creates_directory() {
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("test_db");

    assert!(!db_path.exists());

    let _db = Database::builder()
        .path(&db_path)
        .standard()
        .open()
        .expect("create database");

    assert!(db_path.exists());
}

#[test]
fn persistent_database_survives_reopen() {
    let mut test_db = TestDb::new();
    let branch_id = test_db.branch_id;
    let key = unique_key();

    // Write data
    {
        let kv = test_db.kv();
        kv.put(&branch_id, &key, Value::Int(42)).unwrap();
    }

    // Force sync and reopen
    test_db.db.shutdown().unwrap();
    test_db.reopen();

    // Verify data persisted
    let kv = test_db.kv();
    let result = kv.get(&branch_id, &key).unwrap();

    assert!(result.is_some());
    assert_eq!(result.unwrap(), Value::Int(42));
}

#[test]
fn persistent_database_multiple_reopens() {
    let mut test_db = TestDb::new();
    let branch_id = test_db.branch_id;

    // Write and reopen multiple times
    for i in 0..5 {
        {
            let kv = test_db.kv();
            kv.put(&branch_id, &format!("key_{}", i), Value::Int(i))
                .unwrap();
        }
        test_db.db.shutdown().unwrap();
        test_db.reopen();
    }

    // Verify all data present
    let kv = test_db.kv();
    for i in 0..5 {
        let result = kv.get(&branch_id, &format!("key_{}", i)).unwrap();
        assert!(result.is_some(), "key_{} should exist", i);
        assert_eq!(result.unwrap(), Value::Int(i));
    }
}

// ============================================================================
// Builder API
// ============================================================================

#[test]
fn builder_cache_durability_with_temp_path_uses_temp_files() {
    // builder with a temp path and cache durability still creates files on disk
    // It's NOT truly cache
    let temp_dir = tempfile::tempdir().unwrap();
    let db = Database::builder()
        .path(temp_dir.path())
        .cache()
        .open()
        .unwrap();

    assert!(!db.is_ephemeral()); // Uses temp files, not purely in-memory
}

#[test]
fn database_cache_is_truly_cache() {
    // Database::cache() creates a purely in-memory database
    let db = Database::cache().expect("cache database");
    assert!(db.is_ephemeral());
}

#[test]
fn builder_creates_persistent_with_path() {
    let temp_dir = tempfile::tempdir().unwrap();

    let db = Database::builder()
        .path(temp_dir.path())
        .standard()
        .open()
        .unwrap();

    assert!(!db.is_ephemeral());
}

#[test]
fn builder_always_mode() {
    let temp_dir = tempfile::tempdir().unwrap();

    let db = Database::builder()
        .path(temp_dir.path())
        .always()
        .open()
        .unwrap();

    // Verify it works
    let branch_id = BranchId::new();
    let kv = KVStore::new(db);
    kv.put(&branch_id, "key", Value::Int(1)).unwrap();
    assert_eq!(kv.get(&branch_id, "key").unwrap(), Some(Value::Int(1)));
}

#[test]
fn builder_standard_mode() {
    let temp_dir = tempfile::tempdir().unwrap();

    let db = Database::builder()
        .path(temp_dir.path())
        .standard()
        .open()
        .unwrap();

    // Verify it works
    let branch_id = BranchId::new();
    let kv = KVStore::new(db);
    kv.put(&branch_id, "key", Value::Int(1)).unwrap();
    assert_eq!(kv.get(&branch_id, "key").unwrap(), Some(Value::Int(1)));
}

// ============================================================================
// Shutdown
// ============================================================================

#[test]
fn shutdown_is_idempotent() {
    let test_db = TestDb::new();

    // Multiple shutdowns should not panic
    test_db.db.shutdown().unwrap();
    // Second shutdown - should be safe
    let _ = test_db.db.shutdown();
}

#[test]
fn is_open_reflects_state() {
    let temp_dir = tempfile::tempdir().unwrap();

    let db = Database::builder()
        .path(temp_dir.path())
        .standard()
        .open()
        .unwrap();

    assert!(db.is_open());

    db.shutdown().unwrap();

    assert!(!db.is_open());
}
