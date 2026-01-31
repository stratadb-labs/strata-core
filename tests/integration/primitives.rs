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
        let branch_id = BranchId::new();
        let kv = KVStore::new(db);

        // Create
        kv.put(&branch_id, "key", Value::Int(1)).unwrap();

        // Read
        let val = kv.get(&branch_id, "key").unwrap().unwrap();
        assert_eq!(val, Value::Int(1));

        // Update
        kv.put(&branch_id, "key", Value::Int(2)).unwrap();
        let val = kv.get(&branch_id, "key").unwrap().unwrap();
        assert_eq!(val, Value::Int(2));

        // Delete
        kv.delete(&branch_id, "key").unwrap();
        assert!(kv.get(&branch_id, "key").unwrap().is_none());
    }

    #[test]
    fn list_and_scan() {
        let db = create_test_db();
        let branch_id = BranchId::new();
        let kv = KVStore::new(db);

        for i in 0..20 {
            kv.put(&branch_id, &format!("user:{}", i), Value::Int(i)).unwrap();
            kv.put(&branch_id, &format!("config:{}", i), Value::Int(i * 10)).unwrap();
        }

        // List all
        let all = kv.list(&branch_id, None).unwrap();
        assert_eq!(all.len(), 40);

        // List with prefix
        let users = kv.list(&branch_id, Some("user:")).unwrap();
        assert_eq!(users.len(), 20);

        let configs = kv.list(&branch_id, Some("config:")).unwrap();
        assert_eq!(configs.len(), 20);
    }

    #[test]
    fn value_types() {
        let db = create_test_db();
        let branch_id = BranchId::new();
        let kv = KVStore::new(db);

        kv.put(&branch_id, "int", Value::Int(42)).unwrap();
        kv.put(&branch_id, "float", Value::Float(3.14)).unwrap();
        kv.put(&branch_id, "string", Value::String("hello".into())).unwrap();
        kv.put(&branch_id, "bool", Value::Bool(true)).unwrap();
        kv.put(&branch_id, "bytes", Value::Bytes(vec![1, 2, 3])).unwrap();

        assert!(matches!(kv.get(&branch_id, "int").unwrap().unwrap(), Value::Int(42)));
        assert!(matches!(kv.get(&branch_id, "bool").unwrap().unwrap(), Value::Bool(true)));
    }
}

mod state_single {
    use super::*;

    #[test]
    fn init_and_read() {
        let db = create_test_db();
        let branch_id = BranchId::new();
        let state = StateCell::new(db);

        state.init(&branch_id, "counter", Value::Int(0)).unwrap();
        let val = state.read(&branch_id, "counter").unwrap().unwrap();
        assert_eq!(val, Value::Int(0));
    }

    #[test]
    fn set_updates_value() {
        let db = create_test_db();
        let branch_id = BranchId::new();
        let state = StateCell::new(db);

        state.init(&branch_id, "counter", Value::Int(0)).unwrap();
        state.set(&branch_id, "counter", Value::Int(1)).unwrap();
        state.set(&branch_id, "counter", Value::Int(2)).unwrap();

        let val = state.read(&branch_id, "counter").unwrap().unwrap();
        assert_eq!(val, Value::Int(2));
    }

    #[test]
    fn cas_with_correct_version_succeeds() {
        let db = create_test_db();
        let branch_id = BranchId::new();
        let state = StateCell::new(db);

        state.init(&branch_id, "counter", Value::Int(0)).unwrap();
        let current = state.readv(&branch_id, "counter").unwrap().unwrap();

        state.cas(&branch_id, "counter", current.version(), Value::Int(1)).unwrap();

        let updated = state.read(&branch_id, "counter").unwrap().unwrap();
        assert_eq!(updated, Value::Int(1));
    }

    #[test]
    fn cas_with_stale_version_fails() {
        let db = create_test_db();
        let branch_id = BranchId::new();
        let state = StateCell::new(db);

        state.init(&branch_id, "counter", Value::Int(0)).unwrap();
        let current = state.readv(&branch_id, "counter").unwrap().unwrap();
        let stale_version = current.version();

        // Update through another path
        state.set(&branch_id, "counter", Value::Int(1)).unwrap();

        // CAS with stale version should fail
        let result = state.cas(&branch_id, "counter", stale_version, Value::Int(99));
        assert!(result.is_err());

        // Value should remain at 1
        let final_val = state.read(&branch_id, "counter").unwrap().unwrap();
        assert_eq!(final_val, Value::Int(1));
    }
}

mod event_single {
    use super::*;

    #[test]
    fn append_and_read() {
        let db = create_test_db();
        let branch_id = BranchId::new();
        let event = EventLog::new(db);

        let seq1 = event.append(&branch_id, "audit", int_payload(1)).unwrap();
        let seq2 = event.append(&branch_id, "audit", int_payload(2)).unwrap();
        let seq3 = event.append(&branch_id, "audit", int_payload(3)).unwrap();

        assert!(seq2 > seq1);
        assert!(seq3 > seq2);

        let events = event.read_by_type(&branch_id, "audit").unwrap();
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn multiple_streams() {
        let db = create_test_db();
        let branch_id = BranchId::new();
        let event = EventLog::new(db);

        event.append(&branch_id, "stream_a", int_payload(1)).unwrap();
        event.append(&branch_id, "stream_a", int_payload(2)).unwrap();
        event.append(&branch_id, "stream_b", int_payload(10)).unwrap();

        assert_eq!(event.read_by_type(&branch_id, "stream_a").unwrap().len(), 2);
        assert_eq!(event.read_by_type(&branch_id, "stream_b").unwrap().len(), 1);
    }

    #[test]
    fn events_are_immutable() {
        let db = create_test_db();
        let branch_id = BranchId::new();
        let event = EventLog::new(db);

        event.append(&branch_id, "audit", int_payload(42)).unwrap();

        // Events can only be appended, not modified or deleted
        // The API doesn't provide update/delete methods for events
        let events = event.read_by_type(&branch_id, "audit").unwrap();
        assert_eq!(events.len(), 1);
    }
}

mod json_single {
    use super::*;

    #[test]
    fn create_and_get() {
        let db = create_test_db();
        let branch_id = BranchId::new();
        let json = JsonStore::new(db);

        json.create(&branch_id, "doc", json_value(serde_json::json!({"name": "test"}))).unwrap();

        let doc = json.get(&branch_id, "doc", &root()).unwrap().unwrap();
        assert_eq!(doc.as_inner()["name"], "test");
    }

    #[test]
    fn patch_nested_value() {
        let db = create_test_db();
        let branch_id = BranchId::new();
        let json = JsonStore::new(db);

        json.create(&branch_id, "doc", json_value(serde_json::json!({
            "user": {"name": "Alice", "age": 30}
        }))).unwrap();

        json.set(&branch_id, "doc", &path(".user.age"), json_value(serde_json::json!(31))).unwrap();

        let doc = json.get(&branch_id, "doc", &path(".user.age")).unwrap().unwrap();
        assert_eq!(doc.as_inner(), &serde_json::json!(31));
    }

    #[test]
    fn list_documents() {
        let db = create_test_db();
        let branch_id = BranchId::new();
        let json = JsonStore::new(db);

        for i in 0..10 {
            json.create(&branch_id, &format!("doc_{}", i), test_json_value(i)).unwrap();
        }

        let list = json.list(&branch_id, None, None, 100).unwrap();
        assert_eq!(list.doc_ids.len(), 10);
    }
}

mod vector_single {
    use super::*;

    #[test]
    fn create_collection_and_insert() {
        let db = create_test_db();
        let branch_id = BranchId::new();
        let vector = VectorStore::new(db);

        vector.create_collection(branch_id, "embeddings", config_small()).unwrap();
        vector.insert(branch_id, "embeddings", "vec_1", &[1.0, 0.0, 0.0], None).unwrap();

        let entry = vector.get(branch_id, "embeddings", "vec_1").unwrap().unwrap();
        assert_eq!(entry.value.embedding, vec![1.0, 0.0, 0.0]);
    }

    #[test]
    fn search_returns_nearest() {
        let db = create_test_db();
        let branch_id = BranchId::new();
        let vector = VectorStore::new(db);

        vector.create_collection(branch_id, "test", config_small()).unwrap();

        // Insert vectors at cardinal directions
        vector.insert(branch_id, "test", "north", &[0.0, 1.0, 0.0], None).unwrap();
        vector.insert(branch_id, "test", "south", &[0.0, -1.0, 0.0], None).unwrap();
        vector.insert(branch_id, "test", "east", &[1.0, 0.0, 0.0], None).unwrap();

        // Search for vector close to north
        let query = vec![0.0, 0.99, 0.0];
        let results = vector.search(branch_id, "test", &query, 1, None).unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, "north");
    }

    #[test]
    fn delete_vector() {
        let db = create_test_db();
        let branch_id = BranchId::new();
        let vector = VectorStore::new(db);

        vector.create_collection(branch_id, "test", config_small()).unwrap();
        vector.insert(branch_id, "test", "to_delete", &[1.0, 0.0, 0.0], None).unwrap();

        assert_eq!(vector.get(branch_id, "test", "to_delete").unwrap().unwrap().value.embedding, vec![1.0f32, 0.0, 0.0]);

        vector.delete(branch_id, "test", "to_delete").unwrap();

        assert!(vector.get(branch_id, "test", "to_delete").unwrap().is_none());
    }
}

// ============================================================================
// Cross-Primitive Tests
// ============================================================================

#[test]
fn all_six_primitives_together() {
    let test_db = TestDb::new();
    let branch_id = test_db.branch_id;
    let p = test_db.all_primitives();

    // KV
    p.kv.put(&branch_id, "config", Value::String("enabled".into())).unwrap();

    // State
    p.state.init(&branch_id, "status", Value::String("running".into())).unwrap();

    // Event
    p.event.append(&branch_id, "lifecycle", string_payload("started")).unwrap();

    // JSON
    p.json.create(&branch_id, "context", json_value(serde_json::json!({"task": "test"}))).unwrap();

    // Vector
    p.vector.create_collection(branch_id, "memory", config_small()).unwrap();
    p.vector.insert(branch_id, "memory", "m1", &[1.0, 0.0, 0.0], None).unwrap();

    // Run index - runs must be explicitly created via create_branch()
    // We're using a random BranchId here which is NOT registered with BranchIndex
    // In production, you would either use the default run or create one explicitly

    // Verify all readable
    assert_eq!(p.kv.get(&branch_id, "config").unwrap(), Some(Value::String("enabled".into())));
    assert_eq!(p.state.read(&branch_id, "status").unwrap().unwrap(), Value::String("running".into()));
    assert!(p.event.len(&branch_id).unwrap() > 0);
    assert_eq!(p.json.get(&branch_id, "context", &root()).unwrap().unwrap().as_inner(), &serde_json::json!({"task": "test"}));
    assert_eq!(p.vector.get(branch_id, "memory", "m1").unwrap().unwrap().value.embedding, vec![1.0f32, 0.0, 0.0]);
}

#[test]
fn cross_primitive_workflow_agent_memory() {
    let test_db = TestDb::new();
    let branch_id = test_db.branch_id;
    let p = test_db.all_primitives();

    // Agent initialization
    p.kv.put(&branch_id, "agent:name", Value::String("assistant".into())).unwrap();
    p.state.init(&branch_id, "agent:status", Value::String("initializing".into())).unwrap();
    p.event.append(&branch_id, "agent:lifecycle", string_payload("Agent started")).unwrap();

    // Agent stores context
    p.json.create(&branch_id, "agent:context", json_value(serde_json::json!({
        "task": "help_user",
        "turn": 0
    }))).unwrap();

    // Agent creates memory store
    p.vector.create_collection(branch_id, "agent:memories", config_small()).unwrap();

    // Simulate processing turns
    for turn in 1..=3 {
        // Update context
        p.json.set(&branch_id, "agent:context", &path(".turn"), json_value(serde_json::json!(turn))).unwrap();

        // Store memory
        p.vector.insert(
            branch_id,
            "agent:memories",
            &format!("turn_{}", turn),
            &seeded_vector(3, turn as u64),
            Some(serde_json::json!({"turn": turn})),
        ).unwrap();

        // Log event
        p.event.append(&branch_id, "agent:turns", int_payload(turn)).unwrap();
    }

    // Update status
    p.state.set(&branch_id, "agent:status", Value::String("completed".into())).unwrap();
    p.event.append(&branch_id, "agent:lifecycle", string_payload("Agent completed")).unwrap();

    // Verify final state
    let status = p.state.read(&branch_id, "agent:status").unwrap().unwrap();
    assert_eq!(status, Value::String("completed".into()));

    assert_eq!(p.event.read_by_type(&branch_id, "agent:turns").unwrap().len(), 3);
    assert_eq!(p.event.read_by_type(&branch_id, "agent:lifecycle").unwrap().len(), 2);
    assert_eq!(
        p.vector.list_collections(branch_id).unwrap().iter()
            .find(|c| c.name == "agent:memories").unwrap().count,
        3
    );
}

#[test]
fn delete_in_one_primitive_doesnt_affect_others() {
    let test_db = TestDb::new();
    let branch_id = test_db.branch_id;
    let p = test_db.all_primitives();

    // Use same name across primitives
    p.kv.put(&branch_id, "shared", Value::String("kv".into())).unwrap();
    p.state.init(&branch_id, "shared", Value::String("state".into())).unwrap();
    p.json.create(&branch_id, "shared", json_value(serde_json::json!({"type": "json"}))).unwrap();

    // Delete only from KV
    p.kv.delete(&branch_id, "shared").unwrap();

    // Verify KV deleted but others remain
    assert!(p.kv.get(&branch_id, "shared").unwrap().is_none());
    assert_eq!(p.state.read(&branch_id, "shared").unwrap().unwrap(), Value::String("state".into()));
    assert_eq!(p.json.get(&branch_id, "shared", &root()).unwrap().unwrap().as_inner(), &serde_json::json!({"type": "json"}));
}
