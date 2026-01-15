//! Storage layer for in-mem
//!
//! This crate implements the unified storage backend with:
//! - UnifiedStore: BTreeMap-based storage with RwLock (M3)
//! - ShardedStore: DashMap + HashMap for M4 performance
//! - Secondary indices (run_index, type_index)
//! - TTL index for expiration
//! - TTL cleaner background task
//! - Version management with AtomicU64
//! - ClonedSnapshotView implementation
//!
//! # M4 Performance
//!
//! The `ShardedStore` provides improved concurrency:
//! - Lock-free reads via DashMap
//! - Per-RunId sharding (no cross-run contention)
//! - FxHashMap for O(1) lookups

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod cleaner;
pub mod index;
pub mod sharded;
pub mod snapshot;
pub mod ttl;
pub mod unified;

pub use cleaner::TTLCleaner;
pub use index::{RunIndex, TypeIndex};
pub use sharded::{Shard, ShardedSnapshot, ShardedStore};
pub use snapshot::ClonedSnapshotView;
pub use ttl::TTLIndex;
pub use unified::UnifiedStore;
