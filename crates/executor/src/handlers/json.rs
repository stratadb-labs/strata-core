//! JSON command handlers (MVP).
//!
//! This module implements handlers for the 4 MVP JSON commands.

use std::sync::Arc;

use strata_core::Value;

use crate::bridge::{
    extract_version, json_to_value, parse_path, to_core_branch_id, validate_key, validate_value,
    value_to_json, Primitives,
};
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

/// Handle JsonGetv command — get full version history for a JSON document.
pub fn json_getv(
    p: &Arc<Primitives>,
    branch: BranchId,
    space: String,
    key: String,
) -> Result<Output> {
    let branch_id = to_core_branch_id(&branch)?;
    convert_result(validate_key(&key))?;
    let result = convert_result(p.json.getv(&branch_id, &space, &key))?;
    let mapped = result
        .map(|history| {
            history
                .into_versions()
                .into_iter()
                .map(|v| {
                    let value = convert_result(json_to_value(v.value))?;
                    Ok(VersionedValue {
                        value,
                        version: extract_version(&v.version),
                        timestamp: v.timestamp.into(),
                    })
                })
                .collect::<Result<Vec<VersionedValue>>>()
        })
        .transpose()?;
    Ok(Output::VersionHistory(mapped))
}

// =============================================================================
// MVP Handlers (4)
// =============================================================================

/// Handle JsonSet command.
///
/// Auto-creation logic:
/// - If doc doesn't exist and path is root: create the document.
/// - If doc doesn't exist and path is non-root: create with empty object, then set at path.
/// - If doc exists: set at path.
pub fn json_set(
    p: &Arc<Primitives>,
    branch: BranchId,
    space: String,
    key: String,
    path: String,
    value: Value,
) -> Result<Output> {
    require_branch_exists(p, &branch)?;
    let branch_id = to_core_branch_id(&branch)?;
    convert_result(validate_key(&key))?;
    convert_result(validate_value(&value, &p.limits))?;

    let json_path = convert_result(parse_path(&path))?;
    let json_value = convert_result(value_to_json(value))?;

    // Single atomic transaction: checks existence, creates if needed, sets at path.
    // Produces exactly 1 WAL append (fixes #973).
    let version = convert_result(
        p.json
            .set_or_create(&branch_id, &space, &key, &json_path, json_value),
    )?;

    // Best-effort auto-embed: read back the full document so we embed the complete
    // content, not just the fragment written at this path.
    embed_full_doc(p, branch_id, &space, &key);

    Ok(Output::Version(extract_version(&version)))
}

/// Handle JsonGet command.
///
/// Returns `MaybeVersioned` with value, version, and timestamp metadata.
pub fn json_get(
    p: &Arc<Primitives>,
    branch: BranchId,
    space: String,
    key: String,
    path: String,
) -> Result<Output> {
    let branch_id = to_core_branch_id(&branch)?;
    convert_result(validate_key(&key))?;
    let json_path = convert_result(parse_path(&path))?;

    let result = convert_result(p.json.get_versioned(&branch_id, &space, &key, &json_path))?;
    match result {
        Some(versioned) => {
            let value = convert_result(json_to_value(versioned.value))?;
            Ok(Output::MaybeVersioned(Some(VersionedValue {
                value,
                version: extract_version(&versioned.version),
                timestamp: versioned.timestamp.into(),
            })))
        }
        None => Ok(Output::MaybeVersioned(None)),
    }
}

/// Handle JsonGet with as_of timestamp (time-travel read).
pub fn json_get_at(
    p: &Arc<Primitives>,
    branch: BranchId,
    space: String,
    key: String,
    path: String,
    as_of_ts: u64,
) -> Result<Output> {
    let branch_id = to_core_branch_id(&branch)?;
    convert_result(validate_key(&key))?;
    let json_path = convert_result(parse_path(&path))?;

    let result = convert_result(
        p.json
            .get_at(&branch_id, &space, &key, &json_path, as_of_ts),
    )?;
    match result {
        Some(json_val) => {
            let value = convert_result(json_to_value(json_val))?;
            Ok(Output::Maybe(Some(value)))
        }
        None => Ok(Output::Maybe(None)),
    }
}

/// Handle JsonDelete command.
///
/// - Root path: destroy entire document (returns 1 if existed, 0 otherwise).
/// - Non-root path: delete at path (returns 1).
pub fn json_delete(
    p: &Arc<Primitives>,
    branch: BranchId,
    space: String,
    key: String,
    path: String,
) -> Result<Output> {
    require_branch_exists(p, &branch)?;
    let branch_id = to_core_branch_id(&branch)?;
    convert_result(validate_key(&key))?;
    let json_path = convert_result(parse_path(&path))?;

    if json_path.is_root() {
        let deleted = convert_result(p.json.destroy(&branch_id, &space, &key))?;

        // Best-effort remove shadow embedding when entire document is destroyed
        if deleted {
            super::embed_hook::maybe_remove_embedding(
                p,
                branch_id,
                &space,
                super::embed_hook::SHADOW_JSON,
                &key,
            );
        }

        Ok(Output::Uint(if deleted { 1 } else { 0 }))
    } else {
        match p.json.delete_at_path(&branch_id, &space, &key, &json_path) {
            Ok(_) => {
                // Re-embed the remaining document after sub-path deletion
                embed_full_doc(p, branch_id, &space, &key);
                Ok(Output::Uint(1))
            }
            Err(e) => {
                // If path not found, return 0 (nothing deleted)
                let err = crate::Error::from(e);
                match err {
                    crate::Error::InvalidPath { .. } | crate::Error::InvalidInput { .. } => {
                        Ok(Output::Uint(0))
                    }
                    other => Err(other),
                }
            }
        }
    }
}

/// Handle JsonList command.
pub fn json_list(
    p: &Arc<Primitives>,
    branch: BranchId,
    space: String,
    prefix: Option<String>,
    cursor: Option<String>,
    limit: u64,
) -> Result<Output> {
    let branch_id = to_core_branch_id(&branch)?;

    let result = convert_result(p.json.list(
        &branch_id,
        &space,
        prefix.as_deref(),
        cursor.as_deref(),
        limit as usize,
    ))?;

    Ok(Output::JsonListResult {
        keys: result.doc_ids,
        cursor: result.next_cursor,
    })
}

/// Best-effort: read back the full JSON document and embed its complete text.
///
/// This ensures that partial-path writes (e.g. `$.name`) produce an embedding
/// that reflects the entire document, not just the written fragment.
fn embed_full_doc(
    p: &Arc<Primitives>,
    branch_id: strata_core::types::BranchId,
    space: &str,
    key: &str,
) {
    use strata_core::primitives::json::JsonPath;

    let full_doc = p.json.get(&branch_id, space, key, &JsonPath::root());
    match full_doc {
        Ok(Some(json_val)) => {
            if let Ok(value) = json_to_value(json_val) {
                if let Some(text) = super::embed_hook::extract_text(&value) {
                    super::embed_hook::maybe_embed_text(
                        p,
                        branch_id,
                        space,
                        super::embed_hook::SHADOW_JSON,
                        key,
                        &text,
                        strata_core::EntityRef::json(branch_id, key),
                    );
                }
            }
        }
        Ok(None) => {}
        Err(e) => {
            tracing::warn!(
                target: "strata::embed",
                key = key,
                error = %e,
                "Failed to read back document for embedding"
            );
        }
    }
}

/// Handle JsonList with as_of timestamp (time-travel list).
///
/// Returns only document IDs that existed at or before the given timestamp.
/// Does not support cursor-based pagination (returns all matching docs).
pub fn json_list_at(
    p: &Arc<Primitives>,
    branch: BranchId,
    space: String,
    prefix: Option<String>,
    as_of_ts: u64,
) -> Result<Output> {
    let branch_id = to_core_branch_id(&branch)?;
    let keys = convert_result(
        p.json
            .list_at(&branch_id, &space, prefix.as_deref(), as_of_ts),
    )?;
    Ok(Output::JsonListResult { keys, cursor: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_core_branch_id_default() {
        let branch = BranchId::from("default");
        let core_id = to_core_branch_id(&branch).unwrap();
        assert_eq!(core_id.as_bytes(), &[0u8; 16]);
    }

    #[test]
    fn test_extract_version() {
        use strata_core::Version;
        assert_eq!(extract_version(&Version::Txn(42)), 42);
        assert_eq!(extract_version(&Version::Counter(100)), 100);
    }
}
