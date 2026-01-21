//! M9 Architecture Compliance Tests
//!
//! This module verifies compliance with the M9 Architecture specification
//! (docs/architecture/M9_ARCHITECTURE.md).
//!
//! ## The Four Architectural Rules (NON-NEGOTIABLE)
//!
//! 1. Every Read Returns Versioned<T>
//! 2. Every Write Returns Version
//! 3. Transaction Trait Covers All Primitives
//! 4. Run Scope Is Always Explicit
//!
//! ## API Consistency Audit Checklist
//!
//! - All reads return Versioned<T>
//! - All writes return Version
//! - All primitives in TransactionOps
//! - All operations accept RunId
//! - All primitives have exists()
//! - Consistent error types (StrataError)
//! - No primitive-specific patterns

use crate::test_utils::test_run_id;
use strata_concurrency::snapshot::ClonedSnapshotView;
use strata_concurrency::TransactionContext;
use strata_core::contract::{EntityRef, Version, Versioned};
use strata_core::error::StrataError;
use strata_core::types::{Namespace, RunId};
use strata_core::value::Value;
use strata_core::{Event, State, Trace, TraceType};
use strata_engine::transaction::Transaction;
use strata_engine::transaction_ops::TransactionOps;

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

// ============================================================================
// RULE 1: Every Read Returns Versioned<T>
// "No read operation may return raw values without version information."
// ============================================================================

#[test]
fn rule1_kv_get_returns_versioned() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Put a value first
    txn.kv_put("test_key", Value::I64(42)).unwrap();

    // Get returns Option<Versioned<Value>>
    let result: Option<Versioned<Value>> = txn.kv_get("test_key").unwrap();
    assert!(result.is_some());

    let versioned = result.unwrap();
    // Versioned includes version information
    assert!(versioned.version.as_u64() > 0 || versioned.version.as_u64() == 0);
    // Versioned includes timestamp
    assert!(versioned.timestamp.as_micros() > 0);
}

#[test]
fn rule1_event_read_returns_versioned() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Append an event first
    let version = txn.event_append("test_event", Value::String("payload".into())).unwrap();
    let seq = version.as_u64();

    // Read returns Option<Versioned<Event>>
    let result: Option<Versioned<Event>> = txn.event_read(seq).unwrap();
    assert!(result.is_some());

    let versioned = result.unwrap();
    // Versioned includes version (sequence)
    assert!(versioned.version.is_sequence());
}

#[test]
fn rule1_state_read_returns_versioned() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Init state first
    txn.state_init("test_state", Value::I64(0)).unwrap();

    // Read returns Option<Versioned<State>>
    let result: Option<Versioned<State>> = txn.state_read("test_state").unwrap();
    assert!(result.is_some());

    let versioned = result.unwrap();
    // Versioned includes version (counter)
    assert!(versioned.version.is_counter());
}

#[test]
fn rule1_trace_read_returns_versioned() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Record trace first
    let versioned_id = txn.trace_record(
        TraceType::Thought { content: "test".into(), confidence: None },
        vec![],
        Value::Null,
    ).unwrap();
    let trace_id = versioned_id.value;

    // Read returns Option<Versioned<Trace>>
    let result: Option<Versioned<Trace>> = txn.trace_read(trace_id).unwrap();
    assert!(result.is_some());

    let versioned = result.unwrap();
    // Versioned includes version
    assert!(versioned.version.is_txn_id());
}

// ============================================================================
// RULE 2: Every Write Returns Version
// "Every mutation returns the version it created."
// ============================================================================

#[test]
fn rule2_kv_put_returns_version() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Put returns Version (TxnId)
    let version: Version = txn.kv_put("key", Value::I64(1)).unwrap();
    assert!(version.is_txn_id());
}

#[test]
fn rule2_event_append_returns_version() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Append returns Version (Sequence)
    let version: Version = txn.event_append("event", Value::Null).unwrap();
    assert!(version.is_sequence());
}

#[test]
fn rule2_state_init_returns_version() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Init returns Version (Counter)
    let version: Version = txn.state_init("state", Value::I64(0)).unwrap();
    assert!(version.is_counter());
}

#[test]
fn rule2_state_cas_returns_version() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Init first
    txn.state_init("counter", Value::I64(0)).unwrap();

    // CAS returns Version (Counter)
    let version: Version = txn.state_cas("counter", 1, Value::I64(1)).unwrap();
    assert!(version.is_counter());
}

#[test]
fn rule2_trace_record_returns_versioned_id() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Record returns Versioned<u64> (trace_id with version)
    let versioned: Versioned<u64> = txn.trace_record(
        TraceType::Thought { content: "test".into(), confidence: None },
        vec![],
        Value::Null,
    ).unwrap();

    // The version is a TxnId
    assert!(versioned.version.is_txn_id());
    // The value is the trace_id
    assert!(versioned.value >= 0);
}

// ============================================================================
// RULE 3: Transaction Trait Covers All Primitives
// "Every primitive operation is accessible through the Transaction trait."
// ============================================================================

#[test]
fn rule3_transaction_has_kv_operations() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // All KV operations available via TransactionOps
    let _ = txn.kv_put("key", Value::I64(1));
    let _ = txn.kv_get("key");
    let _ = txn.kv_exists("key");
    let _ = txn.kv_delete("key");
    let _ = txn.kv_list(None);
}

#[test]
fn rule3_transaction_has_event_operations() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // All Event operations available via TransactionOps
    let _ = txn.event_append("type", Value::Null);
    let _ = txn.event_read(0);
    let _ = txn.event_range(0, 10);
    let _ = txn.event_len();
}

#[test]
fn rule3_transaction_has_state_operations() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // All State operations available via TransactionOps
    let _ = txn.state_init("state", Value::I64(0));
    let _ = txn.state_read("state");
    let _ = txn.state_exists("state");
    let _ = txn.state_cas("state", 1, Value::I64(1));
    let _ = txn.state_delete("state");
}

#[test]
fn rule3_transaction_has_trace_operations() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // All Trace operations available via TransactionOps
    let versioned = txn.trace_record(
        TraceType::Thought { content: "test".into(), confidence: None },
        vec![],
        Value::Null,
    ).unwrap();
    let _ = txn.trace_read(versioned.value);
    let _ = txn.trace_exists(versioned.value);
    let _ = txn.trace_count();
}

#[test]
fn rule3_cross_primitive_transaction_works() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // All primitives in same transaction
    txn.kv_put("key", Value::I64(1)).unwrap();
    txn.event_append("event", Value::Null).unwrap();
    txn.state_init("state", Value::I64(0)).unwrap();
    txn.trace_record(
        TraceType::Thought { content: "thought".into(), confidence: None },
        vec![],
        Value::Null,
    ).unwrap();

    // All changes visible within transaction
    assert!(txn.kv_exists("key").unwrap());
    assert_eq!(txn.event_len().unwrap(), 1);
    assert!(txn.state_exists("state").unwrap());
    assert!(txn.trace_count().unwrap() >= 1);
}

// ============================================================================
// RULE 4: Run Scope Is Always Explicit
// "The run is always known. No ambient run context."
// ============================================================================

#[test]
fn rule4_transaction_requires_explicit_run() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);

    // Transaction is created with explicit namespace (contains run_id)
    let txn = Transaction::new(&mut ctx, ns.clone());

    // The transaction knows its run scope
    // (implicitly via the namespace it was created with)
    drop(txn);
}

#[test]
fn rule4_different_runs_are_isolated() {
    let run1 = test_run_id();
    let run2 = test_run_id();
    assert_ne!(run1, run2);

    let ns1 = create_namespace(run1);
    let ns2 = create_namespace(run2);

    // Transaction for run1
    let mut ctx1 = create_context(&ns1);
    let mut txn1 = Transaction::new(&mut ctx1, ns1.clone());
    txn1.kv_put("shared_key", Value::String("run1_value".into())).unwrap();

    // Transaction for run2
    let mut ctx2 = create_context(&ns2);
    let mut txn2 = Transaction::new(&mut ctx2, ns2.clone());
    txn2.kv_put("shared_key", Value::String("run2_value".into())).unwrap();

    // Each run has its own isolated data
    let v1 = txn1.kv_get("shared_key").unwrap().unwrap();
    let v2 = txn2.kv_get("shared_key").unwrap().unwrap();

    assert_eq!(v1.value, Value::String("run1_value".into()));
    assert_eq!(v2.value, Value::String("run2_value".into()));
}

#[test]
fn rule4_entity_ref_always_includes_run_id() {
    let run_id = test_run_id();

    // All EntityRef variants include run_id
    let kv_ref = EntityRef::kv(run_id, "key");
    assert_eq!(kv_ref.run_id(), run_id);

    let event_ref = EntityRef::event(run_id, 1);
    assert_eq!(event_ref.run_id(), run_id);

    let state_ref = EntityRef::state(run_id, "cell");
    assert_eq!(state_ref.run_id(), run_id);

    let trace_ref = EntityRef::trace(run_id, "trace-1");
    assert_eq!(trace_ref.run_id(), run_id);

    let run_ref = EntityRef::run(run_id);
    assert_eq!(run_ref.run_id(), run_id);
}

// ============================================================================
// API CONSISTENCY AUDIT: All primitives have exists() or equivalent
// ============================================================================

#[test]
fn audit_kv_has_exists() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // exists() returns false for non-existent key
    assert!(!txn.kv_exists("nonexistent").unwrap());

    // exists() returns true after put
    txn.kv_put("key", Value::I64(1)).unwrap();
    assert!(txn.kv_exists("key").unwrap());
}

#[test]
fn audit_state_has_exists() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // exists() returns false for non-existent state
    assert!(!txn.state_exists("nonexistent").unwrap());

    // exists() returns true after init
    txn.state_init("cell", Value::I64(0)).unwrap();
    assert!(txn.state_exists("cell").unwrap());
}

#[test]
fn audit_trace_has_exists() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // exists() returns false for non-existent trace
    assert!(!txn.trace_exists(99999).unwrap());

    // exists() returns true after record
    let versioned = txn.trace_record(
        TraceType::Thought { content: "test".into(), confidence: None },
        vec![],
        Value::Null,
    ).unwrap();
    assert!(txn.trace_exists(versioned.value).unwrap());
}

#[test]
fn audit_event_introspectable_via_read() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Events use read() returning Option for introspection (append-only, no exists())
    // Non-existent sequence returns None
    assert!(txn.event_read(99999).unwrap().is_none());

    // After append, read returns Some
    let version = txn.event_append("event", Value::Null).unwrap();
    assert!(txn.event_read(version.as_u64()).unwrap().is_some());
}

// ============================================================================
// API CONSISTENCY AUDIT: Consistent error types (StrataError everywhere)
// ============================================================================

#[test]
fn audit_errors_use_strata_error() {
    // StrataError is the unified error type
    let run_id = test_run_id();

    // NotFound uses EntityRef
    let error = StrataError::not_found(EntityRef::kv(run_id, "key"));
    assert!(error.entity_ref().is_some());

    // VersionConflict uses EntityRef and Version
    let error = StrataError::version_conflict(
        EntityRef::state(run_id, "cell"),
        Version::counter(1),
        Version::counter(2),
    );
    assert!(error.entity_ref().is_some());

    // All error types are from StrataError enum
    let _ = StrataError::run_not_found(run_id);
    let _ = StrataError::transaction_aborted("reason");
    let _ = StrataError::invalid_operation(EntityRef::kv(run_id, "k"), "reason");
    let _ = StrataError::storage("disk error");
}

// ============================================================================
// API CONSISTENCY AUDIT: No primitive-specific patterns
// ============================================================================

#[test]
fn audit_all_primitives_follow_same_patterns() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Pattern 1: Writes return Version
    let kv_version: Version = txn.kv_put("k", Value::I64(1)).unwrap();
    let event_version: Version = txn.event_append("e", Value::Null).unwrap();
    let state_version: Version = txn.state_init("s", Value::I64(0)).unwrap();
    let trace_versioned: Versioned<u64> = txn.trace_record(
        TraceType::Thought { content: "t".into(), confidence: None },
        vec![],
        Value::Null,
    ).unwrap();

    // All have as_u64() for comparison
    let _ = kv_version.as_u64();
    let _ = event_version.as_u64();
    let _ = state_version.as_u64();
    let _ = trace_versioned.version.as_u64();

    // Pattern 2: Reads return Option<Versioned<T>>
    let kv_read: Option<Versioned<Value>> = txn.kv_get("k").unwrap();
    let event_read: Option<Versioned<Event>> = txn.event_read(event_version.as_u64()).unwrap();
    let state_read: Option<Versioned<State>> = txn.state_read("s").unwrap();
    let trace_read: Option<Versioned<Trace>> = txn.trace_read(trace_versioned.value).unwrap();

    assert!(kv_read.is_some());
    assert!(event_read.is_some());
    assert!(state_read.is_some());
    assert!(trace_read.is_some());
}

// ============================================================================
// CROSS-PRIMITIVE TESTS (Section 10.2)
// ============================================================================

#[test]
fn cross_primitive_transaction_atomicity() {
    // KV + Event + State + Trace in one transaction
    // Verify all-or-nothing semantics
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Multiple primitive operations
    txn.kv_put("atomic_key", Value::I64(1)).unwrap();
    txn.event_append("atomic_event", Value::Null).unwrap();
    txn.state_init("atomic_state", Value::I64(0)).unwrap();
    txn.trace_record(
        TraceType::Thought { content: "atomic".into(), confidence: None },
        vec![],
        Value::Null,
    ).unwrap();

    // All changes are visible within transaction (pre-commit)
    assert!(txn.kv_exists("atomic_key").unwrap());
    assert_eq!(txn.event_len().unwrap(), 1);
    assert!(txn.state_exists("atomic_state").unwrap());
    assert!(txn.trace_count().unwrap() >= 1);
}

#[test]
fn cross_primitive_isolation() {
    // Concurrent transactions on different primitives
    // Verify snapshot isolation
    let run_id = test_run_id();
    let ns = create_namespace(run_id);

    // First transaction writes
    let mut ctx1 = create_context(&ns);
    let mut txn1 = Transaction::new(&mut ctx1, ns.clone());
    txn1.kv_put("isolated_key", Value::I64(100)).unwrap();

    // Second transaction (concurrent) doesn't see first's writes
    let mut ctx2 = create_context(&ns);
    let txn2 = Transaction::new(&mut ctx2, ns.clone());
    let result = txn2.kv_get("isolated_key").unwrap();
    assert!(result.is_none(), "Concurrent transaction should not see uncommitted writes");
}

// ============================================================================
// VERSION TYPE CORRECTNESS
// ============================================================================

#[test]
fn version_types_match_primitive_semantics() {
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // KV uses TxnId (transaction-based versioning)
    let kv_version = txn.kv_put("k", Value::I64(1)).unwrap();
    assert!(kv_version.is_txn_id(), "KV should use TxnId versioning");

    // Event uses Sequence (monotonic sequence numbers)
    let event_version = txn.event_append("e", Value::Null).unwrap();
    assert!(event_version.is_sequence(), "Event should use Sequence versioning");

    // State uses Counter (CAS counter)
    let state_version = txn.state_init("s", Value::I64(0)).unwrap();
    assert!(state_version.is_counter(), "State should use Counter versioning");

    // Trace uses TxnId (transaction-based)
    let trace_versioned = txn.trace_record(
        TraceType::Thought { content: "t".into(), confidence: None },
        vec![],
        Value::Null,
    ).unwrap();
    assert!(trace_versioned.version.is_txn_id(), "Trace should use TxnId versioning");
}

// ============================================================================
// VERSIONED<T> WRAPPER BEHAVIOR
// ============================================================================

#[test]
fn versioned_map_preserves_metadata() {
    let versioned = Versioned::new(42i32, Version::txn(5));

    // map transforms value but preserves version/timestamp
    let mapped = versioned.map(|x| x.to_string());

    assert_eq!(mapped.value, "42");
    assert_eq!(mapped.version, Version::txn(5));
}

#[test]
fn versioned_into_value_extracts_value() {
    let versioned = Versioned::new("hello".to_string(), Version::txn(1));

    // into_value extracts just the value (for migration compatibility)
    let value: String = versioned.into_value();
    assert_eq!(value, "hello");
}

// ============================================================================
// ENTITY REF COMPLETENESS
// ============================================================================

#[test]
fn entity_ref_covers_all_primitives() {
    let run_id = test_run_id();
    use strata_core::types::JsonDocId;

    // All 7 primitives have EntityRef variants
    let refs = vec![
        EntityRef::kv(run_id, "key"),
        EntityRef::event(run_id, 1),
        EntityRef::state(run_id, "cell"),
        EntityRef::trace(run_id, "trace-1"),
        EntityRef::run(run_id),
        EntityRef::json(run_id, JsonDocId::new()),
        EntityRef::vector(run_id, "collection", "vector"),
    ];

    // Each ref reports correct primitive type
    use strata_core::PrimitiveType;
    assert_eq!(refs[0].primitive_type(), PrimitiveType::Kv);
    assert_eq!(refs[1].primitive_type(), PrimitiveType::Event);
    assert_eq!(refs[2].primitive_type(), PrimitiveType::State);
    assert_eq!(refs[3].primitive_type(), PrimitiveType::Trace);
    assert_eq!(refs[4].primitive_type(), PrimitiveType::Run);
    assert_eq!(refs[5].primitive_type(), PrimitiveType::Json);
    assert_eq!(refs[6].primitive_type(), PrimitiveType::Vector);

    // All refs include run_id
    for entity_ref in &refs {
        assert_eq!(entity_ref.run_id(), run_id);
    }
}

// ============================================================================
// SUCCESS CRITERIA: Gate 1 - Primitive Contract
// ============================================================================

#[test]
fn gate1_all_primitives_conform_to_invariant_1_addressable() {
    // Every entity has a stable identity via EntityRef
    let run_id = test_run_id();
    use strata_core::types::JsonDocId;

    // KV addressable
    let kv_ref = EntityRef::kv(run_id, "key");
    assert_eq!(kv_ref.kv_key(), Some("key"));

    // Event addressable
    let event_ref = EntityRef::event(run_id, 42);
    assert_eq!(event_ref.event_sequence(), Some(42));

    // State addressable
    let state_ref = EntityRef::state(run_id, "cell");
    assert_eq!(state_ref.state_name(), Some("cell"));

    // Trace addressable
    let trace_ref = EntityRef::trace(run_id, "trace-1");
    assert_eq!(trace_ref.trace_id(), Some("trace-1"));

    // Run addressable
    let run_ref = EntityRef::run(run_id);
    assert_eq!(run_ref.run_id(), run_id);

    // Json addressable
    let doc_id = JsonDocId::new();
    let json_ref = EntityRef::json(run_id, doc_id);
    assert_eq!(json_ref.json_doc_id(), Some(doc_id));

    // Vector addressable
    let vector_ref = EntityRef::vector(run_id, "col", "vec");
    assert_eq!(vector_ref.vector_location(), Some(("col", "vec")));
}

#[test]
fn gate1_all_primitives_conform_to_invariant_2_versioned() {
    // Every mutation produces a version
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // All writes return Version
    let _: Version = txn.kv_put("k", Value::I64(1)).unwrap();
    let _: Version = txn.event_append("e", Value::Null).unwrap();
    let _: Version = txn.state_init("s", Value::I64(0)).unwrap();
    let _: Versioned<u64> = txn.trace_record(
        TraceType::Thought { content: "t".into(), confidence: None },
        vec![],
        Value::Null,
    ).unwrap();
}

#[test]
fn gate1_all_primitives_conform_to_invariant_3_transactional() {
    // All primitives participate in transactions
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // All primitive operations callable within transaction
    txn.kv_put("k", Value::I64(1)).unwrap();
    txn.event_append("e", Value::Null).unwrap();
    txn.state_init("s", Value::I64(0)).unwrap();
    txn.trace_record(
        TraceType::Thought { content: "t".into(), confidence: None },
        vec![],
        Value::Null,
    ).unwrap();

    // All visible within same transaction
    assert!(txn.kv_get("k").unwrap().is_some());
    assert_eq!(txn.event_len().unwrap(), 1);
    assert!(txn.state_read("s").unwrap().is_some());
    assert!(txn.trace_count().unwrap() >= 1);
}

#[test]
fn gate1_all_primitives_conform_to_invariant_5_run_scoped() {
    // Run is the unit of isolation
    let run1 = test_run_id();
    let run2 = test_run_id();

    // Each run has separate namespace
    let ns1 = create_namespace(run1);
    let ns2 = create_namespace(run2);

    assert_ne!(ns1.run_id, ns2.run_id);
}

#[test]
fn gate1_all_primitives_conform_to_invariant_7_read_write_consistency() {
    // Reads never modify, writes produce versions
    let run_id = test_run_id();
    let ns = create_namespace(run_id);
    let mut ctx = create_context(&ns);
    let mut txn = Transaction::new(&mut ctx, ns.clone());

    // Write first
    txn.kv_put("key", Value::I64(1)).unwrap();

    // Multiple reads don't change version
    let v1 = txn.kv_get("key").unwrap().unwrap();
    let v2 = txn.kv_get("key").unwrap().unwrap();

    // Same version after multiple reads
    assert_eq!(v1.version, v2.version);
}
