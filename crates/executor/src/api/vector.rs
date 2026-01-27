//! Vector store operations.

use super::Strata;
use strata_core::Value;
use crate::{Command, Error, Output, Result};
use crate::types::*;

impl Strata {
    // =========================================================================
    // Vector Operations (17)
    // =========================================================================

    /// Create a vector collection.
    pub fn vector_create_collection(
        &self,
        collection: &str,
        dimension: u64,
        metric: DistanceMetric,
    ) -> Result<u64> {
        match self.executor.execute(Command::VectorCreateCollection {
            run: None,
            collection: collection.to_string(),
            dimension,
            metric,
        })? {
            Output::Version(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorCreateCollection".into(),
            }),
        }
    }

    /// Upsert a vector.
    pub fn vector_upsert(
        &self,
        collection: &str,
        key: &str,
        vector: Vec<f32>,
        metadata: Option<Value>,
    ) -> Result<u64> {
        match self.executor.execute(Command::VectorUpsert {
            run: None,
            collection: collection.to_string(),
            key: key.to_string(),
            vector,
            metadata,
        })? {
            Output::Version(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorUpsert".into(),
            }),
        }
    }

    /// Get a vector by key.
    pub fn vector_get(&self, collection: &str, key: &str) -> Result<Option<VersionedVectorData>> {
        match self.executor.execute(Command::VectorGet {
            run: None,
            collection: collection.to_string(),
            key: key.to_string(),
        })? {
            Output::VectorData(data) => Ok(data),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorGet".into(),
            }),
        }
    }

    /// Delete a vector.
    pub fn vector_delete(&self, collection: &str, key: &str) -> Result<bool> {
        match self.executor.execute(Command::VectorDelete {
            run: None,
            collection: collection.to_string(),
            key: key.to_string(),
        })? {
            Output::Bool(deleted) => Ok(deleted),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorDelete".into(),
            }),
        }
    }

    /// Search for similar vectors.
    pub fn vector_search(
        &self,
        collection: &str,
        query: Vec<f32>,
        k: u64,
    ) -> Result<Vec<VectorMatch>> {
        match self.executor.execute(Command::VectorSearch {
            run: None,
            collection: collection.to_string(),
            query,
            k,
            filter: None,
            metric: None,
        })? {
            Output::VectorMatches(matches) => Ok(matches),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorSearch".into(),
            }),
        }
    }

    /// Search for similar vectors with filter and metric options.
    pub fn vector_search_filtered(
        &self,
        collection: &str,
        query: Vec<f32>,
        k: u64,
        filter: Option<Vec<MetadataFilter>>,
        metric: Option<DistanceMetric>,
    ) -> Result<Vec<VectorMatch>> {
        match self.executor.execute(Command::VectorSearch {
            run: None,
            collection: collection.to_string(),
            query,
            k,
            filter,
            metric,
        })? {
            Output::VectorMatches(matches) => Ok(matches),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorSearch".into(),
            }),
        }
    }

    /// Get collection information.
    pub fn vector_get_collection(&self, collection: &str) -> Result<Option<CollectionInfo>> {
        match self.executor.execute(Command::VectorGetCollection {
            run: None,
            collection: collection.to_string(),
        })? {
            Output::VectorGetCollection(info) => Ok(info),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorGetCollection".into(),
            }),
        }
    }

    /// Delete a collection.
    pub fn vector_delete_collection(&self, collection: &str) -> Result<bool> {
        match self.executor.execute(Command::VectorDeleteCollection {
            run: None,
            collection: collection.to_string(),
        })? {
            Output::Bool(dropped) => Ok(dropped),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorDeleteCollection".into(),
            }),
        }
    }

    /// List all collections.
    pub fn vector_list_collections(&self) -> Result<Vec<CollectionInfo>> {
        match self.executor.execute(Command::VectorListCollections {
            run: None,
        })? {
            Output::VectorCollectionList(infos) => Ok(infos),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorListCollections".into(),
            }),
        }
    }

    /// Check if a collection exists.
    pub fn vector_collection_exists(&self, collection: &str) -> Result<bool> {
        match self.executor.execute(Command::VectorCollectionExists {
            run: None,
            collection: collection.to_string(),
        })? {
            Output::Bool(exists) => Ok(exists),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorCollectionExists".into(),
            }),
        }
    }

    /// Get the count of vectors in a collection.
    pub fn vector_count(&self, collection: &str) -> Result<u64> {
        match self.executor.execute(Command::VectorCount {
            run: None,
            collection: collection.to_string(),
        })? {
            Output::Uint(count) => Ok(count),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorCount".into(),
            }),
        }
    }

    /// Batch insert or update vectors.
    pub fn vector_upsert_batch(
        &self,
        collection: &str,
        vectors: Vec<VectorEntry>,
    ) -> Result<Vec<VectorBatchEntry>> {
        match self.executor.execute(Command::VectorUpsertBatch {
            run: None,
            collection: collection.to_string(),
            vectors,
        })? {
            Output::VectorBatchResult(results) => Ok(results),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorUpsertBatch".into(),
            }),
        }
    }

    /// Batch get vectors.
    pub fn vector_get_batch(
        &self,
        collection: &str,
        keys: Vec<String>,
    ) -> Result<Vec<Option<VersionedVectorData>>> {
        match self.executor.execute(Command::VectorGetBatch {
            run: None,
            collection: collection.to_string(),
            keys,
        })? {
            Output::VectorDataList(data) => Ok(data),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorGetBatch".into(),
            }),
        }
    }

    /// Batch delete vectors.
    pub fn vector_delete_batch(&self, collection: &str, keys: Vec<String>) -> Result<Vec<bool>> {
        match self.executor.execute(Command::VectorDeleteBatch {
            run: None,
            collection: collection.to_string(),
            keys,
        })? {
            Output::Bools(results) => Ok(results),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorDeleteBatch".into(),
            }),
        }
    }

    /// Get version history for a vector.
    pub fn vector_history(
        &self,
        collection: &str,
        key: &str,
        limit: Option<u64>,
        before_version: Option<u64>,
    ) -> Result<Vec<VersionedVectorData>> {
        match self.executor.execute(Command::VectorHistory {
            run: None,
            collection: collection.to_string(),
            key: key.to_string(),
            limit,
            before_version,
        })? {
            Output::VectorDataHistory(history) => Ok(history),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorHistory".into(),
            }),
        }
    }

    /// Get a vector at a specific version.
    pub fn vector_get_at(
        &self,
        collection: &str,
        key: &str,
        version: u64,
    ) -> Result<Option<VersionedVectorData>> {
        match self.executor.execute(Command::VectorGetAt {
            run: None,
            collection: collection.to_string(),
            key: key.to_string(),
            version,
        })? {
            Output::VectorData(data) => Ok(data),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorGetAt".into(),
            }),
        }
    }

    /// List all vector keys in a collection.
    pub fn vector_list_keys(
        &self,
        collection: &str,
        limit: Option<u64>,
        cursor: Option<String>,
    ) -> Result<Vec<String>> {
        match self.executor.execute(Command::VectorListKeys {
            run: None,
            collection: collection.to_string(),
            limit,
            cursor,
        })? {
            Output::Keys(keys) => Ok(keys),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorListKeys".into(),
            }),
        }
    }

    /// Scan vectors in a collection.
    pub fn vector_scan(
        &self,
        collection: &str,
        limit: Option<u64>,
        cursor: Option<String>,
    ) -> Result<Vec<(String, VectorData)>> {
        match self.executor.execute(Command::VectorScan {
            run: None,
            collection: collection.to_string(),
            limit,
            cursor,
        })? {
            Output::VectorKeyValues(entries) => Ok(entries),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorScan".into(),
            }),
        }
    }
}
