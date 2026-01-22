# Epic 64: Conformance Testing - Implementation Prompts

**Epic Goal**: Verify all 7 primitives conform to all 7 invariants

**GitHub Issue**: [#468](https://github.com/anibjoshi/in-mem/issues/468)
**Status**: Ready to begin after Epics 60, 61, 62, 63
**Dependencies**: All other M9 epics
**Phases**: 2-5 (incremental with each primitive batch)

---

## NAMING CONVENTION - CRITICAL

> **NEVER use "M9" or "Strata" in the actual codebase or comments.**
>
> - "M9" is an internal milestone tracker only - do not use it in code, comments, or user-facing text
> - All existing crates refer to the database as "in-mem" - use this name consistently
> - Do not use "Strata" anywhere in the codebase
> - This applies to: code, comments, docstrings, error messages, log messages, test names
>
> **CORRECT**: `//! Universal entity reference for any in-mem entity`
> **WRONG**: `//! Universal entity reference for any Strata entity`

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M9_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M9_ARCHITECTURE.md`
2. **Primitive Contract**: `docs/architecture/PRIMITIVE_CONTRACT.md`
3. **Epic Spec**: `docs/milestones/M9/EPIC_64_CONFORMANCE_TESTING.md`
4. **Prompt Header**: `docs/prompts/M9/M9_PROMPT_HEADER.md`

---

## Epic 64 Overview

### The Seven Invariants

Every primitive MUST conform to these invariants:

| # | Invariant | Description | Test Focus |
|---|-----------|-------------|------------|
| 1 | Addressable | Every entity has stable identity | EntityRef creation |
| 2 | Versioned | Reads return Versioned<T>, writes return Version | Return types |
| 3 | Transactional | Participates in transactions | TransactionOps |
| 4 | Lifecycle | Create/exist/evolve/destroy | CRUD operations |
| 5 | Run-scoped | Belongs to exactly one run | Isolation |
| 6 | Introspectable | Has exists() or equivalent | Existence check |
| 7 | Read/Write | Reads don't modify, writes produce versions | Semantics |

### Conformance Matrix (49 Tests)

| Primitive | I1 | I2 | I3 | I4 | I5 | I6 | I7 |
|-----------|----|----|----|----|----|----|-----|
| KVStore | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| EventLog | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| StateCell | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| TraceStore | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| JsonStore | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| VectorStore | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| RunIndex | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |

### Component Breakdown

| Story | Description | Priority |
|-------|-------------|----------|
| #492 | Invariant 1-2 Conformance Tests | CRITICAL |
| #493 | Invariant 3-4 Conformance Tests | CRITICAL |
| #494 | Invariant 5-6 Conformance Tests | CRITICAL |
| #495 | Invariant 7 Conformance Tests | CRITICAL |
| #496 | Cross-Primitive Transaction Conformance | CRITICAL |

---

## Test Structure

Create `tests/conformance/` directory:

```
tests/
  conformance/
    mod.rs
    invariant_1_addressable.rs
    invariant_2_versioned.rs
    invariant_3_transactional.rs
    invariant_4_lifecycle.rs
    invariant_5_run_scoped.rs
    invariant_6_introspectable.rs
    invariant_7_read_write.rs
    cross_primitive.rs
```

---

## Story #492: Invariant 1-2 Conformance Tests

**GitHub Issue**: [#492](https://github.com/anibjoshi/in-mem/issues/492)
**Estimated Time**: 4 hours
**Dependencies**: Epics 60, 61

### Start Story

```bash
gh issue view 492
./scripts/start-story.sh 64 492 invariant-1-2-tests
```

### Implementation

Create `tests/conformance/mod.rs`:

```rust
//! Conformance tests for the 7 invariants across all 7 primitives

mod invariant_1_addressable;
mod invariant_2_versioned;
mod invariant_3_transactional;
mod invariant_4_lifecycle;
mod invariant_5_run_scoped;
mod invariant_6_introspectable;
mod invariant_7_read_write;
mod cross_primitive;

use in_mem::{Database, RunId};

/// Create a test database
fn test_db() -> Database {
    Database::new_in_memory()
}

/// Create a unique run ID for test isolation
fn test_run() -> RunId {
    RunId::new(format!("test-{}", uuid::Uuid::new_v4()))
}
```

Create `tests/conformance/invariant_1_addressable.rs`:

```rust
//! Invariant 1: Everything is Addressable
//!
//! Every entity has a stable identity via EntityRef.

use in_mem::{EntityRef, PrimitiveType, RunId, VectorId, JsonDocId};
use super::{test_db, test_run};

// ============================================================================
// KVStore
// ============================================================================

#[test]
fn kv_has_stable_identity() {
    let db = test_db();
    let run_id = test_run();
    let run = db.run(run_id.clone());

    run.kv_put("test-key", Value::from("value")).unwrap();

    // Can create EntityRef for any KV entry
    let entity_ref = EntityRef::kv(run_id.clone(), "test-key");

    // EntityRef has correct properties
    assert_eq!(entity_ref.run_id(), &run_id);
    assert_eq!(entity_ref.primitive_type(), PrimitiveType::Kv);

    // EntityRef is stable (same key = same ref)
    let entity_ref2 = EntityRef::kv(run_id.clone(), "test-key");
    assert_eq!(entity_ref, entity_ref2);
}

// ============================================================================
// EventLog
// ============================================================================

#[test]
fn event_has_stable_identity() {
    let db = test_db();
    let run_id = test_run();
    let run = db.run(run_id.clone());

    let version = run.event_append("test", json!({})).unwrap();
    let sequence = version.as_u64();

    let entity_ref = EntityRef::event(run_id.clone(), sequence);

    assert_eq!(entity_ref.run_id(), &run_id);
    assert_eq!(entity_ref.primitive_type(), PrimitiveType::Event);
}

// ============================================================================
// StateCell
// ============================================================================

#[test]
fn state_has_stable_identity() {
    let db = test_db();
    let run_id = test_run();
    let run = db.run(run_id.clone());

    run.state_set("counter", StateValue::from(0)).unwrap();

    let entity_ref = EntityRef::state(run_id.clone(), "counter");

    assert_eq!(entity_ref.run_id(), &run_id);
    assert_eq!(entity_ref.primitive_type(), PrimitiveType::State);
}

// ============================================================================
// TraceStore
// ============================================================================

#[test]
fn trace_has_stable_identity() {
    let db = test_db();
    let run_id = test_run();
    let run = db.run(run_id.clone());

    let versioned_id = run.trace_record(TraceType::Custom, json!({})).unwrap();
    let trace_id = versioned_id.value;

    let entity_ref = EntityRef::trace(run_id.clone(), trace_id.clone());

    assert_eq!(entity_ref.run_id(), &run_id);
    assert_eq!(entity_ref.primitive_type(), PrimitiveType::Trace);
}

// ============================================================================
// JsonStore
// ============================================================================

#[test]
fn json_has_stable_identity() {
    let db = test_db();
    let run_id = test_run();
    let run = db.run(run_id.clone());
    let doc_id = JsonDocId::new("doc1");

    run.json_create(&doc_id, json!({})).unwrap();

    let entity_ref = EntityRef::json(run_id.clone(), doc_id.clone());

    assert_eq!(entity_ref.run_id(), &run_id);
    assert_eq!(entity_ref.primitive_type(), PrimitiveType::Json);
}

// ============================================================================
// VectorStore
// ============================================================================

#[test]
fn vector_has_stable_identity() {
    let db = test_db();
    let run_id = test_run();
    let run = db.run(run_id.clone());

    run.vector_create_collection("embeddings", VectorConfig::new(384)).unwrap();
    run.vector_upsert("embeddings", VectorId::new(1), vec![0.0; 384], None).unwrap();

    let entity_ref = EntityRef::vector(run_id.clone(), "embeddings", VectorId::new(1));

    assert_eq!(entity_ref.run_id(), &run_id);
    assert_eq!(entity_ref.primitive_type(), PrimitiveType::Vector);
}

// ============================================================================
// RunIndex
// ============================================================================

#[test]
fn run_has_stable_identity() {
    let run_id = test_run();

    let entity_ref = EntityRef::run(run_id.clone());

    assert_eq!(entity_ref.run_id(), &run_id);
    assert_eq!(entity_ref.primitive_type(), PrimitiveType::Run);
}
```

Create `tests/conformance/invariant_2_versioned.rs`:

```rust
//! Invariant 2: Everything is Versioned
//!
//! Every read returns Versioned<T>, every write returns Version.

use in_mem::{Version, Versioned};
use super::{test_db, test_run};

// ============================================================================
// KVStore
// ============================================================================

#[test]
fn kv_read_returns_versioned() {
    let db = test_db();
    let run = db.run(test_run());

    run.kv_put("key", Value::from("value")).unwrap();

    let result = run.kv_get("key").unwrap();
    assert!(result.is_some());

    let versioned = result.unwrap();
    assert!(versioned.version.is_txn_id());
    assert!(versioned.timestamp.as_micros() > 0);
}

#[test]
fn kv_write_returns_version() {
    let db = test_db();
    let run = db.run(test_run());

    let version = run.kv_put("key", Value::from("value")).unwrap();

    assert!(version.is_txn_id());
    assert!(version.as_u64() > 0);
}

// ============================================================================
// EventLog
// ============================================================================

#[test]
fn event_read_returns_versioned() {
    let db = test_db();
    let run = db.run(test_run());

    run.event_append("test", json!({})).unwrap();

    let result = run.event_read(0).unwrap();
    let versioned = result.unwrap();

    assert!(versioned.version.is_sequence());
    assert_eq!(versioned.version.as_u64(), 0);
}

#[test]
fn event_write_returns_version() {
    let db = test_db();
    let run = db.run(test_run());

    let version = run.event_append("test", json!({})).unwrap();

    assert!(version.is_sequence());
    assert_eq!(version.as_u64(), 0);
}

// ============================================================================
// StateCell
// ============================================================================

#[test]
fn state_read_returns_versioned() {
    let db = test_db();
    let run = db.run(test_run());

    run.state_set("counter", StateValue::from(42)).unwrap();

    let result = run.state_read("counter").unwrap();
    let versioned = result.unwrap();

    assert!(versioned.version.is_counter());
}

#[test]
fn state_write_returns_version() {
    let db = test_db();
    let run = db.run(test_run());

    let version = run.state_set("counter", StateValue::from(0)).unwrap();

    assert!(version.is_counter());
}

// Similar tests for TraceStore, JsonStore, VectorStore, RunIndex...
```

### Validation

```bash
~/.cargo/bin/cargo test --test conformance -- invariant_1
~/.cargo/bin/cargo test --test conformance -- invariant_2
```

### Complete Story

```bash
./scripts/complete-story.sh 492
```

---

## Story #493: Invariant 3-4 Conformance Tests

**GitHub Issue**: [#493](https://github.com/anibjoshi/in-mem/issues/493)
**Estimated Time**: 4 hours

### Start Story

```bash
gh issue view 493
./scripts/start-story.sh 64 493 invariant-3-4-tests
```

### Implementation

Create `tests/conformance/invariant_3_transactional.rs`:

```rust
//! Invariant 3: Everything is Transactional
//!
//! Every primitive participates in transactions.

use super::{test_db, test_run};

#[test]
fn kv_participates_in_transaction() {
    let db = test_db();
    let run = db.run(test_run());

    run.transaction(|txn| {
        txn.kv_put("key", Value::from("value"))?;
        let result = txn.kv_get("key")?;
        assert!(result.is_some());
        Ok(())
    }).unwrap();
}

#[test]
fn event_participates_in_transaction() {
    let db = test_db();
    let run = db.run(test_run());

    run.transaction(|txn| {
        let version = txn.event_append("test", json!({}))?;
        let result = txn.event_read(version.as_u64())?;
        assert!(result.is_some());
        Ok(())
    }).unwrap();
}

#[test]
fn transaction_rollback_affects_all_primitives() {
    let db = test_db();
    let run = db.run(test_run());

    let result = run.transaction(|txn| {
        txn.kv_put("key", Value::from("value"))?;
        txn.event_append("test", json!({}))?;
        // Force rollback
        Err(StrataError::TransactionAborted { reason: "test".into() })
    });

    assert!(result.is_err());

    // Both operations should have been rolled back
    assert!(run.kv_get("key").unwrap().is_none());
}

// Similar tests for state, trace, json, vector, run...
```

Create `tests/conformance/invariant_4_lifecycle.rs`:

```rust
//! Invariant 4: Everything Has a Lifecycle
//!
//! Every primitive follows create/exist/evolve/destroy.

#[test]
fn kv_lifecycle_create_exist_evolve_destroy() {
    let db = test_db();
    let run = db.run(test_run());

    // CREATE
    run.kv_put("key", Value::from("v1")).unwrap();

    // EXIST
    assert!(run.kv_exists("key").unwrap());
    let v = run.kv_get("key").unwrap().unwrap();
    assert_eq!(v.value, Value::from("v1"));

    // EVOLVE
    run.kv_put("key", Value::from("v2")).unwrap();
    let v = run.kv_get("key").unwrap().unwrap();
    assert_eq!(v.value, Value::from("v2"));

    // DESTROY
    assert!(run.kv_delete("key").unwrap());
    assert!(!run.kv_exists("key").unwrap());
}

#[test]
fn event_lifecycle_create_exist() {
    let db = test_db();
    let run = db.run(test_run());

    // CREATE (implicit via append)
    let version = run.event_append("test", json!({})).unwrap();

    // EXIST
    let event = run.event_read(version.as_u64()).unwrap();
    assert!(event.is_some());

    // Events are IMMUTABLE - no evolve, no destroy
}

// Similar tests for state (CRUD), trace (CR), json (CRUD), vector (CRUD), run (CRUD)...
```

### Complete Story

```bash
./scripts/complete-story.sh 493
```

---

## Story #494: Invariant 5-6 Conformance Tests

**GitHub Issue**: [#494](https://github.com/anibjoshi/in-mem/issues/494)
**Estimated Time**: 4 hours

### Start Story

```bash
gh issue view 494
./scripts/start-story.sh 64 494 invariant-5-6-tests
```

### Implementation

Create `tests/conformance/invariant_5_run_scoped.rs`:

```rust
//! Invariant 5: Everything Exists Within a Run
//!
//! Every entity belongs to exactly one run.

#[test]
fn kv_is_run_scoped() {
    let db = test_db();
    let run1 = db.run(RunId::new("run1"));
    let run2 = db.run(RunId::new("run2"));

    // Write to run1
    run1.kv_put("key", Value::from("run1-value")).unwrap();

    // Not visible in run2
    assert!(run2.kv_get("key").unwrap().is_none());

    // Write different value to run2
    run2.kv_put("key", Value::from("run2-value")).unwrap();

    // Each run sees its own value
    assert_eq!(
        run1.kv_get("key").unwrap().unwrap().value,
        Value::from("run1-value")
    );
    assert_eq!(
        run2.kv_get("key").unwrap().unwrap().value,
        Value::from("run2-value")
    );
}

#[test]
fn event_is_run_scoped() {
    let db = test_db();
    let run1 = db.run(RunId::new("run1"));
    let run2 = db.run(RunId::new("run2"));

    // Append to run1
    run1.event_append("test", json!({"run": 1})).unwrap();

    // Not visible in run2
    assert!(run2.event_read(0).unwrap().is_none());

    // Append to run2 gets sequence 0 (independent)
    let v = run2.event_append("test", json!({"run": 2})).unwrap();
    assert_eq!(v.as_u64(), 0);
}

// Similar tests for state, trace, json, vector...
```

Create `tests/conformance/invariant_6_introspectable.rs`:

```rust
//! Invariant 6: Everything is Introspectable
//!
//! Every primitive has exists() or equivalent.

#[test]
fn kv_is_introspectable() {
    let db = test_db();
    let run = db.run(test_run());

    // Can check existence before creation
    assert!(!run.kv_exists("key").unwrap());

    // Can check existence after creation
    run.kv_put("key", Value::from("value")).unwrap();
    assert!(run.kv_exists("key").unwrap());

    // Can check existence after deletion
    run.kv_delete("key").unwrap();
    assert!(!run.kv_exists("key").unwrap());
}

#[test]
fn event_is_introspectable() {
    let db = test_db();
    let run = db.run(test_run());

    // Can check existence via read (None vs Some)
    assert!(run.event_read(0).unwrap().is_none());

    run.event_append("test", json!({})).unwrap();
    assert!(run.event_read(0).unwrap().is_some());
}

// Similar tests for state, trace, json, vector, run...
```

### Complete Story

```bash
./scripts/complete-story.sh 494
```

---

## Story #495: Invariant 7 Conformance Tests

**GitHub Issue**: [#495](https://github.com/anibjoshi/in-mem/issues/495)
**Estimated Time**: 3 hours

### Start Story

```bash
gh issue view 495
./scripts/start-story.sh 64 495 invariant-7-tests
```

### Implementation

Create `tests/conformance/invariant_7_read_write.rs`:

```rust
//! Invariant 7: Reads and Writes Have Consistent Semantics
//!
//! - Reads never modify state
//! - Writes always produce versions
//! - Within a transaction, reads see prior writes

#[test]
fn kv_read_does_not_modify_state() {
    let db = test_db();
    let run = db.run(test_run());

    run.kv_put("key", Value::from("value")).unwrap();
    let v1 = run.kv_get("key").unwrap().unwrap();

    // Multiple reads don't change version
    let v2 = run.kv_get("key").unwrap().unwrap();
    let v3 = run.kv_get("key").unwrap().unwrap();

    assert_eq!(v1.version, v2.version);
    assert_eq!(v2.version, v3.version);
}

#[test]
fn kv_write_always_produces_version() {
    let db = test_db();
    let run = db.run(test_run());

    // Every write produces a version
    let v1 = run.kv_put("key", Value::from("v1")).unwrap();
    let v2 = run.kv_put("key", Value::from("v2")).unwrap();
    let v3 = run.kv_put("key", Value::from("v3")).unwrap();

    // Versions are increasing
    assert!(v2.as_u64() > v1.as_u64());
    assert!(v3.as_u64() > v2.as_u64());
}

#[test]
fn kv_read_your_writes_in_transaction() {
    let db = test_db();
    let run = db.run(test_run());

    run.transaction(|txn| {
        txn.kv_put("key", Value::from("value"))?;

        // Can read what was just written
        let result = txn.kv_get("key")?;
        assert!(result.is_some());
        assert_eq!(result.unwrap().value, Value::from("value"));

        Ok(())
    }).unwrap();
}

// Similar tests for event, state, trace, json, vector, run...
```

### Complete Story

```bash
./scripts/complete-story.sh 495
```

---

## Story #496: Cross-Primitive Transaction Conformance

**GitHub Issue**: [#496](https://github.com/anibjoshi/in-mem/issues/496)
**Estimated Time**: 4 hours

### Start Story

```bash
gh issue view 496
./scripts/start-story.sh 64 496 cross-primitive-tests
```

### Implementation

Create `tests/conformance/cross_primitive.rs`:

```rust
//! Cross-primitive transaction conformance tests

#[test]
fn all_primitives_in_single_transaction() {
    let db = test_db();
    let run = db.run(test_run());

    // Create vector collection first
    run.vector_create_collection("embeddings", VectorConfig::new(384)).unwrap();

    run.transaction(|txn| {
        // KV
        txn.kv_put("config", Value::from("active"))?;

        // Event
        txn.event_append("start", json!({"timestamp": 123}))?;

        // State
        txn.state_set("counter", StateValue::from(0))?;

        // Trace
        txn.trace_record(TraceType::Custom, json!({"action": "init"}))?;

        // JSON
        txn.json_create(&JsonDocId::new("doc1"), json!({"version": 1}))?;

        // Vector
        txn.vector_upsert("embeddings", VectorId::new(1), vec![0.0; 384], None)?;

        Ok(())
    }).unwrap();

    // Verify all writes are visible
    assert!(run.kv_get("config").unwrap().is_some());
    assert!(run.event_read(0).unwrap().is_some());
    assert!(run.state_read("counter").unwrap().is_some());
    assert!(run.json_get(&JsonDocId::new("doc1")).unwrap().is_some());
    assert!(run.vector_get("embeddings", VectorId::new(1)).unwrap().is_some());
}

#[test]
fn cross_primitive_rollback() {
    let db = test_db();
    let run = db.run(test_run());

    run.vector_create_collection("embeddings", VectorConfig::new(384)).unwrap();

    let result = run.transaction(|txn| {
        // Write to multiple primitives
        txn.kv_put("key", Value::from("value"))?;
        txn.event_append("test", json!({}))?;
        txn.state_set("state", StateValue::from(1))?;
        txn.json_create(&JsonDocId::new("doc"), json!({}))?;
        txn.vector_upsert("embeddings", VectorId::new(1), vec![0.0; 384], None)?;

        // Force rollback
        Err(StrataError::TransactionAborted {
            reason: "intentional rollback".into()
        })
    });

    assert!(result.is_err());

    // ALL writes should be rolled back
    assert!(run.kv_get("key").unwrap().is_none());
    assert!(run.event_read(0).unwrap().is_none());
    assert!(run.state_read("state").unwrap().is_none());
    assert!(run.json_get(&JsonDocId::new("doc")).unwrap().is_none());
    assert!(run.vector_get("embeddings", VectorId::new(1)).unwrap().is_none());
}

#[test]
fn cross_primitive_read_your_writes() {
    let db = test_db();
    let run = db.run(test_run());

    run.transaction(|txn| {
        // Write to KV
        txn.kv_put("last_event", Value::from(0))?;

        // Append event
        let version = txn.event_append("action", json!({}))?;

        // Update KV with event sequence
        txn.kv_put("last_event", Value::from(version.as_u64() as i64))?;

        // Read back - should see updated value
        let kv = txn.kv_get("last_event")?.unwrap();
        assert_eq!(kv.value, Value::from(version.as_u64() as i64));

        // Update state based on KV
        txn.state_set("processed", StateValue::from(true))?;

        Ok(())
    }).unwrap();
}

#[test]
fn partial_failure_rolls_back_all() {
    let db = test_db();
    let run = db.run(test_run());

    // Pre-create a KV entry
    run.kv_put("existing", Value::from("original")).unwrap();

    let result = run.transaction(|txn| {
        // Successful write
        txn.kv_put("new_key", Value::from("new_value"))?;

        // Modify existing
        txn.kv_put("existing", Value::from("modified"))?;

        // This should fail (e.g., dimension mismatch)
        txn.vector_upsert("nonexistent_collection", VectorId::new(1), vec![0.0; 100], None)?;

        Ok(())
    });

    assert!(result.is_err());

    // New key should not exist
    assert!(run.kv_get("new_key").unwrap().is_none());

    // Existing key should have original value
    let existing = run.kv_get("existing").unwrap().unwrap();
    assert_eq!(existing.value, Value::from("original"));
}
```

### Validation

```bash
~/.cargo/bin/cargo test --test conformance -- cross_primitive
```

### Complete Story

```bash
./scripts/complete-story.sh 496
```

---

## Epic 64 Completion Checklist

### 1. Final Validation

```bash
# Run all conformance tests
~/.cargo/bin/cargo test --test conformance

# Verify test count
~/.cargo/bin/cargo test --test conformance 2>&1 | grep "test result"
# Should show ~49+ tests
```

### 2. Verify Conformance Matrix

| Primitive | I1 | I2 | I3 | I4 | I5 | I6 | I7 |
|-----------|----|----|----|----|----|----|-----|
| KVStore | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| EventLog | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| StateCell | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| TraceStore | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| JsonStore | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| VectorStore | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |
| RunIndex | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ |

### 3. Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-64-conformance-testing -m "Epic 64: Conformance Testing complete

Delivered:
- 49 conformance tests (7 primitives × 7 invariants)
- Cross-primitive transaction tests
- All primitives verified to conform to all invariants

Stories: #492, #493, #494, #495, #496
"
git push origin develop
gh issue close 468 --comment "Epic 64: Conformance Testing - COMPLETE"
```

---

## Summary

Epic 64 validates that all 7 primitives conform to all 7 invariants. This is the final verification that M9 has achieved its goal: a stable, consistent API across all primitives.

The conformance tests serve as:
1. **Validation** that M9 is complete
2. **Documentation** of expected behavior
3. **Regression protection** for future changes
