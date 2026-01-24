//! Primitive types for Strata
//!
//! This module defines the canonical data structures for all primitives.
//! These types are shared between the `engine` and `primitives` crates.
//!
//! ## Design Principle
//!
//! - **strata-core** defines canonical semantic types (this module)
//! - **strata-primitives** provides stateless facades and implementation logic
//! - **strata-engine** orchestrates transactions and recovery
//!
//! All crates share the same type definitions from core.

pub mod event;
pub mod state;
pub mod vector;

// Re-export all types at module level
pub use event::{ChainVerification, Event};
pub use state::State;
pub use vector::{
    CollectionId, CollectionInfo, DistanceMetric, JsonScalar, MetadataFilter, StorageDtype,
    VectorConfig, VectorEntry, VectorId, VectorMatch,
};
