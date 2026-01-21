# Epic 82: Substrate API Implementation

**Goal**: Implement the power-user Substrate API with explicit run_id, versions, and transactions

**Dependencies**: Epic 80 (Value Model)

**Milestone**: M11a (Core Contract & API)

---

## Test-Driven Development Protocol

> **CRITICAL**: This epic follows strict Test-Driven Development (TDD). Tests are written FIRST, then implementation.

### NEVER Modify Tests to Make Them Pass

> **ABSOLUTE RULE**: When a test fails, the problem is in the implementation, NOT the test.

**FORBIDDEN behaviors:**
- Changing test assertions to match buggy output
- Weakening test conditions
- Removing test cases that expose bugs
- Adding `#[ignore]` to failing tests

**REQUIRED behaviors:**
- Investigate WHY the test fails
- Fix the implementation to match the specification
- If the spec is wrong, get explicit approval before changing both spec AND test

---

## Scope

- Explicit run_id on every operation
- KV operations returning Versioned<T>
- JSON operations with explicit run
- Event operations with version semantics
- Vector operations with explicit parameters
- State (CAS) operations
- Transaction control (begin, commit, rollback)
- Run lifecycle (create_run, close_run, list_runs)
- History operations with version access

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #574 | KV Substrate Operations | FOUNDATION |
| #575 | JSON Substrate Operations | CRITICAL |
| #576 | Event Substrate Operations | CRITICAL |
| #577 | Vector Substrate Operations | CRITICAL |
| #578 | State Substrate Operations | CRITICAL |
| #579 | Transaction Control | CRITICAL |
| #580 | Run Lifecycle Operations | HIGH |

---

## Story #574: KV Substrate Operations

**File**: `crates/api/src/substrate/kv.rs` (NEW)

**Deliverable**: KV operations with explicit run_id and Versioned returns

### Tests FIRST

```rust
#[cfg(test)]
mod substrate_kv_tests {
    use super::*;

    // === kv_put() Tests ===

    #[test]
    fn test_kv_put_with_explicit_run() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        // Must specify run_id explicitly
        substrate.kv_put("default", "key", Value::Int(42)).unwrap();

        let result = substrate.kv_get("default", "key").unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_kv_put_different_runs_isolated() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        substrate.create_run("run_a").unwrap();
        substrate.create_run("run_b").unwrap();

        substrate.kv_put("run_a", "key", Value::Int(1)).unwrap();
        substrate.kv_put("run_b", "key", Value::Int(2)).unwrap();

        // Keys in different runs are isolated
        let a = substrate.kv_get("run_a", "key").unwrap().unwrap();
        let b = substrate.kv_get("run_b", "key").unwrap().unwrap();

        assert_eq!(a.value, Value::Int(1));
        assert_eq!(b.value, Value::Int(2));
    }

    #[test]
    fn test_kv_put_unknown_run_fails() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        let result = substrate.kv_put("nonexistent_run", "key", Value::Int(1));
        assert!(matches!(result, Err(SubstrateError::RunNotFound { .. })));
    }

    // === kv_get() Tests ===

    #[test]
    fn test_kv_get_returns_versioned() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        substrate.kv_put("default", "key", Value::Int(42)).unwrap();

        let result = substrate.kv_get("default", "key").unwrap();
        assert!(result.is_some());

        let versioned = result.unwrap();
        assert_eq!(versioned.value, Value::Int(42));
        assert!(versioned.version.value() > 0);
        assert!(versioned.timestamp > 0);
    }

    #[test]
    fn test_kv_get_version_is_txn_type() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        substrate.kv_put("default", "key", Value::Int(1)).unwrap();

        let versioned = substrate.kv_get("default", "key").unwrap().unwrap();

        // KV uses Txn version type
        assert!(matches!(versioned.version, Version::Txn(_)));
    }

    #[test]
    fn test_kv_get_missing_returns_none() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        let result = substrate.kv_get("default", "nonexistent").unwrap();
        assert!(result.is_none());
    }

    // === kv_delete() Tests ===

    #[test]
    fn test_kv_delete_returns_bool() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        substrate.kv_put("default", "key", Value::Int(1)).unwrap();

        let deleted = substrate.kv_delete("default", "key").unwrap();
        assert!(deleted);

        // Key is gone
        assert!(substrate.kv_get("default", "key").unwrap().is_none());
    }

    #[test]
    fn test_kv_delete_nonexistent_returns_false() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        let deleted = substrate.kv_delete("default", "nonexistent").unwrap();
        assert!(!deleted);
    }

    // === kv_incr() Tests ===

    #[test]
    fn test_kv_incr_returns_new_value() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        substrate.kv_put("default", "counter", Value::Int(10)).unwrap();

        let new_value = substrate.kv_incr("default", "counter", 5).unwrap();
        assert_eq!(new_value, 15);
    }

    #[test]
    fn test_kv_incr_type_must_be_int() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        // Store Float
        substrate.kv_put("default", "key", Value::Float(1.0)).unwrap();

        // incr on Float fails - NO TYPE COERCION
        let result = substrate.kv_incr("default", "key", 1);
        assert!(matches!(result, Err(SubstrateError::WrongType { .. })));
    }
}
```

### Implementation

```rust
use crate::value::Value;
use crate::versioned::{Versioned, Version};
use crate::error::SubstrateError;

pub struct Substrate {
    engine: Engine,
}

impl Substrate {
    /// Put a key-value pair in the specified run
    ///
    /// Requires explicit run_id. Use "default" for the default run.
    pub fn kv_put(&self, run_id: &str, key: &str, value: Value) -> Result<(), SubstrateError> {
        self.validate_run_exists(run_id)?;
        validate_key(key)?;

        self.engine.kv_put(run_id, key, value)?;
        Ok(())
    }

    /// Get a versioned value from the specified run
    ///
    /// Returns full Versioned<Value> with version and timestamp.
    pub fn kv_get(&self, run_id: &str, key: &str) -> Result<Option<Versioned<Value>>, SubstrateError> {
        self.validate_run_exists(run_id)?;
        validate_key(key)?;

        self.engine.kv_get(run_id, key).map_err(Into::into)
    }

    /// Delete a key from the specified run
    ///
    /// Returns true if key existed and was deleted.
    pub fn kv_delete(&self, run_id: &str, key: &str) -> Result<bool, SubstrateError> {
        self.validate_run_exists(run_id)?;
        validate_key(key)?;

        self.engine.kv_delete(run_id, key).map_err(Into::into)
    }

    /// Increment integer value by delta
    ///
    /// Creates key with delta if not exists.
    /// Fails with WrongType if value is not Int (NO COERCION).
    pub fn kv_incr(&self, run_id: &str, key: &str, delta: i64) -> Result<i64, SubstrateError> {
        self.validate_run_exists(run_id)?;
        validate_key(key)?;

        let current = self.engine.kv_get(run_id, key)?;

        let new_value = match current {
            None => delta,
            Some(versioned) => {
                match versioned.value {
                    Value::Int(i) => {
                        i.checked_add(delta)
                            .ok_or(SubstrateError::Overflow)?
                    }
                    other => {
                        return Err(SubstrateError::WrongType {
                            expected: "Int",
                            actual: other.type_name(),
                        });
                    }
                }
            }
        };

        self.engine.kv_put(run_id, key, Value::Int(new_value))?;
        Ok(new_value)
    }

    fn validate_run_exists(&self, run_id: &str) -> Result<(), SubstrateError> {
        if !self.engine.run_exists(run_id)? {
            return Err(SubstrateError::RunNotFound {
                run_id: run_id.to_string(),
            });
        }
        Ok(())
    }
}
```

### Acceptance Criteria

- [ ] All operations require explicit run_id
- [ ] Operations on unknown run return RunNotFound error
- [ ] `kv_get()` returns `Versioned<Value>` with version and timestamp
- [ ] Version type is `Txn` for KV operations
- [ ] Different runs are isolated
- [ ] `kv_incr()` fails with WrongType for non-Int values

---

## Story #575: JSON Substrate Operations

**File**: `crates/api/src/substrate/json.rs` (NEW)

**Deliverable**: JSON operations with explicit run_id

### Tests FIRST

```rust
#[cfg(test)]
mod substrate_json_tests {
    use super::*;

    #[test]
    fn test_json_set_with_explicit_run() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        substrate.json_set("default", "doc", "$", Value::Object(HashMap::new())).unwrap();

        let result = substrate.json_get("default", "doc", "$").unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_json_operations_run_isolated() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        substrate.create_run("run_a").unwrap();
        substrate.create_run("run_b").unwrap();

        substrate.json_set("run_a", "doc", "$", Value::Object({
            let mut m = HashMap::new();
            m.insert("run".to_string(), Value::String("a".into()));
            m
        })).unwrap();

        substrate.json_set("run_b", "doc", "$", Value::Object({
            let mut m = HashMap::new();
            m.insert("run".to_string(), Value::String("b".into()));
            m
        })).unwrap();

        let a = substrate.json_get("run_a", "doc", "$.run").unwrap();
        let b = substrate.json_get("run_b", "doc", "$.run").unwrap();

        assert_eq!(a, Some(Value::String("a".into())));
        assert_eq!(b, Some(Value::String("b".into())));
    }

    #[test]
    fn test_json_get_returns_versioned() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        substrate.json_set("default", "doc", "$", Value::Object(HashMap::new())).unwrap();

        let result = substrate.json_getv("default", "doc", "$").unwrap();
        assert!(result.is_some());

        let versioned = result.unwrap();
        assert!(versioned.version.value() > 0);
    }
}
```

### Acceptance Criteria

- [ ] All JSON operations require explicit run_id
- [ ] `json_getv()` returns versioned result
- [ ] Different runs are isolated
- [ ] Unknown run returns error

---

## Story #576: Event Substrate Operations

**File**: `crates/api/src/substrate/event.rs` (NEW)

**Deliverable**: Event operations with sequence versions

### Tests FIRST

```rust
#[cfg(test)]
mod substrate_event_tests {
    use super::*;

    #[test]
    fn test_event_append_returns_sequence_version() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        let version = substrate.event_append("default", "stream", Value::Int(1)).unwrap();

        // Event logs use Sequence version type
        assert!(matches!(version, Version::Sequence(_)));
    }

    #[test]
    fn test_event_versions_are_sequential() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        let v1 = substrate.event_append("default", "stream", Value::Int(1)).unwrap();
        let v2 = substrate.event_append("default", "stream", Value::Int(2)).unwrap();
        let v3 = substrate.event_append("default", "stream", Value::Int(3)).unwrap();

        match (v1, v2, v3) {
            (Version::Sequence(s1), Version::Sequence(s2), Version::Sequence(s3)) => {
                assert!(s1 < s2);
                assert!(s2 < s3);
            }
            _ => panic!("Expected Sequence versions"),
        }
    }

    #[test]
    fn test_event_range_with_version_bounds() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        let v1 = substrate.event_append("default", "stream", Value::Int(1)).unwrap();
        let _v2 = substrate.event_append("default", "stream", Value::Int(2)).unwrap();
        let v3 = substrate.event_append("default", "stream", Value::Int(3)).unwrap();

        // Get events from v1 to v3
        let events = substrate.event_range(
            "default",
            "stream",
            Some(v1),
            Some(v3),
            None,
        ).unwrap();

        assert_eq!(events.len(), 3);
    }

    #[test]
    fn test_event_get_at_version() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        let v1 = substrate.event_append("default", "stream", Value::Int(42)).unwrap();

        let event = substrate.event_get_at("default", "stream", v1).unwrap();
        assert!(event.is_some());
        assert_eq!(event.unwrap().value, Value::Int(42));
    }
}
```

### Acceptance Criteria

- [ ] `event_append()` returns Sequence version type
- [ ] Sequence versions are monotonically increasing
- [ ] `event_range()` supports version bounds
- [ ] `event_get_at()` retrieves specific version
- [ ] Different runs are isolated

---

## Story #577: Vector Substrate Operations

**File**: `crates/api/src/substrate/vector.rs` (NEW)

**Deliverable**: Vector operations with explicit parameters

### Tests FIRST

```rust
#[cfg(test)]
mod substrate_vector_tests {
    use super::*;

    #[test]
    fn test_vector_put_with_explicit_run() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        substrate.vector_put(
            "default",
            "doc1",
            vec![1.0, 2.0, 3.0],
            Value::Object(HashMap::new()),
        ).unwrap();

        let result = substrate.vector_get("default", "doc1").unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_vector_search_with_explicit_params() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        substrate.vector_put("default", "doc1", vec![1.0, 0.0, 0.0], Value::Null).unwrap();
        substrate.vector_put("default", "doc2", vec![0.0, 1.0, 0.0], Value::Null).unwrap();

        let results = substrate.vector_search(
            "default",
            vec![1.0, 0.0, 0.0],  // Query vector
            10,                    // k
            None,                  // filter
        ).unwrap();

        // doc1 should be most similar
        assert!(!results.is_empty());
        assert_eq!(results[0].id, "doc1");
    }

    #[test]
    fn test_vector_dimension_validation() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        // Store 3D vector
        substrate.vector_put("default", "doc1", vec![1.0, 2.0, 3.0], Value::Null).unwrap();

        // Search with 2D vector should fail
        let result = substrate.vector_search("default", vec![1.0, 2.0], 10, None);
        assert!(matches!(result, Err(SubstrateError::ConstraintViolation { reason })
            if reason == "vector_dim_mismatch"));
    }
}
```

### Acceptance Criteria

- [ ] All vector operations require explicit run_id
- [ ] `vector_search()` validates dimension match
- [ ] Search returns results with scores
- [ ] Different runs are isolated

---

## Story #578: State Substrate Operations

**File**: `crates/api/src/substrate/state.rs` (NEW)

**Deliverable**: CAS operations with explicit parameters

### Tests FIRST

```rust
#[cfg(test)]
mod substrate_state_tests {
    use super::*;

    #[test]
    fn test_cas_with_explicit_run() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        let success = substrate.cas_set("default", "key", Value::Null, Value::Int(1)).unwrap();
        assert!(success);

        let value = substrate.cas_get("default", "key").unwrap();
        assert_eq!(value, Some(Value::Int(1)));
    }

    #[test]
    fn test_cas_version_is_counter_type() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        substrate.cas_set("default", "key", Value::Null, Value::Int(1)).unwrap();

        let versioned = substrate.cas_getv("default", "key").unwrap().unwrap();

        // State cells use Counter version type
        assert!(matches!(versioned.version, Version::Counter(_)));
    }

    #[test]
    fn test_cas_respects_type_distinction() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        // Set Int(1)
        substrate.cas_set("default", "key", Value::Null, Value::Int(1)).unwrap();

        // CAS with Float(1.0) expected should fail
        // CRITICAL: Int(1) != Float(1.0)
        let success = substrate.cas_set("default", "key", Value::Float(1.0), Value::Int(2)).unwrap();
        assert!(!success);

        // Value unchanged
        assert_eq!(substrate.cas_get("default", "key").unwrap(), Some(Value::Int(1)));
    }
}
```

### Acceptance Criteria

- [ ] CAS operations require explicit run_id
- [ ] `cas_getv()` returns Counter version type
- [ ] **CRITICAL**: Type matters in CAS comparison (Int(1) != Float(1.0))
- [ ] Different runs are isolated

---

## Story #579: Transaction Control

**File**: `crates/api/src/substrate/transaction.rs` (NEW)

**Deliverable**: Transaction control for batched operations

### Tests FIRST

```rust
#[cfg(test)]
mod transaction_tests {
    use super::*;

    #[test]
    fn test_transaction_commit() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        substrate.begin_transaction().unwrap();
        substrate.kv_put("default", "key1", Value::Int(1)).unwrap();
        substrate.kv_put("default", "key2", Value::Int(2)).unwrap();
        substrate.commit().unwrap();

        // Both keys persisted
        assert_eq!(substrate.kv_get("default", "key1").unwrap().unwrap().value, Value::Int(1));
        assert_eq!(substrate.kv_get("default", "key2").unwrap().unwrap().value, Value::Int(2));
    }

    #[test]
    fn test_transaction_rollback() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        substrate.kv_put("default", "key", Value::Int(1)).unwrap();

        substrate.begin_transaction().unwrap();
        substrate.kv_put("default", "key", Value::Int(999)).unwrap();
        substrate.rollback().unwrap();

        // Value unchanged
        assert_eq!(substrate.kv_get("default", "key").unwrap().unwrap().value, Value::Int(1));
    }

    #[test]
    fn test_transaction_with_closure() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        substrate.with_transaction(|txn| {
            txn.kv_put("default", "a", Value::Int(1))?;
            txn.kv_put("default", "b", Value::Int(2))?;
            Ok(())
        }).unwrap();

        assert!(substrate.kv_get("default", "a").unwrap().is_some());
        assert!(substrate.kv_get("default", "b").unwrap().is_some());
    }

    #[test]
    fn test_transaction_closure_error_rolls_back() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        let result = substrate.with_transaction(|txn| {
            txn.kv_put("default", "a", Value::Int(1))?;
            // Force error
            Err(SubstrateError::Internal("test error".into()))
        });

        assert!(result.is_err());

        // "a" should not exist (rolled back)
        assert!(substrate.kv_get("default", "a").unwrap().is_none());
    }

    #[test]
    fn test_auto_commit_without_explicit_transaction() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        // Without begin_transaction, each op auto-commits
        substrate.kv_put("default", "key", Value::Int(1)).unwrap();

        // Immediately visible
        assert!(substrate.kv_get("default", "key").unwrap().is_some());
    }
}
```

### Implementation

```rust
impl Substrate {
    /// Begin a new transaction
    ///
    /// Operations after this are batched until commit() or rollback().
    pub fn begin_transaction(&self) -> Result<(), SubstrateError> {
        self.engine.begin_transaction().map_err(Into::into)
    }

    /// Commit the current transaction
    ///
    /// All operations since begin_transaction() are atomically applied.
    pub fn commit(&self) -> Result<(), SubstrateError> {
        self.engine.commit().map_err(Into::into)
    }

    /// Rollback the current transaction
    ///
    /// All operations since begin_transaction() are discarded.
    pub fn rollback(&self) -> Result<(), SubstrateError> {
        self.engine.rollback().map_err(Into::into)
    }

    /// Execute operations in a transaction with automatic commit/rollback
    ///
    /// If the closure returns Ok, commits. If Err, rolls back.
    pub fn with_transaction<F, T>(&self, f: F) -> Result<T, SubstrateError>
    where
        F: FnOnce(&TransactionContext) -> Result<T, SubstrateError>,
    {
        self.begin_transaction()?;

        match f(&TransactionContext { substrate: self }) {
            Ok(result) => {
                self.commit()?;
                Ok(result)
            }
            Err(e) => {
                self.rollback()?;
                Err(e)
            }
        }
    }
}

pub struct TransactionContext<'a> {
    substrate: &'a Substrate,
}

impl<'a> TransactionContext<'a> {
    pub fn kv_put(&self, run_id: &str, key: &str, value: Value) -> Result<(), SubstrateError> {
        self.substrate.kv_put(run_id, key, value)
    }

    // ... other operations delegated to substrate
}
```

### Acceptance Criteria

- [ ] `begin_transaction()` starts a new transaction
- [ ] `commit()` atomically applies all operations
- [ ] `rollback()` discards all operations since begin
- [ ] `with_transaction()` auto-commits on Ok
- [ ] `with_transaction()` auto-rollbacks on Err
- [ ] Operations without explicit transaction auto-commit

---

## Story #580: Run Lifecycle Operations

**File**: `crates/api/src/substrate/run.rs` (NEW)

**Deliverable**: Run creation, listing, and closing

### Tests FIRST

```rust
#[cfg(test)]
mod run_lifecycle_tests {
    use super::*;

    #[test]
    fn test_default_run_always_exists() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        // Default run exists without creation
        let runs = substrate.list_runs().unwrap();
        assert!(runs.iter().any(|r| r.name == "default"));
    }

    #[test]
    fn test_create_run() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        substrate.create_run("myrun").unwrap();

        let runs = substrate.list_runs().unwrap();
        assert!(runs.iter().any(|r| r.name == "myrun"));
    }

    #[test]
    fn test_create_duplicate_run_fails() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        substrate.create_run("myrun").unwrap();

        let result = substrate.create_run("myrun");
        assert!(matches!(result, Err(SubstrateError::RunExists { .. })));
    }

    #[test]
    fn test_close_run() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        substrate.create_run("myrun").unwrap();
        substrate.kv_put("myrun", "key", Value::Int(1)).unwrap();

        substrate.close_run("myrun").unwrap();

        // Writes to closed run fail
        let result = substrate.kv_put("myrun", "key2", Value::Int(2));
        assert!(matches!(result, Err(SubstrateError::RunClosed { .. })));
    }

    #[test]
    fn test_close_run_still_readable() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        substrate.create_run("myrun").unwrap();
        substrate.kv_put("myrun", "key", Value::Int(42)).unwrap();
        substrate.close_run("myrun").unwrap();

        // Reads still work
        let value = substrate.kv_get("myrun", "key").unwrap();
        assert_eq!(value.unwrap().value, Value::Int(42));
    }

    #[test]
    fn test_cannot_close_default_run() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        let result = substrate.close_run("default");
        assert!(matches!(result, Err(SubstrateError::CannotCloseDefaultRun)));
    }

    #[test]
    fn test_list_runs() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        substrate.create_run("run1").unwrap();
        substrate.create_run("run2").unwrap();

        let runs = substrate.list_runs().unwrap();
        assert!(runs.len() >= 3); // default + run1 + run2

        let names: Vec<_> = runs.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"default"));
        assert!(names.contains(&"run1"));
        assert!(names.contains(&"run2"));
    }
}
```

### Acceptance Criteria

- [ ] Default run ("default") always exists
- [ ] `create_run()` creates new run
- [ ] Cannot create duplicate run
- [ ] `close_run()` makes run read-only
- [ ] Cannot close default run
- [ ] `list_runs()` returns all runs

---

## Testing

Integration tests for substrate behavior:

```rust
#[cfg(test)]
mod substrate_integration_tests {
    use super::*;

    #[test]
    fn test_version_types_by_primitive() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        // KV uses Txn
        substrate.kv_put("default", "kvkey", Value::Int(1)).unwrap();
        let kv = substrate.kv_get("default", "kvkey").unwrap().unwrap();
        assert!(matches!(kv.version, Version::Txn(_)));

        // Event uses Sequence
        let event_v = substrate.event_append("default", "stream", Value::Int(1)).unwrap();
        assert!(matches!(event_v, Version::Sequence(_)));

        // State uses Counter
        substrate.cas_set("default", "statekey", Value::Null, Value::Int(1)).unwrap();
        let state = substrate.cas_getv("default", "statekey").unwrap().unwrap();
        assert!(matches!(state.version, Version::Counter(_)));
    }

    #[test]
    fn test_run_isolation_comprehensive() {
        let harness = TestHarness::new();
        let substrate = harness.substrate();

        substrate.create_run("isolated").unwrap();

        // Write to isolated run
        substrate.kv_put("isolated", "key", Value::Int(42)).unwrap();

        // Default run should not see it
        assert!(substrate.kv_get("default", "key").unwrap().is_none());

        // Isolated run should see it
        assert_eq!(
            substrate.kv_get("isolated", "key").unwrap().unwrap().value,
            Value::Int(42)
        );
    }
}
```

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/api/src/substrate/mod.rs` | CREATE - Substrate module |
| `crates/api/src/substrate/kv.rs` | CREATE - KV operations |
| `crates/api/src/substrate/json.rs` | CREATE - JSON operations |
| `crates/api/src/substrate/event.rs` | CREATE - Event operations |
| `crates/api/src/substrate/vector.rs` | CREATE - Vector operations |
| `crates/api/src/substrate/state.rs` | CREATE - CAS operations |
| `crates/api/src/substrate/transaction.rs` | CREATE - Transaction control |
| `crates/api/src/substrate/run.rs` | CREATE - Run lifecycle |
| `crates/api/src/lib.rs` | MODIFY - Export substrate module |

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-21 | Initial epic specification |
