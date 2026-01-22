//! Vector Facade - Simplified vector operations
//!
//! This module provides Redis-like vector operations for similarity search.
//!
//! ## Desugaring
//!
//! | Facade | Substrate |
//! |--------|-----------|
//! | `vadd(coll, key, vec, meta)` | `vector_upsert(default_run, coll, key, vec, meta)` |
//! | `vsim(coll, vec, k)` | `vector_search(default_run, coll, vec, k, None, None)` |

use strata_core::{StrataResult, Value};

/// Vector search result
#[derive(Debug, Clone)]
pub struct VectorResult {
    /// Vector key
    pub key: String,
    /// Similarity score (higher = more similar for cosine/dot, lower = more similar for L2)
    pub score: f32,
    /// Vector data (if requested)
    pub vector: Option<Vec<f32>>,
    /// Metadata (if any)
    pub metadata: Value,
}

/// Search options
#[derive(Debug, Clone, Default)]
pub struct VectorSearchOptions {
    /// Include vector data in results
    pub include_vectors: bool,
    /// Filter by metadata field equals value
    pub filter_eq: Option<(String, Value)>,
}

impl VectorSearchOptions {
    /// Create default options
    pub fn new() -> Self {
        Self::default()
    }

    /// Include vectors in results
    pub fn with_vectors(mut self) -> Self {
        self.include_vectors = true;
        self
    }

    /// Filter by metadata field
    pub fn filter(mut self, field: impl Into<String>, value: Value) -> Self {
        self.filter_eq = Some((field.into(), value));
        self
    }
}

/// Vector Facade - simplified similarity search
///
/// Provides vector storage and K-nearest-neighbor search.
///
/// ## Collection Model
///
/// Vectors are organized into collections. Each collection has a fixed
/// dimension (set from the first vector added).
pub trait VectorFacade {
    /// Add or update a vector
    ///
    /// If the key exists, replaces the vector. Creates collection if needed.
    ///
    /// ## Parameters
    /// - `collection`: Collection name
    /// - `key`: Unique key for this vector
    /// - `vector`: The vector data (f32 array)
    /// - `metadata`: Optional metadata object
    ///
    /// ## Example
    /// ```ignore
    /// // Add embedding with metadata
    /// facade.vadd(
    ///     "embeddings",
    ///     "doc:1",
    ///     &[0.1, 0.2, 0.3, 0.4],
    ///     Some(Value::Object(HashMap::from([
    ///         ("title".to_string(), Value::String("Hello World".to_string())),
    ///     ]))),
    /// )?;
    /// ```
    fn vadd(
        &self,
        collection: &str,
        key: &str,
        vector: &[f32],
        metadata: Option<Value>,
    ) -> StrataResult<()>;

    /// Get a vector by key
    ///
    /// Returns `None` if key doesn't exist.
    fn vget(&self, collection: &str, key: &str) -> StrataResult<Option<(Vec<f32>, Value)>>;

    /// Delete a vector
    ///
    /// Returns `true` if the vector existed.
    fn vdel(&self, collection: &str, key: &str) -> StrataResult<bool>;

    /// Search for similar vectors
    ///
    /// Returns the K most similar vectors to the query.
    ///
    /// ## Parameters
    /// - `collection`: Collection to search
    /// - `query`: Query vector (must match collection dimension)
    /// - `k`: Maximum number of results
    ///
    /// ## Example
    /// ```ignore
    /// let query = vec![0.1, 0.2, 0.3, 0.4];
    /// let results = facade.vsim("embeddings", &query, 10)?;
    ///
    /// for result in results {
    ///     println!("Key: {}, Score: {}", result.key, result.score);
    /// }
    /// ```
    fn vsim(&self, collection: &str, query: &[f32], k: u64) -> StrataResult<Vec<VectorResult>>;

    /// Search with options
    ///
    /// Like `vsim` but with filtering and include options.
    fn vsim_with_options(
        &self,
        collection: &str,
        query: &[f32],
        k: u64,
        options: VectorSearchOptions,
    ) -> StrataResult<Vec<VectorResult>>;

    /// Get collection info
    ///
    /// Returns (dimension, count) or None if collection doesn't exist.
    fn vcollection_info(&self, collection: &str) -> StrataResult<Option<(usize, u64)>>;

    /// Delete a collection
    ///
    /// Removes the collection and all its vectors.
    fn vcollection_drop(&self, collection: &str) -> StrataResult<bool>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trait_is_object_safe() {
        fn _assert_object_safe(_: &dyn VectorFacade) {}
    }

    #[test]
    fn test_search_options() {
        let opts = VectorSearchOptions::new()
            .with_vectors()
            .filter("type", Value::String("article".to_string()));

        assert!(opts.include_vectors);
        assert!(opts.filter_eq.is_some());
    }
}
