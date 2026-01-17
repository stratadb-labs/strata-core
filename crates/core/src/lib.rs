//! Core types and traits for in-mem
//!
//! This crate defines the foundational types used throughout the system:
//! - RunId: Unique identifier for agent runs
//! - Namespace: Hierarchical namespace (tenant/app/agent/run)
//! - Key: Composite key with type tagging
//! - TypeTag: Discriminates between primitive types
//! - Value: Unified value enum for all data types
//! - Error: Error type hierarchy
//! - Traits: Core trait definitions (Storage, SnapshotView)
//! - JSON types (M5): JsonValue, JsonPath, JsonPatch, JsonDocId
//! - JSON limits (M5): MAX_DOCUMENT_SIZE, MAX_NESTING_DEPTH, MAX_PATH_LENGTH, MAX_ARRAY_SIZE
//! - Search types (M6): SearchRequest, SearchResponse, SearchHit, DocRef, PrimitiveKind

#![warn(missing_docs)]
#![warn(clippy::all)]

// Module declarations
pub mod error; // Story #10
pub mod json; // M5 JSON types
pub mod run_types; // Run lifecycle types
pub mod search_types; // M6 search types
pub mod traits; // Story #11
pub mod types; // Story #7, #8
pub mod value; // Story #9

// Re-export commonly used types and traits
pub use error::{Error, Result};
pub use json::{
    apply_patches, delete_at_path, get_at_path, get_at_path_mut, set_at_path, JsonPatch, JsonPath,
    JsonPathError, JsonValue, LimitError, PathParseError, PathSegment, MAX_ARRAY_SIZE,
    MAX_DOCUMENT_SIZE, MAX_NESTING_DEPTH, MAX_PATH_LENGTH,
};
pub use run_types::{RunMetadata, RunStatus, RunEventOffsets};
pub use search_types::{
    DocRef, PrimitiveKind, SearchBudget, SearchHit, SearchMode, SearchRequest, SearchResponse,
    SearchStats,
};
pub use traits::{SnapshotView, Storage};
pub use types::{JsonDocId, Key, Namespace, RunId, TypeTag};
pub use value::{Timestamp, Value, VersionedValue};

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
