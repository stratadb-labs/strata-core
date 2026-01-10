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

#![warn(missing_docs)]
#![warn(clippy::all)]

// Module declarations (will be implemented in future stories)
// pub mod types;      // Story #7, #8
// pub mod value;      // Story #9
// pub mod error;      // Story #10
pub mod traits; // Story #11

// Re-export commonly used traits
pub use traits::{SnapshotView, Storage};

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
