//! Cross-Primitive Recovery Tests
//!
//! Verify that all 6 primitives recover atomically — if one recovers,
//! they all recover. No primitive is left behind.

use crate::common::*;

#[test]
fn all_six_primitives_recover_together() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let p = test_db.all_primitives();

    // Write to all 6 primitives
    p.kv.put(&run_id, "k1", Value::Int(1)).unwrap();

    let doc_id = new_doc_id();
    p.json.create(&run_id, &doc_id, test_json_value(1)).unwrap();

    p.event
        .append(&run_id, "stream", int_payload(42))
        .unwrap();

    p.state
        .init(&run_id, "cell", Value::String("initial".into()))
        .unwrap();

    p.vector
        .create_collection(run_id, "col", config_small())
        .unwrap();
    p.vector
        .insert(run_id, "col", "v1", &[1.0, 0.0, 0.0], None)
        .unwrap();

    drop(p);
    test_db.reopen();

    // Verify all 6 primitives recovered
    let p = test_db.all_primitives();

    let kv_val = p.kv.get(&run_id, "k1").unwrap();
    assert!(kv_val.is_some(), "KV should recover");

    let json_val = p.json.get(&run_id, &doc_id, &root()).unwrap();
    assert!(json_val.is_some(), "JSON should recover");

    let events = p.event.read_by_type(&run_id, "stream").unwrap();
    assert_eq!(events.len(), 1, "EventLog should recover");

    let state_val = p.state.read(&run_id, "cell").unwrap();
    assert!(state_val.is_some(), "StateCell should recover");

    let vec_val = p.vector.get(run_id, "col", "v1").unwrap();
    assert!(vec_val.is_some(), "VectorStore should recover");
}

#[test]
fn interleaved_writes_recover_correctly() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    let event = test_db.event();

    // Interleave KV and EventLog writes
    for i in 0..50 {
        kv.put(&run_id, &format!("k{}", i), Value::Int(i)).unwrap();
        event
            .append(&run_id, "interleaved", int_payload(i))
            .unwrap();
    }

    test_db.reopen();

    let kv = test_db.kv();
    let event = test_db.event();

    for i in 0..50 {
        let val = kv.get(&run_id, &format!("k{}", i)).unwrap();
        assert!(val.is_some(), "KV key k{} missing after recovery", i);
    }

    let events = event.read_by_type(&run_id, "interleaved").unwrap();
    assert_eq!(events.len(), 50, "All 50 events should recover");
}

#[test]
fn multiple_runs_recover_independently() {
    let mut test_db = TestDb::new_strict();
    let run1 = test_db.run_id;
    let run2 = RunId::new();

    let kv = test_db.kv();
    kv.put(&run1, "run1_key", Value::String("from_run1".into()))
        .unwrap();
    kv.put(&run2, "run2_key", Value::String("from_run2".into()))
        .unwrap();

    test_db.reopen();

    let kv = test_db.kv();
    let v1 = kv.get(&run1, "run1_key").unwrap();
    let v2 = kv.get(&run2, "run2_key").unwrap();
    assert!(v1.is_some(), "Run1 data should recover");
    assert!(v2.is_some(), "Run2 data should recover");

    // Cross-contamination check
    let cross = kv.get(&run1, "run2_key").unwrap();
    assert!(cross.is_none(), "Run1 should not see run2's keys");
}

#[test]
fn vector_collection_config_recovers() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let vector = test_db.vector();
    vector
        .create_collection(run_id, "cosine_col", config_small())
        .unwrap();
    vector
        .create_collection(run_id, "euclidean_col", config_euclidean())
        .unwrap();

    // Insert into both
    vector
        .insert(run_id, "cosine_col", "v1", &[1.0, 0.0, 0.0], None)
        .unwrap();
    vector
        .insert(
            run_id,
            "euclidean_col",
            "v1",
            &seeded_vector(384, 1),
            None,
        )
        .unwrap();

    test_db.reopen();

    let vector = test_db.vector();
    assert!(
        vector.get(run_id, "cosine_col", "v1").unwrap().is_some(),
        "Cosine collection should recover"
    );
    assert!(
        vector.get(run_id, "euclidean_col", "v1").unwrap().is_some(),
        "Euclidean collection should recover"
    );
}

#[test]
fn json_mutations_survive_recovery() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let json = test_db.json();
    let doc_id = new_doc_id();
    json.create(
        &run_id,
        &doc_id,
        json_value(serde_json::json!({"count": 0, "items": []})),
    )
    .unwrap();

    // Mutate the document
    json.set(
        &run_id,
        &doc_id,
        &path("count"),
        json_value(serde_json::json!(42)),
    )
    .unwrap();

    test_db.reopen();

    let json = test_db.json();
    let doc = json.get(&run_id, &doc_id, &root()).unwrap().unwrap();
    let inner = doc.value.as_inner();
    assert_eq!(inner["count"], 42, "JSON mutation should survive recovery");
}
