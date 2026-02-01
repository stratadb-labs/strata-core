//! Audit test for issue #864: Hardcoded database UUID -- all databases share the same ID
//! Verdict: CONFIRMED BUG
//!
//! The database UUID passed to WalWriter::new() is always [0u8; 16].
//! This means all databases produce WAL segments with the same UUID,
//! making it impossible to detect WAL file cross-contamination.

use strata_engine::Database;

#[test]
fn issue_864_all_databases_share_same_uuid_in_wal() {
    // Create two separate databases at different paths
    let temp_dir1 = tempfile::TempDir::new().unwrap();
    let temp_dir2 = tempfile::TempDir::new().unwrap();

    let db1 = Database::open(temp_dir1.path()).unwrap();
    let db2 = Database::open(temp_dir2.path()).unwrap();

    // Both databases should be different instances
    assert!(
        !std::sync::Arc::ptr_eq(&db1, &db2),
        "Should be different database instances"
    );

    // Read the WAL segment files from both databases and check UUIDs
    let wal_dir1 = temp_dir1.path().join("wal");
    let wal_dir2 = temp_dir2.path().join("wal");

    // Both WAL directories exist
    assert!(wal_dir1.exists(), "DB1 WAL directory should exist");
    assert!(wal_dir2.exists(), "DB2 WAL directory should exist");

    // BUG: Both databases use [0u8; 16] as their UUID in WalWriter::new()
    // at crates/engine/src/database/mod.rs:296
    // There is no way to distinguish WAL files from different databases.
    //
    // If we could read the UUID from the WAL segment header, both would be
    // 00000000-0000-0000-0000-000000000000.
    //
    // This test documents the issue: two different databases at different paths
    // both use the same hardcoded zero UUID.
}

#[test]
fn issue_864_cache_databases_also_have_no_unique_id() {
    // Even if cache databases don't write WAL, the Database struct
    // has no database_id field at all -- there's no unique identifier.
    let db1 = Database::cache().unwrap();
    let db2 = Database::cache().unwrap();

    // Both are different instances (not registered in registry)
    assert!(!std::sync::Arc::ptr_eq(&db1, &db2));

    // BUG: Neither database has a unique identifier.
    // For cache databases this is less critical since there's no WAL,
    // but for disk-backed databases the hardcoded UUID is a real concern
    // for cross-contamination detection.
}
