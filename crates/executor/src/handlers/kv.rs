//! KV command handlers.
//!
//! This module implements handlers for all 15 KV commands by dispatching
//! to the SubstrateImpl's KVStore and KVStoreBatch trait methods.

use std::sync::Arc;

use strata_api::substrate::kv::KVScanResult as ApiKVScanResult;
use strata_api::substrate::{ApiRunId, KVStore, KVStoreBatch, SubstrateImpl};
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

/// Handle KvPut command.
pub fn kv_put(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    key: String,
    value: Value,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let version = convert_result(substrate.kv_put(&api_run, &key, value))?;
    Ok(Output::Version(extract_version(&version)))
}

/// Handle KvGet command.
pub fn kv_get(substrate: &Arc<SubstrateImpl>, run: RunId, key: String) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let result = convert_result(substrate.kv_get(&api_run, &key))?;
    Ok(Output::MaybeVersioned(result.map(to_versioned_value)))
}

/// Handle KvGetAt command.
pub fn kv_get_at(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    key: String,
    version: u64,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let result = convert_result(substrate.kv_get_at(&api_run, &key, Version::Txn(version)))?;
    Ok(Output::Versioned(to_versioned_value(result)))
}

/// Handle KvDelete command.
pub fn kv_delete(substrate: &Arc<SubstrateImpl>, run: RunId, key: String) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let existed = convert_result(substrate.kv_delete(&api_run, &key))?;
    Ok(Output::Bool(existed))
}

/// Handle KvExists command.
pub fn kv_exists(substrate: &Arc<SubstrateImpl>, run: RunId, key: String) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let exists = convert_result(substrate.kv_exists(&api_run, &key))?;
    Ok(Output::Bool(exists))
}

/// Handle KvHistory command.
pub fn kv_history(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    key: String,
    limit: Option<u64>,
    before: Option<u64>,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let before_version = before.map(Version::Txn);
    let history = convert_result(substrate.kv_history(&api_run, &key, limit, before_version))?;
    let values: Vec<VersionedValue> = history.into_iter().map(to_versioned_value).collect();
    Ok(Output::VersionedValues(values))
}

/// Handle KvIncr command.
pub fn kv_incr(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    key: String,
    delta: i64,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let new_value = convert_result(substrate.kv_incr(&api_run, &key, delta))?;
    Ok(Output::Int(new_value))
}

/// Handle KvCasVersion command.
pub fn kv_cas_version(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    key: String,
    expected_version: Option<u64>,
    new_value: Value,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let expected = expected_version.map(Version::Txn);
    let success = convert_result(substrate.kv_cas_version(&api_run, &key, expected, new_value))?;
    Ok(Output::Bool(success))
}

/// Handle KvCasValue command.
pub fn kv_cas_value(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    key: String,
    expected_value: Option<Value>,
    new_value: Value,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let success = convert_result(substrate.kv_cas_value(&api_run, &key, expected_value, new_value))?;
    Ok(Output::Bool(success))
}

/// Handle KvKeys command.
pub fn kv_keys(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    prefix: String,
    limit: Option<u64>,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let keys = convert_result(substrate.kv_keys(&api_run, &prefix, limit.map(|l| l as usize)))?;
    Ok(Output::Keys(keys))
}

/// Handle KvScan command.
pub fn kv_scan(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    prefix: String,
    limit: u64,
    cursor: Option<String>,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let result: ApiKVScanResult = convert_result(substrate.kv_scan(
        &api_run,
        &prefix,
        limit as usize,
        cursor.as_deref(),
    ))?;

    // Convert entries
    let entries: Vec<(String, VersionedValue)> = result
        .entries
        .into_iter()
        .map(|(k, v)| (k, to_versioned_value(v)))
        .collect();

    Ok(Output::KvScanResult {
        entries,
        cursor: result.cursor,
    })
}

/// Handle KvMget command.
pub fn kv_mget(substrate: &Arc<SubstrateImpl>, run: RunId, keys: Vec<String>) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    // Convert Vec<String> to Vec<&str> for the API
    let key_refs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();
    let results = convert_result(substrate.kv_mget(&api_run, &key_refs))?;
    let values: Vec<Option<VersionedValue>> = results
        .into_iter()
        .map(|opt| opt.map(to_versioned_value))
        .collect();
    Ok(Output::Values(values))
}

/// Handle KvMput command.
pub fn kv_mput(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    entries: Vec<(String, Value)>,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    // Convert Vec<(String, Value)> to Vec<(&str, Value)> for the API
    let entry_refs: Vec<(&str, Value)> = entries
        .iter()
        .map(|(k, v): &(String, Value)| (k.as_str(), v.clone()))
        .collect();
    let version = convert_result(substrate.kv_mput(&api_run, &entry_refs))?;
    Ok(Output::Version(extract_version(&version)))
}

/// Handle KvMdelete command.
pub fn kv_mdelete(substrate: &Arc<SubstrateImpl>, run: RunId, keys: Vec<String>) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let key_refs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();
    let count = convert_result(substrate.kv_mdelete(&api_run, &key_refs))?;
    Ok(Output::Uint(count))
}

/// Handle KvMexists command.
pub fn kv_mexists(substrate: &Arc<SubstrateImpl>, run: RunId, keys: Vec<String>) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let key_refs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();
    let count = convert_result(substrate.kv_mexists(&api_run, &key_refs))?;
    Ok(Output::Uint(count))
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
    fn test_to_api_run_id_uuid() {
        let run = RunId::from("f47ac10b-58cc-4372-a567-0e02b2c3d479");
        let api_run = to_api_run_id(&run).unwrap();
        assert!(!api_run.is_default());
    }

    #[test]
    fn test_to_api_run_id_invalid() {
        let run = RunId::from("not-a-valid-run-id");
        let result = to_api_run_id(&run);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_version() {
        assert_eq!(extract_version(&Version::Txn(42)), 42);
        assert_eq!(extract_version(&Version::Sequence(100)), 100);
        assert_eq!(extract_version(&Version::Counter(7)), 7);
    }
}
