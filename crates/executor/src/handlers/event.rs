//! Event command handlers.
//!
//! This module implements handlers for all 11 Event commands by calling
//! engine primitives directly via `bridge::Primitives`.

use std::sync::Arc;

use strata_core::{Value, Version};

use crate::bridge::{self, Primitives};
use crate::convert::convert_result;
use crate::types::{ChainVerificationResult, RunId, StreamInfo, VersionedValue};
use crate::{Output, Result};

// =============================================================================
// Individual Handlers
// =============================================================================

/// Handle EventAppend command.
pub fn event_append(
    p: &Arc<Primitives>,
    run: RunId,
    stream: String,
    payload: Value,
) -> Result<Output> {
    let core_run = bridge::to_core_run_id(&run)?;
    let version = convert_result(p.event.append(&core_run, &stream, payload))?;
    Ok(Output::Version(bridge::extract_version(&version)))
}

/// Handle EventAppendBatch command.
pub fn event_append_batch(
    p: &Arc<Primitives>,
    run: RunId,
    events: Vec<(String, Value)>,
) -> Result<Output> {
    let core_run = bridge::to_core_run_id(&run)?;
    // Convert Vec<(String, Value)> to Vec<(&str, Value)>
    let event_refs: Vec<(&str, Value)> = events
        .iter()
        .map(|(s, v): &(String, Value)| (s.as_str(), v.clone()))
        .collect();
    let versions = convert_result(p.event.append_batch(&core_run, &event_refs))?;
    let version_nums: Vec<u64> = versions.iter().map(|v| bridge::extract_version(v)).collect();
    Ok(Output::Versions(version_nums))
}

/// Handle EventRange command.
pub fn event_range(
    p: &Arc<Primitives>,
    run: RunId,
    stream: String,
    start: Option<u64>,
    end: Option<u64>,
    limit: Option<u64>,
) -> Result<Output> {
    let core_run = bridge::to_core_run_id(&run)?;
    convert_result(bridge::validate_stream_name(&stream))?;

    // Read all events of this type, then filter by range and limit
    let events = convert_result(p.event.read_by_type(&core_run, &stream))?;

    let filtered: Vec<VersionedValue> = events
        .into_iter()
        .filter(|e| {
            let seq = match e.version {
                Version::Sequence(s) => s,
                _ => return false,
            };
            start.map_or(true, |s| seq >= s) && end.map_or(true, |e| seq <= e)
        })
        .take(limit.unwrap_or(u64::MAX) as usize)
        .map(|e| VersionedValue {
            value: e.value.payload.clone(),
            version: bridge::extract_version(&e.version),
            timestamp: strata_core::Timestamp::from_micros(e.value.timestamp).into(),
        })
        .collect();

    Ok(Output::VersionedValues(filtered))
}

/// Handle EventRead command.
pub fn event_read(
    p: &Arc<Primitives>,
    run: RunId,
    stream: String,
    sequence: u64,
) -> Result<Output> {
    let core_run = bridge::to_core_run_id(&run)?;
    convert_result(bridge::validate_stream_name(&stream))?;

    // Read the event at this sequence
    let event = convert_result(p.event.read(&core_run, sequence))?;

    // Check if it matches the requested stream (event_type)
    let result = match event {
        Some(e) if e.value.event_type == stream => Some(VersionedValue {
            value: e.value.payload,
            version: bridge::extract_version(&e.version),
            timestamp: strata_core::Timestamp::from_micros(e.value.timestamp).into(),
        }),
        _ => None,
    };

    Ok(Output::MaybeVersioned(result))
}

/// Handle EventLen command.
pub fn event_len(
    p: &Arc<Primitives>,
    run: RunId,
    stream: String,
) -> Result<Output> {
    let core_run = bridge::to_core_run_id(&run)?;
    convert_result(bridge::validate_stream_name(&stream))?;
    let count = convert_result(p.event.len_by_type(&core_run, &stream))?;
    Ok(Output::Uint(count))
}

/// Handle EventLatestSequence command.
pub fn event_latest_sequence(
    p: &Arc<Primitives>,
    run: RunId,
    stream: String,
) -> Result<Output> {
    let core_run = bridge::to_core_run_id(&run)?;
    convert_result(bridge::validate_stream_name(&stream))?;
    let sequence = convert_result(p.event.latest_sequence_by_type(&core_run, &stream))?;
    // Use MaybeVersion since it's Option<u64> - sequence numbers are version-like
    Ok(Output::MaybeVersion(sequence))
}

/// Handle EventStreamInfo command.
pub fn event_stream_info(
    p: &Arc<Primitives>,
    run: RunId,
    stream: String,
) -> Result<Output> {
    let core_run = bridge::to_core_run_id(&run)?;
    convert_result(bridge::validate_stream_name(&stream))?;
    let info = convert_result(p.event.stream_info(&core_run, &stream))?;
    let stream_info = match info {
        Some(meta) => StreamInfo {
            name: stream,
            length: meta.count,
            first_sequence: Some(meta.first_sequence),
            last_sequence: Some(meta.last_sequence),
        },
        None => StreamInfo {
            name: stream,
            length: 0,
            first_sequence: None,
            last_sequence: None,
        },
    };
    Ok(Output::StreamInfo(stream_info))
}

/// Handle EventRevRange command.
pub fn event_rev_range(
    p: &Arc<Primitives>,
    run: RunId,
    stream: String,
    start: Option<u64>,
    end: Option<u64>,
    limit: Option<u64>,
) -> Result<Output> {
    let core_run = bridge::to_core_run_id(&run)?;
    convert_result(bridge::validate_stream_name(&stream))?;

    // Read all events of this type, then filter by reversed range and limit
    let events = convert_result(p.event.read_by_type(&core_run, &stream))?;

    // For rev_range: start is the high bound, end is the low bound
    let mut filtered: Vec<VersionedValue> = events
        .into_iter()
        .filter(|e| {
            let seq = match e.version {
                Version::Sequence(s) => s,
                _ => return false,
            };
            start.map_or(true, |s| seq <= s) && end.map_or(true, |e| seq >= e)
        })
        .map(|e| VersionedValue {
            value: e.value.payload.clone(),
            version: bridge::extract_version(&e.version),
            timestamp: strata_core::Timestamp::from_micros(e.value.timestamp).into(),
        })
        .collect();

    // Reverse to get newest first
    filtered.reverse();

    // Apply limit
    if let Some(n) = limit {
        filtered.truncate(n as usize);
    }

    Ok(Output::VersionedValues(filtered))
}

/// Handle EventStreams command.
pub fn event_streams(p: &Arc<Primitives>, run: RunId) -> Result<Output> {
    let core_run = bridge::to_core_run_id(&run)?;
    let streams = convert_result(p.event.stream_names(&core_run))?;
    Ok(Output::Strings(streams))
}

/// Handle EventHead command.
pub fn event_head(
    p: &Arc<Primitives>,
    run: RunId,
    stream: String,
) -> Result<Output> {
    let core_run = bridge::to_core_run_id(&run)?;
    convert_result(bridge::validate_stream_name(&stream))?;
    let result = convert_result(p.event.head_by_type(&core_run, &stream))?;
    let versioned = result.map(|e| VersionedValue {
        value: e.value.payload.clone(),
        version: bridge::extract_version(&e.version),
        timestamp: strata_core::Timestamp::from_micros(e.value.timestamp).into(),
    });
    Ok(Output::MaybeVersioned(versioned))
}

/// Handle EventVerifyChain command.
pub fn event_verify_chain(p: &Arc<Primitives>, run: RunId) -> Result<Output> {
    let core_run = bridge::to_core_run_id(&run)?;
    let verification = convert_result(p.event.verify_chain(&core_run))?;
    Ok(Output::ChainVerification(ChainVerificationResult {
        valid: verification.is_valid,
        checked_count: verification.length,
        error: verification.error,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bridge_extract_version() {
        assert_eq!(bridge::extract_version(&Version::Sequence(42)), 42);
    }
}
