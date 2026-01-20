//! Tier 12: Determinism - VectorId Assignment Determinism Tests

use crate::test_utils::*;

#[test]
fn test_vectorid_assignment_deterministic() {
    // Create two databases with identical operations
    let test_db1 = TestDb::new_strict();
    let test_db2 = TestDb::new_strict();

    let run_id1 = test_db1.run_id;
    let run_id2 = test_db2.run_id;

    let vector1 = test_db1.vector();
    let vector2 = test_db2.vector();

    vector1.create_collection(run_id1, "embeddings", config_minilm()).unwrap();
    vector2.create_collection(run_id2, "embeddings", config_minilm()).unwrap();

    // Perform identical operations in same order
    for i in 0..20 {
        let vec = seeded_random_vector(384, i as u64);
        vector1.insert(run_id1, "embeddings", &format!("key_{}", i), &vec, None).unwrap();
        vector2.insert(run_id2, "embeddings", &format!("key_{}", i), &vec, None).unwrap();
    }

    // VectorIds should be assigned identically
    for i in 0..20 {
        let r1 = vector1.get(run_id1, "embeddings", &format!("key_{}", i)).unwrap().unwrap();
        let r2 = vector2.get(run_id2, "embeddings", &format!("key_{}", i)).unwrap().unwrap();
        assert_eq!(r1.value.vector_id().as_u64(), r2.value.vector_id().as_u64(), "VectorId should be deterministic for key_{}", i);
    }
}

#[test]
fn test_vectorid_after_delete_deterministic() {
    let test_db1 = TestDb::new_strict();
    let test_db2 = TestDb::new_strict();

    let run_id1 = test_db1.run_id;
    let run_id2 = test_db2.run_id;

    let vector1 = test_db1.vector();
    let vector2 = test_db2.vector();

    vector1.create_collection(run_id1, "embeddings", config_minilm()).unwrap();
    vector2.create_collection(run_id2, "embeddings", config_minilm()).unwrap();

    // Insert, delete, insert more
    for i in 0..10 {
        let vec = seeded_random_vector(384, i as u64);
        vector1.insert(run_id1, "embeddings", &format!("key_{}", i), &vec, None).unwrap();
        vector2.insert(run_id2, "embeddings", &format!("key_{}", i), &vec, None).unwrap();
    }

    for i in 0..5 {
        vector1.delete(run_id1, "embeddings", &format!("key_{}", i)).unwrap();
        vector2.delete(run_id2, "embeddings", &format!("key_{}", i)).unwrap();
    }

    // Insert new vectors
    for i in 10..15 {
        let vec = seeded_random_vector(384, i as u64);
        vector1.insert(run_id1, "embeddings", &format!("key_{}", i), &vec, None).unwrap();
        vector2.insert(run_id2, "embeddings", &format!("key_{}", i), &vec, None).unwrap();
    }

    // New VectorIds should be deterministic
    for i in 10..15 {
        let r1 = vector1.get(run_id1, "embeddings", &format!("key_{}", i)).unwrap().unwrap();
        let r2 = vector2.get(run_id2, "embeddings", &format!("key_{}", i)).unwrap().unwrap();
        assert_eq!(r1.value.vector_id().as_u64(), r2.value.vector_id().as_u64(), "VectorId should be deterministic after deletes for key_{}", i);
    }
}

#[test]
fn test_vectorid_determinism_across_recovery() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let ids_before: Vec<u64>;
    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        for i in 0..20 {
            vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
        }

        ids_before = (0..20)
            .map(|i| vector.get(run_id, "embeddings", &format!("key_{}", i)).unwrap().unwrap().value.vector_id().as_u64())
            .collect();
    }

    test_db.reopen();

    let vector = test_db.vector();
    let ids_after: Vec<u64> = (0..20)
        .map(|i| vector.get(run_id, "embeddings", &format!("key_{}", i)).unwrap().unwrap().value.vector_id().as_u64())
        .collect();

    assert_eq!(ids_before, ids_after, "VectorIds should be preserved across recovery");
}
