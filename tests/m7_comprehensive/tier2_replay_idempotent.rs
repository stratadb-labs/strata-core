//! Tier 2.6: P6 - Replay is Idempotent Tests
//!
//! **Invariant P6**: Safe to call multiple times.
//!
//! These tests verify:
//! - Multiple replays don't interfere
//! - Concurrent replays safe
//! - Interleaved replays of different runs

use crate::test_utils::*;
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_primitives::KVStore;
use std::sync::Arc;

/// P6: Multiple replays produce identical views
#[test]
fn test_p6_multiple_replays_identical() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "key", Value::String("value".into()))
        .unwrap();

    // Multiple sequential replays
    let mut states = Vec::new();
    for _ in 0..10 {
        states.push(CapturedState::capture(&test_db.db, &run_id));
    }

    // All views identical
    let first_hash = states[0].hash;
    for (i, state) in states.iter().enumerate() {
        assert_eq!(
            first_hash, state.hash,
            "P6 VIOLATED: Replay {} produced different view",
            i
        );
    }
}

/// P6: Concurrent replays safe
#[test]
fn test_p6_concurrent_replays_safe() {
    use std::thread;

    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    for i in 0..20 {
        kv.put(&run_id, &format!("k{}", i), Value::I64(i)).unwrap();
    }

    let db = test_db.db.clone();

    // Spawn many threads to replay concurrently
    let handles: Vec<_> = (0..20)
        .map(|_| {
            let db = db.clone();
            thread::spawn(move || CapturedState::capture(&db, &run_id))
        })
        .collect();

    let states: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // All views identical
    let first_hash = states[0].hash;
    for state in &states {
        assert_eq!(first_hash, state.hash);
    }
}

/// P6: Interleaved replays don't interfere
#[test]
fn test_p6_interleaved_replays() {
    let test_db = TestDb::new_in_memory();
    let run_id1 = test_db.run_id;
    let run_id2 = RunId::new();

    let kv = test_db.kv();

    kv.put(&run_id1, "key", Value::String("run1_value".into()))
        .unwrap();
    kv.put(&run_id2, "key", Value::String("run2_value".into()))
        .unwrap();

    // Interleaved captures
    let state1a = CapturedState::capture(&test_db.db, &run_id1);
    let state2 = CapturedState::capture(&test_db.db, &run_id2);
    let state1b = CapturedState::capture(&test_db.db, &run_id1);

    // run1 views should be identical
    assert_eq!(
        state1a.hash, state1b.hash,
        "P6 VIOLATED: Interleaved replays interfered"
    );

    // run2 should be different from run1
    assert_ne!(state1a.hash, state2.hash);
}

/// P6: Heavy concurrent replay load
#[test]
fn test_p6_heavy_concurrent_load() {
    use std::thread;

    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    for i in 0..50 {
        kv.put(&run_id, &format!("k{}", i), Value::I64(i)).unwrap();
    }

    let db = test_db.db.clone();

    // Many threads, many replays each
    let handles: Vec<_> = (0..10)
        .map(|_| {
            let db = db.clone();
            thread::spawn(move || {
                let mut hashes = Vec::new();
                for _ in 0..100 {
                    hashes.push(CapturedState::capture(&db, &run_id).hash);
                }
                hashes
            })
        })
        .collect();

    let all_hashes: Vec<Vec<u64>> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // All hashes from all threads should be identical
    let first_hash = all_hashes[0][0];
    for thread_hashes in &all_hashes {
        for hash in thread_hashes {
            assert_eq!(first_hash, *hash, "P6 VIOLATED: Heavy load caused variance");
        }
    }
}

/// P6: Replay doesn't affect subsequent replay
#[test]
fn test_p6_replay_doesnt_affect_subsequent() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "stable", Value::I64(42)).unwrap();

    // Many replays
    for i in 0..100 {
        let state = CapturedState::capture(&test_db.db, &run_id);
        assert!(
            state.kv_entries.contains_key("stable"),
            "P6 VIOLATED: Replay {} lost data",
            i
        );
    }

    // Final state unchanged
    let final_state = CapturedState::capture(&test_db.db, &run_id);
    assert!(final_state.kv_entries.contains_key("stable"));
}

/// P6: Multiple runs can be replayed in any order
#[test]
fn test_p6_any_order_replay() {
    let test_db = TestDb::new_in_memory();
    let run_ids: Vec<RunId> = (0..5).map(|_| RunId::new()).collect();

    let kv = test_db.kv();

    for (i, run_id) in run_ids.iter().enumerate() {
        kv.put(run_id, "key", Value::I64(i as i64)).unwrap();
    }

    // Replay in forward order
    let forward_hashes: Vec<_> = run_ids
        .iter()
        .map(|rid| CapturedState::capture(&test_db.db, rid).hash)
        .collect();

    // Replay in reverse order
    let reverse_hashes: Vec<_> = run_ids
        .iter()
        .rev()
        .map(|rid| CapturedState::capture(&test_db.db, rid).hash)
        .collect();

    // Each run's hash should be consistent regardless of order
    for (i, rid) in run_ids.iter().enumerate() {
        let hash1 = forward_hashes[i];
        let hash2 = reverse_hashes[run_ids.len() - 1 - i];
        assert_eq!(
            hash1, hash2,
            "P6 VIOLATED: Order affected result for run {}",
            i
        );
    }
}

/// P6: Rapid alternating replays
#[test]
fn test_p6_rapid_alternating() {
    let test_db = TestDb::new_in_memory();
    let run_id1 = test_db.run_id;
    let run_id2 = RunId::new();

    let kv = test_db.kv();
    kv.put(&run_id1, "key", Value::String("v1".into())).unwrap();
    kv.put(&run_id2, "key", Value::String("v2".into())).unwrap();

    let expected_hash1 = CapturedState::capture(&test_db.db, &run_id1).hash;
    let expected_hash2 = CapturedState::capture(&test_db.db, &run_id2).hash;

    // Rapid alternation
    for _ in 0..100 {
        let h1 = CapturedState::capture(&test_db.db, &run_id1).hash;
        let h2 = CapturedState::capture(&test_db.db, &run_id2).hash;
        assert_eq!(h1, expected_hash1);
        assert_eq!(h2, expected_hash2);
    }
}

/// P6: Replay after write is independent
#[test]
fn test_p6_replay_after_write_independent() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "key1", Value::I64(1)).unwrap();

    let state1 = CapturedState::capture(&test_db.db, &run_id);

    // Write more
    kv.put(&run_id, "key2", Value::I64(2)).unwrap();

    // New replay should include new data
    let state2 = CapturedState::capture(&test_db.db, &run_id);

    // But original replay's view concept is still valid
    // (Views are computed fresh each time)
    assert_ne!(state1.hash, state2.hash, "New write should change view");
    assert!(state2.kv_entries.contains_key("key1"));
    assert!(state2.kv_entries.contains_key("key2"));
}
