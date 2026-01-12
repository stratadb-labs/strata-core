//! Database engine for in-mem
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

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod coordinator;
pub mod database;
// pub mod run;          // Story #29

pub use coordinator::{TransactionCoordinator, TransactionMetrics};
pub use database::{Database, RetryConfig};
