# Epic 82: Substrate API Implementation - Implementation Prompts

**Epic Goal**: Implement the power-user Substrate API with explicit run/version control

**GitHub Issue**: [#565](https://github.com/anibjoshi/in-mem/issues/565)
**Status**: Ready after Epic 80
**Dependencies**: Epic 80 (Value Model)
**Phase**: 3 (API Layers)

---

## NAMING CONVENTION - CRITICAL

> **NEVER use "M11" in the actual codebase or comments.**
>
> - "Strata" IS allowed (e.g., `StrataSubstrate`, `strata_substrate`)
>
> **CORRECT**: `//! Strata substrate API for power users`
> **WRONG**: `//! M11 substrate implementation`

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

Before starting ANY story in this epic, read:
1. **Contract Spec**: `docs/milestones/M11/M11_CONTRACT.md`
2. **Epic Spec**: `docs/milestones/M11/EPIC_82_SUBSTRATE_API.md`
3. **Prompt Header**: `docs/prompts/M11/M11_PROMPT_HEADER.md`

---

## Epic 82 Overview

### Scope
- KVStore substrate (kv_put, kv_get, kv_get_at, kv_delete, kv_exists, kv_history, kv_incr, kv_cas_version, kv_cas_value)
- JsonStore substrate (json_set, json_get, json_delete, json_merge, json_history)
- EventLog substrate (event_append, event_range)
- StateCell substrate (state_get, state_set, state_cas)
- VectorStore substrate (vector_set, vector_get, vector_delete, vector_history)
- TraceStore substrate (trace_record, trace_get, trace_range)
- RunIndex substrate (run_create, run_get, run_list, run_close)

### Key Difference from Facade

| Aspect | Facade | Substrate |
|--------|--------|-----------|
| Run | Implicit (default) | **Explicit** (required param) |
| Version | Hidden | **Exposed** (Versioned<T>) |
| Transaction | Auto-commit | Explicit control |
| Operations | Convenience wrappers | Full primitives |

### Architectural Invariants

1. **All substrate operations require explicit `run_id` parameter**
2. **All read operations return `Versioned<T>`**
3. **All write operations return `Version`**
4. **Substrate is the source of truth** - Facade desugars to this

### Success Criteria
- [ ] All KVStore operations implemented
- [ ] All JsonStore operations implemented
- [ ] All EventLog operations implemented
- [ ] All StateCell operations implemented
- [ ] All VectorStore operations implemented
- [ ] All RunIndex operations implemented
- [ ] All operations require explicit run_id
- [ ] All reads return Versioned

### Component Breakdown
- **Story #566**: KVStore Substrate
- **Story #567**: JsonStore Substrate
- **Story #568**: EventLog Substrate
- **Story #569**: StateCell Substrate
- **Story #570**: VectorStore Substrate
- **Story #571**: TraceStore Substrate
- **Story #572**: RunIndex Substrate

---

## Story #566: KVStore Substrate

**GitHub Issue**: [#566](https://github.com/anibjoshi/in-mem/issues/566)
**Dependencies**: Epic 80
**Blocks**: Epic 81 Facade KV

### Start Story

```bash
./scripts/start-story.sh 82 566 kvstore-substrate
```

### Key Implementation Points

```rust
pub trait KVStore {
    /// Put a key-value pair, returns the version assigned
    fn kv_put(&self, run_id: &RunId, key: &str, value: Value) -> Result<Version>;

    /// Get current value with version
    fn kv_get(&self, run_id: &RunId, key: &str) -> Result<Option<Versioned<Value>>>;

    /// Get value at specific version
    fn kv_get_at(&self, run_id: &RunId, key: &str, version: Version) -> Result<Option<Versioned<Value>>>;

    /// Delete a key, returns true if existed
    fn kv_delete(&self, run_id: &RunId, key: &str) -> Result<bool>;

    /// Check if key exists
    fn kv_exists(&self, run_id: &RunId, key: &str) -> Result<bool>;

    /// Get version history for a key
    fn kv_history(
        &self,
        run_id: &RunId,
        key: &str,
        limit: Option<usize>,
        before: Option<Version>,
    ) -> Result<Vec<Versioned<Value>>>;

    /// Atomic increment, returns new value
    fn kv_incr(&self, run_id: &RunId, key: &str, delta: i64) -> Result<i64>;

    /// CAS by version comparison
    fn kv_cas_version(
        &self,
        run_id: &RunId,
        key: &str,
        expected_version: Option<Version>,
        new_value: Value,
    ) -> Result<bool>;

    /// CAS by value comparison (structural equality)
    fn kv_cas_value(
        &self,
        run_id: &RunId,
        key: &str,
        expected_value: Value, // Use Absent for "not exists"
        new_value: Value,
    ) -> Result<bool>;
}
```

### Acceptance Criteria

- [ ] All operations require explicit `run_id`
- [ ] `kv_put` returns `Version`
- [ ] `kv_get` returns `Option<Versioned<Value>>`
- [ ] `kv_get_at` returns value at specific version
- [ ] `kv_history` returns versions newest-first
- [ ] `kv_incr` is atomic
- [ ] `kv_cas_version` compares by version
- [ ] `kv_cas_value` compares by structural equality (no coercion!)

---

## Story #567: JsonStore Substrate

**GitHub Issue**: [#567](https://github.com/anibjoshi/in-mem/issues/567)

### Key Implementation Points

```rust
pub trait JsonStore {
    /// Set value at JSON path
    fn json_set(&self, run_id: &RunId, key: &str, path: &str, value: Value) -> Result<Version>;

    /// Get value at JSON path with document version
    fn json_get(&self, run_id: &RunId, key: &str, path: &str) -> Result<Option<Versioned<Value>>>;

    /// Delete at JSON path
    fn json_delete(&self, run_id: &RunId, key: &str, path: &str) -> Result<bool>;

    /// Deep merge at JSON path
    fn json_merge(&self, run_id: &RunId, key: &str, path: &str, value: Value) -> Result<Version>;

    /// Get history for a JSON document
    fn json_history(
        &self,
        run_id: &RunId,
        key: &str,
        limit: Option<usize>,
        before: Option<Version>,
    ) -> Result<Vec<Versioned<Value>>>;
}
```

### Acceptance Criteria

- [ ] `json_get` returns document-level version
- [ ] Path syntax follows JSONPath
- [ ] Invalid paths return `InvalidPath` error
- [ ] `json_merge` does deep merge for objects

---

## Story #568: EventLog Substrate

**GitHub Issue**: [#568](https://github.com/anibjoshi/in-mem/issues/568)

### Key Implementation Points

```rust
pub trait EventLog {
    /// Append event to stream, returns sequence version
    fn event_append(&self, run_id: &RunId, stream: &str, payload: Value) -> Result<Version>;

    /// Read range of events
    fn event_range(
        &self,
        run_id: &RunId,
        stream: &str,
        start: Option<Version>,
        end: Option<Version>,
        limit: Option<usize>,
    ) -> Result<Vec<Versioned<Value>>>;
}
```

### Acceptance Criteria

- [ ] `event_append` returns `Version` with sequence tag
- [ ] Events are ordered by sequence number
- [ ] `event_range` supports pagination

---

## Story #569: StateCell Substrate

**GitHub Issue**: [#569](https://github.com/anibjoshi/in-mem/issues/569)

### Key Implementation Points

```rust
pub trait StateCell {
    /// Get current state
    fn state_get(&self, run_id: &RunId, key: &str) -> Result<Option<Versioned<Value>>>;

    /// Set state unconditionally
    fn state_set(&self, run_id: &RunId, key: &str, value: Value) -> Result<Version>;

    /// Compare-and-swap with structural equality
    fn state_cas(
        &self,
        run_id: &RunId,
        key: &str,
        expected: Value, // Use Absent for "not exists"
        new: Value,
    ) -> Result<bool>;
}
```

### Acceptance Criteria

- [ ] CAS uses structural equality
- [ ] `Absent` value for create-if-missing
- [ ] Returns false on mismatch (not error)

---

## Story #570: VectorStore Substrate

**GitHub Issue**: [#570](https://github.com/anibjoshi/in-mem/issues/570)

### Key Implementation Points

```rust
pub trait VectorStore {
    fn vector_set(
        &self,
        run_id: &RunId,
        key: &str,
        vector: Vec<f32>,
        metadata: Value,
    ) -> Result<Version>;

    fn vector_get(&self, run_id: &RunId, key: &str) -> Result<Option<Versioned<VectorEntry>>>;

    fn vector_delete(&self, run_id: &RunId, key: &str) -> Result<bool>;

    fn vector_history(
        &self,
        run_id: &RunId,
        key: &str,
        limit: Option<usize>,
        before: Option<Version>,
    ) -> Result<Vec<Versioned<VectorEntry>>>;
}
```

### Acceptance Criteria

- [ ] Vector dimension validated
- [ ] Metadata is arbitrary Value
- [ ] History available

---

## Story #572: RunIndex Substrate

**GitHub Issue**: [#572](https://github.com/anibjoshi/in-mem/issues/572)

### Key Implementation Points

```rust
pub trait RunIndex {
    /// Create a new run, returns its ID
    fn run_create(&self, name: Option<&str>) -> Result<RunId>;

    /// Get run metadata
    fn run_get(&self, run_id: &RunId) -> Result<Option<RunInfo>>;

    /// List all runs
    fn run_list(&self) -> Result<Vec<RunInfo>>;

    /// Close a run (make read-only)
    fn run_close(&self, run_id: &RunId) -> Result<()>;
}

pub struct RunInfo {
    pub id: RunId,
    pub name: Option<String>,
    pub created_at: u64,
    pub closed: bool,
}
```

### CRITICAL: Default Run

```rust
pub const DEFAULT_RUN_ID: &str = "default";

// Default run cannot be closed
fn run_close(&self, run_id: &RunId) -> Result<()> {
    if run_id == DEFAULT_RUN_ID {
        return Err(StrataError::ConstraintViolation {
            reason: "Cannot close default run".into(),
            details: None,
        });
    }
    // ...
}
```

### Acceptance Criteria

- [ ] `run_create` returns new UUID
- [ ] Default run always exists
- [ ] Default run cannot be closed
- [ ] Closed runs are read-only

---

## Epic 82 Completion Checklist

### 1. Final Validation

```bash
~/.cargo/bin/cargo test substrate_ -- --nocapture
~/.cargo/bin/cargo test --test m11_comprehensive substrate_api
```

### 2. Verify Explicit Run Requirement

```bash
# All substrate methods should have run_id parameter
grep -r "fn.*(&self, run_id" crates/api/src/substrate/
```

### 3. Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-82-substrate-api -m "Epic 82: Substrate API Implementation complete

Delivered:
- KVStore with kv_put, kv_get, kv_get_at, kv_history, kv_incr, kv_cas_*
- JsonStore with json_set, json_get, json_delete, json_merge
- EventLog with event_append, event_range
- StateCell with state_get, state_set, state_cas
- VectorStore with vector_set, vector_get, vector_delete
- RunIndex with run_create, run_get, run_list, run_close
- All operations require explicit run_id
- All reads return Versioned<T>

Stories: #566, #567, #568, #569, #570, #571, #572
"
git push origin develop
gh issue close 565 --comment "Epic 82: Substrate API Implementation - COMPLETE"
```

---

## Summary

Epic 82 establishes the SUBSTRATE API:

- **Explicit run_id**: Required for all operations
- **Versioned returns**: All reads return Versioned<T>
- **Full control**: Version history, CAS, explicit transactions
- **Default run protection**: Cannot be closed
- **Foundation for Facade**: Facade desugars to this
