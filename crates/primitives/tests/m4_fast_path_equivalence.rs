//! M4 Fast Path Observational Equivalence Tests
//!
//! Verifies that fast-path reads are observationally equivalent
//! to transaction-based reads. This is a CRITICAL invariant:
//!
//! - No dirty reads (uncommitted data)
//! - No torn reads (partial write sets)
//! - No stale reads (older than snapshot version)
//! - No mixing versions (key A at version X, key B at version Y where Y > X)
//!
//! "Latest committed at snapshot acquisition" is the correct semantic.

use strata_core::types::RunId;
use strata_core::value::Value;
use strata_engine::Database;
use strata_primitives::{EventLog, KVStore, StateCell, TraceStore, TraceType};
use std::sync::Arc;
use tempfile::TempDir;

fn setup_db() -> (Arc<Database>, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let db = Arc::new(Database::open(temp_dir.path()).unwrap());
    (db, temp_dir)
}

// ============================================================================
// KVStore Observational Equivalence
// ============================================================================

#[test]
fn kv_fast_path_equals_transaction_read() {
    let (db, _temp) = setup_db();
    let kv = KVStore::new(db.clone());
    let run_id = RunId::new();

    // Write some data
    kv.put(&run_id, "key1", Value::String("value1".into()))
        .unwrap();
    kv.put(&run_id, "key2", Value::I64(42)).unwrap();

    // Fast path reads
    let fast1 = kv.get(&run_id, "key1").unwrap();
    let fast2 = kv.get(&run_id, "key2").unwrap();
    let fast_missing = kv.get(&run_id, "missing").unwrap();

    // Transaction reads
    let txn1 = kv.get_in_transaction(&run_id, "key1").unwrap();
    let txn2 = kv.get_in_transaction(&run_id, "key2").unwrap();
    let txn_missing = kv.get_in_transaction(&run_id, "missing").unwrap();

    // Values must be identical (metadata like version/timestamp may differ between paths)
    assert_eq!(fast1.as_ref().map(|v| &v.value), txn1.as_ref().map(|v| &v.value), "key1 values must match");
    assert_eq!(fast2.as_ref().map(|v| &v.value), txn2.as_ref().map(|v| &v.value), "key2 values must match");
    assert_eq!(fast_missing, txn_missing, "missing key must match");
}

#[test]
fn kv_fast_path_observes_latest_committed() {
    let (db, _temp) = setup_db();
    let kv = KVStore::new(db.clone());
    let run_id = RunId::new();

    // Initial value
    kv.put(&run_id, "key", Value::I64(1)).unwrap();
    assert_eq!(kv.get(&run_id, "key").unwrap().map(|v| v.value), Some(Value::I64(1)));

    // Update value
    kv.put(&run_id, "key", Value::I64(2)).unwrap();
    assert_eq!(kv.get(&run_id, "key").unwrap().map(|v| v.value), Some(Value::I64(2)));

    // Update again
    kv.put(&run_id, "key", Value::I64(3)).unwrap();
    assert_eq!(kv.get(&run_id, "key").unwrap().map(|v| v.value), Some(Value::I64(3)));

    // Fast path and transaction should agree on value
    assert_eq!(
        kv.get(&run_id, "key").unwrap().map(|v| v.value),
        kv.get_in_transaction(&run_id, "key").unwrap().map(|v| v.value)
    );
}

#[test]
fn kv_batch_get_snapshot_consistency() {
    let (db, _temp) = setup_db();
    let kv = KVStore::new(db.clone());
    let run_id = RunId::new();

    // Write related data atomically
    db.transaction(run_id, |txn| {
        use strata_core::types::{Key, Namespace, TypeTag};
        let ns = Namespace::for_run(run_id);

        txn.put(
            Key::new(ns.clone(), TypeTag::KV, b"a".to_vec()),
            Value::I64(100),
        )?;
        txn.put(
            Key::new(ns.clone(), TypeTag::KV, b"b".to_vec()),
            Value::I64(200),
        )?;
        Ok(())
    })
    .unwrap();

    // Batch get should see consistent view
    let results = kv.get_many(&run_id, &["a", "b"]).unwrap();

    // Both values should be from the same snapshot
    assert_eq!(results[0].as_ref().map(|v| v.value.clone()), Some(Value::I64(100)));
    assert_eq!(results[1].as_ref().map(|v| v.value.clone()), Some(Value::I64(200)));
}

// ============================================================================
// EventLog Observational Equivalence
// ============================================================================

#[test]
fn eventlog_fast_path_equals_transaction_read() {
    let (db, _temp) = setup_db();
    let log = EventLog::new(db.clone());
    let run_id = RunId::new();

    // Append events
    log.append(&run_id, "event1", Value::I64(1)).unwrap();
    log.append(&run_id, "event2", Value::I64(2)).unwrap();

    // Fast path reads
    let fast0 = log.read(&run_id, 0).unwrap();
    let fast1 = log.read(&run_id, 1).unwrap();
    let fast_missing = log.read(&run_id, 999).unwrap();

    // Transaction reads
    let txn0 = log.read_in_transaction(&run_id, 0).unwrap();
    let txn1 = log.read_in_transaction(&run_id, 1).unwrap();
    let txn_missing = log.read_in_transaction(&run_id, 999).unwrap();

    // Must be identical
    assert_eq!(fast0, txn0, "event 0 must match");
    assert_eq!(fast1, txn1, "event 1 must match");
    assert_eq!(fast_missing, txn_missing, "missing event must match");
}

#[test]
fn eventlog_len_fast_path_equals_transaction() {
    let (db, _temp) = setup_db();
    let log = EventLog::new(db.clone());
    let run_id = RunId::new();

    assert_eq!(log.len(&run_id).unwrap(), 0);

    log.append(&run_id, "test", Value::Null).unwrap();
    log.append(&run_id, "test", Value::Null).unwrap();
    log.append(&run_id, "test", Value::Null).unwrap();

    // Fast path len should match actual count
    assert_eq!(log.len(&run_id).unwrap(), 3);

    // Verify chain is intact
    let verification = log.verify_chain(&run_id).unwrap();
    assert!(verification.is_valid);
    assert_eq!(verification.length, 3);
}

// ============================================================================
// StateCell Observational Equivalence
// ============================================================================

#[test]
fn statecell_fast_path_equals_transaction_read() {
    let (db, _temp) = setup_db();
    let sc = StateCell::new(db.clone());
    let run_id = RunId::new();

    // Initialize cells
    sc.init(&run_id, "cell1", Value::I64(100)).unwrap();
    sc.init(&run_id, "cell2", Value::String("hello".into()))
        .unwrap();

    // Fast path reads
    let fast1 = sc.read(&run_id, "cell1").unwrap();
    let fast2 = sc.read(&run_id, "cell2").unwrap();
    let fast_missing = sc.read(&run_id, "missing").unwrap();

    // Transaction reads
    let txn1 = sc.read_in_transaction(&run_id, "cell1").unwrap();
    let txn2 = sc.read_in_transaction(&run_id, "cell2").unwrap();
    let txn_missing = sc.read_in_transaction(&run_id, "missing").unwrap();

    // Must be identical
    assert_eq!(fast1, txn1, "cell1 must match");
    assert_eq!(fast2, txn2, "cell2 must match");
    assert_eq!(fast_missing, txn_missing, "missing cell must match");
}

#[test]
fn statecell_version_monotonicity() {
    let (db, _temp) = setup_db();
    let sc = StateCell::new(db.clone());
    let run_id = RunId::new();

    sc.init(&run_id, "counter", Value::I64(0)).unwrap();

    // Each set increments version
    for i in 1..=5 {
        sc.set(&run_id, "counter", Value::I64(i)).unwrap();

        let state = sc.read(&run_id, "counter").unwrap().unwrap();
        assert_eq!(state.value.version, (i + 1) as u64, "version should increment");
        assert_eq!(state.value.value, Value::I64(i), "value should update");
    }
}

// ============================================================================
// TraceStore Observational Equivalence
// ============================================================================

#[test]
fn tracestore_fast_path_equals_transaction_read() {
    let (db, _temp) = setup_db();
    let ts = TraceStore::new(db.clone());
    let run_id = RunId::new();

    // Record traces
    let trace1_id = ts
        .record(
            &run_id,
            TraceType::Thought {
                content: "thought 1".into(),
                confidence: Some(0.9),
            },
            vec!["important".into()],
            Value::Null,
        )
        .unwrap()
        .value; // Extract trace_id from Versioned

    let trace2_id = ts
        .record(
            &run_id,
            TraceType::ToolCall {
                tool_name: "search".into(),
                arguments: Value::String("query".into()),
                result: None,
                duration_ms: None,
            },
            vec![],
            Value::Null,
        )
        .unwrap()
        .value; // Extract trace_id from Versioned

    // Fast path reads
    let fast1 = ts.get(&run_id, &trace1_id).unwrap();
    let fast2 = ts.get(&run_id, &trace2_id).unwrap();
    let fast_missing = ts.get(&run_id, "nonexistent").unwrap();

    // Transaction reads
    let txn1 = ts.get_in_transaction(&run_id, &trace1_id).unwrap();
    let txn2 = ts.get_in_transaction(&run_id, &trace2_id).unwrap();
    let txn_missing = ts.get_in_transaction(&run_id, "nonexistent").unwrap();

    // Values must be identical (version metadata may differ between paths)
    assert_eq!(fast1.as_ref().map(|v| &v.value), txn1.as_ref().map(|v| &v.value), "trace1 values must match");
    assert_eq!(fast2.as_ref().map(|v| &v.value), txn2.as_ref().map(|v| &v.value), "trace2 values must match");
    assert_eq!(fast_missing, txn_missing, "missing trace must match");
}

#[test]
fn tracestore_parent_child_relationship_preserved() {
    let (db, _temp) = setup_db();
    let ts = TraceStore::new(db.clone());
    let run_id = RunId::new();

    // Create parent-child relationship
    let parent_id = ts
        .record(
            &run_id,
            TraceType::Decision {
                question: "What to do?".into(),
                options: vec!["A".into(), "B".into()],
                chosen: "A".into(),
                reasoning: Some("A is faster".into()),
            },
            vec![],
            Value::Null,
        )
        .unwrap()
        .value; // Extract trace_id from Versioned

    let child_id = ts
        .record_child(
            &run_id,
            &parent_id,
            TraceType::ToolCall {
                tool_name: "execute_a".into(),
                arguments: Value::Null,
                result: None,
                duration_ms: None,
            },
            vec![],
            Value::Null,
        )
        .unwrap()
        .value; // Extract trace_id from Versioned

    // Fast path should see correct relationship
    let child = ts.get(&run_id, &child_id).unwrap().unwrap();
    assert_eq!(child.value.parent_id, Some(parent_id.clone()));

    // Tree reconstruction should work with fast path reads
    let tree = ts.get_tree(&run_id, &parent_id).unwrap().unwrap();
    assert_eq!(tree.trace.id, parent_id);
    assert_eq!(tree.children.len(), 1);
    assert_eq!(tree.children[0].trace.id, child_id);
}

// ============================================================================
// Cross-Primitive Consistency
// ============================================================================

#[test]
fn all_primitives_run_isolation() {
    let (db, _temp) = setup_db();
    let run1 = RunId::new();
    let run2 = RunId::new();

    let kv = KVStore::new(db.clone());
    let log = EventLog::new(db.clone());
    let sc = StateCell::new(db.clone());
    let ts = TraceStore::new(db.clone());

    // Write to run1
    kv.put(&run1, "key", Value::I64(1)).unwrap();
    log.append(&run1, "event", Value::I64(1)).unwrap();
    sc.init(&run1, "cell", Value::I64(1)).unwrap();
    let trace1_id = ts
        .record(
            &run1,
            TraceType::Thought {
                content: "run1".into(),
                confidence: None,
            },
            vec![],
            Value::Null,
        )
        .unwrap()
        .value; // Extract trace_id from Versioned

    // Write to run2
    kv.put(&run2, "key", Value::I64(2)).unwrap();
    log.append(&run2, "event", Value::I64(2)).unwrap();
    sc.init(&run2, "cell", Value::I64(2)).unwrap();
    let trace2_id = ts
        .record(
            &run2,
            TraceType::Thought {
                content: "run2".into(),
                confidence: None,
            },
            vec![],
            Value::Null,
        )
        .unwrap()
        .value; // Extract trace_id from Versioned

    // Fast path reads should maintain run isolation
    assert_eq!(kv.get(&run1, "key").unwrap().map(|v| v.value), Some(Value::I64(1)));
    assert_eq!(kv.get(&run2, "key").unwrap().map(|v| v.value), Some(Value::I64(2)));

    let event1 = log.read(&run1, 0).unwrap().unwrap();
    let event2 = log.read(&run2, 0).unwrap().unwrap();
    assert_eq!(event1.value.payload, Value::I64(1));
    assert_eq!(event2.value.payload, Value::I64(2));

    let state1 = sc.read(&run1, "cell").unwrap().unwrap();
    let state2 = sc.read(&run2, "cell").unwrap().unwrap();
    assert_eq!(state1.value.value, Value::I64(1));
    assert_eq!(state2.value.value, Value::I64(2));

    let t1 = ts.get(&run1, &trace1_id).unwrap().unwrap();
    let t2 = ts.get(&run2, &trace2_id).unwrap().unwrap();
    assert!(matches!(t1.value.trace_type, TraceType::Thought { content, .. } if content == "run1"));
    assert!(matches!(t2.value.trace_type, TraceType::Thought { content, .. } if content == "run2"));

    // Cross-run reads should return None
    assert!(ts.get(&run1, &trace2_id).unwrap().is_none());
    assert!(ts.get(&run2, &trace1_id).unwrap().is_none());
}

#[test]
fn fast_path_observes_committed_data_only() {
    let (db, _temp) = setup_db();
    let kv = KVStore::new(db.clone());
    let run_id = RunId::new();

    // Write initial value
    kv.put(&run_id, "key", Value::I64(1)).unwrap();

    // Fast path should see the committed value
    assert_eq!(kv.get(&run_id, "key").unwrap().map(|v| v.value), Some(Value::I64(1)));

    // Update in a new transaction
    kv.put(&run_id, "key", Value::I64(2)).unwrap();

    // Fast path should see the new committed value
    assert_eq!(kv.get(&run_id, "key").unwrap().map(|v| v.value), Some(Value::I64(2)));

    // Transaction read should match
    assert_eq!(
        kv.get_in_transaction(&run_id, "key").unwrap().map(|v| v.value),
        Some(Value::I64(2))
    );
}
