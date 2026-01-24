//! Public API layer for Strata
//!
//! This crate provides the public interface to the database:
//! - **Substrate API**: Power-user surface with explicit runs, versions, and transactions
//! - **Facade API**: Redis-like surface for common patterns (builds on Substrate)
//!
//! ## Two-Layer API Model
//!
//! ### Facade API (Default Mode)
//!
//! The facade hides Strata's complexity behind Redis-familiar patterns:
//! - Implicit default run targeting
//! - Auto-commit for each operation
//! - Simple `get(key) -> Option<Value>` returns
//!
//! ### Substrate API (Advanced Mode)
//!
//! The substrate exposes everything explicitly:
//! - Explicit `run_id` on every operation
//! - `Versioned<T>` returns on all reads
//! - Transaction control (`begin`, `commit`, `rollback`)
//! - Full history access
//!
//! ## Architectural Invariant
//!
//! Every facade call **desugars to exactly one substrate call pattern**.
//! No magic, no hidden semantics.
//!
//! ## Module Structure
//!
//! - `substrate`: Power-user API with explicit parameters
//! - `facade`: Redis-like convenience API
//!
//! ## Quick Start
//!
//! ### Using Facade (Simple)
//!
//! ```ignore
//! use strata_api::facade::KVFacade;
//!
//! // Simple get/set
//! facade.set("counter", Value::Int(0))?;
//! let count = facade.incr("counter")?;
//! ```
//!
//! ### Using Substrate (Advanced)
//!
//! ```ignore
//! use strata_api::substrate::{KVStore, ApiRunId};
//!
//! // Explicit run and versioning
//! let run = ApiRunId::new();
//! let version = store.kv_put(&run, "key", Value::Int(42))?;
//! let versioned = store.kv_get(&run, "key")?;
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod substrate;
pub mod facade;
pub mod desugar;

// Re-export substrate types at crate root for convenience
pub use substrate::{
    // Core types
    ApiRunId, InvalidRunIdError, RetentionPolicy, RunInfo, RunState,
    DEFAULT_RUN_ID, DEFAULT_RUN_NAME, DEFAULT_RUN_UUID_BYTES,
    // Implementation
    SubstrateImpl,
    // Primitive traits
    KVStore, KVStoreBatch, JsonStore, EventLog, StateCell, VectorStore, RunIndex,
    // Transaction control
    TransactionControl, TransactionSavepoint, TxnId, TxnInfo, TxnOptions, TxnStatus,
    // Vector types
    DistanceMetric, SearchFilter, VectorData, VectorMatch,
    // Retention types
    RetentionSubstrate, RetentionVersion, RetentionStats,
};

// Re-export facade types at crate root for convenience
pub use facade::{
    // Configuration
    FacadeConfig, GetOptions, SetOptions,
    // Facade traits
    KVFacade, KVFacadeBatch, JsonFacade, EventFacade, StateFacade, VectorFacade,
    HistoryFacade, RunFacade, ScopedFacade, SystemFacade, Capabilities, CapabilityLimits,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_run_id_default() {
        let id = ApiRunId::default();
        assert!(id.is_default());
        assert_eq!(id.as_str(), DEFAULT_RUN_NAME);
    }

    #[test]
    fn test_default_run_id_constant() {
        assert_eq!(DEFAULT_RUN_ID, "default");
        assert_eq!(DEFAULT_RUN_NAME, "default");
    }
}
