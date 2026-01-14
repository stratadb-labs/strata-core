//! in-mem: High-performance in-memory database for AI agents
//!
//! This crate re-exports the main components of the in-mem database:
//! - `in_mem_core`: Core types and traits
//! - `in_mem_engine`: Database engine with transaction support
//! - `in_mem_storage`: Storage layer
//! - `in_mem_concurrency`: OCC and snapshot isolation
//! - `in_mem_durability`: WAL and persistence

pub use in_mem_concurrency as concurrency;
pub use in_mem_core as core;
pub use in_mem_durability as durability;
pub use in_mem_engine as engine;
pub use in_mem_storage as storage;

// Re-export commonly used types
pub use in_mem_core::types::{Key, Namespace, RunId, TypeTag};
pub use in_mem_core::value::Value;
pub use in_mem_core::VersionedValue;
pub use in_mem_engine::Database;
