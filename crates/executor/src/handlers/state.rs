//! State command handlers.
//!
//! This module implements handlers for the 4 MVP State commands:
//! - StateSet: Unconditional write
//! - StateGet: Read current state
//! - StateCas: Compare-and-swap
//! - StateInit: Initialize if not exists

use std::sync::Arc;

use strata_core::{Value, Version};

use crate::bridge::{self, validate_value, Primitives};
use crate::convert::convert_result;
use crate::types::BranchId;
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

/// Handle StateGetv command — get full version history for a state cell.
pub fn state_getv(
    p: &Arc<Primitives>,
    branch: BranchId,
    space: String,
    cell: String,
) -> Result<Output> {
    let branch_id = bridge::to_core_branch_id(&branch)?;
    convert_result(bridge::validate_key(&cell))?;
    let result = convert_result(p.state.getv(&branch_id, &space, &cell))?;
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
    branch: BranchId,
    space: String,
    cell: String,
    value: Value,
) -> Result<Output> {
    require_branch_exists(p, &branch)?;
    let branch_id = bridge::to_core_branch_id(&branch)?;
    convert_result(bridge::validate_key(&cell))?;
    convert_result(validate_value(&value, &p.limits))?;

    // Extract text before value is consumed
    let text = super::embed_hook::extract_text(&value);

    let version = convert_result(p.state.set(&branch_id, &space, &cell, value))?;

    // Best-effort auto-embed after successful write
    if let Some(ref text) = text {
        super::embed_hook::maybe_embed_text(
            p,
            branch_id,
            &space,
            super::embed_hook::SHADOW_STATE,
            &cell,
            text,
            strata_core::EntityRef::state(branch_id, &cell),
        );
    }

    Ok(Output::Version(bridge::extract_version(&version)))
}

/// Handle StateGet command.
///
/// Returns `MaybeVersioned` with value, version, and timestamp metadata.
pub fn state_get(
    p: &Arc<Primitives>,
    branch: BranchId,
    space: String,
    cell: String,
) -> Result<Output> {
    let branch_id = bridge::to_core_branch_id(&branch)?;
    convert_result(bridge::validate_key(&cell))?;
    let result = convert_result(p.state.get_versioned(&branch_id, &space, &cell))?;
    Ok(Output::MaybeVersioned(
        result.map(bridge::to_versioned_value),
    ))
}

/// Handle StateCas command.
pub fn state_cas(
    p: &Arc<Primitives>,
    branch: BranchId,
    space: String,
    cell: String,
    expected_counter: Option<u64>,
    value: Value,
) -> Result<Output> {
    require_branch_exists(p, &branch)?;
    let branch_id = bridge::to_core_branch_id(&branch)?;
    convert_result(bridge::validate_key(&cell))?;
    convert_result(validate_value(&value, &p.limits))?;

    // Extract text before value is consumed
    let text = super::embed_hook::extract_text(&value);

    let result = match expected_counter {
        None => {
            // Init semantics: create only if cell doesn't exist.
            // Check existence first since init() is idempotent.
            if convert_result(p.state.get(&branch_id, &space, &cell))?.is_some() {
                return Ok(Output::MaybeVersion(None));
            }
            match p.state.init(&branch_id, &space, &cell, value) {
                Ok(version) => Ok(Output::MaybeVersion(Some(bridge::extract_version(
                    &version,
                )))),
                Err(e) => {
                    let err = crate::Error::from(e);
                    match err {
                        crate::Error::VersionConflict { .. } | crate::Error::Conflict { .. } => {
                            Ok(Output::MaybeVersion(None))
                        }
                        other => Err(other),
                    }
                }
            }
        }
        Some(expected) => {
            match p
                .state
                .cas(&branch_id, &space, &cell, Version::Counter(expected), value)
            {
                Ok(version) => Ok(Output::MaybeVersion(Some(bridge::extract_version(
                    &version,
                )))),
                Err(e) => {
                    let err = crate::Error::from(e);
                    match err {
                        crate::Error::VersionConflict { .. } | crate::Error::Conflict { .. } => {
                            Ok(Output::MaybeVersion(None))
                        }
                        other => Err(other),
                    }
                }
            }
        }
    };

    // Best-effort auto-embed only if the CAS/init actually succeeded (returned a version)
    if let Ok(Output::MaybeVersion(Some(_))) = &result {
        if let Some(ref text) = text {
            super::embed_hook::maybe_embed_text(
                p,
                branch_id,
                &space,
                super::embed_hook::SHADOW_STATE,
                &cell,
                text,
                strata_core::EntityRef::state(branch_id, &cell),
            );
        }
    }

    result
}

/// Handle StateInit command.
pub fn state_init(
    p: &Arc<Primitives>,
    branch: BranchId,
    space: String,
    cell: String,
    value: Value,
) -> Result<Output> {
    require_branch_exists(p, &branch)?;
    let branch_id = bridge::to_core_branch_id(&branch)?;
    convert_result(bridge::validate_key(&cell))?;
    convert_result(validate_value(&value, &p.limits))?;

    // Extract text before value is consumed
    let text = super::embed_hook::extract_text(&value);

    let version = convert_result(p.state.init(&branch_id, &space, &cell, value))?;

    // Best-effort auto-embed after successful write
    if let Some(ref text) = text {
        super::embed_hook::maybe_embed_text(
            p,
            branch_id,
            &space,
            super::embed_hook::SHADOW_STATE,
            &cell,
            text,
            strata_core::EntityRef::state(branch_id, &cell),
        );
    }

    Ok(Output::Version(bridge::extract_version(&version)))
}

/// Handle StateDelete command.
pub fn state_delete(
    p: &Arc<Primitives>,
    branch: BranchId,
    space: String,
    cell: String,
) -> Result<Output> {
    require_branch_exists(p, &branch)?;
    let branch_id = bridge::to_core_branch_id(&branch)?;
    convert_result(bridge::validate_key(&cell))?;
    let existed = convert_result(p.state.delete(&branch_id, &space, &cell))?;

    // Best-effort remove shadow embedding
    if existed {
        super::embed_hook::maybe_remove_embedding(
            p,
            branch_id,
            &space,
            super::embed_hook::SHADOW_STATE,
            &cell,
        );
    }

    Ok(Output::Bool(existed))
}

/// Handle StateList command.
pub fn state_list(
    p: &Arc<Primitives>,
    branch: BranchId,
    space: String,
    prefix: Option<String>,
) -> Result<Output> {
    let branch_id = bridge::to_core_branch_id(&branch)?;
    if let Some(ref pfx) = prefix {
        if !pfx.is_empty() {
            convert_result(bridge::validate_key(pfx))?;
        }
    }
    let keys = convert_result(p.state.list(&branch_id, &space, prefix.as_deref()))?;
    Ok(Output::Keys(keys))
}
