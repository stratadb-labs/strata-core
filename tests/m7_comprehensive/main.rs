//! M7 Comprehensive Test Suite
//!
//! Tests for Durability, Snapshots, Replay & Storage Stabilization.
//!
//! ## Test Tier Structure
//!
//! - **Tier 1: Recovery Invariants (R1-R6)** (sacred, must never break)
//! - **Tier 2: Replay Invariants (P1-P6)** (pure function, side-effect free)
//! - **Tier 3: Snapshot System** (format, CRC, atomic write, discovery)
//! - **Tier 4: WAL System** (entry format, CRC validation, transaction framing)
//! - **Tier 5: Crash Scenarios** (partial writes, recovery sequences)
//! - **Tier 6: Cross-Primitive Atomicity** (all-or-nothing commits)
//! - **Tier 7: Run Lifecycle** (begin_run, end_run, orphan detection)
//! - **Tier 8: Storage Stabilization** (PrimitiveStorageExt, registry)
//! - **Tier 9: Property-Based/Fuzzing** (random corruption, crash scenarios)
//! - **Tier 10: Stress & Scale** (large WAL, concurrent operations)
//! - **Tier 11: Non-Regression** (M6 targets maintained)
//! - **Tier 12: Spec Conformance** (direct spec-to-test mapping)
//!
//! ## Running Tests
//!
//! ```bash
//! # Run all M7 comprehensive tests
//! cargo test --test m7_comprehensive
//!
//! # Run specific tier
//! cargo test --test m7_comprehensive tier1
//!
//! # Run stress tests (slow, opt-in)
//! cargo test --test m7_comprehensive stress -- --ignored
//! ```

mod test_utils;

// Tier 1: Recovery Invariants (HIGHEST PRIORITY)
mod tier1_recovery_determinism;
mod tier1_recovery_idempotent;
mod tier1_recovery_may_drop_uncommitted;
mod tier1_recovery_no_drop_committed;
mod tier1_recovery_no_invent;
mod tier1_recovery_prefix;

// Tier 2: Replay Invariants
mod tier2_replay_derived_view;
mod tier2_replay_determinism;
mod tier2_replay_ephemeral;
mod tier2_replay_idempotent;
mod tier2_replay_pure_function;
mod tier2_replay_side_effect;

// Tier 3: Snapshot System
mod tier3_snapshot_atomic_write;
mod tier3_snapshot_crc;
mod tier3_snapshot_discovery;
mod tier3_snapshot_format;

// Tier 4: WAL System
mod tier4_wal_crc_validation;
mod tier4_wal_entry_format;
mod tier4_wal_transaction_framing;

// Tier 5: Crash Scenarios
mod tier5_crash_scenarios;

// Tier 6: Cross-Primitive Atomicity
mod tier6_cross_primitive_atomicity;

// Tier 7: Run Lifecycle
mod tier7_run_lifecycle;

// Tier 8: Storage Stabilization
mod tier8_storage_stabilization;

// Tier 9: Stress Tests (use #[ignore])
mod tier9_stress;
