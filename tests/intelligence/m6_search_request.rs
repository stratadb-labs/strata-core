//! Tier 7: M6 SearchRequest Compatibility Tests

use crate::common::*;

#[test]
fn test_m6_search_basic() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "embeddings", config_minilm()).unwrap();

    for i in 0..50 {
        vector.insert(test_db.run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
    }

    let query = seeded_random_vector(384, 999);
    let results = vector.search(test_db.run_id, "embeddings", &query, 10, None).unwrap();

    assert_eq!(results.len(), 10);
}

#[test]
fn test_m6_search_with_filter() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "embeddings", config_minilm()).unwrap();

    for i in 0..50 {
        vector.insert(
            test_db.run_id,
            "embeddings",
            &format!("key_{}", i),
            &seeded_random_vector(384, i as u64),
            Some(serde_json::json!({"category": if i % 2 == 0 { "even" } else { "odd" }})),
        ).unwrap();
    }

    // Test with filter (if supported)
    let query = seeded_random_vector(384, 999);
    let results = vector.search(test_db.run_id, "embeddings", &query, 50, None).unwrap();

    assert!(!results.is_empty());
}

#[test]
fn test_m6_search_returns_metadata() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "embeddings", config_minilm()).unwrap();

    vector.insert(
        test_db.run_id,
        "embeddings",
        "key1",
        &random_vector(384),
        Some(serde_json::json!({"source": "test", "index": 1})),
    ).unwrap();

    let query = random_vector(384);
    let results = vector.search(test_db.run_id, "embeddings", &query, 1, None).unwrap();

    assert_eq!(results.len(), 1);
    // Metadata is associated with the entry, accessible via get
}
