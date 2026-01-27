//! Run command handlers.
//!
//! This module implements handlers for all Run commands by dispatching
//! directly to engine primitives via `bridge::Primitives`.

use std::sync::Arc;

use strata_core::Value;
use strata_engine::RunMetadata;

use crate::bridge::{extract_version, from_engine_run_status, to_engine_run_status, Primitives};
use crate::convert::convert_result;
use crate::types::{RetentionPolicyInfo, RunId, RunInfo, RunStatus, VersionedRunInfo};
use crate::{Error, Output, Result};

// =============================================================================
// Conversion Helpers
// =============================================================================

/// Convert engine RunMetadata to executor RunInfo.
///
/// Maps engine fields to executor types:
/// - `name` is the user-visible run identifier
/// - `created_at` / `updated_at` are u64 micros, kept as-is
/// - Null metadata becomes None
/// - parent_run string becomes Option<RunId>
fn metadata_to_run_info(m: &RunMetadata) -> RunInfo {
    let metadata = if m.metadata == Value::Null {
        None
    } else {
        Some(m.metadata.clone())
    };
    RunInfo {
        id: RunId::from(m.name.clone()),
        status: from_engine_run_status(m.status),
        created_at: m.created_at,
        updated_at: m.updated_at,
        metadata,
        parent_id: m.parent_run.as_ref().map(|p| RunId::from(p.clone())),
        tags: m.tags.clone(),
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

/// Convert bare RunMetadata (from query_by_status, etc.) to VersionedRunInfo.
///
/// These engine methods return `Vec<RunMetadata>` without Versioned wrappers,
/// so we construct one from the metadata's internal version and updated_at fields.
fn bare_metadata_to_versioned_info(m: RunMetadata) -> VersionedRunInfo {
    let version = m.version;
    let timestamp = m.updated_at;
    let info = metadata_to_run_info(&m);
    VersionedRunInfo {
        info,
        version,
        timestamp,
    }
}

/// Serde-compatible retention policy for JSON storage in run metadata.
///
/// This is a local serialization format. It stores the policy as a tagged JSON value
/// matching the format previously used by the API's `RetentionPolicy` type.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", content = "value")]
enum RetentionPolicySerde {
    #[serde(rename = "keep_all")]
    KeepAll,
    #[serde(rename = "keep_last")]
    KeepLast(u64),
    #[serde(rename = "keep_for")]
    KeepFor(u64), // duration in microseconds
}

/// Convert executor RetentionPolicyInfo to serde-compatible form for storage.
fn to_retention_serde(policy: RetentionPolicyInfo) -> RetentionPolicySerde {
    match policy {
        RetentionPolicyInfo::KeepAll => RetentionPolicySerde::KeepAll,
        RetentionPolicyInfo::KeepLast { count } => RetentionPolicySerde::KeepLast(count),
        RetentionPolicyInfo::KeepFor { duration_secs } => {
            RetentionPolicySerde::KeepFor(duration_secs * 1_000_000) // secs to micros
        }
    }
}

/// Convert serde-compatible retention policy back to executor RetentionPolicyInfo.
fn from_retention_serde(policy: RetentionPolicySerde) -> RetentionPolicyInfo {
    match policy {
        RetentionPolicySerde::KeepAll => RetentionPolicyInfo::KeepAll,
        RetentionPolicySerde::KeepLast(count) => RetentionPolicyInfo::KeepLast { count },
        RetentionPolicySerde::KeepFor(micros) => RetentionPolicyInfo::KeepFor {
            duration_secs: micros / 1_000_000,
        },
    }
}

/// Reserved metadata key for retention policy storage.
const RETENTION_METADATA_KEY: &str = "_strata_retention";

/// Guard: reject operations on the default run that would close/archive/delete it.
fn reject_default_run(run: &RunId, operation: &str) -> Result<()> {
    if run.is_default() {
        return Err(Error::ConstraintViolation {
            reason: format!("Cannot {} the default run", operation),
        });
    }
    Ok(())
}

// =============================================================================
// Individual Handlers
// =============================================================================

/// Handle RunCreate command.
pub fn run_create(
    p: &Arc<Primitives>,
    run_id: Option<String>,
    metadata: Option<Value>,
) -> Result<Output> {
    let run_str = match &run_id {
        Some(s) => {
            // Validate the user-provided ID is a valid UUID or "default"
            if s != "default" {
                uuid::Uuid::parse_str(s).map_err(|_| Error::InvalidInput {
                    reason: format!("Invalid run ID format: '{}'", s),
                })?;
            }
            s.clone()
        }
        None => uuid::Uuid::new_v4().to_string(),
    };

    let versioned = if let Some(meta) = metadata {
        convert_result(
            p.run
                .create_run_with_options(&run_str, None, vec![], meta),
        )?
    } else {
        convert_result(p.run.create_run(&run_str))?
    };

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
    state: Option<RunStatus>,
    limit: Option<u64>,
    _offset: Option<u64>,
) -> Result<Output> {
    let runs = if let Some(s) = state {
        let engine_status = to_engine_run_status(s);
        let metas = convert_result(p.run.query_by_status(engine_status))?;
        metas
    } else {
        let ids = convert_result(p.run.list_runs())?;
        let mut all = Vec::new();
        for id in ids {
            if let Some(versioned) = convert_result(p.run.get_run(&id))? {
                all.push(versioned.value);
            }
        }
        all
    };

    let limited: Vec<RunMetadata> = match limit {
        Some(l) => runs.into_iter().take(l as usize).collect(),
        None => runs,
    };

    let infos: Vec<VersionedRunInfo> = limited
        .into_iter()
        .map(bare_metadata_to_versioned_info)
        .collect();
    Ok(Output::RunInfoList(infos))
}

/// Handle RunComplete command.
pub fn run_complete(p: &Arc<Primitives>, run: RunId) -> Result<Output> {
    reject_default_run(&run, "close")?;
    let versioned = convert_result(p.run.complete_run(run.as_str()))?;
    Ok(Output::Version(extract_version(&versioned.version)))
}

/// Handle RunUpdateMetadata command.
pub fn run_update_metadata(
    p: &Arc<Primitives>,
    run: RunId,
    metadata: Value,
) -> Result<Output> {
    let versioned = convert_result(p.run.update_metadata(run.as_str(), metadata))?;
    Ok(Output::Version(extract_version(&versioned.version)))
}

/// Handle RunExists command.
pub fn run_exists(p: &Arc<Primitives>, run: RunId) -> Result<Output> {
    let exists = convert_result(p.run.exists(run.as_str()))?;
    Ok(Output::Bool(exists))
}

/// Handle RunPause command.
pub fn run_pause(p: &Arc<Primitives>, run: RunId) -> Result<Output> {
    let versioned = convert_result(p.run.pause_run(run.as_str()))?;
    Ok(Output::Version(extract_version(&versioned.version)))
}

/// Handle RunResume command.
pub fn run_resume(p: &Arc<Primitives>, run: RunId) -> Result<Output> {
    let versioned = convert_result(p.run.resume_run(run.as_str()))?;
    Ok(Output::Version(extract_version(&versioned.version)))
}

/// Handle RunFail command.
pub fn run_fail(p: &Arc<Primitives>, run: RunId, error: String) -> Result<Output> {
    let versioned = convert_result(p.run.fail_run(run.as_str(), &error))?;
    Ok(Output::Version(extract_version(&versioned.version)))
}

/// Handle RunCancel command.
pub fn run_cancel(p: &Arc<Primitives>, run: RunId) -> Result<Output> {
    let versioned = convert_result(p.run.cancel_run(run.as_str()))?;
    Ok(Output::Version(extract_version(&versioned.version)))
}

/// Handle RunArchive command.
pub fn run_archive(p: &Arc<Primitives>, run: RunId) -> Result<Output> {
    reject_default_run(&run, "archive")?;
    let versioned = convert_result(p.run.archive_run(run.as_str()))?;
    Ok(Output::Version(extract_version(&versioned.version)))
}

/// Handle RunDelete command.
pub fn run_delete(p: &Arc<Primitives>, run: RunId) -> Result<Output> {
    reject_default_run(&run, "delete")?;
    convert_result(p.run.delete_run(run.as_str()))?;
    Ok(Output::Unit)
}

/// Handle RunQueryByStatus command.
pub fn run_query_by_status(
    p: &Arc<Primitives>,
    state: RunStatus,
) -> Result<Output> {
    let engine_status = to_engine_run_status(state);
    let runs = convert_result(p.run.query_by_status(engine_status))?;
    let infos: Vec<VersionedRunInfo> = runs
        .into_iter()
        .map(bare_metadata_to_versioned_info)
        .collect();
    Ok(Output::RunInfoList(infos))
}

/// Handle RunQueryByTag command.
pub fn run_query_by_tag(p: &Arc<Primitives>, tag: String) -> Result<Output> {
    let runs = convert_result(p.run.query_by_tag(&tag))?;
    let infos: Vec<VersionedRunInfo> = runs
        .into_iter()
        .map(bare_metadata_to_versioned_info)
        .collect();
    Ok(Output::RunInfoList(infos))
}

/// Handle RunCount command.
pub fn run_count(p: &Arc<Primitives>, status: Option<RunStatus>) -> Result<Output> {
    match status {
        Some(s) => {
            let engine_status = to_engine_run_status(s);
            let runs = convert_result(p.run.query_by_status(engine_status))?;
            Ok(Output::Uint(runs.len() as u64))
        }
        None => {
            let count = convert_result(p.run.count())?;
            Ok(Output::Uint(count as u64))
        }
    }
}

/// Handle RunSearch command.
pub fn run_search(
    p: &Arc<Primitives>,
    query: String,
    limit: Option<u64>,
) -> Result<Output> {
    use strata_core::{SearchBudget, SearchRequest};
    use strata_core::types::RunId as CoreRunId;

    let req = SearchRequest {
        run_id: CoreRunId::from_bytes([0; 16]), // Global namespace
        query,
        k: limit.unwrap_or(10) as usize,
        budget: SearchBudget::default(),
        time_range: None,
        mode: Default::default(),
        primitive_filter: None,
        tags_any: vec![],
    };

    let response = convert_result(p.run.search(&req))?;

    // Convert search hits back to RunInfo by looking up each matched run
    let mut results = Vec::new();
    for hit in response.hits {
        if let strata_core::search_types::EntityRef::Run { run_id } = hit.doc_ref {
            let run_uuid = uuid::Uuid::from_bytes(*run_id.as_bytes());
            let run_str = run_uuid.to_string();
            // The run_id in search results is the internal UUID; look up by iterating
            // or try to find by the string. The engine stores runs by name, so we
            // need to find the run whose run_id matches.
            // For simplicity, list all runs and find the matching one.
            if let Ok(Some(versioned)) = p.run.get_run(&run_str) {
                results.push(versioned_to_run_info(versioned));
            }
        }
    }
    Ok(Output::RunInfoList(results))
}

/// Handle RunAddTags command.
pub fn run_add_tags(
    p: &Arc<Primitives>,
    run: RunId,
    tags: Vec<String>,
) -> Result<Output> {
    let versioned = convert_result(p.run.add_tags(run.as_str(), tags))?;
    Ok(Output::Version(extract_version(&versioned.version)))
}

/// Handle RunRemoveTags command.
pub fn run_remove_tags(
    p: &Arc<Primitives>,
    run: RunId,
    tags: Vec<String>,
) -> Result<Output> {
    let versioned = convert_result(p.run.remove_tags(run.as_str(), tags))?;
    Ok(Output::Version(extract_version(&versioned.version)))
}

/// Handle RunGetTags command.
pub fn run_get_tags(p: &Arc<Primitives>, run: RunId) -> Result<Output> {
    let meta = convert_result(p.run.get_run(run.as_str()))?
        .ok_or_else(|| Error::RunNotFound {
            run: run.as_str().to_string(),
        })?;
    Ok(Output::Strings(meta.value.tags))
}

/// Handle RunCreateChild command.
pub fn run_create_child(
    p: &Arc<Primitives>,
    parent: RunId,
    metadata: Option<Value>,
) -> Result<Output> {
    let child_id = uuid::Uuid::new_v4().to_string();
    let versioned = convert_result(p.run.create_run_with_options(
        &child_id,
        Some(parent.as_str().to_string()),
        vec![],
        metadata.unwrap_or(Value::Null),
    ))?;

    Ok(Output::RunWithVersion {
        info: metadata_to_run_info(&versioned.value),
        version: extract_version(&versioned.version),
    })
}

/// Handle RunGetChildren command.
pub fn run_get_children(p: &Arc<Primitives>, parent: RunId) -> Result<Output> {
    let children = convert_result(p.run.get_child_runs(parent.as_str()))?;
    let infos: Vec<VersionedRunInfo> = children
        .into_iter()
        .map(bare_metadata_to_versioned_info)
        .collect();
    Ok(Output::RunInfoList(infos))
}

/// Handle RunGetParent command.
pub fn run_get_parent(p: &Arc<Primitives>, run: RunId) -> Result<Output> {
    let meta = convert_result(p.run.get_run(run.as_str()))?
        .ok_or_else(|| Error::RunNotFound {
            run: run.as_str().to_string(),
        })?;
    Ok(Output::MaybeRunId(
        meta.value.parent_run.map(|p| RunId::from(p)),
    ))
}

/// Handle RunSetRetention command.
pub fn run_set_retention(
    p: &Arc<Primitives>,
    run: RunId,
    policy: RetentionPolicyInfo,
) -> Result<Output> {
    // Get current run metadata
    let current = convert_result(p.run.get_run(run.as_str()))?
        .ok_or_else(|| Error::RunNotFound {
            run: run.as_str().to_string(),
        })?;

    // Merge retention policy into metadata
    let mut map = match current.value.metadata {
        Value::Object(map) => map,
        _ => std::collections::HashMap::new(),
    };

    let serde_policy = to_retention_serde(policy);
    let retention_json = serde_json::to_string(&serde_policy).map_err(|e| Error::Serialization {
        reason: e.to_string(),
    })?;
    map.insert(
        RETENTION_METADATA_KEY.to_string(),
        Value::String(retention_json),
    );

    let versioned = convert_result(p.run.update_metadata(run.as_str(), Value::Object(map)))?;
    Ok(Output::Version(extract_version(&versioned.version)))
}

/// Handle RunGetRetention command.
pub fn run_get_retention(p: &Arc<Primitives>, run: RunId) -> Result<Output> {
    let meta = convert_result(p.run.get_run(run.as_str()))?
        .ok_or_else(|| Error::RunNotFound {
            run: run.as_str().to_string(),
        })?;

    if let Value::Object(map) = &meta.value.metadata {
        if let Some(Value::String(retention_json)) = map.get(RETENTION_METADATA_KEY) {
            let policy: RetentionPolicySerde =
                serde_json::from_str(retention_json).map_err(|e| Error::Serialization {
                    reason: e.to_string(),
                })?;
            return Ok(Output::RetentionPolicy(from_retention_serde(policy)));
        }
    }

    Ok(Output::RetentionPolicy(RetentionPolicyInfo::KeepAll))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reject_default_run() {
        let run = RunId::from("default");
        assert!(reject_default_run(&run, "close").is_err());

        let run = RunId::from("f47ac10b-58cc-4372-a567-0e02b2c3d479");
        assert!(reject_default_run(&run, "close").is_ok());
    }

    #[test]
    fn test_metadata_to_run_info_null_metadata() {
        let m = RunMetadata {
            name: "test-run".to_string(),
            run_id: "some-uuid".to_string(),
            parent_run: None,
            status: strata_engine::RunStatus::Active,
            created_at: 1000000,
            updated_at: 2000000,
            completed_at: None,
            tags: vec!["tag1".to_string()],
            metadata: Value::Null,
            error: None,
            version: 1,
        };
        let info = metadata_to_run_info(&m);
        assert_eq!(info.id.as_str(), "test-run");
        assert_eq!(info.status, RunStatus::Active);
        assert!(info.metadata.is_none());
        assert_eq!(info.tags, vec!["tag1".to_string()]);
    }

    #[test]
    fn test_metadata_to_run_info_with_parent() {
        let m = RunMetadata {
            name: "child-run".to_string(),
            run_id: "child-uuid".to_string(),
            parent_run: Some("parent-run".to_string()),
            status: strata_engine::RunStatus::Completed,
            created_at: 1000000,
            updated_at: 2000000,
            completed_at: Some(3000000),
            tags: vec![],
            metadata: Value::Object(std::collections::HashMap::new()),
            error: None,
            version: 2,
        };
        let info = metadata_to_run_info(&m);
        assert_eq!(info.parent_id, Some(RunId::from("parent-run")));
        assert_eq!(info.status, RunStatus::Completed);
    }
}
