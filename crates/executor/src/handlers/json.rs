//! JSON command handlers (MVP).
//!
//! This module implements handlers for the 4 MVP JSON commands.

use std::sync::Arc;

use strata_core::Value;

use crate::bridge::{
    extract_version, json_to_value, parse_path, to_core_branch_id, validate_key, value_to_json,
    Primitives,
};
use crate::convert::convert_result;
use crate::types::{BranchId, VersionedValue};
use crate::{Output, Result};

/// Handle JsonGetv command — get full version history for a JSON document.
pub fn json_getv(
    p: &Arc<Primitives>,
    run: BranchId,
    key: String,
) -> Result<Output> {
    let branch_id = to_core_branch_id(&run)?;
    convert_result(validate_key(&key))?;
    let result = convert_result(p.json.getv(&branch_id, &key))?;
    let mapped = result.map(|history| {
        history
            .into_versions()
            .into_iter()
            .filter_map(|v| {
                let value = convert_result(json_to_value(v.value)).ok()?;
                Some(VersionedValue {
                    value,
                    version: extract_version(&v.version),
                    timestamp: v.timestamp.into(),
                })
            })
            .collect()
    });
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
    run: BranchId,
    key: String,
    path: String,
    value: Value,
) -> Result<Output> {
    let branch_id = to_core_branch_id(&run)?;
    convert_result(validate_key(&key))?;
    let json_path = convert_result(parse_path(&path))?;
    let json_value = convert_result(value_to_json(value))?;

    let exists = convert_result(p.json.exists(&branch_id, &key))?;

    let version = if !exists && json_path.is_root() {
        // Document doesn't exist and path is root — create it.
        convert_result(p.json.create(&branch_id, &key, json_value))?
    } else if !exists {
        // Document doesn't exist and path is non-root — create empty object first, then set.
        let empty_obj = convert_result(value_to_json(Value::Object(Default::default())))?;
        convert_result(p.json.create(&branch_id, &key, empty_obj))?;
        convert_result(p.json.set(&branch_id, &key, &json_path, json_value))?
    } else {
        // Document exists — set at path.
        convert_result(p.json.set(&branch_id, &key, &json_path, json_value))?
    };

    Ok(Output::Version(extract_version(&version)))
}

/// Handle JsonGet command.
pub fn json_get(
    p: &Arc<Primitives>,
    run: BranchId,
    key: String,
    path: String,
) -> Result<Output> {
    let branch_id = to_core_branch_id(&run)?;
    convert_result(validate_key(&key))?;
    let json_path = convert_result(parse_path(&path))?;

    let result = convert_result(p.json.get(&branch_id, &key, &json_path))?;
    let mapped = result
        .map(|v| convert_result(json_to_value(v)))
        .transpose()?;
    Ok(Output::Maybe(mapped))
}

/// Handle JsonDelete command.
///
/// - Root path: destroy entire document (returns 1 if existed, 0 otherwise).
/// - Non-root path: delete at path (returns 1).
pub fn json_delete(
    p: &Arc<Primitives>,
    run: BranchId,
    key: String,
    path: String,
) -> Result<Output> {
    let branch_id = to_core_branch_id(&run)?;
    convert_result(validate_key(&key))?;
    let json_path = convert_result(parse_path(&path))?;

    if json_path.is_root() {
        let deleted = convert_result(p.json.destroy(&branch_id, &key))?;
        Ok(Output::Uint(if deleted { 1 } else { 0 }))
    } else {
        convert_result(p.json.delete_at_path(&branch_id, &key, &json_path))?;
        Ok(Output::Uint(1))
    }
}

/// Handle JsonList command.
pub fn json_list(
    p: &Arc<Primitives>,
    run: BranchId,
    prefix: Option<String>,
    cursor: Option<String>,
    limit: u64,
) -> Result<Output> {
    let branch_id = to_core_branch_id(&run)?;

    let result = convert_result(p.json.list(
        &branch_id,
        prefix.as_deref(),
        cursor.as_deref(),
        limit as usize,
    ))?;

    Ok(Output::JsonListResult {
        keys: result.doc_ids,
        cursor: result.next_cursor,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_core_run_id_default() {
        let run = BranchId::from("default");
        let core_id = to_core_branch_id(&run).unwrap();
        assert_eq!(core_id.as_bytes(), &[0u8; 16]);
    }

    #[test]
    fn test_extract_version() {
        use strata_core::Version;
        assert_eq!(extract_version(&Version::Txn(42)), 42);
        assert_eq!(extract_version(&Version::Counter(100)), 100);
    }
}
