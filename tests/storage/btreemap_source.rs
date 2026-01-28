//! S7: BTreeMap Sole Source of Truth Tests
//!
//! Invariant S7: id_to_offset (BTreeMap) is the ONLY source of truth for active vectors.

use crate::common::*;

/// Test that BTreeMap determines which vectors are active
#[test]
fn test_s7_btreemap_determines_active_vectors() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    vector
        .insert(test_db.run_id, "embeddings", "key1", &random_vector(384), None)
        .unwrap();
    vector
        .insert(test_db.run_id, "embeddings", "key2", &random_vector(384), None)
        .unwrap();
    vector
        .insert(test_db.run_id, "embeddings", "key3", &random_vector(384), None)
        .unwrap();

    // Delete key2
    vector.delete(test_db.run_id, "embeddings", "key2").unwrap();

    // Count should be 2
    assert_eq!(vector.count(test_db.run_id, "embeddings").unwrap(), 2, "S7 VIOLATED: Wrong count");

    // Get should show key1 and key3 exist, key2 doesn't
    assert!(vector.get(test_db.run_id, "embeddings", "key1").unwrap().is_some());
    assert!(vector.get(test_db.run_id, "embeddings", "key2").unwrap().is_none());
    assert!(vector.get(test_db.run_id, "embeddings", "key3").unwrap().is_some());

    // Search should only return key1 and key3
    let search_results = vector
        .search(test_db.run_id, "embeddings", &random_vector(384), 10, None)
        .unwrap();

    let search_keys: Vec<&str> = search_results.iter().map(|m| m.key.as_str()).collect();
    assert!(search_keys.contains(&"key1"));
    assert!(search_keys.contains(&"key3"));
    assert!(!search_keys.contains(&"key2"));
}

/// Test count matches BTreeMap size
#[test]
fn test_s7_count_matches_btreemap() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Insert 20 vectors
    for i in 0..20 {
        vector
            .insert(
                test_db.run_id,
                "embeddings",
                &format!("key_{}", i),
                &random_vector(384),
                None,
            )
            .unwrap();
    }

    assert_eq!(vector.count(test_db.run_id, "embeddings").unwrap(), 20);

    // Delete 10 vectors
    for i in 0..10 {
        vector
            .delete(test_db.run_id, "embeddings", &format!("key_{}", i))
            .unwrap();
    }

    assert_eq!(vector.count(test_db.run_id, "embeddings").unwrap(), 10);

    // Verify deleted keys don't exist and remaining ones do
    for i in 0..10 {
        assert!(vector.get(test_db.run_id, "embeddings", &format!("key_{}", i)).unwrap().is_none());
    }
    for i in 10..20 {
        assert!(vector.get(test_db.run_id, "embeddings", &format!("key_{}", i)).unwrap().is_some());
    }
}

/// Test that search results come from BTreeMap entries only
#[test]
fn test_s7_search_only_from_btreemap() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Insert vectors
    for i in 0..50 {
        vector
            .insert(
                test_db.run_id,
                "embeddings",
                &format!("key_{}", i),
                &random_vector(384),
                None,
            )
            .unwrap();
    }

    // Delete half
    for i in 0..25 {
        vector
            .delete(test_db.run_id, "embeddings", &format!("key_{}", i))
            .unwrap();
    }

    // Search with k=50 should only return 25 results
    let results = vector
        .search(test_db.run_id, "embeddings", &random_vector(384), 50, None)
        .unwrap();

    assert_eq!(results.len(), 25, "S7 VIOLATED: Search returned deleted vectors");

    // All results should be from remaining keys (25-49)
    for result in &results {
        let key_num: usize = result.key.strip_prefix("key_").unwrap().parse().unwrap();
        assert!(
            key_num >= 25,
            "S7 VIOLATED: Search returned deleted key {}",
            result.key
        );
    }
}

/// Test BTreeMap consistency after restart
#[test]
fn test_s7_btreemap_consistent_after_restart() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector
            .create_collection(run_id, "embeddings", config_minilm())
            .unwrap();

        for i in 0..30 {
            vector
                .insert(run_id, "embeddings", &format!("key_{}", i), &random_vector(384), None)
                .unwrap();
        }

        // Delete some
        for i in 0..10 {
            vector.delete(run_id, "embeddings", &format!("key_{}", i)).unwrap();
        }
    }

    // Restart
    test_db.reopen();

    let vector = test_db.vector();

    // Count should be 20
    assert_eq!(vector.count(run_id, "embeddings").unwrap(), 20);

    // Deleted keys should not exist
    for i in 0..10 {
        assert!(vector.get(run_id, "embeddings", &format!("key_{}", i)).unwrap().is_none());
    }

    // Remaining keys should exist
    for i in 10..30 {
        assert!(vector.get(run_id, "embeddings", &format!("key_{}", i)).unwrap().is_some());
    }
}
