//! Run command handlers.
//!
//! This module implements handlers for all 24 Run commands by dispatching
//! to the SubstrateImpl's RunIndex trait methods.

use std::sync::Arc;

use strata_api::substrate::types::{RetentionPolicy, RunInfo as ApiRunInfo, RunState as ApiRunState};
use strata_api::substrate::{ApiRunId, RunIndex, SubstrateImpl};
use strata_core::{Value, Version, Versioned};

use crate::convert::convert_result;
use crate::types::{RetentionPolicyInfo, RunId, RunInfo, RunStatus, VersionedRunInfo};
use crate::{Error, Output, Result};

/// Convert executor RunId to API RunId.
fn to_api_run_id(run: &RunId) -> Result<ApiRunId> {
    ApiRunId::parse(run.as_str()).ok_or_else(|| Error::InvalidInput {
        reason: format!("Invalid run ID: '{}'", run.as_str()),
    })
}

/// Extract u64 from Version enum.
fn extract_version(v: &Version) -> u64 {
    match v {
        Version::Txn(n) => *n,
        Version::Sequence(n) => *n,
        Version::Counter(n) => *n,
    }
}

/// Convert executor RunStatus to API RunState.
fn to_api_run_state(status: RunStatus) -> ApiRunState {
    match status {
        RunStatus::Active => ApiRunState::Active,
        RunStatus::Completed => ApiRunState::Completed,
        RunStatus::Failed => ApiRunState::Failed,
        RunStatus::Cancelled => ApiRunState::Cancelled,
        RunStatus::Paused => ApiRunState::Paused,
        RunStatus::Archived => ApiRunState::Archived,
    }
}

/// Convert API RunState to executor RunStatus.
fn from_api_run_state(state: ApiRunState) -> RunStatus {
    match state {
        ApiRunState::Active => RunStatus::Active,
        ApiRunState::Completed => RunStatus::Completed,
        ApiRunState::Failed => RunStatus::Failed,
        ApiRunState::Cancelled => RunStatus::Cancelled,
        ApiRunState::Paused => RunStatus::Paused,
        ApiRunState::Archived => RunStatus::Archived,
    }
}

/// Convert API RunInfo to executor RunInfo.
fn to_run_info(info: ApiRunInfo) -> RunInfo {
    let metadata = if info.metadata == Value::Null {
        None
    } else {
        Some(info.metadata)
    };
    RunInfo {
        id: RunId::from(info.run_id.to_string()),
        status: from_api_run_state(info.state),
        created_at: info.created_at,
        updated_at: info.created_at, // API doesn't provide updated_at separately
        metadata,
        parent_id: None, // API doesn't expose parent_id in RunInfo directly
        tags: vec![],    // API doesn't expose tags in RunInfo directly
    }
}

/// Convert Versioned<ApiRunInfo> to VersionedRunInfo.
fn to_versioned_run_info(v: Versioned<ApiRunInfo>) -> VersionedRunInfo {
    VersionedRunInfo {
        info: to_run_info(v.value),
        version: extract_version(&v.version),
        timestamp: v.timestamp.into(),
    }
}

/// Convert executor RetentionPolicyInfo to API RetentionPolicy.
fn to_api_retention(policy: RetentionPolicyInfo) -> RetentionPolicy {
    match policy {
        RetentionPolicyInfo::KeepAll => RetentionPolicy::KeepAll,
        RetentionPolicyInfo::KeepLast { count } => RetentionPolicy::KeepLast(count),
        RetentionPolicyInfo::KeepFor { duration_secs } => {
            RetentionPolicy::KeepFor(std::time::Duration::from_secs(duration_secs))
        }
    }
}

/// Convert API RetentionPolicy to executor RetentionPolicyInfo.
fn from_api_retention(policy: RetentionPolicy) -> RetentionPolicyInfo {
    match policy {
        RetentionPolicy::KeepAll => RetentionPolicyInfo::KeepAll,
        RetentionPolicy::KeepLast(count) => RetentionPolicyInfo::KeepLast { count },
        RetentionPolicy::KeepFor(duration) => {
            RetentionPolicyInfo::KeepFor { duration_secs: duration.as_secs() }
        }
        // Composite policies are not supported in the executor API
        // Fall back to KeepAll
        RetentionPolicy::Composite(_) => RetentionPolicyInfo::KeepAll,
    }
}

// =============================================================================
// Individual Handlers
// =============================================================================

/// Handle RunCreate command.
pub fn run_create(
    substrate: &Arc<SubstrateImpl>,
    run_id: Option<String>,
    metadata: Option<Value>,
) -> Result<Output> {
    // Parse the optional run_id - if provided and invalid, return error
    let api_run_id = match run_id {
        Some(ref s) => Some(ApiRunId::parse(s).ok_or_else(|| Error::InvalidInput {
            reason: format!("Invalid run ID format: '{}'", s),
        })?),
        None => None,
    };
    let (info, version) = convert_result(substrate.run_create(api_run_id.as_ref(), metadata))?;
    Ok(Output::RunWithVersion {
        info: to_run_info(info),
        version: extract_version(&version),
    })
}

/// Handle RunGet command.
pub fn run_get(substrate: &Arc<SubstrateImpl>, run: RunId) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let result = convert_result(substrate.run_get(&api_run))?;
    Ok(Output::RunInfoVersioned(
        match result {
            Some(v) => to_versioned_run_info(v),
            None => return Ok(Output::Maybe(None)),
        },
    ))
}

/// Handle RunList command.
pub fn run_list(
    substrate: &Arc<SubstrateImpl>,
    state: Option<RunStatus>,
    limit: Option<u64>,
    offset: Option<u64>,
) -> Result<Output> {
    let api_state = state.map(to_api_run_state);
    let runs = convert_result(substrate.run_list(api_state, limit, offset))?;
    let infos: Vec<VersionedRunInfo> = runs.into_iter().map(to_versioned_run_info).collect();
    Ok(Output::RunInfoList(infos))
}

/// Handle RunClose command.
pub fn run_close(substrate: &Arc<SubstrateImpl>, run: RunId) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let version = convert_result(substrate.run_close(&api_run))?;
    Ok(Output::Version(extract_version(&version)))
}

/// Handle RunUpdateMetadata command.
pub fn run_update_metadata(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    metadata: Value,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let version = convert_result(substrate.run_update_metadata(&api_run, metadata))?;
    Ok(Output::Version(extract_version(&version)))
}

/// Handle RunExists command.
pub fn run_exists(substrate: &Arc<SubstrateImpl>, run: RunId) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let exists = convert_result(substrate.run_exists(&api_run))?;
    Ok(Output::Bool(exists))
}

/// Handle RunPause command.
pub fn run_pause(substrate: &Arc<SubstrateImpl>, run: RunId) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let version = convert_result(substrate.run_pause(&api_run))?;
    Ok(Output::Version(extract_version(&version)))
}

/// Handle RunResume command.
pub fn run_resume(substrate: &Arc<SubstrateImpl>, run: RunId) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let version = convert_result(substrate.run_resume(&api_run))?;
    Ok(Output::Version(extract_version(&version)))
}

/// Handle RunFail command.
pub fn run_fail(substrate: &Arc<SubstrateImpl>, run: RunId, error: String) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let version = convert_result(substrate.run_fail(&api_run, &error))?;
    Ok(Output::Version(extract_version(&version)))
}

/// Handle RunCancel command.
pub fn run_cancel(substrate: &Arc<SubstrateImpl>, run: RunId) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let version = convert_result(substrate.run_cancel(&api_run))?;
    Ok(Output::Version(extract_version(&version)))
}

/// Handle RunArchive command.
pub fn run_archive(substrate: &Arc<SubstrateImpl>, run: RunId) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let version = convert_result(substrate.run_archive(&api_run))?;
    Ok(Output::Version(extract_version(&version)))
}

/// Handle RunDelete command.
pub fn run_delete(substrate: &Arc<SubstrateImpl>, run: RunId) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    convert_result(substrate.run_delete(&api_run))?;
    Ok(Output::Unit)
}

/// Handle RunQueryByStatus command.
pub fn run_query_by_status(
    substrate: &Arc<SubstrateImpl>,
    state: RunStatus,
) -> Result<Output> {
    let api_state = to_api_run_state(state);
    let runs = convert_result(substrate.run_query_by_status(api_state))?;
    let infos: Vec<VersionedRunInfo> = runs.into_iter().map(to_versioned_run_info).collect();
    Ok(Output::RunInfoList(infos))
}

/// Handle RunQueryByTag command.
pub fn run_query_by_tag(substrate: &Arc<SubstrateImpl>, tag: String) -> Result<Output> {
    let runs = convert_result(substrate.run_query_by_tag(&tag))?;
    let infos: Vec<VersionedRunInfo> = runs.into_iter().map(to_versioned_run_info).collect();
    Ok(Output::RunInfoList(infos))
}

/// Handle RunCount command.
pub fn run_count(substrate: &Arc<SubstrateImpl>, status: Option<RunStatus>) -> Result<Output> {
    let api_status = status.map(to_api_run_state);
    let count = convert_result(substrate.run_count(api_status))?;
    Ok(Output::Uint(count))
}

/// Handle RunSearch command.
pub fn run_search(
    substrate: &Arc<SubstrateImpl>,
    query: String,
    limit: Option<u64>,
) -> Result<Output> {
    let runs = convert_result(substrate.run_search(&query, limit))?;
    let infos: Vec<VersionedRunInfo> = runs.into_iter().map(to_versioned_run_info).collect();
    Ok(Output::RunInfoList(infos))
}

/// Handle RunAddTags command.
pub fn run_add_tags(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    tags: Vec<String>,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let version = convert_result(substrate.run_add_tags(&api_run, &tags))?;
    Ok(Output::Version(extract_version(&version)))
}

/// Handle RunRemoveTags command.
pub fn run_remove_tags(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    tags: Vec<String>,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let version = convert_result(substrate.run_remove_tags(&api_run, &tags))?;
    Ok(Output::Version(extract_version(&version)))
}

/// Handle RunGetTags command.
pub fn run_get_tags(substrate: &Arc<SubstrateImpl>, run: RunId) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let tags = convert_result(substrate.run_get_tags(&api_run))?;
    Ok(Output::Strings(tags))
}

/// Handle RunCreateChild command.
pub fn run_create_child(
    substrate: &Arc<SubstrateImpl>,
    parent: RunId,
    metadata: Option<Value>,
) -> Result<Output> {
    let api_parent = to_api_run_id(&parent)?;
    let (info, version) = convert_result(substrate.run_create_child(&api_parent, metadata))?;
    Ok(Output::RunWithVersion {
        info: to_run_info(info),
        version: extract_version(&version),
    })
}

/// Handle RunGetChildren command.
pub fn run_get_children(substrate: &Arc<SubstrateImpl>, parent: RunId) -> Result<Output> {
    let api_parent = to_api_run_id(&parent)?;
    let children = convert_result(substrate.run_get_children(&api_parent))?;
    let infos: Vec<VersionedRunInfo> = children.into_iter().map(to_versioned_run_info).collect();
    Ok(Output::RunInfoList(infos))
}

/// Handle RunGetParent command.
pub fn run_get_parent(substrate: &Arc<SubstrateImpl>, run: RunId) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let parent = convert_result(substrate.run_get_parent(&api_run))?;
    Ok(Output::MaybeRunId(parent.map(|p| RunId::from(p.to_string()))))
}

/// Handle RunSetRetention command.
pub fn run_set_retention(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    policy: RetentionPolicyInfo,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let api_policy = to_api_retention(policy);
    let version = convert_result(substrate.run_set_retention(&api_run, api_policy))?;
    Ok(Output::Version(extract_version(&version)))
}

/// Handle RunGetRetention command.
pub fn run_get_retention(substrate: &Arc<SubstrateImpl>, run: RunId) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let policy = convert_result(substrate.run_get_retention(&api_run))?;
    Ok(Output::RetentionPolicy(from_api_retention(policy)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_api_run_id_default() {
        let run = RunId::from("default");
        let api_run = to_api_run_id(&run).unwrap();
        assert!(api_run.is_default());
    }

    #[test]
    fn test_run_state_conversion() {
        assert_eq!(
            to_api_run_state(RunStatus::Active),
            ApiRunState::Active
        );
        assert_eq!(
            from_api_run_state(ApiRunState::Completed),
            RunStatus::Completed
        );
    }
}
