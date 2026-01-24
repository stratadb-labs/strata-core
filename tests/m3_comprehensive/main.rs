//! M3 Comprehensive Test Suite
//!
//! Tests for the Primitives layer (KVStore, EventLog, StateCell, RunIndex)
//! and their integration patterns.
//!
//! ## Test Tier Structure
//!
//! - **Tier 1: Core Invariants** (sacred, fast, must pass)
//!   Tests M3.1-M3.24 invariants that define primitive correctness.
//!
//! - **Tier 2: Behavioral Scenarios** (medium, workflow tests)
//!   Tests complete primitive workflows and API coverage.
//!
//! - **Tier 3: Stress/Chaos** (opt-in with #[ignore], slow)
//!   Finds rare bugs under high contention.
//!
//! ## Layer Separation Principle
//!
//! CRITICAL: M3 tests must NOT re-test M1/M2 invariants.
//! - M1 (WAL, storage ordering): Assume correct
//! - M2 (Snapshot isolation, OCC): Assume correct
//! - M3 (Primitive semantics): Test here
//!
//! ## Non-Goals
//!
//! This test suite does NOT test:
//! - WAL correctness (M1)
//! - Snapshot isolation semantics (M2)
//! - OCC conflict detection (M2)
//! - Real-time timestamp ordering (not guaranteed)
//!
//! ## Running Tests
//!
//! ```bash
//! # Run Tier 1 + Tier 2 (every commit)
//! cargo test --test m3_comprehensive
//!
//! # Run stress tests (opt-in)
//! cargo test --test m3_comprehensive stress -- --ignored
//! ```

// Test utilities
mod test_utils;

// Tier 1: Core Invariants
mod eventlog_chain_tests;
mod primitive_invariant_tests;
mod runindex_lifecycle_tests;
mod statecell_cas_tests;
mod substrate_invariant_tests;

// Tier 2: Behavioral Scenarios
mod cross_primitive_transaction_tests;
mod edge_case_tests;
mod primitive_api_tests;
mod recovery_comprehensive_tests;
mod run_isolation_comprehensive_tests;

// Tier 3: Stress/Chaos (use #[ignore])
mod concurrent_primitive_stress_tests;
