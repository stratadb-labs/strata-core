//! Concurrent Collection Mutation Tests
//!
//! Tests concurrent operations on collections.
//!
//! ## Coverage Gap Addressed
//!
//! Previous tests lacked concurrent mutation during search operations.
//! This file tests:
//! - Collection create/delete during search
//! - Vector insert/delete during search
//! - Multiple threads operating on same collection

use crate::test_utils::*;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Test concurrent vector inserts.
///
/// This test verifies that concurrent inserts from multiple threads all correctly
/// persist. The VectorStore is now a stateless facade with shared backend state
/// stored in the Database via the extension mechanism, so all VectorStore instances
/// share the same backend map.
///
/// **Fixed**: GitHub issue #459 - VectorStore concurrent inserts result in data loss
/// The fix was to make VectorStore stateless (like all other primitives) by storing
/// the backend state in the Database via `db.extension::<VectorBackendState>()`.
#[test]
fn test_concurrent_vector_inserts() {
    let test_db = TestDb::new();
    let db = test_db.db.clone();
    let run_id = test_db.run_id;

    // Create collection
    let vector = in_mem_primitives::VectorStore::new(db.clone());
    vector.create_collection(run_id, "concurrent", config_small()).expect("create");

    // Concurrent inserts from multiple threads
    let mut handles = vec![];
    for t in 0..4 {
        let db = db.clone();
        let handle = thread::spawn(move || {
            // Each thread creates its own VectorStore instance, but they all share
            // the same backend state via db.extension::<VectorBackendState>()
            let vector = in_mem_primitives::VectorStore::new(db);
            for i in 0..25 {
                let key = format!("t{}_v{}", t, i);
                vector.insert(run_id, "concurrent", &key, &seeded_vector(3, (t * 100 + i) as u64), None)
                    .expect("insert");
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().expect("join");
    }

    // Verify all vectors inserted - this now works correctly since all VectorStore
    // instances share the same backend state through the Database extension mechanism
    let count = vector.count(run_id, "concurrent").expect("count");

    // FIXED: GitHub issue #459
    // All 100 vectors should be present (4 threads × 25 vectors each)
    assert_eq!(count, 100, "All 100 vectors should be present after concurrent inserts");
}

/// Test concurrent search and insert.
#[test]
fn test_concurrent_search_and_insert() {
    let test_db = TestDb::new();
    let db = test_db.db.clone();
    let run_id = test_db.run_id;

    // Create and populate collection
    let vector = in_mem_primitives::VectorStore::new(db.clone());
    vector.create_collection(run_id, "search_insert", config_small()).expect("create");

    for i in 0..50 {
        vector.insert(run_id, "search_insert", &format!("v{}", i), &seeded_vector(3, i as u64), None)
            .expect("insert");
    }

    // Concurrent: one thread searches, others insert
    let db_search = db.clone();
    let search_handle = thread::spawn(move || {
        let vector = in_mem_primitives::VectorStore::new(db_search);
        for _ in 0..100 {
            let query = seeded_vector(3, 42);
            let results = vector.search(run_id, "search_insert", &query, 10, None);
            // Search should always succeed (snapshot isolation)
            assert!(results.is_ok(), "Search should succeed during concurrent inserts");
            thread::sleep(Duration::from_micros(100));
        }
    });

    let insert_handles: Vec<_> = (0..2).map(|t| {
        let db = db.clone();
        thread::spawn(move || {
            let vector = in_mem_primitives::VectorStore::new(db);
            for i in 0..25 {
                let key = format!("new_t{}_v{}", t, i);
                vector.insert(run_id, "search_insert", &key, &seeded_vector(3, (1000 + t * 100 + i) as u64), None)
                    .expect("insert");
            }
        })
    }).collect();

    search_handle.join().expect("search join");
    for h in insert_handles {
        h.join().expect("insert join");
    }
}

/// Test concurrent collection create and delete.
#[test]
fn test_concurrent_collection_lifecycle() {
    let test_db = TestDb::new();
    let db = test_db.db.clone();

    // Multiple threads creating/deleting collections
    let mut handles = vec![];
    for t in 0..4 {
        let db = db.clone();
        let handle = thread::spawn(move || {
            let vector = in_mem_primitives::VectorStore::new(db);
            let run_id = in_mem_core::types::RunId::new();

            for i in 0..10 {
                let name = format!("t{}_col{}", t, i);

                // Create
                vector.create_collection(run_id, &name, config_small()).expect("create");

                // Insert some data
                vector.insert(run_id, &name, "v1", &[1.0, 0.0, 0.0], None).expect("insert");

                // Delete
                vector.delete_collection(run_id, &name).expect("delete");
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().expect("join");
    }
}

/// Test reads during concurrent modifications.
#[test]
fn test_read_during_modifications() {
    let test_db = TestDb::new();
    let db = test_db.db.clone();
    let run_id = test_db.run_id;

    // Create collection with initial data
    let vector = in_mem_primitives::VectorStore::new(db.clone());
    vector.create_collection(run_id, "read_mod", config_small()).expect("create");

    for i in 0..100 {
        vector.insert(run_id, "read_mod", &format!("v{}", i), &seeded_vector(3, i as u64), None)
            .expect("insert");
    }

    // Reader thread
    let db_read = db.clone();
    let read_handle = thread::spawn(move || {
        let vector = in_mem_primitives::VectorStore::new(db_read);
        for i in 0..100 {
            let key = format!("v{}", i % 100);
            let result = vector.get(run_id, "read_mod", &key);
            // Read should always succeed (snapshot isolation)
            assert!(result.is_ok(), "Read should succeed during modifications");
        }
    });

    // Modifier thread
    let db_mod = db.clone();
    let mod_handle = thread::spawn(move || {
        let vector = in_mem_primitives::VectorStore::new(db_mod);
        for i in 0..50 {
            let key = format!("new_v{}", i);
            vector.insert(run_id, "read_mod", &key, &seeded_vector(3, (1000 + i) as u64), None)
                .expect("insert");
        }
    });

    read_handle.join().expect("read join");
    mod_handle.join().expect("mod join");
}
