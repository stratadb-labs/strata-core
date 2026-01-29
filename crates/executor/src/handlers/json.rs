//! JSON command handlers (MVP).
//!
//! This module implements handlers for the 4 MVP JSON commands.

use std::sync::Arc;

use strata_core::{Value, Versioned};

use crate::bridge::{
    extract_version, json_to_value, parse_path, to_core_run_id, validate_key, value_to_json,
    Primitives,
};
use crate::convert::convert_result;
use crate::types::{RunId, VersionedValue};
use crate::{Output, Result};

// =============================================================================
// Helpers
// =============================================================================

/// Convert a `Versioned<JsonValue>` from the JSON primitive into a `VersionedValue`
/// (which wraps `strata_core::Value`).
fn json_versioned_to_versioned_value(
    v: Versioned<strata_core::primitives::json::JsonValue>,
) -> Result<VersionedValue> {
    let value = convert_result(json_to_value(v.value))?;
    Ok(VersionedValue {
        value,
        version: extract_version(&v.version),
        timestamp: v.timestamp.into(),
    })
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
    run: RunId,
    key: String,
    path: String,
    value: Value,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_key(&key))?;
    let json_path = convert_result(parse_path(&path))?;
    let json_value = convert_result(value_to_json(value))?;

    let exists = convert_result(p.json.exists(&run_id, &key))?;

    let version = if !exists && json_path.is_root() {
        // Document doesn't exist and path is root — create it.
        convert_result(p.json.create(&run_id, &key, json_value))?
    } else if !exists {
        // Document doesn't exist and path is non-root — create empty object first, then set.
        let empty_obj = convert_result(value_to_json(Value::Object(Default::default())))?;
        convert_result(p.json.create(&run_id, &key, empty_obj))?;
        convert_result(p.json.set(&run_id, &key, &json_path, json_value))?
    } else {
        // Document exists — set at path.
        convert_result(p.json.set(&run_id, &key, &json_path, json_value))?
    };

    Ok(Output::Version(extract_version(&version)))
}

/// Handle JsonGet command.
pub fn json_get(
    p: &Arc<Primitives>,
    run: RunId,
    key: String,
    path: String,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_key(&key))?;
    let json_path = convert_result(parse_path(&path))?;

    let result = convert_result(p.json.get(&run_id, &key, &json_path))?;
    let versioned = result
        .map(|v| json_versioned_to_versioned_value(v))
        .transpose()?;
    Ok(Output::MaybeVersioned(versioned))
}

/// Handle JsonDelete command.
///
/// - Root path: destroy entire document (returns 1 if existed, 0 otherwise).
/// - Non-root path: delete at path (returns 1).
pub fn json_delete(
    p: &Arc<Primitives>,
    run: RunId,
    key: String,
    path: String,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_key(&key))?;
    let json_path = convert_result(parse_path(&path))?;

    if json_path.is_root() {
        let deleted = convert_result(p.json.destroy(&run_id, &key))?;
        Ok(Output::Uint(if deleted { 1 } else { 0 }))
    } else {
        convert_result(p.json.delete_at_path(&run_id, &key, &json_path))?;
        Ok(Output::Uint(1))
    }
}

/// Handle JsonList command.
pub fn json_list(
    p: &Arc<Primitives>,
    run: RunId,
    prefix: Option<String>,
    cursor: Option<String>,
    limit: u64,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;

    let result = convert_result(p.json.list(
        &run_id,
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
        let run = RunId::from("default");
        let core_id = to_core_run_id(&run).unwrap();
        assert_eq!(core_id.as_bytes(), &[0u8; 16]);
    }

    #[test]
    fn test_extract_version() {
        use strata_core::Version;
        assert_eq!(extract_version(&Version::Txn(42)), 42);
        assert_eq!(extract_version(&Version::Counter(100)), 100);
    }
}
