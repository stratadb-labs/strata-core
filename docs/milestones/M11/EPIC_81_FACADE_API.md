# Epic 81: Facade API Implementation

**Goal**: Implement the Redis-like Facade API that desugars to Substrate operations

**Dependencies**: Epic 80 (Value Model)

**Milestone**: M11a (Core Contract & API)

---

## Test-Driven Development Protocol

> **CRITICAL**: This epic follows strict Test-Driven Development (TDD). Tests are written FIRST, then implementation.

### The TDD Cycle

1. **Write the test** - Define expected behavior before writing any implementation
2. **Run the test** - Verify it fails (red)
3. **Write minimal implementation** - Just enough to pass the test
4. **Run the test** - Verify it passes (green)
5. **Refactor** - Clean up while keeping tests green

### NEVER Modify Tests to Make Them Pass

> **ABSOLUTE RULE**: When a test fails, the problem is in the implementation, NOT the test.

**FORBIDDEN behaviors:**
- Changing test assertions to match buggy output
- Weakening test conditions (e.g., `==` to `!=`, exact match to contains)
- Removing test cases that expose bugs
- Adding `#[ignore]` to failing tests
- Changing expected values to match actual (wrong) values

**REQUIRED behaviors:**
- Investigate WHY the test fails
- Fix the implementation to match the specification
- If the spec is wrong, get explicit approval before changing both spec AND test
- Document any spec changes in the epic

---

## Scope

- KV operations (set, get, getv, mget, mset, delete, exists, incr, incrby)
- JSON operations (json_set, json_get, json_delete)
- Event operations (xadd, xrange, xlen)
- Vector operations (vset, vget, vdel, vsearch)
- State operations (cas_set, cas_get)
- History operations (history)
- Auto-commit semantics
- Default run targeting

---

## Architectural Invariants

The Facade API MUST adhere to these invariants:

1. **Every facade operation desugars to exactly one substrate operation sequence**
2. **Facade adds NO semantic behavior - only defaults**
3. **Facade NEVER swallows substrate errors**
4. **Facade targets the default run (named "default")**
5. **Facade auto-commits each operation**

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #566 | KV Operations (set, get, getv) | FOUNDATION |
| #567 | KV Batch Operations (mget, mset, delete, exists) | CRITICAL |
| #568 | KV Numeric Operations (incr, incrby) | CRITICAL |
| #569 | JSON Operations | CRITICAL |
| #570 | Event Operations | CRITICAL |
| #571 | Vector Operations | CRITICAL |
| #572 | State (CAS) Operations | CRITICAL |
| #573 | History Operations | HIGH |

---

## Story #566: KV Operations (set, get, getv)

**File**: `crates/api/src/facade/kv.rs` (NEW)

**Deliverable**: Core KV operations that desugar to substrate

### Tests FIRST

```rust
#[cfg(test)]
mod kv_basic_tests {
    use super::*;
    use crate::testing::TestHarness;

    // === set() Tests ===

    #[test]
    fn test_set_returns_unit() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        let result = facade.set("key", Value::Int(42));
        assert!(result.is_ok());
        // set() returns () on success
    }

    #[test]
    fn test_set_creates_new_key() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        facade.set("newkey", Value::Int(1)).unwrap();

        let value = facade.get("newkey").unwrap();
        assert_eq!(value, Some(Value::Int(1)));
    }

    #[test]
    fn test_set_overwrites_existing_key() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        facade.set("key", Value::Int(1)).unwrap();
        facade.set("key", Value::Int(2)).unwrap();

        let value = facade.get("key").unwrap();
        assert_eq!(value, Some(Value::Int(2)));
    }

    #[test]
    fn test_set_preserves_value_type() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        facade.set("intkey", Value::Int(42)).unwrap();
        facade.set("floatkey", Value::Float(3.14)).unwrap();
        facade.set("strkey", Value::String("hello".into())).unwrap();

        assert!(matches!(facade.get("intkey").unwrap(), Some(Value::Int(42))));
        assert!(matches!(facade.get("floatkey").unwrap(), Some(Value::Float(f)) if (f - 3.14).abs() < f64::EPSILON));
        assert!(matches!(facade.get("strkey").unwrap(), Some(Value::String(s)) if s == "hello"));
    }

    #[test]
    fn test_set_invalid_key_returns_error() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        let result = facade.set("", Value::Int(1));
        assert!(matches!(result, Err(FacadeError::InvalidKey(_))));
    }

    #[test]
    fn test_set_reserved_prefix_returns_error() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        let result = facade.set("_strata/internal", Value::Int(1));
        assert!(matches!(result, Err(FacadeError::InvalidKey(_))));
    }

    // === get() Tests ===

    #[test]
    fn test_get_existing_key() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        facade.set("key", Value::Int(42)).unwrap();

        let result = facade.get("key").unwrap();
        assert_eq!(result, Some(Value::Int(42)));
    }

    #[test]
    fn test_get_missing_key_returns_none() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        let result = facade.get("nonexistent").unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_get_returns_value_not_versioned() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        facade.set("key", Value::Int(42)).unwrap();

        // get() returns Option<Value>, NOT Versioned<Value>
        let result = facade.get("key").unwrap();
        assert!(matches!(result, Some(Value::Int(42))));
    }

    // === getv() Tests ===

    #[test]
    fn test_getv_returns_versioned() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        facade.set("key", Value::Int(42)).unwrap();

        let result = facade.getv("key").unwrap();
        assert!(result.is_some());

        let versioned = result.unwrap();
        assert_eq!(versioned.value, Value::Int(42));
        assert!(versioned.version.value() > 0);
        assert!(versioned.timestamp > 0);
    }

    #[test]
    fn test_getv_missing_key_returns_none() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        let result = facade.getv("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_getv_version_increments_on_update() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        facade.set("key", Value::Int(1)).unwrap();
        let v1 = facade.getv("key").unwrap().unwrap().version;

        facade.set("key", Value::Int(2)).unwrap();
        let v2 = facade.getv("key").unwrap().unwrap().version;

        assert!(v2.value() > v1.value());
    }

    // === Desugaring Verification ===

    #[test]
    fn test_set_desugars_to_substrate() {
        let harness = TestHarness::new();
        let facade = harness.facade();
        let substrate = harness.substrate();

        // Facade set
        facade.set("key", Value::Int(42)).unwrap();

        // Verify substrate can read it (proves desugaring happened)
        let result = substrate.kv_get("default", "key").unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().value, Value::Int(42));
    }

    #[test]
    fn test_get_desugars_to_substrate() {
        let harness = TestHarness::new();
        let facade = harness.facade();
        let substrate = harness.substrate();

        // Write via substrate
        substrate.kv_put("default", "key", Value::Int(99)).unwrap();

        // Read via facade (proves desugaring)
        let result = facade.get("key").unwrap();
        assert_eq!(result, Some(Value::Int(99)));
    }
}
```

### Implementation

```rust
use crate::substrate::Substrate;
use crate::value::Value;
use crate::versioned::Versioned;
use crate::error::FacadeError;
use crate::key::validate_key;

/// Default run name (literal string, not UUID)
pub const DEFAULT_RUN: &str = "default";

/// Facade API for KV operations
pub struct Facade {
    substrate: Substrate,
}

impl Facade {
    pub fn new(substrate: Substrate) -> Self {
        Facade { substrate }
    }

    /// Set a key-value pair
    ///
    /// Desugars to: kv_put(DEFAULT_RUN, key, value) with auto-commit
    pub fn set(&self, key: &str, value: Value) -> Result<(), FacadeError> {
        validate_key(key)?;
        self.substrate.kv_put(DEFAULT_RUN, key, value)?;
        Ok(())
    }

    /// Get a value by key
    ///
    /// Desugars to: kv_get(DEFAULT_RUN, key).map(|v| v.value)
    ///
    /// Returns Option<Value>, not Versioned<Value> - version is hidden
    pub fn get(&self, key: &str) -> Result<Option<Value>, FacadeError> {
        validate_key(key)?;
        let result = self.substrate.kv_get(DEFAULT_RUN, key)?;
        Ok(result.map(|v| v.value))
    }

    /// Get a versioned value by key
    ///
    /// Desugars to: kv_get(DEFAULT_RUN, key)
    ///
    /// Escape hatch: returns full Versioned<Value>
    pub fn getv(&self, key: &str) -> Result<Option<Versioned<Value>>, FacadeError> {
        validate_key(key)?;
        self.substrate.kv_get(DEFAULT_RUN, key).map_err(Into::into)
    }
}
```

### Acceptance Criteria

- [ ] `set()` creates/overwrites key-value pairs
- [ ] `set()` returns `()` on success
- [ ] `set()` validates key and returns `InvalidKey` error
- [ ] `get()` returns `Option<Value>` (version hidden)
- [ ] `get()` returns `None` for missing keys
- [ ] `getv()` returns `Option<Versioned<Value>>`
- [ ] All operations desugar to substrate calls with DEFAULT_RUN
- [ ] Auto-commit semantics (each operation is atomic)

---

## Story #567: KV Batch Operations

**File**: `crates/api/src/facade/kv.rs`

**Deliverable**: mget, mset, delete, exists operations

### Tests FIRST

```rust
#[cfg(test)]
mod kv_batch_tests {
    use super::*;

    // === mget() Tests ===

    #[test]
    fn test_mget_returns_values_in_order() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        facade.set("a", Value::Int(1)).unwrap();
        facade.set("b", Value::Int(2)).unwrap();
        facade.set("c", Value::Int(3)).unwrap();

        let result = facade.mget(&["a", "b", "c"]).unwrap();
        assert_eq!(result, vec![
            Some(Value::Int(1)),
            Some(Value::Int(2)),
            Some(Value::Int(3)),
        ]);
    }

    #[test]
    fn test_mget_returns_none_for_missing() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        facade.set("a", Value::Int(1)).unwrap();
        // "b" not set

        let result = facade.mget(&["a", "b"]).unwrap();
        assert_eq!(result, vec![Some(Value::Int(1)), None]);
    }

    #[test]
    fn test_mget_empty_keys() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        let result = facade.mget(&[]).unwrap();
        assert_eq!(result, vec![]);
    }

    #[test]
    fn test_mget_preserves_key_order() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        facade.set("z", Value::Int(26)).unwrap();
        facade.set("a", Value::Int(1)).unwrap();

        // Order should match input order, not alphabetical
        let result = facade.mget(&["z", "a"]).unwrap();
        assert_eq!(result, vec![Some(Value::Int(26)), Some(Value::Int(1))]);
    }

    // === mset() Tests ===

    #[test]
    fn test_mset_sets_multiple_keys() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        facade.mset(&[
            ("a", Value::Int(1)),
            ("b", Value::Int(2)),
            ("c", Value::Int(3)),
        ]).unwrap();

        assert_eq!(facade.get("a").unwrap(), Some(Value::Int(1)));
        assert_eq!(facade.get("b").unwrap(), Some(Value::Int(2)));
        assert_eq!(facade.get("c").unwrap(), Some(Value::Int(3)));
    }

    #[test]
    fn test_mset_atomic_on_error() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        // One invalid key should fail the whole batch
        let result = facade.mset(&[
            ("a", Value::Int(1)),
            ("", Value::Int(2)), // Invalid empty key
            ("c", Value::Int(3)),
        ]);

        assert!(result.is_err());

        // None should be set (atomic failure)
        assert_eq!(facade.get("a").unwrap(), None);
        assert_eq!(facade.get("c").unwrap(), None);
    }

    // === delete() Tests ===

    #[test]
    fn test_delete_returns_count() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        facade.set("a", Value::Int(1)).unwrap();
        facade.set("b", Value::Int(2)).unwrap();

        let count = facade.delete(&["a", "b", "nonexistent"]).unwrap();
        assert_eq!(count, 2); // Only 2 actually deleted
    }

    #[test]
    fn test_delete_removes_keys() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        facade.set("key", Value::Int(1)).unwrap();
        facade.delete(&["key"]).unwrap();

        assert_eq!(facade.get("key").unwrap(), None);
    }

    #[test]
    fn test_delete_nonexistent_returns_zero() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        let count = facade.delete(&["nonexistent"]).unwrap();
        assert_eq!(count, 0);
    }

    // === exists() Tests ===

    #[test]
    fn test_exists_returns_true_for_existing() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        facade.set("key", Value::Int(1)).unwrap();

        assert!(facade.exists("key").unwrap());
    }

    #[test]
    fn test_exists_returns_false_for_missing() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        assert!(!facade.exists("nonexistent").unwrap());
    }

    #[test]
    fn test_exists_after_delete() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        facade.set("key", Value::Int(1)).unwrap();
        assert!(facade.exists("key").unwrap());

        facade.delete(&["key"]).unwrap();
        assert!(!facade.exists("key").unwrap());
    }
}
```

### Implementation

```rust
impl Facade {
    /// Get multiple values by keys
    ///
    /// Desugars to: keys.map(|k| kv_get(DEFAULT_RUN, k).map(|v| v.value))
    ///
    /// Returns values in the same order as keys. Missing keys return None.
    pub fn mget(&self, keys: &[&str]) -> Result<Vec<Option<Value>>, FacadeError> {
        let mut results = Vec::with_capacity(keys.len());
        for key in keys {
            validate_key(key)?;
            let value = self.substrate.kv_get(DEFAULT_RUN, key)?
                .map(|v| v.value);
            results.push(value);
        }
        Ok(results)
    }

    /// Set multiple key-value pairs atomically
    ///
    /// Desugars to: begin_txn(); pairs.for_each(kv_put); commit()
    ///
    /// All-or-nothing: if any key is invalid, none are set.
    pub fn mset(&self, pairs: &[(&str, Value)]) -> Result<(), FacadeError> {
        // Validate all keys first
        for (key, _) in pairs {
            validate_key(key)?;
        }

        // All keys valid - perform batch write
        self.substrate.with_transaction(|txn| {
            for (key, value) in pairs {
                txn.kv_put(DEFAULT_RUN, key, value.clone())?;
            }
            Ok(())
        })?;

        Ok(())
    }

    /// Delete keys
    ///
    /// Desugars to: keys.map(kv_delete).filter(|r| r.deleted).count()
    ///
    /// Returns the count of keys that were actually deleted.
    pub fn delete(&self, keys: &[&str]) -> Result<u64, FacadeError> {
        let mut count = 0;
        for key in keys {
            validate_key(key)?;
            if self.substrate.kv_delete(DEFAULT_RUN, key)? {
                count += 1;
            }
        }
        Ok(count)
    }

    /// Check if a key exists
    ///
    /// Desugars to: kv_get(DEFAULT_RUN, key).is_some()
    pub fn exists(&self, key: &str) -> Result<bool, FacadeError> {
        validate_key(key)?;
        let result = self.substrate.kv_get(DEFAULT_RUN, key)?;
        Ok(result.is_some())
    }
}
```

### Acceptance Criteria

- [ ] `mget()` returns values in input order
- [ ] `mget()` returns `None` for missing keys
- [ ] `mset()` is atomic (all-or-nothing)
- [ ] `delete()` returns count of deleted keys
- [ ] `delete()` returns 0 for nonexistent keys
- [ ] `exists()` returns bool

---

## Story #568: KV Numeric Operations

**File**: `crates/api/src/facade/kv.rs`

**Deliverable**: incr, incrby operations with type checking

### Tests FIRST

```rust
#[cfg(test)]
mod kv_numeric_tests {
    use super::*;

    // === incr() Tests ===

    #[test]
    fn test_incr_increments_int() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        facade.set("counter", Value::Int(10)).unwrap();

        let result = facade.incr("counter").unwrap();
        assert_eq!(result, 11);

        let value = facade.get("counter").unwrap();
        assert_eq!(value, Some(Value::Int(11)));
    }

    #[test]
    fn test_incr_creates_with_one() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        let result = facade.incr("newkey").unwrap();
        assert_eq!(result, 1);

        let value = facade.get("newkey").unwrap();
        assert_eq!(value, Some(Value::Int(1)));
    }

    #[test]
    fn test_incr_on_non_int_fails() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        facade.set("key", Value::String("hello".into())).unwrap();

        let result = facade.incr("key");
        assert!(matches!(result, Err(FacadeError::WrongType { .. })));
    }

    #[test]
    fn test_incr_on_float_fails() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        // CRITICAL: Float(1.0) is NOT Int - no coercion
        facade.set("key", Value::Float(1.0)).unwrap();

        let result = facade.incr("key");
        assert!(matches!(result, Err(FacadeError::WrongType { .. })));
    }

    #[test]
    fn test_incr_overflow_fails() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        facade.set("key", Value::Int(i64::MAX)).unwrap();

        let result = facade.incr("key");
        assert!(matches!(result, Err(FacadeError::Overflow)));
    }

    // === incrby() Tests ===

    #[test]
    fn test_incrby_positive() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        facade.set("key", Value::Int(10)).unwrap();

        let result = facade.incrby("key", 5).unwrap();
        assert_eq!(result, 15);
    }

    #[test]
    fn test_incrby_negative_decrements() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        facade.set("key", Value::Int(10)).unwrap();

        let result = facade.incrby("key", -3).unwrap();
        assert_eq!(result, 7);
    }

    #[test]
    fn test_incrby_creates_with_delta() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        let result = facade.incrby("newkey", 100).unwrap();
        assert_eq!(result, 100);
    }

    #[test]
    fn test_incrby_underflow_fails() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        facade.set("key", Value::Int(i64::MIN)).unwrap();

        let result = facade.incrby("key", -1);
        assert!(matches!(result, Err(FacadeError::Overflow)));
    }
}
```

### Implementation

```rust
impl Facade {
    /// Increment integer value by 1
    ///
    /// Desugars to: kv_incr(DEFAULT_RUN, key, 1)
    ///
    /// Creates key with value 1 if not exists.
    /// Fails with WrongType if value is not Int.
    pub fn incr(&self, key: &str) -> Result<i64, FacadeError> {
        self.incrby(key, 1)
    }

    /// Increment integer value by delta
    ///
    /// Desugars to: kv_incr(DEFAULT_RUN, key, delta)
    ///
    /// Creates key with value delta if not exists.
    /// Fails with WrongType if value is not Int.
    /// Fails with Overflow on i64 overflow/underflow.
    pub fn incrby(&self, key: &str, delta: i64) -> Result<i64, FacadeError> {
        validate_key(key)?;

        // Get current value
        let current = self.substrate.kv_get(DEFAULT_RUN, key)?;

        let new_value = match current {
            None => delta, // Create with delta
            Some(versioned) => {
                match versioned.value {
                    Value::Int(i) => {
                        // Check for overflow
                        i.checked_add(delta)
                            .ok_or(FacadeError::Overflow)?
                    }
                    other => {
                        return Err(FacadeError::WrongType {
                            expected: "Int",
                            actual: other.type_name(),
                        });
                    }
                }
            }
        };

        // Write new value
        self.substrate.kv_put(DEFAULT_RUN, key, Value::Int(new_value))?;

        Ok(new_value)
    }
}
```

### Acceptance Criteria

- [ ] `incr()` increments by 1
- [ ] `incr()` creates new key with value 1
- [ ] `incrby()` increments by delta
- [ ] `incrby()` with negative delta decrements
- [ ] WrongType error for non-Int values
- [ ] WrongType error for Float (no coercion!)
- [ ] Overflow error on i64 overflow/underflow

---

## Story #569: JSON Operations

**File**: `crates/api/src/facade/json.rs` (NEW)

**Deliverable**: JSON document operations with JSONPath

### Tests FIRST

```rust
#[cfg(test)]
mod json_tests {
    use super::*;

    // === json_set() Tests ===

    #[test]
    fn test_json_set_creates_document() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        facade.json_set("doc", "$", Value::Object({
            let mut m = HashMap::new();
            m.insert("name".to_string(), Value::String("test".into()));
            m
        })).unwrap();

        let doc = facade.json_get("doc", "$").unwrap();
        assert!(doc.is_some());
    }

    #[test]
    fn test_json_set_nested_path() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        // Create document
        facade.json_set("doc", "$", Value::Object(HashMap::new())).unwrap();

        // Set nested value
        facade.json_set("doc", "$.user.name", Value::String("Alice".into())).unwrap();

        let name = facade.json_get("doc", "$.user.name").unwrap();
        assert_eq!(name, Some(Value::String("Alice".into())));
    }

    #[test]
    fn test_json_set_root_must_be_object() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        // Root must be object, not scalar
        let result = facade.json_set("doc", "$", Value::Int(42));
        assert!(matches!(result, Err(FacadeError::ConstraintViolation { reason })
            if reason == "root_not_object"));
    }

    // === json_get() Tests ===

    #[test]
    fn test_json_get_root() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        let doc = Value::Object({
            let mut m = HashMap::new();
            m.insert("a".to_string(), Value::Int(1));
            m
        });
        facade.json_set("doc", "$", doc.clone()).unwrap();

        let result = facade.json_get("doc", "$").unwrap();
        assert_eq!(result, Some(doc));
    }

    #[test]
    fn test_json_get_nested() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        let doc = Value::Object({
            let mut m = HashMap::new();
            m.insert("nested".to_string(), Value::Object({
                let mut inner = HashMap::new();
                inner.insert("value".to_string(), Value::Int(42));
                inner
            }));
            m
        });
        facade.json_set("doc", "$", doc).unwrap();

        let result = facade.json_get("doc", "$.nested.value").unwrap();
        assert_eq!(result, Some(Value::Int(42)));
    }

    #[test]
    fn test_json_get_missing_path() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        facade.json_set("doc", "$", Value::Object(HashMap::new())).unwrap();

        let result = facade.json_get("doc", "$.nonexistent").unwrap();
        assert_eq!(result, None);
    }

    // === json_delete() Tests ===

    #[test]
    fn test_json_delete_removes_path() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        let doc = Value::Object({
            let mut m = HashMap::new();
            m.insert("keep".to_string(), Value::Int(1));
            m.insert("remove".to_string(), Value::Int(2));
            m
        });
        facade.json_set("doc", "$", doc).unwrap();

        let deleted = facade.json_delete("doc", "$.remove").unwrap();
        assert_eq!(deleted, 1);

        assert_eq!(facade.json_get("doc", "$.keep").unwrap(), Some(Value::Int(1)));
        assert_eq!(facade.json_get("doc", "$.remove").unwrap(), None);
    }

    #[test]
    fn test_json_delete_nonexistent_returns_zero() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        facade.json_set("doc", "$", Value::Object(HashMap::new())).unwrap();

        let deleted = facade.json_delete("doc", "$.nonexistent").unwrap();
        assert_eq!(deleted, 0);
    }
}
```

### Implementation

```rust
impl Facade {
    /// Set JSON document or path
    ///
    /// Desugars to: json_set(DEFAULT_RUN, key, path, value)
    ///
    /// If path is "$", creates/replaces entire document (must be Object).
    /// If path is nested, creates intermediate objects as needed.
    pub fn json_set(&self, key: &str, path: &str, value: Value) -> Result<(), FacadeError> {
        validate_key(key)?;

        // Root path must be object
        if path == "$" {
            if !matches!(value, Value::Object(_)) {
                return Err(FacadeError::ConstraintViolation {
                    reason: "root_not_object".to_string(),
                });
            }
        }

        self.substrate.json_set(DEFAULT_RUN, key, path, value)?;
        Ok(())
    }

    /// Get JSON value at path
    ///
    /// Desugars to: json_get(DEFAULT_RUN, key, path)
    ///
    /// Returns None if document doesn't exist or path not found.
    pub fn json_get(&self, key: &str, path: &str) -> Result<Option<Value>, FacadeError> {
        validate_key(key)?;
        self.substrate.json_get(DEFAULT_RUN, key, path).map_err(Into::into)
    }

    /// Delete JSON value at path
    ///
    /// Desugars to: json_delete(DEFAULT_RUN, key, path)
    ///
    /// Returns count of deleted paths (0 or 1).
    pub fn json_delete(&self, key: &str, path: &str) -> Result<u64, FacadeError> {
        validate_key(key)?;
        self.substrate.json_delete(DEFAULT_RUN, key, path).map_err(Into::into)
    }
}
```

### Acceptance Criteria

- [ ] `json_set()` creates documents at root
- [ ] `json_set()` requires Object at root path
- [ ] `json_set()` creates nested paths
- [ ] `json_get()` retrieves values at paths
- [ ] `json_get()` returns None for missing paths
- [ ] `json_delete()` removes paths
- [ ] `json_delete()` returns deletion count

---

## Story #570: Event Operations

**File**: `crates/api/src/facade/event.rs` (NEW)

**Deliverable**: Event log operations (xadd, xrange, xlen)

### Tests FIRST

```rust
#[cfg(test)]
mod event_tests {
    use super::*;

    #[test]
    fn test_xadd_returns_version() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        let version = facade.xadd("stream", Value::Object({
            let mut m = HashMap::new();
            m.insert("type".to_string(), Value::String("test".into()));
            m
        })).unwrap();

        assert!(version.value() > 0);
    }

    #[test]
    fn test_xadd_versions_are_sequential() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        let v1 = facade.xadd("stream", Value::Int(1)).unwrap();
        let v2 = facade.xadd("stream", Value::Int(2)).unwrap();
        let v3 = facade.xadd("stream", Value::Int(3)).unwrap();

        assert!(v1.value() < v2.value());
        assert!(v2.value() < v3.value());
    }

    #[test]
    fn test_xrange_returns_events_in_order() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        facade.xadd("stream", Value::Int(1)).unwrap();
        facade.xadd("stream", Value::Int(2)).unwrap();
        facade.xadd("stream", Value::Int(3)).unwrap();

        let events = facade.xrange("stream", None, None, None).unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].value, Value::Int(1));
        assert_eq!(events[1].value, Value::Int(2));
        assert_eq!(events[2].value, Value::Int(3));
    }

    #[test]
    fn test_xrange_with_limit() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        for i in 1..=10 {
            facade.xadd("stream", Value::Int(i)).unwrap();
        }

        let events = facade.xrange("stream", None, None, Some(3)).unwrap();
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn test_xlen() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        assert_eq!(facade.xlen("stream").unwrap(), 0);

        facade.xadd("stream", Value::Int(1)).unwrap();
        assert_eq!(facade.xlen("stream").unwrap(), 1);

        facade.xadd("stream", Value::Int(2)).unwrap();
        assert_eq!(facade.xlen("stream").unwrap(), 2);
    }
}
```

### Acceptance Criteria

- [ ] `xadd()` appends event and returns version
- [ ] Event versions are sequential
- [ ] `xrange()` returns events in order
- [ ] `xrange()` respects limit parameter
- [ ] `xlen()` returns event count

---

## Story #571: Vector Operations

**File**: `crates/api/src/facade/vector.rs` (NEW)

**Deliverable**: Vector store operations (vset, vget, vdel, vsearch)

### Tests FIRST

```rust
#[cfg(test)]
mod vector_tests {
    use super::*;

    #[test]
    fn test_vset_stores_vector() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        facade.vset("doc1", vec![0.1, 0.2, 0.3], Value::Object({
            let mut m = HashMap::new();
            m.insert("tag".to_string(), Value::String("test".into()));
            m
        })).unwrap();

        let result = facade.vget("doc1").unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_vget_returns_versioned() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        let metadata = Value::Object({
            let mut m = HashMap::new();
            m.insert("key".to_string(), Value::String("value".into()));
            m
        });
        facade.vset("doc1", vec![1.0, 2.0], metadata.clone()).unwrap();

        let result = facade.vget("doc1").unwrap().unwrap();
        assert_eq!(result.value, metadata);
    }

    #[test]
    fn test_vdel_removes_vector() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        facade.vset("doc1", vec![1.0], Value::Null).unwrap();
        assert!(facade.vget("doc1").unwrap().is_some());

        let deleted = facade.vdel("doc1").unwrap();
        assert_eq!(deleted, 1);

        assert!(facade.vget("doc1").unwrap().is_none());
    }

    #[test]
    fn test_vdel_nonexistent_returns_zero() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        let deleted = facade.vdel("nonexistent").unwrap();
        assert_eq!(deleted, 0);
    }

    #[test]
    fn test_vector_dimension_mismatch() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        facade.vset("doc1", vec![1.0, 2.0, 3.0], Value::Null).unwrap();

        // Different dimension should fail in search
        let result = facade.vsearch(vec![1.0, 2.0], 10); // 2D vs 3D
        assert!(matches!(result, Err(FacadeError::ConstraintViolation { reason })
            if reason == "vector_dim_mismatch"));
    }
}
```

### Acceptance Criteria

- [ ] `vset()` stores vector with metadata
- [ ] `vget()` returns versioned metadata
- [ ] `vdel()` removes vector
- [ ] `vsearch()` finds similar vectors
- [ ] Dimension mismatch produces error

---

## Story #572: State (CAS) Operations

**File**: `crates/api/src/facade/state.rs` (NEW)

**Deliverable**: Compare-and-swap operations

### Tests FIRST

```rust
#[cfg(test)]
mod cas_tests {
    use super::*;

    #[test]
    fn test_cas_set_creates_new() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        // expected=null means "must not exist"
        let success = facade.cas_set("key", Value::Null, Value::Int(1)).unwrap();
        assert!(success);

        assert_eq!(facade.cas_get("key").unwrap(), Some(Value::Int(1)));
    }

    #[test]
    fn test_cas_set_fails_if_exists() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        facade.cas_set("key", Value::Null, Value::Int(1)).unwrap();

        // Should fail - key already exists
        let success = facade.cas_set("key", Value::Null, Value::Int(2)).unwrap();
        assert!(!success);

        // Value unchanged
        assert_eq!(facade.cas_get("key").unwrap(), Some(Value::Int(1)));
    }

    #[test]
    fn test_cas_set_updates_with_correct_expected() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        facade.cas_set("key", Value::Null, Value::Int(1)).unwrap();

        // Update with correct expected value
        let success = facade.cas_set("key", Value::Int(1), Value::Int(2)).unwrap();
        assert!(success);

        assert_eq!(facade.cas_get("key").unwrap(), Some(Value::Int(2)));
    }

    #[test]
    fn test_cas_set_fails_with_wrong_expected() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        facade.cas_set("key", Value::Null, Value::Int(1)).unwrap();

        // Wrong expected value
        let success = facade.cas_set("key", Value::Int(999), Value::Int(2)).unwrap();
        assert!(!success);

        // Value unchanged
        assert_eq!(facade.cas_get("key").unwrap(), Some(Value::Int(1)));
    }

    #[test]
    fn test_cas_respects_type_distinction() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        // Set Int(1)
        facade.cas_set("key", Value::Null, Value::Int(1)).unwrap();

        // CRITICAL: Float(1.0) should NOT match Int(1)
        let success = facade.cas_set("key", Value::Float(1.0), Value::Int(2)).unwrap();
        assert!(!success); // Must fail - types differ

        // Value unchanged
        assert_eq!(facade.cas_get("key").unwrap(), Some(Value::Int(1)));
    }

    #[test]
    fn test_cas_get_returns_value() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        facade.cas_set("key", Value::Null, Value::Int(42)).unwrap();

        let value = facade.cas_get("key").unwrap();
        assert_eq!(value, Some(Value::Int(42)));
    }

    #[test]
    fn test_cas_get_missing_returns_none() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        let value = facade.cas_get("nonexistent").unwrap();
        assert_eq!(value, None);
    }
}
```

### Implementation

```rust
impl Facade {
    /// Compare-and-swap set
    ///
    /// Desugars to: cas_set(DEFAULT_RUN, key, expected, new)
    ///
    /// - If expected is Null: create only if key doesn't exist
    /// - Otherwise: update only if current value equals expected (including type!)
    ///
    /// Returns true if swap succeeded, false if precondition failed.
    pub fn cas_set(&self, key: &str, expected: Value, new: Value) -> Result<bool, FacadeError> {
        validate_key(key)?;
        self.substrate.cas_set(DEFAULT_RUN, key, expected, new).map_err(Into::into)
    }

    /// Get value from state cell
    ///
    /// Desugars to: cas_get(DEFAULT_RUN, key)
    pub fn cas_get(&self, key: &str) -> Result<Option<Value>, FacadeError> {
        validate_key(key)?;
        self.substrate.cas_get(DEFAULT_RUN, key).map_err(Into::into)
    }
}
```

### Acceptance Criteria

- [ ] `cas_set()` creates when expected is Null and key missing
- [ ] `cas_set()` fails when expected is Null but key exists
- [ ] `cas_set()` updates when expected matches current
- [ ] `cas_set()` fails when expected doesn't match
- [ ] **CRITICAL**: Type matters - Int(1) != Float(1.0) in CAS
- [ ] `cas_get()` returns current value

---

## Story #573: History Operations

**File**: `crates/api/src/facade/history.rs` (NEW)

**Deliverable**: History access operations

### Tests FIRST

```rust
#[cfg(test)]
mod history_tests {
    use super::*;

    #[test]
    fn test_history_returns_versions() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        facade.set("key", Value::Int(1)).unwrap();
        facade.set("key", Value::Int(2)).unwrap();
        facade.set("key", Value::Int(3)).unwrap();

        let history = facade.history("key", None).unwrap();
        assert_eq!(history.len(), 3);
    }

    #[test]
    fn test_history_respects_limit() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        for i in 1..=10 {
            facade.set("key", Value::Int(i)).unwrap();
        }

        let history = facade.history("key", Some(3)).unwrap();
        assert_eq!(history.len(), 3);
    }

    #[test]
    fn test_history_newest_first() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        facade.set("key", Value::Int(1)).unwrap();
        facade.set("key", Value::Int(2)).unwrap();

        let history = facade.history("key", None).unwrap();
        // Newest first
        assert_eq!(history[0].value, Value::Int(2));
        assert_eq!(history[1].value, Value::Int(1));
    }

    #[test]
    fn test_history_missing_key() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        let history = facade.history("nonexistent", None).unwrap();
        assert!(history.is_empty());
    }
}
```

### Acceptance Criteria

- [ ] `history()` returns version history
- [ ] `history()` respects limit parameter
- [ ] History is newest-first order
- [ ] Missing key returns empty history

---

## Testing

Integration tests verifying facade-substrate parity:

```rust
#[cfg(test)]
mod facade_substrate_parity_tests {
    use super::*;

    /// Every facade operation must produce identical results to
    /// the equivalent substrate operation sequence.

    #[test]
    fn test_set_get_parity() {
        let harness = TestHarness::new();
        let facade = harness.facade();
        let substrate = harness.substrate();

        // Facade path
        facade.set("key1", Value::Int(42)).unwrap();
        let facade_result = facade.get("key1").unwrap();

        // Substrate path (manual desugaring)
        substrate.kv_put("default", "key2", Value::Int(42)).unwrap();
        let substrate_result = substrate.kv_get("default", "key2").unwrap()
            .map(|v| v.value);

        // Results must be identical
        assert_eq!(facade_result, substrate_result);
    }

    #[test]
    fn test_error_propagation() {
        let harness = TestHarness::new();
        let facade = harness.facade();

        // Invalid key should produce same error from facade
        let result = facade.set("_strata/forbidden", Value::Int(1));
        assert!(matches!(result, Err(FacadeError::InvalidKey(_))));

        // Facade must not swallow or transform errors
    }
}
```

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/api/src/facade/mod.rs` | CREATE - Facade module |
| `crates/api/src/facade/kv.rs` | CREATE - KV operations |
| `crates/api/src/facade/json.rs` | CREATE - JSON operations |
| `crates/api/src/facade/event.rs` | CREATE - Event operations |
| `crates/api/src/facade/vector.rs` | CREATE - Vector operations |
| `crates/api/src/facade/state.rs` | CREATE - CAS operations |
| `crates/api/src/facade/history.rs` | CREATE - History operations |
| `crates/api/src/lib.rs` | MODIFY - Export facade module |

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-21 | Initial epic specification |
