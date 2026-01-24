# StateCell: Role, Purpose, and Specification

> **Status**: DRAFT
> **Date**: 2026-01-23
> **Purpose**: Define StateCell's role in Strata (architectural commitment)

---

## Executive Summary

StateCell is Strata's **coordination primitive**. It provides compare-and-swap (CAS) based atomic state management for scenarios requiring safe concurrent access to mutable state.

This document clarifies:
1. What StateCell is FOR in Strata
2. How it differs from KVStore
3. The critical purity requirement for transitions
4. The complete API specification

---

## Part 1: The Confusion

### Current Documentation Says

| Source | Statement |
|--------|-----------|
| M3_ARCHITECTURE.md | "Named CAS cells for coordination records, workflow state, and atomic state transitions" |
| PRIMITIVE_CONTRACT.md | StateCell uses counter-based versioning |
| state.rs | "Compare-and-swap cells for single-writer coordination" |

### The Tension

If StateCell is for "coordination" and KVStore is for "storage", why not just use KVStore with transactions?

**The Answer**: StateCell provides **version-aware optimistic concurrency** at the primitive level, enabling patterns that KVStore doesn't naturally support:

1. **CAS Semantics**: "Update only if unchanged since I read"
2. **Counter Versioning**: Every write increments version (not transaction-based)
3. **Automatic Retry**: `transition()` retries on conflict
4. **Init-or-Fail**: `init()` fails if cell exists (no upsert)

### Overlap with KVStore

| Aspect | StateCell | KVStore |
|--------|-----------|---------|
| **Purpose** | Coordination | General storage |
| **Versioning** | Counter (increments each write) | Transaction-based |
| **Write Pattern** | CAS-first (validate before update) | Put-always (overwrite) |
| **Conflict Model** | Cell-level version mismatch | Transaction-level conflict |
| **Retry Mechanism** | Built into `transition()` | External retry logic |
| **Init Semantics** | Fails if exists | N/A (put upserts) |

**Key Distinction**: StateCell answers "has this changed since I read it?" at the cell level. KVStore answers "did any conflicting write occur?" at the transaction level.

---

## Part 2: StateCell's Role (LOCKED)

### The Definitive Answer

**StateCell is the coordination primitive for version-aware atomic state transitions.**

It is designed for scenarios where:
1. Multiple writers may compete for the same state
2. You need to validate current state before updating
3. Conflicts should be detected and retried automatically
4. Version history matters for debugging/audit

It is NOT:
- A general-purpose key-value store (use KVStore)
- An event log (use EventLog)
- A full state machine with transition guards (future: M5+)
- A distributed lock service (though it can implement locks)

It IS:
- A versioned CAS cell primitive
- The building block for coordination patterns
- A single-cell atomic update mechanism
- An optimistic concurrency control implementation

### What StateCell Captures

**Use StateCell for**:
- Workflow status (`pending` → `running` → `completed`)
- Leader election claims
- Distributed lock acquisition/release
- Configuration that needs atomic updates
- Counters with increment semantics
- Any state where "only update if unchanged" matters

**Do NOT use StateCell for**:
- Large datasets (use KVStore)
- Immutable history (use EventLog)
- General storage without coordination needs (use KVStore)
- High-frequency updates to same cell (contention limits ~100 threads)

### The Counter Model

Every StateCell has a **counter-based version**:

```
First write  → version = 1
Second write → version = 2
Third write  → version = 3
...
```

This is NOT transaction-based. Even within a single transaction, multiple writes to the same cell increment the counter.

```rust
// In one transaction:
state_set(run, "cell", Value::Int(1));  // version = 1
state_set(run, "cell", Value::Int(2));  // version = 2
state_set(run, "cell", Value::Int(3));  // version = 3
```

---

## Part 3: Strata's 7 Invariants Applied to StateCell

From PRIMITIVE_CONTRACT.md, every primitive must satisfy:

| Invariant | StateCell Compliance | Notes |
|-----------|---------------------|-------|
| I1: Addressable | Yes | run + cell_name |
| I2: Versioned | Yes | Version::Counter |
| I3: Transactional | Yes | Operations participate in transactions |
| I4: Lifecycle | Yes (CRUD) | init/read/set,cas/delete |
| I5: Run-scoped | Yes | All cells belong to a run |
| I6: Introspectable | Yes | exists, get, list |
| I7: Consistent R/W | Yes | Reads don't modify, writes produce versions |

### I3: Transactional Requirement

StateCell operations participate in transactions:

```rust
// CORRECT: Atomic coordination with other primitives
db.transaction(run_id, |txn| {
    let state = txn.state_get("workflow/status")?;
    if state.value == Value::String("pending".into()) {
        txn.state_cas("workflow/status", Some(state.version), Value::String("running".into()))?;
        txn.event_append("workflow", json!({"transition": "pending->running"}))?;
    }
    Ok(())
})?;
```

### Special Consideration: Counter vs Transaction Versioning

StateCell's counter increments **per write**, not per transaction:

```rust
// Transaction 1:
state_set(run, "a", Value::Int(1));  // a.version = 1
state_set(run, "b", Value::Int(1));  // b.version = 1
state_set(run, "a", Value::Int(2));  // a.version = 2 (NOT 1!)

// Transaction 2:
state_set(run, "a", Value::Int(3));  // a.version = 3
```

This differs from KVStore where version is transaction-based.

---

## Part 4: The Purity Requirement (CRITICAL)

### Closures MUST Be Pure

**This is non-negotiable and must be documented prominently.**

When using `transition()` or `transition_or_init()`, the closure may execute **multiple times** due to OCC retries.

```rust
// Closure signature
fn transition<F>(&self, run: &ApiRunId, cell: &str, f: F) -> StrataResult<(Value, Version)>
where
    F: Fn(&Value) -> StrataResult<Value> + Send + Sync;
```

**Pure Closure (CORRECT)**:
```rust
substrate.state_transition(&run, "counter", |current| {
    let n = current.as_i64().unwrap_or(0);
    Ok(Value::Int(n + 1))  // Deterministic: same input → same output
})?;
```

**Impure Closure (WRONG)**:
```rust
substrate.state_transition(&run, "counter", |current| {
    println!("Incrementing!");           // WRONG: I/O side effect
    external_counter.fetch_add(1, ...);  // WRONG: External mutation
    send_notification(...);              // WRONG: External I/O
    random_value();                      // WRONG: Nondeterministic
    Ok(Value::Int(42))
})?;
```

**Why This Matters**:
- Under contention, `transition()` may retry 1-200 times
- Each retry re-executes the closure
- Side effects would be multiplied (200 notifications!)
- Nondeterminism would cause different values per retry

**Safe Pattern**:
```rust
// Do side effects AFTER transition completes
let (new_value, version) = substrate.state_transition(&run, "status", |v| {
    Ok(Value::String("completed".into()))  // Pure
})?;

// NOW safe to do side effects
send_notification("Status changed to completed");
log::info!("Updated to version {}", version);
```

---

## Part 5: API Specification

### Tier 1: Core Operations (Must Have)

```rust
/// Set cell value unconditionally
/// Creates cell if it doesn't exist
/// Always increments counter (even if value unchanged)
fn state_set(
    &self,
    run: &ApiRunId,
    cell: &str,
    value: Value,
) -> StrataResult<Version>;

/// Get cell value with version
/// Returns None if cell doesn't exist
fn state_get(
    &self,
    run: &ApiRunId,
    cell: &str,
) -> StrataResult<Option<Versioned<Value>>>;

/// Compare-and-swap
/// expected_counter = None: create only if doesn't exist
/// expected_counter = Some(n): update only if counter == n
/// Returns Some(new_version) on success, None on mismatch
fn state_cas(
    &self,
    run: &ApiRunId,
    cell: &str,
    expected_counter: Option<u64>,
    value: Value,
) -> StrataResult<Option<Version>>;

/// Delete cell
/// Returns true if cell existed
fn state_delete(
    &self,
    run: &ApiRunId,
    cell: &str,
) -> StrataResult<bool>;

/// Check if cell exists
fn state_exists(
    &self,
    run: &ApiRunId,
    cell: &str,
) -> StrataResult<bool>;
```

### Tier 2: Initialization & Discovery

```rust
/// Initialize cell (only if doesn't exist)
/// Returns error if cell already exists
/// First version is always 1
fn state_init(
    &self,
    run: &ApiRunId,
    cell: &str,
    value: Value,
) -> StrataResult<Version>;

/// List all cell names in a run
fn state_list(
    &self,
    run: &ApiRunId,
) -> StrataResult<Vec<String>>;
```

### Tier 3: Transition Functions

```rust
/// Apply transition function with automatic retry
/// Closure may be called multiple times (MUST BE PURE)
/// Automatically retries on version mismatch
fn state_transition<F>(
    &self,
    run: &ApiRunId,
    cell: &str,
    f: F,
) -> StrataResult<(Value, Version)>
where
    F: Fn(&Value) -> StrataResult<Value> + Send + Sync;

/// Transition or initialize
/// If cell doesn't exist, initializes with `initial` then applies `f`
fn state_transition_or_init<F>(
    &self,
    run: &ApiRunId,
    cell: &str,
    initial: Value,
    f: F,
) -> StrataResult<(Value, Version)>
where
    F: Fn(&Value) -> StrataResult<Value> + Send + Sync;
```

### Tier 4: History (Should Have)

```rust
/// Get version history for a cell
/// Returns historical versions, newest first
fn state_history(
    &self,
    run: &ApiRunId,
    cell: &str,
    limit: Option<u64>,
    before: Option<Version>,
) -> StrataResult<Vec<Versioned<Value>>>;
```

---

## Part 6: Validation Requirements

### Input Validation

| Input | Constraint | Error |
|-------|------------|-------|
| cell name | Non-empty, no NUL bytes | `InvalidKey` |
| cell name | Max 1024 bytes | `InvalidKey` |
| value | Any Value type allowed | N/A |
| expected_counter | Must match if Some | Returns None (not error) |

### Semantic Constraints

| Constraint | Behavior |
|------------|----------|
| Counter monotonicity | Counter always increments on write |
| Atomic CAS | Compare and swap is single atomic operation |
| Init uniqueness | init() fails if cell exists |
| Delete idempotency | delete() returns false if not exists |

---

## Part 7: Use Case Patterns

### Leader Election

```rust
fn try_become_leader(substrate: &impl StateCell, run: &ApiRunId, my_id: &str) -> bool {
    // Try to claim leadership if no current leader
    match substrate.state_cas(run, "leader", None, Value::String(my_id.into())) {
        Ok(Some(_)) => true,   // I'm the leader
        Ok(None) => false,      // Someone else is leader
        Err(_) => false,        // Error (run closed, etc.)
    }
}

fn release_leadership(substrate: &impl StateCell, run: &ApiRunId, my_id: &str) -> bool {
    let current = substrate.state_get(run, "leader").ok().flatten();
    match current {
        Some(v) if v.value == Value::String(my_id.into()) => {
            // I'm the leader, try to release
            let counter = match v.version {
                Version::Counter(c) => c,
                _ => return false,
            };
            substrate.state_cas(run, "leader", Some(counter), Value::Null).ok().is_some()
        }
        _ => false,  // Not the leader
    }
}
```

### Workflow State Machine

```rust
fn advance_workflow(substrate: &impl StateCell, run: &ApiRunId) -> StrataResult<String> {
    let (new_value, _version) = substrate.state_transition(run, "workflow/status", |current| {
        match current.as_str() {
            Some("pending") => Ok(Value::String("running".into())),
            Some("running") => Ok(Value::String("completed".into())),
            Some("completed") => Err(StrataError::invalid_operation("Already completed")),
            _ => Err(StrataError::invalid_operation("Unknown state")),
        }
    })?;

    Ok(new_value.as_str().unwrap_or("unknown").to_string())
}
```

### Atomic Counter

```rust
fn increment_counter(substrate: &impl StateCell, run: &ApiRunId, cell: &str) -> StrataResult<i64> {
    let (new_value, _) = substrate.state_transition_or_init(
        run,
        cell,
        Value::Int(0),  // Initial value if doesn't exist
        |current| {
            let n = current.as_i64().unwrap_or(0);
            Ok(Value::Int(n + 1))
        },
    )?;

    Ok(new_value.as_i64().unwrap_or(0))
}
```

---

## Part 8: Performance Characteristics

| Operation | Complexity | Contention Behavior |
|-----------|-----------|---------------------|
| state_get | O(1) | No contention (read-only) |
| state_exists | O(1) | No contention (read-only) |
| state_set | O(1) | Low (unconditional write) |
| state_cas | O(1) | Medium (may fail on mismatch) |
| state_init | O(1) | Medium (may fail if exists) |
| state_delete | O(1) | Low |
| state_list | O(n) | Low (read-only) |
| state_transition | O(1) × retries | High under contention |

**Retry Configuration**:
- Max retries: 200
- Backoff: Exponential (1ms base, 50ms max)
- Handles: ~100 concurrent threads on same cell

**When to Avoid StateCell**:
- Very high write frequency to same cell (>1000/sec)
- Large values (prefer KVStore with chunking)
- No coordination needs (use KVStore)

---

## Part 9: Relationship to Other Primitives

### StateCell vs KVStore

| Scenario | Use StateCell | Use KVStore |
|----------|---------------|-------------|
| Workflow status | ✓ | |
| User preferences | | ✓ |
| Leader election | ✓ | |
| Session data | | ✓ |
| Distributed lock | ✓ | |
| Cache entries | | ✓ |
| Atomic counter | ✓ | |
| Document storage | | ✓ |

### StateCell + EventLog

```rust
// Coordinate state change and record it atomically
db.transaction(run_id, |txn| {
    // Record the external input
    txn.event_append("user_input", input.clone())?;

    // Update coordination state
    txn.state_cas("request/status", Some(1), Value::String("processing".into()))?;

    Ok(())
})?;
```

### StateCell + TraceStore

```rust
// Track reasoning while coordinating
db.transaction(run_id, |txn| {
    txn.trace_record(TraceType::Decision {
        reasoning: "Advancing workflow based on user confirmation"
    })?;

    txn.state_transition("workflow", |_| Ok(Value::String("approved".into())))?;

    Ok(())
})?;
```

---

## Part 10: Future Evolution

### M5+: Full StateMachine

The current StateCell is a versioned CAS cell. A future `StateMachine` primitive may add:

```rust
// Future API (not yet implemented)
pub trait StateMachine {
    /// Define allowed transitions
    fn define_transitions(
        &self,
        run: &ApiRunId,
        machine: &str,
        transitions: Vec<Transition>,
    ) -> StrataResult<()>;

    /// Transition with guard validation
    fn transition(
        &self,
        run: &ApiRunId,
        machine: &str,
        event: &str,
    ) -> StrataResult<State>;

    /// Check if in terminal state
    fn is_terminal(
        &self,
        run: &ApiRunId,
        machine: &str,
    ) -> StrataResult<bool>;
}
```

This would add:
- Defined state transition rules
- Guard conditions on transitions
- Terminal state enforcement
- Invalid transition prevention

---

## Summary

**StateCell's Role**: The coordination primitive for version-aware atomic state transitions.

**Key Differentiator from KVStore**: Cell-level version tracking with CAS semantics.

**Critical Requirement**: Closures passed to `transition()` MUST be pure (no side effects, deterministic).

**Primary Use Cases**:
1. Leader election
2. Distributed locks
3. Workflow state tracking
4. Atomic counters
5. Single-writer coordination

**Locked Decisions**:
1. Counter-based versioning (increments per write, not per transaction)
2. CAS returns None on mismatch (not error)
3. init() fails if exists (not upsert)
4. transition() retries automatically (up to 200 times)

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-23 | Initial draft |
