//! Run command handlers (MVP)
//!
//! This module implements handlers for MVP Run commands by dispatching
//! directly to engine primitives via `bridge::Primitives`.

use std::sync::Arc;

use strata_engine::BranchMetadata;

use crate::bridge::{extract_version, from_engine_branch_status, Primitives};
use crate::convert::convert_result;
use crate::types::{BranchId, BranchInfo, VersionedBranchInfo};
use crate::{Error, Output, Result};

// =============================================================================
// Conversion Helpers
// =============================================================================

/// Convert engine BranchMetadata to executor BranchInfo.
fn metadata_to_branch_info(m: &BranchMetadata) -> BranchInfo {
    BranchInfo {
        id: BranchId::from(m.name.clone()),
        status: from_engine_branch_status(m.status),
        created_at: m.created_at,
        updated_at: m.updated_at,
        metadata: None, // MVP: metadata not exposed
        parent_id: None, // MVP: parent-child not exposed
        tags: vec![],   // MVP: tags not exposed
    }
}

/// Convert engine Versioned<BranchMetadata> to executor VersionedBranchInfo.
fn versioned_to_branch_info(v: strata_core::Versioned<BranchMetadata>) -> VersionedBranchInfo {
    let info = metadata_to_branch_info(&v.value);
    VersionedBranchInfo {
        info,
        version: extract_version(&v.version),
        timestamp: v.timestamp.into(),
    }
}

/// Guard: reject operations on the default run that would delete it.
fn reject_default_branch(run: &BranchId, operation: &str) -> Result<()> {
    if run.is_default() {
        return Err(Error::ConstraintViolation {
            reason: format!("Cannot {} the default run", operation),
        });
    }
    Ok(())
}

// =============================================================================
// MVP Handlers
// =============================================================================

/// Handle BranchCreate command.
pub fn branch_create(
    p: &Arc<Primitives>,
    branch_id: Option<String>,
    _metadata: Option<strata_core::Value>,
) -> Result<Output> {
    // Users can provide any string as a run name (like git branch names).
    // If not provided, generate a UUID for anonymous runs.
    let run_str = match &branch_id {
        Some(s) => s.clone(),
        None => uuid::Uuid::new_v4().to_string(),
    };

    // MVP: ignore metadata, use simple create_branch
    let versioned = convert_result(p.branch.create_branch(&run_str))?;

    Ok(Output::BranchWithVersion {
        info: metadata_to_branch_info(&versioned.value),
        version: extract_version(&versioned.version),
    })
}

/// Handle BranchGet command.
pub fn branch_get(p: &Arc<Primitives>, run: BranchId) -> Result<Output> {
    let result = convert_result(p.branch.get_branch(run.as_str()))?;
    match result {
        Some(v) => Ok(Output::BranchInfoVersioned(versioned_to_branch_info(v))),
        None => Ok(Output::Maybe(None)),
    }
}

/// Handle BranchList command.
pub fn branch_list(
    p: &Arc<Primitives>,
    _state: Option<crate::types::BranchStatus>,
    limit: Option<u64>,
    _offset: Option<u64>,
) -> Result<Output> {
    // MVP: ignore status filter, list all runs
    let ids = convert_result(p.branch.list_branches())?;

    let mut all = Vec::new();
    for id in ids {
        if let Some(versioned) = convert_result(p.branch.get_branch(&id))? {
            all.push(versioned_to_branch_info(versioned));
        }
    }

    // Apply limit if specified
    let limited: Vec<VersionedBranchInfo> = match limit {
        Some(l) => all.into_iter().take(l as usize).collect(),
        None => all,
    };

    Ok(Output::BranchInfoList(limited))
}

/// Handle BranchExists command.
pub fn branch_exists(p: &Arc<Primitives>, run: BranchId) -> Result<Output> {
    let exists = convert_result(p.branch.exists(run.as_str()))?;
    Ok(Output::Bool(exists))
}

/// Handle BranchDelete command.
pub fn branch_delete(p: &Arc<Primitives>, run: BranchId) -> Result<Output> {
    reject_default_branch(&run, "delete")?;
    convert_result(p.branch.delete_branch(run.as_str()))?;
    Ok(Output::Unit)
}

// =============================================================================
// Bundle Handlers
// =============================================================================

/// Handle BranchExport command.
pub fn branch_export(p: &Arc<Primitives>, branch_id: String, path: String) -> Result<Output> {
    let export_path = std::path::Path::new(&path);
    let info = strata_engine::bundle::export_run(&p.db, &branch_id, export_path).map_err(|e| {
        Error::Io {
            reason: format!("Export failed: {}", e),
        }
    })?;

    Ok(Output::BranchExported(crate::types::BranchExportResult {
        branch_id: info.branch_id,
        path: info.path.to_string_lossy().to_string(),
        entry_count: info.entry_count,
        bundle_size: info.bundle_size,
    }))
}

/// Handle BranchImport command.
pub fn branch_import(p: &Arc<Primitives>, path: String) -> Result<Output> {
    let import_path = std::path::Path::new(&path);
    let info = strata_engine::bundle::import_run(&p.db, import_path).map_err(|e| {
        Error::Io {
            reason: format!("Import failed: {}", e),
        }
    })?;

    Ok(Output::BranchImported(crate::types::BranchImportResult {
        branch_id: info.branch_id,
        transactions_applied: info.transactions_applied,
        keys_written: info.keys_written,
    }))
}

/// Handle BranchBundleValidate command.
pub fn branch_bundle_validate(path: String) -> Result<Output> {
    let validate_path = std::path::Path::new(&path);
    let info = strata_engine::bundle::validate_bundle(validate_path).map_err(|e| {
        Error::Io {
            reason: format!("Validation failed: {}", e),
        }
    })?;

    Ok(Output::BundleValidated(crate::types::BundleValidateResult {
        branch_id: info.branch_id,
        format_version: info.format_version,
        entry_count: info.entry_count,
        checksums_valid: info.checksums_valid,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reject_default_run() {
        let run = BranchId::from("default");
        assert!(reject_default_branch(&run, "delete").is_err());

        let run = BranchId::from("f47ac10b-58cc-4372-a567-0e02b2c3d479");
        assert!(reject_default_branch(&run, "delete").is_ok());
    }

    #[test]
    fn test_metadata_to_run_info() {
        let m = BranchMetadata {
            name: "test-run".to_string(),
            branch_id: "some-uuid".to_string(),
            parent_run: None,
            status: strata_engine::BranchStatus::Active,
            created_at: 1000000,
            updated_at: 2000000,
            completed_at: None,
            tags: vec![],
            metadata: strata_core::Value::Null,
            error: None,
            version: 1,
        };
        let info = metadata_to_branch_info(&m);
        assert_eq!(info.id.as_str(), "test-run");
        assert_eq!(info.status, crate::types::BranchStatus::Active);
    }
}
