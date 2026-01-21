//! Database engine for Strata
//!
//! This crate orchestrates all lower layers:
//! - Database: Main database struct with open/close
//! - Run lifecycle: begin_run, end_run, fork_run (Epic 5)
//! - Transaction coordination (M2)
//! - Recovery integration
//! - Background tasks (snapshots, TTL cleanup)
//!
//! The engine is the only component that knows about:
//! - Run management
//! - Cross-layer coordination (storage + WAL + recovery)
//! - Replay logic
//!
//! # M4 Performance Instrumentation
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
pub mod durability;
pub mod instrumentation;
pub mod recovery_participant;
pub mod replay; // Story #311-316: Run Lifecycle & Replay
pub mod transaction;
pub mod transaction_ops; // Story #473: TransactionOps Trait Definition

pub use coordinator::{TransactionCoordinator, TransactionMetrics};
pub use database::{Database, DatabaseBuilder, RetryConfig};
pub use recovery_participant::{
    recover_all_participants, register_recovery_participant, RecoveryFn, RecoveryParticipant,
};
pub use durability::{
    BufferedDurability, CommitData, Durability, DurabilityMode, InMemoryDurability,
    StrictDurability,
};
pub use instrumentation::PerfTrace;
pub use replay::{
    diff_views, DiffEntry, DiffPrimitiveKind, ReadOnlyView, ReplayError, RunDiff, RunError,
    RunIndex,
};
pub use transaction::{Transaction, TransactionPool, MAX_POOL_SIZE};
pub use transaction_ops::TransactionOps;

#[cfg(feature = "perf-trace")]
pub use instrumentation::{PerfBreakdown, PerfStats};
