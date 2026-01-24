//! Substrate API - Power-user surface
//!
//! The Substrate API is the canonical semantic contract for Strata. It exposes:
//! - All primitives explicitly (KVStore, JsonStore, EventLog, StateCell, VectorStore)
//! - All versioning (`Versioned<T>` returns on all reads, Version on all writes)
//! - All run scoping (explicit run_id on every operation)
//! - All transactional semantics (begin/commit/rollback)
//!
//! ## Design Philosophy
//!
//! The Substrate API must:
//! - Be deterministic and replayable
//! - Be minimal, not friendly
//! - Be unambiguous and stable
//!
//! ## Module Structure
//!
//! - `types`: Core types (RunId, RunInfo, RunState, RetentionPolicy)
//! - `kv`: KVStore operations
//! - `json`: JsonStore operations
//! - `event`: EventLog operations
//! - `state`: StateCell operations
//! - `vector`: VectorStore operations
//! - `run`: RunIndex operations
//! - `transaction`: Transaction control
//! - `retention`: Retention policy operations
//!
//! ## Usage
//!
//! ```
//! use strata_api::substrate::{
//!     ApiRunId, RetentionPolicy, RunInfo, RunState,
//!     KVStore, JsonStore, EventLog, StateCell, VectorStore,
//!     RunIndex, TransactionControl, RetentionSubstrate,
//! };
//! ```

pub mod types;
pub mod kv;
pub mod json;
pub mod event;
pub mod state;
pub mod vector;
pub mod run;
pub mod transaction;
pub mod retention;
mod impl_;

// Re-export core types
pub use types::{
    ApiRunId, InvalidRunIdError, RetentionPolicy, RunInfo, RunState,
    DEFAULT_RUN_ID, DEFAULT_RUN_NAME, DEFAULT_RUN_UUID_BYTES,
};

// Re-export implementation
pub use impl_::SubstrateImpl;

// Re-export primitive traits
pub use kv::{KVStore, KVStoreBatch};
pub use json::JsonStore;
pub use event::EventLog;
pub use state::StateCell;
pub use vector::{DistanceMetric, SearchFilter, VectorData, VectorMatch, VectorStore};
pub use run::RunIndex;
pub use transaction::{TransactionControl, TransactionSavepoint, TxnId, TxnInfo, TxnOptions, TxnStatus};
pub use retention::{RetentionSubstrate, RetentionVersion, RetentionStats};
