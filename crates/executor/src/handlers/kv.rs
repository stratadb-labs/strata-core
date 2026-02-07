//! KV command handlers.
//!
//! This module implements handlers for the 4 MVP KV commands by dispatching
//! directly to engine primitives via `bridge::Primitives`.

use std::sync::Arc;

use strata_core::Value;

use crate::bridge::{
    extract_version, to_core_branch_id, to_versioned_value, validate_key, validate_value,
    Primitives,
};
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

/// Handle KvGetv command — get full version history for a key.
pub fn kv_getv(
    p: &Arc<Primitives>,
    branch: BranchId,
    space: String,
    key: String,
) -> Result<Output> {
    let branch_id = to_core_branch_id(&branch)?;
    convert_result(validate_key(&key))?;
    let result = convert_result(p.kv.getv(&branch_id, &space, &key))?;
    let mapped = result.map(|history| {
        history
            .into_versions()
            .into_iter()
            .map(to_versioned_value)
            .collect()
    });
    Ok(Output::VersionHistory(mapped))
}

// =============================================================================
// MVP Handlers (4 commands)
// =============================================================================

/// Handle KvPut command.
pub fn kv_put(
    p: &Arc<Primitives>,
    branch: BranchId,
    space: String,
    key: String,
    value: Value,
) -> Result<Output> {
    require_branch_exists(p, &branch)?;
    let branch_id = to_core_branch_id(&branch)?;
    convert_result(validate_key(&key))?;
    convert_result(validate_value(&value, &p.limits))?;

    // Extract text before the value is consumed by put()
    let text = super::embed_hook::extract_text(&value);

    let version = convert_result(p.kv.put(&branch_id, &space, &key, value))?;

    // Best-effort auto-embed after successful write
    if let Some(ref text) = text {
        super::embed_hook::maybe_embed_text(
            p,
            branch_id,
            &space,
            super::embed_hook::SHADOW_KV,
            &key,
            text,
            strata_core::EntityRef::kv(branch_id, &key),
        );
    }

    Ok(Output::Version(extract_version(&version)))
}

/// Handle KvGet command.
///
/// Returns `MaybeVersioned` with value, version, and timestamp metadata.
pub fn kv_get(p: &Arc<Primitives>, branch: BranchId, space: String, key: String) -> Result<Output> {
    let branch_id = to_core_branch_id(&branch)?;
    convert_result(validate_key(&key))?;
    let result = convert_result(p.kv.get_versioned(&branch_id, &space, &key))?;
    Ok(Output::MaybeVersioned(result.map(to_versioned_value)))
}

/// Handle KvDelete command.
pub fn kv_delete(
    p: &Arc<Primitives>,
    branch: BranchId,
    space: String,
    key: String,
) -> Result<Output> {
    require_branch_exists(p, &branch)?;
    let branch_id = to_core_branch_id(&branch)?;
    convert_result(validate_key(&key))?;
    let existed = convert_result(p.kv.delete(&branch_id, &space, &key))?;

    // Best-effort remove shadow embedding
    if existed {
        super::embed_hook::maybe_remove_embedding(
            p,
            branch_id,
            &space,
            super::embed_hook::SHADOW_KV,
            &key,
        );
    }

    Ok(Output::Bool(existed))
}

/// Handle KvList command.
pub fn kv_list(
    p: &Arc<Primitives>,
    branch: BranchId,
    space: String,
    prefix: Option<String>,
    cursor: Option<String>,
    limit: Option<u64>,
) -> Result<Output> {
    let branch_id = to_core_branch_id(&branch)?;
    if let Some(ref pfx) = prefix {
        if !pfx.is_empty() {
            convert_result(validate_key(pfx))?;
        }
    }
    let keys = convert_result(p.kv.list(&branch_id, &space, prefix.as_deref()))?;

    // Apply cursor-based pagination if limit is present
    if let Some(lim) = limit {
        let start_idx = if let Some(ref cur) = cursor {
            keys.iter().position(|k| k > cur).unwrap_or(keys.len())
        } else {
            0
        };
        let end_idx = std::cmp::min(start_idx + lim as usize, keys.len());
        let page = keys[start_idx..end_idx].to_vec();
        Ok(Output::Keys(page))
    } else {
        Ok(Output::Keys(keys))
    }
}
