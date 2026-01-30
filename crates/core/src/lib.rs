//! Core types and traits for Strata
//!
//! This crate defines the foundational types used throughout the system:
//! - RunId: Unique identifier for agent runs
//! - Namespace: Hierarchical namespace (tenant/app/agent/run)
//! - Key: Composite key with type tagging
//! - TypeTag: Discriminates between primitive types
//! - Value: Unified value enum for all data types
//! - Error: Error type hierarchy
//! - Traits: Core trait definitions (Storage, SnapshotView)
//! - Primitive types: Event, State, JSON, Vector types (in `primitives` module)
//! - Contract types: EntityRef, Versioned<T>, Version, Timestamp, PrimitiveType, RunName

#![warn(missing_docs)]
#![warn(clippy::all)]

// Module declarations
pub mod contract; // contract types
pub mod error;
pub mod primitive_ext; // extension trait for primitives to integrate with storage/durability
pub mod primitives; // primitive types (Event, State, Vector, JSON types)
pub mod run_types; // Run lifecycle types
pub mod search_types; // search types (EntityRef/PrimitiveType re-exports only; types moved to engine)
pub mod traits;
pub mod types;
pub mod value;

// Re-export commonly used types and traits
pub use error::{
    ConstraintReason, DetailValue, ErrorCode, ErrorDetails, StrataError, StrataResult,
};
pub use run_types::{RunEventOffsets, RunMetadata, RunStatus};
pub use traits::{SnapshotView, Storage};
pub use types::{Key, Namespace, RunId, TypeTag};
pub use value::Value;

// Re-export contract types at crate root for convenience
pub use contract::{
    EntityRef, PrimitiveType, RunName, RunNameError, Timestamp, Version, Versioned,
    VersionedValue, MAX_RUN_NAME_LENGTH,
};

// Re-export primitive extension trait and helpers
pub use primitive_ext::{
    is_future_wal_type, is_vector_wal_type, primitive_for_wal_type, primitive_type_ids, wal_ranges,
    PrimitiveExtError, PrimitiveStorageExt,
};

// Re-export primitive types at crate root for convenience
pub use primitives::{
    // Event types
    ChainVerification, Event,
    // JSON types
    apply_patches, delete_at_path, get_at_path, get_at_path_mut, merge_patch, set_at_path,
    JsonLimitError, JsonPatch, JsonPath, JsonPathError, JsonValue, PathParseError, PathSegment,
    MAX_ARRAY_SIZE, MAX_DOCUMENT_SIZE, MAX_NESTING_DEPTH, MAX_PATH_LENGTH,
    // State types
    State,
    // Vector types
    CollectionId, CollectionInfo, DistanceMetric, JsonScalar, MetadataFilter, StorageDtype,
    VectorConfig, VectorEntry, VectorId, VectorMatch,
};

