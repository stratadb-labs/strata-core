//! Primitives layer for Strata
//!
//! Provides high-level primitives as stateless facades over the Database engine:
//! - **KVStore**: General-purpose key-value storage
//! - **EventLog**: Immutable append-only event stream with causal hash chaining
//! - **StateCell**: CAS-based versioned cells for coordination
//! - **BranchIndex**: Branch lifecycle management
//! - **JsonStore**: JSON document storage with path-based operations
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
//! ## Branch Isolation
//!
//! Every operation is scoped to a `BranchId`. Different runs cannot see
//! each other's data. This is enforced through key prefix isolation.
//!
//! ## Cross-Primitive Transactions
//!
//! Primitives can be combined within a single transaction using extension traits:
//!
//! ```rust,ignore
//! use strata_engine::primitives::extensions::*;
//!
//! db.transaction(branch_id, |txn| {
//!     txn.kv_put("key", value)?;
//!     txn.event_append("type", payload)?;
//!     txn.state_cas("cell", version, new_value)?;
//!     Ok(())
//! })?;
//! ```

pub mod event;
pub mod extensions;
pub mod json;
pub mod kv;
pub mod branch;
pub mod state;
pub mod vector;

// Re-exports - primitives are exported as they're implemented
pub use event::{Event, EventLog};
pub use json::{JsonDoc, JsonStore};
pub use kv::KVStore;
pub use branch::{BranchHandle, EventHandle, JsonHandle, KvHandle, StateHandle, VectorHandle};
pub use branch::{BranchIndex, BranchMetadata, BranchStatus};
pub use state::{State, StateCell};
pub use vector::{
    register_vector_recovery, validate_collection_name, validate_vector_key, BruteForceBackend,
    CollectionId, CollectionInfo, CollectionRecord, DistanceMetric, IndexBackendFactory,
    JsonScalar, MetadataFilter, StorageDtype, VectorConfig, VectorConfigSerde, VectorEntry,
    VectorError, VectorHeap, VectorId, VectorIndexBackend, VectorMatch, VectorRecord,
    VectorResult, VectorStore, VectorBackendState,
};

// Re-export search types for convenience (from search module)
pub use crate::search::{
    build_search_response, build_search_response_with_index,
    BM25LiteScorer, Scorer, ScorerContext, SearchCandidate, SearchDoc, Searchable, SimpleScorer,
    InvertedIndex, PostingEntry, PostingList, tokenize, tokenize_unique,
};

// Re-export extension traits for convenience
pub use extensions::*;
