# M7 Comprehensive Test Plan

**Version**: 1.0
**Status**: Planning
**Date**: 2026-01-17

---

## Overview

This document defines the comprehensive test suite for M7 Durability, Snapshots, Replay & Storage Stabilization, **separate from the unit and integration tests written during development**.

The goal is to create a battery of tests that:
1. **Lock in recovery invariants (R1-R6)** - Deterministic, idempotent, prefix-consistent recovery
2. **Lock in replay invariants (P1-P6)** - Pure function, side-effect free, derived view
3. **Validate crash recovery** - Database survives any single crash scenario
4. **Verify snapshot correctness** - Snapshots are valid caches over WAL history
5. **Test WAL integrity** - CRC32 validation catches corruption
6. **Ensure atomic transactions** - Cross-primitive operations are all-or-nothing
7. **Test run lifecycle** - Begin, end, orphan detection work correctly
8. **Validate storage extensibility** - PrimitiveStorageExt enables new primitives
9. **Prevent regressions** - M6 performance and semantics are maintained

---

## Test Structure

```
tests/
└── m7_comprehensive/
    ├── main.rs                           # Test harness and utilities
    │
    │   # Tier 1: Recovery Invariants (HIGHEST PRIORITY)
    ├── recovery_determinism_tests.rs     # 1.1 R1: Same WAL → same state
    ├── recovery_idempotent_tests.rs      # 1.2 R2: Replay is idempotent
    ├── recovery_prefix_tests.rs          # 1.3 R3: Prefix-consistent recovery
    ├── recovery_no_invent_tests.rs       # 1.4 R4: Never invents data
    ├── recovery_no_drop_committed.rs     # 1.5 R5: Never drops committed
    ├── recovery_may_drop_uncommitted.rs  # 1.6 R6: May drop uncommitted
    │
    │   # Tier 2: Replay Invariants
    ├── replay_pure_function_tests.rs     # 2.1 P1: Pure function
    ├── replay_side_effect_tests.rs       # 2.2 P2: Side-effect free
    ├── replay_derived_view_tests.rs      # 2.3 P3: Derived view, not state
    ├── replay_ephemeral_tests.rs         # 2.4 P4: Does not persist
    ├── replay_determinism_tests.rs       # 2.5 P5: Deterministic
    ├── replay_idempotent_tests.rs        # 2.6 P6: Idempotent
    │
    │   # Tier 3: Snapshot System
    ├── snapshot_format_tests.rs          # 3.1 Envelope format validation
    ├── snapshot_crc_tests.rs             # 3.2 CRC32 integrity
    ├── snapshot_atomic_write_tests.rs    # 3.3 Atomic write protocol
    ├── snapshot_discovery_tests.rs       # 3.4 Discovery and ordering
    ├── snapshot_fallback_tests.rs        # 3.5 Corrupt snapshot fallback
    │
    │   # Tier 4: WAL System
    ├── wal_entry_format_tests.rs         # 4.1 Entry envelope format
    ├── wal_crc_validation_tests.rs       # 4.2 CRC32 catches corruption
    ├── wal_transaction_framing_tests.rs  # 4.3 TxBegin/TxCommit framing
    ├── wal_entry_type_tests.rs           # 4.4 Type registry correctness
    ├── wal_truncation_tests.rs           # 4.5 Truncation to valid boundary
    │
    │   # Tier 5: Crash Scenarios
    ├── crash_during_wal_write_tests.rs   # 5.1 Partial WAL entry
    ├── crash_during_commit_tests.rs      # 5.2 Between commit and fsync
    ├── crash_during_snapshot_tests.rs    # 5.3 Partial snapshot write
    ├── crash_multi_primitive_tests.rs    # 5.4 Mid-transaction crash
    ├── crash_recovery_sequence_tests.rs  # 5.5 Full recovery sequence
    │
    │   # Tier 6: Cross-Primitive Atomicity
    ├── atomic_multi_write_tests.rs       # 6.1 All-or-nothing commits
    ├── atomic_recovery_boundary_tests.rs # 6.2 Transaction boundaries
    ├── atomic_cross_primitive_tests.rs   # 6.3 Atomicity across KV+JSON+Event
    │
    │   # Tier 7: Run Lifecycle
    ├── run_begin_end_tests.rs            # 7.1 begin_run/end_run
    ├── run_status_transitions_tests.rs   # 7.2 Status state machine
    ├── run_orphan_detection_tests.rs     # 7.3 Orphaned run detection
    ├── run_replay_tests.rs               # 7.4 replay_run() functionality
    ├── run_diff_tests.rs                 # 7.5 diff_runs() functionality
    │
    │   # Tier 8: Storage Stabilization
    ├── primitive_storage_ext_tests.rs    # 8.1 Trait implementation
    ├── primitive_registry_tests.rs       # 8.2 Registry operations
    ├── storage_extension_tests.rs        # 8.3 Adding new primitives
    ├── wal_type_allocation_tests.rs      # 8.4 Type tag allocation
    │
    │   # Tier 9: Property-Based/Fuzzing
    ├── recovery_fuzzing_tests.rs         # 9.1 Random crash scenarios
    ├── wal_fuzzing_tests.rs              # 9.2 Random WAL corruption
    ├── snapshot_fuzzing_tests.rs         # 9.3 Random snapshot corruption
    │
    │   # Tier 10: Stress & Scale
    ├── recovery_large_wal_tests.rs       # 10.1 Large WAL recovery
    ├── snapshot_large_state_tests.rs     # 10.2 Large snapshot handling
    ├── concurrent_recovery_tests.rs      # 10.3 Concurrent operations
    │
    │   # Tier 11: Non-Regression
    ├── m6_regression_tests.rs            # 11.1 M6 targets maintained
    ├── operation_latency_tests.rs        # 11.2 Write latency with WAL
    │
    │   # Tier 12: Spec Conformance
    └── spec_conformance_tests.rs         # 12. Direct spec-to-test mapping
```

---

## Tier 1: Recovery Invariants (HIGHEST PRIORITY)

These tests ensure the **sacred recovery guarantees** are never violated.
They directly correspond to the six recovery invariants (R1-R6).

### 1.1 R1: Deterministic Recovery (`recovery_determinism_tests.rs`)

**Invariant R1**: Same WAL → same state every replay.

```rust
#[test]
fn test_r1_same_wal_same_state() {
    // Given: A sequence of operations
    let ops = vec![
        KvPut("k1", "v1"),
        JsonSet("doc1", json!({"a": 1})),
        EventAppend("log1", Event::new("test")),
    ];

    // Execute and capture WAL
    let wal = execute_and_capture_wal(&ops);

    // Replay 100 times
    let mut states = Vec::new();
    for _ in 0..100 {
        let state = replay_wal(&wal);
        states.push(hash_state(&state));
    }

    // ALL hashes must be identical
    assert!(states.windows(2).all(|w| w[0] == w[1]),
        "R1 VIOLATED: Same WAL produced different states");
}

#[test]
fn test_r1_deterministic_across_restarts() {
    let db = create_test_db();

    // Write operations
    db.kv.put(&run_id, "key1", "value1").unwrap();
    db.json.create(&run_id, json!({"x": 1})).unwrap();

    let state_before = capture_state(&db);

    // Simulate crash and restart
    drop(db);
    let db = reopen_database();

    let state_after = capture_state(&db);

    assert_eq!(state_before, state_after,
        "R1 VIOLATED: State differs after restart");
}

#[test]
fn test_r1_deterministic_with_all_primitives() {
    // Operations across all primitives must be deterministic
    let ops = vec![
        Op::KvPut("k1", "v1"),
        Op::JsonCreate(json!({"a": 1})),
        Op::EventAppend("log", event),
        Op::StateInit("cell", 0),
        Op::TraceRecord(span),
        Op::RunBegin(metadata),
    ];

    let wal = execute_and_capture_wal(&ops);

    let state1 = replay_wal(&wal);
    let state2 = replay_wal(&wal);

    assert_eq!(hash_state(&state1), hash_state(&state2));
}

#[test]
fn test_r1_ordering_preserved() {
    // Operations must replay in WAL order
    let ops = vec![
        KvPut("key", "v1"),
        KvPut("key", "v2"),
        KvPut("key", "v3"),
    ];

    let wal = execute_and_capture_wal(&ops);
    let state = replay_wal(&wal);

    // Final value must be "v3" (last write wins)
    assert_eq!(state.kv.get("key"), Some("v3"));
}
```

### 1.2 R2: Idempotent Recovery (`recovery_idempotent_tests.rs`)

**Invariant R2**: replay(replay(S, WAL), WAL) = replay(S, WAL).

```rust
#[test]
fn test_r2_replay_idempotent() {
    let ops = vec![
        KvPut("k1", "v1"),
        JsonSet("doc1", json!({"a": 1})),
    ];

    let wal = execute_and_capture_wal(&ops);

    // Single replay
    let state1 = replay_wal(&wal);

    // Double replay (replay same WAL onto result)
    let state2 = replay_wal_onto(&state1, &wal);

    assert_eq!(hash_state(&state1), hash_state(&state2),
        "R2 VIOLATED: Double replay changed state");
}

#[test]
fn test_r2_multiple_recovery_attempts() {
    let db = create_test_db();
    populate_test_data(&db);

    let original_state = capture_state(&db);

    // Simulate multiple crash/recovery cycles
    for i in 0..10 {
        drop(db);
        let db = reopen_database();

        let recovered_state = capture_state(&db);
        assert_eq!(original_state, recovered_state,
            "R2 VIOLATED: Recovery attempt {} changed state", i);
    }
}

#[test]
fn test_r2_replay_from_snapshot_idempotent() {
    let db = create_test_db();
    populate_test_data(&db);

    // Create snapshot
    db.create_snapshot().unwrap();

    let state_before = capture_state(&db);

    // Simulate recovery from snapshot multiple times
    for _ in 0..5 {
        drop(db);
        let db = reopen_database(); // Uses snapshot

        let state_after = capture_state(&db);
        assert_eq!(state_before, state_after);
    }
}
```

### 1.3 R3: Prefix-Consistent Recovery (`recovery_prefix_tests.rs`)

**Invariant R3**: Recover prefix of committed transactions.

```rust
#[test]
fn test_r3_recovers_committed_prefix() {
    let db = create_test_db();

    // Commit 10 transactions
    for i in 0..10 {
        db.begin_tx();
        db.kv.put(&run_id, &format!("key_{}", i), "value").unwrap();
        db.commit().unwrap();
    }

    // Simulate crash
    let committed_count = db.committed_tx_count();
    drop(db);

    // Recover
    let db = reopen_database();

    // Must see all committed transactions
    for i in 0..committed_count {
        assert!(db.kv.get(&run_id, &format!("key_{}", i)).is_some(),
            "R3 VIOLATED: Committed tx {} missing after recovery", i);
    }
}

#[test]
fn test_r3_prefix_consistent_not_partial() {
    // Given: Tx1 commits, Tx2 starts, Tx3 commits, crash during Tx2
    // Recovery MUST see: Tx1 and Tx3 (all committed)
    // Recovery MUST NOT see: partial Tx2

    let db = create_test_db();

    // Tx1: Commit
    db.begin_tx();
    db.kv.put(&run_id, "tx1_key", "tx1_value").unwrap();
    db.commit().unwrap();

    // Tx2: Start but don't commit (simulate crash)
    db.begin_tx();
    db.kv.put(&run_id, "tx2_key", "tx2_value").unwrap();
    // No commit - crash happens here

    // Tx3: Commit (if concurrent transactions supported)
    // For sequential: skip this

    // Simulate crash without committing Tx2
    simulate_crash(&db);

    let db = reopen_database();

    // Tx1 must be present
    assert!(db.kv.get(&run_id, "tx1_key").is_some());

    // Tx2 must NOT be present (uncommitted)
    assert!(db.kv.get(&run_id, "tx2_key").is_none());
}

#[test]
fn test_r3_all_or_nothing_within_transaction() {
    let db = create_test_db();

    // Start transaction with multiple operations
    db.begin_tx();
    db.kv.put(&run_id, "key1", "v1").unwrap();
    db.kv.put(&run_id, "key2", "v2").unwrap();
    db.kv.put(&run_id, "key3", "v3").unwrap();

    // Crash before commit
    simulate_crash(&db);

    let db = reopen_database();

    // Either ALL three present or NONE
    let count = [
        db.kv.get(&run_id, "key1").is_some(),
        db.kv.get(&run_id, "key2").is_some(),
        db.kv.get(&run_id, "key3").is_some(),
    ].iter().filter(|&&x| x).count();

    assert!(count == 0 || count == 3,
        "R3 VIOLATED: Partial transaction visible ({}/3 keys)", count);
}
```

### 1.4 R4: Never Invents Data (`recovery_no_invent_tests.rs`)

**Invariant R4**: Only data explicitly written appears.

```rust
#[test]
fn test_r4_no_phantom_keys() {
    let db = create_test_db();

    // Write specific keys
    let written_keys: HashSet<_> = (0..100)
        .map(|i| format!("key_{}", i))
        .collect();

    for key in &written_keys {
        db.kv.put(&run_id, key, "value").unwrap();
    }

    // Simulate crash and recovery
    drop(db);
    let db = reopen_database();

    // Scan all keys
    let recovered_keys: HashSet<_> = db.kv.list(&run_id)
        .map(|k| k.to_string())
        .collect();

    // No extra keys should exist
    let phantoms: Vec<_> = recovered_keys.difference(&written_keys).collect();
    assert!(phantoms.is_empty(),
        "R4 VIOLATED: Phantom keys appeared: {:?}", phantoms);
}

#[test]
fn test_r4_no_phantom_values() {
    let db = create_test_db();

    db.kv.put(&run_id, "key", "original_value").unwrap();

    drop(db);
    let db = reopen_database();

    let value = db.kv.get(&run_id, "key").unwrap();
    assert_eq!(value, "original_value",
        "R4 VIOLATED: Value changed to phantom: {}", value);
}

#[test]
fn test_r4_no_invented_json_fields() {
    let db = create_test_db();

    let doc = json!({"name": "test", "count": 42});
    let doc_id = db.json.create(&run_id, doc.clone()).unwrap();

    drop(db);
    let db = reopen_database();

    let recovered = db.json.get(&run_id, &doc_id, &JsonPath::root()).unwrap();

    // No extra fields should exist
    assert_eq!(recovered, doc,
        "R4 VIOLATED: JSON document has phantom fields");
}

#[test]
fn test_r4_corruption_detected_not_invented() {
    // When WAL is corrupted, recovery should:
    // - Detect the corruption (CRC mismatch)
    // - Truncate to last valid boundary
    // - NOT invent data to fill gaps

    let db = create_test_db();
    db.kv.put(&run_id, "key", "value").unwrap();

    // Corrupt WAL
    corrupt_wal_entry(db.wal_path(), 1);

    let db = reopen_database();

    // Recovery may see fewer entries, but never phantom entries
    let keys: Vec<_> = db.kv.list(&run_id).collect();

    for key in keys {
        // Every key must have been explicitly written
        assert!(key.starts_with("key"),
            "R4 VIOLATED: Phantom key {} appeared after corruption", key);
    }
}
```

### 1.5 R5: Never Drops Committed (`recovery_no_drop_committed.rs`)

**Invariant R5**: Committed data survives any single crash.

```rust
#[test]
fn test_r5_committed_survives_crash() {
    let db = create_test_db();

    // Commit transaction
    db.begin_tx();
    db.kv.put(&run_id, "committed_key", "committed_value").unwrap();
    db.commit().unwrap();

    // Force WAL to disk
    db.sync().unwrap();

    // Simulate crash
    drop(db);

    let db = reopen_database();

    // Committed data must be present
    let value = db.kv.get(&run_id, "committed_key");
    assert!(value.is_some(),
        "R5 VIOLATED: Committed key disappeared after crash");
    assert_eq!(value.unwrap(), "committed_value",
        "R5 VIOLATED: Committed value changed after crash");
}

#[test]
fn test_r5_committed_survives_multiple_crashes() {
    let db = create_test_db();

    for i in 0..10 {
        db.begin_tx();
        db.kv.put(&run_id, &format!("key_{}", i), &format!("value_{}", i)).unwrap();
        db.commit().unwrap();
        db.sync().unwrap();
    }

    // Simulate 5 consecutive crashes
    for crash_num in 0..5 {
        drop(db);
        let db = reopen_database();

        // All committed data must still be present
        for i in 0..10 {
            let value = db.kv.get(&run_id, &format!("key_{}", i));
            assert!(value.is_some(),
                "R5 VIOLATED: Committed key_{} disappeared after crash {}", i, crash_num);
        }
    }
}

#[test]
fn test_r5_fsync_guarantees_durability() {
    let db = create_test_db();

    db.begin_tx();
    db.kv.put(&run_id, "key", "value").unwrap();
    db.commit().unwrap();

    // After fsync, data is durable
    db.sync().unwrap();

    // Even with process kill (not graceful shutdown)
    std::process::abort(); // Would normally kill process

    // Recovery must see the data
    // (This test verifies fsync contract, may need special setup)
}

#[test]
fn test_r5_cross_primitive_committed_survives() {
    let db = create_test_db();

    db.begin_tx();
    db.kv.put(&run_id, "kv_key", "kv_value").unwrap();
    db.json.create(&run_id, json!({"x": 1})).unwrap();
    db.event.append(&run_id, "test", json!({})).unwrap();
    db.commit().unwrap();
    db.sync().unwrap();

    drop(db);
    let db = reopen_database();

    // All primitives must have their committed data
    assert!(db.kv.get(&run_id, "kv_key").is_some(), "R5: KV data lost");
    assert!(db.json.list(&run_id).count() > 0, "R5: JSON data lost");
    assert!(db.event.count(&run_id) > 0, "R5: Event data lost");
}
```

### 1.6 R6: May Drop Uncommitted (`recovery_may_drop_uncommitted.rs`)

**Invariant R6**: Incomplete transactions may vanish.

```rust
#[test]
fn test_r6_uncommitted_may_vanish() {
    let db = create_test_db();

    // Start transaction but don't commit
    db.begin_tx();
    db.kv.put(&run_id, "uncommitted_key", "uncommitted_value").unwrap();
    // Note: No commit()

    // Crash
    simulate_crash(&db);

    let db = reopen_database();

    // Uncommitted data MAY be absent (this is allowed)
    let value = db.kv.get(&run_id, "uncommitted_key");
    // We don't assert absence - just that it's acceptable
    // The key point is that recovery doesn't fail
}

#[test]
fn test_r6_partial_transaction_discarded() {
    let db = create_test_db();

    // Transaction with multiple writes
    db.begin_tx();
    db.kv.put(&run_id, "key1", "v1").unwrap();
    db.kv.put(&run_id, "key2", "v2").unwrap();
    // Crash before commit - partial WAL

    simulate_crash_after_n_entries(&db, 1); // Only first entry written

    let db = reopen_database();

    // Partial transaction should be discarded entirely
    // Either both keys present (if fully committed somehow) or neither
    let has_key1 = db.kv.get(&run_id, "key1").is_some();
    let has_key2 = db.kv.get(&run_id, "key2").is_some();

    assert!(has_key1 == has_key2,
        "R6 VIOLATED: Partial transaction visible (key1={}, key2={})", has_key1, has_key2);
}

#[test]
fn test_r6_uncommitted_does_not_affect_committed() {
    let db = create_test_db();

    // First: committed transaction
    db.begin_tx();
    db.kv.put(&run_id, "committed_key", "committed_value").unwrap();
    db.commit().unwrap();
    db.sync().unwrap();

    // Second: uncommitted transaction
    db.begin_tx();
    db.kv.put(&run_id, "uncommitted_key", "uncommitted_value").unwrap();
    // No commit - crash

    simulate_crash(&db);

    let db = reopen_database();

    // Committed data MUST be present (R5)
    assert!(db.kv.get(&run_id, "committed_key").is_some(),
        "Committed data affected by uncommitted transaction");
}

#[test]
fn test_r6_txbegin_without_commit_discarded() {
    let db = create_test_db();

    // Write TxBegin to WAL
    db.begin_tx();
    db.kv.put(&run_id, "key", "value").unwrap();
    // Force WAL write but no commit
    db.flush_wal().unwrap();

    simulate_crash(&db);

    let db = reopen_database();

    // Transaction without TxCommit should be discarded
    // Recovery should succeed (not fail)
    assert!(db.is_healthy());
}
```

---

## Tier 2: Replay Invariants

These tests ensure **replay_run() behaves as a pure function**.

### 2.1 P1: Pure Function (`replay_pure_function_tests.rs`)

**Invariant P1**: fn(run_id, event_log) → ReadOnlyView.

```rust
#[test]
fn test_p1_replay_is_function() {
    let db = create_test_db();
    let run_id = db.run_index.create_run("test").unwrap();

    // Write some data
    db.kv.put(&run_id, "key", "value").unwrap();
    db.event.append(&run_id, "test.event", json!({})).unwrap();

    // Replay is a function: same inputs → same output
    let view1 = db.replay_run(run_id).unwrap();
    let view2 = db.replay_run(run_id).unwrap();

    assert_eq!(view1.kv_state, view2.kv_state);
    assert_eq!(view1.events, view2.events);
}

#[test]
fn test_p1_replay_takes_run_id_and_returns_view() {
    let db = create_test_db();
    let run_id = db.run_index.create_run("test").unwrap();

    db.kv.put(&run_id, "key", "value").unwrap();

    // Function signature: RunId → ReadOnlyView
    let view: ReadOnlyView = db.replay_run(run_id).unwrap();

    // View contains computed state
    assert_eq!(view.kv_state.get("key"), Some(&"value".to_string()));
}

#[test]
fn test_p1_different_runs_different_views() {
    let db = create_test_db();

    let run1 = db.run_index.create_run("run1").unwrap();
    let run2 = db.run_index.create_run("run2").unwrap();

    db.kv.put(&run1, "key", "run1_value").unwrap();
    db.kv.put(&run2, "key", "run2_value").unwrap();

    let view1 = db.replay_run(run1).unwrap();
    let view2 = db.replay_run(run2).unwrap();

    // Different runs → different views
    assert_ne!(view1.kv_state.get("key"), view2.kv_state.get("key"));
}
```

### 2.2 P2: Side-Effect Free (`replay_side_effect_tests.rs`)

**Invariant P2**: Does NOT mutate any persistent state.

```rust
#[test]
fn test_p2_replay_does_not_mutate_state() {
    let db = create_test_db();
    let run_id = db.run_index.create_run("test").unwrap();

    db.kv.put(&run_id, "key", "original").unwrap();

    let state_before = capture_state(&db);

    // Replay should NOT modify anything
    let _view = db.replay_run(run_id).unwrap();

    let state_after = capture_state(&db);

    assert_eq!(state_before, state_after,
        "P2 VIOLATED: replay_run() mutated persistent state");
}

#[test]
fn test_p2_replay_does_not_write_to_wal() {
    let db = create_test_db();
    let run_id = db.run_index.create_run("test").unwrap();

    db.kv.put(&run_id, "key", "value").unwrap();

    let wal_size_before = db.wal_size();

    // Replay should NOT write to WAL
    let _view = db.replay_run(run_id).unwrap();

    let wal_size_after = db.wal_size();

    assert_eq!(wal_size_before, wal_size_after,
        "P2 VIOLATED: replay_run() wrote to WAL");
}

#[test]
fn test_p2_replay_does_not_create_snapshots() {
    let db = create_test_db();
    let run_id = db.run_index.create_run("test").unwrap();

    let snapshot_count_before = db.snapshot_count();

    let _view = db.replay_run(run_id).unwrap();

    let snapshot_count_after = db.snapshot_count();

    assert_eq!(snapshot_count_before, snapshot_count_after,
        "P2 VIOLATED: replay_run() created snapshot");
}
```

### 2.3 P3: Derived View (`replay_derived_view_tests.rs`)

**Invariant P3**: Computes view, does NOT reconstruct state.

```rust
#[test]
fn test_p3_view_is_computed_not_stored() {
    let db = create_test_db();
    let run_id = db.run_index.create_run("test").unwrap();

    db.kv.put(&run_id, "key", "value").unwrap();

    // View is computed on-demand
    let view = db.replay_run(run_id).unwrap();

    // View is ReadOnlyView, not Database state
    assert!(view.is_read_only());

    // Cannot write through view
    // view.kv_state.insert("new_key", "value"); // Should not compile or fail
}

#[test]
fn test_p3_view_reflects_events_not_mutations() {
    let db = create_test_db();
    let run_id = db.run_index.create_run("test").unwrap();

    // Write sequence: put, put, put (overwrites)
    db.kv.put(&run_id, "key", "v1").unwrap();
    db.kv.put(&run_id, "key", "v2").unwrap();
    db.kv.put(&run_id, "key", "v3").unwrap();

    // View should show final computed state
    let view = db.replay_run(run_id).unwrap();

    assert_eq!(view.kv_state.get("key"), Some(&"v3".to_string()));
}
```

### 2.4 P4: Does Not Persist (`replay_ephemeral_tests.rs`)

**Invariant P4**: Result is ephemeral, discarded after use.

```rust
#[test]
fn test_p4_view_not_persisted() {
    let db = create_test_db();
    let run_id = db.run_index.create_run("test").unwrap();

    db.kv.put(&run_id, "key", "value").unwrap();

    // Create view
    let view = db.replay_run(run_id).unwrap();

    // Simulate crash and recovery
    drop(view);
    drop(db);

    let db = reopen_database();

    // View should NOT be stored anywhere
    // Only original data should be present
    let value = db.kv.get(&run_id, "key").unwrap();
    assert_eq!(value, "value");
}

#[test]
fn test_p4_view_garbage_collected() {
    let db = create_test_db();
    let run_id = db.run_index.create_run("test").unwrap();

    db.kv.put(&run_id, "key", "value").unwrap();

    let mem_before = get_memory_usage();

    // Create and drop many views
    for _ in 0..1000 {
        let _view = db.replay_run(run_id).unwrap();
    }

    let mem_after = get_memory_usage();

    // Memory should not grow significantly
    assert!(mem_after - mem_before < 10_000_000,
        "P4 VIOLATED: Views not garbage collected");
}
```

### 2.5 P5: Deterministic (`replay_determinism_tests.rs`)

**Invariant P5**: Same inputs → identical view.

```rust
#[test]
fn test_p5_replay_deterministic() {
    let db = create_test_db();
    let run_id = db.run_index.create_run("test").unwrap();

    db.kv.put(&run_id, "key1", "value1").unwrap();
    db.kv.put(&run_id, "key2", "value2").unwrap();
    db.event.append(&run_id, "test", json!({})).unwrap();

    // Replay 100 times
    let mut hashes = Vec::new();
    for _ in 0..100 {
        let view = db.replay_run(run_id).unwrap();
        hashes.push(hash_view(&view));
    }

    // All hashes identical
    assert!(hashes.windows(2).all(|w| w[0] == w[1]),
        "P5 VIOLATED: replay_run() not deterministic");
}

#[test]
fn test_p5_deterministic_across_restarts() {
    let db = create_test_db();
    let run_id = db.run_index.create_run("test").unwrap();

    db.kv.put(&run_id, "key", "value").unwrap();

    let view1 = db.replay_run(run_id).unwrap();
    let hash1 = hash_view(&view1);

    drop(db);
    let db = reopen_database();

    let view2 = db.replay_run(run_id).unwrap();
    let hash2 = hash_view(&view2);

    assert_eq!(hash1, hash2);
}
```

### 2.6 P6: Idempotent (`replay_idempotent_tests.rs`)

**Invariant P6**: Safe to call multiple times.

```rust
#[test]
fn test_p6_multiple_replays_safe() {
    let db = create_test_db();
    let run_id = db.run_index.create_run("test").unwrap();

    db.kv.put(&run_id, "key", "value").unwrap();

    // Multiple concurrent replays
    let handles: Vec<_> = (0..10)
        .map(|_| {
            let db = db.clone();
            std::thread::spawn(move || {
                db.replay_run(run_id).unwrap()
            })
        })
        .collect();

    let views: Vec<_> = handles.into_iter()
        .map(|h| h.join().unwrap())
        .collect();

    // All views identical
    let first_hash = hash_view(&views[0]);
    for view in &views[1..] {
        assert_eq!(first_hash, hash_view(view));
    }
}

#[test]
fn test_p6_replay_does_not_interfere() {
    let db = create_test_db();

    let run1 = db.run_index.create_run("run1").unwrap();
    let run2 = db.run_index.create_run("run2").unwrap();

    db.kv.put(&run1, "key", "run1_value").unwrap();
    db.kv.put(&run2, "key", "run2_value").unwrap();

    // Interleaved replays don't interfere
    let view1a = db.replay_run(run1).unwrap();
    let view2 = db.replay_run(run2).unwrap();
    let view1b = db.replay_run(run1).unwrap();

    assert_eq!(hash_view(&view1a), hash_view(&view1b));
    assert_ne!(hash_view(&view1a), hash_view(&view2));
}
```

---

## Tier 3: Snapshot System

### 3.1 Snapshot Format (`snapshot_format_tests.rs`)

```rust
#[test]
fn test_snapshot_magic_number() {
    let db = create_test_db();
    populate_test_data(&db);

    let snapshot_path = db.create_snapshot().unwrap();

    let mut file = File::open(&snapshot_path).unwrap();
    let mut magic = [0u8; 8];
    file.read_exact(&mut magic).unwrap();

    assert_eq!(&magic, b"INMEMSNP",
        "Snapshot magic number incorrect");
}

#[test]
fn test_snapshot_version_field() {
    let db = create_test_db();
    let snapshot_path = db.create_snapshot().unwrap();

    let snapshot = Snapshot::load(&snapshot_path).unwrap();

    assert!(snapshot.version() >= 1,
        "Snapshot version must be >= 1");
}

#[test]
fn test_snapshot_contains_all_primitives() {
    let db = create_test_db();

    // Write to all primitives
    db.kv.put(&run_id, "key", "value").unwrap();
    db.json.create(&run_id, json!({"x": 1})).unwrap();
    db.event.append(&run_id, "test", json!({})).unwrap();
    db.state.init(&run_id, "cell", 0).unwrap();

    let snapshot_path = db.create_snapshot().unwrap();
    let snapshot = Snapshot::load(&snapshot_path).unwrap();

    // All primitives should have blobs
    assert!(snapshot.has_blob(PrimitiveKind::Kv));
    assert!(snapshot.has_blob(PrimitiveKind::Json));
    assert!(snapshot.has_blob(PrimitiveKind::Event));
    assert!(snapshot.has_blob(PrimitiveKind::State));
}

#[test]
fn test_snapshot_wal_offset_recorded() {
    let db = create_test_db();

    let wal_offset_before = db.wal_offset();

    db.kv.put(&run_id, "key", "value").unwrap();

    let snapshot_path = db.create_snapshot().unwrap();
    let snapshot = Snapshot::load(&snapshot_path).unwrap();

    assert!(snapshot.wal_offset() > wal_offset_before,
        "Snapshot wal_offset not recorded correctly");
}
```

### 3.2 Snapshot CRC (`snapshot_crc_tests.rs`)

```rust
#[test]
fn test_snapshot_crc_validates() {
    let db = create_test_db();
    populate_test_data(&db);

    let snapshot_path = db.create_snapshot().unwrap();

    // Valid snapshot loads without error
    let result = Snapshot::load(&snapshot_path);
    assert!(result.is_ok());
}

#[test]
fn test_snapshot_crc_detects_corruption() {
    let db = create_test_db();
    populate_test_data(&db);

    let snapshot_path = db.create_snapshot().unwrap();

    // Corrupt the snapshot file
    let mut file = OpenOptions::new()
        .read(true).write(true)
        .open(&snapshot_path).unwrap();
    file.seek(SeekFrom::Start(100)).unwrap();
    file.write_all(&[0xFF; 10]).unwrap();

    // Load should fail with CRC error
    let result = Snapshot::load(&snapshot_path);
    assert!(matches!(result, Err(SnapshotError::CrcMismatch)));
}

#[test]
fn test_snapshot_blob_crc_validates() {
    let db = create_test_db();
    db.kv.put(&run_id, "key", "value").unwrap();

    let snapshot_path = db.create_snapshot().unwrap();
    let snapshot = Snapshot::load(&snapshot_path).unwrap();

    // Each blob should have valid CRC
    for blob in snapshot.blobs() {
        assert!(blob.validate_crc().is_ok());
    }
}
```

### 3.3 Atomic Write (`snapshot_atomic_write_tests.rs`)

```rust
#[test]
fn test_snapshot_atomic_write_via_rename() {
    let db = create_test_db();
    populate_test_data(&db);

    // Start snapshot write
    let snapshot_dir = db.snapshot_dir();

    // During write, .tmp file exists
    // After complete, only .snap file exists

    let snapshot_path = db.create_snapshot().unwrap();

    // No .tmp files should remain
    let tmp_files: Vec<_> = std::fs::read_dir(&snapshot_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension() == Some(OsStr::new("tmp")))
        .collect();

    assert!(tmp_files.is_empty(),
        "Temp files remaining after snapshot complete");
}

#[test]
fn test_partial_snapshot_ignored() {
    let db = create_test_db();
    populate_test_data(&db);

    // Create a partial .tmp snapshot file
    let tmp_path = db.snapshot_dir().join("snapshot.tmp");
    std::fs::write(&tmp_path, b"partial snapshot data").unwrap();

    // Create valid snapshot
    let snapshot_path = db.create_snapshot().unwrap();

    // Discovery should ignore .tmp file
    let snapshots = db.discover_snapshots().unwrap();

    for snap in &snapshots {
        assert!(!snap.path().to_string_lossy().contains(".tmp"),
            "Partial .tmp snapshot should be ignored");
    }
}

#[test]
fn test_crash_during_snapshot_write() {
    let db = create_test_db();
    populate_test_data(&db);

    // Simulate crash during snapshot write
    // (Create .tmp file but don't rename)
    let tmp_path = db.snapshot_dir().join("snapshot.tmp");
    std::fs::write(&tmp_path, b"partial data").unwrap();

    // Recovery should ignore incomplete snapshot
    drop(db);
    let db = reopen_database();

    // Database should still work
    assert!(db.is_healthy());
}
```

### 3.4 Snapshot Discovery (`snapshot_discovery_tests.rs`)

```rust
#[test]
fn test_discover_snapshots_ordered_by_offset() {
    let db = create_test_db();

    // Create multiple snapshots
    db.kv.put(&run_id, "key1", "v1").unwrap();
    db.create_snapshot().unwrap();

    db.kv.put(&run_id, "key2", "v2").unwrap();
    db.create_snapshot().unwrap();

    db.kv.put(&run_id, "key3", "v3").unwrap();
    db.create_snapshot().unwrap();

    let snapshots = db.discover_snapshots().unwrap();

    // Should be ordered by wal_offset descending (most recent first)
    for window in snapshots.windows(2) {
        assert!(window[0].wal_offset() >= window[1].wal_offset(),
            "Snapshots not ordered by wal_offset");
    }
}

#[test]
fn test_discover_snapshots_filters_invalid() {
    let db = create_test_db();

    // Create valid snapshot
    db.kv.put(&run_id, "key", "value").unwrap();
    let valid_path = db.create_snapshot().unwrap();

    // Create invalid snapshot file
    let invalid_path = db.snapshot_dir().join("snapshot-invalid.snap");
    std::fs::write(&invalid_path, b"not a valid snapshot").unwrap();

    let snapshots = db.discover_snapshots().unwrap();

    // Should only return valid snapshots
    for snap in &snapshots {
        assert!(snap.is_valid());
    }
}
```

### 3.5 Snapshot Fallback (`snapshot_fallback_tests.rs`)

```rust
#[test]
fn test_corrupt_snapshot_falls_back_to_older() {
    let db = create_test_db();

    // Create first snapshot
    db.kv.put(&run_id, "key1", "v1").unwrap();
    let snapshot1 = db.create_snapshot().unwrap();

    // Create second snapshot
    db.kv.put(&run_id, "key2", "v2").unwrap();
    let snapshot2 = db.create_snapshot().unwrap();

    // Corrupt most recent snapshot
    corrupt_file(&snapshot2);

    drop(db);
    let db = reopen_database();

    // Should recover using older snapshot
    assert!(db.kv.get(&run_id, "key1").is_some());
    // key2 may or may not be present (depends on WAL)
}

#[test]
fn test_all_snapshots_corrupt_falls_back_to_wal() {
    let db = create_test_db();

    db.kv.put(&run_id, "key", "value").unwrap();
    let snapshot = db.create_snapshot().unwrap();

    // Corrupt all snapshots
    for snap in std::fs::read_dir(db.snapshot_dir()).unwrap() {
        if let Ok(entry) = snap {
            if entry.path().extension() == Some(OsStr::new("snap")) {
                corrupt_file(&entry.path());
            }
        }
    }

    drop(db);
    let db = reopen_database();

    // Should recover from WAL alone
    assert!(db.kv.get(&run_id, "key").is_some(),
        "Full WAL replay failed");
}
```

---

## Tier 4: WAL System

### 4.1 WAL Entry Format (`wal_entry_format_tests.rs`)

```rust
#[test]
fn test_wal_entry_has_length_prefix() {
    let entry = WalEntry::new(WalEntryType::KvPut, tx_id, payload);
    let bytes = entry.serialize();

    // First 4 bytes are length
    let length = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
    assert_eq!(length as usize, bytes.len());
}

#[test]
fn test_wal_entry_has_type_field() {
    let entry = WalEntry::new(WalEntryType::KvPut, tx_id, payload);
    let bytes = entry.serialize();

    // Byte 4 is type
    assert_eq!(bytes[4], WalEntryType::KvPut as u8);
}

#[test]
fn test_wal_entry_has_version_field() {
    let entry = WalEntry::new(WalEntryType::KvPut, tx_id, payload);
    let bytes = entry.serialize();

    // Byte 5 is version
    assert!(bytes[5] >= 1);
}

#[test]
fn test_wal_entry_has_txid() {
    let tx_id = TxId::new(run_id, 42);
    let entry = WalEntry::new(WalEntryType::KvPut, tx_id, payload);
    let bytes = entry.serialize();

    // Bytes 6-21 are TxId (16 bytes)
    let recovered_tx_id = TxId::from_bytes(&bytes[6..22]);
    assert_eq!(recovered_tx_id, tx_id);
}

#[test]
fn test_wal_entry_has_crc32() {
    let entry = WalEntry::new(WalEntryType::KvPut, tx_id, payload);
    let bytes = entry.serialize();

    // Last 4 bytes are CRC32
    let crc = u32::from_le_bytes(bytes[bytes.len()-4..].try_into().unwrap());

    // Verify CRC
    let computed = crc32(&bytes[4..bytes.len()-4]); // type through payload
    assert_eq!(crc, computed);
}
```

### 4.2 WAL CRC Validation (`wal_crc_validation_tests.rs`)

```rust
#[test]
fn test_wal_crc_catches_bit_flip() {
    let entry = WalEntry::new(WalEntryType::KvPut, tx_id, payload);
    let mut bytes = entry.serialize();

    // Flip a bit in payload
    bytes[20] ^= 0x01;

    // Parsing should fail
    let result = WalEntry::parse(&bytes);
    assert!(matches!(result, Err(WalError::CrcMismatch)));
}

#[test]
fn test_wal_crc_catches_truncation() {
    let entry = WalEntry::new(WalEntryType::KvPut, tx_id, payload);
    let bytes = entry.serialize();

    // Truncate entry
    let truncated = &bytes[..bytes.len()-10];

    // Parsing should fail
    let result = WalEntry::parse(truncated);
    assert!(result.is_err());
}

#[test]
fn test_wal_crc_catches_extension() {
    let entry = WalEntry::new(WalEntryType::KvPut, tx_id, payload);
    let mut bytes = entry.serialize();

    // Extend entry
    bytes.extend_from_slice(b"extra garbage");

    // Parsing should fail (length mismatch)
    let result = WalEntry::parse(&bytes);
    assert!(result.is_err());
}

#[test]
fn test_wal_crc_covers_type_version_txid_payload() {
    let entry = WalEntry::new(WalEntryType::KvPut, tx_id, payload);
    let bytes = entry.serialize();

    // Modify type (byte 4)
    let mut corrupted = bytes.clone();
    corrupted[4] ^= 0xFF;
    assert!(WalEntry::parse(&corrupted).is_err());

    // Modify version (byte 5)
    let mut corrupted = bytes.clone();
    corrupted[5] ^= 0xFF;
    assert!(WalEntry::parse(&corrupted).is_err());

    // Modify txid (bytes 6-21)
    let mut corrupted = bytes.clone();
    corrupted[10] ^= 0xFF;
    assert!(WalEntry::parse(&corrupted).is_err());
}
```

### 4.3 Transaction Framing (`wal_transaction_framing_tests.rs`)

```rust
#[test]
fn test_transaction_has_begin_and_commit() {
    let db = create_test_db();

    db.begin_tx();
    db.kv.put(&run_id, "key", "value").unwrap();
    db.commit().unwrap();

    let entries = db.read_wal_entries();

    // Should have TxBegin ... operations ... TxCommit
    let tx_begin_count = entries.iter()
        .filter(|e| e.entry_type() == WalEntryType::TxBegin)
        .count();
    let tx_commit_count = entries.iter()
        .filter(|e| e.entry_type() == WalEntryType::TxCommit)
        .count();

    assert_eq!(tx_begin_count, 1);
    assert_eq!(tx_commit_count, 1);
}

#[test]
fn test_all_entries_in_transaction_share_txid() {
    let db = create_test_db();

    db.begin_tx();
    db.kv.put(&run_id, "key1", "v1").unwrap();
    db.kv.put(&run_id, "key2", "v2").unwrap();
    db.commit().unwrap();

    let entries = db.read_wal_entries();

    // All entries should have same TxId
    let tx_ids: HashSet<_> = entries.iter().map(|e| e.tx_id()).collect();
    assert_eq!(tx_ids.len(), 1,
        "Transaction entries have inconsistent TxIds");
}

#[test]
fn test_uncommitted_transaction_has_no_commit() {
    let db = create_test_db();

    db.begin_tx();
    db.kv.put(&run_id, "key", "value").unwrap();
    // No commit

    db.flush_wal().unwrap();

    let entries = db.read_wal_entries();

    let has_begin = entries.iter().any(|e| e.entry_type() == WalEntryType::TxBegin);
    let has_commit = entries.iter().any(|e| e.entry_type() == WalEntryType::TxCommit);

    assert!(has_begin);
    assert!(!has_commit, "Uncommitted transaction should not have TxCommit");
}
```

### 4.4 WAL Entry Types (`wal_entry_type_tests.rs`)

```rust
#[test]
fn test_core_entry_types_in_range() {
    // 0x00-0x0F reserved for core
    assert!(WalEntryType::TxBegin as u8 <= 0x0F);
    assert!(WalEntryType::TxCommit as u8 <= 0x0F);
    assert!(WalEntryType::TxAbort as u8 <= 0x0F);
    assert!(WalEntryType::Checkpoint as u8 <= 0x0F);
}

#[test]
fn test_kv_entry_types_in_range() {
    // 0x10-0x1F reserved for KV
    let kv_types = [WalEntryType::KvPut, WalEntryType::KvDelete, WalEntryType::KvClear];
    for entry_type in kv_types {
        let type_byte = entry_type as u8;
        assert!(type_byte >= 0x10 && type_byte <= 0x1F,
            "{:?} not in KV range", entry_type);
    }
}

#[test]
fn test_json_entry_types_in_range() {
    // 0x20-0x2F reserved for JSON
    let json_types = [
        WalEntryType::JsonCreate,
        WalEntryType::JsonSet,
        WalEntryType::JsonDelete,
        WalEntryType::JsonPatch,
    ];
    for entry_type in json_types {
        let type_byte = entry_type as u8;
        assert!(type_byte >= 0x20 && type_byte <= 0x2F);
    }
}

#[test]
fn test_entry_type_roundtrip() {
    for type_byte in 0x00..=0x6F {
        if let Ok(entry_type) = WalEntryType::try_from(type_byte) {
            assert_eq!(entry_type as u8, type_byte);
        }
    }
}

#[test]
fn test_unknown_entry_type_handled() {
    // Future/unknown entry types should be handled gracefully
    let result = WalEntryType::try_from(0xFE);
    assert!(result.is_err() || matches!(result, Ok(WalEntryType::Unknown)));
}
```

### 4.5 WAL Truncation (`wal_truncation_tests.rs`)

```rust
#[test]
fn test_truncate_to_last_valid_entry() {
    let db = create_test_db();

    // Write valid entries
    db.begin_tx();
    db.kv.put(&run_id, "key1", "v1").unwrap();
    db.commit().unwrap();

    db.begin_tx();
    db.kv.put(&run_id, "key2", "v2").unwrap();
    db.commit().unwrap();

    // Corrupt last entry
    corrupt_last_wal_entry(&db);

    drop(db);
    let db = reopen_database();

    // First transaction should be present
    assert!(db.kv.get(&run_id, "key1").is_some());

    // Second transaction may or may not be present
    // (depends on corruption location)
}

#[test]
fn test_truncate_to_transaction_boundary() {
    let db = create_test_db();

    // First transaction
    db.begin_tx();
    db.kv.put(&run_id, "key1", "v1").unwrap();
    db.commit().unwrap();

    // Second transaction (incomplete in WAL)
    db.begin_tx();
    db.kv.put(&run_id, "key2", "v2").unwrap();
    // Crash before commit

    simulate_crash(&db);

    let db = reopen_database();

    // Should truncate to transaction boundary
    // First tx complete: present
    // Second tx incomplete: absent
    assert!(db.kv.get(&run_id, "key1").is_some());
    assert!(db.kv.get(&run_id, "key2").is_none());
}

#[test]
fn test_truncate_preserves_earlier_transactions() {
    let db = create_test_db();

    // Write 10 transactions
    for i in 0..10 {
        db.begin_tx();
        db.kv.put(&run_id, &format!("key_{}", i), "value").unwrap();
        db.commit().unwrap();
    }

    // Corrupt entry in middle
    corrupt_wal_entry_at_offset(&db, 500);

    drop(db);
    let db = reopen_database();

    // Earlier transactions should be preserved
    // Some later transactions may be lost
    let present_count = (0..10)
        .filter(|i| db.kv.get(&run_id, &format!("key_{}", i)).is_some())
        .count();

    assert!(present_count > 0,
        "All transactions lost due to mid-WAL corruption");
}
```

---

## Tier 5: Crash Scenarios

### 5.1 Crash During WAL Write (`crash_during_wal_write_tests.rs`)

```rust
#[test]
fn test_crash_partial_wal_entry() {
    let db = create_test_db();

    db.begin_tx();
    db.kv.put(&run_id, "key", "value").unwrap();

    // Write partial entry (simulate crash mid-write)
    write_partial_wal_entry(&db, 10); // Only 10 bytes

    drop(db);
    let db = reopen_database();

    // Partial entry should be discarded
    // Recovery should succeed
    assert!(db.is_healthy());
}

#[test]
fn test_crash_between_entries() {
    let db = create_test_db();

    // Complete entry
    db.begin_tx();
    db.kv.put(&run_id, "key1", "v1").unwrap();
    db.commit().unwrap();

    // Crash between entries (no partial data)
    simulate_crash(&db);

    let db = reopen_database();

    // Complete entry should be present
    assert!(db.kv.get(&run_id, "key1").is_some());
}

#[test]
fn test_crash_corrupted_length() {
    let db = create_test_db();

    db.begin_tx();
    db.kv.put(&run_id, "key", "value").unwrap();
    db.commit().unwrap();

    // Corrupt length field
    corrupt_wal_length_field(&db);

    drop(db);
    let db = reopen_database();

    // Should detect corruption via CRC or bounds check
    // Recovery should handle gracefully
    assert!(db.is_healthy());
}
```

### 5.2 Crash During Commit (`crash_during_commit_tests.rs`)

```rust
#[test]
fn test_crash_before_txcommit_write() {
    let db = create_test_db();

    db.begin_tx();
    db.kv.put(&run_id, "key", "value").unwrap();
    // Crash before TxCommit written

    simulate_crash_before_commit(&db);

    let db = reopen_database();

    // Transaction without TxCommit is discarded
    assert!(db.kv.get(&run_id, "key").is_none());
}

#[test]
fn test_crash_after_txcommit_before_fsync() {
    let db = create_test_db();

    db.begin_tx();
    db.kv.put(&run_id, "key", "value").unwrap();
    db.commit_without_fsync().unwrap(); // TxCommit in buffer

    // Crash before fsync
    simulate_crash(&db);

    let db = reopen_database();

    // Transaction MAY or MAY NOT be present
    // (depends on OS buffer behavior)
    // Key point: recovery succeeds
    assert!(db.is_healthy());
}

#[test]
fn test_crash_after_fsync_guarantees_durability() {
    let db = create_test_db();

    db.begin_tx();
    db.kv.put(&run_id, "key", "value").unwrap();
    db.commit().unwrap();
    db.sync().unwrap(); // Force to disk

    simulate_crash(&db);

    let db = reopen_database();

    // After fsync, data is guaranteed durable
    assert!(db.kv.get(&run_id, "key").is_some(),
        "Data lost after commit+fsync");
}
```

### 5.3 Crash During Snapshot (`crash_during_snapshot_tests.rs`)

```rust
#[test]
fn test_crash_partial_snapshot() {
    let db = create_test_db();

    db.kv.put(&run_id, "key", "value").unwrap();

    // Start snapshot write
    start_snapshot_write(&db);

    // Crash mid-write
    simulate_crash(&db);

    let db = reopen_database();

    // Partial snapshot should be ignored
    // Full WAL replay should work
    assert!(db.kv.get(&run_id, "key").is_some());
}

#[test]
fn test_crash_during_snapshot_fsync() {
    let db = create_test_db();
    populate_test_data(&db);

    // Write snapshot to temp file
    // Crash during fsync (before rename)
    simulate_crash_during_snapshot_fsync(&db);

    let db = reopen_database();

    // Recovery should work using WAL
    assert!(db.is_healthy());
}

#[test]
fn test_crash_during_snapshot_rename() {
    let db = create_test_db();
    populate_test_data(&db);

    // Snapshot written, fsync done
    // Crash during rename (atomic, so either old or new)

    let db = reopen_database();

    // Either old snapshot or no snapshot
    // Either way, recovery should work
    assert!(db.is_healthy());
}
```

### 5.4 Crash Multi-Primitive (`crash_multi_primitive_tests.rs`)

```rust
#[test]
fn test_crash_mid_cross_primitive_transaction() {
    let db = create_test_db();

    db.begin_tx();
    db.kv.put(&run_id, "kv_key", "kv_value").unwrap();
    db.json.create(&run_id, json!({"x": 1})).unwrap();
    // Crash before commit

    simulate_crash(&db);

    let db = reopen_database();

    // Entire transaction should be absent
    assert!(db.kv.get(&run_id, "kv_key").is_none());
    assert_eq!(db.json.list(&run_id).count(), 0);
}

#[test]
fn test_crash_preserves_prior_cross_primitive() {
    let db = create_test_db();

    // First transaction: commit
    db.begin_tx();
    db.kv.put(&run_id, "kv_key1", "v1").unwrap();
    db.json.create(&run_id, json!({"committed": true})).unwrap();
    db.commit().unwrap();
    db.sync().unwrap();

    // Second transaction: crash
    db.begin_tx();
    db.kv.put(&run_id, "kv_key2", "v2").unwrap();
    db.json.create(&run_id, json!({"uncommitted": true})).unwrap();
    // No commit

    simulate_crash(&db);

    let db = reopen_database();

    // First transaction: present in both primitives
    assert!(db.kv.get(&run_id, "kv_key1").is_some());
    assert_eq!(db.json.list(&run_id).count(), 1);

    // Second transaction: absent in both primitives
    assert!(db.kv.get(&run_id, "kv_key2").is_none());
}
```

### 5.5 Recovery Sequence (`crash_recovery_sequence_tests.rs`)

```rust
#[test]
fn test_full_recovery_sequence() {
    let db = create_test_db();

    // Setup: data + snapshots + WAL
    for i in 0..100 {
        db.kv.put(&run_id, &format!("key_{}", i), "value").unwrap();
        if i % 20 == 0 {
            db.create_snapshot().unwrap();
        }
    }

    // Crash
    simulate_crash(&db);

    // Recovery sequence:
    // 1. Discover snapshots
    // 2. Load most recent valid
    // 3. Replay WAL from snapshot point
    // 4. Validate invariants

    let db = reopen_database();

    // All 100 keys should be present
    for i in 0..100 {
        assert!(db.kv.get(&run_id, &format!("key_{}", i)).is_some(),
            "Key {} missing after recovery", i);
    }
}

#[test]
fn test_recovery_validates_invariants() {
    let db = create_test_db();
    populate_test_data(&db);

    drop(db);
    let db = reopen_database();

    // Recovery should validate:
    // - No orphaned WAL entries
    // - No duplicate TxIds
    // - State is consistent

    assert!(db.validate_invariants().is_ok());
}
```

---

## Tier 6: Cross-Primitive Atomicity

### 6.1 Atomic Multi-Write (`atomic_multi_write_tests.rs`)

```rust
#[test]
fn test_atomic_all_or_nothing() {
    let db = create_test_db();

    // Transaction with writes to all primitives
    db.begin_tx();
    db.kv.put(&run_id, "kv_key", "kv_value").unwrap();
    db.json.create(&run_id, json!({"x": 1})).unwrap();
    db.event.append(&run_id, "test", json!({})).unwrap();
    db.state.init(&run_id, "cell", 0).unwrap();
    db.commit().unwrap();

    drop(db);
    let db = reopen_database();

    // ALL primitives should have data
    assert!(db.kv.get(&run_id, "kv_key").is_some());
    assert!(db.json.list(&run_id).count() > 0);
    assert!(db.event.count(&run_id) > 0);
    assert!(db.state.get(&run_id, "cell").is_some());
}

#[test]
fn test_atomic_failure_rolls_back_all() {
    let db = create_test_db();

    db.begin_tx();
    db.kv.put(&run_id, "kv_key", "kv_value").unwrap();
    db.json.create(&run_id, json!({"x": 1})).unwrap();
    // Force failure
    db.abort();

    // Nothing should be committed
    assert!(db.kv.get(&run_id, "kv_key").is_none());
    assert_eq!(db.json.list(&run_id).count(), 0);
}
```

### 6.2 Atomic Recovery Boundaries (`atomic_recovery_boundary_tests.rs`)

```rust
#[test]
fn test_recovery_respects_transaction_boundaries() {
    let db = create_test_db();

    // Committed transaction
    db.begin_tx();
    db.kv.put(&run_id, "committed", "yes").unwrap();
    db.commit().unwrap();

    // Uncommitted transaction
    db.begin_tx();
    db.kv.put(&run_id, "uncommitted", "yes").unwrap();
    // No commit

    simulate_crash(&db);

    let db = reopen_database();

    // Only committed transaction visible
    assert!(db.kv.get(&run_id, "committed").is_some());
    assert!(db.kv.get(&run_id, "uncommitted").is_none());
}

#[test]
fn test_recovery_all_entries_same_tx_or_none() {
    let db = create_test_db();

    db.begin_tx();
    for i in 0..10 {
        db.kv.put(&run_id, &format!("key_{}", i), "value").unwrap();
    }
    // Crash before commit

    simulate_crash(&db);

    let db = reopen_database();

    // Either all 10 present or none
    let count = (0..10)
        .filter(|i| db.kv.get(&run_id, &format!("key_{}", i)).is_some())
        .count();

    assert!(count == 0 || count == 10,
        "Partial transaction visible: {}/10", count);
}
```

### 6.3 Atomic Cross-Primitive Tests (`atomic_cross_primitive_tests.rs`)

```rust
#[test]
fn test_kv_json_event_atomic() {
    let db = create_test_db();

    for _ in 0..10 {
        db.begin_tx();
        db.kv.put(&run_id, "key", &format!("iter")).unwrap();
        db.json.create(&run_id, json!({"iter": true})).unwrap();
        db.event.append(&run_id, "iter.event", json!({})).unwrap();
        db.commit().unwrap();
    }

    drop(db);
    let db = reopen_database();

    // Counts should be consistent across primitives
    // (Each transaction adds 1 to each)
    let json_count = db.json.list(&run_id).count();
    let event_count = db.event.count(&run_id);

    assert_eq!(json_count, event_count,
        "Cross-primitive counts inconsistent: json={}, event={}", json_count, event_count);
}
```

---

## Tier 7: Run Lifecycle

### 7.1 Begin/End Tests (`run_begin_end_tests.rs`)

```rust
#[test]
fn test_begin_run_creates_active_run() {
    let db = create_test_db();

    let run_id = db.begin_run("test run").unwrap();

    let run = db.run_index.get_run(run_id).unwrap();
    assert_eq!(run.status, RunStatus::Active);
}

#[test]
fn test_end_run_marks_completed() {
    let db = create_test_db();

    let run_id = db.begin_run("test run").unwrap();
    db.end_run(run_id, RunStatus::Completed).unwrap();

    let run = db.run_index.get_run(run_id).unwrap();
    assert_eq!(run.status, RunStatus::Completed);
}

#[test]
fn test_end_run_with_failure() {
    let db = create_test_db();

    let run_id = db.begin_run("test run").unwrap();
    db.end_run(run_id, RunStatus::Failed).unwrap();

    let run = db.run_index.get_run(run_id).unwrap();
    assert_eq!(run.status, RunStatus::Failed);
}

#[test]
fn test_begin_run_persisted() {
    let db = create_test_db();

    let run_id = db.begin_run("test run").unwrap();

    drop(db);
    let db = reopen_database();

    let run = db.run_index.get_run(run_id).unwrap();
    assert_eq!(run.name, "test run");
}
```

### 7.2 Status Transitions (`run_status_transitions_tests.rs`)

```rust
#[test]
fn test_valid_status_transitions() {
    // Active → Completed: valid
    // Active → Failed: valid
    // Active → Orphaned: valid (system only)
    // Completed → anything: invalid
    // Failed → anything: invalid

    let db = create_test_db();
    let run_id = db.begin_run("test").unwrap();

    // Active → Completed
    db.end_run(run_id, RunStatus::Completed).unwrap();

    // Completed → anything: should fail
    let result = db.end_run(run_id, RunStatus::Failed);
    assert!(result.is_err());
}

#[test]
fn test_cannot_transition_from_terminal() {
    let db = create_test_db();

    let run_id = db.begin_run("test").unwrap();
    db.end_run(run_id, RunStatus::Completed).unwrap();

    // Cannot transition from Completed
    assert!(db.end_run(run_id, RunStatus::Active).is_err());
    assert!(db.end_run(run_id, RunStatus::Failed).is_err());
}
```

### 7.3 Orphan Detection (`run_orphan_detection_tests.rs`)

```rust
#[test]
fn test_detect_orphaned_after_crash() {
    let db = create_test_db();

    let run_id = db.begin_run("will be orphaned").unwrap();
    // Run is Active, no end_run called

    simulate_crash(&db);

    let db = reopen_database();

    // Run should be detected as orphaned
    let orphaned = db.detect_orphaned_runs();
    assert!(orphaned.contains(&run_id));

    let run = db.run_index.get_run(run_id).unwrap();
    assert_eq!(run.status, RunStatus::Orphaned);
}

#[test]
fn test_completed_runs_not_orphaned() {
    let db = create_test_db();

    let run_id = db.begin_run("completed run").unwrap();
    db.end_run(run_id, RunStatus::Completed).unwrap();

    simulate_crash(&db);

    let db = reopen_database();

    let orphaned = db.detect_orphaned_runs();
    assert!(!orphaned.contains(&run_id));
}

#[test]
fn test_multiple_orphaned_runs() {
    let db = create_test_db();

    let run1 = db.begin_run("orphan1").unwrap();
    let run2 = db.begin_run("orphan2").unwrap();
    let run3 = db.begin_run("completed").unwrap();
    db.end_run(run3, RunStatus::Completed).unwrap();

    simulate_crash(&db);

    let db = reopen_database();

    let orphaned = db.detect_orphaned_runs();
    assert!(orphaned.contains(&run1));
    assert!(orphaned.contains(&run2));
    assert!(!orphaned.contains(&run3));
}
```

### 7.4 Replay Run (`run_replay_tests.rs`)

```rust
#[test]
fn test_replay_run_returns_view() {
    let db = create_test_db();
    let run_id = db.begin_run("test").unwrap();

    db.kv.put(&run_id, "key1", "v1").unwrap();
    db.kv.put(&run_id, "key2", "v2").unwrap();

    let view = db.replay_run(run_id).unwrap();

    assert_eq!(view.kv_state.get("key1"), Some(&"v1".to_string()));
    assert_eq!(view.kv_state.get("key2"), Some(&"v2".to_string()));
}

#[test]
fn test_replay_run_isolates_runs() {
    let db = create_test_db();

    let run1 = db.begin_run("run1").unwrap();
    let run2 = db.begin_run("run2").unwrap();

    db.kv.put(&run1, "key", "run1_value").unwrap();
    db.kv.put(&run2, "key", "run2_value").unwrap();

    let view1 = db.replay_run(run1).unwrap();
    let view2 = db.replay_run(run2).unwrap();

    // Views should be isolated
    assert_eq!(view1.kv_state.get("key"), Some(&"run1_value".to_string()));
    assert_eq!(view2.kv_state.get("key"), Some(&"run2_value".to_string()));
}

#[test]
fn test_replay_run_after_recovery() {
    let db = create_test_db();
    let run_id = db.begin_run("test").unwrap();

    db.kv.put(&run_id, "key", "value").unwrap();
    db.end_run(run_id, RunStatus::Completed).unwrap();

    drop(db);
    let db = reopen_database();

    let view = db.replay_run(run_id).unwrap();
    assert_eq!(view.kv_state.get("key"), Some(&"value".to_string()));
}
```

### 7.5 Diff Runs (`run_diff_tests.rs`)

```rust
#[test]
fn test_diff_runs_shows_added() {
    let db = create_test_db();

    let run1 = db.begin_run("run1").unwrap();
    db.kv.put(&run1, "key1", "v1").unwrap();
    db.end_run(run1, RunStatus::Completed).unwrap();

    let run2 = db.begin_run("run2").unwrap();
    db.kv.put(&run2, "key1", "v1").unwrap();
    db.kv.put(&run2, "key2", "v2").unwrap(); // Added
    db.end_run(run2, RunStatus::Completed).unwrap();

    let diff = db.diff_runs(run1, run2).unwrap();

    assert!(diff.added_keys.contains(&"key2".to_string()));
}

#[test]
fn test_diff_runs_shows_removed() {
    let db = create_test_db();

    let run1 = db.begin_run("run1").unwrap();
    db.kv.put(&run1, "key1", "v1").unwrap();
    db.kv.put(&run1, "key2", "v2").unwrap();
    db.end_run(run1, RunStatus::Completed).unwrap();

    let run2 = db.begin_run("run2").unwrap();
    db.kv.put(&run2, "key1", "v1").unwrap();
    // key2 not present (removed)
    db.end_run(run2, RunStatus::Completed).unwrap();

    let diff = db.diff_runs(run1, run2).unwrap();

    assert!(diff.removed_keys.contains(&"key2".to_string()));
}

#[test]
fn test_diff_runs_shows_modified() {
    let db = create_test_db();

    let run1 = db.begin_run("run1").unwrap();
    db.kv.put(&run1, "key", "old_value").unwrap();
    db.end_run(run1, RunStatus::Completed).unwrap();

    let run2 = db.begin_run("run2").unwrap();
    db.kv.put(&run2, "key", "new_value").unwrap();
    db.end_run(run2, RunStatus::Completed).unwrap();

    let diff = db.diff_runs(run1, run2).unwrap();

    assert!(diff.modified_keys.iter().any(|(k, old, new)|
        k == "key" && old == "old_value" && new == "new_value"));
}
```

---

## Tier 8: Storage Stabilization

### 8.1 PrimitiveStorageExt Trait (`primitive_storage_ext_tests.rs`)

```rust
#[test]
fn test_all_primitives_implement_ext() {
    // KV
    let _: &dyn PrimitiveStorageExt = &KvStore::default();
    // JSON
    let _: &dyn PrimitiveStorageExt = &JsonStore::default();
    // Event
    let _: &dyn PrimitiveStorageExt = &EventLog::default();
    // State
    let _: &dyn PrimitiveStorageExt = &StateCell::default();
    // Trace
    let _: &dyn PrimitiveStorageExt = &TraceStore::default();
    // Run
    let _: &dyn PrimitiveStorageExt = &RunIndex::default();
}

#[test]
fn test_type_tag_unique() {
    let tags = [
        KvStore::TYPE_TAG,
        JsonStore::TYPE_TAG,
        EventLog::TYPE_TAG,
        StateCell::TYPE_TAG,
        TraceStore::TYPE_TAG,
        RunIndex::TYPE_TAG,
    ];

    let unique: HashSet<_> = tags.iter().collect();
    assert_eq!(unique.len(), tags.len(),
        "Primitive TYPE_TAGs must be unique");
}

#[test]
fn test_to_wal_entries_produces_valid_entries() {
    let kv = KvStore::default();
    let ops = vec![KvOp::Put("key".into(), "value".into())];

    let entries = kv.to_wal_entries(&ops);

    for entry in entries {
        assert!(entry.validate_crc().is_ok());
        assert_eq!(entry.entry_type() as u8 & 0xF0, 0x10); // KV range
    }
}

#[test]
fn test_snapshot_roundtrip() {
    let mut kv = KvStore::default();
    kv.put("key", "value");

    let blob = kv.to_snapshot_blob();
    let recovered = KvStore::from_snapshot_blob(&blob).unwrap();

    assert_eq!(recovered.get("key"), Some("value"));
}
```

### 8.2 Primitive Registry (`primitive_registry_tests.rs`)

```rust
#[test]
fn test_register_primitive() {
    let mut registry = PrimitiveRegistry::new();

    registry.register::<KvStore>();
    registry.register::<JsonStore>();

    assert!(registry.get(KvStore::TYPE_TAG).is_some());
    assert!(registry.get(JsonStore::TYPE_TAG).is_some());
}

#[test]
fn test_registry_dispatch_by_type_tag() {
    let mut registry = PrimitiveRegistry::new();
    registry.register::<KvStore>();

    let entry = WalEntry::new(WalEntryType::KvPut, tx_id, payload);

    // Should dispatch to KvStore
    let primitive = registry.get_mut(0x10).unwrap();
    primitive.apply_wal_entry(&entry).unwrap();
}

#[test]
fn test_unknown_type_tag_handled() {
    let registry = PrimitiveRegistry::new();

    // Unknown type tag
    assert!(registry.get(0xFE).is_none());
}
```

### 8.3 Storage Extension (`storage_extension_tests.rs`)

```rust
#[test]
fn test_new_primitive_via_trait() {
    // Simulate adding VectorStore (M8)
    struct MockVectorStore;

    impl PrimitiveStorageExt for MockVectorStore {
        const TYPE_TAG: u8 = 0x70;

        fn to_wal_entries(&self, _ops: &[Op]) -> Vec<WalEntry> {
            vec![]
        }

        fn apply_wal_entry(&mut self, _entry: &WalEntry) -> Result<()> {
            Ok(())
        }

        fn to_snapshot_blob(&self) -> Vec<u8> {
            vec![]
        }

        fn from_snapshot_blob(_blob: &[u8]) -> Result<Self> {
            Ok(MockVectorStore)
        }
    }

    // Should be registrable
    let mut registry = PrimitiveRegistry::new();
    registry.register::<MockVectorStore>();

    assert!(registry.get(0x70).is_some());
}

#[test]
fn test_extension_does_not_break_existing() {
    let db = create_test_db();

    // Add data with existing primitives
    db.kv.put(&run_id, "key", "value").unwrap();

    // Simulate adding new primitive (no actual implementation needed)
    // Recovery should still work for existing primitives

    drop(db);
    let db = reopen_database();

    assert!(db.kv.get(&run_id, "key").is_some());
}
```

### 8.4 WAL Type Allocation (`wal_type_allocation_tests.rs`)

```rust
#[test]
fn test_type_ranges_non_overlapping() {
    // Verify ranges don't overlap
    let ranges = [
        (0x00, 0x0F, "Core"),
        (0x10, 0x1F, "KV"),
        (0x20, 0x2F, "JSON"),
        (0x30, 0x3F, "Event"),
        (0x40, 0x4F, "State"),
        (0x50, 0x5F, "Trace"),
        (0x60, 0x6F, "Run"),
        (0x70, 0x7F, "Vector (M8)"),
    ];

    for i in 0..ranges.len() {
        for j in (i+1)..ranges.len() {
            assert!(ranges[i].1 < ranges[j].0,
                "Ranges {} and {} overlap", ranges[i].2, ranges[j].2);
        }
    }
}

#[test]
fn test_reserved_range_available() {
    // 0x80-0xFF should be available for future primitives
    for type_byte in 0x80..=0xFF {
        let result = WalEntryType::try_from(type_byte);
        // Should either return Unknown or an error
        // Should NOT conflict with existing types
    }
}
```

---

## Tier 9: Property-Based/Fuzzing

### 9.1 Recovery Fuzzing (`recovery_fuzzing_tests.rs`)

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn fuzz_recovery_never_panics(
        ops in prop::collection::vec(any::<FuzzOp>(), 1..100),
        crash_point in 0usize..100,
    ) {
        let db = create_test_db();

        for (i, op) in ops.iter().enumerate() {
            if i == crash_point {
                simulate_crash(&db);
                break;
            }
            apply_fuzz_op(&db, op);
        }

        // Recovery should not panic
        let result = std::panic::catch_unwind(|| {
            reopen_database()
        });

        prop_assert!(result.is_ok());
    }

    #[test]
    fn fuzz_recovery_deterministic(
        ops in prop::collection::vec(any::<FuzzOp>(), 1..50),
    ) {
        let db = create_test_db();

        for op in &ops {
            apply_fuzz_op(&db, op);
        }

        let state1 = capture_state(&db);

        drop(db);
        let db = reopen_database();

        let state2 = capture_state(&db);

        prop_assert_eq!(state1, state2);
    }
}
```

### 9.2 WAL Fuzzing (`wal_fuzzing_tests.rs`)

```rust
proptest! {
    #[test]
    fn fuzz_wal_corruption_handled(
        valid_ops in prop::collection::vec(any::<FuzzOp>(), 1..20),
        corrupt_offset in 0usize..10000,
        corrupt_bytes in prop::collection::vec(any::<u8>(), 1..10),
    ) {
        let db = create_test_db();

        for op in &valid_ops {
            apply_fuzz_op(&db, op);
        }

        // Corrupt WAL at random offset
        corrupt_wal_at(&db, corrupt_offset, &corrupt_bytes);

        drop(db);

        // Recovery should handle corruption gracefully
        let result = std::panic::catch_unwind(|| {
            reopen_database()
        });

        prop_assert!(result.is_ok());
    }
}
```

### 9.3 Snapshot Fuzzing (`snapshot_fuzzing_tests.rs`)

```rust
proptest! {
    #[test]
    fn fuzz_snapshot_corruption_handled(
        ops in prop::collection::vec(any::<FuzzOp>(), 1..20),
        corrupt_bytes in prop::collection::vec(any::<u8>(), 1..50),
    ) {
        let db = create_test_db();

        for op in &ops {
            apply_fuzz_op(&db, op);
        }

        let snapshot_path = db.create_snapshot().unwrap();

        // Corrupt snapshot
        corrupt_file_with_bytes(&snapshot_path, &corrupt_bytes);

        drop(db);

        // Recovery should fall back to WAL
        let db = reopen_database();

        prop_assert!(db.is_healthy());
    }
}
```

---

## Tier 10: Stress & Scale

### 10.1 Large WAL Recovery (`recovery_large_wal_tests.rs`)

```rust
#[test]
#[ignore] // Slow
fn test_recovery_100k_entries() {
    let db = create_test_db();

    for i in 0..100_000 {
        db.kv.put(&run_id, &format!("key_{}", i), "value").unwrap();
    }

    let start = Instant::now();
    drop(db);
    let db = reopen_database();
    let elapsed = start.elapsed();

    assert!(elapsed < Duration::from_secs(5),
        "Recovery took too long: {:?}", elapsed);

    // Verify all data present
    assert_eq!(db.kv.list(&run_id).count(), 100_000);
}

#[test]
#[ignore]
fn test_recovery_1m_entries_with_snapshot() {
    let db = create_test_db();

    for i in 0..1_000_000 {
        db.kv.put(&run_id, &format!("key_{}", i), "value").unwrap();
        if i % 100_000 == 0 {
            db.create_snapshot().unwrap();
        }
    }

    let start = Instant::now();
    drop(db);
    let db = reopen_database();
    let elapsed = start.elapsed();

    assert!(elapsed < Duration::from_secs(10),
        "Recovery with snapshot took too long: {:?}", elapsed);
}
```

### 10.2 Large Snapshot (`snapshot_large_state_tests.rs`)

```rust
#[test]
#[ignore]
fn test_snapshot_100mb_state() {
    let db = create_test_db();

    // Create ~100MB of data
    let large_value = "x".repeat(10_000); // 10KB
    for i in 0..10_000 {
        db.kv.put(&run_id, &format!("key_{}", i), &large_value).unwrap();
    }

    let start = Instant::now();
    let snapshot_path = db.create_snapshot().unwrap();
    let elapsed = start.elapsed();

    assert!(elapsed < Duration::from_secs(5),
        "Snapshot creation too slow: {:?}", elapsed);

    // Verify snapshot can be loaded
    let snapshot = Snapshot::load(&snapshot_path).unwrap();
    assert!(snapshot.is_valid());
}
```

### 10.3 Concurrent Recovery (`concurrent_recovery_tests.rs`)

```rust
#[test]
#[ignore]
fn test_concurrent_writes_during_snapshot() {
    let db = Arc::new(create_test_db());

    // Writer thread
    let db_writer = db.clone();
    let writer = std::thread::spawn(move || {
        for i in 0..10_000 {
            db_writer.kv.put(&run_id, &format!("key_{}", i), "value").unwrap();
        }
    });

    // Snapshot thread
    let db_snapshot = db.clone();
    let snapshotter = std::thread::spawn(move || {
        for _ in 0..10 {
            db_snapshot.create_snapshot().unwrap();
            std::thread::sleep(Duration::from_millis(100));
        }
    });

    writer.join().unwrap();
    snapshotter.join().unwrap();

    // Database should be consistent
    assert!(db.validate_invariants().is_ok());
}
```

---

## Tier 11: Non-Regression

### 11.1 M6 Regression (`m6_regression_tests.rs`)

```rust
#[test]
fn test_m6_search_still_works() {
    let db = create_test_db();

    db.kv.put(&run_id, "key", "searchable value").unwrap();

    let req = SearchRequest::new(run_id, "searchable");
    let result = db.kv.search(&req).unwrap();

    assert!(!result.hits.is_empty());
}

#[test]
fn test_m6_hybrid_search_still_works() {
    let db = create_test_db();

    db.kv.put(&run_id, "key", "test").unwrap();
    db.json.create(&run_id, json!({"data": "test"})).unwrap();

    let req = SearchRequest::new(run_id, "test");
    let result = db.hybrid().search(&req).unwrap();

    assert!(result.hits.len() >= 2);
}

#[test]
fn test_m6_indexing_still_works() {
    let db = create_test_db();

    db.enable_search_index(PrimitiveKind::Kv).unwrap();
    db.kv.put(&run_id, "key", "indexed value").unwrap();

    let req = SearchRequest::new(run_id, "indexed");
    let result = db.kv.search(&req).unwrap();

    assert!(!result.hits.is_empty());
}
```

### 11.2 Operation Latency (`operation_latency_tests.rs`)

```rust
#[test]
fn test_kv_put_latency_with_wal() {
    let db = create_test_db();

    let latencies: Vec<_> = (0..1000).map(|i| {
        let start = Instant::now();
        db.kv.put(&run_id, &format!("key_{}", i), "value").unwrap();
        start.elapsed()
    }).collect();

    let mean_ns = latencies.iter().map(|d| d.as_nanos()).sum::<u128>() / 1000;

    // M7 allows slight increase for WAL: < 10µs (vs M6 < 8µs)
    assert!(mean_ns < 10_000,
        "KV put latency regression: {} ns", mean_ns);
}

#[test]
fn test_json_set_latency_with_wal() {
    let db = create_test_db();
    let doc_id = db.json.create(&run_id, json!({"x": 0})).unwrap();

    let latencies: Vec<_> = (0..1000).map(|i| {
        let start = Instant::now();
        db.json.set(&run_id, &doc_id, &JsonPath::root(), json!({"x": i})).unwrap();
        start.elapsed()
    }).collect();

    let mean_ns = latencies.iter().map(|d| d.as_nanos()).sum::<u128>() / 1000;

    // M7 allows slight increase: < 250µs (vs M6 < 200µs)
    assert!(mean_ns < 250_000,
        "JSON set latency regression: {} ns", mean_ns);
}

#[test]
fn test_event_append_latency_with_wal() {
    let db = create_test_db();

    let latencies: Vec<_> = (0..1000).map(|_| {
        let start = Instant::now();
        db.event.append(&run_id, "test.event", json!({})).unwrap();
        start.elapsed()
    }).collect();

    let mean_ns = latencies.iter().map(|d| d.as_nanos()).sum::<u128>() / 1000;

    // M7 allows slight increase: < 15µs (vs M6 < 10µs)
    assert!(mean_ns < 15_000,
        "Event append latency regression: {} ns", mean_ns);
}
```

---

## Tier 12: Spec Conformance

### 12.1 Spec Conformance (`spec_conformance_tests.rs`)

```rust
// Recovery Invariants (M7_ARCHITECTURE.md Section 2)

#[test]
fn test_spec_r1_deterministic() {
    // R1: Same WAL → same state every replay
    // (Covered by recovery_determinism_tests.rs)
}

#[test]
fn test_spec_r2_idempotent() {
    // R2: replay(replay(S,WAL),WAL) = replay(S,WAL)
    // (Covered by recovery_idempotent_tests.rs)
}

#[test]
fn test_spec_r3_prefix_consistent() {
    // R3: Recover prefix of committed transactions
    // (Covered by recovery_prefix_tests.rs)
}

#[test]
fn test_spec_r4_never_invents() {
    // R4: Only data explicitly written appears
    // (Covered by recovery_no_invent_tests.rs)
}

#[test]
fn test_spec_r5_never_drops_committed() {
    // R5: Committed data survives any single crash
    // (Covered by recovery_no_drop_committed.rs)
}

#[test]
fn test_spec_r6_may_drop_uncommitted() {
    // R6: Incomplete transactions may vanish
    // (Covered by recovery_may_drop_uncommitted.rs)
}

// Replay Invariants

#[test]
fn test_spec_p1_pure_function() {
    // P1: fn(run_id, event_log) → ReadOnlyView
}

#[test]
fn test_spec_p2_side_effect_free() {
    // P2: Does NOT mutate any persistent state
}

#[test]
fn test_spec_p3_derived_view() {
    // P3: Computes view, does NOT reconstruct state
}

#[test]
fn test_spec_p4_ephemeral() {
    // P4: Result is ephemeral, discarded after use
}

#[test]
fn test_spec_p5_deterministic() {
    // P5: Same inputs → identical view
}

#[test]
fn test_spec_p6_idempotent() {
    // P6: Safe to call multiple times
}

// Snapshot System

#[test]
fn test_spec_snapshot_magic() {
    // Snapshot magic number is "INMEMSNP"
}

#[test]
fn test_spec_snapshot_crc32() {
    // Snapshot has CRC32 footer
}

#[test]
fn test_spec_snapshot_atomic_write() {
    // Snapshot uses temp file + rename
}

// WAL System

#[test]
fn test_spec_wal_entry_envelope() {
    // WAL entry: length + type + version + txid + payload + crc32
}

#[test]
fn test_spec_wal_type_registry() {
    // Type ranges: 0x00-0x0F core, 0x10-0x1F KV, etc.
}

#[test]
fn test_spec_wal_transaction_framing() {
    // Transactions framed with TxBegin/TxCommit
}

// Storage Stabilization

#[test]
fn test_spec_primitive_storage_ext() {
    // PrimitiveStorageExt trait exists with required methods
}

#[test]
fn test_spec_primitive_registry() {
    // PrimitiveRegistry allows dynamic registration
}
```

---

## Test Utilities (`main.rs`)

```rust
//! M7 Comprehensive Test Suite
//!
//! Tests for Durability, Snapshots, Replay & Storage Stabilization.
//!
//! ## Test Tier Structure
//!
//! - **Tier 1: Recovery Invariants** (R1-R6, sacred)
//! - **Tier 2: Replay Invariants** (P1-P6, pure function guarantees)
//! - **Tier 3: Snapshot System** (format, CRC, atomic write)
//! - **Tier 4: WAL System** (format, CRC, framing, types)
//! - **Tier 5: Crash Scenarios** (comprehensive crash testing)
//! - **Tier 6: Cross-Primitive Atomicity** (all-or-nothing)
//! - **Tier 7: Run Lifecycle** (begin, end, orphan, replay, diff)
//! - **Tier 8: Storage Stabilization** (extension traits, registry)
//! - **Tier 9: Property-Based/Fuzzing** (random crash/corruption)
//! - **Tier 10: Stress/Scale** (large WAL, large snapshot)
//! - **Tier 11: Non-Regression** (M6 maintained)
//! - **Tier 12: Spec Conformance** (spec → test)
//!
//! ## Running Tests
//!
//! ```bash
//! # Run all M7 comprehensive tests
//! cargo test --test m7_comprehensive
//!
//! # Run only recovery invariants (highest priority)
//! cargo test --test m7_comprehensive recovery
//!
//! # Run crash scenario tests
//! cargo test --test m7_comprehensive crash
//!
//! # Run property-based tests
//! cargo test --test m7_comprehensive fuzz
//!
//! # Run stress tests (slow, opt-in)
//! cargo test --test m7_comprehensive stress -- --ignored
//! ```

// Utilities
mod test_utils;

// Tier 1: Recovery Invariants (R1-R6)
mod recovery_determinism_tests;
mod recovery_idempotent_tests;
mod recovery_prefix_tests;
mod recovery_no_invent_tests;
mod recovery_no_drop_committed;
mod recovery_may_drop_uncommitted;

// Tier 2: Replay Invariants (P1-P6)
mod replay_pure_function_tests;
mod replay_side_effect_tests;
mod replay_derived_view_tests;
mod replay_ephemeral_tests;
mod replay_determinism_tests;
mod replay_idempotent_tests;

// Tier 3: Snapshot System
mod snapshot_format_tests;
mod snapshot_crc_tests;
mod snapshot_atomic_write_tests;
mod snapshot_discovery_tests;
mod snapshot_fallback_tests;

// Tier 4: WAL System
mod wal_entry_format_tests;
mod wal_crc_validation_tests;
mod wal_transaction_framing_tests;
mod wal_entry_type_tests;
mod wal_truncation_tests;

// Tier 5: Crash Scenarios
mod crash_during_wal_write_tests;
mod crash_during_commit_tests;
mod crash_during_snapshot_tests;
mod crash_multi_primitive_tests;
mod crash_recovery_sequence_tests;

// Tier 6: Cross-Primitive Atomicity
mod atomic_multi_write_tests;
mod atomic_recovery_boundary_tests;
mod atomic_cross_primitive_tests;

// Tier 7: Run Lifecycle
mod run_begin_end_tests;
mod run_status_transitions_tests;
mod run_orphan_detection_tests;
mod run_replay_tests;
mod run_diff_tests;

// Tier 8: Storage Stabilization
mod primitive_storage_ext_tests;
mod primitive_registry_tests;
mod storage_extension_tests;
mod wal_type_allocation_tests;

// Tier 9: Property-Based/Fuzzing
mod recovery_fuzzing_tests;
mod wal_fuzzing_tests;
mod snapshot_fuzzing_tests;

// Tier 10: Stress & Scale (use #[ignore])
mod recovery_large_wal_tests;
mod snapshot_large_state_tests;
mod concurrent_recovery_tests;

// Tier 11: Non-Regression
mod m6_regression_tests;
mod operation_latency_tests;

// Tier 12: Spec Conformance
mod spec_conformance_tests;
```

---

## Test Utilities (`test_utils.rs`)

```rust
use in_mem_core::types::{RunId, TxId};
use in_mem_engine::Database;
use in_mem_primitives::*;
use in_mem_storage::*;
use std::collections::HashSet;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Create a test database with persistent storage
pub fn create_test_db() -> Database {
    Database::builder()
        .durability(DurabilityMode::Persistent)
        .open_temp()
        .expect("Failed to create test database")
}

/// Reopen database after simulated crash
pub fn reopen_database() -> Database {
    // Uses same path as create_test_db()
    Database::builder()
        .durability(DurabilityMode::Persistent)
        .open_existing()
        .expect("Failed to reopen database")
}

/// Create test run ID
pub fn test_run_id() -> RunId {
    RunId::new()
}

/// Capture database state for comparison
pub fn capture_state(db: &Database) -> StateSnapshot {
    StateSnapshot {
        kv: db.kv.snapshot(),
        json: db.json.snapshot(),
        event_count: db.event.count(&test_run_id()),
        // ... other primitives
    }
}

/// Hash state for comparison
pub fn hash_state(state: &StateSnapshot) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    state.hash(&mut hasher);
    hasher.finish()
}

/// Hash ReadOnlyView for comparison
pub fn hash_view(view: &ReadOnlyView) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    view.hash(&mut hasher);
    hasher.finish()
}

/// Simulate crash (drop without cleanup)
pub fn simulate_crash(db: &Database) {
    // Force drop without cleanup
    std::mem::forget(db.clone());
}

/// Corrupt file at offset
pub fn corrupt_file(path: &Path) {
    let mut file = OpenOptions::new()
        .read(true).write(true)
        .open(path).unwrap();
    file.seek(SeekFrom::Start(100)).unwrap();
    file.write_all(&[0xFF; 10]).unwrap();
}

/// Corrupt WAL entry
pub fn corrupt_wal_entry(wal_path: &Path, entry_num: usize) {
    // Implementation: find entry by number, corrupt its CRC
}

/// Corrupt last WAL entry
pub fn corrupt_last_wal_entry(db: &Database) {
    // Implementation
}

/// Execute operations and capture WAL
pub fn execute_and_capture_wal(ops: &[Op]) -> Vec<u8> {
    let db = create_test_db();
    for op in ops {
        apply_op(&db, op);
    }
    db.read_raw_wal()
}

/// Replay WAL and return state
pub fn replay_wal(wal_bytes: &[u8]) -> StateSnapshot {
    let db = Database::replay_from_wal(wal_bytes).unwrap();
    capture_state(&db)
}

/// Replay WAL onto existing state
pub fn replay_wal_onto(state: &StateSnapshot, wal_bytes: &[u8]) -> StateSnapshot {
    // Implementation
    state.clone()
}

/// Populate test data across primitives
pub fn populate_test_data(db: &Database) {
    let run_id = test_run_id();

    for i in 0..100 {
        db.kv.put(&run_id, &format!("key_{}", i), &format!("value_{}", i)).unwrap();
    }

    for i in 0..50 {
        db.json.create(&run_id, serde_json::json!({
            "name": format!("item_{}", i),
            "data": i
        })).unwrap();
    }

    for i in 0..50 {
        db.event.append(&run_id, "test.event",
            serde_json::json!({"num": i})).unwrap();
    }
}

/// Assert latency is within target
pub fn assert_latency_under(actual: Duration, target_micros: u64) {
    assert!(actual.as_micros() < target_micros as u128,
        "Latency {} µs exceeds target {} µs", actual.as_micros(), target_micros);
}

/// Get memory usage
pub fn get_memory_usage() -> usize {
    // Platform-specific implementation
    0
}

/// Operation for fuzzing
#[derive(Debug, Clone, arbitrary::Arbitrary)]
pub enum FuzzOp {
    KvPut(String, String),
    KvDelete(String),
    JsonCreate(serde_json::Value),
    EventAppend(String),
    BeginTx,
    Commit,
    Abort,
    CreateSnapshot,
}

/// Apply fuzz operation
pub fn apply_fuzz_op(db: &Database, op: &FuzzOp) {
    let run_id = test_run_id();
    match op {
        FuzzOp::KvPut(k, v) => { let _ = db.kv.put(&run_id, k, v); }
        FuzzOp::KvDelete(k) => { let _ = db.kv.delete(&run_id, k); }
        FuzzOp::JsonCreate(v) => { let _ = db.json.create(&run_id, v.clone()); }
        FuzzOp::EventAppend(t) => { let _ = db.event.append(&run_id, t, serde_json::json!({})); }
        FuzzOp::BeginTx => { let _ = db.begin_tx(); }
        FuzzOp::Commit => { let _ = db.commit(); }
        FuzzOp::Abort => { let _ = db.abort(); }
        FuzzOp::CreateSnapshot => { let _ = db.create_snapshot(); }
    }
}
```

---

## Implementation Priority

| Priority | Tier | Estimated Tests | Rationale |
|----------|------|-----------------|-----------|
| **P0** | Tier 1: Recovery Invariants | ~25 | Sacred R1-R6, must never break |
| **P0** | Tier 2: Replay Invariants | ~20 | Pure function guarantees P1-P6 |
| **P0** | Tier 5: Crash Scenarios | ~15 | Core durability validation |
| **P0** | Tier 6: Cross-Primitive Atomicity | ~10 | All-or-nothing guarantee |
| **P1** | Tier 3: Snapshot System | ~15 | Snapshot correctness |
| **P1** | Tier 4: WAL System | ~15 | WAL integrity |
| **P1** | Tier 7: Run Lifecycle | ~15 | Run management |
| **P1** | Tier 9: Fuzzing | ~10 | Catches edge cases |
| **P2** | Tier 8: Storage Stabilization | ~10 | Extension point validation |
| **P2** | Tier 11: Non-Regression | ~10 | M6 maintained |
| **P2** | Tier 12: Spec Conformance | ~20 | Spec coverage |
| **P3** | Tier 10: Stress & Scale | ~10 | Scale verification |

**Total: ~175 new tests**

---

## Dependencies

```toml
[dev-dependencies]
proptest = "1.4"          # Property-based testing
arbitrary = "1.3"         # Fuzz input generation
criterion = "0.5"         # Benchmarking
tempfile = "3.10"         # Temporary directories
```

---

## Success Criteria

1. **All Tier 1 tests pass** - Recovery invariants R1-R6 locked
2. **All Tier 2 tests pass** - Replay invariants P1-P6 locked
3. **Crash scenarios handled** - Database survives all crash patterns
4. **Snapshot correctness** - CRC validates, atomic write works
5. **WAL integrity** - CRC catches corruption, framing correct
6. **Cross-primitive atomicity** - All-or-nothing guaranteed
7. **Run lifecycle works** - Begin, end, orphan, replay, diff all work
8. **Storage extensible** - New primitives can be added via trait
9. **Fuzzing finds no violations** - 10,000+ random cases pass
10. **M6 not regressed** - Search still works, latency within bounds

---

## Notes

- These tests are **separate from unit tests** - they test recovery guarantees
- **Recovery invariants (R1-R6) are sacred** - Tier 1 tests must never fail
- **Replay invariants (P1-P6) guarantee purity** - No side effects
- **Crash tests should be comprehensive** - Every crash point tested
- **Fuzzing is mandatory** - Property-based tests catch what humans miss
- **Storage stabilization enables M8+** - Extension trait is the contract
- Run stress tests **before every release** - Find rare bugs early
- **WAL format frozen after M7** - Changes require migration

---

*End of M7 Comprehensive Test Plan*
