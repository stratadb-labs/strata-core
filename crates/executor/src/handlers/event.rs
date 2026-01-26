//! Event command handlers.
//!
//! This module implements handlers for all 11 Event commands by dispatching
//! to the SubstrateImpl's EventLog trait methods.

use std::sync::Arc;

use strata_api::substrate::event::{ChainVerification as ApiChainVerification, StreamInfo as ApiStreamInfo};
use strata_api::substrate::{ApiRunId, EventLog, SubstrateImpl};
use strata_core::{Value, Version, Versioned};

use crate::convert::convert_result;
use crate::types::{ChainVerificationResult, RunId, StreamInfo, VersionedValue};
use crate::{Error, Output, Result};

/// Convert executor RunId to API RunId.
fn to_api_run_id(run: &RunId) -> Result<ApiRunId> {
    ApiRunId::parse(run.as_str()).ok_or_else(|| Error::InvalidInput {
        reason: format!("Invalid run ID: '{}'", run.as_str()),
    })
}

/// Convert Versioned<Value> to VersionedValue.
fn to_versioned_value(v: Versioned<Value>) -> VersionedValue {
    VersionedValue {
        value: v.value,
        version: extract_version(&v.version),
        timestamp: v.timestamp.into(),
    }
}

/// Extract u64 from Version enum.
fn extract_version(v: &Version) -> u64 {
    match v {
        Version::Txn(n) => *n,
        Version::Sequence(n) => *n,
        Version::Counter(n) => *n,
    }
}

/// Convert API StreamInfo to executor StreamInfo.
fn to_stream_info(name: String, info: ApiStreamInfo) -> StreamInfo {
    StreamInfo {
        name,
        length: info.count,
        first_sequence: info.first_sequence,
        last_sequence: info.last_sequence,
    }
}

/// Convert API ChainVerification to executor ChainVerificationResult.
fn to_chain_verification(v: ApiChainVerification) -> ChainVerificationResult {
    ChainVerificationResult {
        valid: v.is_valid,
        checked_count: v.length,
        error: v.error,
    }
}

// =============================================================================
// Individual Handlers
// =============================================================================

/// Handle EventAppend command.
pub fn event_append(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    stream: String,
    payload: Value,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let version = convert_result(substrate.event_append(&api_run, &stream, payload))?;
    Ok(Output::Version(extract_version(&version)))
}

/// Handle EventAppendBatch command.
pub fn event_append_batch(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    events: Vec<(String, Value)>,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    // Convert Vec<(String, Value)> to Vec<(&str, Value)>
    let event_refs: Vec<(&str, Value)> = events
        .iter()
        .map(|(s, v): &(String, Value)| (s.as_str(), v.clone()))
        .collect();
    let versions = convert_result(substrate.event_append_batch(&api_run, &event_refs))?;
    let version_nums: Vec<u64> = versions.into_iter().map(|v| extract_version(&v)).collect();
    Ok(Output::Versions(version_nums))
}

/// Handle EventRange command.
pub fn event_range(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    stream: String,
    start: Option<u64>,
    end: Option<u64>,
    limit: Option<u64>,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let events = convert_result(substrate.event_range(&api_run, &stream, start, end, limit))?;
    let values: Vec<VersionedValue> = events.into_iter().map(to_versioned_value).collect();
    Ok(Output::VersionedValues(values))
}

/// Handle EventGet command.
pub fn event_get(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    stream: String,
    sequence: u64,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let result = convert_result(substrate.event_get(&api_run, &stream, sequence))?;
    Ok(Output::MaybeVersioned(result.map(to_versioned_value)))
}

/// Handle EventLen command.
pub fn event_len(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    stream: String,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let count = convert_result(substrate.event_len(&api_run, &stream))?;
    Ok(Output::Uint(count))
}

/// Handle EventLatestSequence command.
pub fn event_latest_sequence(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    stream: String,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let sequence = convert_result(substrate.event_latest_sequence(&api_run, &stream))?;
    // Use MaybeVersion since it's Option<u64> - sequence numbers are version-like
    Ok(Output::MaybeVersion(sequence))
}

/// Handle EventStreamInfo command.
pub fn event_stream_info(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    stream: String,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let info = convert_result(substrate.event_stream_info(&api_run, &stream))?;
    Ok(Output::StreamInfo(to_stream_info(stream, info)))
}

/// Handle EventRevRange command.
pub fn event_rev_range(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    stream: String,
    start: Option<u64>,
    end: Option<u64>,
    limit: Option<u64>,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let events = convert_result(substrate.event_rev_range(&api_run, &stream, start, end, limit))?;
    let values: Vec<VersionedValue> = events.into_iter().map(to_versioned_value).collect();
    Ok(Output::VersionedValues(values))
}

/// Handle EventStreams command.
pub fn event_streams(substrate: &Arc<SubstrateImpl>, run: RunId) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let streams = convert_result(substrate.event_streams(&api_run))?;
    Ok(Output::Strings(streams))
}

/// Handle EventHead command.
pub fn event_head(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    stream: String,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let result = convert_result(substrate.event_head(&api_run, &stream))?;
    Ok(Output::MaybeVersioned(result.map(to_versioned_value)))
}

/// Handle EventVerifyChain command.
pub fn event_verify_chain(substrate: &Arc<SubstrateImpl>, run: RunId) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let verification = convert_result(substrate.event_verify_chain(&api_run))?;
    Ok(Output::ChainVerification(to_chain_verification(verification)))
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
    fn test_extract_version() {
        assert_eq!(extract_version(&Version::Sequence(42)), 42);
    }
}
