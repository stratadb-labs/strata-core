//! Vector primitive types and operations
//!
//! This module provides vector storage and similarity search capabilities.
//! It includes:
//!
//! - **VectorStore**: Main facade for vector storage and search
//! - **VectorConfig**: Collection configuration (dimension, metric, storage type)
//! - **DistanceMetric**: Similarity metrics (Cosine, Euclidean, DotProduct)
//! - **VectorEntry/Match**: Vector storage and search result types
//! - **VectorHeap**: Contiguous embedding storage with slot reuse
//! - **VectorIndexBackend**: Trait for swappable index implementations
//! - **BruteForceBackend**: O(n) brute-force search
//! - **MetadataFilter**: Equality-based metadata filtering
//! - **VectorError**: Error types for vector operations
//!
//! ## Recovery
//!
//! VectorStore participates in Database recovery via the recovery participant
//! mechanism. Call `register_vector_recovery()` during application startup
//! to enable vector state recovery after database restart.

pub mod backend;
pub mod brute_force;
pub mod collection;
pub mod distance;
pub mod error;
pub mod filter;
pub mod heap;
pub mod hnsw;
pub mod recovery;
pub mod segmented;
pub mod snapshot;
pub mod store;
pub mod types;
pub mod wal;

pub use backend::{IndexBackendFactory, VectorIndexBackend};
pub use brute_force::BruteForceBackend;
pub use collection::{
    validate_collection_name, validate_system_collection_name, validate_vector_key,
};
pub use error::{VectorError, VectorResult};
pub use filter::{FilterCondition, FilterOp, JsonScalar, MetadataFilter};
pub use heap::VectorHeap;
pub use hnsw::{HnswBackend, HnswConfig};
pub use segmented::{SegmentedHnswBackend, SegmentedHnswConfig};
pub use recovery::register_vector_recovery;
pub use snapshot::{CollectionSnapshotHeader, VECTOR_SNAPSHOT_VERSION};
pub use store::{RecoveryStats, VectorBackendState, VectorStore};
pub use types::{
    CollectionId, CollectionInfo, CollectionRecord, DistanceMetric, StorageDtype, VectorConfig,
    VectorConfigSerde, VectorEntry, VectorId, VectorMatch, VectorMatchWithSource, VectorRecord,
};
pub use wal::{
    create_wal_collection_create, create_wal_collection_delete, create_wal_delete,
    create_wal_upsert, create_wal_upsert_with_source, VectorWalReplayer, WalVectorCollectionCreate,
    WalVectorCollectionDelete, WalVectorDelete, WalVectorUpsert,
};
