//! Run command handlers (MVP)
//!
//! This module implements handlers for MVP Run commands by dispatching
//! directly to engine primitives via `bridge::Primitives`.

use std::sync::Arc;

use strata_engine::RunMetadata;

use crate::bridge::{extract_version, from_engine_run_status, Primitives};
use crate::convert::convert_result;
use crate::types::{RunId, RunInfo, VersionedRunInfo};
use crate::{Error, Output, Result};

// =============================================================================
// Conversion Helpers
// =============================================================================

/// Convert engine RunMetadata to executor RunInfo.
fn metadata_to_run_info(m: &RunMetadata) -> RunInfo {
    RunInfo {
        id: RunId::from(m.name.clone()),
        status: from_engine_run_status(m.status),
        created_at: m.created_at,
        updated_at: m.updated_at,
        metadata: None, // MVP: metadata not exposed
        parent_id: None, // MVP: parent-child not exposed
        tags: vec![],   // MVP: tags not exposed
    }
}

/// Convert engine Versioned<RunMetadata> to executor VersionedRunInfo.
fn versioned_to_run_info(v: strata_core::Versioned<RunMetadata>) -> VersionedRunInfo {
    let info = metadata_to_run_info(&v.value);
    VersionedRunInfo {
        info,
        version: extract_version(&v.version),
        timestamp: v.timestamp.into(),
    }
}

/// Guard: reject operations on the default run that would delete it.
fn reject_default_run(run: &RunId, operation: &str) -> Result<()> {
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

/// Handle RunCreate command.
pub fn run_create(
    p: &Arc<Primitives>,
    run_id: Option<String>,
    _metadata: Option<strata_core::Value>,
) -> Result<Output> {
    // Users can provide any string as a run name (like git branch names).
    // If not provided, generate a UUID for anonymous runs.
    let run_str = match &run_id {
        Some(s) => s.clone(),
        None => uuid::Uuid::new_v4().to_string(),
    };

    // MVP: ignore metadata, use simple create_run
    let versioned = convert_result(p.run.create_run(&run_str))?;

    Ok(Output::RunWithVersion {
        info: metadata_to_run_info(&versioned.value),
        version: extract_version(&versioned.version),
    })
}

/// Handle RunGet command.
pub fn run_get(p: &Arc<Primitives>, run: RunId) -> Result<Output> {
    let result = convert_result(p.run.get_run(run.as_str()))?;
    match result {
        Some(v) => Ok(Output::RunInfoVersioned(versioned_to_run_info(v))),
        None => Ok(Output::Maybe(None)),
    }
}

/// Handle RunList command.
pub fn run_list(
    p: &Arc<Primitives>,
    _state: Option<crate::types::RunStatus>,
    limit: Option<u64>,
    _offset: Option<u64>,
) -> Result<Output> {
    // MVP: ignore status filter, list all runs
    let ids = convert_result(p.run.list_runs())?;

    let mut all = Vec::new();
    for id in ids {
        if let Some(versioned) = convert_result(p.run.get_run(&id))? {
            all.push(versioned_to_run_info(versioned));
        }
    }

    // Apply limit if specified
    let limited: Vec<VersionedRunInfo> = match limit {
        Some(l) => all.into_iter().take(l as usize).collect(),
        None => all,
    };

    Ok(Output::RunInfoList(limited))
}

/// Handle RunExists command.
pub fn run_exists(p: &Arc<Primitives>, run: RunId) -> Result<Output> {
    let exists = convert_result(p.run.exists(run.as_str()))?;
    Ok(Output::Bool(exists))
}

/// Handle RunDelete command.
pub fn run_delete(p: &Arc<Primitives>, run: RunId) -> Result<Output> {
    reject_default_run(&run, "delete")?;
    convert_result(p.run.delete_run(run.as_str()))?;
    Ok(Output::Unit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reject_default_run() {
        let run = RunId::from("default");
        assert!(reject_default_run(&run, "delete").is_err());

        let run = RunId::from("f47ac10b-58cc-4372-a567-0e02b2c3d479");
        assert!(reject_default_run(&run, "delete").is_ok());
    }

    #[test]
    fn test_metadata_to_run_info() {
        let m = RunMetadata {
            name: "test-run".to_string(),
            run_id: "some-uuid".to_string(),
            parent_run: None,
            status: strata_engine::RunStatus::Active,
            created_at: 1000000,
            updated_at: 2000000,
            completed_at: None,
            tags: vec![],
            metadata: strata_core::Value::Null,
            error: None,
            version: 1,
        };
        let info = metadata_to_run_info(&m);
        assert_eq!(info.id.as_str(), "test-run");
        assert_eq!(info.status, crate::types::RunStatus::Active);
    }
}
