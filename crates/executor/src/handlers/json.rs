//! JSON command handlers.
//!
//! This module implements handlers for all 17 JSON commands by dispatching
//! to the SubstrateImpl's JsonStore trait methods.

use std::sync::Arc;

use strata_api::substrate::json::{JsonListResult as ApiJsonListResult, JsonSearchHit as ApiJsonSearchHit};
use strata_api::substrate::{ApiRunId, JsonStore, SubstrateImpl};
use strata_core::{Value, Version, Versioned};

use crate::convert::convert_result;
use crate::types::{JsonSearchHit, RunId, VersionedValue};
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

/// Convert API JsonSearchHit to executor JsonSearchHit.
fn to_search_hit(hit: ApiJsonSearchHit) -> JsonSearchHit {
    JsonSearchHit {
        key: hit.key,
        score: hit.score,
        highlights: vec![], // API doesn't provide highlights currently
    }
}

// =============================================================================
// Individual Handlers
// =============================================================================

/// Handle JsonSet command.
pub fn json_set(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    key: String,
    path: String,
    value: Value,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let version = convert_result(substrate.json_set(&api_run, &key, &path, value))?;
    Ok(Output::Version(extract_version(&version)))
}

/// Handle JsonGet command.
pub fn json_get(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    key: String,
    path: String,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let result = convert_result(substrate.json_get(&api_run, &key, &path))?;
    Ok(Output::MaybeVersioned(result.map(to_versioned_value)))
}

/// Handle JsonDelete command.
pub fn json_delete(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    key: String,
    path: String,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let count = convert_result(substrate.json_delete(&api_run, &key, &path))?;
    Ok(Output::Uint(count))
}

/// Handle JsonMerge command.
pub fn json_merge(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    key: String,
    path: String,
    patch: Value,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let version = convert_result(substrate.json_merge(&api_run, &key, &path, patch))?;
    Ok(Output::Version(extract_version(&version)))
}

/// Handle JsonHistory command.
pub fn json_history(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    key: String,
    limit: Option<u64>,
    before: Option<u64>,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let before_version = before.map(Version::Counter);
    let history = convert_result(substrate.json_history(&api_run, &key, limit, before_version))?;
    let values: Vec<VersionedValue> = history.into_iter().map(to_versioned_value).collect();
    Ok(Output::VersionedValues(values))
}

/// Handle JsonExists command.
pub fn json_exists(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    key: String,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let exists = convert_result(substrate.json_exists(&api_run, &key))?;
    Ok(Output::Bool(exists))
}

/// Handle JsonGetVersion command.
pub fn json_get_version(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    key: String,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let version = convert_result(substrate.json_get_version(&api_run, &key))?;
    Ok(Output::MaybeVersion(version))
}

/// Handle JsonSearch command.
pub fn json_search(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    query: String,
    k: u64,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let hits = convert_result(substrate.json_search(&api_run, &query, k))?;
    let search_hits: Vec<JsonSearchHit> = hits.into_iter().map(to_search_hit).collect();
    Ok(Output::JsonSearchHits(search_hits))
}

/// Handle JsonList command.
pub fn json_list(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    prefix: Option<String>,
    cursor: Option<String>,
    limit: u64,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let result: ApiJsonListResult = convert_result(substrate.json_list(
        &api_run,
        prefix.as_deref(),
        cursor.as_deref(),
        limit,
    ))?;
    Ok(Output::JsonListResult {
        keys: result.keys,
        cursor: result.next_cursor,
    })
}

/// Handle JsonCas command.
pub fn json_cas(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    key: String,
    expected_version: u64,
    path: String,
    value: Value,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let version = convert_result(substrate.json_cas(&api_run, &key, expected_version, &path, value))?;
    Ok(Output::Version(extract_version(&version)))
}

/// Handle JsonQuery command.
pub fn json_query(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    path: String,
    value: Value,
    limit: u64,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let keys = convert_result(substrate.json_query(&api_run, &path, value, limit))?;
    Ok(Output::Keys(keys))
}

/// Handle JsonCount command.
pub fn json_count(substrate: &Arc<SubstrateImpl>, run: RunId) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let count = convert_result(substrate.json_count(&api_run))?;
    Ok(Output::Uint(count))
}

/// Handle JsonBatchGet command.
pub fn json_batch_get(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    keys: Vec<String>,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let key_refs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();
    let results = convert_result(substrate.json_batch_get(&api_run, &key_refs))?;
    let values: Vec<Option<VersionedValue>> = results
        .into_iter()
        .map(|opt| opt.map(to_versioned_value))
        .collect();
    Ok(Output::Values(values))
}

/// Handle JsonBatchCreate command.
pub fn json_batch_create(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    docs: Vec<(String, Value)>,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let doc_refs: Vec<(&str, Value)> = docs
        .iter()
        .map(|(k, v): &(String, Value)| (k.as_str(), v.clone()))
        .collect();
    let versions = convert_result(substrate.json_batch_create(&api_run, doc_refs))?;
    let version_nums: Vec<u64> = versions.into_iter().map(|v| extract_version(&v)).collect();
    Ok(Output::Versions(version_nums))
}

/// Handle JsonArrayPush command.
pub fn json_array_push(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    key: String,
    path: String,
    values: Vec<Value>,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let new_len = convert_result(substrate.json_array_push(&api_run, &key, &path, values))?;
    Ok(Output::Uint(new_len as u64))
}

/// Handle JsonIncrement command.
pub fn json_increment(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    key: String,
    path: String,
    delta: f64,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let new_value = convert_result(substrate.json_increment(&api_run, &key, &path, delta))?;
    Ok(Output::Float(new_value))
}

/// Handle JsonArrayPop command.
pub fn json_array_pop(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    key: String,
    path: String,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let popped = convert_result(substrate.json_array_pop(&api_run, &key, &path))?;
    Ok(Output::Maybe(popped))
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
        assert_eq!(extract_version(&Version::Txn(42)), 42);
        assert_eq!(extract_version(&Version::Counter(100)), 100);
    }
}
