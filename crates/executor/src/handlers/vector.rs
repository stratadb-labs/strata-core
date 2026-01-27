//! Vector command handlers.
//!
//! This module implements handlers for all vector commands by dispatching
//! directly to engine primitives via `bridge::Primitives`.

use std::sync::Arc;

use strata_core::{StrataError, Value, Version};

use crate::bridge::{
    extract_version, from_engine_metric, is_internal_collection, to_core_run_id, to_engine_filter,
    to_engine_metric, validate_not_internal_collection, value_to_serde_json_public,
    serde_json_to_value_public, Primitives,
};
use crate::convert::convert_result;
use crate::types::{
    CollectionInfo, DistanceMetric, MetadataFilter, RunId, VectorBatchEntry, VectorData,
    VectorEntry, VectorMatch, VersionedVectorData,
};
use crate::{Output, Result};

/// Convert an engine `VectorResult<T>` to an executor `Result<T>`.
///
/// Maps `VectorError` → `StrataError` → executor `Error`.
fn convert_vector_result<T>(
    r: std::result::Result<T, strata_engine::VectorError>,
) -> Result<T> {
    convert_result(r.map_err(StrataError::from))
}

/// Convert engine `VectorEntry` to executor `VersionedVectorData`.
fn to_versioned_vector_data(
    entry: &strata_engine::VectorEntry,
    version: u64,
    timestamp: u64,
) -> Result<VersionedVectorData> {
    let metadata = entry
        .metadata
        .clone()
        .map(serde_json_to_value_public)
        .transpose()
        .map_err(|e| crate::Error::from(e))?;
    Ok(VersionedVectorData {
        key: entry.key.clone(),
        data: VectorData {
            embedding: entry.embedding.clone(),
            metadata,
        },
        version,
        timestamp,
    })
}

/// Convert engine `VectorMatch` metadata to executor `VectorMatch`.
fn to_vector_match(m: strata_engine::VectorMatch) -> Result<VectorMatch> {
    let metadata = m
        .metadata
        .map(serde_json_to_value_public)
        .transpose()
        .map_err(|e| crate::Error::from(e))?;
    Ok(VectorMatch {
        key: m.key,
        score: m.score,
        metadata,
    })
}

// =============================================================================
// Individual Handlers
// =============================================================================

/// Handle VectorUpsert command.
pub fn vector_upsert(
    p: &Arc<Primitives>,
    run: RunId,
    collection: String,
    key: String,
    vector: Vec<f32>,
    metadata: Option<Value>,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_not_internal_collection(&collection))?;

    // Auto-create collection on first upsert
    let exists = convert_vector_result(p.vector.collection_exists(run_id, &collection))?;
    if !exists {
        let dim = vector.len();
        let config = convert_result(
            strata_core::primitives::VectorConfig::new(dim, strata_engine::DistanceMetric::Cosine),
        )?;
        convert_vector_result(p.vector.create_collection(run_id, &collection, config))?;
    }

    let json_metadata = metadata
        .map(value_to_serde_json_public)
        .transpose()
        .map_err(|e| crate::Error::from(e))?;
    let version = convert_vector_result(
        p.vector.insert(run_id, &collection, &key, &vector, json_metadata),
    )?;
    Ok(Output::Version(extract_version(&version)))
}

/// Handle VectorGet command.
pub fn vector_get(
    p: &Arc<Primitives>,
    run: RunId,
    collection: String,
    key: String,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_not_internal_collection(&collection))?;

    let result = convert_vector_result(p.vector.get(run_id, &collection, &key))?;
    match result {
        Some(versioned) => {
            let version = extract_version(&versioned.version);
            let timestamp: u64 = versioned.timestamp.into();
            let data = to_versioned_vector_data(&versioned.value, version, timestamp)?;
            Ok(Output::VectorData(Some(data)))
        }
        None => Ok(Output::VectorData(None)),
    }
}

/// Handle VectorDelete command.
pub fn vector_delete(
    p: &Arc<Primitives>,
    run: RunId,
    collection: String,
    key: String,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_not_internal_collection(&collection))?;
    let existed = convert_vector_result(p.vector.delete(run_id, &collection, &key))?;
    Ok(Output::Bool(existed))
}

/// Handle VectorSearch command.
pub fn vector_search(
    p: &Arc<Primitives>,
    run: RunId,
    collection: String,
    query: Vec<f32>,
    k: u64,
    filter: Option<Vec<MetadataFilter>>,
    _metric: Option<DistanceMetric>,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_not_internal_collection(&collection))?;

    let engine_filter = filter.as_ref().and_then(|f| to_engine_filter(f));
    let matches = convert_vector_result(
        p.vector.search(run_id, &collection, &query, k as usize, engine_filter),
    )?;

    let results: Result<Vec<VectorMatch>> = matches.into_iter().map(to_vector_match).collect();
    Ok(Output::VectorMatches(results?))
}

/// Handle VectorGetCollection command.
pub fn vector_get_collection(
    p: &Arc<Primitives>,
    run: RunId,
    collection: String,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_not_internal_collection(&collection))?;

    let info = convert_vector_result(p.vector.get_collection(run_id, &collection))?;
    Ok(Output::VectorGetCollection(info.map(|i| CollectionInfo {
        name: collection,
        dimension: i.value.config.dimension,
        metric: from_engine_metric(i.value.config.metric),
        count: i.value.count as u64,
    })))
}

/// Handle VectorCreateCollection command.
pub fn vector_create_collection(
    p: &Arc<Primitives>,
    run: RunId,
    collection: String,
    dimension: u64,
    metric: DistanceMetric,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_not_internal_collection(&collection))?;

    let config = convert_result(
        strata_core::primitives::VectorConfig::new(dimension as usize, to_engine_metric(metric)),
    )?;
    let versioned =
        convert_vector_result(p.vector.create_collection(run_id, &collection, config))?;
    Ok(Output::Version(extract_version(&versioned.version)))
}

/// Handle VectorDeleteCollection command.
pub fn vector_delete_collection(
    p: &Arc<Primitives>,
    run: RunId,
    collection: String,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_not_internal_collection(&collection))?;

    // Check existence then delete (primitive returns () not bool)
    let existed = convert_vector_result(p.vector.collection_exists(run_id, &collection))?;
    if existed {
        convert_vector_result(p.vector.delete_collection(run_id, &collection))?;
    }
    Ok(Output::Bool(existed))
}

/// Handle VectorListCollections command.
pub fn vector_list_collections(p: &Arc<Primitives>, run: RunId) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    let collections = convert_vector_result(p.vector.list_collections(run_id))?;

    // Filter out internal collections (starting with '_')
    let infos: Vec<CollectionInfo> = collections
        .into_iter()
        .filter(|info| !is_internal_collection(&info.name))
        .map(|info| CollectionInfo {
            name: info.name,
            dimension: info.config.dimension,
            metric: from_engine_metric(info.config.metric),
            count: info.count as u64,
        })
        .collect();
    Ok(Output::VectorCollectionList(infos))
}

/// Handle VectorCollectionExists command.
pub fn vector_collection_exists(
    p: &Arc<Primitives>,
    run: RunId,
    collection: String,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_not_internal_collection(&collection))?;
    let exists = convert_vector_result(p.vector.collection_exists(run_id, &collection))?;
    Ok(Output::Bool(exists))
}

/// Handle VectorCount command.
pub fn vector_count(
    p: &Arc<Primitives>,
    run: RunId,
    collection: String,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_not_internal_collection(&collection))?;

    // Return 0 if collection doesn't exist
    let exists = convert_vector_result(p.vector.collection_exists(run_id, &collection))?;
    if !exists {
        return Ok(Output::Uint(0));
    }

    let count = convert_vector_result(p.vector.count(run_id, &collection))?;
    Ok(Output::Uint(count as u64))
}

/// Handle VectorUpsertBatch command.
pub fn vector_upsert_batch(
    p: &Arc<Primitives>,
    run: RunId,
    collection: String,
    vectors: Vec<VectorEntry>,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_not_internal_collection(&collection))?;

    if vectors.is_empty() {
        return Ok(Output::VectorBatchResult(Vec::new()));
    }

    // Auto-create collection if it doesn't exist (dimension from first vector)
    let exists = convert_vector_result(p.vector.collection_exists(run_id, &collection))?;
    if !exists {
        if let Some(first) = vectors.iter().find(|e| !e.embedding.is_empty()) {
            let config = convert_result(strata_core::primitives::VectorConfig::new(
                first.embedding.len(),
                strata_engine::DistanceMetric::Cosine,
            ))?;
            convert_vector_result(p.vector.create_collection(run_id, &collection, config))?;
        }
    }

    // Convert metadata from Value to serde_json::Value
    let mut primitive_vectors: Vec<(&str, &[f32], Option<serde_json::Value>)> =
        Vec::with_capacity(vectors.len());
    let mut json_metadata_store: Vec<Option<serde_json::Value>> =
        Vec::with_capacity(vectors.len());

    for entry in &vectors {
        let json_meta = entry
            .metadata
            .clone()
            .map(value_to_serde_json_public)
            .transpose()
            .map_err(|e| crate::Error::from(e))?;
        json_metadata_store.push(json_meta);
    }
    for (i, entry) in vectors.iter().enumerate() {
        primitive_vectors.push((
            entry.key.as_str(),
            entry.embedding.as_slice(),
            json_metadata_store[i].clone(),
        ));
    }

    let results =
        convert_vector_result(p.vector.insert_batch(run_id, &collection, primitive_vectors))?;

    let entries: Vec<VectorBatchEntry> = results
        .into_iter()
        .zip(vectors.iter())
        .map(|(r, v)| VectorBatchEntry {
            key: v.key.clone(),
            result: r
                .map(|(_, version)| extract_version(&version))
                .map_err(|e| StrataError::from(e).to_string()),
        })
        .collect();
    Ok(Output::VectorBatchResult(entries))
}

/// Handle VectorGetBatch command.
pub fn vector_get_batch(
    p: &Arc<Primitives>,
    run: RunId,
    collection: String,
    keys: Vec<String>,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_not_internal_collection(&collection))?;

    if keys.is_empty() {
        return Ok(Output::VectorDataList(Vec::new()));
    }

    let key_refs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();
    let results = convert_vector_result(p.vector.get_batch(run_id, &collection, &key_refs))?;

    let data: Result<Vec<Option<VersionedVectorData>>> = results
        .into_iter()
        .map(|opt| {
            opt.map(|versioned| {
                let version = extract_version(&versioned.version);
                let timestamp: u64 = versioned.timestamp.into();
                to_versioned_vector_data(&versioned.value, version, timestamp)
            })
            .transpose()
        })
        .collect();
    Ok(Output::VectorDataList(data?))
}

/// Handle VectorDeleteBatch command.
pub fn vector_delete_batch(
    p: &Arc<Primitives>,
    run: RunId,
    collection: String,
    keys: Vec<String>,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_not_internal_collection(&collection))?;

    if keys.is_empty() {
        return Ok(Output::Bools(Vec::new()));
    }

    let key_refs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();
    let results = convert_vector_result(p.vector.delete_batch(run_id, &collection, &key_refs))?;
    Ok(Output::Bools(results))
}

/// Handle VectorHistory command.
pub fn vector_history(
    p: &Arc<Primitives>,
    run: RunId,
    collection: String,
    key: String,
    limit: Option<u64>,
    before_version: Option<u64>,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_not_internal_collection(&collection))?;

    let history = convert_vector_result(p.vector.history(
        run_id,
        &collection,
        &key,
        limit.map(|l| l as usize),
        before_version,
    ))?;

    let data: Result<Vec<VersionedVectorData>> = history
        .into_iter()
        .map(|versioned| {
            let version = extract_version(&versioned.version);
            let timestamp: u64 = versioned.timestamp.into();
            to_versioned_vector_data(&versioned.value, version, timestamp)
        })
        .collect();
    Ok(Output::VectorDataHistory(data?))
}

/// Handle VectorGetAt command.
pub fn vector_get_at(
    p: &Arc<Primitives>,
    run: RunId,
    collection: String,
    key: String,
    version: u64,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_not_internal_collection(&collection))?;

    let result = convert_vector_result(
        p.vector
            .get_at(run_id, &collection, &key, Version::txn(version)),
    )?;
    match result {
        Some(versioned) => {
            let ver = extract_version(&versioned.version);
            let timestamp: u64 = versioned.timestamp.into();
            let data = to_versioned_vector_data(&versioned.value, ver, timestamp)?;
            Ok(Output::VectorData(Some(data)))
        }
        None => Ok(Output::VectorData(None)),
    }
}

/// Handle VectorListKeys command.
pub fn vector_list_keys(
    p: &Arc<Primitives>,
    run: RunId,
    collection: String,
    limit: Option<u64>,
    cursor: Option<String>,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_not_internal_collection(&collection))?;

    let keys = convert_vector_result(p.vector.list_keys(
        run_id,
        &collection,
        limit.map(|l| l as usize),
        cursor.as_deref(),
    ))?;
    Ok(Output::Keys(keys))
}

/// Handle VectorScan command.
pub fn vector_scan(
    p: &Arc<Primitives>,
    run: RunId,
    collection: String,
    limit: Option<u64>,
    cursor: Option<String>,
) -> Result<Output> {
    let run_id = to_core_run_id(&run)?;
    convert_result(validate_not_internal_collection(&collection))?;

    let results = convert_vector_result(p.vector.scan(
        run_id,
        &collection,
        limit.map(|l| l as usize),
        cursor.as_deref(),
    ))?;

    let data: Result<Vec<(String, VectorData)>> = results
        .into_iter()
        .map(|(key, embedding, metadata, _version)| {
            let meta = metadata
                .map(serde_json_to_value_public)
                .transpose()
                .map_err(|e| crate::Error::from(e))?;
            Ok((
                key,
                VectorData {
                    embedding,
                    metadata: meta,
                },
            ))
        })
        .collect();
    Ok(Output::VectorKeyValues(data?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge;

    #[test]
    fn test_to_core_run_id_default() {
        let run = RunId::from("default");
        let core_id = bridge::to_core_run_id(&run).unwrap();
        assert_eq!(core_id.as_bytes(), &[0u8; 16]);
    }
}
