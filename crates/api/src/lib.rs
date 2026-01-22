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
//! - `facade`: Redis-like convenience API (coming soon)
//!
//! ## Quick Start
//!
//! ```ignore
//! use strata_api::substrate::{KVStore, ApiRunId};
//!
//! // Get from default run
//! let value = store.kv_get(&ApiRunId::default(), "my-key")?;
//!
//! // Put with version tracking
//! let version = store.kv_put(&ApiRunId::default(), "my-key", Value::Int(42))?;
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod substrate;

// Re-export substrate types at crate root for convenience
pub use substrate::{
    // Core types
    ApiRunId, InvalidRunIdError, RetentionPolicy, RunInfo, RunState,
    DEFAULT_RUN_ID, DEFAULT_RUN_NAME,
    // Primitive traits
    KVStore, KVStoreBatch, JsonStore, EventLog, StateCell, VectorStore, TraceStore, RunIndex,
    // Transaction control
    TransactionControl, TransactionSavepoint, TxnId, TxnInfo, TxnOptions, TxnStatus,
    // Vector types
    DistanceMetric, SearchFilter, VectorMatch,
    // Trace types
    TraceEntry, TraceType,
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
