//! Tier 8: WAL Write Tests

use crate::common::*;

#[test]
fn test_wal_write_vector_operations() {
    let test_db = TestDb::new_strict();
    let vector = test_db.vector();

    let wal_before = wal_size(&test_db.wal_path());

    vector.create_collection(test_db.run_id, "embeddings", config_minilm()).unwrap();
    vector.insert(test_db.run_id, "embeddings", "key1", &random_vector(384), None).unwrap();
    vector.delete(test_db.run_id, "embeddings", "key1").unwrap();
    vector.delete_collection(test_db.run_id, "embeddings").unwrap();

    let wal_after = wal_size(&test_db.wal_path());

    assert!(wal_after > wal_before, "WAL should grow with vector operations");
}

#[test]
fn test_wal_write_on_insert() {
    let test_db = TestDb::new_strict();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "embeddings", config_minilm()).unwrap();

    let wal_before = wal_size(&test_db.wal_path());

    for i in 0..10 {
        vector.insert(test_db.run_id, "embeddings", &format!("key_{}", i), &random_vector(384), None).unwrap();
    }

    let wal_after = wal_size(&test_db.wal_path());

    assert!(wal_after > wal_before, "WAL should grow with inserts");
}

#[test]
fn test_search_does_not_write_wal() {
    let test_db = TestDb::new_strict();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "embeddings", config_minilm()).unwrap();

    for i in 0..50 {
        vector.insert(test_db.run_id, "embeddings", &format!("key_{}", i), &seeded_random_vector(384, i as u64), None).unwrap();
    }

    let wal_before = wal_size(&test_db.wal_path());

    // Many searches
    for _ in 0..100 {
        let _ = vector.search(test_db.run_id, "embeddings", &random_vector(384), 10, None);
    }

    let wal_after = wal_size(&test_db.wal_path());

    assert_eq!(wal_before, wal_after, "Search should not write to WAL");
}
