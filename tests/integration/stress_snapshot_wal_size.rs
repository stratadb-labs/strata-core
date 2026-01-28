//! Tier 13: Stress - Snapshot and WAL Size Tests

use crate::common::*;

#[test]
fn test_wal_growth_with_operations() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

    let initial_wal_size = wal_size(&test_db.wal_path());

    // Insert many vectors
    for i in 0..100 {
        vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
    }

    let after_insert_wal_size = wal_size(&test_db.wal_path());
    assert!(after_insert_wal_size > initial_wal_size, "WAL should grow after inserts");

    // Delete some vectors
    for i in 0..50 {
        vector.delete(run_id, "embeddings", &format!("key_{}", i)).unwrap();
    }

    let after_delete_wal_size = wal_size(&test_db.wal_path());
    assert!(after_delete_wal_size > after_insert_wal_size, "WAL should grow after deletes");
}

#[test]
fn test_data_persistence_through_multiple_reopens() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        // Insert many vectors
        for i in 0..100 {
            vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
        }
    }

    // Reopen multiple times
    for round in 0..3 {
        test_db.reopen();
        let vector = test_db.vector();
        assert_eq!(vector.count(run_id, "embeddings").unwrap(), 100, "Round {} should have 100 vectors", round);
    }
}

#[test]
fn test_multiple_checkpoints() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        // Create multiple checkpoints with data changes
        for round in 0..5 {
            for i in 0..10 {
                vector.insert(
                    run_id,
                    "embeddings",
                    &format!("key_{}_{}", round, i),
                    &seeded_random_vector(384, (round * 10 + i) as u64),
                    None
                ).unwrap();
            }
        }
    }

    test_db.reopen();

    let vector = test_db.vector();
    assert_eq!(vector.count(run_id, "embeddings").unwrap(), 50);
}

#[test]
fn test_large_data_snapshot_recovery() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        // Insert before checkpoint
        for i in 0..200 {
            vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
        }
    }

    // Simulate checkpoint
    test_db.reopen();

    {
        let vector = test_db.vector();
        // More inserts after checkpoint
        for i in 200..300 {
            vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
        }
    }

    test_db.reopen();

    let vector = test_db.vector();
    assert_eq!(vector.count(run_id, "embeddings").unwrap(), 300);

    // Verify some vectors from before and after checkpoint
    for i in [0, 100, 199, 200, 250, 299].iter() {
        let result = vector.get(run_id, "embeddings", &format!("key_{}", i)).unwrap();
        assert!(result.is_some(), "key_{} should exist after recovery", i);
    }
}
