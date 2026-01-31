//! Engine-level run export/import API
//!
//! This module provides high-level functions for exporting and importing
//! runs as `.runbundle.tar.zst` archives. It bridges the engine's
//! `Database`/`BranchIndex` types with the durability crate's RunBundle format.
//!
//! ## Export
//!
//! Exports scan the KV store for all keys in a run's namespace and
//! reconstruct `RunlogPayload` records grouped by version.
//!
//! ## Import
//!
//! Imports replay each `RunlogPayload` as a transaction, writing puts
//! and deletes into the target database.

use crate::database::Database;
use crate::BranchIndex;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use strata_core::types::{Key, Namespace, BranchId, TypeTag};
use strata_core::StrataError;
use strata_core::StrataResult;
use strata_durability::branch_bundle::{
    BundleRunInfo, ExportOptions, RunBundleReader, RunBundleWriter,
    RunlogPayload,
};

// =============================================================================
// Public result types
// =============================================================================

/// Information returned after exporting a run
#[derive(Debug, Clone)]
pub struct ExportInfo {
    /// Run ID of the exported run
    pub branch_id: String,
    /// Path where the bundle was written
    pub path: PathBuf,
    /// Number of transaction payloads in the bundle
    pub entry_count: u64,
    /// Size of the bundle file in bytes
    pub bundle_size: u64,
}

/// Information returned after importing a run
#[derive(Debug, Clone)]
pub struct ImportInfo {
    /// Run ID of the imported run
    pub branch_id: String,
    /// Number of transactions applied
    pub transactions_applied: u64,
    /// Total number of keys written
    pub keys_written: u64,
}

/// Information about a bundle (from validation)
#[derive(Debug, Clone)]
pub struct BundleInfo {
    /// Run ID from the bundle
    pub branch_id: String,
    /// Format version of the bundle
    pub format_version: u32,
    /// Number of transaction payloads
    pub entry_count: u64,
    /// Whether all checksums are valid
    pub checksums_valid: bool,
}

// =============================================================================
// Export
// =============================================================================

/// Export a run to a `.runbundle.tar.zst` archive
///
/// The run must exist in the database. All data in the run's namespace
/// is scanned and grouped into per-version `RunlogPayload` records.
///
/// # Errors
///
/// - Run does not exist
/// - I/O errors writing the archive
pub fn export_run(db: &Arc<Database>, branch_id: &str, path: &Path) -> StrataResult<ExportInfo> {
    export_run_with_options(db, branch_id, path, &ExportOptions::default())
}

/// Export a run with custom options (e.g., compression level)
pub fn export_run_with_options(
    db: &Arc<Database>,
    branch_id: &str,
    path: &Path,
    options: &ExportOptions,
) -> StrataResult<ExportInfo> {
    let run_index = BranchIndex::new(db.clone());

    // 1. Verify run exists and get metadata
    let run_meta = run_index
        .get_branch(branch_id)?
        .ok_or_else(|| StrataError::invalid_input(format!("Run '{}' not found", branch_id)))?
        .value;

    // 2. Build BundleRunInfo from metadata
    let bundle_run_info = BundleRunInfo {
        branch_id: run_meta.branch_id.clone(),
        name: run_meta.name.clone(),
        state: run_meta.status.as_str().to_lowercase(),
        created_at: format_micros(run_meta.created_at),
        closed_at: format_micros(run_meta.completed_at.unwrap_or(run_meta.updated_at)),
        parent_run_id: run_meta.parent_run.clone(),
        error: run_meta.error.clone(),
    };

    // 3. Scan storage for all run data -> Vec<RunlogPayload>
    let core_run_id = BranchId::from_string(&run_meta.name)
        .or_else(|| BranchId::from_string(&run_meta.branch_id))
        .ok_or_else(|| {
            StrataError::invalid_input(format!(
                "Cannot resolve BranchId for run '{}' (branch_id='{}')",
                run_meta.name, run_meta.branch_id
            ))
        })?;

    let payloads = scan_run_data(db, core_run_id, branch_id)?;

    // 4. Write bundle
    let writer = RunBundleWriter::new(options);
    let export_info = writer
        .write(&bundle_run_info, &payloads, path)
        .map_err(|e| StrataError::storage(format!("Failed to write bundle: {}", e)))?;

    Ok(ExportInfo {
        branch_id: branch_id.to_string(),
        path: export_info.path,
        entry_count: export_info.wal_entry_count,
        bundle_size: export_info.bundle_size_bytes,
    })
}

/// Scan all data in a run's namespace and group into RunlogPayload records
fn scan_run_data(
    db: &Arc<Database>,
    core_run_id: BranchId,
    run_id_str: &str,
) -> StrataResult<Vec<RunlogPayload>> {
    let ns = Namespace::for_branch(core_run_id);
    let mut all_entries: Vec<(Key, strata_core::value::Value)> = Vec::new();

    // Scan all type tags
    let type_tags = [
        TypeTag::KV,
        TypeTag::Event,
        TypeTag::State,
        TypeTag::Json,
        TypeTag::Vector,
    ];

    for type_tag in type_tags {
        let entries = db.transaction(core_run_id, |txn| {
            let prefix = Key::new(ns.clone(), type_tag, vec![]);
            txn.scan_prefix(&prefix)
        })?;
        all_entries.extend(entries);
    }

    // Group all entries into a single RunlogPayload
    // Since we're scanning the current state (not WAL), all entries are at the
    // same logical version. We create one payload per batch.
    if all_entries.is_empty() {
        return Ok(vec![]);
    }

    // Create a single payload with all current state
    Ok(vec![RunlogPayload {
        branch_id: run_id_str.to_string(),
        version: 1,
        puts: all_entries,
        deletes: vec![],
    }])
}

// =============================================================================
// Import
// =============================================================================

/// Import a run from a `.runbundle.tar.zst` archive
///
/// Creates the run in the database and replays all transaction payloads.
///
/// # Errors
///
/// - Bundle is invalid or corrupt
/// - Run with same ID already exists
/// - I/O errors reading the archive
pub fn import_run(db: &Arc<Database>, path: &Path) -> StrataResult<ImportInfo> {
    // 1. Read and validate bundle
    let contents = RunBundleReader::read_all(path)
        .map_err(|e| StrataError::storage(format!("Failed to read bundle: {}", e)))?;

    let run_id_str = &contents.run_info.name;
    let run_index = BranchIndex::new(db.clone());

    // 2. Check run doesn't already exist
    if run_index.exists(run_id_str)? {
        return Err(StrataError::invalid_input(format!(
            "Run '{}' already exists. Delete it first or use a different name.",
            run_id_str
        )));
    }

    // 3. Create run via BranchIndex
    run_index.create_branch(run_id_str)?;

    // 4. Resolve BranchId for namespace
    let run_meta = run_index
        .get_branch(run_id_str)?
        .ok_or_else(|| {
            StrataError::internal(format!(
                "Run '{}' was just created but cannot be found",
                run_id_str
            ))
        })?
        .value;

    let core_run_id = BranchId::from_string(&run_meta.name)
        .or_else(|| BranchId::from_string(&run_meta.branch_id))
        .ok_or_else(|| {
            StrataError::internal(format!(
                "Cannot resolve BranchId for imported run '{}'",
                run_id_str
            ))
        })?;

    // 5. Replay each payload as a transaction
    let mut transactions_applied = 0u64;
    let mut keys_written = 0u64;

    for payload in &contents.payloads {
        let put_count = payload.puts.len() as u64;

        db.transaction(core_run_id, |txn| {
            // Apply puts
            for (key, value) in &payload.puts {
                txn.put(key.clone(), value.clone())?;
            }

            // Apply deletes
            for key in &payload.deletes {
                txn.delete(key.clone())?;
            }

            Ok(())
        })?;

        transactions_applied += 1;
        keys_written += put_count;
    }

    Ok(ImportInfo {
        branch_id: run_id_str.to_string(),
        transactions_applied,
        keys_written,
    })
}

// =============================================================================
// Validate
// =============================================================================

/// Validate a bundle without importing it
///
/// Checks the archive structure, checksums, and format version.
pub fn validate_bundle(path: &Path) -> StrataResult<BundleInfo> {
    let verify = RunBundleReader::validate(path)
        .map_err(|e| StrataError::storage(format!("Bundle validation failed: {}", e)))?;

    Ok(BundleInfo {
        branch_id: verify.branch_id,
        format_version: verify.format_version,
        entry_count: verify.wal_entry_count,
        checksums_valid: verify.checksums_valid,
    })
}

// =============================================================================
// Helpers
// =============================================================================

/// Format microsecond timestamp as ISO 8601 string
fn format_micros(micros: u64) -> String {
    let secs = micros / 1_000_000;
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    let years = 1970 + (days / 365);
    let day_of_year = days % 365;
    let month = (day_of_year / 30).min(11) + 1;
    let day = (day_of_year % 30) + 1;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        years, month, day, hours, minutes, seconds
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Arc<Database>) {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path()).unwrap();
        (temp_dir, db)
    }

    fn setup_with_branch(run_name: &str) -> (TempDir, Arc<Database>) {
        let (temp_dir, db) = setup();
        let run_index = BranchIndex::new(db.clone());
        run_index.create_branch(run_name).unwrap();
        (temp_dir, db)
    }

    #[test]
    fn test_export_run_exists() {
        let (temp_dir, db) = setup_with_branch("test-run");
        let path = temp_dir.path().join("test.runbundle.tar.zst");

        let result = export_run(&db, "test-run", &path);
        assert!(result.is_ok());

        let info = result.unwrap();
        assert_eq!(info.branch_id, "test-run");
        assert!(info.path.exists());
    }

    #[test]
    fn test_export_run_not_found() {
        let (temp_dir, db) = setup();
        let path = temp_dir.path().join("test.runbundle.tar.zst");

        let result = export_run(&db, "nonexistent", &path);
        assert!(result.is_err());
    }

    #[test]
    fn test_export_with_data() {
        let (temp_dir, db) = setup_with_branch("data-run");

        // Write some data to the run
        let run_index = BranchIndex::new(db.clone());
        let meta = run_index.get_branch("data-run").unwrap().unwrap().value;
        let core_run_id = BranchId::from_string(&meta.name)
            .or_else(|| BranchId::from_string(&meta.branch_id))
            .unwrap();
        let ns = Namespace::for_branch(core_run_id);

        db.transaction(core_run_id, |txn| {
            txn.put(
                Key::new(ns.clone(), TypeTag::KV, b"key1".to_vec()),
                strata_core::value::Value::String("value1".to_string()),
            )?;
            txn.put(
                Key::new(ns.clone(), TypeTag::KV, b"key2".to_vec()),
                strata_core::value::Value::Int(42),
            )?;
            Ok(())
        })
        .unwrap();

        let path = temp_dir.path().join("data.runbundle.tar.zst");
        let info = export_run(&db, "data-run", &path).unwrap();

        assert_eq!(info.branch_id, "data-run");
        assert!(info.entry_count > 0);
        assert!(info.bundle_size > 0);
    }

    #[test]
    fn test_validate_bundle() {
        let (temp_dir, db) = setup_with_branch("validate-run");
        let path = temp_dir.path().join("validate.runbundle.tar.zst");

        export_run(&db, "validate-run", &path).unwrap();

        let info = validate_bundle(&path).unwrap();
        assert!(!info.branch_id.is_empty());
        assert!(info.checksums_valid);
    }

    #[test]
    fn test_validate_nonexistent_bundle() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().join("nonexistent.runbundle.tar.zst");

        let result = validate_bundle(&path);
        assert!(result.is_err());
    }

    #[test]
    fn test_import_branch() {
        let (temp_dir, db) = setup_with_branch("export-run");

        // Write data
        let run_index = BranchIndex::new(db.clone());
        let meta = run_index.get_branch("export-run").unwrap().unwrap().value;
        let core_run_id = BranchId::from_string(&meta.name)
            .or_else(|| BranchId::from_string(&meta.branch_id))
            .unwrap();
        let ns = Namespace::for_branch(core_run_id);

        db.transaction(core_run_id, |txn| {
            txn.put(
                Key::new(ns.clone(), TypeTag::KV, b"key1".to_vec()),
                strata_core::value::Value::String("hello".to_string()),
            )?;
            Ok(())
        })
        .unwrap();

        // Export
        let bundle_path = temp_dir.path().join("export.runbundle.tar.zst");
        export_run(&db, "export-run", &bundle_path).unwrap();

        // Import into a fresh database
        let import_dir = TempDir::new().unwrap();
        let import_db = Database::open(import_dir.path()).unwrap();

        let import_info = import_run(&import_db, &bundle_path).unwrap();
        assert_eq!(import_info.branch_id, "export-run");
        assert!(import_info.transactions_applied > 0);
    }

    #[test]
    fn test_import_duplicate_run_fails() {
        let (temp_dir, db) = setup_with_branch("dup-run");

        let path = temp_dir.path().join("dup.runbundle.tar.zst");
        export_run(&db, "dup-run", &path).unwrap();

        // Importing into same db should fail (run already exists)
        let result = import_run(&db, &path);
        assert!(result.is_err());
    }

    #[test]
    fn test_export_empty_branch() {
        let (temp_dir, db) = setup_with_branch("empty-run");
        let path = temp_dir.path().join("empty.runbundle.tar.zst");

        let info = export_run(&db, "empty-run", &path).unwrap();
        assert_eq!(info.entry_count, 0);
    }

    #[test]
    fn test_format_micros() {
        // Epoch should be 1970
        assert!(format_micros(0).starts_with("1970-"));

        // Some timestamp in 2025
        let ts = 1706_000_000_000_000u64; // approx Jan 2024
        let formatted = format_micros(ts);
        assert!(!formatted.is_empty());
        assert!(formatted.ends_with('Z'));
    }
}
