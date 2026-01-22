//! VectorStore Substrate Operations
//!
//! The VectorStore provides dense vector storage and similarity search for embeddings.
//! It supports multiple collections with different dimensions and distance metrics.
//!
//! ## Collection Model
//!
//! - Vectors are organized into named collections
//! - Each collection has a fixed dimension (set on first insert)
//! - Vectors within a collection must all have the same dimension
//! - Metadata can be attached to vectors and used for filtering
//!
//! ## Distance Metrics
//!
//! - `Cosine`: Cosine similarity (normalized, range [0, 1] for similarity)
//! - `Euclidean`: L2 distance (smaller = more similar)
//! - `DotProduct`: Inner product (larger = more similar)
//!
//! ## Versioning
//!
//! Vectors use transaction-based versioning (`Version::Txn`).

use super::types::ApiRunId;
use strata_core::{StrataResult, Value, Version, Versioned};
use serde::{Deserialize, Serialize};

/// Vector data with metadata
///
/// Type alias for a vector and its associated metadata.
pub type VectorData = (Vec<f32>, Value);

/// Distance metric for vector similarity search
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DistanceMetric {
    /// Cosine similarity (1 - cosine distance)
    #[default]
    Cosine,
    /// Euclidean (L2) distance
    Euclidean,
    /// Dot product (inner product)
    DotProduct,
}

/// Vector search result
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VectorMatch {
    /// Vector key
    pub key: String,
    /// Similarity/distance score
    pub score: f32,
    /// Vector data
    pub vector: Vec<f32>,
    /// Attached metadata
    pub metadata: Value,
    /// Version of the vector
    pub version: Version,
}

/// Search filter for metadata
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchFilter {
    /// Exact match: `metadata[field] == value`
    Equals {
        /// Metadata field name
        field: String,
        /// Value to match
        value: Value,
    },
    /// Prefix match: `metadata[field].starts_with(prefix)`
    Prefix {
        /// Metadata field name
        field: String,
        /// Prefix to match
        prefix: String,
    },
    /// Range match: `min <= metadata[field] <= max`
    Range {
        /// Metadata field name
        field: String,
        /// Minimum value (inclusive)
        min: Value,
        /// Maximum value (inclusive)
        max: Value,
    },
    /// AND of multiple filters
    And(Vec<SearchFilter>),
    /// OR of multiple filters
    Or(Vec<SearchFilter>),
    /// NOT of a filter
    Not(Box<SearchFilter>),
}

/// VectorStore substrate operations
///
/// This trait defines the canonical vector store operations.
/// All operations require explicit run_id and return versioned results.
///
/// ## Contract
///
/// - Collections have fixed dimension (set on first insert)
/// - All vectors in a collection must match the dimension
/// - Metadata is `Value::Object` or `Value::Null`
///
/// ## Error Handling
///
/// | Condition | Error |
/// |-----------|-------|
/// | Invalid collection name | `InvalidKey` |
/// | Invalid vector key | `InvalidKey` |
/// | Dimension mismatch | `ConstraintViolation` |
/// | Dimension too large | `ConstraintViolation` |
/// | Run not found | `NotFound` |
/// | Run is closed | `ConstraintViolation` |
pub trait VectorStore {
    /// Insert or update a vector
    ///
    /// Stores a vector with optional metadata.
    /// Returns the version of the stored vector.
    ///
    /// ## Semantics
    ///
    /// - Creates collection if it doesn't exist (dimension set from first vector)
    /// - Replaces vector if key exists (creates new version)
    /// - Validates dimension matches collection
    ///
    /// ## Parameters
    ///
    /// - `collection`: Collection name
    /// - `key`: Vector key (unique within collection)
    /// - `vector`: The vector data (f32 array)
    /// - `metadata`: Optional metadata (must be Object or Null)
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Collection or key name is invalid
    /// - `ConstraintViolation`: Dimension mismatch, too large, or run is closed
    /// - `NotFound`: Run does not exist
    fn vector_upsert(
        &self,
        run: &ApiRunId,
        collection: &str,
        key: &str,
        vector: &[f32],
        metadata: Option<Value>,
    ) -> StrataResult<Version>;

    /// Get a vector by key
    ///
    /// Returns the vector data and metadata.
    ///
    /// ## Return Value
    ///
    /// - `Some((vector, metadata, version))`: Vector exists
    /// - `None`: Vector does not exist
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Collection or key name is invalid
    /// - `NotFound`: Run does not exist
    fn vector_get(
        &self,
        run: &ApiRunId,
        collection: &str,
        key: &str,
    ) -> StrataResult<Option<Versioned<VectorData>>>;

    /// Delete a vector
    ///
    /// Removes the vector from the collection.
    /// Returns `true` if the vector existed.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Collection or key name is invalid
    /// - `NotFound`: Run does not exist
    /// - `ConstraintViolation`: Run is closed
    fn vector_delete(&self, run: &ApiRunId, collection: &str, key: &str) -> StrataResult<bool>;

    /// Search for similar vectors
    ///
    /// Returns the K most similar vectors to the query.
    ///
    /// ## Parameters
    ///
    /// - `collection`: Collection to search
    /// - `query`: Query vector (must match collection dimension)
    /// - `k`: Maximum results to return
    /// - `filter`: Optional metadata filter
    /// - `metric`: Distance metric (defaults to collection default)
    ///
    /// ## Return Value
    ///
    /// Vector of matches sorted by similarity (most similar first).
    /// Empty if collection doesn't exist or no matches.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Collection name is invalid
    /// - `ConstraintViolation`: Query dimension mismatch
    /// - `NotFound`: Run does not exist
    fn vector_search(
        &self,
        run: &ApiRunId,
        collection: &str,
        query: &[f32],
        k: u64,
        filter: Option<SearchFilter>,
        metric: Option<DistanceMetric>,
    ) -> StrataResult<Vec<VectorMatch>>;

    /// Get collection info
    ///
    /// Returns information about a collection.
    ///
    /// ## Return Value
    ///
    /// - `Some((dimension, count, metric))`: Collection exists
    /// - `None`: Collection does not exist
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Collection name is invalid
    /// - `NotFound`: Run does not exist
    fn vector_collection_info(
        &self,
        run: &ApiRunId,
        collection: &str,
    ) -> StrataResult<Option<(usize, u64, DistanceMetric)>>;

    /// Create a collection with explicit configuration
    ///
    /// Pre-creates a collection with specific dimension and metric.
    /// Returns the version.
    ///
    /// ## Semantics
    ///
    /// - If collection exists, validates dimension matches
    /// - If collection doesn't exist, creates with config
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Collection name is invalid
    /// - `ConstraintViolation`: Dimension mismatch with existing, or run is closed
    /// - `NotFound`: Run does not exist
    fn vector_create_collection(
        &self,
        run: &ApiRunId,
        collection: &str,
        dimension: usize,
        metric: DistanceMetric,
    ) -> StrataResult<Version>;

    /// Delete a collection
    ///
    /// Removes the entire collection including all vectors.
    /// Returns `true` if the collection existed.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Collection name is invalid
    /// - `NotFound`: Run does not exist
    /// - `ConstraintViolation`: Run is closed
    fn vector_drop_collection(&self, run: &ApiRunId, collection: &str) -> StrataResult<bool>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trait_is_object_safe() {
        fn _assert_object_safe(_: &dyn VectorStore) {}
    }

    #[test]
    fn test_distance_metric_default() {
        assert_eq!(DistanceMetric::default(), DistanceMetric::Cosine);
    }

    #[test]
    fn test_distance_metric_serialization() {
        let metric = DistanceMetric::Euclidean;
        let json = serde_json::to_string(&metric).unwrap();
        assert_eq!(json, "\"euclidean\"");

        let restored: DistanceMetric = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, metric);
    }
}
