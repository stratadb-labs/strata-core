//! State command handlers.
//!
//! This module implements handlers for the 8 serializable State commands.
//! Note: state_transition, state_transition_or_init, and state_get_or_init
//! are excluded from the executor as they require closures.

use std::sync::Arc;

use strata_api::substrate::{ApiRunId, StateCell, SubstrateImpl};
use strata_core::{Value, Version, Versioned};

use crate::convert::convert_result;
use crate::types::{RunId, VersionedValue};
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

// =============================================================================
// Individual Handlers
// =============================================================================

/// Handle StateSet command.
pub fn state_set(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    cell: String,
    value: Value,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let version = convert_result(substrate.state_set(&api_run, &cell, value))?;
    Ok(Output::Version(extract_version(&version)))
}

/// Handle StateGet command.
pub fn state_get(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    cell: String,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let result = convert_result(substrate.state_get(&api_run, &cell))?;
    Ok(Output::MaybeVersioned(result.map(to_versioned_value)))
}

/// Handle StateCas command.
pub fn state_cas(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    cell: String,
    expected_counter: Option<u64>,
    value: Value,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let result = convert_result(substrate.state_cas(&api_run, &cell, expected_counter, value))?;
    Ok(Output::MaybeVersion(result.map(|v| extract_version(&v))))
}

/// Handle StateDelete command.
pub fn state_delete(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    cell: String,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let existed = convert_result(substrate.state_delete(&api_run, &cell))?;
    Ok(Output::Bool(existed))
}

/// Handle StateExists command.
pub fn state_exists(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    cell: String,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let exists = convert_result(substrate.state_exists(&api_run, &cell))?;
    Ok(Output::Bool(exists))
}

/// Handle StateHistory command.
pub fn state_history(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    cell: String,
    limit: Option<u64>,
    before: Option<u64>,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let before_version = before.map(Version::Counter);
    let history = convert_result(substrate.state_history(&api_run, &cell, limit, before_version))?;
    let values: Vec<VersionedValue> = history.into_iter().map(to_versioned_value).collect();
    Ok(Output::VersionedValues(values))
}

/// Handle StateInit command.
pub fn state_init(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    cell: String,
    value: Value,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let version = convert_result(substrate.state_init(&api_run, &cell, value))?;
    Ok(Output::Version(extract_version(&version)))
}

/// Handle StateList command.
pub fn state_list(substrate: &Arc<SubstrateImpl>, run: RunId) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let cells = convert_result(substrate.state_list(&api_run))?;
    Ok(Output::Strings(cells))
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
        assert_eq!(extract_version(&Version::Counter(42)), 42);
    }
}
