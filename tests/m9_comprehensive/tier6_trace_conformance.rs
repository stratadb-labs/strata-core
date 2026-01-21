//! TraceStore Conformance Tests
//!
//! This module verifies that the TraceStore primitive conforms to all 7 invariants:
//!
//! 1. Addressable - Traces have stable identity via EntityRef
//! 2. Versioned - Reads return Versioned<Trace>, writes return Version::TxnId
//! 3. Transactional - Trace operations participate in transactions
//! 4. Lifecycle - Traces support record, read (append-only)
//! 5. Run-scoped - Traces are isolated by run
//! 6. Introspectable - Traces have exists() and count()
//! 7. Read/Write - Reads don't modify, writes produce versions
//!
//! # Story #492: Invariant 1-2 Conformance Tests (Trace portion)
//! # Story #493: Invariant 3-4 Conformance Tests (Trace portion)
//! # Story #494: Invariant 5-6 Conformance Tests (Trace portion)
//! # Story #495: Invariant 7 Conformance Tests (Trace portion)

use crate::test_utils::test_run_id;
use strata_concurrency::snapshot::ClonedSnapshotView;
use strata_concurrency::TransactionContext;
use strata_core::types::{Namespace, RunId};
use strata_core::{EntityRef, PrimitiveType, TraceType, Value};
use strata_engine::transaction::Transaction;
use strata_engine::transaction_ops::TransactionOps;
use std::collections::HashMap;

/// Create a test namespace for a run
fn create_namespace(run_id: RunId) -> Namespace {
    Namespace::new(
        "test-tenant".to_string(),
        "test-app".to_string(),
        "test-agent".to_string(),
        run_id,
    )
}

/// Create a transaction context for testing
fn create_context(ns: &Namespace) -> TransactionContext {
    let snapshot = Box::new(ClonedSnapshotView::empty(100));
    TransactionContext::with_snapshot(1, ns.run_id, snapshot)
}

/// Create a simple test trace type
fn test_trace_type() -> TraceType {
    TraceType::Thought {
        content: "Test thought".to_string(),
        confidence: Some(0.8),
    }
}

// ============================================================================
// Invariant 1: Everything is Addressable
// ============================================================================

#[test]
fn trace_invariant1_has_stable_identity() {
    let run_id = test_run_id();

    // Trace can be addressed via EntityRef
    let entity_ref = EntityRef::trace(run_id, "trace-0");

    // EntityRef identifies the primitive type
    assert_eq!(entity_ref.primitive_type(), PrimitiveType::Trace);

    // EntityRef provides the run context
    assert_eq!(entity_ref.run_id(), run_id);

    // EntityRef provides the trace ID
    assert_eq!(entity_ref.trace_id(), Some("trace-0"));
}

#[test]
fn trace_invariant1_recorded_trace_has_identity() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Record a trace
    let versioned_id = txn.trace_record(test_trace_type(), vec![], Value::Null).unwrap();

    // Read back and verify it has an ID
    let trace = txn.trace_read(versioned_id.value).unwrap().unwrap();
    assert!(!trace.value.id.is_empty());
    assert!(trace.value.id.starts_with("trace-"));
}

#[test]
fn trace_invariant1_identity_can_be_stored() {
    let run_id = test_run_id();

    // EntityRef can be used as a key in collections
    let mut store: HashMap<EntityRef, String> = HashMap::new();

    let ref1 = EntityRef::trace(run_id, "trace-0");
    let ref2 = EntityRef::trace(run_id, "trace-1");

    store.insert(ref1.clone(), "trace1-data".to_string());
    store.insert(ref2.clone(), "trace2-data".to_string());

    // Can retrieve using the same identity
    assert_eq!(store.get(&ref1), Some(&"trace1-data".to_string()));
    assert_eq!(store.get(&ref2), Some(&"trace2-data".to_string()));
}

#[test]
fn trace_invariant1_identity_survives_serialization() {
    let run_id = test_run_id();
    let entity_ref = EntityRef::trace(run_id, "trace-42");

    // Serialize
    let json = serde_json::to_string(&entity_ref).expect("serialize");

    // Deserialize
    let restored: EntityRef = serde_json::from_str(&json).expect("deserialize");

    // Identity preserved
    assert_eq!(entity_ref, restored);
}

// ============================================================================
// Invariant 2: Everything is Versioned
// ============================================================================

#[test]
fn trace_invariant2_read_returns_versioned() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Record a trace
    let versioned_id = txn.trace_record(test_trace_type(), vec!["test".to_string()], Value::Null).unwrap();

    // Read returns Versioned<Trace>
    let result = txn.trace_read(versioned_id.value).unwrap();
    assert!(result.is_some());

    let versioned = result.unwrap();
    // Has version (TxnId type for traces)
    assert!(versioned.version.is_txn_id());
    // Has timestamp
    assert!(versioned.timestamp.as_micros() > 0);
    // Has trace data
    assert!(!versioned.value.id.is_empty());
}

#[test]
fn trace_invariant2_record_returns_versioned_sequence() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Record returns Versioned<u64> (sequence number with version)
    let result = txn.trace_record(test_trace_type(), vec![], Value::Null).unwrap();

    // Has sequence number
    assert_eq!(result.value, 0);
    // Has version
    assert!(result.version.is_txn_id());
}

#[test]
fn trace_invariant2_sequence_monotonically_increasing() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Record multiple traces
    let t0 = txn.trace_record(test_trace_type(), vec![], Value::Null).unwrap();
    let t1 = txn.trace_record(test_trace_type(), vec![], Value::Null).unwrap();
    let t2 = txn.trace_record(test_trace_type(), vec![], Value::Null).unwrap();

    // Sequence is monotonically increasing
    assert_eq!(t0.value, 0);
    assert_eq!(t1.value, 1);
    assert_eq!(t2.value, 2);
}

// ============================================================================
// Invariant 3: Everything is Transactional
// ============================================================================

#[test]
fn trace_invariant3_operations_in_transaction() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // All trace operations happen within a transaction context
    txn.trace_record(test_trace_type(), vec![], Value::Null).unwrap();
    let _ = txn.trace_read(0).unwrap();
    let _ = txn.trace_exists(0).unwrap();
    let _ = txn.trace_count().unwrap();

    // Transaction tracks changes
    assert!(!ctx.write_set.is_empty());
}

#[test]
fn trace_invariant3_read_your_writes() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Record within transaction
    let trace_type = TraceType::ToolCall {
        tool_name: "test-tool".to_string(),
        arguments: Value::String("args".to_string()),
        result: Some(Value::I64(42)),
        duration_ms: Some(100),
    };
    let result = txn.trace_record(trace_type, vec!["important".to_string()], Value::Null).unwrap();

    // Read sees uncommitted write (read-your-writes)
    let trace = txn.trace_read(result.value).unwrap().unwrap();
    assert!(trace.value.tags.contains(&"important".to_string()));
    match &trace.value.trace_type {
        TraceType::ToolCall { tool_name, .. } => {
            assert_eq!(tool_name, "test-tool");
        }
        _ => panic!("Expected ToolCall trace type"),
    }
}

#[test]
fn trace_invariant3_pending_traces_buffered() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Record traces
    txn.trace_record(test_trace_type(), vec![], Value::Null).unwrap();
    txn.trace_record(test_trace_type(), vec![], Value::Null).unwrap();

    // Pending traces are buffered
    let pending = txn.pending_traces();
    assert_eq!(pending.len(), 2);
}

// ============================================================================
// Invariant 4: Everything has Lifecycle
// ============================================================================

#[test]
fn trace_invariant4_append_only() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Record trace
    let t0 = txn.trace_record(test_trace_type(), vec![], Value::Null).unwrap();
    assert_eq!(t0.value, 0);

    // Read
    let trace = txn.trace_read(0).unwrap().unwrap();
    assert!(!trace.value.id.is_empty());

    // Record more (append-only, no update/delete on traces)
    let t1 = txn.trace_record(test_trace_type(), vec![], Value::Null).unwrap();
    assert_eq!(t1.value, 1);

    // Count increases
    assert_eq!(txn.trace_count().unwrap(), 2);
}

#[test]
fn trace_invariant4_multiple_trace_types() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Record different trace types
    txn.trace_record(
        TraceType::Thought { content: "thinking...".to_string(), confidence: Some(0.9) },
        vec![],
        Value::Null,
    ).unwrap();

    txn.trace_record(
        TraceType::Decision {
            question: "What to do?".to_string(),
            options: vec!["A".to_string(), "B".to_string()],
            chosen: "A".to_string(),
            reasoning: Some("A is better".to_string()),
        },
        vec!["decision".to_string()],
        Value::Null,
    ).unwrap();

    txn.trace_record(
        TraceType::Query {
            query_type: "database".to_string(),
            query: "SELECT * FROM users".to_string(),
            results_count: Some(10),
        },
        vec![],
        Value::Null,
    ).unwrap();

    txn.trace_record(
        TraceType::Error {
            error_type: "ValidationError".to_string(),
            message: "Invalid input".to_string(),
            recoverable: true,
        },
        vec!["error".to_string()],
        Value::Null,
    ).unwrap();

    txn.trace_record(
        TraceType::Custom {
            name: "custom-type".to_string(),
            data: Value::String("custom data".to_string()),
        },
        vec![],
        Value::Null,
    ).unwrap();

    assert_eq!(txn.trace_count().unwrap(), 5);
}

#[test]
fn trace_invariant4_traces_have_timestamps() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Record traces with small delay
    txn.trace_record(test_trace_type(), vec![], Value::Null).unwrap();
    txn.trace_record(test_trace_type(), vec![], Value::Null).unwrap();

    let t0 = txn.trace_read(0).unwrap().unwrap();
    let t1 = txn.trace_read(1).unwrap().unwrap();

    // Both have timestamps
    assert!(t0.value.timestamp > 0);
    assert!(t1.value.timestamp > 0);

    // t1 timestamp >= t0 timestamp (same or later)
    assert!(t1.value.timestamp >= t0.value.timestamp);
}

// ============================================================================
// Invariant 5: Everything is Run-Scoped
// ============================================================================

#[test]
fn trace_invariant5_isolated_by_run() {
    let run_id1 = test_run_id();
    let run_id2 = test_run_id();

    // Create namespaces for two different runs
    let ns1 = create_namespace(run_id1);
    let ns2 = create_namespace(run_id2);

    let mut ctx1 = create_context(&ns1);
    let mut ctx2 = create_context(&ns2);

    let mut txn1 = Transaction::new(&mut ctx1, ns1.clone());
    let mut txn2 = Transaction::new(&mut ctx2, ns2.clone());

    // Record traces in run1
    txn1.trace_record(test_trace_type(), vec!["run1".to_string()], Value::Null).unwrap();
    txn1.trace_record(test_trace_type(), vec!["run1".to_string()], Value::Null).unwrap();

    // Record traces in run2
    txn2.trace_record(test_trace_type(), vec!["run2".to_string()], Value::Null).unwrap();

    // Each run has isolated count
    assert_eq!(txn1.trace_count().unwrap(), 2);
    assert_eq!(txn2.trace_count().unwrap(), 1);

    // Each run sees its own traces
    let t1 = txn1.trace_read(0).unwrap().unwrap();
    let t2 = txn2.trace_read(0).unwrap().unwrap();

    assert!(t1.value.tags.contains(&"run1".to_string()));
    assert!(t2.value.tags.contains(&"run2".to_string()));
}

#[test]
fn trace_invariant5_entity_ref_includes_run() {
    let run_id1 = test_run_id();
    let run_id2 = test_run_id();

    let ref1 = EntityRef::trace(run_id1, "trace-0");
    let ref2 = EntityRef::trace(run_id2, "trace-0");

    // Same trace ID, different runs = different entities
    assert_ne!(ref1, ref2);
    assert_eq!(ref1.run_id(), run_id1);
    assert_eq!(ref2.run_id(), run_id2);
}

// ============================================================================
// Invariant 6: Everything is Introspectable
// ============================================================================

#[test]
fn trace_invariant6_exists_check() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Before recording
    assert!(!txn.trace_exists(0).unwrap());

    // After recording
    txn.trace_record(test_trace_type(), vec![], Value::Null).unwrap();
    assert!(txn.trace_exists(0).unwrap());
    assert!(!txn.trace_exists(1).unwrap());
}

#[test]
fn trace_invariant6_count_introspection() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Initially empty
    assert_eq!(txn.trace_count().unwrap(), 0);

    // After recording
    txn.trace_record(test_trace_type(), vec![], Value::Null).unwrap();
    assert_eq!(txn.trace_count().unwrap(), 1);

    txn.trace_record(test_trace_type(), vec![], Value::Null).unwrap();
    txn.trace_record(test_trace_type(), vec![], Value::Null).unwrap();
    assert_eq!(txn.trace_count().unwrap(), 3);
}

#[test]
fn trace_invariant6_read_non_existent_returns_none() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let txn = Transaction::new(&mut ctx, ns.clone());

    // Reading non-existent trace returns None (not error)
    let result = txn.trace_read(999).unwrap();
    assert!(result.is_none());
}

// ============================================================================
// Invariant 7: Read/Write Semantics
// ============================================================================

#[test]
fn trace_invariant7_read_does_not_modify() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Record
    txn.trace_record(test_trace_type(), vec![], Value::Null).unwrap();

    let count_before = txn.trace_count().unwrap();

    // Multiple reads
    let _ = txn.trace_read(0).unwrap();
    let _ = txn.trace_read(0).unwrap();
    let _ = txn.trace_read(0).unwrap();

    let count_after = txn.trace_count().unwrap();

    // Count unchanged
    assert_eq!(count_before, count_after);
}

#[test]
fn trace_invariant7_record_produces_version() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Each record produces a versioned result
    let v1 = txn.trace_record(test_trace_type(), vec![], Value::Null).unwrap();
    assert!(v1.version.as_u64() > 0);

    let v2 = txn.trace_record(test_trace_type(), vec![], Value::Null).unwrap();
    // Same transaction ID for both
    assert_eq!(v1.version, v2.version);
}

#[test]
fn trace_invariant7_version_is_txn_id_type() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Trace uses TxnId version (per spec: TxnId for Trace)
    let versioned = txn.trace_record(test_trace_type(), vec![], Value::Null).unwrap();
    assert!(versioned.version.is_txn_id(), "Trace should use TxnId version type");
}

#[test]
fn trace_invariant7_exists_does_not_modify() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Record
    txn.trace_record(test_trace_type(), vec![], Value::Null).unwrap();

    let count_before = txn.trace_count().unwrap();

    // Multiple exists calls
    let _ = txn.trace_exists(0).unwrap();
    let _ = txn.trace_exists(0).unwrap();
    let _ = txn.trace_exists(0).unwrap();

    let count_after = txn.trace_count().unwrap();

    // Count unchanged
    assert_eq!(count_before, count_after);
}

#[test]
fn trace_invariant7_count_does_not_modify() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Record
    txn.trace_record(test_trace_type(), vec![], Value::Null).unwrap();

    // Multiple count calls
    let c1 = txn.trace_count().unwrap();
    let c2 = txn.trace_count().unwrap();
    let c3 = txn.trace_count().unwrap();

    // All same (count is read-only)
    assert_eq!(c1, c2);
    assert_eq!(c2, c3);
}
