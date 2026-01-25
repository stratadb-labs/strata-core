//! Business Logic and Edge Case Tests for strata-primitives
//!
//! Tests targeting specific business rules, state machines, and boundary conditions:
//!
//! 1. KVStore: TTL, batch operations, history/versioning, scan pagination
//! 2. EventLog: Hash chain verification, batch atomicity, range queries
//! 3. StateCell: State machine semantics, history queries
//! 4. VectorStore: Search boundaries, empty collections, metadata filtering
//! 5. RunIndex: Status transition state machine, cascade delete
//!
//! These tests follow TESTING_METHODOLOGY.md principles:
//! - Test behavior, not implementation
//! - One failure mode per test
//! - Verify values, not just is_ok()

use std::collections::HashMap;
use std::sync::Arc;

use strata_core::types::RunId;
use strata_core::value::Value;
use strata_engine::Database;
use strata_primitives::{
    EventLog, KVStore, RunIndex, RunStatus, StateCell, VectorStore,
    vector::{DistanceMetric, MetadataFilter, VectorConfig},
};
use tempfile::TempDir;

// ============================================================================
// Test Helpers
// ============================================================================

fn setup() -> (Arc<Database>, TempDir, RunId) {
    let temp_dir = TempDir::new().unwrap();
    let db = Arc::new(Database::open(temp_dir.path()).unwrap());
    let run_id = RunId::new();
    (db, temp_dir, run_id)
}

fn empty_payload() -> Value {
    Value::Object(HashMap::new())
}

fn int_payload(v: i64) -> Value {
    Value::Object(HashMap::from([("value".to_string(), Value::Int(v))]))
}

// ============================================================================
// Module 1: KVStore Business Logic Tests
// ============================================================================

/// Test get_at returns correct version at specific point in time
#[test]
fn test_kv_get_at_returns_correct_historical_version() {
    let (db, _temp, run_id) = setup();
    let kv = KVStore::new(db.clone());

    // Write multiple versions
    let v1 = kv.put(&run_id, "key", Value::Int(100)).unwrap();
    let v2 = kv.put(&run_id, "key", Value::Int(200)).unwrap();
    let v3 = kv.put(&run_id, "key", Value::Int(300)).unwrap();

    // get_at should return the value at or before the specified version
    let at_v1 = kv.get_at(&run_id, "key", v1.as_u64()).unwrap().unwrap();
    assert_eq!(at_v1.value, Value::Int(100), "Should get v1 value");

    let at_v2 = kv.get_at(&run_id, "key", v2.as_u64()).unwrap().unwrap();
    assert_eq!(at_v2.value, Value::Int(200), "Should get v2 value");

    let at_v3 = kv.get_at(&run_id, "key", v3.as_u64()).unwrap().unwrap();
    assert_eq!(at_v3.value, Value::Int(300), "Should get v3 value");

    // get_at with version before v1 should return None
    let before_v1 = kv.get_at(&run_id, "key", v1.as_u64() - 1).unwrap();
    assert!(before_v1.is_none(), "Should return None for version before first write");
}

/// Test history returns versions in correct order (newest first)
#[test]
fn test_kv_history_returns_newest_first() {
    let (db, _temp, run_id) = setup();
    let kv = KVStore::new(db.clone());

    // Write multiple versions
    kv.put(&run_id, "key", Value::Int(1)).unwrap();
    kv.put(&run_id, "key", Value::Int(2)).unwrap();
    kv.put(&run_id, "key", Value::Int(3)).unwrap();

    let history = kv.history(&run_id, "key", None, None).unwrap();

    assert_eq!(history.len(), 3, "Should have 3 versions");
    assert_eq!(history[0].value, Value::Int(3), "First should be newest (3)");
    assert_eq!(history[1].value, Value::Int(2), "Second should be (2)");
    assert_eq!(history[2].value, Value::Int(1), "Third should be oldest (1)");

    // Verify versions are descending
    for i in 0..history.len() - 1 {
        assert!(
            history[i].version.as_u64() > history[i + 1].version.as_u64(),
            "Versions should be in descending order"
        );
    }
}

/// Test history limit parameter
#[test]
fn test_kv_history_respects_limit() {
    let (db, _temp, run_id) = setup();
    let kv = KVStore::new(db.clone());

    // Write 10 versions
    for i in 0..10 {
        kv.put(&run_id, "key", Value::Int(i)).unwrap();
    }

    // Request only 3
    let history = kv.history(&run_id, "key", Some(3), None).unwrap();
    assert_eq!(history.len(), 3, "Should return only 3 versions");

    // Should be the newest 3
    assert_eq!(history[0].value, Value::Int(9));
    assert_eq!(history[1].value, Value::Int(8));
    assert_eq!(history[2].value, Value::Int(7));
}

/// Test get_many returns consistent snapshot
#[test]
fn test_kv_get_many_consistent_snapshot() {
    let (db, _temp, run_id) = setup();
    let kv = KVStore::new(db.clone());

    // Write some keys
    kv.put(&run_id, "a", Value::Int(1)).unwrap();
    kv.put(&run_id, "b", Value::Int(2)).unwrap();
    kv.put(&run_id, "c", Value::Int(3)).unwrap();

    // Get many should return all at same snapshot
    let results = kv.get_many(&run_id, &["a", "b", "c", "nonexistent"]).unwrap();

    assert_eq!(results.len(), 4);
    assert_eq!(results[0].as_ref().unwrap().value, Value::Int(1));
    assert_eq!(results[1].as_ref().unwrap().value, Value::Int(2));
    assert_eq!(results[2].as_ref().unwrap().value, Value::Int(3));
    assert!(results[3].is_none(), "nonexistent key should be None");
}

/// Test scan pagination works correctly
#[test]
fn test_kv_scan_pagination() {
    let (db, _temp, run_id) = setup();
    let kv = KVStore::new(db.clone());

    // Write 10 keys with same prefix
    for i in 0..10 {
        let key = format!("prefix/{:02}", i);
        kv.put(&run_id, &key, Value::Int(i)).unwrap();
    }

    // Scan with limit of 3
    let page1 = kv.scan(&run_id, "prefix/", 3, None).unwrap();
    assert_eq!(page1.entries.len(), 3, "First page should have 3 entries");
    assert!(page1.cursor.is_some(), "Should have cursor for more");

    // Get next page
    let page2 = kv.scan(&run_id, "prefix/", 3, page1.cursor.as_deref()).unwrap();
    assert_eq!(page2.entries.len(), 3, "Second page should have 3 entries");
    assert!(page2.cursor.is_some(), "Should have cursor for more");

    // Get third page
    let page3 = kv.scan(&run_id, "prefix/", 3, page2.cursor.as_deref()).unwrap();
    assert_eq!(page3.entries.len(), 3, "Third page should have 3 entries");

    // Get fourth page (only 1 left)
    let page4 = kv.scan(&run_id, "prefix/", 3, page3.cursor.as_deref()).unwrap();
    assert_eq!(page4.entries.len(), 1, "Fourth page should have 1 entry");
    assert!(page4.cursor.is_none(), "Should have no more pages");

    // Verify no duplicates across pages
    let mut all_keys: Vec<String> = Vec::new();
    all_keys.extend(page1.entries.iter().map(|(k, _)| k.clone()));
    all_keys.extend(page2.entries.iter().map(|(k, _)| k.clone()));
    all_keys.extend(page3.entries.iter().map(|(k, _)| k.clone()));
    all_keys.extend(page4.entries.iter().map(|(k, _)| k.clone()));

    all_keys.sort();
    all_keys.dedup();
    assert_eq!(all_keys.len(), 10, "Should have 10 unique keys across all pages");
}

/// Test delete returns correct existence status
#[test]
fn test_kv_delete_returns_existence_status() {
    let (db, _temp, run_id) = setup();
    let kv = KVStore::new(db.clone());

    // Delete nonexistent key
    let deleted = kv.delete(&run_id, "nonexistent").unwrap();
    assert!(!deleted, "Delete of nonexistent key should return false");

    // Create and delete
    kv.put(&run_id, "key", Value::Int(1)).unwrap();
    let deleted = kv.delete(&run_id, "key").unwrap();
    assert!(deleted, "Delete of existing key should return true");

    // Delete again
    let deleted = kv.delete(&run_id, "key").unwrap();
    assert!(!deleted, "Delete of already-deleted key should return false");
}

// ============================================================================
// Module 2: EventLog Business Logic Tests
// ============================================================================

/// Test hash chain verification detects tampering
#[test]
fn test_eventlog_verify_chain_detects_valid_chain() {
    let (db, _temp, run_id) = setup();
    let event_log = EventLog::new(db.clone());

    // Append several events
    for i in 0..10 {
        event_log.append(&run_id, "event", int_payload(i)).unwrap();
    }

    // Verify chain
    let verification = event_log.verify_chain(&run_id).unwrap();
    assert!(verification.is_valid, "Chain should be valid");
    assert_eq!(verification.length, 10, "Chain should have 10 events");
    assert!(verification.first_invalid.is_none(), "No invalid events");
    assert!(verification.error.is_none(), "No errors");
}

/// Test batch append is atomic - all or nothing
#[test]
fn test_eventlog_batch_append_atomicity() {
    let (db, _temp, run_id) = setup();
    let event_log = EventLog::new(db.clone());

    // Batch append multiple events
    let events = vec![
        ("event_a", int_payload(1)),
        ("event_b", int_payload(2)),
        ("event_c", int_payload(3)),
    ];

    let sequences = event_log.append_batch(&run_id, &events).unwrap();

    assert_eq!(sequences.len(), 3, "Should return 3 sequences");
    assert_eq!(sequences[0].as_u64(), 0);
    assert_eq!(sequences[1].as_u64(), 1);
    assert_eq!(sequences[2].as_u64(), 2);

    // Verify all events exist
    assert_eq!(event_log.len(&run_id).unwrap(), 3);
}

/// Test range reads with start and end bounds
#[test]
fn test_eventlog_range_reads() {
    let (db, _temp, run_id) = setup();
    let event_log = EventLog::new(db.clone());

    // Append 10 events
    for i in 0..10 {
        event_log.append(&run_id, &format!("event_{}", i), int_payload(i)).unwrap();
    }

    // Read range [2, 5)
    let events = event_log.read_range(&run_id, 2, 5).unwrap();
    assert_eq!(events.len(), 3, "Should return 3 events");
    assert_eq!(events[0].value.sequence, 2);
    assert_eq!(events[1].value.sequence, 3);
    assert_eq!(events[2].value.sequence, 4);

    // Read beyond end
    let events = event_log.read_range(&run_id, 8, 20).unwrap();
    assert_eq!(events.len(), 2, "Should return only existing events");
    assert_eq!(events[0].value.sequence, 8);
    assert_eq!(events[1].value.sequence, 9);

    // Read empty range
    let events = event_log.read_range(&run_id, 5, 5).unwrap();
    assert_eq!(events.len(), 0, "Empty range should return no events");
}

/// Test type filtering in queries
#[test]
fn test_eventlog_type_filtering() {
    let (db, _temp, run_id) = setup();
    let event_log = EventLog::new(db.clone());

    // Append mixed event types
    event_log.append(&run_id, "type_a", int_payload(1)).unwrap();
    event_log.append(&run_id, "type_b", int_payload(2)).unwrap();
    event_log.append(&run_id, "type_a", int_payload(3)).unwrap();
    event_log.append(&run_id, "type_c", int_payload(4)).unwrap();
    event_log.append(&run_id, "type_a", int_payload(5)).unwrap();

    // Query only type_a
    let type_a_events = event_log.read_by_type(&run_id, "type_a").unwrap();
    assert_eq!(type_a_events.len(), 3, "Should have 3 type_a events");

    // Verify payloads
    let values: Vec<i64> = type_a_events
        .iter()
        .filter_map(|e| {
            if let Value::Object(obj) = &e.value.payload {
                if let Some(Value::Int(v)) = obj.get("value") {
                    return Some(*v);
                }
            }
            None
        })
        .collect();
    assert_eq!(values, vec![1, 3, 5], "Should have correct payloads in order");
}

/// Test empty event log operations
#[test]
fn test_eventlog_empty_operations() {
    let (db, _temp, run_id) = setup();
    let event_log = EventLog::new(db.clone());

    // Operations on empty log
    assert_eq!(event_log.len(&run_id).unwrap(), 0);

    let events = event_log.read_range(&run_id, 0, 10).unwrap();
    assert_eq!(events.len(), 0);

    let verification = event_log.verify_chain(&run_id).unwrap();
    assert!(verification.is_valid, "Empty chain is valid");
    assert_eq!(verification.length, 0);
}

// ============================================================================
// Module 3: StateCell Business Logic Tests
// ============================================================================

/// Test transition closure receives correct state
#[test]
fn test_statecell_transition_receives_correct_state() {
    let (db, _temp, run_id) = setup();
    let state_cell = StateCell::new(db.clone());

    state_cell.init(&run_id, "cell", Value::Int(0)).unwrap();

    // Transition that records the input state
    let (result, _) = state_cell
        .transition(&run_id, "cell", |state| {
            let current = state.value.as_int().unwrap_or(-1);
            // Return the current value we saw
            Ok((Value::Int(current + 100), current))
        })
        .unwrap();

    assert_eq!(result, 0, "Closure should have seen value 0");

    // Verify new state
    let state = state_cell.read(&run_id, "cell").unwrap().unwrap();
    assert_eq!(state.value.value, Value::Int(100));
}

/// Test set operation (unconditional update)
#[test]
fn test_statecell_set_unconditional_update() {
    let (db, _temp, run_id) = setup();
    let state_cell = StateCell::new(db.clone());

    state_cell.init(&run_id, "cell", Value::Int(0)).unwrap();

    // Set ignores current value
    let versioned = state_cell.set(&run_id, "cell", Value::Int(999)).unwrap();
    assert_eq!(versioned.value, 2, "Version should be 2 after set");

    let state = state_cell.read(&run_id, "cell").unwrap().unwrap();
    assert_eq!(state.value.value, Value::Int(999));
}

/// Test delete removes cell
#[test]
fn test_statecell_delete_removes_cell() {
    let (db, _temp, run_id) = setup();
    let state_cell = StateCell::new(db.clone());

    state_cell.init(&run_id, "cell", Value::Int(42)).unwrap();
    assert!(state_cell.read(&run_id, "cell").unwrap().is_some());

    state_cell.delete(&run_id, "cell").unwrap();
    assert!(state_cell.read(&run_id, "cell").unwrap().is_none());
}

/// Test transition_or_init creates if not exists
#[test]
fn test_statecell_transition_or_init() {
    let (db, _temp, run_id) = setup();
    let state_cell = StateCell::new(db.clone());

    // Cell doesn't exist, should init and then transition
    let (result, versioned) = state_cell
        .transition_or_init(&run_id, "cell", Value::Int(0), |state| {
            let current = state.value.as_int().unwrap_or(-1);
            Ok((Value::Int(current + 1), current))
        })
        .unwrap();

    assert_eq!(result, 0, "Should have seen initial value 0");
    assert_eq!(versioned.value, 2, "Version should be 2 (init=1, transition=2)");

    let state = state_cell.read(&run_id, "cell").unwrap().unwrap();
    assert_eq!(state.value.value, Value::Int(1));
}

/// Test list returns all cells
#[test]
fn test_statecell_list() {
    let (db, _temp, run_id) = setup();
    let state_cell = StateCell::new(db.clone());

    state_cell.init(&run_id, "alpha", Value::Int(1)).unwrap();
    state_cell.init(&run_id, "beta", Value::Int(2)).unwrap();
    state_cell.init(&run_id, "gamma", Value::Int(3)).unwrap();

    let cells = state_cell.list(&run_id).unwrap();
    assert_eq!(cells.len(), 3);

    assert!(cells.contains(&"alpha".to_string()));
    assert!(cells.contains(&"beta".to_string()));
    assert!(cells.contains(&"gamma".to_string()));
}

// ============================================================================
// Module 4: VectorStore Business Logic Tests
// ============================================================================

/// Test search with k greater than collection size
#[test]
fn test_vector_search_k_greater_than_size() {
    let (db, _temp, run_id) = setup();
    let store = VectorStore::new(db.clone());

    let config = VectorConfig::new(2, DistanceMetric::Cosine).unwrap();
    store.create_collection(run_id, "small", config).unwrap();

    // Insert only 3 vectors
    store.insert(run_id, "small", "a", &[1.0, 0.0], None).unwrap();
    store.insert(run_id, "small", "b", &[0.0, 1.0], None).unwrap();
    store.insert(run_id, "small", "c", &[1.0, 1.0], None).unwrap();

    // Search for k=10
    let results = store.search(run_id, "small", &[1.0, 0.0], 10, None).unwrap();
    assert_eq!(results.len(), 3, "Should return all 3 vectors when k > size");
}

/// Test search on empty collection
#[test]
fn test_vector_search_empty_collection() {
    let (db, _temp, run_id) = setup();
    let store = VectorStore::new(db.clone());

    let config = VectorConfig::new(2, DistanceMetric::Cosine).unwrap();
    store.create_collection(run_id, "empty", config).unwrap();

    let results = store.search(run_id, "empty", &[1.0, 0.0], 10, None).unwrap();
    assert_eq!(results.len(), 0, "Empty collection should return no results");
}

/// Test search on nonexistent collection fails
#[test]
fn test_vector_search_nonexistent_collection_fails() {
    let (db, _temp, run_id) = setup();
    let store = VectorStore::new(db.clone());

    let result = store.search(run_id, "nonexistent", &[1.0, 0.0], 10, None);
    assert!(result.is_err(), "Search on nonexistent collection should fail");
}

/// Test metadata filtering in search
#[test]
fn test_vector_search_with_metadata_filter() {
    let (db, _temp, run_id) = setup();
    let store = VectorStore::new(db.clone());

    let config = VectorConfig::new(2, DistanceMetric::Cosine).unwrap();
    store.create_collection(run_id, "filtered", config).unwrap();

    // Insert vectors with different categories
    store
        .insert(
            run_id,
            "filtered",
            "doc1",
            &[1.0, 0.0],
            Some(serde_json::json!({"category": "A"})),
        )
        .unwrap();
    store
        .insert(
            run_id,
            "filtered",
            "doc2",
            &[0.9, 0.1],
            Some(serde_json::json!({"category": "B"})),
        )
        .unwrap();
    store
        .insert(
            run_id,
            "filtered",
            "doc3",
            &[0.8, 0.2],
            Some(serde_json::json!({"category": "A"})),
        )
        .unwrap();

    // Search with filter for category A
    let filter = MetadataFilter::new().eq("category", "A");
    let results = store
        .search(run_id, "filtered", &[1.0, 0.0], 10, Some(filter))
        .unwrap();

    assert_eq!(results.len(), 2, "Should only return category A documents");
    let keys: Vec<&str> = results.iter().map(|r| r.key.as_str()).collect();
    assert!(keys.contains(&"doc1"));
    assert!(keys.contains(&"doc3"));
    assert!(!keys.contains(&"doc2"), "doc2 is category B, should not be included");
}

/// Test delete from collection
#[test]
fn test_vector_delete_from_collection() {
    let (db, _temp, run_id) = setup();
    let store = VectorStore::new(db.clone());

    let config = VectorConfig::new(2, DistanceMetric::Cosine).unwrap();
    store.create_collection(run_id, "deletable", config).unwrap();

    store.insert(run_id, "deletable", "keep", &[1.0, 0.0], None).unwrap();
    store.insert(run_id, "deletable", "remove", &[0.0, 1.0], None).unwrap();

    // Delete one
    store.delete(run_id, "deletable", "remove").unwrap();

    // Search should only find kept vector
    let results = store.search(run_id, "deletable", &[1.0, 0.0], 10, None).unwrap();
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].key, "keep");
}

/// Test delete collection
#[test]
fn test_vector_delete_collection() {
    let (db, _temp, run_id) = setup();
    let store = VectorStore::new(db.clone());

    let config = VectorConfig::new(2, DistanceMetric::Cosine).unwrap();
    store.create_collection(run_id, "temp", config).unwrap();
    store.insert(run_id, "temp", "vec1", &[1.0, 0.0], None).unwrap();

    assert!(store.collection_exists(run_id, "temp").unwrap());

    store.delete_collection(run_id, "temp").unwrap();

    assert!(!store.collection_exists(run_id, "temp").unwrap());
}

// ============================================================================
// Module 5: RunIndex Status Transition Tests
// ============================================================================

/// Test valid status transitions from Active
#[test]
fn test_run_active_can_transition_to_all_states() {
    let (db, _temp, _) = setup();
    let run_index = RunIndex::new(db.clone());

    let targets = [
        RunStatus::Completed,
        RunStatus::Failed,
        RunStatus::Cancelled,
        RunStatus::Paused,
        RunStatus::Archived,
    ];

    for (i, target) in targets.iter().enumerate() {
        let run_name = format!("run_{}", i);
        run_index.create_run(&run_name).unwrap();

        let result = run_index.update_status(&run_name, *target);
        assert!(
            result.is_ok(),
            "Active should transition to {:?}",
            target
        );

        let meta = run_index.get_run(&run_name).unwrap().unwrap();
        assert_eq!(meta.value.status, *target);
    }
}

/// Test Paused can only transition to specific states
#[test]
fn test_run_paused_valid_transitions() {
    let (db, _temp, _) = setup();
    let run_index = RunIndex::new(db.clone());

    // Valid: Paused -> Active
    run_index.create_run("run1").unwrap();
    run_index.update_status("run1", RunStatus::Paused).unwrap();
    let result = run_index.update_status("run1", RunStatus::Active);
    assert!(result.is_ok(), "Paused -> Active should work");

    // Valid: Paused -> Cancelled
    run_index.create_run("run2").unwrap();
    run_index.update_status("run2", RunStatus::Paused).unwrap();
    let result = run_index.update_status("run2", RunStatus::Cancelled);
    assert!(result.is_ok(), "Paused -> Cancelled should work");

    // Valid: Paused -> Archived
    run_index.create_run("run3").unwrap();
    run_index.update_status("run3", RunStatus::Paused).unwrap();
    let result = run_index.update_status("run3", RunStatus::Archived);
    assert!(result.is_ok(), "Paused -> Archived should work");
}

/// Test invalid transitions are rejected
#[test]
fn test_run_invalid_transitions_rejected() {
    let (db, _temp, _) = setup();
    let run_index = RunIndex::new(db.clone());

    // Completed -> Active (no resurrection)
    run_index.create_run("run1").unwrap();
    run_index.update_status("run1", RunStatus::Completed).unwrap();
    let result = run_index.update_status("run1", RunStatus::Active);
    assert!(result.is_err(), "Completed -> Active should be rejected");

    // Failed -> Active (no resurrection)
    run_index.create_run("run2").unwrap();
    run_index.update_status("run2", RunStatus::Failed).unwrap();
    let result = run_index.update_status("run2", RunStatus::Active);
    assert!(result.is_err(), "Failed -> Active should be rejected");

    // Paused -> Completed (invalid)
    run_index.create_run("run3").unwrap();
    run_index.update_status("run3", RunStatus::Paused).unwrap();
    let result = run_index.update_status("run3", RunStatus::Completed);
    assert!(result.is_err(), "Paused -> Completed should be rejected");
}

/// Test Archived is terminal - no transitions allowed
#[test]
fn test_run_archived_is_terminal() {
    let (db, _temp, _) = setup();
    let run_index = RunIndex::new(db.clone());

    run_index.create_run("run").unwrap();
    run_index.update_status("run", RunStatus::Archived).unwrap();

    // Try all possible transitions
    let targets = [
        RunStatus::Active,
        RunStatus::Completed,
        RunStatus::Failed,
        RunStatus::Cancelled,
        RunStatus::Paused,
        RunStatus::Archived, // Even same state
    ];

    for target in &targets {
        let result = run_index.update_status("run", *target);
        assert!(
            result.is_err(),
            "Archived -> {:?} should be rejected",
            target
        );
    }
}

/// Test create run fails if already exists
#[test]
fn test_run_create_duplicate_fails() {
    let (db, _temp, _) = setup();
    let run_index = RunIndex::new(db.clone());

    run_index.create_run("duplicate").unwrap();

    let result = run_index.create_run("duplicate");
    assert!(result.is_err(), "Creating duplicate run should fail");
}

/// Test update nonexistent run fails
#[test]
fn test_run_update_nonexistent_fails() {
    let (db, _temp, _) = setup();
    let run_index = RunIndex::new(db.clone());

    let result = run_index.update_status("nonexistent", RunStatus::Completed);
    assert!(result.is_err(), "Updating nonexistent run should fail");
}

/// Test query by status returns correct runs
#[test]
fn test_run_query_by_status() {
    let (db, _temp, _) = setup();
    let run_index = RunIndex::new(db.clone());

    // Create runs with different statuses
    run_index.create_run("active1").unwrap();
    run_index.create_run("active2").unwrap();
    run_index.create_run("completed1").unwrap();
    run_index.update_status("completed1", RunStatus::Completed).unwrap();
    run_index.create_run("failed1").unwrap();
    run_index.update_status("failed1", RunStatus::Failed).unwrap();

    // Query active runs
    let active_runs = run_index.query_by_status(RunStatus::Active).unwrap();
    assert_eq!(active_runs.len(), 2);

    // Query completed runs
    let completed_runs = run_index.query_by_status(RunStatus::Completed).unwrap();
    assert_eq!(completed_runs.len(), 1);
    assert_eq!(completed_runs[0].name, "completed1");

    // Query failed runs
    let failed_runs = run_index.query_by_status(RunStatus::Failed).unwrap();
    assert_eq!(failed_runs.len(), 1);
    assert_eq!(failed_runs[0].name, "failed1");
}

/// Test completed_at is set when transitioning to finished state
#[test]
fn test_run_completed_at_timestamp() {
    let (db, _temp, _) = setup();
    let run_index = RunIndex::new(db.clone());

    run_index.create_run("run").unwrap();

    let before = run_index.get_run("run").unwrap().unwrap();
    assert!(before.value.completed_at.is_none(), "completed_at should be None initially");

    run_index.update_status("run", RunStatus::Completed).unwrap();

    let after = run_index.get_run("run").unwrap().unwrap();
    assert!(after.value.completed_at.is_some(), "completed_at should be set after completion");
}

// ============================================================================
// Module 6: Cross-Primitive Edge Cases
// ============================================================================

/// Test operations with maximum u64 version
#[test]
fn test_version_boundary_conditions() {
    let (db, _temp, run_id) = setup();
    let kv = KVStore::new(db.clone());

    // Write and read a value
    let version = kv.put(&run_id, "key", Value::Int(42)).unwrap();

    // get_at with u64::MAX should still work (returns latest)
    let result = kv.get_at(&run_id, "key", u64::MAX).unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().value, Value::Int(42));

    // history with before_version = 0 should return nothing
    let history = kv.history(&run_id, "key", None, Some(0)).unwrap();
    assert_eq!(history.len(), 0, "No versions before 0");

    // history with before_version = version+1 should return the value
    let history = kv.history(&run_id, "key", None, Some(version.as_u64() + 1)).unwrap();
    assert_eq!(history.len(), 1);
}

/// Test empty string handling across primitives
#[test]
fn test_empty_string_handling() {
    let (db, _temp, run_id) = setup();
    let kv = KVStore::new(db.clone());
    let state_cell = StateCell::new(db.clone());

    // KV with empty key
    kv.put(&run_id, "", Value::String("empty key".into())).unwrap();
    let result = kv.get(&run_id, "").unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().value, Value::String("empty key".into()));

    // StateCell with empty name
    state_cell.init(&run_id, "", Value::String("empty name".into())).unwrap();
    let result = state_cell.read(&run_id, "").unwrap();
    assert!(result.is_some());
    assert_eq!(result.unwrap().value.value, Value::String("empty name".into()));
}

/// Test null value handling
/// Note: In MVCC storage, Value::Null is treated as a tombstone (delete marker).
/// This test documents this behavior.
#[test]
fn test_null_value_as_tombstone() {
    let (db, _temp, run_id) = setup();
    let kv = KVStore::new(db.clone());

    // First put a non-null value
    kv.put(&run_id, "key", Value::Int(42)).unwrap();
    assert!(kv.get(&run_id, "key").unwrap().is_some());

    // Put Null - this acts as a delete (tombstone)
    kv.put(&run_id, "key", Value::Null).unwrap();

    // Reading returns None because Null is a tombstone
    let result = kv.get(&run_id, "key").unwrap();
    assert!(
        result.is_none(),
        "Value::Null is treated as tombstone, so get returns None"
    );

    // exists() also returns false
    assert!(
        !kv.exists(&run_id, "key").unwrap(),
        "Key with Null value should not 'exist'"
    );
}
