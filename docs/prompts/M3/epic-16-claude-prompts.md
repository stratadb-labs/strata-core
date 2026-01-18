# Epic 16: StateCell Primitive - Implementation Prompts

**Epic Goal**: CAS-based versioned cells for coordination.

**GitHub Issue**: [#162](https://github.com/anibjoshi/in-mem/issues/162)
**Status**: Ready to begin (after Epic 13)
**Dependencies**: Epic 13 (Primitives Foundation) complete

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M3_ARCHITECTURE.md` is the GOSPEL for ALL M3 implementation.**

Before starting ANY story in this epic, read:
- Section 6: StateCell Primitive
- Section 6.4: Purity Requirement (CRITICAL)
- Section 12: Invariant Enforcement

See `docs/prompts/M3_PROMPT_HEADER.md` for complete guidelines.

---

## Epic 16 Overview

### Critical Design Decision: Purity Requirement

**Closures passed to `transition()` MUST be pure functions.**

The `transition()` method may execute its closure multiple times due to OCC retries:

| Requirement | Explanation |
|-------------|-------------|
| Pure function of inputs | Closure result depends only on `&State` |
| No I/O | No file, network, console operations |
| No external mutation | Don't modify variables outside closure |
| No irreversible effects | No logging, metrics, API calls |
| Idempotent | Same input produces same output |

**WRONG:**
```rust
sc.transition(run_id, "counter", |state| {
    println!("Incrementing");  // WRONG: I/O
    external_var += 1;         // WRONG: External mutation
    Ok(...)
})?;
```

**CORRECT:**
```rust
sc.transition(run_id, "counter", |state| {
    let current = state.value.as_i64()?;
    Ok((Value::I64(current + 1), current + 1))  // Pure computation
})?;
```

### Scope
- StateCell struct as stateless facade
- State structure with value, version, updated_at
- Init, read, CAS, set, delete operations
- Transition closure pattern with automatic retry
- StateCellExt transaction extension

### Success Criteria
- [ ] StateCell struct implemented with `Arc<Database>` reference
- [ ] State struct with value, version, updated_at fields
- [ ] `init()` creates cell only if not exists
- [ ] `read()` returns current state with version
- [ ] `cas()` atomically updates only if version matches
- [ ] `set()` unconditionally updates (force write)
- [ ] `delete()` removes cell
- [ ] `transition()` closure pattern with automatic OCC retry
- [ ] Version monotonicity enforced
- [ ] StateCellExt transaction extension trait
- [ ] All unit tests pass (>95% coverage)

### Component Breakdown
- **Story #180**: StateCell Core & State Structure - BLOCKS others
- **Story #181**: StateCell Read/Init/Delete Operations
- **Story #182**: StateCell CAS & Set Operations
- **Story #183**: StateCell Transition Closure Pattern
- **Story #184**: StateCellExt Transaction Extension

---

## Dependency Graph

```
Phase 1 (Sequential):
  Story #180 (StateCell Core)
    └─> BLOCKS #181, #182, #183

Phase 2 (Parallel - 3 Claudes after #180):
  Story #181 (Read/Init/Delete)
  Story #182 (CAS & Set)
  Story #183 (Transition)
    └─> All depend on #180
    └─> Independent of each other

Phase 3 (Sequential):
  Story #184 (StateCellExt)
    └─> Depends on all previous stories
```

---

## Story #180: StateCell Core & State Structure

**GitHub Issue**: [#180](https://github.com/anibjoshi/in-mem/issues/180)
**Estimated Time**: 3 hours
**Dependencies**: Epic 13 complete
**Blocks**: Stories #181, #182, #183

### Start Story

```bash
/opt/homebrew/bin/gh issue view 180
./scripts/start-story.sh 16 180 statecell-core
```

### Implementation

Create `crates/primitives/src/state_cell.rs`:

```rust
//! StateCell: CAS-based versioned cells for coordination
//!
//! ## Design Principles
//!
//! 1. **Versioned Updates**: Every update increments the version.
//! 2. **CAS Semantics**: Compare-and-swap ensures safe concurrent updates.
//! 3. **Purity Requirement**: Transition closures MUST be pure functions.
//!
//! ## Naming Note
//!
//! This is "StateCell" not "StateMachine". In M3, this is a simple CAS cell.
//! Full state machine semantics (transitions, guards) may come in M5+.

use std::sync::Arc;
use serde::{Serialize, Deserialize};
use in_mem_engine::Database;
use in_mem_core::{Key, Namespace, RunId, Value, Result, Error};

/// Current state of a cell
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct State {
    /// Current value
    pub value: Value,
    /// Version number (monotonically increasing)
    pub version: u64,
    /// Last update timestamp (milliseconds since epoch)
    pub updated_at: i64,
}

impl State {
    /// Create a new state with version 1
    pub fn new(value: Value) -> Self {
        Self {
            value,
            version: 1,
            updated_at: Self::now(),
        }
    }

    /// Get current timestamp
    fn now() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64
    }
}

/// CAS-based versioned cells for coordination
///
/// DESIGN: Each cell has a value and monotonically increasing version.
/// Updates via CAS ensure safe concurrent access.
#[derive(Clone)]
pub struct StateCell {
    db: Arc<Database>,
}

impl StateCell {
    /// Create new StateCell instance
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Get the underlying database reference
    pub fn database(&self) -> &Arc<Database> {
        &self.db
    }

    /// Build namespace for run-scoped operations
    fn namespace_for_run(&self, run_id: &RunId) -> Namespace {
        Namespace::for_run(run_id)
    }

    /// Build key for state cell
    fn key_for(&self, run_id: &RunId, name: &str) -> Key {
        Key::new_state(self.namespace_for_run(run_id), name)
    }
}
```

### Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_creation() {
        let state = State::new(Value::I64(42));
        assert_eq!(state.version, 1);
        assert!(state.updated_at > 0);
    }

    #[test]
    fn test_state_serialization() {
        let state = State::new(Value::String("test".into()));
        let json = serde_json::to_string(&state).unwrap();
        let restored: State = serde_json::from_str(&json).unwrap();
        assert_eq!(state.value, restored.value);
        assert_eq!(state.version, restored.version);
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 180
```

---

## Story #181: StateCell Read/Init/Delete Operations

**GitHub Issue**: [#181](https://github.com/anibjoshi/in-mem/issues/181)
**Estimated Time**: 3 hours
**Dependencies**: Story #180

### Implementation

```rust
impl StateCell {
    /// Initialize a cell with a value (only if it doesn't exist)
    ///
    /// Returns Ok(version) if created, Err if already exists.
    pub fn init(&self, run_id: &RunId, name: &str, value: Value) -> Result<u64> {
        self.db.transaction(run_id, |txn| {
            let key = self.key_for(run_id, name);

            // Check if exists
            if txn.get(&key)?.is_some() {
                return Err(Error::AlreadyExists(format!("StateCell '{}' already exists", name)));
            }

            // Create new state
            let state = State::new(value);
            txn.put(key, Value::from_json(serde_json::to_value(&state)?)?)?;
            Ok(state.version)
        })
    }

    /// Read current state
    pub fn read(&self, run_id: &RunId, name: &str) -> Result<Option<State>> {
        self.db.transaction(run_id, |txn| {
            let key = self.key_for(run_id, name);
            match txn.get(&key)? {
                Some(v) => Ok(Some(serde_json::from_value(v.into_json()?)?)),
                None => Ok(None),
            }
        })
    }

    /// Delete a cell
    pub fn delete(&self, run_id: &RunId, name: &str) -> Result<bool> {
        self.db.transaction(run_id, |txn| {
            let key = self.key_for(run_id, name);
            txn.delete(&key)
        })
    }

    /// Check if a cell exists
    pub fn exists(&self, run_id: &RunId, name: &str) -> Result<bool> {
        Ok(self.read(run_id, name)?.is_some())
    }

    /// List all cell names in run
    pub fn list(&self, run_id: &RunId) -> Result<Vec<String>> {
        self.db.transaction(run_id, |txn| {
            let ns = self.namespace_for_run(run_id);
            let prefix = Key::new_state(ns, "");
            let results = txn.scan_prefix(&prefix)?;
            Ok(results
                .into_iter()
                .filter_map(|(key, _)| key.user_key_string())
                .collect())
        })
    }
}
```

### Tests

```rust
#[test]
fn test_init_and_read() {
    let (_temp, db, sc) = setup();
    let run_id = RunId::new();
    db.begin_run(&run_id).unwrap();

    sc.init(&run_id, "counter", Value::I64(0)).unwrap();
    let state = sc.read(&run_id, "counter").unwrap().unwrap();
    assert_eq!(state.value, Value::I64(0));
    assert_eq!(state.version, 1);
}

#[test]
fn test_init_already_exists() {
    let (_temp, db, sc) = setup();
    let run_id = RunId::new();
    db.begin_run(&run_id).unwrap();

    sc.init(&run_id, "cell", Value::Null).unwrap();
    let result = sc.init(&run_id, "cell", Value::Null);
    assert!(result.is_err());
}

#[test]
fn test_delete() {
    let (_temp, db, sc) = setup();
    let run_id = RunId::new();
    db.begin_run(&run_id).unwrap();

    sc.init(&run_id, "temp", Value::Null).unwrap();
    assert!(sc.exists(&run_id, "temp").unwrap());

    sc.delete(&run_id, "temp").unwrap();
    assert!(!sc.exists(&run_id, "temp").unwrap());
}
```

### Complete Story

```bash
./scripts/complete-story.sh 181
```

---

## Story #182: StateCell CAS & Set Operations

**GitHub Issue**: [#182](https://github.com/anibjoshi/in-mem/issues/182)
**Estimated Time**: 4 hours
**Dependencies**: Story #180

### Implementation

```rust
impl StateCell {
    /// Compare-and-swap: Update only if version matches
    ///
    /// Returns new version on success, error on conflict.
    pub fn cas(
        &self,
        run_id: &RunId,
        name: &str,
        expected_version: u64,
        new_value: Value,
    ) -> Result<u64> {
        self.db.transaction(run_id, |txn| {
            let key = self.key_for(run_id, name);

            let current: State = match txn.get(&key)? {
                Some(v) => serde_json::from_value(v.into_json()?)?,
                None => return Err(Error::NotFound(format!("StateCell '{}' not found", name))),
            };

            if current.version != expected_version {
                return Err(Error::CASConflict {
                    expected: expected_version,
                    actual: current.version,
                });
            }

            let new_state = State {
                value: new_value,
                version: current.version + 1,
                updated_at: State::now(),
            };

            txn.put(key, Value::from_json(serde_json::to_value(&new_state)?)?)?;
            Ok(new_state.version)
        })
    }

    /// Unconditional set (force write)
    ///
    /// Always succeeds, overwrites any existing value.
    pub fn set(&self, run_id: &RunId, name: &str, value: Value) -> Result<u64> {
        self.db.transaction(run_id, |txn| {
            let key = self.key_for(run_id, name);

            let new_version = match txn.get(&key)? {
                Some(v) => {
                    let current: State = serde_json::from_value(v.into_json()?)?;
                    current.version + 1
                }
                None => 1,
            };

            let new_state = State {
                value,
                version: new_version,
                updated_at: State::now(),
            };

            txn.put(key, Value::from_json(serde_json::to_value(&new_state)?)?)?;
            Ok(new_state.version)
        })
    }
}
```

### Tests

```rust
#[test]
fn test_cas_success() {
    let (_temp, db, sc) = setup();
    let run_id = RunId::new();
    db.begin_run(&run_id).unwrap();

    sc.init(&run_id, "counter", Value::I64(0)).unwrap();

    // CAS with correct version
    let new_version = sc.cas(&run_id, "counter", 1, Value::I64(1)).unwrap();
    assert_eq!(new_version, 2);

    let state = sc.read(&run_id, "counter").unwrap().unwrap();
    assert_eq!(state.value, Value::I64(1));
}

#[test]
fn test_cas_conflict() {
    let (_temp, db, sc) = setup();
    let run_id = RunId::new();
    db.begin_run(&run_id).unwrap();

    sc.init(&run_id, "counter", Value::I64(0)).unwrap();

    // CAS with wrong version
    let result = sc.cas(&run_id, "counter", 999, Value::I64(1));
    assert!(matches!(result, Err(Error::CASConflict { .. })));
}

#[test]
fn test_set_creates_if_not_exists() {
    let (_temp, db, sc) = setup();
    let run_id = RunId::new();
    db.begin_run(&run_id).unwrap();

    let version = sc.set(&run_id, "new-cell", Value::I64(42)).unwrap();
    assert_eq!(version, 1);
}

#[test]
fn test_version_monotonicity() {
    let (_temp, db, sc) = setup();
    let run_id = RunId::new();
    db.begin_run(&run_id).unwrap();

    sc.init(&run_id, "cell", Value::I64(0)).unwrap();

    for i in 1..=10 {
        let v = sc.set(&run_id, "cell", Value::I64(i)).unwrap();
        assert_eq!(v, (i + 1) as u64);
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 182
```

---

## Story #183: StateCell Transition Closure Pattern

**GitHub Issue**: [#183](https://github.com/anibjoshi/in-mem/issues/183)
**Estimated Time**: 4 hours
**Dependencies**: Story #180

### Implementation

```rust
impl StateCell {
    /// Apply a transition function with automatic retry on conflict
    ///
    /// ## Purity Requirement
    ///
    /// The closure `f` MAY BE CALLED MULTIPLE TIMES due to OCC retries.
    /// It MUST be a pure function:
    /// - No I/O (file, network, console)
    /// - No external mutation
    /// - No irreversible effects (logging, metrics)
    /// - Idempotent (same input -> same output)
    ///
    /// ## Example
    ///
    /// ```rust,ignore
    /// sc.transition(run_id, "counter", |state| {
    ///     let current = state.value.as_i64()?;
    ///     Ok((Value::I64(current + 1), current + 1))
    /// })?;
    /// ```
    pub fn transition<F, T>(
        &self,
        run_id: &RunId,
        name: &str,
        f: F,
    ) -> Result<T>
    where
        F: Fn(&State) -> Result<(Value, T)>,
    {
        const MAX_RETRIES: usize = 10;

        for attempt in 0..MAX_RETRIES {
            // Read current state
            let current = self.read(run_id, name)?
                .ok_or_else(|| Error::NotFound(format!("StateCell '{}' not found", name)))?;

            // Compute new value (closure may be called multiple times!)
            let (new_value, result) = f(&current)?;

            // Try CAS
            match self.cas(run_id, name, current.version, new_value) {
                Ok(_) => return Ok(result),
                Err(Error::CASConflict { .. }) if attempt < MAX_RETRIES - 1 => {
                    // Retry on conflict
                    continue;
                }
                Err(e) => return Err(e),
            }
        }

        Err(Error::TooManyRetries(MAX_RETRIES))
    }

    /// Apply transition or initialize if cell doesn't exist
    pub fn transition_or_init<F, T>(
        &self,
        run_id: &RunId,
        name: &str,
        initial: Value,
        f: F,
    ) -> Result<T>
    where
        F: Fn(&State) -> Result<(Value, T)>,
    {
        // Try to init first
        let _ = self.init(run_id, name, initial.clone());

        // Then transition
        self.transition(run_id, name, f)
    }
}
```

### Tests

```rust
#[test]
fn test_transition_increment() {
    let (_temp, db, sc) = setup();
    let run_id = RunId::new();
    db.begin_run(&run_id).unwrap();

    sc.init(&run_id, "counter", Value::I64(0)).unwrap();

    let result = sc.transition(&run_id, "counter", |state| {
        let current = state.value.as_i64().unwrap();
        Ok((Value::I64(current + 1), current + 1))
    }).unwrap();

    assert_eq!(result, 1);

    let state = sc.read(&run_id, "counter").unwrap().unwrap();
    assert_eq!(state.value, Value::I64(1));
}

#[test]
fn test_transition_or_init() {
    let (_temp, db, sc) = setup();
    let run_id = RunId::new();
    db.begin_run(&run_id).unwrap();

    // Cell doesn't exist, should init then transition
    let result = sc.transition_or_init(&run_id, "new-counter", Value::I64(0), |state| {
        let current = state.value.as_i64().unwrap();
        Ok((Value::I64(current + 10), current + 10))
    }).unwrap();

    assert_eq!(result, 10);
}
```

### Complete Story

```bash
./scripts/complete-story.sh 183
```

---

## Story #184: StateCellExt Transaction Extension

**GitHub Issue**: [#184](https://github.com/anibjoshi/in-mem/issues/184)
**Estimated Time**: 3 hours
**Dependencies**: Stories #180-#183

### Implementation

```rust
use crate::extensions::StateCellExt;

impl StateCellExt for TransactionContext {
    fn state_read(&mut self, name: &str) -> Result<Option<Value>> {
        let key = Key::new_state(self.namespace().clone(), name);
        match self.get(&key)? {
            Some(v) => {
                let state: State = serde_json::from_value(v.into_json()?)?;
                Ok(Some(state.value))
            }
            None => Ok(None),
        }
    }

    fn state_cas(&mut self, name: &str, expected_version: u64, new_value: Value) -> Result<u64> {
        let key = Key::new_state(self.namespace().clone(), name);

        let current: State = match self.get(&key)? {
            Some(v) => serde_json::from_value(v.into_json()?)?,
            None => return Err(Error::NotFound(format!("StateCell '{}' not found", name))),
        };

        if current.version != expected_version {
            return Err(Error::CASConflict {
                expected: expected_version,
                actual: current.version,
            });
        }

        let new_state = State {
            value: new_value,
            version: current.version + 1,
            updated_at: State::now(),
        };

        self.put(key, Value::from_json(serde_json::to_value(&new_state)?)?)?;
        Ok(new_state.version)
    }

    fn state_set(&mut self, name: &str, value: Value) -> Result<u64> {
        let key = Key::new_state(self.namespace().clone(), name);

        let new_version = match self.get(&key)? {
            Some(v) => {
                let current: State = serde_json::from_value(v.into_json()?)?;
                current.version + 1
            }
            None => 1,
        };

        let new_state = State {
            value,
            version: new_version,
            updated_at: State::now(),
        };

        self.put(key, Value::from_json(serde_json::to_value(&new_state)?)?)?;
        Ok(new_state.version)
    }
}
```

Update `crates/primitives/src/lib.rs`:

```rust
pub mod state_cell;
pub use state_cell::{StateCell, State};
```

### Complete Story

```bash
./scripts/complete-story.sh 184
```

---

## Epic 16 Completion Checklist

### Verify Deliverables

- [ ] StateCell struct is stateless
- [ ] State struct has value, version, updated_at
- [ ] init/read/delete/exists/list work correctly
- [ ] CAS enforces version matching
- [ ] Version monotonically increases
- [ ] transition() has retry logic
- [ ] Purity requirement documented
- [ ] StateCellExt works in transactions
- [ ] All tests pass

### Merge and Close

```bash
git checkout develop
git merge --no-ff epic-16-statecell-primitive -m "Epic 16: StateCell Primitive

Complete:
- StateCell stateless facade
- State structure (value, version, updated_at)
- Init/Read/Delete operations
- CAS with version validation
- Set (unconditional)
- Transition closure pattern with retry
- StateCellExt transaction extension

IMPORTANT: Transition closures must be PURE functions.

Stories: #180, #181, #182, #183, #184
"

/opt/homebrew/bin/gh issue close 162 --comment "Epic 16: StateCell Primitive - COMPLETE"
```

---

## Summary

Epic 16 implements the StateCell primitive - CAS-based versioned cells for coordination. The critical design decision is the **purity requirement** for transition closures, which may execute multiple times.
