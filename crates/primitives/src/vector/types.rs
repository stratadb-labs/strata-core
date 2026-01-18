//! Core types for the Vector primitive
//!
//! This module defines the foundational types used throughout
//! the vector storage and search system.

use crate::vector::error::{VectorError, VectorResult};
use in_mem_core::RunId;
use serde_json::Value as JsonValue;

// ============================================================================
// Story #395: DistanceMetric
// ============================================================================

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
    pub fn parse(s: &str) -> Option<Self> {
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

// ============================================================================
// Story #394: VectorConfig and StorageDtype
// ============================================================================

/// Storage data type for embeddings
///
/// Only F32 supported initially. F16 and Int8 are reserved for future quantization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StorageDtype {
    /// 32-bit floating point (default)
    #[default]
    F32,
    // F16,     // Reserved for half precision (value = 1)
    // Int8,    // Reserved for scalar quantization (value = 2)
}

impl StorageDtype {
    /// Serialization value for WAL/snapshot
    pub fn to_byte(&self) -> u8 {
        match self {
            StorageDtype::F32 => 0,
            // StorageDtype::F16 => 1,
            // StorageDtype::Int8 => 2,
        }
    }

    /// Deserialization from WAL/snapshot
    ///
    /// Returns None for unknown values, allowing forward compatibility
    /// when new storage types are added.
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0 => Some(StorageDtype::F32),
            // 1 => Some(StorageDtype::F16),
            // 2 => Some(StorageDtype::Int8),
            _ => None,
        }
    }
}

/// Collection configuration - immutable after creation
///
/// IMPORTANT: This struct must NOT contain backend-specific fields.
/// HNSW parameters (ef_construction, M, etc.) belong in HnswConfig, not here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorConfig {
    /// Embedding dimension (e.g., 384, 768, 1536)
    /// Must be > 0. Immutable after collection creation.
    pub dimension: usize,

    /// Distance metric for similarity calculation
    /// Immutable after collection creation.
    pub metric: DistanceMetric,

    /// Storage data type
    /// Only F32 supported initially. Reserved for F16/Int8 in future.
    pub storage_dtype: StorageDtype,
}

impl VectorConfig {
    /// Create a new VectorConfig with validation
    pub fn new(dimension: usize, metric: DistanceMetric) -> VectorResult<Self> {
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

// ============================================================================
// Story #396: VectorId, VectorEntry, VectorMatch, CollectionInfo, CollectionId
// ============================================================================

/// Internal vector identifier (stable within collection)
///
/// IMPORTANT: VectorIds are never reused (Invariant S4).
/// Storage slots may be reused, but the ID value is monotonically increasing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VectorId(pub(crate) u64);

impl VectorId {
    /// Create a new VectorId
    pub fn new(id: u64) -> Self {
        VectorId(id)
    }

    /// Get the underlying u64 value
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for VectorId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "VectorId({})", self.0)
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

    /// Get the vector ID
    pub fn vector_id(&self) -> VectorId {
        self.vector_id
    }

    /// Get the version
    pub fn version(&self) -> u64 {
        self.version
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
    /// Create a new VectorMatch
    pub fn new(key: String, score: f32, metadata: Option<JsonValue>) -> Self {
        VectorMatch {
            key,
            score,
            metadata,
        }
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
    /// Run ID this collection belongs to
    pub run_id: RunId,
    /// Collection name
    pub name: String,
}

impl CollectionId {
    /// Create a new CollectionId
    pub fn new(run_id: RunId, name: impl Into<String>) -> Self {
        CollectionId {
            run_id,
            name: name.into(),
        }
    }
}

// Manual Ord implementation for BTreeMap usage
// Orders by run_id bytes, then by name
impl Ord for CollectionId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.run_id
            .as_bytes()
            .cmp(other.run_id.as_bytes())
            .then(self.name.cmp(&other.name))
    }
}

impl PartialOrd for CollectionId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

// ============================================================================
// Story #339: VectorRecord and CollectionRecord
// ============================================================================

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Get current time in microseconds since Unix epoch
fn now_micros() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_micros() as u64
}

/// Metadata stored in KV (MessagePack serialized)
///
/// This is stored separately from the embedding for:
/// 1. Transaction participation (KV has full tx support)
/// 2. Flexible schema (JSON metadata)
/// 3. WAL integration (reuses existing infrastructure)
///
/// The embedding is stored in VectorHeap for cache-friendly scanning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorRecord {
    /// Internal vector ID (maps to VectorHeap)
    pub vector_id: u64,

    /// User-provided metadata (optional)
    pub metadata: Option<JsonValue>,

    /// Version for optimistic concurrency
    pub version: u64,

    /// Creation timestamp (microseconds since epoch)
    pub created_at: u64,

    /// Last update timestamp (microseconds since epoch)
    pub updated_at: u64,
}

impl VectorRecord {
    /// Create a new VectorRecord
    pub fn new(vector_id: VectorId, metadata: Option<JsonValue>) -> Self {
        let now = now_micros();
        VectorRecord {
            vector_id: vector_id.as_u64(),
            metadata,
            version: 1,
            created_at: now,
            updated_at: now,
        }
    }

    /// Update metadata and version
    pub fn update(&mut self, metadata: Option<JsonValue>) {
        self.metadata = metadata;
        self.version += 1;
        self.updated_at = now_micros();
    }

    /// Get VectorId
    pub fn vector_id(&self) -> VectorId {
        VectorId::new(self.vector_id)
    }

    /// Serialize to bytes (MessagePack)
    pub fn to_bytes(&self) -> Result<Vec<u8>, crate::vector::VectorError> {
        rmp_serde::to_vec(self)
            .map_err(|e| crate::vector::VectorError::Serialization(e.to_string()))
    }

    /// Deserialize from bytes (MessagePack)
    pub fn from_bytes(data: &[u8]) -> Result<Self, crate::vector::VectorError> {
        rmp_serde::from_slice(data)
            .map_err(|e| crate::vector::VectorError::Serialization(e.to_string()))
    }
}

/// Collection configuration stored in KV
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionRecord {
    /// Collection configuration (serializable form)
    pub config: VectorConfigSerde,

    /// Creation timestamp
    pub created_at: u64,
}

impl CollectionRecord {
    /// Create a new CollectionRecord
    pub fn new(config: &VectorConfig) -> Self {
        CollectionRecord {
            config: VectorConfigSerde::from(config),
            created_at: now_micros(),
        }
    }

    /// Serialize to bytes (MessagePack)
    pub fn to_bytes(&self) -> Result<Vec<u8>, crate::vector::VectorError> {
        rmp_serde::to_vec(self)
            .map_err(|e| crate::vector::VectorError::Serialization(e.to_string()))
    }

    /// Deserialize from bytes (MessagePack)
    pub fn from_bytes(data: &[u8]) -> Result<Self, crate::vector::VectorError> {
        rmp_serde::from_slice(data)
            .map_err(|e| crate::vector::VectorError::Serialization(e.to_string()))
    }
}

/// Serializable version of VectorConfig
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorConfigSerde {
    /// Embedding dimension
    pub dimension: usize,
    /// Distance metric (as byte)
    pub metric: u8,
    /// Storage data type (as byte)
    pub storage_dtype: u8,
}

impl From<&VectorConfig> for VectorConfigSerde {
    fn from(config: &VectorConfig) -> Self {
        VectorConfigSerde {
            dimension: config.dimension,
            metric: config.metric.to_byte(),
            storage_dtype: config.storage_dtype.to_byte(),
        }
    }
}

impl TryFrom<VectorConfigSerde> for VectorConfig {
    type Error = crate::vector::VectorError;

    fn try_from(serde: VectorConfigSerde) -> Result<Self, Self::Error> {
        let metric = DistanceMetric::from_byte(serde.metric).ok_or_else(|| {
            crate::vector::VectorError::Serialization(format!(
                "Invalid metric byte: {}",
                serde.metric
            ))
        })?;

        // Default to F32 for forward compatibility with old WAL entries
        let storage_dtype = StorageDtype::from_byte(serde.storage_dtype)
            .unwrap_or(StorageDtype::F32);

        Ok(VectorConfig {
            dimension: serde.dimension,
            metric,
            storage_dtype,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================
    // DistanceMetric Tests (#395)
    // ========================================

    #[test]
    fn test_distance_metric_serialization_roundtrip() {
        for metric in [
            DistanceMetric::Cosine,
            DistanceMetric::Euclidean,
            DistanceMetric::DotProduct,
        ] {
            let byte = metric.to_byte();
            let parsed = DistanceMetric::from_byte(byte).unwrap();
            assert_eq!(metric, parsed);
        }
    }

    #[test]
    fn test_distance_metric_from_string() {
        assert_eq!(
            DistanceMetric::parse("cosine"),
            Some(DistanceMetric::Cosine)
        );
        assert_eq!(
            DistanceMetric::parse("COSINE"),
            Some(DistanceMetric::Cosine)
        );
        assert_eq!(
            DistanceMetric::parse("euclidean"),
            Some(DistanceMetric::Euclidean)
        );
        assert_eq!(DistanceMetric::parse("l2"), Some(DistanceMetric::Euclidean));
        assert_eq!(
            DistanceMetric::parse("dot_product"),
            Some(DistanceMetric::DotProduct)
        );
        assert_eq!(
            DistanceMetric::parse("dot"),
            Some(DistanceMetric::DotProduct)
        );
        assert_eq!(
            DistanceMetric::parse("inner_product"),
            Some(DistanceMetric::DotProduct)
        );
        assert_eq!(DistanceMetric::parse("invalid"), None);
    }

    #[test]
    fn test_distance_metric_default() {
        assert_eq!(DistanceMetric::default(), DistanceMetric::Cosine);
    }

    #[test]
    fn test_distance_metric_name() {
        assert_eq!(DistanceMetric::Cosine.name(), "cosine");
        assert_eq!(DistanceMetric::Euclidean.name(), "euclidean");
        assert_eq!(DistanceMetric::DotProduct.name(), "dot_product");
    }

    #[test]
    fn test_distance_metric_from_byte_invalid() {
        assert_eq!(DistanceMetric::from_byte(3), None);
        assert_eq!(DistanceMetric::from_byte(255), None);
    }

    // ========================================
    // VectorConfig Tests (#394)
    // ========================================

    #[test]
    fn test_vector_config_valid() {
        let config = VectorConfig::new(768, DistanceMetric::Cosine).unwrap();
        assert_eq!(config.dimension, 768);
        assert_eq!(config.metric, DistanceMetric::Cosine);
        assert_eq!(config.storage_dtype, StorageDtype::F32);
    }

    #[test]
    fn test_vector_config_zero_dimension() {
        let result = VectorConfig::new(0, DistanceMetric::Cosine);
        assert!(matches!(
            result,
            Err(VectorError::InvalidDimension { dimension: 0 })
        ));
    }

    #[test]
    fn test_preset_configs() {
        assert_eq!(VectorConfig::for_openai_ada().dimension, 1536);
        assert_eq!(VectorConfig::for_openai_large().dimension, 3072);
        assert_eq!(VectorConfig::for_minilm().dimension, 384);
        assert_eq!(VectorConfig::for_mpnet().dimension, 768);
    }

    #[test]
    fn test_vector_config_equality() {
        let config1 = VectorConfig::new(768, DistanceMetric::Cosine).unwrap();
        let config2 = VectorConfig::new(768, DistanceMetric::Cosine).unwrap();
        let config3 = VectorConfig::new(384, DistanceMetric::Cosine).unwrap();

        assert_eq!(config1, config2);
        assert_ne!(config1, config3);
    }

    #[test]
    fn test_storage_dtype_default() {
        assert_eq!(StorageDtype::default(), StorageDtype::F32);
    }

    // ========================================
    // VectorId Tests (#396)
    // ========================================

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

    #[test]
    fn test_vector_id_display() {
        let id = VectorId::new(42);
        assert_eq!(format!("{}", id), "VectorId(42)");
    }

    #[test]
    fn test_vector_id_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(VectorId::new(1));
        set.insert(VectorId::new(2));
        set.insert(VectorId::new(1)); // Duplicate

        assert_eq!(set.len(), 2);
    }

    // ========================================
    // VectorEntry Tests (#396)
    // ========================================

    #[test]
    fn test_vector_entry_dimension() {
        let entry = VectorEntry::new(
            "test".to_string(),
            vec![1.0, 2.0, 3.0],
            None,
            VectorId::new(1),
        );
        assert_eq!(entry.dimension(), 3);
    }

    #[test]
    fn test_vector_entry_with_metadata() {
        let metadata = serde_json::json!({"category": "test"});
        let entry = VectorEntry::new(
            "test".to_string(),
            vec![1.0, 2.0, 3.0],
            Some(metadata.clone()),
            VectorId::new(1),
        );
        assert_eq!(entry.metadata, Some(metadata));
        assert_eq!(entry.version(), 1);
    }

    #[test]
    fn test_vector_entry_vector_id() {
        let entry = VectorEntry::new(
            "test".to_string(),
            vec![1.0, 2.0, 3.0],
            None,
            VectorId::new(42),
        );
        assert_eq!(entry.vector_id(), VectorId::new(42));
    }

    // ========================================
    // VectorMatch Tests (#396)
    // ========================================

    #[test]
    fn test_vector_match_creation() {
        let m = VectorMatch::new("key".to_string(), 0.95, None);
        assert_eq!(m.key, "key");
        assert!((m.score - 0.95).abs() < f32::EPSILON);
        assert!(m.metadata.is_none());
    }

    #[test]
    fn test_vector_match_with_metadata() {
        let metadata = serde_json::json!({"source": "doc1"});
        let m = VectorMatch::new("key".to_string(), 0.95, Some(metadata.clone()));
        assert_eq!(m.metadata, Some(metadata));
    }

    // ========================================
    // CollectionInfo Tests (#396)
    // ========================================

    #[test]
    fn test_collection_info() {
        let config = VectorConfig::new(768, DistanceMetric::Cosine).unwrap();
        let info = CollectionInfo {
            name: "test_collection".to_string(),
            config: config.clone(),
            count: 100,
            created_at: 1234567890,
        };

        assert_eq!(info.name, "test_collection");
        assert_eq!(info.config, config);
        assert_eq!(info.count, 100);
        assert_eq!(info.created_at, 1234567890);
    }

    // ========================================
    // CollectionId Tests (#396)
    // ========================================

    #[test]
    fn test_collection_id() {
        let run_id = RunId::new();
        let id = CollectionId::new(run_id, "my_collection");

        assert_eq!(id.run_id, run_id);
        assert_eq!(id.name, "my_collection");
    }

    #[test]
    fn test_collection_id_equality() {
        let run_id = RunId::new();
        let id1 = CollectionId::new(run_id, "collection1");
        let id2 = CollectionId::new(run_id, "collection1");
        let id3 = CollectionId::new(run_id, "collection2");

        assert_eq!(id1, id2);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_collection_id_hash() {
        use std::collections::HashSet;
        let run_id = RunId::new();

        let mut set = HashSet::new();
        set.insert(CollectionId::new(run_id, "collection1"));
        set.insert(CollectionId::new(run_id, "collection2"));
        set.insert(CollectionId::new(run_id, "collection1")); // Duplicate

        assert_eq!(set.len(), 2);
    }
}
