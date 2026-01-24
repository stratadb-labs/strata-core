# StateCell Defects and Gaps

> Consolidated from architecture review, primitive vs substrate analysis, and coordination primitive best practices.
> Source: `crates/api/src/substrate/state.rs` and `crates/primitives/src/state_cell.rs`
> **Updated**: 2026-01-23 - Many gaps have been resolved.

## Summary

| Category | Count | Priority |
|----------|-------|----------|
| ~~Hidden Primitive Features~~ | ~~4~~ 0 | ~~P0~~ RESOLVED |
| ~~Stubbed/Unimplemented~~ | ~~1~~ 0 | ~~P1~~ RESOLVED |
| ~~Missing Convenience APIs~~ | ~~2~~ 1 | P2 |
| API Design Considerations | 2 | P2 |
| Advanced Coordination Features | 9 | P1-P2 |
| **Total Open Issues** | **12** | |

---

## What is StateCell?

StateCell is a **coordination primitive** for single-value state machines, NOT a general key-value store.

**Purpose:** Atomic state transitions with compare-and-swap semantics
- Locks and mutexes
- Leader election
- Distributed barriers
- State machine coordination
- Configuration with atomic updates

**vs KVStore:**
| Aspect | StateCell | KVStore |
|--------|-----------|---------|
| Model | Single named cell | Multiple key-value pairs |
| Versioning | Counter (1, 2, 3...) | Transaction version |
| Core Pattern | CAS + transitions | Read/write/delete |
| Use Case | Coordination | Storage |

---

## Current Substrate API (12 methods)

```rust
// Core operations
fn state_set(run, cell, value) -> Version;
fn state_get(run, cell) -> Option<Versioned<Value>>;
fn state_cas(run, cell, expected_counter, value) -> Option<Version>;
fn state_delete(run, cell) -> bool;
fn state_exists(run, cell) -> bool;

// Initialization
fn state_init(run, cell, value) -> Version;  // ✅ IMPLEMENTED
fn state_get_or_init(run, cell, default: FnOnce) -> Versioned<Value>;  // ✅ IMPLEMENTED (MVP)

// Discovery
fn state_list(run) -> Vec<String>;  // ✅ IMPLEMENTED

// Transition operations (THE CORE FEATURE)
fn state_transition(run, cell, f) -> (Value, Version);  // ✅ IMPLEMENTED
fn state_transition_or_init(run, cell, initial, f) -> (Value, Version);  // ✅ IMPLEMENTED

// History
fn state_history(run, cell, limit, before) -> Vec<Versioned<Value>>;  // ✅ IMPLEMENTED
```

---

## Part 1: RESOLVED - Previously Hidden Features Now Exposed

These features existed in the primitive layer but were not exposed at the substrate level. **They are now fully implemented.**

### ✅ RESOLVED: `state_transition` - Atomic State Machine Transitions

**Status:** IMPLEMENTED at `state.rs:306-329`

This is the CORE feature of StateCell - atomic read-modify-write with automatic OCC retry (200 retries).

```rust
// Users CAN now do:
let (old_count, version) = substrate.state_transition(&run, "counter", |current| {
    let n = current.as_int().unwrap_or(0);
    Ok(Value::Int(n + 1))
})?;
```

**Test Coverage:** 22 comprehensive tests in `statecell/transitions.rs`

---

### ✅ RESOLVED: `state_transition_or_init` - Initialize Then Transition

**Status:** IMPLEMENTED at `state.rs:331-355`

```rust
// Initialize with 0 if doesn't exist, then apply transition
let (value, version) = substrate.state_transition_or_init(
    &run, "counter", Value::Int(0), |current| {
        let n = current.as_int().unwrap_or(0);
        Ok(Value::Int(n + 1))
    }
)?;
```

---

### ✅ RESOLVED: `state_list` - List All Cells

**Status:** IMPLEMENTED at `state.rs:301-303`

```rust
let cell_names = substrate.state_list(&run)?;
```

---

### ✅ RESOLVED: `state_init` - Conditional Create (Init If Absent)

**Status:** IMPLEMENTED at `state.rs:295-299`

```rust
// Creates cell only if it doesn't exist, returns error otherwise
let version = substrate.state_init(&run, "lock", Value::String("holder-1".into()))?;
```

---

## Part 2: RESOLVED - Previously Stubbed Features

### ✅ RESOLVED: `state_history` - Version History

**Status:** IMPLEMENTED

The primitive now has a `history()` method that uses the storage layer's `get_history()` to retrieve version history. The substrate `state_history()` calls through to this primitive method.

```rust
// Working implementation
let history = substrate.state_history(&run, "cell", Some(10), None)?;
for entry in history {
    println!("Value: {:?}, Counter: {:?}", entry.value, entry.version);
}
```

**Key behaviors:**
- Returns historical versions with `Version::Counter` semantics
- Supports `limit` for pagination
- Supports `before` filter (must be `Version::Counter`, not `Version::Txn`)
- Returns empty vec for non-existent cells
- Uses storage layer's `VersionChain` for history tracking

**Test Coverage:** 9 tests in `statecell/transitions.rs`

---

## Part 3: Convenience APIs

### ✅ RESOLVED: `state_get_or_init` - Get or Initialize (Lazy Default)

**Status:** IMPLEMENTED at `state.rs:419-447`

**API:**
```rust
fn state_get_or_init<F>(
    &self,
    run: &ApiRunId,
    cell: &str,
    default: F,
) -> StrataResult<Versioned<Value>>
where
    F: FnOnce() -> Value;
```

**Key Design Decision:** Uses `FnOnce() -> Value` for **lazy default evaluation**.
This avoids allocating defaults on the hot path when reading existing cells.

```rust
// Example: expensive default only computed if cell doesn't exist
let state = substrate.state_get_or_init(&run, "config", || {
    compute_expensive_default()  // Only called if cell is missing
})?;
```

**Semantics:**
- If cell exists: returns current value (default closure NOT called)
- If cell doesn't exist: calls `default()`, initializes cell, returns new value
- New cells always have version 1

**Test Coverage:** 8 tests in `statecell/basic_ops.rs`

---

### Gap 3: `state_info` - Cell Metadata (O(1)) [DEFERRED]

**Priority:** P2 - Performance optimization

**Proposed API:**
```rust
struct CellInfo {
    version: u64,
    updated_at: i64,
    exists: bool,
}

fn state_info(&self, run: &ApiRunId, cell: &str) -> StrataResult<Option<CellInfo>>;
```

**Why Useful:**
- Check version/timestamp without reading full value
- Staleness checks
- Monitoring cell activity

**Current Workaround:** Use `state_get()` which returns full value.

---

## Part 4: API Design Considerations (P2)

### Design 1: CAS Returns Option Instead of Result

**Current:**
```rust
fn state_cas(...) -> StrataResult<Option<Version>>;
// None = version mismatch (not an error)
```

**Consideration:**
- Conflates "operation failed" with "version mismatch"
- Users can't distinguish network error from CAS failure
- May be intentional for ease of use in retry loops

**Alternative:**
```rust
fn state_cas(...) -> StrataResult<Version>;
// Err(VersionMismatch) on conflict
```

**Status:** Not a bug, but worth documenting the tradeoff.

---

### Design 2: Timestamp Exposure

**Current Return Type:**
```rust
Versioned<Value>  // Has version + value + timestamp
```

The `Versioned` struct includes timestamp, but users may want metadata without the value.

**Status:** Low priority - `state_info()` would address this if needed.

---

## Part 5: Known Limitations (Not Bugs)

### Limitation 1: Counter Versioning (Not Transaction Versioning)

StateCell uses counter versioning (1, 2, 3...) not transaction versioning.

**Implication:** Cannot correlate StateCell versions with KVStore versions in cross-primitive transactions.

**Status:** By design - different versioning semantics

---

### Limitation 2: Transition Closure Purity Requirement

Transition closures MUST be pure functions (no I/O, no side effects) because they may be retried up to 200 times.

**Status:** By design - documented requirement. Test coverage verifies behavior.

---

## Part 6: Advanced Coordination Features (Future Work)

These features don't exist anywhere in the codebase but are found in production coordination systems like ZooKeeper, etcd, and Consul.

### Gap 4: Fencing Tokens - Distributed Lock Correctness (P1)

Without fencing tokens, distributed locks can suffer from the "paused client" problem.

### Gap 5: Multi-Cell Transactions - Atomic Cross-Cell Operations (P1)

Cannot atomically check/update multiple cells in a single operation.

### Gap 6: Atomic Increment - Built-in Counter Operations (P2)

Counters are common but require full read-modify-write via `state_transition`.

### Gap 7: Compare-and-Delete - Conditional Deletion (P2)

Cannot delete a cell only if it has expected version.

### Gap 8: Batch Operations - Multi-Cell Read/Write (P2)

Reading 10 cells requires 10 operations (or 10 round trips in distributed setup).

### Gap 9: Prefix/Namespace Queries - List by Pattern (P2)

`state_list` returns ALL cells. Cannot list subset by prefix.

### Gap 10: Watch/Subscribe - Change Notification (P2)

No way to watch for changes without polling.

### Gap 11: TTL/Lease - Ephemeral Cells (P2)

No automatic cell expiration for ephemeral coordination.

### Gap 12: Session/Ownership Semantics (P2)

No way to associate cells with client sessions for automatic cleanup.

---

## Priority Matrix

| ID | Issue | Priority | Status | Effort |
|----|-------|----------|--------|--------|
| ~~Gap 1~~ | ~~Transition closures~~ | ~~P0~~ | ✅ RESOLVED | - |
| ~~Gap 2~~ | ~~List cells~~ | ~~P0~~ | ✅ RESOLVED | - |
| ~~Gap 3~~ | ~~Init (create if absent)~~ | ~~P0~~ | ✅ RESOLVED | - |
| ~~Gap 4~~ | ~~Transition or init~~ | ~~P0~~ | ✅ RESOLVED | - |
| ~~Gap 5~~ | ~~History stubbed~~ | ~~P1~~ | ✅ RESOLVED | - |
| ~~Gap 6~~ | ~~Get or init~~ | ~~P2~~ | ✅ RESOLVED (MVP) | - |
| Gap 3 | Cell info/metadata | P2 | DEFERRED | Low |
| Gap 4 | Fencing tokens | P1 | OPEN | Medium |
| Gap 5 | Multi-cell transactions | P1 | OPEN | High |
| Gap 6 | Atomic increment | P2 | OPEN | Low |
| Gap 7 | Compare-and-delete | P2 | OPEN | Low |
| Gap 8 | Batch operations | P2 | OPEN | Medium |
| Gap 9 | Prefix queries | P2 | OPEN | Medium |
| Gap 10 | Watch/subscribe | P2 | OPEN | High |
| Gap 11 | TTL/lease | P2 | OPEN | High |
| Gap 12 | Session/ownership | P2 | OPEN | High |
| Design 1 | CAS return type | P2 | OPEN | Low |
| Design 2 | Timestamp exposure | P2 | OPEN | Low |

---

## Test Coverage Status

| API | Test File | Tests |
|-----|-----------|-------|
| `state_set` | `basic_ops.rs` | ✅ |
| `state_get` | `basic_ops.rs` | ✅ |
| `state_cas` | `cas_ops.rs` | ✅ |
| `state_delete` | `basic_ops.rs` | ✅ |
| `state_exists` | `basic_ops.rs` | ✅ |
| `state_init` | `invariants.rs` | ✅ |
| `state_get_or_init` | `basic_ops.rs` | ✅ 8 tests |
| `state_list` | `invariants.rs` | ✅ |
| `state_transition` | `transitions.rs` | ✅ 22 tests |
| `state_transition_or_init` | `transitions.rs` | ✅ |
| `state_history` | `transitions.rs` | ✅ 9 tests |

**Total StateCell Tests:** 114

---

## Comparison with Industry Standards

| Feature | Strata StateCell | ZooKeeper | etcd | Consul KV |
|---------|------------------|-----------|------|-----------|
| Get/Set | ✅ | ✅ | ✅ | ✅ |
| CAS | ✅ | ✅ | ✅ | ✅ |
| Delete | ✅ | ✅ | ✅ | ✅ |
| **Transitions** | ✅ | ❌ | ❌ | ❌ |
| **List** | ✅ | ✅ | ✅ | ✅ |
| **Init if absent** | ✅ | ✅ | ❌ | ❌ |
| **Get or init (lazy)** | ✅ | ❌ | ❌ | ❌ |
| **History** | ✅ | ❌ | ✅ | ❌ |
| Watch | ❌ | ✅ | ✅ | ✅ |
| TTL/Lease | ❌ | ✅ | ✅ | ✅ |
| Fencing | ❌ | ✅ | ✅ | ❌ |
| Multi-key Txn | ❌ | ✅ | ✅ | ❌ |
| Sessions | ❌ | ✅ | ✅ | ✅ |

**Strata's Unique Strengths:**
1. Transition closures with automatic OCC retry (200 retries) - no other system has this built-in
2. Lazy `get_or_init` with `FnOnce` - avoids default allocation on hot path

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 2.2 | 2026-01-23 | Implemented state_get_or_init with lazy default (MVP complete) |
| 2.1 | 2026-01-23 | Implemented state_history using storage layer's get_history |
| 2.0 | 2026-01-23 | Major update: marked resolved gaps, updated test coverage |
| 1.0 | 2026-01-22 | Initial audit |
