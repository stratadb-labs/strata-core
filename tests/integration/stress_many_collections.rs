//! Tier 13: Stress - Many Collections Tests

use crate::common::*;

#[test]
fn test_many_collections_create() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    // Create 100 collections
    for i in 0..100 {
        vector.create_collection(run_id, &format!("col_{}", i), config_small()).unwrap();
    }

    let collections = vector.list_collections(run_id).unwrap();
    assert_eq!(collections.len(), 100);
}

#[test]
fn test_many_collections_with_data() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    // Create 50 collections with 10 vectors each
    for i in 0..50 {
        vector.create_collection(run_id, &format!("col_{}", i), config_small()).unwrap();

        for j in 0..10 {
            vector.insert(run_id, &format!("col_{}", i), &format!("key_{}", j), &seeded_random_vector(3, (i * 10 + j) as u64), None).unwrap();
        }
    }

    // Verify all collections have data
    for i in 0..50 {
        assert_eq!(vector.count(run_id, &format!("col_{}", i)).unwrap(), 10);
    }
}

#[test]
fn test_many_collections_recovery() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();

        for i in 0..30 {
            vector.create_collection(run_id, &format!("col_{}", i), config_small()).unwrap();

            for j in 0..5 {
                vector.insert(run_id, &format!("col_{}", i), &format!("key_{}", j), &seeded_random_vector(3, (i * 5 + j) as u64), None).unwrap();
            }
        }
    }

    test_db.reopen();

    let vector = test_db.vector();
    let collections = vector.list_collections(run_id).unwrap();
    assert_eq!(collections.len(), 30);

    for i in 0..30 {
        assert_eq!(vector.count(run_id, &format!("col_{}", i)).unwrap(), 5);
    }
}

#[test]
fn test_many_collections_search_each() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    // Create 20 collections with 20 vectors each
    for i in 0..20 {
        vector.create_collection(run_id, &format!("col_{}", i), config_small()).unwrap();

        for j in 0..20 {
            vector.insert(run_id, &format!("col_{}", i), &format!("key_{}", j), &seeded_random_vector(3, (i * 20 + j) as u64), None).unwrap();
        }
    }

    // Search each collection
    let query = seeded_random_vector(3, 999);
    for i in 0..20 {
        let results = vector.search(run_id, &format!("col_{}", i), &query, 5, None).unwrap();
        assert_eq!(results.len(), 5, "Collection col_{} should return 5 results", i);
    }
}

#[test]
fn test_many_collections_delete_some() {
    let test_db = TestDb::new_strict();
    let run_id = test_db.run_id;
    let vector = test_db.vector();

    // Create 50 collections
    for i in 0..50 {
        vector.create_collection(run_id, &format!("col_{}", i), config_small()).unwrap();
    }

    // Delete half
    for i in 0..25 {
        vector.delete_collection(run_id, &format!("col_{}", i)).unwrap();
    }

    let collections = vector.list_collections(run_id).unwrap();
    assert_eq!(collections.len(), 25);

    // Deleted collections should not exist
    for i in 0..25 {
        assert!(vector.get_collection(run_id, &format!("col_{}", i)).unwrap().is_none());
    }

    // Remaining collections should exist
    for i in 25..50 {
        assert!(vector.get_collection(run_id, &format!("col_{}", i)).unwrap().is_some());
    }
}
