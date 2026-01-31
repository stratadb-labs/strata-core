//! Database engine for Strata
//!
//! This crate orchestrates all lower layers:
//! - Database: Main database struct with open/close
//! - Branch lifecycle: begin_run, end_run, fork_run (Epic 5)
//! - Transaction coordination
//! - Recovery integration
//! - Background tasks (snapshots, TTL cleanup)
//!
//! The engine is the only component that knows about:
//! - Branch management
//! - Cross-layer coordination (storage + WAL + recovery)
//! - Replay logic
//!
//! # Performance Instrumentation
//!
//! Enable the `perf-trace` feature for per-operation timing:
//!
//! ```bash
//! cargo build --features perf-trace
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod coordinator;
pub mod database;
pub mod instrumentation;
pub mod recovery;
pub mod transaction;
pub mod transaction_ops; // TransactionOps Trait Definition

pub use coordinator::{TransactionCoordinator, TransactionMetrics};
pub use database::{Database, DatabaseBuilder, RetryConfig};
pub use recovery::{
    recover_all_participants, register_recovery_participant, RecoveryFn, RecoveryParticipant,
    diff_views, DiffEntry, ReadOnlyView, ReplayError, BranchDiff, BranchError,
    ReplayBranchIndex,
};
pub use strata_durability::wal::DurabilityMode;
pub use instrumentation::PerfTrace;
// Note: Use strata_core::PrimitiveType for DiffEntry.primitive field
pub use transaction::{Transaction, TransactionPool, MAX_POOL_SIZE};
pub use strata_concurrency::TransactionContext;
pub use transaction_ops::TransactionOps;

pub mod bundle;
pub mod primitives;
pub mod search;

// Re-export search types at crate root for convenience
pub use search::{SearchBudget, SearchHit, SearchMode, SearchRequest, SearchResponse, SearchStats};

// Re-export submodules for `strata_engine::vector::*` and `strata_engine::extensions::*` access
pub use primitives::vector;
pub use primitives::extensions;

// Re-export primitive types at crate root for convenience
pub use primitives::{
    // Primitives
    KVStore, EventLog, Event,
    StateCell, State, JsonStore, JsonDoc, VectorStore,
    BranchIndex, BranchMetadata, BranchStatus,
    // Handles
    BranchHandle, EventHandle, JsonHandle, KvHandle, StateHandle, VectorHandle,
    // Search & Scoring
    Searchable, SearchCandidate, SimpleScorer,
    BM25LiteScorer, Scorer, ScorerContext, SearchDoc,
    build_search_response_with_index,
    // Index
    InvertedIndex, PostingEntry, PostingList,
    // Vector types
    VectorConfig, VectorEntry, VectorMatch, DistanceMetric,
    CollectionId, CollectionInfo, VectorIndexBackend, BruteForceBackend, VectorError,
    VectorId, VectorRecord, VectorResult, VectorConfigSerde, VectorHeap,
    CollectionRecord, IndexBackendFactory, JsonScalar, MetadataFilter, StorageDtype,
    VectorBackendState,
    validate_collection_name, validate_vector_key, build_search_response,
    // Extension traits
    KVStoreExt, EventLogExt, StateCellExt, JsonStoreExt, VectorStoreExt,
    // Recovery
    register_vector_recovery,
};

// Re-export bundle types at crate root
pub use bundle::{ExportInfo, ImportInfo, BundleInfo};

#[cfg(feature = "perf-trace")]
pub use instrumentation::{PerfBreakdown, PerfStats};
