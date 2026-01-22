# Epic 61: Versioned Returns - Implementation Prompts

**Epic Goal**: Wrap all read returns in Versioned<T>, all writes return Version

**GitHub Issue**: [#465](https://github.com/anibjoshi/in-mem/issues/465)
**Status**: Ready to begin after Epic 60
**Dependencies**: Epic 60 (Core Types)
**Phases**: 2, 3, 4 (incremental)

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
3. **Epic Spec**: `docs/milestones/M9/EPIC_61_VERSIONED_RETURNS.md`
4. **Prompt Header**: `docs/prompts/M9/M9_PROMPT_HEADER.md` for the 4 architectural rules

---

## Epic 61 Overview

### CRITICAL: Phased Implementation

> **Do not convert all 7 primitives in one pass.**

This epic spans multiple phases:

| Phase | Stories | Primitives |
|-------|---------|------------|
| Phase 2 | #475, #476 | KV, EventLog |
| Phase 3 | #477, #478 | StateCell, TraceStore |
| Phase 4 | #479, #480, #481 | JsonStore, VectorStore, RunIndex |

**Each phase must be completed and tested before proceeding to the next.**

### Key Rules

**Rule 1: Every Read Returns Versioned<T>**
```rust
// CORRECT
pub fn get(&self, run_id: &RunId, key: &str) -> Result<Option<Versioned<Value>>>

// WRONG - FORBIDDEN
pub fn get(&self, run_id: &RunId, key: &str) -> Result<Option<Value>>
```

**Rule 2: Every Write Returns Version**
```rust
// CORRECT
pub fn put(&self, run_id: &RunId, key: &str, value: Value) -> Result<Version>

// WRONG - FORBIDDEN
pub fn put(&self, run_id: &RunId, key: &str, value: Value) -> Result<()>
```

### Component Breakdown

| Story | Description | Phase | Priority |
|-------|-------------|-------|----------|
| #475 | KVStore Versioned Returns | 2 | CRITICAL |
| #476 | EventLog Versioned Returns | 2 | CRITICAL |
| #477 | StateCell Versioned Returns | 3 | CRITICAL |
| #478 | TraceStore Versioned Returns | 3 | CRITICAL |
| #479 | JsonStore Versioned Returns | 4 | CRITICAL |
| #480 | VectorStore Versioned Returns | 4 | CRITICAL |
| #481 | RunIndex Versioned Returns | 4 | CRITICAL |

---

## Phase 2: KV + EventLog

### Story #475: KVStore Versioned Returns

**GitHub Issue**: [#475](https://github.com/anibjoshi/in-mem/issues/475)
**Estimated Time**: 3 hours
**Dependencies**: Epic 60 complete
**Phase**: 2

#### Start Story

```bash
gh issue view 475
./scripts/start-story.sh 61 475 kv-versioned
```

#### Current API (Before)

```rust
impl KVStore {
    pub fn get(&self, run_id: &RunId, key: &str) -> Result<Option<Value>>;
    pub fn put(&self, run_id: &RunId, key: &str, value: Value) -> Result<()>;
    pub fn delete(&self, run_id: &RunId, key: &str) -> Result<bool>;
    pub fn exists(&self, run_id: &RunId, key: &str) -> Result<bool>;
}
```

#### New API (After)

```rust
use crate::{Versioned, Version, Timestamp};

impl KVStore {
    /// Get a value by key
    ///
    /// Returns Versioned<Value> if found, None if not found.
    /// The version indicates when this value was last written.
    pub fn get(&self, run_id: &RunId, key: &str) -> Result<Option<Versioned<Value>>> {
        let storage_key = self.make_key(run_id, key);

        match self.store.get(&storage_key)? {
            Some(entry) => {
                let value = Value::from_bytes(&entry.value)?;
                Ok(Some(Versioned::new(
                    value,
                    Version::TxnId(entry.version),
                    Timestamp::from_micros(entry.timestamp),
                )))
            }
            None => Ok(None),
        }
    }

    /// Put a value by key (upsert semantics)
    ///
    /// Returns the version created by this write.
    pub fn put(&self, run_id: &RunId, key: &str, value: Value) -> Result<Version> {
        let storage_key = self.make_key(run_id, key);
        let version = self.store.put(&storage_key, value.to_bytes()?)?;
        Ok(Version::TxnId(version))
    }

    /// Delete a key
    ///
    /// Returns true if the key existed and was deleted.
    /// Note: Delete doesn't return a version because the entity no longer exists.
    pub fn delete(&self, run_id: &RunId, key: &str) -> Result<bool> {
        let storage_key = self.make_key(run_id, key);
        self.store.delete(&storage_key)
    }

    /// Check if a key exists
    ///
    /// This is a read operation that doesn't return version info.
    /// Use get() if you need the version.
    pub fn exists(&self, run_id: &RunId, key: &str) -> Result<bool> {
        let storage_key = self.make_key(run_id, key);
        self.store.exists(&storage_key)
    }
}
```

#### Implementation Steps

1. **Import new types** at top of file:
   ```rust
   use crate::{Versioned, Version, Timestamp};
   ```

2. **Update get() signature and body**:
   - Change return type from `Option<Value>` to `Option<Versioned<Value>>`
   - Wrap value in `Versioned::new()`
   - Extract version and timestamp from storage entry

3. **Update put() signature and body**:
   - Change return type from `Result<()>` to `Result<Version>`
   - Return `Version::TxnId(version)` from storage

4. **Update all tests** to expect versioned returns:
   ```rust
   // BEFORE
   let value = kv.get(&run_id, "key")?.unwrap();
   assert_eq!(value, expected);

   // AFTER
   let versioned = kv.get(&run_id, "key")?.unwrap();
   assert_eq!(versioned.value, expected);
   assert!(versioned.version.is_txn_id());
   ```

#### Tests

```rust
#[test]
fn test_kv_get_returns_versioned() {
    let kv = KVStore::new_in_memory();
    let run_id = RunId::new("test");

    kv.put(&run_id, "key", Value::from("value")).unwrap();

    let result = kv.get(&run_id, "key").unwrap();
    assert!(result.is_some());

    let versioned = result.unwrap();
    assert_eq!(versioned.value, Value::from("value"));
    assert!(versioned.version.is_txn_id());
    assert!(versioned.timestamp.as_micros() > 0);
}

#[test]
fn test_kv_put_returns_version() {
    let kv = KVStore::new_in_memory();
    let run_id = RunId::new("test");

    let version = kv.put(&run_id, "key", Value::from("value")).unwrap();

    assert!(version.is_txn_id());
    assert!(version.as_u64() > 0);
}

#[test]
fn test_kv_versions_increase() {
    let kv = KVStore::new_in_memory();
    let run_id = RunId::new("test");

    let v1 = kv.put(&run_id, "key", Value::from("v1")).unwrap();
    let v2 = kv.put(&run_id, "key", Value::from("v2")).unwrap();
    let v3 = kv.put(&run_id, "key", Value::from("v3")).unwrap();

    assert!(v2.as_u64() > v1.as_u64());
    assert!(v3.as_u64() > v2.as_u64());
}
```

#### Validation

```bash
~/.cargo/bin/cargo test -p in-mem-primitives -- kv
~/.cargo/bin/cargo clippy -p in-mem-primitives -- -D warnings
```

#### Complete Story

```bash
./scripts/complete-story.sh 475
```

---

### Story #476: EventLog Versioned Returns

**GitHub Issue**: [#476](https://github.com/anibjoshi/in-mem/issues/476)
**Estimated Time**: 3 hours
**Dependencies**: Epic 60 complete
**Phase**: 2

#### Start Story

```bash
gh issue view 476
./scripts/start-story.sh 61 476 event-versioned
```

#### Current API (Before)

```rust
impl EventLog {
    pub fn append(&self, run_id: &RunId, event_type: &str, payload: Value) -> Result<u64>;
    pub fn read(&self, run_id: &RunId, sequence: u64) -> Result<Option<Event>>;
    pub fn range(&self, run_id: &RunId, start: u64, end: u64) -> Result<Vec<Event>>;
}
```

#### New API (After)

```rust
impl EventLog {
    /// Append an event to the log
    ///
    /// Returns the sequence number (version) assigned to this event.
    pub fn append(&self, run_id: &RunId, event_type: &str, payload: Value) -> Result<Version> {
        let sequence = self.store.append(run_id, event_type, payload)?;
        Ok(Version::Sequence(sequence))
    }

    /// Read a single event by sequence number
    ///
    /// Returns Versioned<Event> if found.
    pub fn read(&self, run_id: &RunId, sequence: u64) -> Result<Option<Versioned<Event>>> {
        match self.store.read(run_id, sequence)? {
            Some(event) => Ok(Some(Versioned::new(
                event.clone(),
                Version::Sequence(sequence),
                Timestamp::from_micros(event.timestamp),
            ))),
            None => Ok(None),
        }
    }

    /// Read a range of events
    ///
    /// Returns Vec<Versioned<Event>> for the range.
    pub fn range(&self, run_id: &RunId, start: u64, end: u64) -> Result<Vec<Versioned<Event>>> {
        let events = self.store.range(run_id, start, end)?;
        Ok(events
            .into_iter()
            .enumerate()
            .map(|(i, event)| {
                let seq = start + i as u64;
                Versioned::new(
                    event.clone(),
                    Version::Sequence(seq),
                    Timestamp::from_micros(event.timestamp),
                )
            })
            .collect())
    }
}
```

#### Tests

```rust
#[test]
fn test_event_append_returns_version() {
    let events = EventLog::new_in_memory();
    let run_id = RunId::new("test");

    let version = events.append(&run_id, "user_action", json!({})).unwrap();

    assert!(version.is_sequence());
    assert_eq!(version.as_u64(), 0); // First event
}

#[test]
fn test_event_read_returns_versioned() {
    let events = EventLog::new_in_memory();
    let run_id = RunId::new("test");

    events.append(&run_id, "test", json!({"data": 1})).unwrap();

    let result = events.read(&run_id, 0).unwrap();
    assert!(result.is_some());

    let versioned = result.unwrap();
    assert_eq!(versioned.value.event_type, "test");
    assert!(versioned.version.is_sequence());
    assert_eq!(versioned.version.as_u64(), 0);
}

#[test]
fn test_event_range_returns_versioned() {
    let events = EventLog::new_in_memory();
    let run_id = RunId::new("test");

    for i in 0..5 {
        events.append(&run_id, "test", json!({"i": i})).unwrap();
    }

    let range = events.range(&run_id, 1, 4).unwrap();

    assert_eq!(range.len(), 3);
    assert_eq!(range[0].version.as_u64(), 1);
    assert_eq!(range[1].version.as_u64(), 2);
    assert_eq!(range[2].version.as_u64(), 3);
}
```

#### Complete Story

```bash
./scripts/complete-story.sh 476
```

---

## Phase 2 Completion

After completing #475 and #476:

1. **Run full test suite**:
   ```bash
   ~/.cargo/bin/cargo test --workspace
   ```

2. **Write conformance tests** for KV and EventLog (see Epic 64)

3. **Verify pattern works** before proceeding to Phase 3

---

## Phase 3: State + Trace

### Story #477: StateCell Versioned Returns

**GitHub Issue**: [#477](https://github.com/anibjoshi/in-mem/issues/477)
**Estimated Time**: 3 hours
**Phase**: 3

#### Start Story

```bash
gh issue view 477
./scripts/start-story.sh 61 477 state-versioned
```

#### New API

```rust
impl StateCell {
    /// Read a state cell
    pub fn read(&self, run_id: &RunId, name: &str) -> Result<Option<Versioned<StateValue>>> {
        match self.store.read(run_id, name)? {
            Some((value, counter, timestamp)) => Ok(Some(Versioned::new(
                value,
                Version::Counter(counter),
                Timestamp::from_micros(timestamp),
            ))),
            None => Ok(None),
        }
    }

    /// Set a state cell (unconditional)
    pub fn set(&self, run_id: &RunId, name: &str, value: StateValue) -> Result<Version> {
        let counter = self.store.set(run_id, name, value)?;
        Ok(Version::Counter(counter))
    }

    /// Compare-and-swap
    ///
    /// Returns Ok(Version) if successful, Err(VersionConflict) if not.
    pub fn cas(&self, run_id: &RunId, name: &str, expected: u64, value: StateValue) -> Result<Version> {
        let counter = self.store.cas(run_id, name, expected, value)?;
        Ok(Version::Counter(counter))
    }
}
```

**Note**: CAS now returns `Result<Version>` instead of `Result<bool>`. On mismatch, return `StrataError::VersionConflict`.

#### Complete Story

```bash
./scripts/complete-story.sh 477
```

---

### Story #478: TraceStore Versioned Returns

**GitHub Issue**: [#478](https://github.com/anibjoshi/in-mem/issues/478)
**Estimated Time**: 2 hours
**Phase**: 3

#### Start Story

```bash
gh issue view 478
./scripts/start-story.sh 61 478 trace-versioned
```

#### New API

```rust
impl TraceStore {
    /// Record a trace
    ///
    /// Returns Versioned<TraceId> - the new trace ID with version info.
    pub fn record(&self, run_id: &RunId, trace_type: TraceType, data: Value) -> Result<Versioned<TraceId>> {
        let (trace_id, timestamp) = self.store.record(run_id, trace_type, data)?;
        Ok(Versioned::new(
            trace_id.clone(),
            Version::TxnId(trace_id.as_u64()),
            Timestamp::from_micros(timestamp),
        ))
    }

    /// Read a trace by ID
    pub fn read(&self, run_id: &RunId, trace_id: &TraceId) -> Result<Option<Versioned<Trace>>> {
        match self.store.read(run_id, trace_id)? {
            Some(trace) => Ok(Some(Versioned::new(
                trace.clone(),
                Version::TxnId(trace_id.as_u64()),
                Timestamp::from_micros(trace.timestamp),
            ))),
            None => Ok(None),
        }
    }
}
```

#### Complete Story

```bash
./scripts/complete-story.sh 478
```

---

## Phase 4: Json + Vector + RunIndex

### Story #479: JsonStore Versioned Returns

**GitHub Issue**: [#479](https://github.com/anibjoshi/in-mem/issues/479)
**Phase**: 4

#### Start Story

```bash
gh issue view 479
./scripts/start-story.sh 61 479 json-versioned
```

Follow same pattern as KV. Key changes:
- `get()` returns `Option<Versioned<JsonValue>>`
- `get_path()` returns `Option<Versioned<JsonValue>>` with document version
- `create()`, `set()`, `set_path()` return `Version`

#### Complete Story

```bash
./scripts/complete-story.sh 479
```

---

### Story #480: VectorStore Versioned Returns

**GitHub Issue**: [#480](https://github.com/anibjoshi/in-mem/issues/480)
**Phase**: 4

#### Start Story

```bash
gh issue view 480
./scripts/start-story.sh 61 480 vector-versioned
```

Key changes:
- `get()` returns `Option<Versioned<VectorEntry>>`
- `upsert()` returns `Version`
- `search()` returns `Vec<Versioned<VectorMatch>>`

#### Complete Story

```bash
./scripts/complete-story.sh 480
```

---

### Story #481: RunIndex Versioned Returns

**GitHub Issue**: [#481](https://github.com/anibjoshi/in-mem/issues/481)
**Phase**: 4

#### Start Story

```bash
gh issue view 481
./scripts/start-story.sh 61 481 run-versioned
```

Key changes:
- `get()` returns `Option<Versioned<RunMetadata>>`
- `create()` returns `Version`
- `transition()` returns `Version`
- `list()` returns `Vec<Versioned<RunId>>`

#### Complete Story

```bash
./scripts/complete-story.sh 481
```

---

## Epic 61 Completion Checklist

### After All Phases Complete

```bash
# Full test suite
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace -- -D warnings

# Verify all primitives have versioned returns
~/.cargo/bin/cargo test versioned_returns
```

### Verify Deliverables

- [ ] KVStore: get() → Versioned, put() → Version
- [ ] EventLog: read() → Versioned, append() → Version
- [ ] StateCell: read() → Versioned, set()/cas() → Version
- [ ] TraceStore: read() → Versioned, record() → Versioned<TraceId>
- [ ] JsonStore: get()/get_path() → Versioned, set()/create() → Version
- [ ] VectorStore: get() → Versioned, upsert() → Version
- [ ] RunIndex: get() → Versioned, create()/transition() → Version
- [ ] All existing tests updated
- [ ] No raw value returns from any read operation

### Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-61-versioned-returns -m "Epic 61: Versioned Returns complete

All 7 primitives now:
- Return Versioned<T> from reads
- Return Version from writes

Stories: #475, #476, #477, #478, #479, #480, #481
"
git push origin develop
gh issue close 465 --comment "Epic 61: Versioned Returns - COMPLETE"
```
