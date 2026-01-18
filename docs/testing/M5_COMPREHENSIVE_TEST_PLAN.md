# M5 Comprehensive Test Plan

**Version**: 1.0
**Status**: Planning
**Date**: 2026-01-16

---

## Overview

This document defines the comprehensive test suite for M5 JSON Primitive, **separate from the unit and integration tests written during development**.

The goal is to create a battery of tests that:
1. **Lock in semantic invariants** - Prevent accidental breakage in future refactors
2. **Validate spec compliance** - Every spec statement becomes executable
3. **Detect edge cases through fuzzing** - Property-based testing for conflicts
4. **Ensure durability guarantees** - WAL replay and crash recovery torture tests
5. **Verify cross-primitive atomicity** - JSON integrates with the unified transaction system
6. **Prevent regressions** - M4 performance and semantics are maintained

---

## Test Structure

```
tests/
└── m5_comprehensive/
    ├── main.rs                           # Test harness and utilities
    │
    │   # Tier 1: Semantic Invariants (MOST IMPORTANT)
    ├── path_semantics_invariants.rs      # 1.1 Path positional semantics
    ├── patch_semantics_invariants.rs     # 1.2 Patch ordering and conflicts
    ├── snapshot_semantics_invariants.rs  # 1.3 Weak snapshot isolation
    ├── conflict_matrix_tests.rs          # Spec conflict matrix as tests
    │
    │   # Tier 2: Property-Based/Fuzzing
    ├── conflict_detection_fuzzing.rs     # 2. Random path/patch/txn interleavings
    │
    │   # Tier 3: Durability & Recovery
    ├── wal_replay_torture_tests.rs       # 3. WAL torture tests
    ├── crash_recovery_tests.rs           # 3. Crash simulation
    │
    │   # Tier 4: Cross-Primitive
    ├── cross_primitive_atomicity.rs      # 4. JSON + KV + Event atomicity
    ├── cross_primitive_rollback.rs       # 4. Conflict rollback across primitives
    │
    │   # Tier 5: Serializability
    ├── serializability_tests.rs          # 5. Final state explainable by serial ordering
    │
    │   # Tier 6: Mutation Algebra
    ├── mutation_algebra_tests.rs         # 6. Algebraic properties of patches
    │
    │   # Tier 7: Stress & Scale
    ├── stress_tests.rs                   # 7. Deep nesting, large docs, many txns
    │
    │   # Tier 8: Backward Compatibility
    ├── backward_compatibility.rs         # 8. Future-proofing regression tests
    │
    │   # Tier 9: Spec Conformance
    └── spec_conformance_tests.rs         # 9. Direct spec-to-test mapping
```

---

## Tier 1: Semantic Invariants (HIGHEST PRIORITY)

These tests ensure you **never accidentally violate the M5 contract** in future refactors.
They are written as **black-box tests against the public API**.

### 1.1 Path Semantics Invariants (`path_semantics_invariants.rs`)

**Invariant**: Paths are positional, not identity-based. Path meaning changes when structure changes.

```rust
// Test cases - These must never change

#[test]
fn test_paths_are_positional_not_identity() {
    // Given: Array [A, B, C] at $.items
    // When: Insert X at index 0 → [X, A, B, C]
    // Then: $.items[0] now refers to X, not A
    // This is fundamental to M5 semantics
}

#[test]
fn test_read_path_invalidated_by_ancestor_mutation() {
    // Given: txn A reads $.a.b
    // When: txn B sets $.a (replaces entire subtree)
    // Then: txn A commit must fail (conflict)
}

#[test]
fn test_read_at_path_conflict_with_set_at_same_path() {
    // Given: txn A reads $.a.b
    // When: txn B sets $.a.b
    // Then: txn A commit must fail (same path conflict)
}

#[test]
fn test_read_at_path_conflict_with_set_at_ancestor() {
    // Given: txn A reads $.a.b.c
    // When: txn B sets $.a
    // Then: txn A commit must fail (ancestor overwrite)
}

#[test]
fn test_read_at_path_conflict_with_set_at_descendant() {
    // Given: txn A reads $.a
    // When: txn B sets $.a.b.c
    // Then: txn A commit must fail (descendant modified)
}

#[test]
fn test_sibling_paths_do_not_conflict() {
    // Given: txn A writes $.a.b
    // When: txn B writes $.a.c
    // Then: Both can commit (siblings, no overlap)
}

#[test]
fn test_array_read_invalidated_by_array_mutation() {
    // Given: txn A reads $.items[0]
    // When: txn B replaces $.items (rewrite array)
    // Then: txn A commit must fail (array changed)
}

#[test]
fn test_root_path_conflicts_with_everything() {
    // Given: txn A writes $ (root)
    // When: txn B writes $.anything
    // Then: Conflict (root overlaps all paths)
}
```

### 1.2 Patch Semantics Invariants (`patch_semantics_invariants.rs`)

**Invariant**: Patches are ordered, non-commutative, and sequentially applied.

```rust
#[test]
fn test_patch_ordering_matters() {
    // [Set($.a, {}), Set($.a.b, 1)] → $.a = { "b": 1 }
    // [Set($.a.b, 1), Set($.a, {})] → $.a = {}
    // Order determines final state
}

#[test]
fn test_patches_are_not_commutative() {
    // Apply [A, B] vs [B, A]
    // Results must differ (patches are programs, not sets)
}

#[test]
fn test_overlapping_patches_in_same_txn_fail() {
    // Given: txn applies [Set($.a, X), Set($.a.b, Y)]
    // Then: Transaction must fail ($.a and $.a.b overlap)
}

#[test]
fn test_disjoint_patches_succeed() {
    // Given: txn applies [Set($.a, X), Set($.b, Y)]
    // Then: Transaction succeeds (disjoint paths)
}

#[test]
fn test_delete_then_set_on_same_subtree_fails() {
    // Given: txn applies [Delete($.a), Set($.a.b, X)]
    // Then: Transaction must fail (overlap)
}

#[test]
fn test_set_then_delete_same_path_fails() {
    // Given: txn applies [Set($.a.b, X), Delete($.a.b)]
    // Then: Transaction must fail (same path)
}

#[test]
fn test_patches_applied_sequentially() {
    // Given: Initial doc { "x": 1 }
    // Apply: [Set($.y, 2), Set($.z, $.y)] where $.z references $.y
    // Then: Each patch sees effects of prior patches
}
```

### 1.3 Weak Snapshot Semantics (`snapshot_semantics_invariants.rs`)

**Invariant**: M5 provides weak snapshot isolation. Modified documents fail reads.

```rust
#[test]
fn test_weak_snapshot_modified_doc_fails_read() {
    // Given: txn A starts, reads doc D at version V
    // When: txn B modifies doc D, commits
    // Then: txn A subsequent read of D must fail
    //       (DocumentModifiedAfterSnapshot)
}

#[test]
fn test_weak_snapshot_unmodified_doc_succeeds() {
    // Given: txn A starts, reads doc D at version V
    // When: txn B modifies doc E (different doc)
    // Then: txn A can still read doc D successfully
}

#[test]
fn test_read_your_writes_in_same_txn() {
    // Given: txn A writes $.x = 1
    // When: txn A reads $.x (before commit)
    // Then: txn A sees its own write (value = 1)
}

#[test]
fn test_read_your_writes_overlapping_path() {
    // Given: txn A writes $.a = { "b": 1, "c": 2 }
    // When: txn A reads $.a.b
    // Then: txn A sees 1 from its own write
}

#[test]
fn test_snapshot_version_check_at_commit() {
    // Given: txn A reads doc D at version V
    // When: txn B modifies doc D (version becomes V+1)
    // Then: txn A commit fails with ConflictError
}

#[test]
fn test_no_stale_reads() {
    // M5 never returns stale data
    // Either read succeeds with current data, or fails explicitly
}
```

### 1.4 Conflict Matrix Tests (`conflict_matrix_tests.rs`)

**Every row of the spec conflict matrix becomes a test.**

```rust
// From M5_ARCHITECTURE.md Section 8.2

#[test]
fn test_conflict_matrix_siblings_no_conflict() {
    // $.a.b vs $.a.c → NO conflict (siblings)
}

#[test]
fn test_conflict_matrix_same_path() {
    // $.a.b vs $.a.b → YES conflict (same)
}

#[test]
fn test_conflict_matrix_ancestor_descendant() {
    // $.a vs $.a.b → YES conflict (ancestor)
}

#[test]
fn test_conflict_matrix_descendant_ancestor() {
    // $.a.b vs $.a → YES conflict (descendant)
}

#[test]
fn test_conflict_matrix_different_subtrees() {
    // $.x vs $.y → NO conflict (different subtrees)
}

#[test]
fn test_conflict_matrix_root_vs_any() {
    // $ vs $.anything → YES conflict (root)
}

// Array conflict tests (Section 8.3)
#[test]
fn test_array_insert_conflicts_with_element_access() {
    // insert($.items, 0, X) vs set($.items[1].price, 10) → YES conflict
}

#[test]
fn test_array_remove_conflicts_with_element_read() {
    // remove($.items, 0) vs get($.items[0]) → YES conflict
}

#[test]
fn test_array_mutation_different_subtree_no_conflict() {
    // insert($.items, 0, X) vs set($.other, Y) → NO conflict
}

#[test]
fn test_array_push_conflicts_with_element_access() {
    // push($.items, X) vs set($.items[0], Y) → YES conflict
}
```

---

## Tier 2: Conflict Detection Fuzzing (`conflict_detection_fuzzing.rs`)

**HIGHEST VALUE**: Property-based testing catches 90% of bugs humans miss.

### Strategy

Randomly generate:
1. JSON trees (nested objects/arrays)
2. Random paths into those trees
3. Random patch sequences
4. Random transaction interleavings

Assert invariants:
- If two paths overlap → at most one txn commits
- If two paths are disjoint → both txns can commit
- Final state is equivalent to some serial ordering
- No silent overwrites
- No lost updates

```rust
use proptest::prelude::*;

#[derive(Debug, Clone)]
struct RandomJsonTree {
    value: JsonValue,
    all_paths: Vec<JsonPath>,  // All valid paths into this tree
}

fn arbitrary_json_tree(depth: usize) -> impl Strategy<Value = RandomJsonTree> {
    // Generate random JSON with tracked paths
}

fn arbitrary_path_pair(tree: &RandomJsonTree) -> impl Strategy<Value = (JsonPath, JsonPath)> {
    // Pick two random paths from the tree
}

proptest! {
    #[test]
    fn fuzz_overlapping_paths_conflict(
        tree in arbitrary_json_tree(5),
        (p1, p2) in arbitrary_path_pair(&tree),
    ) {
        // If p1.overlaps(p2):
        //   Create txn A writing p1, txn B writing p2
        //   At most one should commit
        // Else:
        //   Both should commit
    }

    #[test]
    fn fuzz_no_lost_updates(
        tree in arbitrary_json_tree(5),
        patches in prop::collection::vec(arbitrary_patch(), 1..10),
    ) {
        // Every committed patch must be reflected in final state
        // No silent overwrites
    }

    #[test]
    fn fuzz_serializable_final_state(
        initial_tree in arbitrary_json_tree(5),
        txn_count in 2..10usize,
    ) {
        // Run N transactions with random patches
        // Final state must be explainable by some serial ordering
    }
}
```

### Invariants to Test

| Invariant | Property |
|-----------|----------|
| **Overlap → Conflict** | If `p1.overlaps(p2)`, at most one txn commits |
| **Disjoint → Both Commit** | If `!p1.overlaps(p2)`, both txns commit |
| **No Lost Updates** | Every committed write visible in final state |
| **Serializability** | Final state = some serial ordering |
| **Determinism** | Same inputs → same outputs |

---

## Tier 3: WAL Replay and Recovery Torture Tests

### 3.1 WAL Replay Tests (`wal_replay_torture_tests.rs`)

```rust
#[test]
fn test_wal_replay_is_deterministic() {
    // Write patches → crash → replay → verify state
    // Repeat 100 times → state must be identical
}

#[test]
fn test_wal_replay_is_idempotent() {
    // Replay same WAL twice
    // State must be identical (safe to double-apply)
}

#[test]
fn test_wal_replay_order_matters() {
    // Apply patches in order A, B, C
    // Replay must apply in same order
    // Reordering produces different state (this is expected)
}

#[test]
fn test_wal_replay_never_partially_applies_patch() {
    // A patch is atomic
    // Either fully applied or not at all
}

#[test]
fn test_wal_replay_interleaved_primitives() {
    // JSON + KV + Event entries interleaved
    // All replay correctly in order
}

#[test]
fn test_wal_replay_after_truncated_entry() {
    // Write partial WAL entry (simulate crash mid-write)
    // Recovery must skip incomplete entry
    // State must be consistent (last complete txn)
}
```

### 3.2 Crash Recovery Tests (`crash_recovery_tests.rs`)

```rust
#[test]
fn test_crash_after_n_wal_entries() {
    for n in 1..100 {
        // Write N entries
        // Simulate crash
        // Recover
        // Verify state matches first N committed operations
    }
}

#[test]
fn test_crash_during_commit() {
    // Begin txn, write patches
    // Crash before commit marker
    // Recovery: txn should be rolled back
}

#[test]
fn test_crash_after_commit_before_sync() {
    // (For non-Strict modes)
    // Commit succeeds, crash before fsync
    // Behavior depends on durability mode
}

#[test]
fn test_recovery_preserves_document_versions() {
    // Create doc at v1, modify to v2, v3
    // Crash and recover
    // Document version must be v3
}

#[test]
fn test_recovery_with_multiple_docs() {
    // Create 100 docs with various modifications
    // Crash at random point
    // All committed state must be recovered
}
```

---

## Tier 4: Cross-Primitive Atomicity Tests

### 4.1 Atomicity Tests (`cross_primitive_atomicity.rs`)

```rust
#[test]
fn test_json_kv_atomic_commit() {
    // txn { json.set(...), kv.put(...) }
    // Both commit or both rollback
}

#[test]
fn test_json_kv_event_atomic_commit() {
    // txn { json.set(...), kv.put(...), event.append(...) }
    // All three commit or all rollback
}

#[test]
fn test_json_conflict_rolls_back_kv() {
    // txn A: { json.set(doc1, $.a, X), kv.put("k", V1) }
    // txn B: { json.set(doc1, $.a, Y) } // commits first, creates conflict
    // txn A commit fails
    // Verify: "k" does NOT have value V1
}

#[test]
fn test_kv_conflict_rolls_back_json() {
    // txn A: { kv.put("k", V1), json.set(doc1, $.a, X) }
    // txn B: { kv.put("k", V2) } // commits first
    // txn A commit fails
    // Verify: doc1.$.a does NOT have value X
}

#[test]
fn test_json_event_atomicity() {
    // txn { json.create(...), event.append("created", ...) }
    // Both visible or neither
}
```

### 4.2 Rollback Tests (`cross_primitive_rollback.rs`)

```rust
#[test]
fn test_rollback_clears_all_primitive_writes() {
    // txn writes to JSON, KV, Event
    // txn aborts (explicit or conflict)
    // None of the writes are visible
}

#[test]
fn test_partial_commit_impossible() {
    // There is no state where JSON committed but KV didn't
    // Atomic commit is all-or-nothing
}

#[test]
fn test_abort_mid_txn_leaves_no_trace() {
    // Begin txn
    // Write to multiple primitives
    // Abort explicitly
    // Verify clean state
}
```

---

## Tier 5: Serializability Tests (`serializability_tests.rs`)

```rust
#[test]
fn test_final_state_has_serial_explanation() {
    // Run N concurrent transactions
    // Final state must be explainable by SOME serial ordering
    // (Not necessarily the submission order)
}

#[test]
fn test_no_impossible_states() {
    // Define impossible states (e.g., A committed after B but sees pre-B state)
    // Generate random interleavings
    // Assert no impossible state reached
}

#[test]
fn test_conflict_produces_serializable_outcome() {
    // Two conflicting txns
    // Exactly one commits
    // Final state = that one txn's effects
}

proptest! {
    #[test]
    fn fuzz_serializable_history(
        ops in arbitrary_operation_sequence(10),
    ) {
        // Generate random read/write operations
        // Execute concurrently
        // Verify final state is serializable
    }
}
```

---

## Tier 6: Mutation Algebra Tests (`mutation_algebra_tests.rs`)

Lock down algebraic properties so future optimizations don't break meaning.

```rust
#[test]
fn test_set_then_delete_equals_delete() {
    // Set($.a, X) then Delete($.a) → result is deleted
}

#[test]
fn test_set_then_set_equals_last_set() {
    // Set($.a, X) then Set($.a, Y) → result is Y
}

#[test]
fn test_delete_then_delete_equals_delete() {
    // Delete($.a) then Delete($.a) → still deleted (idempotent)
}

#[test]
fn test_delete_then_set_equals_set() {
    // Delete($.a) then Set($.a, X) → result is X
}

#[test]
fn test_set_nested_preserves_siblings() {
    // Initial: { "a": { "b": 1, "c": 2 } }
    // Set($.a.b, 10)
    // Result: { "a": { "b": 10, "c": 2 } } (c preserved)
}

#[test]
fn test_delete_nested_preserves_siblings() {
    // Initial: { "a": { "b": 1, "c": 2 } }
    // Delete($.a.b)
    // Result: { "a": { "c": 2 } } (c preserved)
}

#[test]
fn test_set_root_replaces_everything() {
    // Initial: { "a": 1, "b": 2 }
    // Set($, { "x": 99 })
    // Result: { "x": 99 } (complete replacement)
}

#[test]
fn test_delete_root_empties_doc() {
    // Delete($) on non-empty doc
    // Doc becomes null or empty (depending on semantics)
}
```

---

## Tier 7: Stress and Scale Tests (`stress_tests.rs`)

Not for speed, but for **correctness under stress**.

```rust
#[test]
#[ignore] // Slow, opt-in
fn test_deep_nesting_at_limit() {
    // Create document with 100 levels of nesting (the limit)
    // Read and write at deepest path
    // No stack overflow, correct behavior
}

#[test]
#[ignore]
fn test_large_document_near_size_limit() {
    // Create 15MB document (near 16MB limit)
    // Operations succeed
    // 17MB document fails with DocumentTooLarge
}

#[test]
#[ignore]
fn test_large_array_at_limit() {
    // Array with 1M elements
    // Read/write individual elements
    // Replace entire array
}

#[test]
#[ignore]
fn test_many_concurrent_transactions() {
    // 100 concurrent transactions on same document
    // All either commit or properly conflict
    // No deadlocks, no hangs
}

#[test]
#[ignore]
fn test_many_documents_per_run() {
    // Create 10,000 documents in single run
    // Operations on each succeed
    // Memory usage reasonable
}

#[test]
#[ignore]
fn test_long_transaction_with_many_patches() {
    // Single txn with 1000 patches (disjoint paths)
    // Commits successfully
    // All patches applied
}
```

---

## Tier 8: Backward Compatibility Tests (`backward_compatibility.rs`)

Future-proofing: ensure M6+ changes don't break M5 semantics.

```rust
#[test]
fn test_storage_format_change_preserves_semantics() {
    // M5 uses blob storage
    // Future: structural storage
    // This test asserts: same inputs → same outputs
    // Run this test after M6 migration to catch drift
}

#[test]
fn test_path_semantics_unchanged() {
    // $.a.b means the same thing now as in M5
    // This is a regression test for path parsing
}

#[test]
fn test_conflict_detection_unchanged() {
    // Same path overlap rules in future versions
}

#[test]
fn test_version_semantics_unchanged() {
    // Document versions increment the same way
}

// These tests should be FROZEN after M5 ships
// Any failure means unintentional semantic change
```

---

## Tier 9: Spec Conformance Tests (`spec_conformance_tests.rs`)

**Convert M5_ARCHITECTURE.md directly into tests.**

```rust
// Section 4: Semantic Invariants

#[test]
fn test_spec_3_1_paths_are_positional() {
    // From Section 3.1: "Paths refer to positions, not identities"
}

#[test]
fn test_spec_3_2_mutations_are_path_based() {
    // From Section 3.2: "All JSON writes are defined as mutations to paths"
}

#[test]
fn test_spec_3_3_conflict_detection_is_region_based() {
    // From Section 3.3: "Two operations conflict if paths overlap"
}

#[test]
fn test_spec_3_4_wal_is_patch_based() {
    // From Section 3.4: "WAL entries describe mutations, not full documents"
}

#[test]
fn test_spec_3_5_cross_primitive_atomicity() {
    // From Section 3.5: "JSON obeys same atomicity as other primitives"
}

// Section 5: Document Model

#[test]
fn test_spec_4_4_max_document_size() {
    // 16MB limit
}

#[test]
fn test_spec_4_4_max_nesting_depth() {
    // 100 levels limit
}

#[test]
fn test_spec_4_4_max_path_length() {
    // 256 segments limit
}

#[test]
fn test_spec_4_4_max_array_size() {
    // 1M elements limit
}

// Section 8: Conflict Detection (already covered in conflict_matrix_tests.rs)

// Section 9: Versioning

#[test]
fn test_spec_8_1_document_granular_versioning() {
    // Single version per document, increments on ANY change
}

// Section 10: Snapshot Semantics

#[test]
fn test_spec_9_1_weak_snapshot_guarantee() {
    // Reads fail on concurrent modification
}

// Section 12: Patch Ordering

#[test]
fn test_spec_11_1_sequential_application() {
    // Patches applied in order
}

#[test]
fn test_spec_11_2_intra_txn_overlap_fails() {
    // Overlapping patches in same txn = invalid
}

#[test]
fn test_spec_11_3_ordering_matters() {
    // Patch order determines result
}
```

---

## Test Utilities (`main.rs`)

```rust
//! M5 Comprehensive Test Suite
//!
//! Tests for the JSON Primitive semantic guarantees.
//!
//! ## Test Tier Structure
//!
//! - **Tier 1: Semantic Invariants** (sacred, must never break)
//! - **Tier 2: Property-Based/Fuzzing** (catch edge cases)
//! - **Tier 3: WAL/Recovery** (durability guarantees)
//! - **Tier 4: Cross-Primitive** (atomicity with KV, Event, etc.)
//! - **Tier 5: Serializability** (correct final states)
//! - **Tier 6: Mutation Algebra** (patch composition rules)
//! - **Tier 7: Stress/Scale** (correctness under load)
//! - **Tier 8: Backward Compat** (future-proofing)
//! - **Tier 9: Spec Conformance** (spec → test)
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

// Utilities
mod test_utils;

// Tier 1: Semantic Invariants
mod path_semantics_invariants;
mod patch_semantics_invariants;
mod snapshot_semantics_invariants;
mod conflict_matrix_tests;

// Tier 2: Fuzzing
mod conflict_detection_fuzzing;

// Tier 3: WAL & Recovery
mod wal_replay_torture_tests;
mod crash_recovery_tests;

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
```

---

## Test Utilities (`test_utils.rs`)

```rust
use in_mem_core::json::{JsonDocId, JsonPath, JsonValue, JsonPatch};
use in_mem_core::types::RunId;
use in_mem_engine::Database;
use in_mem_primitives::JsonStore;
use std::sync::Arc;

/// Create a test database with InMemory durability
pub fn create_test_db() -> Arc<Database> {
    Arc::new(
        Database::builder()
            .durability(DurabilityMode::InMemory)
            .open_temp()
            .expect("Failed to create test database")
    )
}

/// Create a test database with specified durability
pub fn create_test_db_with_mode(mode: DurabilityMode) -> Arc<Database> {
    Arc::new(
        Database::builder()
            .durability(mode)
            .open_temp()
            .expect("Failed to create test database")
    )
}

/// Run test across all durability modes
pub fn test_across_modes<F, T>(test_name: &str, workload: F)
where
    F: Fn(Arc<Database>) -> T,
    T: PartialEq + std::fmt::Debug,
{
    // Same pattern as M4 regression tests
}

/// Helper to create JsonPath from string
pub fn path(s: &str) -> JsonPath {
    JsonPath::parse(s).expect("Invalid path")
}

/// Helper to create JSON value
pub fn json_val(v: serde_json::Value) -> JsonValue {
    JsonValue::from(v)
}

/// Helper to create a test document
pub fn create_test_doc(json: &JsonStore, run_id: &RunId, value: JsonValue) -> JsonDocId {
    json.create(run_id, value).expect("Failed to create doc")
}

/// Assert two transactions conflict
pub fn assert_conflict<F1, F2>(db: Arc<Database>, run_id: &RunId, txn_a: F1, txn_b: F2)
where
    F1: FnOnce(&mut TransactionContext) -> Result<(), Error>,
    F2: FnOnce(&mut TransactionContext) -> Result<(), Error>,
{
    // Start both transactions
    // Execute both
    // Exactly one should fail with conflict
}

/// Assert two transactions do not conflict
pub fn assert_no_conflict<F1, F2>(db: Arc<Database>, run_id: &RunId, txn_a: F1, txn_b: F2)
where
    F1: FnOnce(&mut TransactionContext) -> Result<(), Error>,
    F2: FnOnce(&mut TransactionContext) -> Result<(), Error>,
{
    // Both should commit successfully
}

/// Generate random JSON tree for fuzzing
pub fn random_json_tree(depth: usize, seed: u64) -> (JsonValue, Vec<JsonPath>) {
    // Returns tree and all valid paths into it
}

/// Simulate crash by dropping DB without flush
pub fn simulate_crash(db: Arc<Database>) {
    // Force drop without proper shutdown
    drop(db);
}

/// Recover database from WAL
pub fn recover_db(path: &Path) -> Arc<Database> {
    // Re-open database, triggering WAL replay
}
```

---

## Implementation Priority

| Priority | Tier | Estimated Tests | Rationale |
|----------|------|-----------------|-----------|
| **P0** | Tier 1: Semantic Invariants | ~30 | Lock in the contract |
| **P0** | Tier 2: Conflict Fuzzing | ~10 | Catches most bugs |
| **P1** | Tier 9: Spec Conformance | ~25 | Spec = executable |
| **P1** | Tier 4: Cross-Primitive | ~15 | Atomicity guarantee |
| **P1** | Tier 3: WAL/Recovery | ~15 | Durability guarantee |
| **P2** | Tier 6: Mutation Algebra | ~10 | Lock in algebra |
| **P2** | Tier 5: Serializability | ~5 | Correctness proof |
| **P3** | Tier 7: Stress | ~10 | Edge case coverage |
| **P3** | Tier 8: Backward Compat | ~5 | Future-proofing |

**Total: ~125 new tests**

---

## Dependencies

```toml
[dev-dependencies]
proptest = "1.4"          # Property-based testing
tempfile = "3.10"         # Temporary directories for crash tests
```

---

## Success Criteria

1. **All Tier 1 tests pass** - Semantic invariants locked
2. **Fuzzing finds no violations** - 10,000+ random cases pass
3. **WAL replay is idempotent** - Replay twice = same state
4. **Cross-primitive atomicity verified** - All-or-nothing commit
5. **Spec coverage > 95%** - Every spec statement has a test
6. **No M4 regressions** - Existing tests still pass

---

## Notes

- These tests are **separate from unit tests** - they test public API behavior
- Tests should read like **English specifications**, not implementation details
- **Fuzzing is mandatory** - Property-based tests catch what humans miss
- **WAL torture tests are mandatory** - Most databases die in recovery
- Run stress tests **before every release** - Find rare bugs early

---

*End of M5 Comprehensive Test Plan*
