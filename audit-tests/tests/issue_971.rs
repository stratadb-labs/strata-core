//! Audit test for issue #971: branch/switch triggers WAL write
//!
//! Switching branches (`set_branch`) checks branch existence via a read-only
//! `db.transaction()`. Before issue #970's fix, this produced a spurious
//! WAL write. This test verifies that branch switching is now WAL-free.

use strata_engine::Database;
use strata_executor::Strata;
use tempfile::TempDir;

/// Helper: get current WAL append count from a Strata instance.
fn wal_appends(strata: &Strata) -> u64 {
    strata
        .database()
        .durability_counters()
        .map(|c| c.wal_appends)
        .unwrap_or(0)
}

#[test]
fn branch_switch_produces_no_wal_writes() {
    let dir = TempDir::new().expect("tempdir");
    let db = Database::builder()
        .path(dir.path())
        .always()
        .open()
        .expect("open db");

    let mut strata = Strata::from_database(db).expect("strata");

    // Create a second branch to switch to
    strata.branches().create("other-branch").unwrap();

    let before = wal_appends(&strata);

    // Switch to the other branch (read-only: just checks existence)
    strata.set_branch("other-branch").unwrap();

    let after = wal_appends(&strata);
    assert_eq!(
        after, before,
        "branch switch should produce zero WAL appends, but produced {}",
        after - before
    );
}

#[test]
fn repeated_branch_switches_produce_no_wal_writes() {
    let dir = TempDir::new().expect("tempdir");
    let db = Database::builder()
        .path(dir.path())
        .always()
        .open()
        .expect("open db");

    let mut strata = Strata::from_database(db).expect("strata");

    // Create branches
    strata.branches().create("branch-a").unwrap();
    strata.branches().create("branch-b").unwrap();

    let before = wal_appends(&strata);

    // Switch back and forth 10 times
    for _ in 0..10 {
        strata.set_branch("branch-a").unwrap();
        strata.set_branch("branch-b").unwrap();
    }

    let after = wal_appends(&strata);
    assert_eq!(
        after, before,
        "20 branch switches should produce zero WAL appends, but produced {}",
        after - before
    );
}

#[test]
fn branch_exists_check_produces_no_wal_writes() {
    let dir = TempDir::new().expect("tempdir");
    let db = Database::builder()
        .path(dir.path())
        .always()
        .open()
        .expect("open db");

    let strata = Strata::from_database(db).expect("strata");

    // Create a branch
    strata.branches().create("check-branch").unwrap();

    let before = wal_appends(&strata);

    // Check existence (read-only)
    let exists = strata.branches().exists("check-branch").unwrap();
    assert!(exists);

    let not_exists = strata.branches().exists("nonexistent").unwrap();
    assert!(!not_exists);

    let after = wal_appends(&strata);
    assert_eq!(
        after, before,
        "branch existence checks should produce zero WAL appends, but produced {}",
        after - before
    );
}
