//! Strata: High-performance in-memory database for AI agents
//!
//! This crate re-exports the main components of the Strata database:
//! - `strata_core`: Core types and traits
//! - `strata_engine`: Database engine with transaction support
//! - `strata_storage`: Storage layer
//! - `strata_concurrency`: OCC and snapshot isolation
//! - `strata_durability`: WAL and persistence

pub use strata_concurrency as concurrency;
pub use strata_core as core;
pub use strata_durability as durability;
pub use strata_engine as engine;
pub use strata_storage as storage;

// Re-export commonly used types
pub use strata_core::types::{Key, Namespace, RunId, TypeTag};
pub use strata_core::value::Value;
pub use strata_core::VersionedValue;
pub use strata_engine::Database;
