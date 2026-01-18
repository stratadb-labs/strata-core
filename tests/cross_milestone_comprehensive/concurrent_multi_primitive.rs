//! Concurrent Multi-Primitive Tests
//!
//! Tests concurrent operations across multiple primitives.

use crate::test_utils::*;
use in_mem_core::json::JsonValue;
use in_mem_core::types::JsonDocId;
use in_mem_core::value::Value;
use std::sync::Arc;
use std::thread;

/// Test concurrent operations on all primitives.
#[test]
fn test_concurrent_all_primitives() {
    let test_db = TestDb::new();
    let db = test_db.db.clone();

    let mut handles = vec![];

    // KV thread
    let db_kv = db.clone();
    handles.push(thread::spawn(move || {
        let kv = in_mem_primitives::KVStore::new(db_kv);
        let run_id = in_mem_core::types::RunId::new();
        for i in 0..100 {
            kv.put(&run_id, &format!("kv_{}", i), Value::I64(i)).expect("kv put");
        }
    }));

    // JSON thread
    let db_json = db.clone();
    handles.push(thread::spawn(move || {
        let json = in_mem_primitives::JsonStore::new(db_json);
        let run_id = in_mem_core::types::RunId::new();
        for i in 0..100 {
            let doc_id = JsonDocId::new();
            json.create(&run_id, &doc_id, JsonValue::from(serde_json::json!({"i": i})))
                .expect("json create");
        }
    }));

    // Event thread
    let db_event = db.clone();
    handles.push(thread::spawn(move || {
        let event = in_mem_primitives::EventLog::new(db_event);
        let run_id = in_mem_core::types::RunId::new();
        for i in 0..100 {
            event.append(&run_id, "type", Value::I64(i))
                .expect("event append");
        }
    }));

    // Vector thread
    let db_vec = db.clone();
    handles.push(thread::spawn(move || {
        let vector = in_mem_primitives::VectorStore::new(db_vec);
        let run_id = in_mem_core::types::RunId::new();
        vector.create_collection(run_id, "concurrent_col", config_small()).expect("create");
        for i in 0..100 {
            vector.insert(run_id, "concurrent_col", &format!("v_{}", i), &seeded_vector(3, i as u64), None)
                .expect("insert");
        }
    }));

    // Wait for all threads
    for h in handles {
        h.join().expect("thread join");
    }
}

/// Test concurrent primitive access from same run.
#[test]
fn test_concurrent_same_run() {
    let test_db = TestDb::new();
    let db = test_db.db.clone();
    let run_id = test_db.run_id;

    // Setup: create vector collection
    let vector = in_mem_primitives::VectorStore::new(db.clone());
    vector.create_collection(run_id, "same_run", config_small()).expect("create");

    let mut handles = vec![];

    // Multiple threads accessing same run
    for t in 0..4 {
        let db = db.clone();
        let handle = thread::spawn(move || {
            let kv = in_mem_primitives::KVStore::new(db.clone());
            let vector = in_mem_primitives::VectorStore::new(db);

            for i in 0..25 {
                let key = format!("t{}_item{}", t, i);
                kv.put(&run_id, &key, Value::I64((t * 100 + i) as i64)).expect("kv put");
                vector.insert(run_id, "same_run", &key, &seeded_vector(3, (t * 100 + i) as u64), None)
                    .expect("vector insert");
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().expect("join");
    }
}
