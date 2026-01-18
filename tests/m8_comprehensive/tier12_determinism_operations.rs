//! Tier 12: Determinism - Operation Order Determinism Tests

use crate::test_utils::*;

#[test]
fn test_insert_order_determinism() {
    let test_db1 = TestDb::new_strict();
    let test_db2 = TestDb::new_strict();

    let run_id1 = test_db1.run_id;
    let run_id2 = test_db2.run_id;

    let vector1 = test_db1.vector();
    let vector2 = test_db2.vector();

    vector1.create_collection(run_id1, "embeddings", config_minilm()).unwrap();
    vector2.create_collection(run_id2, "embeddings", config_minilm()).unwrap();

    // Insert in same order
    for i in 0..50 {
        let vec = seeded_random_vector(384, i as u64);
        vector1.insert(run_id1, "embeddings", &format!("key_{}", i), &vec, None).unwrap();
        vector2.insert(run_id2, "embeddings", &format!("key_{}", i), &vec, None).unwrap();
    }

    let query = seeded_random_vector(384, 999);

    let results1 = vector1.search(run_id1, "embeddings", &query, 20, None).unwrap();
    let results2 = vector2.search(run_id2, "embeddings", &query, 20, None).unwrap();

    // Results should be identical
    assert_eq!(results1.len(), results2.len());
    for (r1, r2) in results1.iter().zip(results2.iter()) {
        assert_eq!(r1.key, r2.key);
        assert_eq!(r1.score, r2.score);
    }
}

#[test]
fn test_list_collections_order_deterministic() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    // Create collections in specific order
    let names = ["alpha", "beta", "gamma", "delta", "epsilon"];
    for name in names.iter() {
        vector.create_collection(run_id, name, config_minilm()).unwrap();
    }

    // List should be deterministic
    let list1 = vector.list_collections(run_id).unwrap();
    let list2 = vector.list_collections(run_id).unwrap();

    assert_eq!(list1.len(), list2.len());
    for (c1, c2) in list1.iter().zip(list2.iter()) {
        assert_eq!(c1.name, c2.name);
    }
}

#[test]
fn test_iteration_order_deterministic() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

    for i in 0..30 {
        vector.insert(run_id, "embeddings", &format!("key_{:02}", i), &seeded_random_vector(384, i as u64), None).unwrap();
    }

    // Get all vectors multiple times
    let get_all_keys = || -> Vec<String> {
        (0..30)
            .filter_map(|i| {
                vector.get(run_id, "embeddings", &format!("key_{:02}", i)).unwrap()
                    .map(|_| format!("key_{:02}", i))
            })
            .collect()
    };

    let keys1 = get_all_keys();
    let keys2 = get_all_keys();

    assert_eq!(keys1, keys2, "Iteration should be deterministic");
}

#[test]
fn test_count_deterministic() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

    for i in 0..100 {
        vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
    }

    // Delete some
    for i in 0..30 {
        vector.delete(run_id, "embeddings", &format!("key_{}", i)).unwrap();
    }

    // Count should be deterministic
    let counts: Vec<_> = (0..5).map(|_| vector.count(run_id, "embeddings").unwrap()).collect();

    assert!(counts.iter().all(|&c| c == 70), "Count should be deterministic");
}
