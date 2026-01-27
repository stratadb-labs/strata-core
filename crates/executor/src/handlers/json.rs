//! JSON command handlers.
//!
//! This module implements handlers for all JSON commands by dispatching
//! directly to engine primitives via `bridge::Primitives`.

use std::sync::Arc;

use strata_core::{SearchRequest, Value, Versioned};

use crate::bridge::{
    extract_version, json_to_value, parse_path, to_core_run_id, validate_key, value_to_json,
    Primitives,
};
use crate::convert::convert_result;
use crate::types::{JsonSearchHit, RunId, VersionedValue};
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
// Individual Handlers
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

/// Handle JsonMerge command.
pub fn json_merge(
    p: &Arc<Primitives>,
    run: RunId,
    key: String,
    path: String,
    patch: Value,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_key(&key))?;
    let json_path = convert_result(parse_path(&path))?;
    let patch_json = convert_result(value_to_json(patch))?;

    let version = convert_result(p.json.merge(&run_id, &key, &json_path, patch_json))?;
    Ok(Output::Version(extract_version(&version)))
}

/// Handle JsonHistory command.
pub fn json_history(
    p: &Arc<Primitives>,
    run: RunId,
    key: String,
    limit: Option<u64>,
    before: Option<u64>,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_key(&key))?;

    let history = convert_result(p.json.history(
        &run_id,
        &key,
        limit.map(|l| l as usize),
        before,
    ))?;

    // Convert Vec<Versioned<JsonDoc>> to Vec<VersionedValue>
    let values: Vec<VersionedValue> = history
        .into_iter()
        .map(|v| {
            let val = convert_result(json_to_value(v.value.value))?;
            Ok(VersionedValue {
                value: val,
                version: extract_version(&v.version),
                timestamp: v.timestamp.into(),
            })
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(Output::VersionedValues(values))
}

/// Handle JsonExists command.
pub fn json_exists(
    p: &Arc<Primitives>,
    run: RunId,
    key: String,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_key(&key))?;

    let exists = convert_result(p.json.exists(&run_id, &key))?;
    Ok(Output::Bool(exists))
}

/// Handle JsonGetVersion command.
pub fn json_get_version(
    p: &Arc<Primitives>,
    run: RunId,
    key: String,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_key(&key))?;

    let version = convert_result(p.json.get_version(&run_id, &key))?;
    Ok(Output::MaybeVersion(version))
}

/// Handle JsonSearch command.
pub fn json_search(
    p: &Arc<Primitives>,
    run: RunId,
    query: String,
    k: u64,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;

    let request = SearchRequest::new(run_id, &query).with_k(k as usize);
    let response = convert_result(p.json.search(&request))?;

    let search_hits: Vec<JsonSearchHit> = response
        .hits
        .into_iter()
        .map(|hit| JsonSearchHit {
            key: hit.doc_ref.to_string(),
            score: hit.score,
            highlights: vec![],
        })
        .collect();

    Ok(Output::JsonSearchHits(search_hits))
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

/// Handle JsonCas command.
pub fn json_cas(
    p: &Arc<Primitives>,
    run: RunId,
    key: String,
    expected_version: u64,
    path: String,
    value: Value,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_key(&key))?;
    let json_path = convert_result(parse_path(&path))?;
    let json_value = convert_result(value_to_json(value))?;

    let version = convert_result(p.json.cas(
        &run_id,
        &key,
        expected_version,
        &json_path,
        json_value,
    ))?;
    Ok(Output::Version(extract_version(&version)))
}

/// Handle JsonQuery command.
pub fn json_query(
    p: &Arc<Primitives>,
    run: RunId,
    path: String,
    value: Value,
    limit: u64,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    let json_path = convert_result(parse_path(&path))?;
    let json_value = convert_result(value_to_json(value))?;

    let keys = convert_result(p.json.query(&run_id, &json_path, &json_value, limit as usize))?;
    Ok(Output::Keys(keys))
}

/// Handle JsonCount command.
pub fn json_count(p: &Arc<Primitives>, run: RunId) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    let count = convert_result(p.json.count(&run_id))?;
    Ok(Output::Uint(count))
}

/// Handle JsonBatchGet command.
pub fn json_batch_get(
    p: &Arc<Primitives>,
    run: RunId,
    keys: Vec<String>,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    let key_refs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();

    let results = convert_result(p.json.batch_get(&run_id, &key_refs))?;

    // Convert Vec<Option<Versioned<JsonDoc>>> to Vec<Option<VersionedValue>>
    let values: Vec<Option<VersionedValue>> = results
        .into_iter()
        .map(|opt| {
            opt.map(|v| {
                let val = convert_result(json_to_value(v.value.value))?;
                Ok(VersionedValue {
                    value: val,
                    version: extract_version(&v.version),
                    timestamp: v.timestamp.into(),
                })
            })
            .transpose()
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(Output::Values(values))
}

/// Handle JsonBatchCreate command.
pub fn json_batch_create(
    p: &Arc<Primitives>,
    run: RunId,
    docs: Vec<(String, Value)>,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;

    let json_docs: Vec<(String, strata_core::primitives::json::JsonValue)> = docs
        .into_iter()
        .map(|(k, v)| {
            let jv = convert_result(value_to_json(v))?;
            Ok((k, jv))
        })
        .collect::<Result<Vec<_>>>()?;

    let versions = convert_result(p.json.batch_create(&run_id, json_docs))?;
    let version_nums: Vec<u64> = versions.into_iter().map(|v| extract_version(&v)).collect();
    Ok(Output::Versions(version_nums))
}

/// Handle JsonArrayPush command.
pub fn json_array_push(
    p: &Arc<Primitives>,
    run: RunId,
    key: String,
    path: String,
    values: Vec<Value>,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_key(&key))?;
    let json_path = convert_result(parse_path(&path))?;

    let json_values: Vec<strata_core::primitives::json::JsonValue> = values
        .into_iter()
        .map(|v| convert_result(value_to_json(v)))
        .collect::<Result<Vec<_>>>()?;

    let (_version, new_len) =
        convert_result(p.json.array_push(&run_id, &key, &json_path, json_values))?;
    Ok(Output::Uint(new_len as u64))
}

/// Handle JsonIncrement command.
pub fn json_increment(
    p: &Arc<Primitives>,
    run: RunId,
    key: String,
    path: String,
    delta: f64,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_key(&key))?;
    let json_path = convert_result(parse_path(&path))?;

    let (_version, new_value) =
        convert_result(p.json.increment(&run_id, &key, &json_path, delta))?;
    Ok(Output::Float(new_value))
}

/// Handle JsonArrayPop command.
pub fn json_array_pop(
    p: &Arc<Primitives>,
    run: RunId,
    key: String,
    path: String,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_key(&key))?;
    let json_path = convert_result(parse_path(&path))?;

    let (_version, popped_json) =
        convert_result(p.json.array_pop(&run_id, &key, &json_path))?;

    let popped = popped_json
        .map(|jv| convert_result(json_to_value(jv)))
        .transpose()?;
    Ok(Output::Maybe(popped))
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
