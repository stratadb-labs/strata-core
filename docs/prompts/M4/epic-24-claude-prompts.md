# Epic 24: Read Path Optimization - Implementation Prompts

**Epic Goal**: Bypass transaction overhead for read-only operations

**GitHub Issue**: [#215](https://github.com/anibjoshi/in-mem/issues/215)
**Status**: Ready after Epic 22
**Dependencies**: Epic 22 complete (ShardedStore with snapshot)

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M4_ARCHITECTURE.md` is the GOSPEL for ALL M4 implementation.**

See `docs/prompts/M4_PROMPT_HEADER.md` for complete guidelines.

---

## Epic 24 Overview

### Reference: Critical Invariants

See `docs/prompts/M4_PROMPT_HEADER.md` for critical invariants. **This is the MOST CRITICAL epic for invariants:**
- **Snapshot Semantic Invariant** - Every fast-path read MUST be observationally equivalent

### CRITICAL INVARIANT (NON-NEGOTIABLE)

> **Fast-path reads MUST be observationally equivalent to a snapshot-based transaction.**

This means:
- **No dirty reads** (uncommitted data)
- **No torn reads** (partial write sets)
- **No stale reads** (older than snapshot version)
- **No mixing versions** (key A at version X, key B at version Y where Y > X)

**"Latest committed at snapshot acquisition"** is the correct semantic definition. Fast-path reads return the same value a read-only transaction started at that moment would return.

**Why this matters:**
- Breaking this breaks agent reasoning guarantees
- Breaking this makes replay non-deterministic
- Breaking this prevents promoting fast-path to transaction later

Every fast-path implementation in this epic MUST maintain this invariant. Tests MUST verify observational equivalence.

### Scope
- KVStore fast path get()
- Batch get_many() operation
- Other primitive fast paths (EventLog, StateCell, TraceStore)
- Observational equivalence verification

### Success Criteria
- [ ] KVStore.get() bypasses full transaction
- [ ] KVStore.get() uses direct snapshot read
- [ ] KVStore.get() < 10µs (target: <5µs)
- [ ] get_many() uses single snapshot for batch
- [ ] EventLog.read() and len() have fast paths
- [ ] StateCell.read() has fast path
- [ ] TraceStore.get() has fast path
- [ ] **INVARIANT**: All fast paths observationally equivalent to transaction reads
- [ ] **INVARIANT**: No dirty reads, stale reads, or torn reads
- [ ] **INVARIANT**: No version mixing across keys in same snapshot

### Component Breakdown
- **Story #216 (GitHub #236)**: KVStore Fast Path Get - FOUNDATION
- **Story #217 (GitHub #237)**: KVStore Fast Path Batch Get
- **Story #218 (GitHub #238)**: Other Primitive Fast Paths
- **Story #219 (GitHub #239)**: Observational Equivalence Tests

---

## Dependency Graph

```
Story #236 (KV Fast Get) ──┬──> Story #237 (Batch Get)
                          └──> Story #238 (Other Primitives)
                                    └──> Story #239 (Equivalence Tests)
```

---

## Story #236: KVStore Fast Path Get

**GitHub Issue**: [#236](https://github.com/anibjoshi/in-mem/issues/236)
**Estimated Time**: 3 hours
**Dependencies**: Epic 22 complete

### CRITICAL INVARIANT

> **This implementation MUST be observationally equivalent to a transaction-based read.**

The fast path get() must return the same value that a read-only transaction started at the same moment would return. No dirty reads, no stale reads, no torn reads, no mixing versions.

### Start Story

```bash
gh issue view 236
./scripts/start-story.sh 24 236 kv-fast-get
```

### Implementation

Update `crates/primitives/src/kv.rs`:

```rust
impl KVStore {
    /// Get a value by key (FAST PATH)
    ///
    /// Bypasses full transaction overhead:
    /// - No transaction object allocation
    /// - No read-set recording
    /// - No commit validation
    /// - No WAL append
    ///
    /// PRESERVES:
    /// - Snapshot isolation (consistent view)
    /// - Run isolation (key prefixing)
    ///
    /// # Performance Contract
    /// - < 10µs (target: <5µs)
    /// - Zero allocations (except return value clone)
    ///
    /// # Invariant
    /// Observationally equivalent to transaction-based read.
    pub fn get(&self, run_id: RunId, key: &str) -> Result<Option<Value>> {
        // Fast path: direct snapshot read
        let snapshot = self.db.snapshot();
        let storage_key = Key::new_kv(run_id.namespace(), key);

        Ok(snapshot.get(&run_id, &storage_key).map(|v| v.value.clone()))
    }

    /// Get with full transaction (for comparison/fallback)
    ///
    /// Use this when you need transaction semantics (e.g., read-modify-write).
    pub fn get_in_transaction(&self, run_id: RunId, key: &str) -> Result<Option<Value>> {
        self.db.transaction(run_id, |txn| {
            let storage_key = Key::new_kv(run_id.namespace(), key);
            txn.get(&storage_key)
        })
    }
}
```

### Tests

```rust
#[test]
fn test_fast_get_returns_correct_value() {
    let db = setup_test_db();
    let kv = KVStore::new(db.clone());
    let run_id = RunId::new();

    kv.put(run_id, "key", Value::I64(42)).unwrap();

    let result = kv.get(run_id, "key").unwrap();
    assert_eq!(result, Some(Value::I64(42)));
}

#[test]
fn test_fast_get_returns_none_for_missing() {
    let db = setup_test_db();
    let kv = KVStore::new(db.clone());
    let run_id = RunId::new();

    let result = kv.get(run_id, "missing").unwrap();
    assert_eq!(result, None);
}

#[test]
fn test_fast_get_equals_transaction_get() {
    let db = setup_test_db();
    let kv = KVStore::new(db.clone());
    let run_id = RunId::new();

    kv.put(run_id, "key", Value::I64(42)).unwrap();

    let fast = kv.get(run_id, "key").unwrap();
    let txn = kv.get_in_transaction(run_id, "key").unwrap();

    assert_eq!(fast, txn);
}
```

### Validation

```bash
~/.cargo/bin/cargo test -p in-mem-primitives kv::tests::test_fast_get
~/.cargo/bin/cargo bench --bench m4_performance -- kvstore/get
```

### Complete Story

```bash
./scripts/complete-story.sh 236
```

---

## Story #237: KVStore Fast Path Batch Get

**GitHub Issue**: [#237](https://github.com/anibjoshi/in-mem/issues/237)
**Estimated Time**: 3 hours
**Dependencies**: Story #236

### Start Story

```bash
gh issue view 237
./scripts/start-story.sh 24 237 kv-batch-get
```

### Implementation

```rust
impl KVStore {
    /// Get multiple values in a single snapshot (FAST PATH)
    ///
    /// Single snapshot acquisition for all keys.
    /// More efficient than multiple get() calls.
    ///
    /// # Performance
    /// For N keys: ~(snapshot_time + N * lookup_time)
    /// vs N * (snapshot_time + lookup_time) for individual gets
    pub fn get_many(&self, run_id: RunId, keys: &[&str]) -> Result<Vec<Option<Value>>> {
        // Single snapshot for consistency
        let snapshot = self.db.snapshot();
        let namespace = run_id.namespace();

        keys.iter()
            .map(|key| {
                let storage_key = Key::new_kv(namespace.clone(), key);
                Ok(snapshot.get(&run_id, &storage_key).map(|v| v.value.clone()))
            })
            .collect()
    }

    /// Get multiple values as a HashMap
    pub fn get_many_map(&self, run_id: RunId, keys: &[&str]) -> Result<HashMap<String, Value>> {
        let snapshot = self.db.snapshot();
        let namespace = run_id.namespace();

        let mut result = HashMap::with_capacity(keys.len());
        for key in keys {
            let storage_key = Key::new_kv(namespace.clone(), key);
            if let Some(v) = snapshot.get(&run_id, &storage_key) {
                result.insert(key.to_string(), v.value.clone());
            }
        }
        Ok(result)
    }

    /// Check if key exists (FAST PATH)
    pub fn contains(&self, run_id: RunId, key: &str) -> Result<bool> {
        let snapshot = self.db.snapshot();
        let storage_key = Key::new_kv(run_id.namespace(), key);
        Ok(snapshot.get(&run_id, &storage_key).is_some())
    }
}
```

### Tests

```rust
#[test]
fn test_get_many_returns_all_values() {
    let db = setup_test_db();
    let kv = KVStore::new(db.clone());
    let run_id = RunId::new();

    kv.put(run_id, "a", Value::I64(1)).unwrap();
    kv.put(run_id, "b", Value::I64(2)).unwrap();
    kv.put(run_id, "c", Value::I64(3)).unwrap();

    let results = kv.get_many(run_id, &["a", "b", "c", "missing"]).unwrap();

    assert_eq!(results[0], Some(Value::I64(1)));
    assert_eq!(results[1], Some(Value::I64(2)));
    assert_eq!(results[2], Some(Value::I64(3)));
    assert_eq!(results[3], None);
}

#[test]
fn test_get_many_uses_single_snapshot() {
    // Verify consistent view even with concurrent writes
    let db = setup_test_db();
    let kv = KVStore::new(db.clone());
    let run_id = RunId::new();

    kv.put(run_id, "a", Value::I64(1)).unwrap();
    kv.put(run_id, "b", Value::I64(2)).unwrap();

    // Concurrent write during batch read shouldn't affect consistency
    let results = kv.get_many(run_id, &["a", "b"]).unwrap();

    // Both from same snapshot - either both old or both new
    assert!(results[0].is_some());
    assert!(results[1].is_some());
}
```

### Complete Story

```bash
./scripts/complete-story.sh 237
```

---

## Story #238: Other Primitive Fast Paths

**GitHub Issue**: [#238](https://github.com/anibjoshi/in-mem/issues/238)
**Estimated Time**: 4 hours
**Dependencies**: Story #236

### Start Story

```bash
gh issue view 238
./scripts/start-story.sh 24 238 other-fast-paths
```

### Implementation

**EventLog** (`crates/primitives/src/event_log.rs`):

```rust
impl EventLog {
    /// Read event by sequence (FAST PATH)
    pub fn read(&self, run_id: RunId, sequence: u64) -> Result<Option<Event>> {
        let snapshot = self.db.snapshot();
        let ns = Namespace::for_run(run_id);
        let event_key = Key::new_event(ns, sequence);

        match snapshot.get(&run_id, &event_key) {
            Some(v) => Ok(Some(serde_json::from_value(v.value.into_json()?)?)),
            None => Ok(None),
        }
    }

    /// Get log length (FAST PATH)
    pub fn len(&self, run_id: RunId) -> Result<u64> {
        let snapshot = self.db.snapshot();
        let ns = Namespace::for_run(run_id);
        let meta_key = Key::new_event_meta(ns);

        match snapshot.get(&run_id, &meta_key) {
            Some(v) => {
                let meta: EventLogMeta = serde_json::from_value(v.value.into_json()?)?;
                Ok(meta.next_sequence)
            }
            None => Ok(0),
        }
    }

    /// Check if log is empty (FAST PATH)
    pub fn is_empty(&self, run_id: RunId) -> Result<bool> {
        Ok(self.len(run_id)? == 0)
    }
}
```

**StateCell** (`crates/primitives/src/state_cell.rs`):

```rust
impl StateCell {
    /// Read state (FAST PATH)
    pub fn read(&self, run_id: RunId, name: &str) -> Result<Option<State>> {
        let snapshot = self.db.snapshot();
        let ns = Namespace::for_run(run_id);
        let state_key = Key::new_state(ns, name);

        match snapshot.get(&run_id, &state_key) {
            Some(v) => Ok(Some(serde_json::from_value(v.value.into_json()?)?)),
            None => Ok(None),
        }
    }

    /// Check if cell exists (FAST PATH)
    pub fn exists(&self, run_id: RunId, name: &str) -> Result<bool> {
        let snapshot = self.db.snapshot();
        let ns = Namespace::for_run(run_id);
        let state_key = Key::new_state(ns, name);
        Ok(snapshot.get(&run_id, &state_key).is_some())
    }
}
```

**TraceStore** (`crates/primitives/src/trace.rs`):

```rust
impl TraceStore {
    /// Get trace by ID (FAST PATH)
    pub fn get(&self, run_id: RunId, trace_id: &str) -> Result<Option<Trace>> {
        let snapshot = self.db.snapshot();
        let ns = Namespace::for_run(run_id);
        let trace_key = Key::new_trace(ns, trace_id);

        match snapshot.get(&run_id, &trace_key) {
            Some(v) => Ok(Some(serde_json::from_value(v.value.into_json()?)?)),
            None => Ok(None),
        }
    }

    /// Check if trace exists (FAST PATH)
    pub fn exists(&self, run_id: RunId, trace_id: &str) -> Result<bool> {
        let snapshot = self.db.snapshot();
        let ns = Namespace::for_run(run_id);
        let trace_key = Key::new_trace(ns, trace_id);
        Ok(snapshot.get(&run_id, &trace_key).is_some())
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 238
```

---

## Story #239: Observational Equivalence Tests

**GitHub Issue**: [#239](https://github.com/anibjoshi/in-mem/issues/239)
**Estimated Time**: 3 hours
**Dependencies**: Story #238

### Start Story

```bash
gh issue view 239
./scripts/start-story.sh 24 239 equivalence-tests
```

### Implementation

Create `tests/m4_fast_path_equivalence.rs`:

```rust
//! Tests verifying fast path reads are observationally equivalent
//! to transaction-based reads.

use in_mem_primitives::{KVStore, EventLog, StateCell, TraceStore};
use in_mem_core::{Value, RunId};

#[test]
fn kv_fast_path_equivalent() {
    let db = Database::builder().in_memory().open_temp().unwrap();
    let kv = KVStore::new(db.clone());
    let run_id = RunId::new();

    // Write some data
    kv.put(run_id, "key1", Value::String("value1".into())).unwrap();
    kv.put(run_id, "key2", Value::I64(42)).unwrap();

    // Fast path reads
    let fast1 = kv.get(run_id, "key1").unwrap();
    let fast2 = kv.get(run_id, "key2").unwrap();
    let fast_missing = kv.get(run_id, "missing").unwrap();

    // Transaction reads
    let txn1 = kv.get_in_transaction(run_id, "key1").unwrap();
    let txn2 = kv.get_in_transaction(run_id, "key2").unwrap();
    let txn_missing = kv.get_in_transaction(run_id, "missing").unwrap();

    // Must be identical
    assert_eq!(fast1, txn1);
    assert_eq!(fast2, txn2);
    assert_eq!(fast_missing, txn_missing);
}

#[test]
fn fast_path_no_dirty_read() {
    let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
    let kv = KVStore::new((*db).clone());
    let run_id = RunId::new();

    kv.put(run_id, "key", Value::I64(1)).unwrap();

    // Start uncommitted transaction
    let mut txn = db.begin_transaction(run_id).unwrap();
    txn.put(Key::new_kv(run_id.namespace(), "key"), Value::I64(999)).unwrap();
    // Don't commit!

    // Fast path should NOT see uncommitted value
    let result = kv.get(run_id, "key").unwrap();
    assert_eq!(result, Some(Value::I64(1))); // Original, not 999
}

#[test]
fn batch_read_snapshot_consistency() {
    use std::thread;
    use std::sync::Barrier;

    let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
    let kv = KVStore::new((*db).clone());
    let run_id = RunId::new();

    kv.put(run_id, "a", Value::I64(1)).unwrap();
    kv.put(run_id, "b", Value::I64(2)).unwrap();

    let barrier = Arc::new(Barrier::new(2));

    // Writer thread
    let kv2 = kv.clone();
    let barrier2 = Arc::clone(&barrier);
    let handle = thread::spawn(move || {
        barrier2.wait();
        kv2.put(run_id, "a", Value::I64(100)).unwrap();
        kv2.put(run_id, "b", Value::I64(200)).unwrap();
    });

    // Reader - batch read should see consistent snapshot
    barrier.wait();
    let results = kv.get_many(run_id, &["a", "b"]).unwrap();

    let a = results[0].as_ref().unwrap().as_i64().unwrap();
    let b = results[1].as_ref().unwrap().as_i64().unwrap();

    // Either both old OR both new - never mixed
    assert!(
        (a == 1 && b == 2) || (a == 100 && b == 200),
        "Snapshot should be consistent: a={}, b={}",
        a, b
    );

    handle.join().unwrap();
}
```

### Complete Story

```bash
./scripts/complete-story.sh 239
```

---

## Epic 24 Completion Checklist

### 1. Final Validation

```bash
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo test --test m4_fast_path_equivalence
~/.cargo/bin/cargo bench --bench m4_performance -- kvstore/get
```

### 2. Verify Deliverables

- [ ] KVStore.get() < 10µs
- [ ] get_many() uses single snapshot
- [ ] EventLog/StateCell/TraceStore fast paths work
- [ ] All fast paths observationally equivalent

### 3. Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-24-read-path-optimization -m "Epic 24: Read Path Optimization complete"
git push origin develop
gh issue close 215 --comment "Epic 24 complete. All 4 stories delivered."
```
