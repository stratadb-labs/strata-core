//! Vector store operations (7 MVP).
//!
//! MVP: upsert, get, delete, search, create_collection, delete_collection, list_collections

use super::Strata;
use crate::{Command, Error, Output, Result, Value};
use crate::types::*;

impl Strata {
    // =========================================================================
    // Vector Operations (7 MVP)
    // =========================================================================

    /// Create a vector collection.
    pub fn vector_create_collection(
        &self,
        collection: &str,
        dimension: u64,
        metric: DistanceMetric,
    ) -> Result<u64> {
        match self.executor.execute(Command::VectorCreateCollection {
            run: self.run_id(),
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

    /// Delete a collection.
    pub fn vector_delete_collection(&self, collection: &str) -> Result<bool> {
        match self.executor.execute(Command::VectorDeleteCollection {
            run: self.run_id(),
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
            run: self.run_id(),
        })? {
            Output::VectorCollectionList(infos) => Ok(infos),
            _ => Err(Error::Internal {
                reason: "Unexpected output for VectorListCollections".into(),
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
            run: self.run_id(),
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
            run: self.run_id(),
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
            run: self.run_id(),
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
            run: self.run_id(),
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
}
