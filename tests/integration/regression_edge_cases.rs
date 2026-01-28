//! Tier 14: Non-Regression - Edge Case Tests
//!
//! Tests for edge cases that caused issues in the past.

use crate::common::*;

#[test]
fn test_collection_name_special_characters() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    // Test various collection name patterns
    let valid_names = [
        "simple",
        "with_underscore",
        "with-dash",
        "with123numbers",
        "MixedCase",
        "a",  // Single character
    ];

    for name in valid_names.iter() {
        vector.create_collection(run_id, name, config_small()).unwrap();
        assert!(vector.get_collection(run_id, name).unwrap().is_some());
    }
}

#[test]
fn test_key_special_characters() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    vector.create_collection(run_id, "embeddings", config_small()).unwrap();

    // Test various key patterns
    let keys = [
        "simple",
        "with_underscore",
        "with-dash",
        "with/slash",
        "with:colon",
        "with.dot",
        "user@example.com",
        "key with spaces",
        "123numeric",
    ];

    for (i, key) in keys.iter().enumerate() {
        vector.insert(run_id, "embeddings", key, &seeded_random_vector(3, i as u64), None).unwrap();
    }

    // Verify all keys exist
    for key in keys.iter() {
        assert!(vector.get(run_id, "embeddings", key).unwrap().is_some(), "Key '{}' should exist", key);
    }

    assert_eq!(vector.count(run_id, "embeddings").unwrap(), keys.len());
}

#[test]
fn test_empty_string_key() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    vector.create_collection(run_id, "embeddings", config_small()).unwrap();

    // Empty string key should be valid
    vector.insert(run_id, "embeddings", "", &random_vector(3), None).unwrap();
    assert!(vector.get(run_id, "embeddings", "").unwrap().is_some());
}

#[test]
fn test_very_long_key() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    vector.create_collection(run_id, "embeddings", config_small()).unwrap();

    // Long key (1000 characters)
    let long_key: String = (0..1000).map(|_| 'a').collect();
    vector.insert(run_id, "embeddings", &long_key, &random_vector(3), None).unwrap();

    assert!(vector.get(run_id, "embeddings", &long_key).unwrap().is_some());
}

#[test]
fn test_unicode_in_keys() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    vector.create_collection(run_id, "embeddings", config_small()).unwrap();

    let unicode_keys = [
        "日本語",
        "émoji🎉",
        "Ελληνικά",
        "العربية",
        "中文",
        "🔥🚀💯",
    ];

    for (i, key) in unicode_keys.iter().enumerate() {
        vector.insert(run_id, "embeddings", key, &seeded_random_vector(3, i as u64), None).unwrap();
    }

    for key in unicode_keys.iter() {
        assert!(vector.get(run_id, "embeddings", key).unwrap().is_some(), "Unicode key '{}' should exist", key);
    }
}

#[test]
fn test_zero_k_search() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

    for i in 0..10 {
        vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
    }

    // Search with k=0 should return empty
    let results = vector.search(run_id, "embeddings", &random_vector(384), 0, None).unwrap();
    assert!(results.is_empty());
}

#[test]
fn test_single_vector_search() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

    // Only one vector
    let single_vec = seeded_random_vector(384, 42);
    vector.insert(run_id, "embeddings", "only_one", &single_vec, None).unwrap();

    // Search should return it
    let results = vector.search(run_id, "embeddings", &random_vector(384), 10, None).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].key, "only_one");
}

#[test]
fn test_recovery_after_only_deletes() {
    // Edge case: WAL contains only delete operations (after checkpoint)
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "embeddings", config_minilm()).unwrap();

        for i in 0..10 {
            vector.insert(run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
        }
    }

    // Simulate checkpoint
    test_db.reopen();

    {
        let vector = test_db.vector();
        // Delete all (these will be in WAL only after checkpoint)
        for i in 0..10 {
            vector.delete(run_id, "embeddings", &format!("key_{}", i)).unwrap();
        }
    }

    test_db.reopen();

    let vector = test_db.vector();
    assert_eq!(vector.count(run_id, "embeddings").unwrap(), 0);
}
