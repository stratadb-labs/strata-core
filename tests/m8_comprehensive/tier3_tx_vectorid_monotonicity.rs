//! T4: VectorId Monotonicity Across Crashes Tests
//!
//! Invariant T4: After crash recovery, new VectorIds must be > all previous IDs.

use crate::test_utils::*;

/// Test VectorId monotonicity across crash
#[test]
fn test_t4_vectorid_monotonic_across_crash() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let max_id_before_crash;
    {
        let vector = test_db.vector();
        vector
            .create_collection(run_id, "embeddings", config_minilm())
            .unwrap();

        // Insert vectors and track max ID
        for i in 0..100 {
            vector
                .insert(
                    run_id,
                    "embeddings",
                    &format!("key_{}", i),
                    &seeded_random_vector(384, i as u64),
                    None,
                )
                .unwrap();
        }

        max_id_before_crash = (0..100)
            .map(|i| {
                vector
                    .get(run_id, "embeddings", &format!("key_{}", i))
                    .unwrap()
                    .unwrap()
                    .value.vector_id()
                    .as_u64()
            })
            .max()
            .unwrap();
    }

    // Simulate crash and recovery
    test_db.reopen();

    let vector = test_db.vector();

    // Insert new vectors after recovery
    for i in 100..110 {
        vector
            .insert(
                run_id,
                "embeddings",
                &format!("key_{}", i),
                &seeded_random_vector(384, i as u64),
                None,
            )
            .unwrap();
    }

    // All new IDs must be > max_id_before_crash
    for i in 100..110 {
        let id = vector
            .get(run_id, "embeddings", &format!("key_{}", i))
            .unwrap()
            .unwrap()
            .value.vector_id()
            .as_u64();

        assert!(
            id > max_id_before_crash,
            "T4 VIOLATED: Post-crash VectorId {} <= pre-crash max {}",
            id,
            max_id_before_crash
        );
    }
}

/// Test max_id preserved across checkpoint
#[test]
fn test_t4_max_id_in_snapshot() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let max_id_at_checkpoint;
    {
        let vector = test_db.vector();
        vector
            .create_collection(run_id, "embeddings", config_minilm())
            .unwrap();

        for i in 0..50 {
            vector
                .insert(
                    run_id,
                    "embeddings",
                    &format!("key_{}", i),
                    &seeded_random_vector(384, i as u64),
                    None,
                )
                .unwrap();
        }

        max_id_at_checkpoint = (0..50)
            .map(|i| {
                vector
                    .get(run_id, "embeddings", &format!("key_{}", i))
                    .unwrap()
                    .unwrap()
                    .value.vector_id()
                    .as_u64()
            })
            .max()
            .unwrap();
    }

    // Simulate checkpoint
    test_db.reopen();

    {
        let vector = test_db.vector();
        // More inserts after checkpoint
        for i in 50..60 {
            vector
                .insert(
                    run_id,
                    "embeddings",
                    &format!("key_{}", i),
                    &seeded_random_vector(384, i as u64),
                    None,
                )
                .unwrap();
        }
    }

    // Recover from checkpoint + WAL
    test_db.reopen();

    let vector = test_db.vector();

    // Get max ID after recovery (should include post-checkpoint inserts)
    let max_id_after_recovery = (0..60)
        .filter_map(|i| {
            vector
                .get(run_id, "embeddings", &format!("key_{}", i))
                .unwrap()
                .map(|e| e.value.vector_id().as_u64())
        })
        .max()
        .unwrap();

    // Insert new vector
    vector
        .insert(
            run_id,
            "embeddings",
            "new_key",
            &seeded_random_vector(384, 999),
            None,
        )
        .unwrap();

    let new_id = vector
        .get(run_id, "embeddings", "new_key")
        .unwrap()
        .unwrap()
        .value.vector_id()
        .as_u64();

    assert!(
        new_id > max_id_after_recovery,
        "T4 VIOLATED: New VectorId {} <= max after recovery {}",
        new_id,
        max_id_after_recovery
    );
}

/// Test free_slots preserved across checkpoint
#[test]
fn test_t4_free_slots_in_snapshot() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let ids_before_delete: Vec<u64>;
    {
        let vector = test_db.vector();
        vector
            .create_collection(run_id, "embeddings", config_minilm())
            .unwrap();

        // Insert and delete to create free slots
        for i in 0..20 {
            vector
                .insert(
                    run_id,
                    "embeddings",
                    &format!("key_{}", i),
                    &seeded_random_vector(384, i as u64),
                    None,
                )
                .unwrap();
        }

        ids_before_delete = (0..20)
            .map(|i| {
                vector
                    .get(run_id, "embeddings", &format!("key_{}", i))
                    .unwrap()
                    .unwrap()
                    .value.vector_id()
                    .as_u64()
            })
            .collect();

        // Delete first 10 (creates free slots)
        for i in 0..10 {
            vector.delete(run_id, "embeddings", &format!("key_{}", i)).unwrap();
        }
    }

    // Simulate checkpoint + recover
    test_db.reopen();

    let vector = test_db.vector();

    // Insert new vectors - should get new IDs (not reuse deleted ones)
    for i in 20..25 {
        vector
            .insert(
                run_id,
                "embeddings",
                &format!("key_{}", i),
                &seeded_random_vector(384, i as u64),
                None,
            )
            .unwrap();
    }

    // New IDs should all be > max(original IDs)
    let max_original_id = ids_before_delete.iter().max().unwrap();
    for i in 20..25 {
        let new_id = vector
            .get(run_id, "embeddings", &format!("key_{}", i))
            .unwrap()
            .unwrap()
            .value.vector_id()
            .as_u64();

        assert!(
            new_id > *max_original_id,
            "T4 VIOLATED: New VectorId {} <= max original {}",
            new_id,
            max_original_id
        );

        // Also verify no reuse of deleted IDs
        for &deleted_id in &ids_before_delete[0..10] {
            assert_ne!(
                new_id, deleted_id,
                "T4 VIOLATED: Deleted VectorId {} was reused",
                deleted_id
            );
        }
    }
}

/// Test multiple crash cycles maintain monotonicity
#[test]
fn test_t4_multiple_crash_cycles() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let mut max_seen_id: u64 = 0;

    // Create collection
    {
        let vector = test_db.vector();
        vector
            .create_collection(run_id, "embeddings", config_minilm())
            .unwrap();
    }

    // Multiple crash cycles
    for cycle in 0..5 {
        let vector = test_db.vector();

        // Insert vectors
        for i in 0..10 {
            let key = format!("cycle_{}_{}", cycle, i);
            vector
                .insert(run_id, "embeddings", &key, &seeded_random_vector(384, (cycle * 10 + i) as u64), None)
                .unwrap();

            let id = vector.get(run_id, "embeddings", &key).unwrap().unwrap().value.vector_id().as_u64();

            assert!(
                id > max_seen_id,
                "T4 VIOLATED: Cycle {} got VectorId {} <= max seen {}",
                cycle,
                id,
                max_seen_id
            );

            max_seen_id = max_seen_id.max(id);
        }

        // Crash and recover
        if cycle < 4 {
            test_db.reopen();
        }
    }
}

/// Test delete and insert same key across crash
#[test]
fn test_t4_delete_insert_same_key_across_crash() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let original_id;
    {
        let vector = test_db.vector();
        vector
            .create_collection(run_id, "embeddings", config_minilm())
            .unwrap();

        // Insert and delete
        vector
            .insert(run_id, "embeddings", "key1", &random_vector(384), None)
            .unwrap();
        original_id = vector
            .get(run_id, "embeddings", "key1")
            .unwrap()
            .unwrap()
            .value.vector_id()
            .as_u64();
        vector.delete(run_id, "embeddings", "key1").unwrap();
    }

    // Crash
    test_db.reopen();

    let vector = test_db.vector();

    // Re-insert same key
    vector
        .insert(run_id, "embeddings", "key1", &random_vector(384), None)
        .unwrap();

    let new_id = vector
        .get(run_id, "embeddings", "key1")
        .unwrap()
        .unwrap()
        .value.vector_id()
        .as_u64();

    assert!(
        new_id > original_id,
        "T4 VIOLATED: Re-inserted key got VectorId {} <= original {}",
        new_id,
        original_id
    );
}
