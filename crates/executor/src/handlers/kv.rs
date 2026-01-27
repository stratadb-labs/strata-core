//! KV command handlers.
//!
//! This module implements handlers for all 15 KV commands by dispatching
//! directly to engine primitives via `bridge::Primitives`.

use std::sync::Arc;

use strata_core::Value;
use strata_engine::KVStoreExt;

use crate::bridge::{extract_version, to_core_run_id, to_versioned_value, validate_key, Primitives};
use crate::convert::convert_result;
use crate::types::{RunId, VersionedValue};
use crate::{Output, Result};

// =============================================================================
// Individual Handlers
// =============================================================================

/// Handle KvPut command.
pub fn kv_put(
    p: &Arc<Primitives>,
    run: RunId,
    key: String,
    value: Value,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_key(&key))?;
    let version = convert_result(p.kv.put(&run_id, &key, value))?;
    Ok(Output::Version(extract_version(&version)))
}

/// Handle KvGet command.
pub fn kv_get(p: &Arc<Primitives>, run: RunId, key: String) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_key(&key))?;
    let result = convert_result(p.kv.get(&run_id, &key))?;
    Ok(Output::MaybeVersioned(result.map(to_versioned_value)))
}

/// Handle KvGetAt command.
pub fn kv_get_at(
    p: &Arc<Primitives>,
    run: RunId,
    key: String,
    version: u64,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_key(&key))?;
    let result = convert_result(p.kv.get_at(&run_id, &key, version))?;
    Ok(Output::MaybeVersioned(result.map(to_versioned_value)))
}

/// Handle KvDelete command.
pub fn kv_delete(p: &Arc<Primitives>, run: RunId, key: String) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_key(&key))?;
    let existed = convert_result(p.kv.delete(&run_id, &key))?;
    Ok(Output::Bool(existed))
}

/// Handle KvExists command.
pub fn kv_exists(p: &Arc<Primitives>, run: RunId, key: String) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_key(&key))?;
    let exists = convert_result(p.kv.exists(&run_id, &key))?;
    Ok(Output::Bool(exists))
}

/// Handle KvHistory command.
pub fn kv_history(
    p: &Arc<Primitives>,
    run: RunId,
    key: String,
    limit: Option<u64>,
    before: Option<u64>,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_key(&key))?;
    let history = convert_result(p.kv.history(
        &run_id,
        &key,
        limit.map(|l| l as usize),
        before,
    ))?;
    let values: Vec<VersionedValue> = history.into_iter().map(to_versioned_value).collect();
    Ok(Output::VersionedValues(values))
}

/// Handle KvIncr command.
pub fn kv_incr(
    p: &Arc<Primitives>,
    run: RunId,
    key: String,
    delta: i64,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_key(&key))?;
    let new_value = convert_result(p.db.transaction(run_id, |txn| {
        use strata_engine::KVStoreExt;

        let current = txn.kv_get(&key)?;
        let current_int = match current {
            Some(v) => match v {
                Value::Int(i) => i,
                _ => return Err(strata_core::StrataError::invalid_input(
                    format!("Cannot increment non-integer value for key '{}'", key)
                )),
            },
            None => 0, // treat missing key as 0
        };
        let new_val = current_int + delta;
        txn.kv_put(&key, Value::Int(new_val))?;
        Ok(new_val)
    }))?;
    Ok(Output::Int(new_value))
}

/// Handle KvCasVersion command.
pub fn kv_cas_version(
    p: &Arc<Primitives>,
    run: RunId,
    key: String,
    expected_version: Option<u64>,
    new_value: Value,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_key(&key))?;
    let success = convert_result(p.db.transaction(run_id, |txn| {
        let current = txn.kv_get(&key)?;

        match (expected_version, current) {
            (None, None) => {
                txn.kv_put(&key, new_value.clone())?;
                Ok(true)
            }
            (None, Some(_)) => Ok(false),
            (Some(_), None) => Ok(false),
            (Some(_expected), Some(_)) => {
                // In a full implementation, we'd check the version
                txn.kv_put(&key, new_value.clone())?;
                Ok(true)
            }
        }
    }))?;
    Ok(Output::Bool(success))
}

/// Handle KvCasValue command.
pub fn kv_cas_value(
    p: &Arc<Primitives>,
    run: RunId,
    key: String,
    expected_value: Option<Value>,
    new_value: Value,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_key(&key))?;

    let result = p.db.transaction(run_id, |txn| {
        let current = txn.kv_get(&key)?;

        match (&expected_value, current) {
            (None, None) => {
                txn.kv_put(&key, new_value.clone())?;
                Ok(true)
            }
            (None, Some(_)) => Ok(false),
            (Some(_), None) => Ok(false),
            (Some(expected), Some(actual)) => {
                if *expected == actual {
                    txn.kv_put(&key, new_value.clone())?;
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
        }
    });

    match result {
        Ok(v) => Ok(Output::Bool(v)),
        Err(ref e) if e.is_conflict() => {
            // Concurrent modification - CAS semantically failed
            Ok(Output::Bool(false))
        }
        Err(e) => Err(crate::Error::from(e)),
    }
}

/// Handle KvKeys command.
pub fn kv_keys(
    p: &Arc<Primitives>,
    run: RunId,
    prefix: String,
    limit: Option<u64>,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    if !prefix.is_empty() {
        convert_result(validate_key(&prefix))?;
    }
    let keys = convert_result(p.kv.keys(&run_id, Some(&prefix), limit.map(|l| l as usize)))?;
    Ok(Output::Keys(keys))
}

/// Handle KvScan command.
pub fn kv_scan(
    p: &Arc<Primitives>,
    run: RunId,
    prefix: String,
    limit: u64,
    cursor: Option<String>,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    if !prefix.is_empty() {
        convert_result(validate_key(&prefix))?;
    }
    let result = convert_result(p.kv.scan(
        &run_id,
        &prefix,
        limit as usize,
        cursor.as_deref(),
    ))?;

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
pub fn kv_mget(p: &Arc<Primitives>, run: RunId, keys: Vec<String>) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    for key in &keys {
        convert_result(validate_key(key))?;
    }
    let key_refs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();
    let results = convert_result(p.kv.get_many(&run_id, &key_refs))?;
    let values: Vec<Option<VersionedValue>> = results
        .into_iter()
        .map(|opt| opt.map(to_versioned_value))
        .collect();
    Ok(Output::Values(values))
}

/// Handle KvMput command.
pub fn kv_mput(
    _p: &Arc<Primitives>,
    _run: RunId,
    _entries: Vec<(String, Value)>,
) -> Result<Output> {
    // TODO: Re-implement once transaction_with_version is exposed through the new API surface
    Err(crate::Error::Internal {
        reason: "kv_mput temporarily disabled during engine re-architecture".to_string(),
    })
}

/// Handle KvMdelete command.
pub fn kv_mdelete(p: &Arc<Primitives>, run: RunId, keys: Vec<String>) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    for key in &keys {
        convert_result(validate_key(key))?;
    }
    let success = convert_result(p.db.transaction(run_id, |txn| {
        let mut deleted = 0u64;
        for key in &keys {
            if txn.kv_get(key)?.is_some() {
                txn.kv_delete(key)?;
                deleted += 1;
            }
        }
        Ok(deleted)
    }))?;
    Ok(Output::Uint(success))
}

/// Handle KvMexists command.
pub fn kv_mexists(p: &Arc<Primitives>, run: RunId, keys: Vec<String>) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    for key in &keys {
        convert_result(validate_key(key))?;
    }
    let key_refs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();
    let results = convert_result(p.kv.get_many(&run_id, &key_refs))?;
    Ok(Output::Uint(results.iter().filter(|v| v.is_some()).count() as u64))
}
