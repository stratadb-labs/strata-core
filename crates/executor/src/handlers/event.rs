//! Event command handlers (4 MVP).
//!
//! MVP: append, read, read_by_type, len

use std::sync::Arc;

use crate::bridge::{self, Primitives};
use crate::convert::convert_result;
use crate::types::{BranchId, VersionedValue};
use crate::{Error, Output, Result};

/// Validate that a branch exists before performing a write operation (#951).
///
/// The default branch is always allowed (it is implicit and not stored in BranchIndex).
/// For all other branches, checks `BranchIndex::exists()` and returns
/// `Error::BranchNotFound` if the branch does not exist.
fn require_branch_exists(p: &Arc<Primitives>, branch: &BranchId) -> Result<()> {
    if branch.is_default() {
        return Ok(());
    }
    let exists = convert_result(p.branch.exists(branch.as_str()))?;
    if !exists {
        return Err(Error::BranchNotFound {
            branch: branch.as_str().to_string(),
        });
    }
    Ok(())
}

// =============================================================================
// Individual Handlers (4 MVP)
// =============================================================================

/// Handle EventAppend command.
pub fn event_append(
    p: &Arc<Primitives>,
    branch: BranchId,
    space: String,
    event_type: String,
    payload: strata_core::Value,
) -> Result<Output> {
    require_branch_exists(p, &branch)?;
    let core_branch_id = bridge::to_core_branch_id(&branch)?;
    let version = convert_result(p.event.append(&core_branch_id, &space, &event_type, payload))?;
    Ok(Output::Version(bridge::extract_version(&version)))
}

/// Handle EventRead command.
pub fn event_read(p: &Arc<Primitives>, branch: BranchId, space: String, sequence: u64) -> Result<Output> {
    let core_branch_id = bridge::to_core_branch_id(&branch)?;
    let event = convert_result(p.event.read(&core_branch_id, &space, sequence))?;

    let result = event.map(|e| VersionedValue {
        value: e.value.payload,
        version: bridge::extract_version(&e.version),
        timestamp: strata_core::Timestamp::from_micros(e.value.timestamp).into(),
    });

    Ok(Output::MaybeVersioned(result))
}

/// Handle EventReadByType command.
pub fn event_read_by_type(
    p: &Arc<Primitives>,
    branch: BranchId,
    space: String,
    event_type: String,
    limit: Option<u64>,
    after_sequence: Option<u64>,
) -> Result<Output> {
    let core_branch_id = bridge::to_core_branch_id(&branch)?;
    let events = convert_result(p.event.read_by_type(&core_branch_id, &space, &event_type))?;

    // Apply after_sequence filter
    let filtered: Vec<_> = if let Some(after_seq) = after_sequence {
        events
            .into_iter()
            .filter(|e| {
                if let strata_core::Version::Sequence(seq) = e.version {
                    seq > after_seq
                } else {
                    true
                }
            })
            .collect()
    } else {
        events
    };

    // Apply limit
    let limited: Vec<_> = if let Some(lim) = limit {
        filtered.into_iter().take(lim as usize).collect()
    } else {
        filtered
    };

    let versioned: Vec<VersionedValue> = limited
        .into_iter()
        .map(|e| VersionedValue {
            value: e.value.payload.clone(),
            version: bridge::extract_version(&e.version),
            timestamp: strata_core::Timestamp::from_micros(e.value.timestamp).into(),
        })
        .collect();

    Ok(Output::VersionedValues(versioned))
}

/// Handle EventLen command.
pub fn event_len(p: &Arc<Primitives>, branch: BranchId, space: String) -> Result<Output> {
    let core_branch_id = bridge::to_core_branch_id(&branch)?;
    let count = convert_result(p.event.len(&core_branch_id, &space))?;
    Ok(Output::Uint(count))
}

#[cfg(test)]
mod tests {
    use super::*;
    use strata_core::Version;

    #[test]
    fn test_bridge_extract_version() {
        assert_eq!(bridge::extract_version(&Version::Sequence(42)), 42);
    }
}
