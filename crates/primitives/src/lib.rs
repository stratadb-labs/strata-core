//! Primitives layer for in-mem
//!
//! Provides high-level primitives as stateless facades over the Database engine:
//! - **KVStore**: General-purpose key-value storage
//! - **EventLog**: Immutable append-only event stream with causal hash chaining
//! - **StateCell**: CAS-based versioned cells for coordination
//! - **TraceStore**: Structured reasoning traces with indexing
//! - **RunIndex**: Run lifecycle management
//! - **VectorStore**: Vector storage with similarity search and collection management
//!
//! ## Design Principle: Stateless Facades
//!
//! All primitives are logically stateful but operationally stateless.
//! They hold only an `Arc<Database>` reference and delegate all operations
//! to the transactional engine. This means:
//!
//! - Multiple primitive instances on the same Database are safe
//! - No warm-up or cache invalidation concerns
//! - Idempotent retry works correctly
//! - Replay produces same results
//!
//! ## Run Isolation
//!
//! Every operation is scoped to a `RunId`. Different runs cannot see
//! each other's data. This is enforced through key prefix isolation.
//!
//! ## Cross-Primitive Transactions
//!
//! Primitives can be combined within a single transaction using extension traits:
//!
//! ```rust,ignore
//! use in_mem_primitives::extensions::*;
//!
//! db.transaction(run_id, |txn| {
//!     txn.kv_put("key", value)?;
//!     txn.event_append("type", payload)?;
//!     txn.state_cas("cell", version, new_value)?;
//!     Ok(())
//! })?;
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod event_log;
pub mod extensions;
pub mod json_store;
pub mod kv;
pub mod run_index;
pub mod searchable;
pub mod state_cell;
pub mod trace;
pub mod vector;

// Re-exports - primitives are exported as they're implemented
pub use event_log::{ChainVerification, Event, EventLog};
pub use json_store::{JsonDoc, JsonStore};
pub use kv::{KVStore, KVTransaction};
pub use run_index::{RunIndex, RunMetadata, RunStatus};
pub use searchable::{build_search_response, SearchCandidate, Searchable, SimpleScorer};
pub use state_cell::{State, StateCell};
pub use trace::{Trace, TraceStore, TraceTree, TraceType};
pub use vector::{
    register_vector_recovery, validate_collection_name, validate_vector_key, BruteForceBackend,
    CollectionId, CollectionInfo, CollectionRecord, DistanceMetric, IndexBackendFactory,
    JsonScalar, MetadataFilter, StorageDtype, VectorConfig, VectorConfigSerde, VectorEntry,
    VectorError, VectorHeap, VectorId, VectorIndexBackend, VectorMatch, VectorRecord,
    VectorResult, VectorStore, VectorBackendState,
};

// Re-export extension traits for convenience
pub use extensions::*;

#[cfg(test)]
mod tests {
    #[test]
    fn test_crate_compiles() {
        // Basic smoke test to ensure crate compiles
    }
}
