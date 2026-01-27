//! State command handlers.
//!
//! This module implements handlers for the 8 serializable State commands.
//! Note: state_transition, state_transition_or_init, and state_read_or_init
//! are excluded from the executor as they require closures.

use std::sync::Arc;

use strata_core::{Value, Version, Versioned};

use crate::bridge::{self, Primitives};
use crate::convert::convert_result;
use crate::types::{RunId, VersionedValue};
use crate::{Output, Result};

// =============================================================================
// Individual Handlers
// =============================================================================

/// Handle StateSet command.
pub fn state_set(
    p: &Arc<Primitives>,
    run: RunId,
    cell: String,
    value: Value,
) -> Result<Output> {
    let run_id = bridge::to_core_run_id(&run)?;
    convert_result(bridge::validate_key(&cell))?;
    let versioned = convert_result(p.state.set(&run_id, &cell, value))?;
    Ok(Output::Version(bridge::extract_version(&versioned.version)))
}

/// Handle StateRead command.
pub fn state_read(
    p: &Arc<Primitives>,
    run: RunId,
    cell: String,
) -> Result<Output> {
    let run_id = bridge::to_core_run_id(&run)?;
    convert_result(bridge::validate_key(&cell))?;
    let result = convert_result(p.state.read(&run_id, &cell))?;
    let mapped = result.map(|state| {
        let combined = Versioned {
            value: state.value.value,
            version: state.version,
            timestamp: state.timestamp,
        };
        bridge::to_versioned_value(combined)
    });
    Ok(Output::MaybeVersioned(mapped))
}

/// Handle StateCas command.
pub fn state_cas(
    p: &Arc<Primitives>,
    run: RunId,
    cell: String,
    expected_counter: Option<u64>,
    value: Value,
) -> Result<Output> {
    let run_id = bridge::to_core_run_id(&run)?;
    convert_result(bridge::validate_key(&cell))?;
    match expected_counter {
        None => {
            // Init semantics: create only if cell doesn't exist.
            match p.state.init(&run_id, &cell, value) {
                Ok(versioned) => Ok(Output::MaybeVersion(Some(bridge::extract_version(&versioned.version)))),
                Err(_) => Ok(Output::MaybeVersion(None)),
            }
        }
        Some(expected) => {
            match p.state.cas(&run_id, &cell, Version::Counter(expected), value) {
                Ok(versioned) => Ok(Output::MaybeVersion(Some(bridge::extract_version(&versioned.version)))),
                Err(_) => Ok(Output::MaybeVersion(None)),
            }
        }
    }
}

/// Handle StateDelete command.
pub fn state_delete(
    p: &Arc<Primitives>,
    run: RunId,
    cell: String,
) -> Result<Output> {
    let run_id = bridge::to_core_run_id(&run)?;
    convert_result(bridge::validate_key(&cell))?;
    let existed = convert_result(p.state.delete(&run_id, &cell))?;
    Ok(Output::Bool(existed))
}

/// Handle StateExists command.
pub fn state_exists(
    p: &Arc<Primitives>,
    run: RunId,
    cell: String,
) -> Result<Output> {
    let run_id = bridge::to_core_run_id(&run)?;
    convert_result(bridge::validate_key(&cell))?;
    let exists = convert_result(p.state.exists(&run_id, &cell))?;
    Ok(Output::Bool(exists))
}

/// Handle StateHistory command.
pub fn state_history(
    p: &Arc<Primitives>,
    run: RunId,
    cell: String,
    limit: Option<u64>,
    before: Option<u64>,
) -> Result<Output> {
    let run_id = bridge::to_core_run_id(&run)?;
    convert_result(bridge::validate_key(&cell))?;
    let limit_usize = limit.map(|l| l as usize);
    let history = convert_result(p.state.history(&run_id, &cell, limit_usize, before))?;
    let values: Vec<VersionedValue> = history.into_iter().map(bridge::to_versioned_value).collect();
    Ok(Output::VersionedValues(values))
}

/// Handle StateInit command.
pub fn state_init(
    p: &Arc<Primitives>,
    run: RunId,
    cell: String,
    value: Value,
) -> Result<Output> {
    let run_id = bridge::to_core_run_id(&run)?;
    convert_result(bridge::validate_key(&cell))?;
    let versioned = convert_result(p.state.init(&run_id, &cell, value))?;
    Ok(Output::Version(bridge::extract_version(&versioned.version)))
}

/// Handle StateList command.
pub fn state_list(p: &Arc<Primitives>, run: RunId) -> Result<Output> {
    let run_id = bridge::to_core_run_id(&run)?;
    let cells = convert_result(p.state.list(&run_id))?;
    Ok(Output::Strings(cells))
}
