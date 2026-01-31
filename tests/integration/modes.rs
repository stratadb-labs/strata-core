//! Storage and Durability Mode Tests
//!
//! Tests behavior across:
//! - Ephemeral (no disk) vs Persistent (disk-backed)
//! - Durability: None (no sync), Batched (periodic sync), Strict (immediate sync)

use crate::common::*;
use std::sync::Arc;
use tempfile::TempDir;

// ============================================================================
// Mode Creation Helpers
// ============================================================================

fn create_ephemeral() -> Arc<Database> {
    Database::ephemeral().expect("ephemeral db")
}

fn create_persistent_no_durability(dir: &TempDir) -> Arc<Database> {
    Database::builder()
        .path(dir.path())
        .no_durability()
        .open()
        .expect("no-durability db")
}

fn create_persistent_batched(dir: &TempDir) -> Arc<Database> {
    Database::builder()
        .path(dir.path())
        .buffered()
        .open()
        .expect("batched db")
}

fn create_persistent_strict(dir: &TempDir) -> Arc<Database> {
    Database::builder()
        .path(dir.path())
        .strict()
        .open()
        .expect("strict db")
}

// ============================================================================
// Ephemeral Mode Tests
// ============================================================================

#[test]
fn ephemeral_basic_operations() {
    let db = create_ephemeral();
    let branch_id = BranchId::new();
    let kv = KVStore::new(db);

    // Basic write/read cycle
    kv.put(&branch_id, "key", Value::Int(42)).unwrap();
    let value = kv.get(&branch_id, "key").unwrap().unwrap();
    assert_eq!(value, Value::Int(42));
}

#[test]
fn ephemeral_all_primitives() {
    let db = create_ephemeral();
    let branch_id = BranchId::new();

    let kv = KVStore::new(db.clone());
    let state = StateCell::new(db.clone());
    let event = EventLog::new(db.clone());
    let json = JsonStore::new(db.clone());
    let vector = VectorStore::new(db.clone());

    // KV
    kv.put(&branch_id, "k", Value::Int(1)).unwrap();
    assert_eq!(kv.get(&branch_id, "k").unwrap(), Some(Value::Int(1)));

    // State
    state.init(&branch_id, "s", Value::Int(2)).unwrap();
    assert_eq!(state.read(&branch_id, "s").unwrap().unwrap(), Value::Int(2));

    // Event
    event.append(&branch_id, "stream", int_payload(3)).unwrap();
    assert!(event.len(&branch_id).unwrap() > 0);

    // JSON
    json.create(&branch_id, "doc", json_value(serde_json::json!({"x": 4}))).unwrap();
    assert_eq!(json.get(&branch_id, "doc", &root()).unwrap().unwrap().as_inner(), &serde_json::json!({"x": 4}));

    // Vector
    vector.create_collection(branch_id, "coll", config_small()).unwrap();
    vector.insert(branch_id, "coll", "v", &[1.0, 0.0, 0.0], None).unwrap();
    assert_eq!(vector.get(branch_id, "coll", "v").unwrap().unwrap().value.embedding, vec![1.0f32, 0.0, 0.0]);
}

#[test]
fn ephemeral_data_is_lost_on_drop() {
    let branch_id = BranchId::new();

    // Write data
    {
        let db = create_ephemeral();
        let kv = KVStore::new(db);
        kv.put(&branch_id, "ephemeral_key", Value::Int(42)).unwrap();
    }

    // New ephemeral database should have no data
    let db = create_ephemeral();
    let kv = KVStore::new(db);
    // Note: Different ephemeral instance, so this key won't exist
    assert!(kv.get(&branch_id, "ephemeral_key").unwrap().is_none());
}

// ============================================================================
// Persistent Mode Tests
// ============================================================================

#[test]
fn persistent_no_durability_basic() {
    let dir = TempDir::new().unwrap();
    let db = create_persistent_no_durability(&dir);
    let branch_id = BranchId::new();
    let kv = KVStore::new(db);

    kv.put(&branch_id, "key", Value::Int(42)).unwrap();
    let value = kv.get(&branch_id, "key").unwrap().unwrap();
    assert_eq!(value, Value::Int(42));
}

#[test]
fn persistent_batched_basic() {
    let dir = TempDir::new().unwrap();
    let db = create_persistent_batched(&dir);
    let branch_id = BranchId::new();
    let kv = KVStore::new(db);

    kv.put(&branch_id, "key", Value::Int(42)).unwrap();
    let value = kv.get(&branch_id, "key").unwrap().unwrap();
    assert_eq!(value, Value::Int(42));
}

#[test]
fn persistent_strict_basic() {
    let dir = TempDir::new().unwrap();
    let db = create_persistent_strict(&dir);
    let branch_id = BranchId::new();
    let kv = KVStore::new(db);

    kv.put(&branch_id, "key", Value::Int(42)).unwrap();
    let value = kv.get(&branch_id, "key").unwrap().unwrap();
    assert_eq!(value, Value::Int(42));
}

// ============================================================================
// Recovery Tests (Strict Mode Only)
// ============================================================================

#[test]
fn strict_mode_survives_reopen() {
    let dir = TempDir::new().unwrap();
    let branch_id = BranchId::new();

    // Write with strict durability
    {
        let db = create_persistent_strict(&dir);
        let kv = KVStore::new(db);
        for i in 0..100 {
            kv.put(&branch_id, &format!("key_{}", i), Value::Int(i)).unwrap();
        }
    }

    // Reopen and verify
    {
        let db = create_persistent_strict(&dir);
        let kv = KVStore::new(db);
        for i in 0..100 {
            let val = kv.get(&branch_id, &format!("key_{}", i)).unwrap();
            assert!(val.is_some(), "Key {} should survive reopen", i);
            assert_eq!(val.unwrap(), Value::Int(i));
        }
    }
}

#[test]
fn strict_mode_all_primitives_survive_reopen() {
    let dir = TempDir::new().unwrap();
    let branch_id = BranchId::new();

    // Write to all primitives
    {
        let db = create_persistent_strict(&dir);

        let kv = KVStore::new(db.clone());
        kv.put(&branch_id, "kv_key", Value::String("kv_val".into())).unwrap();

        let state = StateCell::new(db.clone());
        state.init(&branch_id, "state_cell", Value::Int(42)).unwrap();

        let event = EventLog::new(db.clone());
        event.append(&branch_id, "audit", int_payload(123)).unwrap();

        let json = JsonStore::new(db.clone());
        json.create(&branch_id, "doc", json_value(serde_json::json!({"k": "v"}))).unwrap();

        let vector = VectorStore::new(db.clone());
        vector.create_collection(branch_id, "coll", config_small()).unwrap();
        vector.insert(branch_id, "coll", "vec", &[1.0, 0.0, 0.0], None).unwrap();
    }

    // Reopen and verify all primitives
    {
        let db = create_persistent_strict(&dir);

        let kv = KVStore::new(db.clone());
        assert_eq!(kv.get(&branch_id, "kv_key").unwrap(), Some(Value::String("kv_val".into())));

        let state = StateCell::new(db.clone());
        assert_eq!(state.read(&branch_id, "state_cell").unwrap().unwrap(), Value::Int(42));

        let event = EventLog::new(db.clone());
        assert!(event.len(&branch_id).unwrap() > 0);

        let json = JsonStore::new(db.clone());
        assert_eq!(json.get(&branch_id, "doc", &root()).unwrap().unwrap().as_inner(), &serde_json::json!({"k": "v"}));

        let vector = VectorStore::new(db.clone());
        assert_eq!(vector.get(branch_id, "coll", "vec").unwrap().unwrap().value.embedding, vec![1.0f32, 0.0, 0.0]);
    }
}

// ============================================================================
// Mode Equivalence Tests
// ============================================================================

/// Verify that all modes produce the same results for the same operations
#[test]
fn all_modes_produce_same_results() {
    let branch_id = BranchId::new();

    // Test workload
    fn workload(db: Arc<Database>, branch_id: BranchId) -> Vec<i64> {
        let kv = KVStore::new(db);
        for i in 0..10 {
            kv.put(&branch_id, &format!("key_{}", i), Value::Int(i)).unwrap();
        }

        let mut results = Vec::new();
        for i in 0..10 {
            if let Some(v) = kv.get(&branch_id, &format!("key_{}", i)).unwrap() {
                if let Value::Int(n) = v {
                    results.push(n);
                }
            }
        }
        results
    }

    // Run workload on each mode
    let ephemeral_result = workload(create_ephemeral(), branch_id);

    let dir1 = TempDir::new().unwrap();
    let no_dur_result = workload(create_persistent_no_durability(&dir1), branch_id);

    let dir2 = TempDir::new().unwrap();
    let batched_result = workload(create_persistent_batched(&dir2), branch_id);

    let dir3 = TempDir::new().unwrap();
    let strict_result = workload(create_persistent_strict(&dir3), branch_id);

    // All should produce identical results
    assert_eq!(ephemeral_result, no_dur_result, "Ephemeral != NoDurability");
    assert_eq!(no_dur_result, batched_result, "NoDurability != Batched");
    assert_eq!(batched_result, strict_result, "Batched != Strict");
}

// ============================================================================
// Performance Characteristics (Verify Mode Properties)
// ============================================================================

#[test]
fn ephemeral_mode_is_fast() {
    let db = create_ephemeral();
    let branch_id = BranchId::new();
    let kv = KVStore::new(db);

    let start = std::time::Instant::now();
    for i in 0..10_000 {
        kv.put(&branch_id, &format!("key_{}", i), Value::Int(i)).unwrap();
    }
    let elapsed = start.elapsed();

    // Ephemeral should be very fast (no disk I/O)
    assert!(
        elapsed.as_millis() < 5000,
        "Ephemeral 10k writes took {:?}, expected < 5s",
        elapsed
    );
}

#[test]
fn strict_mode_is_durable() {
    let dir = TempDir::new().unwrap();
    let branch_id = BranchId::new();

    // Write single important value with strict mode
    {
        let db = create_persistent_strict(&dir);
        let kv = KVStore::new(db);
        kv.put(&branch_id, "critical", Value::String("important_data".into())).unwrap();
        // Strict mode syncs on every write - no explicit flush needed
    }

    // Simulate crash by just dropping the database
    // Then reopen and verify

    {
        let db = create_persistent_strict(&dir);
        let kv = KVStore::new(db);
        let val = kv.get(&branch_id, "critical").unwrap();
        assert!(val.is_some(), "Critical data should survive in strict mode");
        assert_eq!(val.unwrap(), Value::String("important_data".into()));
    }
}
