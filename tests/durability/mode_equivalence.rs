//! Durability Mode Equivalence Tests
//!
//! Verifies that in-memory, buffered, and strict modes produce
//! semantically identical results for the same workload.

use crate::common::*;

#[test]
fn kv_operations_equivalent_across_modes() {
    test_across_modes("kv_put_get", |db| {
        let run_id = RunId::new();
        let kv = KVStore::new(db);

        kv.put(&run_id, "a", Value::Int(1)).unwrap();
        kv.put(&run_id, "b", Value::Int(2)).unwrap();
        kv.put(&run_id, "c", Value::Int(3)).unwrap();

        let a = kv.get(&run_id, "a").unwrap().map(|v| v.value);
        let b = kv.get(&run_id, "b").unwrap().map(|v| v.value);
        let c = kv.get(&run_id, "c").unwrap().map(|v| v.value);
        (a, b, c)
    });
}

#[test]
fn json_operations_equivalent_across_modes() {
    test_across_modes("json_create_get", |db| {
        let run_id = RunId::new();
        let json = JsonStore::new(db);
        let doc_id = "mode_test_doc";

        json.create(
            &run_id,
            doc_id,
            json_value(serde_json::json!({"x": 1})),
        )
        .unwrap();

        let doc = json.get(&run_id, doc_id, &root()).unwrap();
        doc.map(|v| v.value.as_inner().clone())
    });
}

#[test]
fn event_operations_equivalent_across_modes() {
    test_across_modes("event_append_read", |db| {
        let run_id = RunId::new();
        let event = EventLog::new(db);

        event.append(&run_id, "stream", int_payload(1)).unwrap();
        event.append(&run_id, "stream", int_payload(2)).unwrap();
        event.append(&run_id, "stream", int_payload(3)).unwrap();

        let events = event.read_by_type(&run_id, "stream").unwrap();
        events.len() as u64
    });
}

#[test]
fn statecell_cas_equivalent_across_modes() {
    test_across_modes("statecell_cas", |db| {
        let run_id = RunId::new();
        let state = StateCell::new(db);

        let v = state.init(&run_id, "counter", Value::Int(0)).unwrap();
        state.cas(&run_id, "counter", v.value, Value::Int(1)).unwrap();

        let val = state.read(&run_id, "counter").unwrap();
        val.map(|v| format!("{:?}", v.value.value))
    });
}

#[test]
fn overwrite_semantics_equivalent_across_modes() {
    test_across_modes("overwrite", |db| {
        let run_id = RunId::new();
        let kv = KVStore::new(db);

        kv.put(&run_id, "key", Value::Int(1)).unwrap();
        kv.put(&run_id, "key", Value::Int(2)).unwrap();
        kv.put(&run_id, "key", Value::Int(3)).unwrap();

        kv.get(&run_id, "key").unwrap().map(|v| v.value)
    });
}

#[test]
fn delete_semantics_equivalent_across_modes() {
    test_across_modes("delete", |db| {
        let run_id = RunId::new();
        let kv = KVStore::new(db);

        kv.put(&run_id, "ephemeral", Value::Int(1)).unwrap();
        kv.delete(&run_id, "ephemeral").unwrap();

        kv.get(&run_id, "ephemeral").unwrap().is_none()
    });
}

#[test]
fn buffered_mode_recovers_after_restart() {
    let mut test_db = TestDb::new(); // buffered mode
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    for i in 0..20 {
        kv.put(&run_id, &format!("buf_{}", i), Value::Int(i)).unwrap();
    }

    let state_before = CapturedState::capture(&test_db.db, &run_id);

    // Reopen (triggers flush + recovery)
    test_db.reopen();

    let state_after = CapturedState::capture(&test_db.db, &run_id);
    assert_states_equal(
        &state_before,
        &state_after,
        "Buffered mode should recover all data",
    );
}

#[test]
fn strict_mode_recovers_after_restart() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    for i in 0..20 {
        kv.put(&run_id, &format!("strict_{}", i), Value::Int(i))
            .unwrap();
    }

    let state_before = CapturedState::capture(&test_db.db, &run_id);

    test_db.reopen();

    let state_after = CapturedState::capture(&test_db.db, &run_id);
    assert_states_equal(
        &state_before,
        &state_after,
        "Strict mode should recover all data",
    );
}
