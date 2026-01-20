//! S4: VectorId Never Reused Tests
//!
//! Invariant S4: Once assigned, a VectorId is never recycled (even after deletion).

use crate::test_utils::*;
use in_mem_primitives::vector::VectorId;

/// Test that VectorIds are never reused after deletion
#[test]
fn test_s4_vectorid_never_reused_after_delete() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Insert and capture IDs
    vector
        .insert(test_db.run_id, "embeddings", "key1", &random_vector(384), None)
        .unwrap();
    vector
        .insert(test_db.run_id, "embeddings", "key2", &random_vector(384), None)
        .unwrap();
    vector
        .insert(test_db.run_id, "embeddings", "key3", &random_vector(384), None)
        .unwrap();

    let id1 = vector
        .get(test_db.run_id, "embeddings", "key1")
        .unwrap()
        .unwrap()
        .value.vector_id();
    let id2 = vector
        .get(test_db.run_id, "embeddings", "key2")
        .unwrap()
        .unwrap()
        .value.vector_id();
    let id3 = vector
        .get(test_db.run_id, "embeddings", "key3")
        .unwrap()
        .unwrap()
        .value.vector_id();

    let max_id_before = id1.as_u64().max(id2.as_u64()).max(id3.as_u64());

    // Delete all vectors
    vector.delete(test_db.run_id, "embeddings", "key1").unwrap();
    vector.delete(test_db.run_id, "embeddings", "key2").unwrap();
    vector.delete(test_db.run_id, "embeddings", "key3").unwrap();

    // Insert new vectors
    vector
        .insert(test_db.run_id, "embeddings", "key4", &random_vector(384), None)
        .unwrap();
    vector
        .insert(test_db.run_id, "embeddings", "key5", &random_vector(384), None)
        .unwrap();

    let id4 = vector
        .get(test_db.run_id, "embeddings", "key4")
        .unwrap()
        .unwrap()
        .value.vector_id();
    let id5 = vector
        .get(test_db.run_id, "embeddings", "key5")
        .unwrap()
        .unwrap()
        .value.vector_id();

    // All new IDs must be > max_id_before
    assert!(
        id4.as_u64() > max_id_before,
        "S4 VIOLATED: VectorId {} reused (max before was {})",
        id4.as_u64(),
        max_id_before
    );
    assert!(
        id5.as_u64() > max_id_before,
        "S4 VIOLATED: VectorId {} reused (max before was {})",
        id5.as_u64(),
        max_id_before
    );

    // Deleted IDs must not be reused
    assert_ne!(id4.as_u64(), id1.as_u64());
    assert_ne!(id4.as_u64(), id2.as_u64());
    assert_ne!(id4.as_u64(), id3.as_u64());
    assert_ne!(id5.as_u64(), id1.as_u64());
    assert_ne!(id5.as_u64(), id2.as_u64());
    assert_ne!(id5.as_u64(), id3.as_u64());
}

/// Test that VectorIds are strictly monotonic within a session
#[test]
fn test_s4_vectorid_monotonic_within_session() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    let mut previous_id = 0u64;

    for i in 0..100 {
        let key = format!("key_{}", i);
        vector
            .insert(test_db.run_id, "embeddings", &key, &random_vector(384), None)
            .unwrap();

        let current_id = vector
            .get(test_db.run_id, "embeddings", &key)
            .unwrap()
            .unwrap()
            .value.vector_id()
            .as_u64();

        assert!(
            current_id > previous_id,
            "S4 VIOLATED: VectorId {} not greater than previous {}",
            current_id,
            previous_id
        );

        previous_id = current_id;
    }
}

/// Test that re-inserting same key after delete gets new ID
#[test]
fn test_s4_insert_delete_insert_same_key_new_id() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    // Insert key
    vector
        .insert(test_db.run_id, "embeddings", "key1", &random_vector(384), None)
        .unwrap();
    let id_first = vector
        .get(test_db.run_id, "embeddings", "key1")
        .unwrap()
        .unwrap()
        .value.vector_id();

    // Delete key
    vector.delete(test_db.run_id, "embeddings", "key1").unwrap();

    // Insert same key again
    vector
        .insert(test_db.run_id, "embeddings", "key1", &random_vector(384), None)
        .unwrap();
    let id_second = vector
        .get(test_db.run_id, "embeddings", "key1")
        .unwrap()
        .unwrap()
        .value.vector_id();

    // Second insert must have a NEW VectorId
    assert!(
        id_second.as_u64() > id_first.as_u64(),
        "S4 VIOLATED: Re-inserted key got same or lower VectorId ({} vs {})",
        id_second.as_u64(),
        id_first.as_u64()
    );
}

/// Test VectorId monotonicity across restart
#[test]
fn test_s4_vectorid_monotonic_across_restart() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let max_id_before;
    {
        let vector = test_db.vector();
        vector
            .create_collection(run_id, "embeddings", config_minilm())
            .unwrap();

        for i in 0..50 {
            vector
                .insert(run_id, "embeddings", &format!("key_{}", i), &random_vector(384), None)
                .unwrap();
        }

        // Get max ID
        max_id_before = (0..50)
            .map(|i| {
                vector
                    .get(run_id, "embeddings", &format!("key_{}", i))
                    .unwrap()
                    .unwrap()
                    .value.vector_id()
                    .as_u64()
            })
            .max()
            .unwrap();
    }

    // Restart
    test_db.reopen();

    let vector = test_db.vector();

    // Insert new vectors after restart
    for i in 50..60 {
        vector
            .insert(run_id, "embeddings", &format!("key_{}", i), &random_vector(384), None)
            .unwrap();
    }

    // All new IDs must be > max_id_before
    for i in 50..60 {
        let id = vector
            .get(run_id, "embeddings", &format!("key_{}", i))
            .unwrap()
            .unwrap()
            .value.vector_id()
            .as_u64();
        assert!(
            id > max_id_before,
            "S4 VIOLATED: Post-restart VectorId {} <= pre-restart max {}",
            id,
            max_id_before
        );
    }
}

/// Test many delete/insert cycles don't reuse IDs
#[test]
fn test_s4_many_delete_insert_cycles() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector
        .create_collection(test_db.run_id, "embeddings", config_minilm())
        .unwrap();

    let mut all_seen_ids = std::collections::HashSet::new();

    // Perform 50 cycles of insert/delete
    for cycle in 0..50 {
        let key = format!("cycle_key_{}", cycle);

        // Insert
        vector
            .insert(test_db.run_id, "embeddings", &key, &random_vector(384), None)
            .unwrap();
        let id = vector
            .get(test_db.run_id, "embeddings", &key)
            .unwrap()
            .unwrap()
            .value.vector_id()
            .as_u64();

        // Check ID hasn't been seen before
        assert!(
            all_seen_ids.insert(id),
            "S4 VIOLATED: VectorId {} was reused in cycle {}",
            id,
            cycle
        );

        // Delete
        vector.delete(test_db.run_id, "embeddings", &key).unwrap();
    }

    // All IDs should be unique
    assert_eq!(all_seen_ids.len(), 50);
}
