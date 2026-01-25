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
//! - JSON types: JsonValue, JsonPath, JsonPatch, JsonDocId
//! - JSON limits: MAX_DOCUMENT_SIZE, MAX_NESTING_DEPTH, MAX_PATH_LENGTH, MAX_ARRAY_SIZE
//! - Search types: SearchRequest, SearchResponse, SearchHit, DocRef, PrimitiveKind
//! - Contract types: EntityRef, Versioned<T>, Version, Timestamp, PrimitiveType, RunName

#![warn(missing_docs)]
#![warn(clippy::all)]

// Module declarations
pub mod api_error; // API-level error types for wire encoding
pub mod contract; // contract types
pub mod error;
pub mod json; // JSON types
pub mod primitives; // primitive types (Event, State, Vector types)
pub mod run_types; // Run lifecycle types
pub mod search_types; // search types
pub mod traits;
pub mod types;
pub mod value;

// Re-export commonly used types and traits
pub use api_error::{ApiError, WireError};
pub use error::{
    ConstraintReason, DetailValue, Error, ErrorCode, ErrorDetails, Result, StrataError,
    StrataResult,
};
pub use json::{
    apply_patches, delete_at_path, get_at_path, get_at_path_mut, merge_patch, set_at_path,
    JsonPatch, JsonPath, JsonPathError, JsonValue, LimitError, PathParseError, PathSegment,
    MAX_ARRAY_SIZE, MAX_DOCUMENT_SIZE, MAX_NESTING_DEPTH, MAX_PATH_LENGTH,
};
pub use run_types::{RunEventOffsets, RunMetadata, RunStatus};
pub use search_types::{SearchBudget, SearchHit, SearchMode, SearchRequest, SearchResponse, SearchStats};
pub use traits::{SnapshotView, Storage};
pub use types::{JsonDocId, Key, Namespace, RunId, TypeTag};
pub use value::Value;

// Re-export contract types at crate root for convenience
pub use contract::{
    DocRef, EntityRef, PrimitiveType, RunName, RunNameError, Timestamp, Version, Versioned,
    VersionedValue, MAX_RUN_NAME_LENGTH,
};

// Re-export primitive types at crate root for convenience
pub use primitives::{
    ChainVerification, CollectionId, CollectionInfo, DistanceMetric, Event, JsonScalar,
    MetadataFilter, State, StorageDtype, VectorConfig, VectorEntry,
    VectorId, VectorMatch,
};

// Backwards compatibility: PrimitiveKind is now PrimitiveType
#[doc(hidden)]
#[deprecated(since = "0.9.0", note = "Use PrimitiveType instead")]
pub type PrimitiveKind = PrimitiveType;

/// Placeholder for core functionality
/// This will be populated by stories #7-11
pub fn placeholder() {
    // This crate will contain core types once implemented
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_placeholder() {
        placeholder();
    }
}
