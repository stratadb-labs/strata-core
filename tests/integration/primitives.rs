//! Single and Cross-Primitive Integration Tests
//!
//! Tests each primitive in isolation and in combination.

use crate::common::*;

// ============================================================================
// Single Primitive Tests
// ============================================================================

mod kv_single {
    use super::*;

    #[test]
    fn basic_crud() {
        let db = create_test_db();
        let run_id = RunId::new();
        let kv = KVStore::new(db);

        // Create
        kv.put(&run_id, "key", Value::Int(1)).unwrap();

        // Read
        let val = kv.get(&run_id, "key").unwrap().unwrap();
        assert_eq!(val.value, Value::Int(1));

        // Update
        kv.put(&run_id, "key", Value::Int(2)).unwrap();
        let val = kv.get(&run_id, "key").unwrap().unwrap();
        assert_eq!(val.value, Value::Int(2));

        // Delete
        kv.delete(&run_id, "key").unwrap();
        assert!(kv.get(&run_id, "key").unwrap().is_none());
    }

    #[test]
    fn list_and_scan() {
        let db = create_test_db();
        let run_id = RunId::new();
        let kv = KVStore::new(db);

        for i in 0..20 {
            kv.put(&run_id, &format!("user:{}", i), Value::Int(i)).unwrap();
            kv.put(&run_id, &format!("config:{}", i), Value::Int(i * 10)).unwrap();
        }

        // List all
        let all = kv.list(&run_id, None).unwrap();
        assert_eq!(all.len(), 40);

        // List with prefix
        let users = kv.list(&run_id, Some("user:")).unwrap();
        assert_eq!(users.len(), 20);

        let configs = kv.list(&run_id, Some("config:")).unwrap();
        assert_eq!(configs.len(), 20);
    }

    #[test]
    fn value_types() {
        let db = create_test_db();
        let run_id = RunId::new();
        let kv = KVStore::new(db);

        kv.put(&run_id, "int", Value::Int(42)).unwrap();
        kv.put(&run_id, "float", Value::Float(3.14)).unwrap();
        kv.put(&run_id, "string", Value::String("hello".into())).unwrap();
        kv.put(&run_id, "bool", Value::Bool(true)).unwrap();
        kv.put(&run_id, "bytes", Value::Bytes(vec![1, 2, 3])).unwrap();

        assert!(matches!(kv.get(&run_id, "int").unwrap().unwrap().value, Value::Int(42)));
        assert!(matches!(kv.get(&run_id, "bool").unwrap().unwrap().value, Value::Bool(true)));
    }
}

mod state_single {
    use super::*;

    #[test]
    fn init_and_read() {
        let db = create_test_db();
        let run_id = RunId::new();
        let state = StateCell::new(db);

        state.init(&run_id, "counter", Value::Int(0)).unwrap();
        let val = state.read(&run_id, "counter").unwrap().unwrap();
        assert_eq!(val.value.value, Value::Int(0));
    }

    #[test]
    fn set_updates_value() {
        let db = create_test_db();
        let run_id = RunId::new();
        let state = StateCell::new(db);

        state.init(&run_id, "counter", Value::Int(0)).unwrap();
        state.set(&run_id, "counter", Value::Int(1)).unwrap();
        state.set(&run_id, "counter", Value::Int(2)).unwrap();

        let val = state.read(&run_id, "counter").unwrap().unwrap();
        assert_eq!(val.value.value, Value::Int(2));
    }

    #[test]
    fn cas_with_correct_version_succeeds() {
        let db = create_test_db();
        let run_id = RunId::new();
        let state = StateCell::new(db);

        state.init(&run_id, "counter", Value::Int(0)).unwrap();
        let current = state.read(&run_id, "counter").unwrap().unwrap();

        state.cas(&run_id, "counter", current.version, Value::Int(1)).unwrap();

        let updated = state.read(&run_id, "counter").unwrap().unwrap();
        assert_eq!(updated.value.value, Value::Int(1));
    }

    #[test]
    fn cas_with_stale_version_fails() {
        let db = create_test_db();
        let run_id = RunId::new();
        let state = StateCell::new(db);

        state.init(&run_id, "counter", Value::Int(0)).unwrap();
        let current = state.read(&run_id, "counter").unwrap().unwrap();
        let stale_version = current.version;

        // Update through another path
        state.set(&run_id, "counter", Value::Int(1)).unwrap();

        // CAS with stale version should fail
        let result = state.cas(&run_id, "counter", stale_version, Value::Int(99));
        assert!(result.is_err());

        // Value should remain at 1
        let final_val = state.read(&run_id, "counter").unwrap().unwrap();
        assert_eq!(final_val.value.value, Value::Int(1));
    }
}

mod event_single {
    use super::*;

    #[test]
    fn append_and_read() {
        let db = create_test_db();
        let run_id = RunId::new();
        let event = EventLog::new(db);

        let seq1 = event.append(&run_id, "audit", int_payload(1)).unwrap();
        let seq2 = event.append(&run_id, "audit", int_payload(2)).unwrap();
        let seq3 = event.append(&run_id, "audit", int_payload(3)).unwrap();

        assert!(seq2 > seq1);
        assert!(seq3 > seq2);

        let events = event.read_by_type(&run_id, "audit").unwrap();
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn multiple_streams() {
        let db = create_test_db();
        let run_id = RunId::new();
        let event = EventLog::new(db);

        event.append(&run_id, "stream_a", int_payload(1)).unwrap();
        event.append(&run_id, "stream_a", int_payload(2)).unwrap();
        event.append(&run_id, "stream_b", int_payload(10)).unwrap();

        assert_eq!(event.len_by_type(&run_id, "stream_a").unwrap(), 2);
        assert_eq!(event.len_by_type(&run_id, "stream_b").unwrap(), 1);
    }

    #[test]
    fn events_are_immutable() {
        let db = create_test_db();
        let run_id = RunId::new();
        let event = EventLog::new(db);

        event.append(&run_id, "audit", int_payload(42)).unwrap();

        // Events can only be appended, not modified or deleted
        // The API doesn't provide update/delete methods for events
        let events = event.read_by_type(&run_id, "audit").unwrap();
        assert_eq!(events.len(), 1);
    }
}

mod json_single {
    use super::*;

    #[test]
    fn create_and_get() {
        let db = create_test_db();
        let run_id = RunId::new();
        let json = JsonStore::new(db);

        json.create(&run_id, "doc", json_value(serde_json::json!({"name": "test"}))).unwrap();

        let doc = json.get(&run_id, "doc", &root()).unwrap().unwrap();
        assert_eq!(doc.value.as_inner()["name"], "test");
    }

    #[test]
    fn patch_nested_value() {
        let db = create_test_db();
        let run_id = RunId::new();
        let json = JsonStore::new(db);

        json.create(&run_id, "doc", json_value(serde_json::json!({
            "user": {"name": "Alice", "age": 30}
        }))).unwrap();

        json.set(&run_id, "doc", &path(".user.age"), json_value(serde_json::json!(31))).unwrap();

        let doc = json.get(&run_id, "doc", &path(".user.age")).unwrap().unwrap();
        assert_eq!(doc.value.as_inner(), &serde_json::json!(31));
    }

    #[test]
    fn list_documents() {
        let db = create_test_db();
        let run_id = RunId::new();
        let json = JsonStore::new(db);

        for i in 0..10 {
            json.create(&run_id, &format!("doc_{}", i), test_json_value(i)).unwrap();
        }

        let list = json.list(&run_id, None, None, 100).unwrap();
        assert_eq!(list.doc_ids.len(), 10);
    }
}

mod vector_single {
    use super::*;

    #[test]
    fn create_collection_and_insert() {
        let db = create_test_db();
        let run_id = RunId::new();
        let vector = VectorStore::new(db);

        vector.create_collection(run_id, "embeddings", config_small()).unwrap();
        vector.insert(run_id, "embeddings", "vec_1", &[1.0, 0.0, 0.0], None).unwrap();

        let entry = vector.get(run_id, "embeddings", "vec_1").unwrap().unwrap();
        assert_eq!(entry.value.embedding, vec![1.0, 0.0, 0.0]);
    }

    #[test]
    fn search_returns_nearest() {
        let db = create_test_db();
        let run_id = RunId::new();
        let vector = VectorStore::new(db);

        vector.create_collection(run_id, "test", config_small()).unwrap();

        // Insert vectors at cardinal directions
        vector.insert(run_id, "test", "north", &[0.0, 1.0, 0.0], None).unwrap();
        vector.insert(run_id, "test", "south", &[0.0, -1.0, 0.0], None).unwrap();
        vector.insert(run_id, "test", "east", &[1.0, 0.0, 0.0], None).unwrap();

        // Search for vector close to north
        let query = vec![0.0, 0.99, 0.0];
        let results = vector.search(run_id, "test", &query, 1, None).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, "north");
    }

    #[test]
    fn delete_vector() {
        let db = create_test_db();
        let run_id = RunId::new();
        let vector = VectorStore::new(db);

        vector.create_collection(run_id, "test", config_small()).unwrap();
        vector.insert(run_id, "test", "to_delete", &[1.0, 0.0, 0.0], None).unwrap();

        assert!(vector.get(run_id, "test", "to_delete").unwrap().is_some());

        vector.delete(run_id, "test", "to_delete").unwrap();

        assert!(vector.get(run_id, "test", "to_delete").unwrap().is_none());
    }
}

// ============================================================================
// Cross-Primitive Tests
// ============================================================================

#[test]
fn all_six_primitives_together() {
    let test_db = TestDb::new();
    let run_id = test_db.run_id;
    let p = test_db.all_primitives();

    // KV
    p.kv.put(&run_id, "config", Value::String("enabled".into())).unwrap();

    // State
    p.state.init(&run_id, "status", Value::String("running".into())).unwrap();

    // Event
    p.event.append(&run_id, "lifecycle", string_payload("started")).unwrap();

    // JSON
    p.json.create(&run_id, "context", json_value(serde_json::json!({"task": "test"}))).unwrap();

    // Vector
    p.vector.create_collection(run_id, "memory", config_small()).unwrap();
    p.vector.insert(run_id, "memory", "m1", &[1.0, 0.0, 0.0], None).unwrap();

    // Run index - note: runs are implicitly created when data is written
    // The list_runs() only shows runs explicitly created via create_run()
    // Data can be stored in any RunId without explicit registration

    // Verify all readable
    assert!(p.kv.get(&run_id, "config").unwrap().is_some());
    assert!(p.state.read(&run_id, "status").unwrap().is_some());
    assert!(p.event.len(&run_id).unwrap() > 0);
    assert!(p.json.get(&run_id, "context", &root()).unwrap().is_some());
    assert!(p.vector.get(run_id, "memory", "m1").unwrap().is_some());
}

#[test]
fn cross_primitive_workflow_agent_memory() {
    let test_db = TestDb::new();
    let run_id = test_db.run_id;
    let p = test_db.all_primitives();

    // Agent initialization
    p.kv.put(&run_id, "agent:name", Value::String("assistant".into())).unwrap();
    p.state.init(&run_id, "agent:status", Value::String("initializing".into())).unwrap();
    p.event.append(&run_id, "agent:lifecycle", string_payload("Agent started")).unwrap();

    // Agent stores context
    p.json.create(&run_id, "agent:context", json_value(serde_json::json!({
        "task": "help_user",
        "turn": 0
    }))).unwrap();

    // Agent creates memory store
    p.vector.create_collection(run_id, "agent:memories", config_small()).unwrap();

    // Simulate processing turns
    for turn in 1..=3 {
        // Update context
        p.json.set(&run_id, "agent:context", &path(".turn"), json_value(serde_json::json!(turn))).unwrap();

        // Store memory
        p.vector.insert(
            run_id,
            "agent:memories",
            &format!("turn_{}", turn),
            &seeded_vector(3, turn as u64),
            Some(serde_json::json!({"turn": turn})),
        ).unwrap();

        // Log event
        p.event.append(&run_id, "agent:turns", int_payload(turn)).unwrap();
    }

    // Update status
    p.state.set(&run_id, "agent:status", Value::String("completed".into())).unwrap();
    p.event.append(&run_id, "agent:lifecycle", string_payload("Agent completed")).unwrap();

    // Verify final state
    let status = p.state.read(&run_id, "agent:status").unwrap().unwrap();
    assert_eq!(status.value.value, Value::String("completed".into()));

    assert_eq!(p.event.len_by_type(&run_id, "agent:turns").unwrap(), 3);
    assert_eq!(p.event.len_by_type(&run_id, "agent:lifecycle").unwrap(), 2);
    assert_eq!(p.vector.count(run_id, "agent:memories").unwrap(), 3);
}

#[test]
fn delete_in_one_primitive_doesnt_affect_others() {
    let test_db = TestDb::new();
    let run_id = test_db.run_id;
    let p = test_db.all_primitives();

    // Use same name across primitives
    p.kv.put(&run_id, "shared", Value::String("kv".into())).unwrap();
    p.state.init(&run_id, "shared", Value::String("state".into())).unwrap();
    p.json.create(&run_id, "shared", json_value(serde_json::json!({"type": "json"}))).unwrap();

    // Delete only from KV
    p.kv.delete(&run_id, "shared").unwrap();

    // Verify KV deleted but others remain
    assert!(p.kv.get(&run_id, "shared").unwrap().is_none());
    assert!(p.state.read(&run_id, "shared").unwrap().is_some());
    assert!(p.json.get(&run_id, "shared", &root()).unwrap().is_some());
}
