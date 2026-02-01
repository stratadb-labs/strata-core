//! Audit test for issue #846: Bundle export does not capture version history
//! Verdict: CONFIRMED BUG
//!
//! Branch export scans the current state via scan_prefix, which returns only
//! the latest version of each key. All version history is lost. Deletions
//! are also not captured (deletes vec is always empty).

use std::sync::Arc;
use strata_core::value::Value;
use strata_engine::database::Database;
use strata_engine::BranchIndex;
use strata_engine::KVStore;
use tempfile::TempDir;

fn setup_with_branch(branch_name: &str) -> (TempDir, Arc<Database>, BranchIndex) {
    let temp_dir = TempDir::new().unwrap();
    let db = Database::open(temp_dir.path()).unwrap();
    let branch_index = BranchIndex::new(db.clone());
    branch_index.create_branch(branch_name).unwrap();
    (temp_dir, db, branch_index)
}

/// Demonstrates that exporting a branch with version history loses all but
/// the latest version.
#[test]
fn issue_846_export_loses_version_history() {
    let (temp_dir, db, branch_index) = setup_with_branch("history-test");

    // Get the branch metadata to resolve the core BranchId
    let meta = branch_index
        .get_branch("history-test")
        .unwrap()
        .unwrap()
        .value;
    let core_branch_id = strata_engine::primitives::branch::resolve_branch_name(&meta.name);

    // Create a KV store and write multiple versions
    let kv = KVStore::new(db.clone());
    kv.put(&core_branch_id, "versioned_key", Value::Int(1))
        .unwrap();
    kv.put(&core_branch_id, "versioned_key", Value::Int(2))
        .unwrap();
    kv.put(&core_branch_id, "versioned_key", Value::Int(3))
        .unwrap();

    // Verify version history exists (3 versions)
    let history = kv.getv(&core_branch_id, "versioned_key").unwrap();
    assert!(history.is_some(), "Should have version history");
    let history = history.unwrap();
    assert!(
        history.len() >= 3,
        "Should have at least 3 versions, got {}",
        history.len()
    );

    // Also write a key and then delete it
    kv.put(
        &core_branch_id,
        "deleted_key",
        Value::String("will be deleted".into()),
    )
    .unwrap();
    kv.delete(&core_branch_id, "deleted_key").unwrap();

    // Export the branch
    let bundle_path = temp_dir.path().join("test.branchbundle.tar.zst");
    let export_info =
        strata_engine::bundle::export_branch(&db, "history-test", &bundle_path).unwrap();

    // The export captures only the current state, not version history.
    // entry_count reflects the number of BranchlogPayload records (1),
    // not the number of versions.
    eprintln!(
        "Export info: {} entries, {} bytes",
        export_info.entry_count, export_info.bundle_size
    );

    // Import into a fresh database
    let import_dir = TempDir::new().unwrap();
    let import_db = Database::open(import_dir.path()).unwrap();

    let import_info = strata_engine::bundle::import_branch(&import_db, &bundle_path).unwrap();

    // Verify imported data
    let import_branch_index = BranchIndex::new(import_db.clone());
    let import_meta = import_branch_index
        .get_branch("history-test")
        .unwrap()
        .unwrap()
        .value;
    let import_branch_id =
        strata_engine::primitives::branch::resolve_branch_name(&import_meta.name);

    let import_kv = KVStore::new(import_db.clone());

    // Check the versioned key - should have only 1 version (latest)
    let import_history = import_kv.getv(&import_branch_id, "versioned_key").unwrap();

    match import_history {
        Some(h) => {
            if h.len() < 3 {
                // BUG CONFIRMED: Version history was lost during export/import
                eprintln!(
                    "BUG CONFIRMED: Imported key has {} version(s) instead of 3. \
                     Version history was lost during export.",
                    h.len()
                );
            }
            // The latest value should still be Int(3)
            assert_eq!(
                *h.value(),
                Value::Int(3),
                "Latest value should be preserved"
            );
        }
        None => {
            panic!("Key should exist after import");
        }
    }

    // Check the deleted key - it should NOT exist after import
    // BUG: The deletion is not captured (deletes vec is always empty in export)
    let deleted_result = import_kv.get(&import_branch_id, "deleted_key").unwrap();
    // The deleted key should be None (correctly not exported since scan_prefix
    // filters tombstones via the Storage trait). But the delete operation itself
    // is lost -- if we needed to replay the deletion for auditing, it's gone.
    assert!(
        deleted_result.is_none(),
        "Deleted key should not exist after import (tombstone not exported)"
    );

    eprintln!(
        "Import stats: {} transactions applied, {} keys written",
        import_info.transactions_applied, import_info.keys_written
    );
}
