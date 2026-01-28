//! Executor Layer Tests
//!
//! Tests for the strata-executor crate which provides:
//! - Command enum (106 variants) - the instruction set
//! - Output enum - typed results
//! - Executor - stateless command dispatch
//! - Session - stateful transaction support
//! - Strata - high-level typed wrapper API

mod common;

mod command_dispatch;
mod session_transactions;
mod strata_api;
mod serialization;
mod error_handling;
mod adversarial;
mod run_invariants;
