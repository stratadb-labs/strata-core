//! Tier 13: Stress - Rapid Operations Tests

use crate::test_utils::*;

#[test]
fn test_rapid_upsert_same_key() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

    // Rapid upserts to the same key
    for i in 0..100 {
        vector.insert(run_id, "embeddings", "same_key", &seeded_random_vector(384, i as u64), None).unwrap();
    }

    // Only one vector should exist
    assert_eq!(vector.count(run_id, "embeddings").unwrap(), 1);

    // Should have the last vector
    let result = vector.get(run_id, "embeddings", "same_key").unwrap().unwrap();
    assert_eq!(result.value.embedding, seeded_random_vector(384, 99));
}

#[test]
fn test_rapid_insert_delete_cycles() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

    // Rapid insert/delete cycles
    for cycle in 0..20 {
        // Insert 10 vectors
        for i in 0..10 {
            vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, (cycle * 10 + i) as u64), None).unwrap();
        }

        // Delete all
        for i in 0..10 {
            vector.delete(run_id, "embeddings", &format!("key_{}", i)).unwrap();
        }
    }

    // Should be empty
    assert_eq!(vector.count(run_id, "embeddings").unwrap(), 0);
}

#[test]
fn test_rapid_collection_create_delete() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    // Rapid collection create/delete cycles
    for cycle in 0..20 {
        let name = format!("col_{}", cycle % 5);

        // Delete if exists (ignore errors on first cycle)
        let _ = vector.delete_collection(run_id, &name);

        // Create
        vector.create_collection(run_id, &name, config_small()).unwrap();

        // Add some data
        for i in 0..5 {
            vector.insert(run_id, &name, &format!("key_{}", i), &seeded_random_vector(3, i as u64), None).unwrap();
        }
    }

    // Should have 5 collections (the last ones created)
    let collections = vector.list_collections(run_id).unwrap();
    assert_eq!(collections.len(), 5);
}

#[test]
fn test_rapid_search_during_modifications() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

    // Initial data
    for i in 0..50 {
        vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
    }

    let query = seeded_random_vector(384, 999);

    // Interleave searches with modifications
    for i in 50..100 {
        // Insert
        vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();

        // Search
        let results = vector.search(run_id, "embeddings", &query, 10, None).unwrap();
        assert_eq!(results.len(), 10);
    }

    // Delete while searching
    for i in 0..50 {
        // Delete
        vector.delete(run_id, "embeddings", &format!("key_{}", i)).unwrap();

        // Search
        let results = vector.search(run_id, "embeddings", &query, 10, None).unwrap();
        assert!(results.len() <= 10);
    }

    assert_eq!(vector.count(run_id, "embeddings").unwrap(), 50);
}
