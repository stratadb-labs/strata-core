//! Vector command handlers.
//!
//! This module implements handlers for all 17 Vector commands by dispatching
//! to the SubstrateImpl's VectorStore trait methods.

use std::sync::Arc;

use strata_api::substrate::vector::{
    DistanceMetric as ApiDistanceMetric, SearchFilter, VectorCollectionInfo as ApiVectorCollectionInfo,
    VectorMatch as ApiVectorMatch,
};
use strata_api::substrate::{ApiRunId, VectorStore, SubstrateImpl};
use strata_core::{Value, Version, Versioned};

use crate::convert::convert_result;
use crate::types::{
    CollectionInfo, DistanceMetric, FilterOp, MetadataFilter, RunId, VectorBatchEntry,
    VectorData, VectorEntry, VectorMatch, VersionedVectorData,
};
use crate::{Error, Output, Result};

/// Convert executor RunId to API RunId.
fn to_api_run_id(run: &RunId) -> Result<ApiRunId> {
    ApiRunId::parse(run.as_str()).ok_or_else(|| Error::InvalidInput {
        reason: format!("Invalid run ID: '{}'", run.as_str()),
    })
}

/// Extract u64 from Version enum.
fn extract_version(v: &Version) -> u64 {
    match v {
        Version::Txn(n) => *n,
        Version::Sequence(n) => *n,
        Version::Counter(n) => *n,
    }
}

/// Convert executor DistanceMetric to API DistanceMetric.
fn to_api_metric(metric: DistanceMetric) -> ApiDistanceMetric {
    match metric {
        DistanceMetric::Cosine => ApiDistanceMetric::Cosine,
        DistanceMetric::Euclidean => ApiDistanceMetric::Euclidean,
        DistanceMetric::DotProduct => ApiDistanceMetric::DotProduct,
    }
}

/// Convert API DistanceMetric to executor DistanceMetric.
fn from_api_metric(metric: ApiDistanceMetric) -> DistanceMetric {
    match metric {
        ApiDistanceMetric::Cosine => DistanceMetric::Cosine,
        ApiDistanceMetric::Euclidean => DistanceMetric::Euclidean,
        ApiDistanceMetric::DotProduct => DistanceMetric::DotProduct,
    }
}

/// Convert API VectorCollectionInfo to executor CollectionInfo.
fn to_collection_info(info: ApiVectorCollectionInfo) -> CollectionInfo {
    CollectionInfo {
        name: info.name,
        dimension: info.dimension,
        metric: from_api_metric(info.metric),
        count: info.count,
    }
}

/// Convert API VectorMatch to executor VectorMatch.
fn to_vector_match(m: ApiVectorMatch) -> VectorMatch {
    VectorMatch {
        key: m.key,
        score: m.score,
        metadata: if m.metadata == Value::Null {
            None
        } else {
            Some(m.metadata)
        },
    }
}

/// Convert executor MetadataFilter to API SearchFilter.
fn to_search_filter(filters: &[MetadataFilter]) -> Option<SearchFilter> {
    if filters.is_empty() {
        return None;
    }

    // Convert each filter to an Equals filter (API only supports Equals for now)
    let api_filters: Vec<SearchFilter> = filters
        .iter()
        .filter_map(|f| {
            // Only support Eq operations for now (API limitation)
            if matches!(f.op, FilterOp::Eq) {
                Some(SearchFilter::Equals {
                    field: f.field.clone(),
                    value: f.value.clone(),
                })
            } else {
                None // Skip unsupported operations
            }
        })
        .collect();

    if api_filters.is_empty() {
        None
    } else if api_filters.len() == 1 {
        Some(api_filters.into_iter().next().unwrap())
    } else {
        Some(SearchFilter::And(api_filters))
    }
}

/// Convert API Versioned<VectorData> to executor VersionedVectorData.
fn to_versioned_vector_data(
    key: &str,
    v: Versioned<(Vec<f32>, Value)>,
) -> VersionedVectorData {
    VersionedVectorData {
        key: key.to_string(),
        data: VectorData {
            embedding: v.value.0,
            metadata: if v.value.1 == Value::Null {
                None
            } else {
                Some(v.value.1)
            },
        },
        version: extract_version(&v.version),
        timestamp: v.timestamp.into(),
    }
}

// =============================================================================
// Individual Handlers
// =============================================================================

/// Handle VectorUpsert command.
pub fn vector_upsert(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    collection: String,
    key: String,
    vector: Vec<f32>,
    metadata: Option<Value>,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let version = convert_result(substrate.vector_upsert(
        &api_run,
        &collection,
        &key,
        &vector,
        metadata,
    ))?;
    Ok(Output::Version(extract_version(&version)))
}

/// Handle VectorGet command.
pub fn vector_get(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    collection: String,
    key: String,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let result = convert_result(substrate.vector_get(&api_run, &collection, &key))?;
    Ok(Output::VectorData(
        result.map(|v| to_versioned_vector_data(&key, v)),
    ))
}

/// Handle VectorDelete command.
pub fn vector_delete(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    collection: String,
    key: String,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let existed = convert_result(substrate.vector_delete(&api_run, &collection, &key))?;
    Ok(Output::Bool(existed))
}

/// Handle VectorSearch command.
pub fn vector_search(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    collection: String,
    query: Vec<f32>,
    k: u64,
    filter: Option<Vec<MetadataFilter>>,
    metric: Option<DistanceMetric>,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let api_filter = filter.as_ref().and_then(|f| to_search_filter(f));
    let api_metric = metric.map(to_api_metric);
    let matches = convert_result(substrate.vector_search(
        &api_run,
        &collection,
        &query,
        k,
        api_filter,
        api_metric,
    ))?;
    let results: Vec<VectorMatch> = matches.into_iter().map(to_vector_match).collect();
    Ok(Output::VectorMatches(results))
}

/// Handle VectorCollectionInfo command.
pub fn vector_collection_info(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    collection: String,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let info = convert_result(substrate.vector_collection_info(&api_run, &collection))?;
    Ok(Output::VectorCollectionInfo(info.map(to_collection_info)))
}

/// Handle VectorCreateCollection command.
pub fn vector_create_collection(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    collection: String,
    dimension: u64,
    metric: DistanceMetric,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let version = convert_result(substrate.vector_create_collection(
        &api_run,
        &collection,
        dimension as usize,
        to_api_metric(metric),
    ))?;
    Ok(Output::Version(extract_version(&version)))
}

/// Handle VectorDropCollection command.
pub fn vector_drop_collection(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    collection: String,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let existed = convert_result(substrate.vector_drop_collection(&api_run, &collection))?;
    Ok(Output::Bool(existed))
}

/// Handle VectorListCollections command.
pub fn vector_list_collections(substrate: &Arc<SubstrateImpl>, run: RunId) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let collections = convert_result(substrate.vector_list_collections(&api_run))?;
    let infos: Vec<CollectionInfo> = collections.into_iter().map(to_collection_info).collect();
    Ok(Output::VectorCollectionList(infos))
}

/// Handle VectorCollectionExists command.
pub fn vector_collection_exists(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    collection: String,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let exists = convert_result(substrate.vector_collection_exists(&api_run, &collection))?;
    Ok(Output::Bool(exists))
}

/// Handle VectorCount command.
pub fn vector_count(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    collection: String,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let count = convert_result(substrate.vector_count(&api_run, &collection))?;
    Ok(Output::Uint(count))
}

/// Handle VectorUpsertBatch command.
pub fn vector_upsert_batch(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    collection: String,
    vectors: Vec<VectorEntry>,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    // Convert VectorEntry to API format
    let api_vectors: Vec<(&str, &[f32], Option<Value>)> = vectors
        .iter()
        .map(|e| (e.key.as_str(), e.embedding.as_slice(), e.metadata.clone()))
        .collect();
    let results = convert_result(substrate.vector_upsert_batch(&api_run, &collection, api_vectors))?;
    let entries: Vec<VectorBatchEntry> = results
        .into_iter()
        .zip(vectors.iter())
        .map(|(r, v)| VectorBatchEntry {
            key: v.key.clone(),
            result: r
                .map(|(_, version)| extract_version(&version))
                .map_err(|e| e.to_string()),
        })
        .collect();
    Ok(Output::VectorBatchResult(entries))
}

/// Handle VectorGetBatch command.
pub fn vector_get_batch(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    collection: String,
    keys: Vec<String>,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let key_refs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();
    let results = convert_result(substrate.vector_get_batch(&api_run, &collection, &key_refs))?;
    let data: Vec<Option<VersionedVectorData>> = results
        .into_iter()
        .zip(keys.iter())
        .map(|(opt, key)| opt.map(|v| to_versioned_vector_data(key, v)))
        .collect();
    Ok(Output::VectorDataList(data))
}

/// Handle VectorDeleteBatch command.
pub fn vector_delete_batch(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    collection: String,
    keys: Vec<String>,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let key_refs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();
    let results = convert_result(substrate.vector_delete_batch(&api_run, &collection, &key_refs))?;
    Ok(Output::Bools(results))
}

/// Handle VectorHistory command.
pub fn vector_history(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    collection: String,
    key: String,
    limit: Option<u64>,
    before_version: Option<u64>,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let history = convert_result(substrate.vector_history(
        &api_run,
        &collection,
        &key,
        limit.map(|l| l as usize),
        before_version,
    ))?;
    let data: Vec<VersionedVectorData> = history
        .into_iter()
        .map(|v| to_versioned_vector_data(&key, v))
        .collect();
    Ok(Output::VectorDataHistory(data))
}

/// Handle VectorGetAt command.
pub fn vector_get_at(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    collection: String,
    key: String,
    version: u64,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let result = convert_result(substrate.vector_get_at(&api_run, &collection, &key, version))?;
    Ok(Output::VectorData(
        result.map(|v| to_versioned_vector_data(&key, v)),
    ))
}

/// Handle VectorListKeys command.
pub fn vector_list_keys(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    collection: String,
    limit: Option<u64>,
    cursor: Option<String>,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let keys = convert_result(substrate.vector_list_keys(
        &api_run,
        &collection,
        limit.map(|l| l as usize),
        cursor.as_deref(),
    ))?;
    Ok(Output::Keys(keys))
}

/// Handle VectorScan command.
pub fn vector_scan(
    substrate: &Arc<SubstrateImpl>,
    run: RunId,
    collection: String,
    limit: Option<u64>,
    cursor: Option<String>,
) -> Result<Output> {
    let api_run = to_api_run_id(&run)?;
    let results = convert_result(substrate.vector_scan(
        &api_run,
        &collection,
        limit.map(|l| l as usize),
        cursor.as_deref(),
    ))?;
    let data: Vec<(String, VectorData)> = results
        .into_iter()
        .map(|(key, (embedding, metadata))| {
            (
                key,
                VectorData {
                    embedding,
                    metadata: if metadata == Value::Null {
                        None
                    } else {
                        Some(metadata)
                    },
                },
            )
        })
        .collect();
    Ok(Output::VectorKeyValues(data))
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
    fn test_metric_conversion() {
        assert_eq!(
            to_api_metric(DistanceMetric::Cosine),
            ApiDistanceMetric::Cosine
        );
        assert_eq!(
            from_api_metric(ApiDistanceMetric::Euclidean),
            DistanceMetric::Euclidean
        );
    }
}
