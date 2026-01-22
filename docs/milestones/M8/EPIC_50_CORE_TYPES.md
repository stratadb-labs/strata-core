# Epic 50: Core Types & Configuration

**Goal**: Define all type definitions for the Vector primitive

**Dependencies**: M7 complete

---

## Scope

- VectorConfig with dimension, metric, storage_dtype
- DistanceMetric enum with Cosine, Euclidean, DotProduct
- VectorEntry, VectorMatch, CollectionInfo types
- MetadataFilter and JsonScalar for filtering
- VectorError enum with all error variants

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #330 | VectorConfig Type Definition | FOUNDATION |
| #331 | DistanceMetric Enum | FOUNDATION |
| #332 | VectorEntry and VectorMatch Types | FOUNDATION |
| #333 | MetadataFilter and JsonScalar Types | HIGH |
| #334 | VectorError Enum | FOUNDATION |

---

## Story #330: VectorConfig Type Definition

**File**: `crates/primitives/src/vector/types.rs` (NEW)

**Deliverable**: Immutable collection configuration type

### Implementation

```rust
/// Collection configuration - immutable after creation
///
/// IMPORTANT: This struct must NOT contain backend-specific fields.
/// HNSW parameters (ef_construction, M, etc.) belong in HnswConfig, not here.
/// See Rule 7 in M8_ARCHITECTURE.md.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorConfig {
    /// Embedding dimension (e.g., 384, 768, 1536)
    /// Must be > 0. Immutable after collection creation.
    pub dimension: usize,

    /// Distance metric for similarity calculation
    /// Immutable after collection creation.
    pub metric: DistanceMetric,

    /// Storage data type
    /// Only F32 supported in M8. Reserved for F16/Int8 in M9.
    pub storage_dtype: StorageDtype,
}

impl VectorConfig {
    /// Create a new VectorConfig with validation
    pub fn new(dimension: usize, metric: DistanceMetric) -> Result<Self, VectorError> {
        if dimension == 0 {
            return Err(VectorError::InvalidDimension { dimension });
        }
        Ok(VectorConfig {
            dimension,
            metric,
            storage_dtype: StorageDtype::F32,
        })
    }

    /// Config for OpenAI text-embedding-ada-002 (1536 dims)
    pub fn for_openai_ada() -> Self {
        VectorConfig {
            dimension: 1536,
            metric: DistanceMetric::Cosine,
            storage_dtype: StorageDtype::F32,
        }
    }

    /// Config for OpenAI text-embedding-3-large (3072 dims)
    pub fn for_openai_large() -> Self {
        VectorConfig {
            dimension: 3072,
            metric: DistanceMetric::Cosine,
            storage_dtype: StorageDtype::F32,
        }
    }

    /// Config for MiniLM (384 dims)
    pub fn for_minilm() -> Self {
        VectorConfig {
            dimension: 384,
            metric: DistanceMetric::Cosine,
            storage_dtype: StorageDtype::F32,
        }
    }

    /// Config for sentence-transformers/all-mpnet-base-v2 (768 dims)
    pub fn for_mpnet() -> Self {
        VectorConfig {
            dimension: 768,
            metric: DistanceMetric::Cosine,
            storage_dtype: StorageDtype::F32,
        }
    }
}

/// Storage data type for embeddings
///
/// M8 only supports F32. F16 and Int8 are reserved for M9 quantization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StorageDtype {
    #[default]
    F32,
    // F16,     // M9: Half precision
    // Int8,    // M9: Scalar quantization
}
```

### Acceptance Criteria

- [ ] VectorConfig with dimension, metric, storage_dtype fields
- [ ] `new()` validates dimension > 0
- [ ] Helper constructors for common embedding models
- [ ] StorageDtype enum with F32 (reserved variants commented)
- [ ] Implements Debug, Clone, PartialEq, Eq
- [ ] NO backend-specific fields (Rule 7)

---

## Story #331: DistanceMetric Enum

**File**: `crates/primitives/src/vector/types.rs`

**Deliverable**: Distance metric enum with score normalization

### Implementation

```rust
/// Distance metric for similarity calculation
///
/// All metrics are normalized to "higher = more similar".
/// This normalization is part of the interface contract (Invariant R2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DistanceMetric {
    /// Cosine similarity: dot(a,b) / (||a|| * ||b||)
    /// Range: [-1, 1], higher = more similar
    /// Best for: normalized embeddings, semantic similarity
    #[default]
    Cosine,

    /// Euclidean similarity: 1 / (1 + l2_distance)
    /// Range: (0, 1], higher = more similar
    /// Best for: absolute position comparisons
    Euclidean,

    /// Dot product (raw value)
    /// Range: unbounded, higher = more similar
    /// Best for: pre-normalized embeddings, retrieval
    /// WARNING: Assumes vectors are normalized. Non-normalized vectors
    /// will produce unbounded scores.
    DotProduct,
}

impl DistanceMetric {
    /// Human-readable name for display
    pub fn name(&self) -> &'static str {
        match self {
            DistanceMetric::Cosine => "cosine",
            DistanceMetric::Euclidean => "euclidean",
            DistanceMetric::DotProduct => "dot_product",
        }
    }

    /// Parse from string (case-insensitive)
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "cosine" => Some(DistanceMetric::Cosine),
            "euclidean" | "l2" => Some(DistanceMetric::Euclidean),
            "dot_product" | "dot" | "inner_product" => Some(DistanceMetric::DotProduct),
            _ => None,
        }
    }

    /// Serialization value for WAL/snapshot
    pub fn to_byte(&self) -> u8 {
        match self {
            DistanceMetric::Cosine => 0,
            DistanceMetric::Euclidean => 1,
            DistanceMetric::DotProduct => 2,
        }
    }

    /// Deserialization from WAL/snapshot
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0 => Some(DistanceMetric::Cosine),
            1 => Some(DistanceMetric::Euclidean),
            2 => Some(DistanceMetric::DotProduct),
            _ => None,
        }
    }
}
```

### Acceptance Criteria

- [ ] Three metrics: Cosine, Euclidean, DotProduct
- [ ] All metrics documented with score ranges
- [ ] Serialization to/from byte for WAL/snapshot
- [ ] String parsing for API convenience
- [ ] Implements Debug, Clone, Copy, PartialEq, Eq, Default

---

## Story #332: VectorEntry and VectorMatch Types

**File**: `crates/primitives/src/vector/types.rs`

**Deliverable**: Types for vector storage and search results

### Implementation

```rust
use serde_json::Value as JsonValue;

/// Internal vector identifier (stable within collection)
///
/// IMPORTANT: VectorIds are never reused (Invariant S4).
/// Storage slots may be reused, but the ID value is monotonically increasing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VectorId(pub(crate) u64);

impl VectorId {
    pub fn new(id: u64) -> Self {
        VectorId(id)
    }

    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

/// Vector entry stored in the database
#[derive(Debug, Clone)]
pub struct VectorEntry {
    /// User-provided key (unique within collection)
    pub key: String,

    /// Embedding vector
    pub embedding: Vec<f32>,

    /// Optional JSON metadata
    pub metadata: Option<JsonValue>,

    /// Internal ID (for index backend)
    pub(crate) vector_id: VectorId,

    /// Version for optimistic concurrency
    pub(crate) version: u64,
}

impl VectorEntry {
    /// Create a new VectorEntry
    pub fn new(
        key: String,
        embedding: Vec<f32>,
        metadata: Option<JsonValue>,
        vector_id: VectorId,
    ) -> Self {
        VectorEntry {
            key,
            embedding,
            metadata,
            vector_id,
            version: 1,
        }
    }

    /// Get the embedding dimension
    pub fn dimension(&self) -> usize {
        self.embedding.len()
    }
}

/// Search result entry
///
/// Returned by search operations. Score is always "higher = more similar"
/// regardless of the underlying distance metric.
#[derive(Debug, Clone)]
pub struct VectorMatch {
    /// User-provided key
    pub key: String,

    /// Similarity score (higher = more similar)
    /// This is normalized per Invariant R2.
    pub score: f32,

    /// Optional metadata (if requested and present)
    pub metadata: Option<JsonValue>,
}

impl VectorMatch {
    pub fn new(key: String, score: f32, metadata: Option<JsonValue>) -> Self {
        VectorMatch { key, score, metadata }
    }
}

/// Collection metadata
#[derive(Debug, Clone)]
pub struct CollectionInfo {
    /// Collection name
    pub name: String,

    /// Immutable configuration
    pub config: VectorConfig,

    /// Current vector count
    pub count: usize,

    /// Creation timestamp (microseconds since epoch)
    pub created_at: u64,
}

/// Unique identifier for a collection within a run
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CollectionId {
    pub run_id: RunId,
    pub name: String,
}

impl CollectionId {
    pub fn new(run_id: RunId, name: impl Into<String>) -> Self {
        CollectionId {
            run_id,
            name: name.into(),
        }
    }
}
```

### Acceptance Criteria

- [ ] VectorId with u64 inner value, Ord trait for deterministic ordering
- [ ] VectorEntry with key, embedding, metadata, vector_id, version
- [ ] VectorMatch with key, score, metadata
- [ ] CollectionInfo with name, config, count, created_at
- [ ] CollectionId as (RunId, name) tuple
- [ ] All types implement Debug, Clone

---

## Story #333: MetadataFilter and JsonScalar Types

**File**: `crates/primitives/src/vector/filter.rs` (NEW)

**Deliverable**: Metadata filtering types for search

### Implementation

```rust
use serde_json::Value as JsonValue;
use std::collections::HashMap;

/// Metadata filter for search (M8: equality only)
///
/// M8 supports only top-level field equality filtering.
/// Complex filters (ranges, nested paths, arrays) are deferred to M9.
#[derive(Debug, Clone, Default)]
pub struct MetadataFilter {
    /// Top-level field equality (scalar values only)
    /// All conditions must match (AND semantics)
    pub equals: HashMap<String, JsonScalar>,
}

impl MetadataFilter {
    /// Create an empty filter (matches all)
    pub fn new() -> Self {
        MetadataFilter {
            equals: HashMap::new(),
        }
    }

    /// Add an equality condition
    pub fn eq(mut self, field: impl Into<String>, value: impl Into<JsonScalar>) -> Self {
        self.equals.insert(field.into(), value.into());
        self
    }

    /// Check if metadata matches this filter
    ///
    /// Returns true if all conditions match.
    /// Returns false if metadata is None and filter is non-empty.
    pub fn matches(&self, metadata: &Option<JsonValue>) -> bool {
        if self.equals.is_empty() {
            return true;
        }

        let Some(meta) = metadata else {
            return false;
        };

        let Some(obj) = meta.as_object() else {
            return false;
        };

        for (key, expected) in &self.equals {
            let Some(actual) = obj.get(key) else {
                return false;
            };
            if !expected.matches_json(actual) {
                return false;
            }
        }

        true
    }

    /// Check if filter is empty (matches all)
    pub fn is_empty(&self) -> bool {
        self.equals.is_empty()
    }
}

/// JSON scalar value for filtering
///
/// Only scalar values can be used in equality filters.
/// Complex types (arrays, objects) are not supported in M8.
#[derive(Debug, Clone, PartialEq)]
pub enum JsonScalar {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
}

impl JsonScalar {
    /// Check if this scalar matches a JSON value
    pub fn matches_json(&self, value: &JsonValue) -> bool {
        match (self, value) {
            (JsonScalar::Null, JsonValue::Null) => true,
            (JsonScalar::Bool(a), JsonValue::Bool(b)) => a == b,
            (JsonScalar::Number(a), JsonValue::Number(b)) => {
                b.as_f64().map_or(false, |n| (a - n).abs() < f64::EPSILON)
            }
            (JsonScalar::String(a), JsonValue::String(b)) => a == b,
            _ => false,
        }
    }
}

// Convenience conversions
impl From<bool> for JsonScalar {
    fn from(b: bool) -> Self {
        JsonScalar::Bool(b)
    }
}

impl From<i64> for JsonScalar {
    fn from(n: i64) -> Self {
        JsonScalar::Number(n as f64)
    }
}

impl From<f64> for JsonScalar {
    fn from(n: f64) -> Self {
        JsonScalar::Number(n)
    }
}

impl From<String> for JsonScalar {
    fn from(s: String) -> Self {
        JsonScalar::String(s)
    }
}

impl From<&str> for JsonScalar {
    fn from(s: &str) -> Self {
        JsonScalar::String(s.to_string())
    }
}

impl From<()> for JsonScalar {
    fn from(_: ()) -> Self {
        JsonScalar::Null
    }
}
```

### Acceptance Criteria

- [ ] MetadataFilter with equals HashMap
- [ ] Builder pattern with `eq()` method
- [ ] `matches()` checks all conditions (AND semantics)
- [ ] Returns false if metadata is None and filter is non-empty
- [ ] JsonScalar with Null, Bool, Number, String variants
- [ ] Number comparison handles floating point precision
- [ ] Convenient From implementations

---

## Story #334: VectorError Enum

**File**: `crates/primitives/src/vector/error.rs` (NEW)

**Deliverable**: Error types for Vector primitive

### Implementation

```rust
use thiserror::Error;

/// Errors specific to the Vector primitive
#[derive(Debug, Error)]
pub enum VectorError {
    #[error("Collection not found: {name}")]
    CollectionNotFound { name: String },

    #[error("Collection already exists: {name}")]
    CollectionAlreadyExists { name: String },

    #[error("Dimension mismatch: expected {expected}, got {got}")]
    DimensionMismatch { expected: usize, got: usize },

    #[error("Invalid dimension: {dimension} (must be > 0)")]
    InvalidDimension { dimension: usize },

    #[error("Vector not found: {key}")]
    VectorNotFound { key: String },

    #[error("Empty embedding")]
    EmptyEmbedding,

    #[error("Invalid collection name: {name} ({reason})")]
    InvalidCollectionName { name: String, reason: String },

    #[error("Invalid key: {key} ({reason})")]
    InvalidKey { key: String, reason: String },

    #[error("Collection config mismatch: {field} cannot be changed")]
    ConfigMismatch { field: String },

    #[error("Search limit exceeded: requested {requested}, max {max}")]
    SearchLimitExceeded { requested: usize, max: usize },

    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("Transaction error: {0}")]
    Transaction(#[from] TransactionError),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

impl VectorError {
    /// Check if this error indicates the vector/collection was not found
    pub fn is_not_found(&self) -> bool {
        matches!(
            self,
            VectorError::CollectionNotFound { .. } | VectorError::VectorNotFound { .. }
        )
    }

    /// Check if this error is a validation error
    pub fn is_validation_error(&self) -> bool {
        matches!(
            self,
            VectorError::DimensionMismatch { .. }
                | VectorError::InvalidDimension { .. }
                | VectorError::EmptyEmbedding
                | VectorError::InvalidCollectionName { .. }
                | VectorError::InvalidKey { .. }
                | VectorError::ConfigMismatch { .. }
        )
    }
}

/// Result type alias for Vector operations
pub type VectorResult<T> = Result<T, VectorError>;
```

### Acceptance Criteria

- [ ] All error variants from M8_ARCHITECTURE.md
- [ ] Uses thiserror for Display impl
- [ ] From impls for StorageError, TransactionError
- [ ] Helper methods: is_not_found(), is_validation_error()
- [ ] VectorResult type alias

---

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vector_config_validation() {
        // Valid config
        let config = VectorConfig::new(768, DistanceMetric::Cosine).unwrap();
        assert_eq!(config.dimension, 768);

        // Invalid dimension
        let result = VectorConfig::new(0, DistanceMetric::Cosine);
        assert!(matches!(result, Err(VectorError::InvalidDimension { .. })));
    }

    #[test]
    fn test_distance_metric_serialization() {
        for metric in [DistanceMetric::Cosine, DistanceMetric::Euclidean, DistanceMetric::DotProduct] {
            let byte = metric.to_byte();
            let parsed = DistanceMetric::from_byte(byte).unwrap();
            assert_eq!(metric, parsed);
        }
    }

    #[test]
    fn test_metadata_filter_matches() {
        let filter = MetadataFilter::new()
            .eq("category", "document")
            .eq("year", 2024);

        // Matching metadata
        let meta = serde_json::json!({
            "category": "document",
            "year": 2024,
            "extra": "ignored"
        });
        assert!(filter.matches(&Some(meta)));

        // Missing field
        let meta = serde_json::json!({
            "category": "document"
        });
        assert!(!filter.matches(&Some(meta)));

        // Wrong value
        let meta = serde_json::json!({
            "category": "image",
            "year": 2024
        });
        assert!(!filter.matches(&Some(meta)));

        // None metadata
        assert!(!filter.matches(&None));

        // Empty filter matches all
        let empty_filter = MetadataFilter::new();
        assert!(empty_filter.matches(&None));
    }

    #[test]
    fn test_vector_id_ordering() {
        let id1 = VectorId::new(1);
        let id2 = VectorId::new(2);
        let id3 = VectorId::new(1);

        assert!(id1 < id2);
        assert_eq!(id1, id3);

        // BTreeMap ordering for determinism
        use std::collections::BTreeMap;
        let mut map = BTreeMap::new();
        map.insert(id2, "second");
        map.insert(id1, "first");

        let keys: Vec<_> = map.keys().collect();
        assert_eq!(keys, vec![&id1, &id2]); // Sorted order
    }
}
```

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/primitives/src/vector/mod.rs` | CREATE - Module entry point |
| `crates/primitives/src/vector/types.rs` | CREATE - Core types |
| `crates/primitives/src/vector/filter.rs` | CREATE - Filter types |
| `crates/primitives/src/vector/error.rs` | CREATE - Error types |
| `crates/primitives/src/lib.rs` | MODIFY - Export vector module |
