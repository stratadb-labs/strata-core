//! M5 Comprehensive Test Suite
//!
//! Tests for the JSON Primitive semantic guarantees.
//!
//! ## Test Tier Structure
//!
//! - **Tier 1: Semantic Invariants** (sacred, must never break)
//!   Tests that lock in M5 semantic contract.
//!
//! - **Tier 2: Property-Based/Fuzzing** (catch edge cases)
//!   Random path/patch/txn interleavings.
//!
//! - **Tier 3: WAL/Recovery** (durability guarantees)
//!   WAL replay torture tests and crash simulation.
//!
//! - **Tier 4: Cross-Primitive** (atomicity with KV, Event, etc.)
//!   JSON + KV + Event atomic transactions.
//!
//! - **Tier 5: Serializability** (correct final states)
//!   Final state explainable by serial ordering.
//!
//! - **Tier 6: Mutation Algebra** (patch composition rules)
//!   Algebraic properties of patches.
//!
//! - **Tier 7: Stress/Scale** (correctness under load)
//!   Deep nesting, large docs, many concurrent txns.
//!
//! - **Tier 8: Backward Compat** (future-proofing)
//!   Tests frozen after M5 ships.
//!
//! - **Tier 9: Spec Conformance** (spec → test)
//!   Direct spec-to-test mapping.
//!
//! ## Running Tests
//!
//! ```bash
//! # Run all M5 comprehensive tests
//! cargo test --test m5_comprehensive
//!
//! # Run only semantic invariants (fastest)
//! cargo test --test m5_comprehensive invariant
//!
//! # Run property-based tests
//! cargo test --test m5_comprehensive fuzz
//!
//! # Run stress tests (slow, opt-in)
//! cargo test --test m5_comprehensive stress -- --ignored
//! ```

// Test utilities
mod test_utils;

// Tier 1: Semantic Invariants
mod conflict_matrix_tests;
mod patch_semantics_invariants;
mod path_semantics_invariants;
mod snapshot_semantics_invariants;

// Tier 2: Fuzzing (requires proptest feature)
#[cfg(feature = "proptest")]
mod conflict_detection_fuzzing;

// Tier 3: WAL & Recovery
mod crash_recovery_tests;
mod wal_replay_tests;

// Tier 4: Cross-Primitive
mod cross_primitive_atomicity;
mod cross_primitive_rollback;

// Tier 5: Serializability
mod serializability_tests;

// Tier 6: Mutation Algebra
mod mutation_algebra_tests;

// Tier 7: Stress (use #[ignore])
mod stress_tests;

// Tier 8: Backward Compatibility
mod backward_compatibility;

// Tier 9: Spec Conformance
mod spec_conformance_tests;
