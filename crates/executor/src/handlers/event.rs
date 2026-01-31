//! Event command handlers (4 MVP).
//!
//! MVP: append, read, read_by_type, len

use std::sync::Arc;

use strata_core::Version;

use crate::bridge::{self, Primitives};
use crate::convert::convert_result;
use crate::types::{BranchId, VersionedValue};
use crate::{Output, Result};

// =============================================================================
// Individual Handlers (4 MVP)
// =============================================================================

/// Handle EventAppend command.
pub fn event_append(
    p: &Arc<Primitives>,
    run: BranchId,
    event_type: String,
    payload: strata_core::Value,
) -> Result<Output> {
    let core_run = bridge::to_core_branch_id(&run)?;
    let version = convert_result(p.event.append(&core_run, &event_type, payload))?;
    Ok(Output::Version(bridge::extract_version(&version)))
}

/// Handle EventRead command.
pub fn event_read(p: &Arc<Primitives>, run: BranchId, sequence: u64) -> Result<Output> {
    let core_run = bridge::to_core_branch_id(&run)?;
    let event = convert_result(p.event.read(&core_run, sequence))?;

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
    run: BranchId,
    event_type: String,
) -> Result<Output> {
    let core_run = bridge::to_core_branch_id(&run)?;
    let events = convert_result(p.event.read_by_type(&core_run, &event_type))?;

    let versioned: Vec<VersionedValue> = events
        .into_iter()
        .map(|e| VersionedValue {
            value: e.value.payload.clone(),
            version: match e.version {
                Version::Sequence(s) => s,
                _ => 0,
            },
            timestamp: strata_core::Timestamp::from_micros(e.value.timestamp).into(),
        })
        .collect();

    Ok(Output::VersionedValues(versioned))
}

/// Handle EventLen command.
pub fn event_len(p: &Arc<Primitives>, run: BranchId) -> Result<Output> {
    let core_run = bridge::to_core_branch_id(&run)?;
    let count = convert_result(p.event.len(&core_run))?;
    Ok(Output::Uint(count))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bridge_extract_version() {
        assert_eq!(bridge::extract_version(&Version::Sequence(42)), 42);
    }
}
