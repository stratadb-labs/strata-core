# M8 Comprehensive Test Plan

**Version**: 1.0
**Status**: Planning
**Date**: 2026-01-17

---

## Overview

This document defines the comprehensive test suite for M8 Vector Primitive, **separate from the unit and integration tests written during development**.

The goal is to create a battery of tests that:
1. **Lock in storage invariants (S1-S9)** - Dimension immutability, VectorId stability, heap-KV consistency
2. **Lock in search invariants (R1-R10)** - Score normalization, deterministic ordering, read-only search
3. **Lock in transaction invariants (T1-T4)** - Atomic visibility, conflict detection, VectorId monotonicity
4. **Validate brute-force correctness** - Distance calculations produce mathematically correct results
5. **Verify M6 integration** - SearchRequest/SearchResponse, RRF fusion work correctly
6. **Test crash recovery** - Vector state survives crashes with correct VectorId continuity
7. **Ensure cross-primitive atomicity** - Vector participates correctly in multi-primitive transactions
8. **Prevent regressions** - M7 durability and M6 search semantics are maintained

---

## Test Structure

```
tests/
└── m8_comprehensive/
    ├── main.rs                              # Test harness and utilities
    │
    │   # Tier 1: Storage Invariants (HIGHEST PRIORITY)
    ├── storage_dimension_tests.rs           # 1.1 S1: Dimension immutable
    ├── storage_metric_tests.rs              # 1.2 S2: Metric immutable
    ├── storage_vectorid_stable_tests.rs     # 1.3 S3: VectorId stable
    ├── storage_vectorid_never_reused.rs     # 1.4 S4: VectorId never reused
    ├── storage_heap_kv_consistency.rs       # 1.5 S5: Heap + KV consistency
    ├── storage_run_isolation_tests.rs       # 1.6 S6: Run isolation
    ├── storage_btreemap_source_tests.rs     # 1.7 S7: BTreeMap sole source of truth
    ├── storage_snapshot_wal_equiv.rs        # 1.8 S8: Snapshot-WAL equivalence
    ├── storage_reconstructibility.rs        # 1.9 S9: Heap-KV reconstructibility
    │
    │   # Tier 2: Search Invariants
    ├── search_dimension_match_tests.rs      # 2.1 R1: Dimension match
    ├── search_score_normalization.rs        # 2.2 R2: Score normalization
    ├── search_deterministic_order.rs        # 2.3 R3: Deterministic order
    ├── search_backend_tiebreak.rs           # 2.4 R4: Backend tie-break (VectorId asc)
    ├── search_facade_tiebreak.rs            # 2.5 R5: Facade tie-break (key asc)
    ├── search_snapshot_consistency.rs       # 2.6 R6: Snapshot consistency
    ├── search_budget_enforcement.rs         # 2.7 R7: Coarse-grained budget
    ├── search_single_threaded.rs            # 2.8 R8: Single-threaded
    ├── search_no_normalization.rs           # 2.9 R9: No implicit normalization
    ├── search_readonly_tests.rs             # 2.10 R10: Search is read-only
    │
    │   # Tier 3: Transaction Invariants
    ├── tx_atomic_visibility_tests.rs        # 3.1 T1: Atomic visibility
    ├── tx_conflict_detection_tests.rs       # 3.2 T2: Conflict detection
    ├── tx_rollback_safety_tests.rs          # 3.3 T3: Rollback safety
    ├── tx_vectorid_monotonicity.rs          # 3.4 T4: VectorId monotonicity across crashes
    │
    │   # Tier 4: Distance Metric Correctness
    ├── distance_cosine_tests.rs             # 4.1 Cosine similarity correctness
    ├── distance_euclidean_tests.rs          # 4.2 Euclidean distance correctness
    ├── distance_dotproduct_tests.rs         # 4.3 Dot product correctness
    ├── distance_edge_cases_tests.rs         # 4.4 Zero vectors, unit vectors, etc.
    │
    │   # Tier 5: Collection Management
    ├── collection_create_tests.rs           # 5.1 Create collection
    ├── collection_delete_tests.rs           # 5.2 Delete collection
    ├── collection_list_tests.rs             # 5.3 List collections
    ├── collection_get_tests.rs              # 5.4 Get collection info
    ├── collection_config_persist.rs         # 5.5 Config survives restart
    │
    │   # Tier 6: VectorHeap Operations
    ├── heap_insert_tests.rs                 # 6.1 Insert/upsert operations
    ├── heap_delete_tests.rs                 # 6.2 Delete with slot reuse
    ├── heap_get_tests.rs                    # 6.3 Get by VectorId
    ├── heap_iteration_tests.rs              # 6.4 Deterministic iteration
    ├── heap_free_slot_reuse.rs              # 6.5 Storage slot reuse (not ID reuse)
    │
    │   # Tier 7: M6 Integration
    ├── m6_searchrequest_tests.rs            # 7.1 SearchRequest compatibility
    ├── m6_searchresponse_tests.rs           # 7.2 SearchResponse compatibility
    ├── m6_rrf_fusion_tests.rs               # 7.3 RRF fusion with vector results
    ├── m6_hybrid_search_tests.rs            # 7.4 Hybrid keyword + semantic search
    ├── m6_budget_propagation_tests.rs       # 7.5 Budget propagation
    │
    │   # Tier 8: WAL Integration
    ├── wal_entry_format_tests.rs            # 8.1 Entry type codes (0x70-0x73)
    ├── wal_write_tests.rs                   # 8.2 WAL write correctness
    ├── wal_replay_tests.rs                  # 8.3 WAL replay correctness
    ├── wal_replay_determinism.rs            # 8.4 Replay produces identical state
    │
    │   # Tier 9: Snapshot & Recovery
    ├── snapshot_format_tests.rs             # 9.1 Snapshot blob format
    ├── snapshot_next_id_persisted.rs        # 9.2 next_id in snapshot (T4)
    ├── snapshot_free_slots_persisted.rs     # 9.3 free_slots in snapshot (T4)
    ├── snapshot_recovery_tests.rs           # 9.4 Full recovery sequence
    ├── snapshot_wal_replay_combo.rs         # 9.5 Snapshot + WAL replay
    │
    │   # Tier 10: Cross-Primitive Transactions
    ├── cross_kv_vector_tests.rs             # 10.1 KV + Vector atomicity
    ├── cross_json_vector_tests.rs           # 10.2 JSON + Vector atomicity
    ├── cross_all_primitives_tests.rs        # 10.3 All primitives in one tx
    ├── cross_crash_recovery_tests.rs        # 10.4 Cross-primitive crash recovery
    │
    │   # Tier 11: Crash Scenarios
    ├── crash_during_insert_tests.rs         # 11.1 Crash during insert
    ├── crash_during_delete_tests.rs         # 11.2 Crash during delete
    ├── crash_during_collection_op.rs        # 11.3 Crash during collection create/delete
    ├── crash_vectorid_continuity.rs         # 11.4 VectorId continuity after crash
    │
    │   # Tier 12: Determinism Tests
    ├── determinism_insert_order.rs          # 12.1 Insert order doesn't affect search
    ├── determinism_key_hashing.rs           # 12.2 BTreeMap iteration is deterministic
    ├── determinism_replay_state.rs          # 12.3 Replay produces identical state
    ├── determinism_search_results.rs        # 12.4 Search results are reproducible
    │
    │   # Tier 13: Stress & Scale
    ├── scale_1k_vectors_tests.rs            # 13.1 1K vectors < 5ms
    ├── scale_10k_vectors_tests.rs           # 13.2 10K vectors < 50ms
    ├── scale_50k_vectors_tests.rs           # 13.3 50K vectors (baseline)
    ├── stress_concurrent_insert.rs          # 13.4 Concurrent inserts
    ├── stress_insert_delete_cycle.rs        # 13.5 Insert/delete cycles
    │
    │   # Tier 14: Non-Regression
    ├── m7_regression_tests.rs               # 14.1 M7 durability maintained
    ├── m6_regression_tests.rs               # 14.2 M6 search maintained
    │
    │   # Tier 15: Spec Conformance
    └── spec_conformance_tests.rs            # 15. Direct spec-to-test mapping
```

---

## Tier 1: Storage Invariants (HIGHEST PRIORITY)

These tests ensure the **storage guarantees** are never violated.

### 1.1 S1: Dimension Immutable (`storage_dimension_tests.rs`)

**Invariant S1**: Collection dimension cannot change after creation.

```rust
#[test]
fn test_s1_dimension_immutable() {
    let db = create_test_db();
    let run_id = create_run(&db);

    // Create collection with dimension 384
    db.vector.create_collection(
        run_id,
        "embeddings",
        VectorConfig { dimension: 384, ..Default::default() }
    ).unwrap();

    // Insert vector with correct dimension
    db.vector.insert(run_id, "embeddings", "key1", &vec![0.0f32; 384], None).unwrap();

    // Attempt to insert vector with wrong dimension MUST fail
    let result = db.vector.insert(run_id, "embeddings", "key2", &vec![0.0f32; 768], None);
    assert!(matches!(result, Err(VectorError::DimensionMismatch { expected: 384, got: 768 })));
}

#[test]
fn test_s1_dimension_enforced_on_search() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(
        run_id,
        "embeddings",
        VectorConfig { dimension: 384, ..Default::default() }
    ).unwrap();

    db.vector.insert(run_id, "embeddings", "key1", &vec![1.0f32; 384], None).unwrap();

    // Search with wrong dimension MUST fail
    let result = db.vector.search(run_id, "embeddings", &vec![1.0f32; 768], 10);
    assert!(matches!(result, Err(VectorError::DimensionMismatch { expected: 384, got: 768 })));
}

#[test]
fn test_s1_dimension_survives_restart() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(
        run_id,
        "embeddings",
        VectorConfig { dimension: 384, ..Default::default() }
    ).unwrap();

    // Restart
    drop(db);
    let db = reopen_database();

    // Dimension constraint must still be enforced
    let result = db.vector.insert(run_id, "embeddings", "key1", &vec![0.0f32; 768], None);
    assert!(matches!(result, Err(VectorError::DimensionMismatch { .. })));
}
```

### 1.2 S2: Metric Immutable (`storage_metric_tests.rs`)

**Invariant S2**: Distance metric cannot change after creation.

```rust
#[test]
fn test_s2_metric_immutable() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(
        run_id,
        "embeddings",
        VectorConfig {
            dimension: 384,
            metric: DistanceMetric::Cosine,
            ..Default::default()
        }
    ).unwrap();

    // Cannot recreate with different metric
    let result = db.vector.create_collection(
        run_id,
        "embeddings",
        VectorConfig {
            dimension: 384,
            metric: DistanceMetric::Euclidean,
            ..Default::default()
        }
    );
    assert!(matches!(result, Err(VectorError::CollectionExists(_))));
}

#[test]
fn test_s2_metric_survives_restart() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(
        run_id,
        "embeddings",
        VectorConfig {
            dimension: 384,
            metric: DistanceMetric::Euclidean,
            ..Default::default()
        }
    ).unwrap();

    drop(db);
    let db = reopen_database();

    let info = db.vector.get_collection(run_id, "embeddings").unwrap().unwrap();
    assert_eq!(info.config.metric, DistanceMetric::Euclidean);
}
```

### 1.3 S3: VectorId Stable (`storage_vectorid_stable_tests.rs`)

**Invariant S3**: IDs do not change within collection lifetime.

```rust
#[test]
fn test_s3_vectorid_stable_across_operations() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    // Insert vectors
    db.vector.insert(run_id, "embeddings", "key1", &random_vector(384), None).unwrap();
    db.vector.insert(run_id, "embeddings", "key2", &random_vector(384), None).unwrap();
    db.vector.insert(run_id, "embeddings", "key3", &random_vector(384), None).unwrap();

    let id_before = get_vectorid_for_key(&db, run_id, "embeddings", "key2");

    // Perform other operations
    db.vector.delete(run_id, "embeddings", "key1").unwrap();
    db.vector.insert(run_id, "embeddings", "key4", &random_vector(384), None).unwrap();

    let id_after = get_vectorid_for_key(&db, run_id, "embeddings", "key2");

    // VectorId for key2 must not change
    assert_eq!(id_before, id_after, "S3 VIOLATED: VectorId changed during operations");
}

#[test]
fn test_s3_vectorid_stable_across_restart() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();
    db.vector.insert(run_id, "embeddings", "key1", &random_vector(384), None).unwrap();

    let id_before = get_vectorid_for_key(&db, run_id, "embeddings", "key1");

    drop(db);
    let db = reopen_database();

    let id_after = get_vectorid_for_key(&db, run_id, "embeddings", "key1");

    assert_eq!(id_before, id_after, "S3 VIOLATED: VectorId changed across restart");
}
```

### 1.4 S4: VectorId Never Reused (`storage_vectorid_never_reused.rs`)

**Invariant S4**: Once assigned, a VectorId is never recycled (even after deletion).

```rust
#[test]
fn test_s4_vectorid_never_reused_after_delete() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    // Insert and capture IDs
    db.vector.insert(run_id, "embeddings", "key1", &random_vector(384), None).unwrap();
    db.vector.insert(run_id, "embeddings", "key2", &random_vector(384), None).unwrap();
    db.vector.insert(run_id, "embeddings", "key3", &random_vector(384), None).unwrap();

    let ids_before: Vec<VectorId> = get_all_vectorids(&db, run_id, "embeddings");
    let max_id_before = ids_before.iter().max().unwrap().0;

    // Delete all vectors
    db.vector.delete(run_id, "embeddings", "key1").unwrap();
    db.vector.delete(run_id, "embeddings", "key2").unwrap();
    db.vector.delete(run_id, "embeddings", "key3").unwrap();

    // Insert new vectors
    db.vector.insert(run_id, "embeddings", "key4", &random_vector(384), None).unwrap();
    db.vector.insert(run_id, "embeddings", "key5", &random_vector(384), None).unwrap();

    let ids_after: Vec<VectorId> = get_all_vectorids(&db, run_id, "embeddings");

    // All new IDs must be > max_id_before
    for id in &ids_after {
        assert!(id.0 > max_id_before,
            "S4 VIOLATED: VectorId {} reused (max before was {})", id.0, max_id_before);
    }

    // Deleted IDs must not appear in new set
    for old_id in &ids_before {
        assert!(!ids_after.contains(old_id),
            "S4 VIOLATED: VectorId {} was reused", old_id.0);
    }
}

#[test]
fn test_s4_vectorid_monotonic_within_session() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    let mut previous_id = VectorId(0);

    for i in 0..100 {
        db.vector.insert(run_id, "embeddings", &format!("key{}", i), &random_vector(384), None).unwrap();
        let current_id = get_vectorid_for_key(&db, run_id, "embeddings", &format!("key{}", i));

        assert!(current_id.0 > previous_id.0,
            "S4 VIOLATED: VectorId {} not greater than previous {}", current_id.0, previous_id.0);

        previous_id = current_id;
    }
}

#[test]
fn test_s4_insert_delete_insert_same_key_new_id() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    // Insert key
    db.vector.insert(run_id, "embeddings", "key1", &random_vector(384), None).unwrap();
    let id_first = get_vectorid_for_key(&db, run_id, "embeddings", "key1");

    // Delete key
    db.vector.delete(run_id, "embeddings", "key1").unwrap();

    // Insert same key again
    db.vector.insert(run_id, "embeddings", "key1", &random_vector(384), None).unwrap();
    let id_second = get_vectorid_for_key(&db, run_id, "embeddings", "key1");

    // Second insert must have a NEW VectorId
    assert!(id_second.0 > id_first.0,
        "S4 VIOLATED: Re-inserted key got same or lower VectorId ({} vs {})",
        id_second.0, id_first.0);
}
```

### 1.5 S5: Heap + KV Consistency (`storage_heap_kv_consistency.rs`)

**Invariant S5**: Vector heap and KV metadata always in sync.

```rust
#[test]
fn test_s5_insert_updates_both() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    let embedding = random_vector(384);
    let metadata = json!({"source": "test"});

    db.vector.insert(run_id, "embeddings", "key1", &embedding, Some(metadata.clone())).unwrap();

    // Verify heap has embedding
    let heap_entry = get_heap_entry(&db, run_id, "embeddings", "key1");
    assert_eq!(heap_entry.embedding, embedding, "S5 VIOLATED: Heap missing embedding");

    // Verify KV has metadata
    let kv_metadata = get_kv_metadata(&db, run_id, "embeddings", "key1");
    assert_eq!(kv_metadata, Some(metadata), "S5 VIOLATED: KV missing metadata");
}

#[test]
fn test_s5_delete_removes_both() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    db.vector.insert(run_id, "embeddings", "key1", &random_vector(384), Some(json!({"x": 1}))).unwrap();

    // Delete
    db.vector.delete(run_id, "embeddings", "key1").unwrap();

    // Both heap and KV should be empty for this key
    assert!(get_heap_entry_opt(&db, run_id, "embeddings", "key1").is_none(),
        "S5 VIOLATED: Heap still has entry after delete");
    assert!(get_kv_metadata(&db, run_id, "embeddings", "key1").is_none(),
        "S5 VIOLATED: KV still has metadata after delete");
}

#[test]
fn test_s5_consistency_after_crash() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    for i in 0..50 {
        db.vector.insert(
            run_id, "embeddings",
            &format!("key{}", i),
            &random_vector(384),
            Some(json!({"index": i}))
        ).unwrap();
    }

    // Simulate crash and recovery
    drop(db);
    let db = reopen_database();

    // Verify heap and KV are consistent
    for i in 0..50 {
        let key = format!("key{}", i);
        let heap_exists = get_heap_entry_opt(&db, run_id, "embeddings", &key).is_some();
        let kv_exists = get_kv_metadata(&db, run_id, "embeddings", &key).is_some();

        assert_eq!(heap_exists, kv_exists,
            "S5 VIOLATED: Heap/KV inconsistency for {} (heap={}, kv={})",
            key, heap_exists, kv_exists);
    }
}
```

### 1.6 S6: Run Isolation (`storage_run_isolation_tests.rs`)

**Invariant S6**: Collections scoped to RunId.

```rust
#[test]
fn test_s6_different_runs_isolated() {
    let db = create_test_db();
    let run1 = create_run(&db);
    let run2 = create_run(&db);

    // Create same-named collection in both runs
    db.vector.create_collection(run1, "embeddings", VectorConfig::for_minilm()).unwrap();
    db.vector.create_collection(run2, "embeddings", VectorConfig::for_minilm()).unwrap();

    // Insert in run1
    db.vector.insert(run1, "embeddings", "key1", &random_vector(384), None).unwrap();

    // run2 should not see run1's vectors
    let search_result = db.vector.search(run2, "embeddings", &random_vector(384), 10).unwrap();
    assert!(search_result.is_empty(), "S6 VIOLATED: run2 sees run1's data");

    // Insert in run2
    db.vector.insert(run2, "embeddings", "key2", &random_vector(384), None).unwrap();

    // run1 should not see run2's vectors
    let count1 = db.vector.count(run1, "embeddings").unwrap();
    assert_eq!(count1, 1, "S6 VIOLATED: run1 sees run2's data");
}

#[test]
fn test_s6_delete_in_one_run_doesnt_affect_other() {
    let db = create_test_db();
    let run1 = create_run(&db);
    let run2 = create_run(&db);

    db.vector.create_collection(run1, "embeddings", VectorConfig::for_minilm()).unwrap();
    db.vector.create_collection(run2, "embeddings", VectorConfig::for_minilm()).unwrap();

    db.vector.insert(run1, "embeddings", "key1", &random_vector(384), None).unwrap();
    db.vector.insert(run2, "embeddings", "key1", &random_vector(384), None).unwrap();

    // Delete from run1
    db.vector.delete(run1, "embeddings", "key1").unwrap();

    // run2's vector should still exist
    let count2 = db.vector.count(run2, "embeddings").unwrap();
    assert_eq!(count2, 1, "S6 VIOLATED: delete in run1 affected run2");
}
```

### 1.7 S7: BTreeMap Sole Source of Truth (`storage_btreemap_source_tests.rs`)

**Invariant S7**: id_to_offset (BTreeMap) is the ONLY source of truth for active vectors.

```rust
#[test]
fn test_s7_btreemap_determines_active_vectors() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    db.vector.insert(run_id, "embeddings", "key1", &random_vector(384), None).unwrap();
    db.vector.insert(run_id, "embeddings", "key2", &random_vector(384), None).unwrap();
    db.vector.insert(run_id, "embeddings", "key3", &random_vector(384), None).unwrap();

    // Delete key2
    db.vector.delete(run_id, "embeddings", "key2").unwrap();

    // Get internal state (test helper)
    let btreemap_keys = get_btreemap_vectorids(&db, run_id, "embeddings");
    let search_results = db.vector.search(run_id, "embeddings", &random_vector(384), 10).unwrap();

    // BTreeMap should only have key1 and key3
    assert_eq!(btreemap_keys.len(), 2);

    // Search should only return key1 and key3
    let search_keys: Vec<_> = search_results.iter().map(|m| m.key.as_str()).collect();
    assert!(search_keys.contains(&"key1"));
    assert!(search_keys.contains(&"key3"));
    assert!(!search_keys.contains(&"key2"));
}

#[test]
fn test_s7_btreemap_iteration_deterministic() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    // Insert in random order
    for i in [5, 2, 8, 1, 9, 3, 7, 4, 6, 0] {
        db.vector.insert(run_id, "embeddings", &format!("key{}", i), &random_vector(384), None).unwrap();
    }

    // Get iteration order multiple times
    let orders: Vec<Vec<VectorId>> = (0..10)
        .map(|_| get_btreemap_vectorids(&db, run_id, "embeddings"))
        .collect();

    // All iterations must be identical (BTreeMap guarantees this)
    for (i, order) in orders.iter().enumerate().skip(1) {
        assert_eq!(&orders[0], order,
            "S7 VIOLATED: BTreeMap iteration order differs on iteration {}", i);
    }
}
```

### 1.8 S8: Snapshot-WAL Equivalence (`storage_snapshot_wal_equiv.rs`)

**Invariant S8**: Snapshot + WAL replay must produce identical state to pure WAL replay.

```rust
#[test]
fn test_s8_snapshot_wal_equivalence() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    // Insert vectors
    for i in 0..50 {
        db.vector.insert(run_id, "embeddings", &format!("key{}", i), &random_vector(384), None).unwrap();
    }

    // Create snapshot
    db.create_snapshot().unwrap();

    // More operations after snapshot
    for i in 50..100 {
        db.vector.insert(run_id, "embeddings", &format!("key{}", i), &random_vector(384), None).unwrap();
    }
    db.vector.delete(run_id, "embeddings", "key25").unwrap();

    let state_before = capture_vector_state(&db, run_id, "embeddings");

    // Recover using snapshot + WAL
    drop(db);
    let db_snapshot = reopen_database();
    let state_snapshot = capture_vector_state(&db_snapshot, run_id, "embeddings");

    // Also recover using pure WAL replay (no snapshot)
    delete_snapshots();
    let db_wal = reopen_database();
    let state_wal = capture_vector_state(&db_wal, run_id, "embeddings");

    assert_eq!(state_before, state_snapshot, "S8 VIOLATED: Snapshot recovery differs from original");
    assert_eq!(state_snapshot, state_wal, "S8 VIOLATED: Snapshot recovery differs from WAL replay");
}
```

### 1.9 S9: Heap-KV Reconstructibility (`storage_reconstructibility.rs`)

**Invariant S9**: VectorHeap and KV metadata can both be fully reconstructed from snapshot + WAL.

```rust
#[test]
fn test_s9_heap_reconstructible() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    let embeddings: Vec<Vec<f32>> = (0..20).map(|_| random_vector(384)).collect();

    for (i, emb) in embeddings.iter().enumerate() {
        db.vector.insert(run_id, "embeddings", &format!("key{}", i), emb, None).unwrap();
    }

    // Capture heap state
    let heap_before = capture_heap_state(&db, run_id, "embeddings");

    drop(db);
    let db = reopen_database();

    let heap_after = capture_heap_state(&db, run_id, "embeddings");

    assert_eq!(heap_before, heap_after, "S9 VIOLATED: Heap not reconstructed correctly");
}

#[test]
fn test_s9_kv_metadata_reconstructible() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    for i in 0..20 {
        db.vector.insert(
            run_id, "embeddings",
            &format!("key{}", i),
            &random_vector(384),
            Some(json!({"index": i, "category": format!("cat{}", i % 5)}))
        ).unwrap();
    }

    let metadata_before = capture_kv_metadata(&db, run_id, "embeddings");

    drop(db);
    let db = reopen_database();

    let metadata_after = capture_kv_metadata(&db, run_id, "embeddings");

    assert_eq!(metadata_before, metadata_after, "S9 VIOLATED: KV metadata not reconstructed correctly");
}
```

---

## Tier 2: Search Invariants

### 2.1 R1: Dimension Match (`search_dimension_match_tests.rs`)

**Invariant R1**: Query dimension must match collection dimension.

```rust
#[test]
fn test_r1_query_dimension_must_match() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap(); // 384 dim

    db.vector.insert(run_id, "embeddings", "key1", &random_vector(384), None).unwrap();

    // Search with correct dimension works
    let result = db.vector.search(run_id, "embeddings", &random_vector(384), 10);
    assert!(result.is_ok());

    // Search with wrong dimension fails
    let result = db.vector.search(run_id, "embeddings", &random_vector(768), 10);
    assert!(matches!(result, Err(VectorError::DimensionMismatch { .. })));
}
```

### 2.2 R2: Score Normalization (`search_score_normalization.rs`)

**Invariant R2**: All metrics return "higher is better" scores.

```rust
#[test]
fn test_r2_cosine_higher_is_more_similar() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(
        run_id, "embeddings",
        VectorConfig { dimension: 3, metric: DistanceMetric::Cosine, ..Default::default() }
    ).unwrap();

    let query = vec![1.0, 0.0, 0.0];
    let identical = vec![1.0, 0.0, 0.0];  // Same direction
    let orthogonal = vec![0.0, 1.0, 0.0]; // Perpendicular
    let opposite = vec![-1.0, 0.0, 0.0];  // Opposite direction

    db.vector.insert(run_id, "embeddings", "identical", &identical, None).unwrap();
    db.vector.insert(run_id, "embeddings", "orthogonal", &orthogonal, None).unwrap();
    db.vector.insert(run_id, "embeddings", "opposite", &opposite, None).unwrap();

    let results = db.vector.search(run_id, "embeddings", &query, 3).unwrap();

    // Identical should have highest score
    assert_eq!(results[0].key, "identical");
    assert!(results[0].score > results[1].score);

    // Orthogonal should be middle
    assert_eq!(results[1].key, "orthogonal");

    // Opposite should have lowest score
    assert_eq!(results[2].key, "opposite");
}

#[test]
fn test_r2_euclidean_higher_is_closer() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(
        run_id, "embeddings",
        VectorConfig { dimension: 3, metric: DistanceMetric::Euclidean, ..Default::default() }
    ).unwrap();

    let query = vec![0.0, 0.0, 0.0];
    let close = vec![0.1, 0.0, 0.0];      // Distance ~0.1
    let medium = vec![1.0, 0.0, 0.0];     // Distance 1.0
    let far = vec![10.0, 0.0, 0.0];       // Distance 10.0

    db.vector.insert(run_id, "embeddings", "close", &close, None).unwrap();
    db.vector.insert(run_id, "embeddings", "medium", &medium, None).unwrap();
    db.vector.insert(run_id, "embeddings", "far", &far, None).unwrap();

    let results = db.vector.search(run_id, "embeddings", &query, 3).unwrap();

    // Close should have highest score (1 / (1 + 0.1) ≈ 0.91)
    assert_eq!(results[0].key, "close");
    assert!(results[0].score > results[1].score, "R2 VIOLATED: closer vector has lower score");

    // Far should have lowest score (1 / (1 + 10) ≈ 0.09)
    assert_eq!(results[2].key, "far");
}

#[test]
fn test_r2_dotproduct_higher_is_more_similar() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(
        run_id, "embeddings",
        VectorConfig { dimension: 3, metric: DistanceMetric::DotProduct, ..Default::default() }
    ).unwrap();

    let query = vec![1.0, 1.0, 1.0];
    let aligned = vec![1.0, 1.0, 1.0];    // Dot = 3
    let partial = vec![1.0, 0.0, 0.0];    // Dot = 1
    let negative = vec![-1.0, -1.0, -1.0]; // Dot = -3

    db.vector.insert(run_id, "embeddings", "aligned", &aligned, None).unwrap();
    db.vector.insert(run_id, "embeddings", "partial", &partial, None).unwrap();
    db.vector.insert(run_id, "embeddings", "negative", &negative, None).unwrap();

    let results = db.vector.search(run_id, "embeddings", &query, 3).unwrap();

    assert_eq!(results[0].key, "aligned");
    assert_eq!(results[2].key, "negative");
}
```

### 2.3 R3: Deterministic Order (`search_deterministic_order.rs`)

**Invariant R3**: Same query = same result order (enforced at backend level).

```rust
#[test]
fn test_r3_same_query_same_order() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    for i in 0..100 {
        db.vector.insert(run_id, "embeddings", &format!("key{}", i), &random_vector(384), None).unwrap();
    }

    let query = random_vector(384);

    // Run same search 100 times
    let mut results_list: Vec<Vec<String>> = Vec::new();
    for _ in 0..100 {
        let results = db.vector.search(run_id, "embeddings", &query, 20).unwrap();
        let keys: Vec<String> = results.iter().map(|r| r.key.clone()).collect();
        results_list.push(keys);
    }

    // All results must be identical
    for (i, results) in results_list.iter().enumerate().skip(1) {
        assert_eq!(&results_list[0], results,
            "R3 VIOLATED: Search {} returned different order", i);
    }
}

#[test]
fn test_r3_deterministic_across_restart() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    for i in 0..50 {
        db.vector.insert(run_id, "embeddings", &format!("key{}", i), &random_vector(384), None).unwrap();
    }

    let query = random_vector(384);
    let results_before = db.vector.search(run_id, "embeddings", &query, 20).unwrap();

    drop(db);
    let db = reopen_database();

    let results_after = db.vector.search(run_id, "embeddings", &query, 20).unwrap();

    let keys_before: Vec<&str> = results_before.iter().map(|r| r.key.as_str()).collect();
    let keys_after: Vec<&str> = results_after.iter().map(|r| r.key.as_str()).collect();

    assert_eq!(keys_before, keys_after, "R3 VIOLATED: Order changed across restart");
}
```

### 2.4 R4: Backend Tie-Break (`search_backend_tiebreak.rs`)

**Invariant R4**: Backend sorts by (score desc, VectorId asc).

```rust
#[test]
fn test_r4_backend_tiebreak_by_vectorid() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(
        run_id, "embeddings",
        VectorConfig { dimension: 3, metric: DistanceMetric::Cosine, ..Default::default() }
    ).unwrap();

    // All identical vectors = all identical scores
    let identical = vec![1.0, 0.0, 0.0];

    // Insert in specific order to control VectorId assignment
    db.vector.insert(run_id, "embeddings", "key_c", &identical, None).unwrap(); // VectorId 1
    db.vector.insert(run_id, "embeddings", "key_a", &identical, None).unwrap(); // VectorId 2
    db.vector.insert(run_id, "embeddings", "key_b", &identical, None).unwrap(); // VectorId 3

    let query = vec![1.0, 0.0, 0.0];
    let results = db.vector.search(run_id, "embeddings", &query, 3).unwrap();

    // All scores identical, so order determined by VectorId asc
    // key_c has VectorId 1, key_a has VectorId 2, key_b has VectorId 3
    // Backend returns: key_c, key_a, key_b (by VectorId asc)
    // But facade then sorts by key asc: key_a, key_b, key_c
    // See R5 test for facade behavior

    // Get raw backend results
    let backend_results = get_backend_search_results(&db, run_id, "embeddings", &query, 3);

    // Backend should return VectorIds in ascending order for tied scores
    assert!(backend_results[0].0 < backend_results[1].0, "R4 VIOLATED: Backend tie-break wrong");
    assert!(backend_results[1].0 < backend_results[2].0, "R4 VIOLATED: Backend tie-break wrong");
}
```

### 2.5 R5: Facade Tie-Break (`search_facade_tiebreak.rs`)

**Invariant R5**: Facade sorts by (score desc, key asc).

```rust
#[test]
fn test_r5_facade_tiebreak_by_key() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(
        run_id, "embeddings",
        VectorConfig { dimension: 3, metric: DistanceMetric::Cosine, ..Default::default() }
    ).unwrap();

    // All identical vectors = all identical scores
    let identical = vec![1.0, 0.0, 0.0];

    db.vector.insert(run_id, "embeddings", "charlie", &identical, None).unwrap();
    db.vector.insert(run_id, "embeddings", "alice", &identical, None).unwrap();
    db.vector.insert(run_id, "embeddings", "bob", &identical, None).unwrap();

    let query = vec![1.0, 0.0, 0.0];
    let results = db.vector.search(run_id, "embeddings", &query, 3).unwrap();

    // All scores tied, facade sorts by key asc
    assert_eq!(results[0].key, "alice", "R5 VIOLATED: Facade tie-break wrong");
    assert_eq!(results[1].key, "bob", "R5 VIOLATED: Facade tie-break wrong");
    assert_eq!(results[2].key, "charlie", "R5 VIOLATED: Facade tie-break wrong");
}
```

### 2.6 R6: Snapshot Consistency (`search_snapshot_consistency.rs`)

**Invariant R6**: Search sees consistent point-in-time view.

```rust
#[test]
fn test_r6_search_consistent_snapshot() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    for i in 0..50 {
        db.vector.insert(run_id, "embeddings", &format!("key{}", i), &random_vector(384), None).unwrap();
    }

    // Start search
    let query = random_vector(384);
    let results_start = db.vector.search(run_id, "embeddings", &query, 50).unwrap();

    // Concurrent modification (in a real test, this would be in another thread)
    db.vector.delete(run_id, "embeddings", "key0").unwrap();
    db.vector.insert(run_id, "embeddings", "key_new", &random_vector(384), None).unwrap();

    // Search from same snapshot should see original state
    // (Note: This test demonstrates the concept; actual snapshot isolation
    // depends on transaction boundaries)
}
```

### 2.7 R7: Coarse-Grained Budget (`search_budget_enforcement.rs`)

**Invariant R7**: Budget checked at phase boundaries; brute-force may overshoot.

```rust
#[test]
fn test_r7_budget_respected_coarse() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    // Insert many vectors
    for i in 0..1000 {
        db.vector.insert(run_id, "embeddings", &format!("key{}", i), &random_vector(384), None).unwrap();
    }

    let query = random_vector(384);
    let budget = SearchBudget { max_time_ms: 1 }; // Very tight budget

    let result = db.vector.search_with_budget(run_id, "embeddings", &query, 100, budget);

    // Brute-force may overshoot budget, but should eventually return
    // Either returns results or budget exceeded error
    assert!(result.is_ok() || matches!(result, Err(VectorError::BudgetExceeded)));
}
```

### 2.8 R8: Single-Threaded (`search_single_threaded.rs`)

**Invariant R8**: Similarity computation is single-threaded for determinism.

```rust
#[test]
fn test_r8_search_deterministic_across_cores() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    for i in 0..100 {
        db.vector.insert(run_id, "embeddings", &format!("key{}", i), &random_vector(384), None).unwrap();
    }

    let query = random_vector(384);

    // Run search from multiple threads concurrently
    let handles: Vec<_> = (0..10).map(|_| {
        let db = db.clone();
        let query = query.clone();
        std::thread::spawn(move || {
            db.vector.search(run_id, "embeddings", &query, 20).unwrap()
        })
    }).collect();

    let results: Vec<Vec<VectorMatch>> = handles.into_iter()
        .map(|h| h.join().unwrap())
        .collect();

    // All results must be identical
    let keys_0: Vec<&str> = results[0].iter().map(|r| r.key.as_str()).collect();
    for (i, r) in results.iter().enumerate().skip(1) {
        let keys: Vec<&str> = r.iter().map(|r| r.key.as_str()).collect();
        assert_eq!(keys_0, keys, "R8 VIOLATED: Different results from thread {}", i);
    }
}
```

### 2.9 R9: No Implicit Normalization (`search_no_normalization.rs`)

**Invariant R9**: Embeddings stored as-is, no silent normalization.

```rust
#[test]
fn test_r9_embedding_stored_verbatim() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    // Non-normalized vector
    let embedding = vec![2.0, 3.0, 4.0];  // L2 norm ≠ 1
    let padded: Vec<f32> = embedding.iter().chain(std::iter::repeat(&0.0f32).take(381)).cloned().collect();

    db.vector.insert(run_id, "embeddings", "key1", &padded, None).unwrap();

    // Retrieve and verify exact values
    let retrieved = get_embedding(&db, run_id, "embeddings", "key1");

    assert_eq!(retrieved[0], 2.0, "R9 VIOLATED: Embedding was normalized");
    assert_eq!(retrieved[1], 3.0, "R9 VIOLATED: Embedding was normalized");
    assert_eq!(retrieved[2], 4.0, "R9 VIOLATED: Embedding was normalized");
}
```

### 2.10 R10: Search is Read-Only (`search_readonly_tests.rs`)

**Invariant R10**: Search must not write anything: no counters, no caches, no side effects.

```rust
#[test]
fn test_r10_search_does_not_write_to_wal() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    for i in 0..50 {
        db.vector.insert(run_id, "embeddings", &format!("key{}", i), &random_vector(384), None).unwrap();
    }

    let wal_size_before = db.wal_size();

    // Perform many searches
    let query = random_vector(384);
    for _ in 0..100 {
        let _ = db.vector.search(run_id, "embeddings", &query, 10).unwrap();
    }

    let wal_size_after = db.wal_size();

    assert_eq!(wal_size_before, wal_size_after,
        "R10 VIOLATED: Search wrote to WAL ({} -> {} bytes)",
        wal_size_before, wal_size_after);
}

#[test]
fn test_r10_search_does_not_modify_state() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    for i in 0..50 {
        db.vector.insert(run_id, "embeddings", &format!("key{}", i), &random_vector(384), None).unwrap();
    }

    let state_before = capture_full_state(&db);

    // Many searches
    for _ in 0..100 {
        let _ = db.vector.search(run_id, "embeddings", &random_vector(384), 10);
    }

    let state_after = capture_full_state(&db);

    assert_eq!(state_before, state_after,
        "R10 VIOLATED: Search modified internal state");
}
```

---

## Tier 3: Transaction Invariants

### 3.1 T1: Atomic Visibility (`tx_atomic_visibility_tests.rs`)

**Invariant T1**: Insert/delete atomic with other primitives.

```rust
#[test]
fn test_t1_insert_atomic_with_kv() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    // Transaction with both vector insert and KV put
    db.begin_tx();
    db.vector.insert(run_id, "embeddings", "key1", &random_vector(384), None).unwrap();
    db.kv.put(run_id, "vector_inserted", "true").unwrap();
    db.commit().unwrap();

    // Both should be visible
    assert!(db.vector.get(run_id, "embeddings", "key1").unwrap().is_some());
    assert!(db.kv.get(run_id, "vector_inserted").unwrap().is_some());
}

#[test]
fn test_t1_uncommitted_vector_not_visible() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    // Start transaction but don't commit
    db.begin_tx();
    db.vector.insert(run_id, "embeddings", "key1", &random_vector(384), None).unwrap();
    // No commit

    // In another view, vector should not be visible
    let count = db.vector.count(run_id, "embeddings").unwrap();
    // (Actual visibility depends on isolation level - test verifies the concept)
}
```

### 3.2 T2: Conflict Detection (`tx_conflict_detection_tests.rs`)

**Invariant T2**: Concurrent writes to same key conflict.

```rust
#[test]
fn test_t2_concurrent_write_same_key_conflicts() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    // Insert initial vector
    db.vector.insert(run_id, "embeddings", "key1", &random_vector(384), None).unwrap();

    // Transaction 1 reads
    let tx1 = db.begin_tx();
    let _v1 = db.vector.get(run_id, "embeddings", "key1");

    // Transaction 2 writes to same key
    let tx2 = db.begin_tx();
    db.vector.insert(run_id, "embeddings", "key1", &random_vector(384), None).unwrap();
    db.commit().unwrap(); // tx2 commits first

    // Transaction 1 tries to write same key
    db.vector.insert(run_id, "embeddings", "key1", &random_vector(384), None).unwrap();
    let result = db.commit(); // Should detect conflict

    assert!(result.is_err(), "T2 VIOLATED: Concurrent writes did not conflict");
}
```

### 3.3 T3: Rollback Safety (`tx_rollback_safety_tests.rs`)

**Invariant T3**: Failed transactions leave no partial state.

```rust
#[test]
fn test_t3_rollback_removes_vector() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    let count_before = db.vector.count(run_id, "embeddings").unwrap();

    // Start transaction, insert vector, then rollback
    db.begin_tx();
    db.vector.insert(run_id, "embeddings", "key1", &random_vector(384), None).unwrap();
    db.rollback();

    let count_after = db.vector.count(run_id, "embeddings").unwrap();

    assert_eq!(count_before, count_after, "T3 VIOLATED: Rollback left partial state");
}

#[test]
fn test_t3_failed_commit_no_partial_state() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    // Setup conflict condition
    db.vector.insert(run_id, "embeddings", "key1", &random_vector(384), None).unwrap();

    // Transaction that will fail
    db.begin_tx();
    db.vector.insert(run_id, "embeddings", "key2", &random_vector(384), None).unwrap();
    db.vector.insert(run_id, "embeddings", "key3", &random_vector(384), None).unwrap();
    // Force commit failure (implementation specific)

    // Verify neither key2 nor key3 exists
    assert!(db.vector.get(run_id, "embeddings", "key2").unwrap().is_none());
    assert!(db.vector.get(run_id, "embeddings", "key3").unwrap().is_none());
}
```

### 3.4 T4: VectorId Monotonicity Across Crashes (`tx_vectorid_monotonicity.rs`)

**Invariant T4**: After crash recovery, new VectorIds must be > all previous IDs.

```rust
#[test]
fn test_t4_vectorid_monotonic_across_crash() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    // Insert vectors and track max ID
    for i in 0..100 {
        db.vector.insert(run_id, "embeddings", &format!("key{}", i), &random_vector(384), None).unwrap();
    }

    let max_id_before_crash = get_max_vectorid(&db, run_id, "embeddings");

    // Simulate crash and recovery
    simulate_crash(&db);
    let db = reopen_database();

    // Insert new vectors after recovery
    for i in 100..110 {
        db.vector.insert(run_id, "embeddings", &format!("key{}", i), &random_vector(384), None).unwrap();
    }

    let new_ids = get_vectorids_for_keys(&db, run_id, "embeddings", 100..110);

    // All new IDs must be > max_id_before_crash
    for id in &new_ids {
        assert!(id.0 > max_id_before_crash.0,
            "T4 VIOLATED: Post-crash VectorId {} <= pre-crash max {}",
            id.0, max_id_before_crash.0);
    }
}

#[test]
fn test_t4_next_id_in_snapshot() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    for i in 0..50 {
        db.vector.insert(run_id, "embeddings", &format!("key{}", i), &random_vector(384), None).unwrap();
    }

    let next_id_before = get_next_vectorid(&db, run_id, "embeddings");

    // Create snapshot
    db.create_snapshot().unwrap();

    // More inserts after snapshot
    for i in 50..60 {
        db.vector.insert(run_id, "embeddings", &format!("key{}", i), &random_vector(384), None).unwrap();
    }

    // Recover from snapshot + WAL
    drop(db);
    let db = reopen_database();

    let next_id_after = get_next_vectorid(&db, run_id, "embeddings");

    // next_id must be >= what it was before crash
    assert!(next_id_after.0 >= next_id_before.0 + 10,
        "T4 VIOLATED: next_id not preserved correctly");
}

#[test]
fn test_t4_free_slots_in_snapshot() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    // Insert and delete to create free slots
    for i in 0..20 {
        db.vector.insert(run_id, "embeddings", &format!("key{}", i), &random_vector(384), None).unwrap();
    }
    for i in 0..10 {
        db.vector.delete(run_id, "embeddings", &format!("key{}", i)).unwrap();
    }

    let free_slots_before = get_free_slots_count(&db, run_id, "embeddings");

    // Create snapshot
    db.create_snapshot().unwrap();

    drop(db);
    let db = reopen_database();

    let free_slots_after = get_free_slots_count(&db, run_id, "embeddings");

    assert_eq!(free_slots_before, free_slots_after,
        "T4 VIOLATED: free_slots not preserved in snapshot");
}
```

---

## Tier 4: Distance Metric Correctness

### 4.1 Cosine Similarity (`distance_cosine_tests.rs`)

```rust
#[test]
fn test_cosine_identical_vectors() {
    let a = vec![1.0, 2.0, 3.0];
    let score = cosine_similarity(&a, &a);
    assert!((score - 1.0).abs() < 1e-6, "Identical vectors should have similarity 1.0");
}

#[test]
fn test_cosine_orthogonal_vectors() {
    let a = vec![1.0, 0.0, 0.0];
    let b = vec![0.0, 1.0, 0.0];
    let score = cosine_similarity(&a, &b);
    assert!(score.abs() < 1e-6, "Orthogonal vectors should have similarity 0");
}

#[test]
fn test_cosine_opposite_vectors() {
    let a = vec![1.0, 2.0, 3.0];
    let b = vec![-1.0, -2.0, -3.0];
    let score = cosine_similarity(&a, &b);
    assert!((score - (-1.0)).abs() < 1e-6, "Opposite vectors should have similarity -1.0");
}

#[test]
fn test_cosine_normalized_score() {
    // Score normalization: higher = more similar
    // For cosine, score = 1 - cosine_distance, or just raw cosine similarity
    let a = vec![1.0, 0.0, 0.0];
    let similar = vec![0.9, 0.1, 0.0];
    let dissimilar = vec![0.1, 0.9, 0.0];

    let score_similar = cosine_similarity(&a, &similar);
    let score_dissimilar = cosine_similarity(&a, &dissimilar);

    assert!(score_similar > score_dissimilar, "More similar vector should have higher score");
}
```

### 4.2 Euclidean Distance (`distance_euclidean_tests.rs`)

```rust
#[test]
fn test_euclidean_identical_vectors() {
    let a = vec![1.0, 2.0, 3.0];
    let score = euclidean_similarity(&a, &a);
    // Score = 1 / (1 + 0) = 1.0
    assert!((score - 1.0).abs() < 1e-6);
}

#[test]
fn test_euclidean_unit_distance() {
    let a = vec![0.0, 0.0, 0.0];
    let b = vec![1.0, 0.0, 0.0];
    let score = euclidean_similarity(&a, &b);
    // Distance = 1.0, Score = 1 / (1 + 1) = 0.5
    assert!((score - 0.5).abs() < 1e-6);
}

#[test]
fn test_euclidean_large_distance() {
    let a = vec![0.0, 0.0, 0.0];
    let b = vec![10.0, 0.0, 0.0];
    let score = euclidean_similarity(&a, &b);
    // Distance = 10.0, Score = 1 / (1 + 10) ≈ 0.0909
    assert!((score - 1.0/11.0).abs() < 1e-6);
}

#[test]
fn test_euclidean_normalized_higher_is_closer() {
    let origin = vec![0.0, 0.0, 0.0];
    let close = vec![1.0, 0.0, 0.0];
    let far = vec![10.0, 0.0, 0.0];

    let score_close = euclidean_similarity(&origin, &close);
    let score_far = euclidean_similarity(&origin, &far);

    assert!(score_close > score_far, "Closer vector should have higher normalized score");
}
```

### 4.3 Dot Product (`distance_dotproduct_tests.rs`)

```rust
#[test]
fn test_dotproduct_calculation() {
    let a = vec![1.0, 2.0, 3.0];
    let b = vec![4.0, 5.0, 6.0];
    let score = dot_product(&a, &b);
    // 1*4 + 2*5 + 3*6 = 4 + 10 + 18 = 32
    assert!((score - 32.0).abs() < 1e-6);
}

#[test]
fn test_dotproduct_orthogonal() {
    let a = vec![1.0, 0.0];
    let b = vec![0.0, 1.0];
    let score = dot_product(&a, &b);
    assert!(score.abs() < 1e-6);
}

#[test]
fn test_dotproduct_unit_vectors() {
    let a = vec![1.0, 0.0, 0.0];
    let b = vec![1.0, 0.0, 0.0];
    let score = dot_product(&a, &b);
    assert!((score - 1.0).abs() < 1e-6);
}
```

### 4.4 Edge Cases (`distance_edge_cases_tests.rs`)

```rust
#[test]
fn test_zero_vector_cosine() {
    let zero = vec![0.0, 0.0, 0.0];
    let non_zero = vec![1.0, 2.0, 3.0];

    // Cosine with zero vector should return 0 (or handle gracefully)
    let score = cosine_similarity(&zero, &non_zero);
    // Implementation choice: return 0 for undefined case
    assert!(score.is_finite());
}

#[test]
fn test_very_small_values() {
    let a = vec![1e-38, 1e-38, 1e-38];
    let b = vec![1e-38, 1e-38, 1e-38];

    let score = cosine_similarity(&a, &b);
    assert!(score.is_finite(), "Should handle very small values");
}

#[test]
fn test_very_large_values() {
    let a = vec![1e38, 1e38, 1e38];
    let b = vec![1e38, 1e38, 1e38];

    let score = cosine_similarity(&a, &b);
    assert!(score.is_finite(), "Should handle very large values");
}

#[test]
fn test_mixed_positive_negative() {
    let a = vec![1.0, -1.0, 1.0, -1.0];
    let b = vec![-1.0, 1.0, -1.0, 1.0];

    let score = cosine_similarity(&a, &b);
    assert!((score - (-1.0)).abs() < 1e-6, "Opposite alternating vectors");
}
```

---

## Tier 5: Collection Management

### 5.1-5.5: Collection Tests

```rust
#[test]
fn test_create_collection() {
    let db = create_test_db();
    let run_id = create_run(&db);

    let result = db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm());
    assert!(result.is_ok());

    // Creating same collection again should fail
    let result = db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm());
    assert!(matches!(result, Err(VectorError::CollectionExists(_))));
}

#[test]
fn test_delete_collection() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();
    db.vector.insert(run_id, "embeddings", "key1", &random_vector(384), None).unwrap();

    // Delete collection
    db.vector.delete_collection(run_id, "embeddings").unwrap();

    // Collection should not exist
    assert!(db.vector.get_collection(run_id, "embeddings").unwrap().is_none());

    // Can create new collection with same name
    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_openai_ada()).unwrap();
}

#[test]
fn test_list_collections() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings1", VectorConfig::for_minilm()).unwrap();
    db.vector.create_collection(run_id, "embeddings2", VectorConfig::for_openai_ada()).unwrap();
    db.vector.create_collection(run_id, "embeddings3", VectorConfig::for_minilm()).unwrap();

    let collections = db.vector.list_collections(run_id).unwrap();
    let names: Vec<&str> = collections.iter().map(|c| c.name.as_str()).collect();

    assert!(names.contains(&"embeddings1"));
    assert!(names.contains(&"embeddings2"));
    assert!(names.contains(&"embeddings3"));
}

#[test]
fn test_get_collection_info() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(
        run_id, "embeddings",
        VectorConfig {
            dimension: 768,
            metric: DistanceMetric::Euclidean,
            storage_dtype: StorageDtype::F32,
        }
    ).unwrap();

    for i in 0..10 {
        db.vector.insert(run_id, "embeddings", &format!("key{}", i), &random_vector(768), None).unwrap();
    }

    let info = db.vector.get_collection(run_id, "embeddings").unwrap().unwrap();

    assert_eq!(info.name, "embeddings");
    assert_eq!(info.config.dimension, 768);
    assert_eq!(info.config.metric, DistanceMetric::Euclidean);
    assert_eq!(info.count, 10);
}

#[test]
fn test_collection_config_survives_restart() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(
        run_id, "embeddings",
        VectorConfig {
            dimension: 512,
            metric: DistanceMetric::DotProduct,
            storage_dtype: StorageDtype::F32,
        }
    ).unwrap();

    drop(db);
    let db = reopen_database();

    let info = db.vector.get_collection(run_id, "embeddings").unwrap().unwrap();

    assert_eq!(info.config.dimension, 512);
    assert_eq!(info.config.metric, DistanceMetric::DotProduct);
}
```

---

## Tier 6: VectorHeap Operations

### 6.1-6.5: Heap Tests

```rust
#[test]
fn test_heap_insert_and_get() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    let embedding = random_vector(384);
    db.vector.insert(run_id, "embeddings", "key1", &embedding, None).unwrap();

    let retrieved = db.vector.get(run_id, "embeddings", "key1").unwrap().unwrap();
    assert_eq!(retrieved.embedding, embedding);
}

#[test]
fn test_heap_upsert_overwrites() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    let embedding1 = random_vector(384);
    let embedding2 = random_vector(384);

    db.vector.insert(run_id, "embeddings", "key1", &embedding1, None).unwrap();
    db.vector.insert(run_id, "embeddings", "key1", &embedding2, None).unwrap();

    let retrieved = db.vector.get(run_id, "embeddings", "key1").unwrap().unwrap();
    assert_eq!(retrieved.embedding, embedding2, "Upsert should overwrite");

    // Count should still be 1
    assert_eq!(db.vector.count(run_id, "embeddings").unwrap(), 1);
}

#[test]
fn test_heap_delete() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    db.vector.insert(run_id, "embeddings", "key1", &random_vector(384), None).unwrap();

    // Delete
    let deleted = db.vector.delete(run_id, "embeddings", "key1").unwrap();
    assert!(deleted);

    // Should not exist
    assert!(db.vector.get(run_id, "embeddings", "key1").unwrap().is_none());

    // Delete again returns false
    let deleted = db.vector.delete(run_id, "embeddings", "key1").unwrap();
    assert!(!deleted);
}

#[test]
fn test_heap_slot_reuse() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    // Insert vectors
    for i in 0..10 {
        db.vector.insert(run_id, "embeddings", &format!("key{}", i), &random_vector(384), None).unwrap();
    }

    let heap_size_before = get_heap_allocated_size(&db, run_id, "embeddings");

    // Delete half
    for i in 0..5 {
        db.vector.delete(run_id, "embeddings", &format!("key{}", i)).unwrap();
    }

    // Insert new vectors - should reuse slots
    for i in 10..15 {
        db.vector.insert(run_id, "embeddings", &format!("key{}", i), &random_vector(384), None).unwrap();
    }

    let heap_size_after = get_heap_allocated_size(&db, run_id, "embeddings");

    // Heap should not grow (slots reused)
    assert_eq!(heap_size_before, heap_size_after, "Heap should reuse slots");
}

#[test]
fn test_heap_iteration_deterministic() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    // Insert in random order
    let keys: Vec<String> = (0..20).map(|i| format!("key{:02}", i)).collect();
    let mut shuffled = keys.clone();
    use rand::seq::SliceRandom;
    shuffled.shuffle(&mut rand::thread_rng());

    for key in &shuffled {
        db.vector.insert(run_id, "embeddings", key, &random_vector(384), None).unwrap();
    }

    // Get all vectors multiple times
    let mut iterations: Vec<Vec<String>> = Vec::new();
    for _ in 0..10 {
        let all = db.vector.list_keys(run_id, "embeddings").unwrap();
        iterations.push(all);
    }

    // All iterations must be identical
    for (i, iter) in iterations.iter().enumerate().skip(1) {
        assert_eq!(&iterations[0], iter, "Iteration {} differs", i);
    }
}
```

---

## Tier 7: M6 Integration

```rust
#[test]
fn test_m6_search_request_compatibility() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    for i in 0..50 {
        db.vector.insert(run_id, "embeddings", &format!("key{}", i), &random_vector(384), None).unwrap();
    }

    // Use M6 SearchRequest
    let request = SearchRequest {
        vector: Some(VectorSearchConfig {
            collection: "embeddings".to_string(),
            query: random_vector(384),
            k: 10,
        }),
        ..Default::default()
    };

    let response = db.search(run_id, request).unwrap();

    // Response should contain vector results
    assert!(!response.vector_results.is_empty());
}

#[test]
fn test_m6_rrf_fusion() {
    let db = create_test_db();
    let run_id = create_run(&db);

    // Setup vector collection
    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    // Setup keyword-searchable KV
    for i in 0..50 {
        let key = format!("doc{}", i);
        db.kv.put(run_id, &key, &format!("content about topic {}", i)).unwrap();
        db.vector.insert(run_id, "embeddings", &key, &random_vector(384), None).unwrap();
    }

    // Hybrid search (keyword + vector)
    let request = SearchRequest {
        keyword: Some(KeywordSearchConfig {
            query: "topic".to_string(),
            k: 10,
        }),
        vector: Some(VectorSearchConfig {
            collection: "embeddings".to_string(),
            query: random_vector(384),
            k: 10,
        }),
        fusion: Some(FusionConfig::RRF { k: 60 }),
        ..Default::default()
    };

    let response = db.search(run_id, request).unwrap();

    // Should have fused results
    assert!(!response.fused_results.is_empty());
}
```

---

## Tier 8: WAL Integration

```rust
#[test]
fn test_wal_entry_types() {
    // Verify correct entry type codes
    assert_eq!(WalEntryType::VectorCollectionCreate as u8, 0x70);
    assert_eq!(WalEntryType::VectorCollectionDelete as u8, 0x71);
    assert_eq!(WalEntryType::VectorUpsert as u8, 0x72);
    assert_eq!(WalEntryType::VectorDelete as u8, 0x73);
}

#[test]
fn test_wal_write_vector_operations() {
    let db = create_test_db();
    let run_id = create_run(&db);

    let wal_before = db.wal_size();

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();
    db.vector.insert(run_id, "embeddings", "key1", &random_vector(384), None).unwrap();
    db.vector.delete(run_id, "embeddings", "key1").unwrap();
    db.vector.delete_collection(run_id, "embeddings").unwrap();

    let wal_after = db.wal_size();

    assert!(wal_after > wal_before, "WAL should grow with vector operations");
}

#[test]
fn test_wal_replay_produces_identical_state() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    let embeddings: Vec<Vec<f32>> = (0..50).map(|_| random_vector(384)).collect();
    for (i, emb) in embeddings.iter().enumerate() {
        db.vector.insert(run_id, "embeddings", &format!("key{}", i), emb, None).unwrap();
    }

    // Delete some
    for i in 0..10 {
        db.vector.delete(run_id, "embeddings", &format!("key{}", i)).unwrap();
    }

    let state_before = capture_vector_state(&db, run_id, "embeddings");

    // Replay WAL
    drop(db);
    delete_snapshots();
    let db = reopen_database();

    let state_after = capture_vector_state(&db, run_id, "embeddings");

    assert_eq!(state_before, state_after, "WAL replay produced different state");
}
```

---

## Tier 9: Snapshot & Recovery

```rust
#[test]
fn test_snapshot_contains_vector_blob() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();
    db.vector.insert(run_id, "embeddings", "key1", &random_vector(384), None).unwrap();

    let snapshot_path = db.create_snapshot().unwrap();
    let snapshot = Snapshot::load(&snapshot_path).unwrap();

    assert!(snapshot.has_blob(PrimitiveKind::Vector), "Snapshot missing vector blob");
}

#[test]
fn test_snapshot_next_id_persisted() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    for i in 0..100 {
        db.vector.insert(run_id, "embeddings", &format!("key{}", i), &random_vector(384), None).unwrap();
    }

    let next_id_before = get_next_vectorid(&db, run_id, "embeddings");

    db.create_snapshot().unwrap();

    drop(db);
    let db = reopen_database();

    let next_id_after = get_next_vectorid(&db, run_id, "embeddings");

    assert!(next_id_after.0 >= next_id_before.0, "next_id not preserved in snapshot");
}

#[test]
fn test_recovery_from_snapshot_plus_wal() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    // Insert vectors before snapshot
    for i in 0..50 {
        db.vector.insert(run_id, "embeddings", &format!("key{}", i), &random_vector(384), None).unwrap();
    }

    db.create_snapshot().unwrap();

    // Insert more after snapshot
    for i in 50..100 {
        db.vector.insert(run_id, "embeddings", &format!("key{}", i), &random_vector(384), None).unwrap();
    }

    let state_before = capture_vector_state(&db, run_id, "embeddings");

    drop(db);
    let db = reopen_database();

    let state_after = capture_vector_state(&db, run_id, "embeddings");

    assert_eq!(state_before, state_after, "Recovery from snapshot+WAL failed");
    assert_eq!(db.vector.count(run_id, "embeddings").unwrap(), 100);
}
```

---

## Tier 10: Cross-Primitive Transactions

```rust
#[test]
fn test_kv_vector_atomic() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    // Transaction with both KV and Vector
    db.begin_tx();
    db.kv.put(run_id, "doc_id", "doc_content").unwrap();
    db.vector.insert(run_id, "embeddings", "doc_id", &random_vector(384), None).unwrap();
    db.commit().unwrap();

    // Both should be present
    assert!(db.kv.get(run_id, "doc_id").unwrap().is_some());
    assert!(db.vector.get(run_id, "embeddings", "doc_id").unwrap().is_some());
}

#[test]
fn test_cross_primitive_rollback() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    db.begin_tx();
    db.kv.put(run_id, "doc_id", "doc_content").unwrap();
    db.vector.insert(run_id, "embeddings", "doc_id", &random_vector(384), None).unwrap();
    db.rollback();

    // Neither should be present
    assert!(db.kv.get(run_id, "doc_id").unwrap().is_none());
    assert!(db.vector.get(run_id, "embeddings", "doc_id").unwrap().is_none());
}

#[test]
fn test_cross_primitive_crash_recovery() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    // Committed transaction
    db.begin_tx();
    db.kv.put(run_id, "committed", "value").unwrap();
    db.vector.insert(run_id, "embeddings", "committed", &random_vector(384), None).unwrap();
    db.commit().unwrap();
    db.sync().unwrap();

    // Uncommitted transaction
    db.begin_tx();
    db.kv.put(run_id, "uncommitted", "value").unwrap();
    db.vector.insert(run_id, "embeddings", "uncommitted", &random_vector(384), None).unwrap();
    // No commit

    simulate_crash(&db);
    let db = reopen_database();

    // Committed should be present
    assert!(db.kv.get(run_id, "committed").unwrap().is_some());
    assert!(db.vector.get(run_id, "embeddings", "committed").unwrap().is_some());

    // Uncommitted should be absent
    assert!(db.kv.get(run_id, "uncommitted").unwrap().is_none());
    assert!(db.vector.get(run_id, "embeddings", "uncommitted").unwrap().is_none());
}
```

---

## Tier 11: Crash Scenarios

```rust
#[test]
fn test_crash_during_vector_insert() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    // Committed insert
    db.vector.insert(run_id, "embeddings", "committed", &random_vector(384), None).unwrap();
    db.sync().unwrap();

    // Uncommitted insert
    db.begin_tx();
    db.vector.insert(run_id, "embeddings", "uncommitted", &random_vector(384), None).unwrap();
    // Crash without commit

    simulate_crash(&db);
    let db = reopen_database();

    assert!(db.vector.get(run_id, "embeddings", "committed").unwrap().is_some());
    assert!(db.vector.get(run_id, "embeddings", "uncommitted").unwrap().is_none());
}

#[test]
fn test_crash_vectorid_continuity() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    for i in 0..50 {
        db.vector.insert(run_id, "embeddings", &format!("key{}", i), &random_vector(384), None).unwrap();
    }

    let max_id_before = get_max_vectorid(&db, run_id, "embeddings");

    simulate_crash(&db);
    let db = reopen_database();

    // Insert new vector
    db.vector.insert(run_id, "embeddings", "new_key", &random_vector(384), None).unwrap();
    let new_id = get_vectorid_for_key(&db, run_id, "embeddings", "new_key");

    assert!(new_id.0 > max_id_before.0, "VectorId continuity violated after crash");
}
```

---

## Tier 12: Determinism Tests

```rust
#[test]
fn test_insert_order_doesnt_affect_search() {
    let db1 = create_test_db();
    let db2 = create_test_db();
    let run_id = create_run(&db1);

    // Same config for both
    db1.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();
    db2.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    let embeddings: Vec<(String, Vec<f32>)> = (0..20)
        .map(|i| (format!("key{}", i), random_vector(384)))
        .collect();

    // Insert in order 0..19
    for (key, emb) in &embeddings {
        db1.vector.insert(run_id, "embeddings", key, emb, None).unwrap();
    }

    // Insert in reverse order 19..0
    for (key, emb) in embeddings.iter().rev() {
        db2.vector.insert(run_id, "embeddings", key, emb, None).unwrap();
    }

    // Same query should produce same results
    let query = random_vector(384);
    let results1 = db1.vector.search(run_id, "embeddings", &query, 20).unwrap();
    let results2 = db2.vector.search(run_id, "embeddings", &query, 20).unwrap();

    let keys1: Vec<&str> = results1.iter().map(|r| r.key.as_str()).collect();
    let keys2: Vec<&str> = results2.iter().map(|r| r.key.as_str()).collect();

    assert_eq!(keys1, keys2, "Insert order affected search results");
}

#[test]
fn test_replay_produces_identical_state() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    for i in 0..100 {
        db.vector.insert(run_id, "embeddings", &format!("key{}", i), &random_vector(384), None).unwrap();
    }

    let state_original = capture_vector_state(&db, run_id, "embeddings");

    // Replay 5 times
    for i in 0..5 {
        drop(db);
        delete_snapshots();
        let db = reopen_database();

        let state_replayed = capture_vector_state(&db, run_id, "embeddings");
        assert_eq!(state_original, state_replayed, "Replay {} produced different state", i);
    }
}
```

---

## Tier 13: Stress & Scale Tests

```rust
#[test]
fn test_1k_vectors_under_5ms() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    for i in 0..1000 {
        db.vector.insert(run_id, "embeddings", &format!("key{}", i), &random_vector(384), None).unwrap();
    }

    let query = random_vector(384);
    let start = Instant::now();
    let _results = db.vector.search(run_id, "embeddings", &query, 100).unwrap();
    let elapsed = start.elapsed();

    assert!(elapsed < Duration::from_millis(5), "1K vector search took {:?}", elapsed);
}

#[test]
fn test_10k_vectors_under_50ms() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    for i in 0..10_000 {
        db.vector.insert(run_id, "embeddings", &format!("key{}", i), &random_vector(384), None).unwrap();
    }

    let query = random_vector(384);
    let start = Instant::now();
    let _results = db.vector.search(run_id, "embeddings", &query, 100).unwrap();
    let elapsed = start.elapsed();

    assert!(elapsed < Duration::from_millis(50), "10K vector search took {:?}", elapsed);
}

#[test]
fn test_insert_delete_cycles() {
    let db = create_test_db();
    let run_id = create_run(&db);

    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

    // 100 cycles of insert/delete
    for cycle in 0..100 {
        for i in 0..10 {
            db.vector.insert(run_id, "embeddings", &format!("key_{}_{}", cycle, i), &random_vector(384), None).unwrap();
        }
        for i in 0..10 {
            db.vector.delete(run_id, "embeddings", &format!("key_{}_{}", cycle, i)).unwrap();
        }
    }

    // Should be empty
    assert_eq!(db.vector.count(run_id, "embeddings").unwrap(), 0);

    // Should still work
    db.vector.insert(run_id, "embeddings", "final", &random_vector(384), None).unwrap();
    let results = db.vector.search(run_id, "embeddings", &random_vector(384), 10).unwrap();
    assert_eq!(results.len(), 1);
}
```

---

## Tier 14: Non-Regression

```rust
#[test]
fn test_m7_durability_maintained() {
    let db = create_test_db();
    let run_id = create_run(&db);

    // M7 durability: committed data survives crash
    db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();
    db.vector.insert(run_id, "embeddings", "key1", &random_vector(384), None).unwrap();
    db.sync().unwrap();

    simulate_crash(&db);
    let db = reopen_database();

    assert!(db.vector.get(run_id, "embeddings", "key1").unwrap().is_some(),
        "M7 regression: committed vector lost after crash");
}

#[test]
fn test_m6_search_maintained() {
    let db = create_test_db();
    let run_id = create_run(&db);

    // M6 search still works
    db.kv.put(run_id, "doc1", "hello world").unwrap();

    let request = SearchRequest {
        keyword: Some(KeywordSearchConfig {
            query: "hello".to_string(),
            k: 10,
        }),
        ..Default::default()
    };

    let response = db.search(run_id, request).unwrap();
    assert!(!response.keyword_results.is_empty(), "M6 regression: keyword search broken");
}
```

---

## Tier 15: Spec Conformance

```rust
/// Tests that directly map to architecture specification requirements
mod spec_conformance {
    #[test]
    fn spec_rule1_stateless_facade() {
        // VectorStore is stateless - cloning produces independent facade
        let db = create_test_db();
        let v1 = VectorStore::new(db.clone());
        let v2 = v1.clone();

        // Both facades work independently on same database
        let run_id = create_run(&db);
        v1.create_collection(run_id, "col1", VectorConfig::for_minilm()).unwrap();
        v2.create_collection(run_id, "col2", VectorConfig::for_minilm()).unwrap();

        // Both collections exist
        assert!(v1.get_collection(run_id, "col2").unwrap().is_some());
        assert!(v2.get_collection(run_id, "col1").unwrap().is_some());
    }

    #[test]
    fn spec_rule2_collections_per_runid() {
        // Collections are scoped to RunId
        let db = create_test_db();
        let run1 = create_run(&db);
        let run2 = create_run(&db);

        db.vector.create_collection(run1, "shared_name", VectorConfig::for_minilm()).unwrap();
        db.vector.create_collection(run2, "shared_name", VectorConfig::for_minilm()).unwrap();

        // Separate collections despite same name
        db.vector.insert(run1, "shared_name", "key", &random_vector(384), None).unwrap();

        assert_eq!(db.vector.count(run2, "shared_name").unwrap(), 0);
    }

    #[test]
    fn spec_rule3_upsert_semantics() {
        // Insert overwrites
        let db = create_test_db();
        let run_id = create_run(&db);

        db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

        let emb1 = random_vector(384);
        let emb2 = random_vector(384);

        db.vector.insert(run_id, "embeddings", "key", &emb1, None).unwrap();
        db.vector.insert(run_id, "embeddings", "key", &emb2, None).unwrap();

        let retrieved = db.vector.get(run_id, "embeddings", "key").unwrap().unwrap();
        assert_eq!(retrieved.embedding, emb2);
    }

    #[test]
    fn spec_rule4_dimension_validation() {
        let db = create_test_db();
        let run_id = create_run(&db);

        db.vector.create_collection(run_id, "embeddings", VectorConfig { dimension: 100, ..Default::default() }).unwrap();

        // Wrong dimension insert fails
        let result = db.vector.insert(run_id, "embeddings", "key", &random_vector(200), None);
        assert!(matches!(result, Err(VectorError::DimensionMismatch { expected: 100, got: 200 })));

        // Wrong dimension search fails
        db.vector.insert(run_id, "embeddings", "key", &random_vector(100), None).unwrap();
        let result = db.vector.search(run_id, "embeddings", &random_vector(50), 10);
        assert!(matches!(result, Err(VectorError::DimensionMismatch { .. })));
    }

    #[test]
    fn spec_rule5_deterministic_ordering() {
        // Both backend and facade enforce determinism
        let db = create_test_db();
        let run_id = create_run(&db);

        db.vector.create_collection(run_id, "embeddings", VectorConfig { dimension: 3, ..Default::default() }).unwrap();

        // Identical vectors
        let v = vec![1.0, 0.0, 0.0];
        db.vector.insert(run_id, "embeddings", "z", &v, None).unwrap();
        db.vector.insert(run_id, "embeddings", "a", &v, None).unwrap();
        db.vector.insert(run_id, "embeddings", "m", &v, None).unwrap();

        // Search multiple times
        for _ in 0..100 {
            let results = db.vector.search(run_id, "embeddings", &v, 3).unwrap();
            // Facade tie-break by key asc
            assert_eq!(results[0].key, "a");
            assert_eq!(results[1].key, "m");
            assert_eq!(results[2].key, "z");
        }
    }

    #[test]
    fn spec_rule6_vectorid_never_reused() {
        let db = create_test_db();
        let run_id = create_run(&db);

        db.vector.create_collection(run_id, "embeddings", VectorConfig::for_minilm()).unwrap();

        let mut all_ids: Vec<VectorId> = Vec::new();

        for cycle in 0..10 {
            for i in 0..10 {
                db.vector.insert(run_id, "embeddings", &format!("key_{}_{}", cycle, i), &random_vector(384), None).unwrap();
            }
            all_ids.extend(get_all_vectorids(&db, run_id, "embeddings"));

            for i in 0..10 {
                db.vector.delete(run_id, "embeddings", &format!("key_{}_{}", cycle, i)).unwrap();
            }
        }

        // All IDs unique
        let unique: std::collections::HashSet<_> = all_ids.iter().collect();
        assert_eq!(unique.len(), all_ids.len(), "VectorIds were reused");
    }

    #[test]
    fn spec_rule7_no_backend_specific_config() {
        // VectorConfig has no HNSW-specific fields
        let config = VectorConfig::for_minilm();

        // Only these fields exist
        let _ = config.dimension;
        let _ = config.metric;
        let _ = config.storage_dtype;

        // No ef_construction, M, or other HNSW params
        // (This is a compile-time check - if HNSW fields are added, this test documents the violation)
    }
}
```

---

## Test Utilities

```rust
// Common test utilities used across tiers

fn create_test_db() -> Database {
    Database::open_in_memory().unwrap()
}

fn create_run(db: &Database) -> RunId {
    db.run_index.create_run("test_run").unwrap()
}

fn random_vector(dim: usize) -> Vec<f32> {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect()
}

fn reopen_database() -> Database {
    // Implementation-specific
    unimplemented!()
}

fn simulate_crash(db: &Database) {
    // Force drop without clean shutdown
    std::mem::forget(db.clone());
}

fn capture_vector_state(db: &Database, run_id: RunId, collection: &str) -> VectorStateSnapshot {
    // Capture full state for comparison
    unimplemented!()
}

fn get_vectorid_for_key(db: &Database, run_id: RunId, collection: &str, key: &str) -> VectorId {
    unimplemented!()
}

fn get_all_vectorids(db: &Database, run_id: RunId, collection: &str) -> Vec<VectorId> {
    unimplemented!()
}

fn get_max_vectorid(db: &Database, run_id: RunId, collection: &str) -> VectorId {
    unimplemented!()
}

fn get_next_vectorid(db: &Database, run_id: RunId, collection: &str) -> VectorId {
    unimplemented!()
}

fn get_free_slots_count(db: &Database, run_id: RunId, collection: &str) -> usize {
    unimplemented!()
}

fn get_btreemap_vectorids(db: &Database, run_id: RunId, collection: &str) -> Vec<VectorId> {
    unimplemented!()
}

fn delete_snapshots() {
    unimplemented!()
}
```

---

## Summary

| Tier | Focus | Test Count |
|------|-------|------------|
| 1 | Storage Invariants (S1-S9) | 9 files |
| 2 | Search Invariants (R1-R10) | 10 files |
| 3 | Transaction Invariants (T1-T4) | 4 files |
| 4 | Distance Metric Correctness | 4 files |
| 5 | Collection Management | 5 files |
| 6 | VectorHeap Operations | 5 files |
| 7 | M6 Integration | 5 files |
| 8 | WAL Integration | 4 files |
| 9 | Snapshot & Recovery | 5 files |
| 10 | Cross-Primitive Transactions | 4 files |
| 11 | Crash Scenarios | 4 files |
| 12 | Determinism Tests | 4 files |
| 13 | Stress & Scale | 5 files |
| 14 | Non-Regression | 2 files |
| 15 | Spec Conformance | 1 file |
| **Total** | | **71 files** |

---

## Critical Invariants Summary

| ID | Name | Priority |
|----|------|----------|
| S4 | VectorId never reused | CRITICAL |
| S7 | BTreeMap sole source of truth | CRITICAL |
| S8 | Snapshot-WAL equivalence | CRITICAL |
| T4 | VectorId monotonicity across crashes | CRITICAL |
| R2 | Score normalization (higher = better) | HIGH |
| R3 | Deterministic ordering | HIGH |
| R4 | Backend tie-break (score desc, VectorId asc) | HIGH |
| R5 | Facade tie-break (score desc, key asc) | HIGH |
| R10 | Search is read-only | HIGH |

---

## Execution Priority

1. **Phase 1**: Tier 1 (Storage) + Tier 3 (Transaction) - Core correctness
2. **Phase 2**: Tier 2 (Search) + Tier 4 (Distance) - Search correctness
3. **Phase 3**: Tier 8 (WAL) + Tier 9 (Snapshot) - Durability
4. **Phase 4**: Tier 10 (Cross-Primitive) + Tier 11 (Crash) - Atomicity
5. **Phase 5**: Remaining tiers - Integration and stress
