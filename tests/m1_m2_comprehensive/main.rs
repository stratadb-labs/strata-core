//! M1+M2 Comprehensive Test Suite
//!
//! This test suite validates the in-mem database across M1 (Storage) and
//! M2 (Transaction) functionality. Tests are organized by tier and invariant.
//!
//! **See INVARIANTS.md for the formal invariant definitions.**
//!
//! ## Test Tiers
//!
//! ### Tier 1: Core Invariants (sacred, fast, must pass)
//! Run on every commit. Enforce fundamental correctness.
//!
//! - `wal_invariant_tests.rs` - M1.1-M1.8 (WAL semantics)
//! - `snapshot_invariant_tests.rs` - M2.2-M2.5 (Snapshot Isolation)
//! - `acid_property_tests.rs` - M2.1, M2.6 (Atomicity)
//!
//! ### Tier 2: Behavioral Scenarios (medium, workflow tests)
//! Run on every commit. Test complete workflows.
//!
//! - `database_api_tests.rs` - API correctness
//! - `transaction_context_tests.rs` - Transaction API
//! - `transaction_workflow_tests.rs` - End-to-end flows
//! - `recovery_tests.rs` - Crash/recovery workflows
//! - `edge_case_tests.rs` - Boundary conditions
//! - `error_handling_tests.rs` - Error paths
//!
//! ### Tier 3: Stress/Chaos (opt-in, slow)
//! NOT run on every commit. Find rare bugs.
//!
//! - `concurrent_stress_tests.rs` - Race conditions, contention
//!
//! ## Running Tests
//!
//! ```bash
//! # Run Tier 1 + Tier 2 (default, every commit)
//! cargo test --test m1_m2_comprehensive
//!
//! # Run only core invariants (Tier 1)
//! cargo test --test m1_m2_comprehensive invariant
//!
//! # Run stress tests (Tier 3, opt-in)
//! cargo test --test m1_m2_comprehensive stress -- --ignored
//!
//! # Run specific invariant category
//! cargo test --test m1_m2_comprehensive wal_invariant
//! cargo test --test m1_m2_comprehensive snapshot_invariant
//! cargo test --test m1_m2_comprehensive acid
//! ```
//!
//! ## Invariant Mapping
//!
//! Each test maps to exactly one invariant from INVARIANTS.md.
//! Tests that don't map to an invariant are noise.

// =============================================================================
// Tier 1: Core Invariants (sacred, fast, must pass)
// =============================================================================

mod wal_invariant_tests;      // M1.1-M1.8: WAL semantics
mod snapshot_invariant_tests; // M2.2-M2.13: Snapshot Isolation
mod acid_property_tests;      // M2.1, M2.6, M2.14-M2.16: ACID properties

// =============================================================================
// Tier 2: Behavioral Scenarios (medium, workflow tests)
// =============================================================================

mod database_api_tests;
mod transaction_context_tests;
mod transaction_workflow_tests;
mod edge_case_tests;
mod error_handling_tests;
mod recovery_tests;

// =============================================================================
// Tier 3: Stress/Chaos (opt-in, slow) - use #[ignore] on individual tests
// =============================================================================

mod concurrent_stress_tests;

// =============================================================================
// Common test utilities
// =============================================================================

pub mod test_utils;
