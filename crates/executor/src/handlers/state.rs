//! State command handlers.
//!
//! This module implements handlers for the 4 MVP State commands:
//! - StateSet: Unconditional write
//! - StateRead: Read current state
//! - StateCas: Compare-and-swap
//! - StateInit: Initialize if not exists

use std::sync::Arc;

use strata_core::{Value, Version};

use crate::bridge::{self, Primitives};
use crate::convert::convert_result;
use crate::types::BranchId;
use crate::{Output, Result};

/// Handle StateReadv command — get full version history for a state cell.
pub fn state_readv(
    p: &Arc<Primitives>,
    run: BranchId,
    cell: String,
) -> Result<Output> {
    let branch_id = bridge::to_core_branch_id(&run)?;
    convert_result(bridge::validate_key(&cell))?;
    let result = convert_result(p.state.readv(&branch_id, &cell))?;
    let mapped = result.map(|history| {
        history
            .into_versions()
            .into_iter()
            .map(bridge::to_versioned_value)
            .collect()
    });
    Ok(Output::VersionHistory(mapped))
}

// =============================================================================
// Individual Handlers
// =============================================================================

/// Handle StateSet command.
pub fn state_set(
    p: &Arc<Primitives>,
    run: BranchId,
    cell: String,
    value: Value,
) -> Result<Output> {
    let branch_id = bridge::to_core_branch_id(&run)?;
    convert_result(bridge::validate_key(&cell))?;
    let versioned = convert_result(p.state.set(&branch_id, &cell, value))?;
    Ok(Output::Version(bridge::extract_version(&versioned.version)))
}

/// Handle StateRead command.
pub fn state_read(
    p: &Arc<Primitives>,
    run: BranchId,
    cell: String,
) -> Result<Output> {
    let branch_id = bridge::to_core_branch_id(&run)?;
    convert_result(bridge::validate_key(&cell))?;
    let result = convert_result(p.state.read(&branch_id, &cell))?;
    Ok(Output::Maybe(result))
}

/// Handle StateCas command.
pub fn state_cas(
    p: &Arc<Primitives>,
    run: BranchId,
    cell: String,
    expected_counter: Option<u64>,
    value: Value,
) -> Result<Output> {
    let branch_id = bridge::to_core_branch_id(&run)?;
    convert_result(bridge::validate_key(&cell))?;
    match expected_counter {
        None => {
            // Init semantics: create only if cell doesn't exist.
            match p.state.init(&branch_id, &cell, value) {
                Ok(versioned) => Ok(Output::MaybeVersion(Some(bridge::extract_version(&versioned.version)))),
                Err(_) => Ok(Output::MaybeVersion(None)),
            }
        }
        Some(expected) => {
            match p.state.cas(&branch_id, &cell, Version::Counter(expected), value) {
                Ok(versioned) => Ok(Output::MaybeVersion(Some(bridge::extract_version(&versioned.version)))),
                Err(_) => Ok(Output::MaybeVersion(None)),
            }
        }
    }
}

/// Handle StateInit command.
pub fn state_init(
    p: &Arc<Primitives>,
    run: BranchId,
    cell: String,
    value: Value,
) -> Result<Output> {
    let branch_id = bridge::to_core_branch_id(&run)?;
    convert_result(bridge::validate_key(&cell))?;
    let versioned = convert_result(p.state.init(&branch_id, &cell, value))?;
    Ok(Output::Version(bridge::extract_version(&versioned.version)))
}
