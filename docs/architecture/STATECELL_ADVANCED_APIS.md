# StateCell: Advanced APIs Analysis

> **Purpose**: Detailed analysis of potential StateCell APIs to inform MVP scoping decisions.
> **Date**: 2026-01-23
> **Updated**: 2026-01-23 - `state_get_or_init` implemented with lazy default pattern

---

## Current State

StateCell currently provides **12 methods**:

| Category | Methods |
|----------|---------|
| Core CRUD | `state_set`, `state_get`, `state_delete`, `state_exists` |
| CAS | `state_cas` |
| Initialization | `state_init`, `state_get_or_init` ✅ |
| Discovery | `state_list` |
| Transitions | `state_transition`, `state_transition_or_init` |
| History | `state_history` |

This document analyzes **convenience APIs** and **advanced coordination features** that could extend StateCell.

---

## Part 1: Convenience APIs

These are quality-of-life improvements that simplify common patterns. They don't add new capabilities—users can achieve the same results with existing APIs, but with more code.

---

### 1.1 `state_get_or_init` - Get Existing or Create with Default ✅ IMPLEMENTED

#### Status: IMPLEMENTED (MVP)

Returns the current value if the cell exists, otherwise creates it with a default value and returns that.

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

#### Key Design: Lazy Default

Per user feedback, the default uses `FnOnce() -> Value` for **lazy evaluation**:
- The default closure is only called if the cell doesn't exist
- Avoids allocating defaults on the hot path when reading existing cells
- More ergonomic for expensive default construction

```rust
// Expensive default only computed if cell doesn't exist
let state = substrate.state_get_or_init(&run, "config", || {
    compute_expensive_default()  // Only called if cell is missing
})?;
```

#### Problem It Solves

The "get or create" pattern is extremely common:

```rust
// WITHOUT state_get_or_init (verbose):
let value = match substrate.state_get(&run, "config")? {
    Some(v) => v,
    None => {
        substrate.state_init(&run, "config", default.clone())?;
        substrate.state_get(&run, "config")?.unwrap()
    }
};

// WITH state_get_or_init (clean):
let value = substrate.state_get_or_init(&run, "config", || default)?;
```

#### Use Cases

1. **Configuration with defaults**: Get user config, create with defaults if first access
2. **Lazy initialization**: Initialize resources on first use
3. **Counter initialization**: Get counter, start at 0 if new
4. **Session state**: Get session data, create empty session if none

#### Race Condition Analysis

This is **safe** because:
- If two threads race, one `init` succeeds, one fails (already exists)
- Both threads then `get` the same value
- No data loss or corruption possible

#### Test Coverage

8 tests in `statecell/basic_ops.rs`:
- `test_state_get_or_init_existing_value`
- `test_state_get_or_init_creates_new_cell`
- `test_state_get_or_init_lazy_default`
- `test_state_get_or_init_preserves_version`
- `test_state_get_or_init_expensive_default`
- `test_state_get_or_init_all_value_types`
- `test_state_get_or_init_run_isolation`
- `test_state_get_or_init_after_delete`

---

### 1.2 `state_info` - Cell Metadata Without Value

#### What It Does

Returns metadata about a cell (version, timestamp, exists) without reading the full value.

```rust
struct CellInfo {
    exists: bool,
    version: Option<u64>,      // None if doesn't exist
    updated_at: Option<i64>,   // None if doesn't exist
}

fn state_info(
    &self,
    run: &ApiRunId,
    cell: &str,
) -> StrataResult<CellInfo>;
```

#### Problem It Solves

Sometimes you only need metadata, not the value:

```rust
// WITHOUT state_info (reads entire value):
let versioned = substrate.state_get(&run, "large_blob")?;
let version = versioned.map(|v| v.version);
let exists = versioned.is_some();
// Problem: We read potentially megabytes just to check version!

// WITH state_info:
let info = substrate.state_info(&run, "large_blob")?;
if info.version == Some(expected_version) {
    // Only now read the full value
    let value = substrate.state_get(&run, "large_blob")?;
}
```

#### Use Cases

1. **Staleness checks**: Is my cached copy still current?
2. **Conditional fetch**: Only download if version changed
3. **Monitoring**: Track when cells were last updated
4. **Existence with metadata**: More info than `exists()` alone

#### Implementation Complexity

**Low** - Can be implemented by reading the State struct and extracting metadata:
```rust
fn state_info(&self, run: &ApiRunId, cell: &str) -> StrataResult<CellInfo> {
    match self.state_get(run, cell)? {
        Some(v) => Ok(CellInfo {
            exists: true,
            version: Some(match v.version {
                Version::Counter(c) => c,
                _ => 0,
            }),
            updated_at: Some(v.timestamp.as_micros() as i64),
        }),
        None => Ok(CellInfo {
            exists: false,
            version: None,
            updated_at: None,
        }),
    }
}
```

#### Performance Note

Current implementation would still read the full value internally. True O(1) metadata-only access would require storage layer changes to store metadata separately.

#### Recommendation

**Defer** - The current `state_get` returns `Versioned<Value>` which includes version and timestamp. The only benefit of `state_info` is avoiding value deserialization for large values, which requires storage layer changes. Not worth it for MVP.

---

## Part 2: Advanced Coordination Features

These add **new capabilities** that cannot be achieved with existing APIs. They are essential for production-grade distributed coordination but require significant implementation effort.

---

### 2.1 Fencing Tokens - Distributed Lock Correctness

#### What It Does

Provides monotonically increasing tokens when acquiring locks, enabling safe coordination even with clock skew and process pauses.

```rust
struct LockGrant {
    holder_id: String,
    fence_token: u64,      // Monotonically increasing across ALL lock acquisitions
    acquired_at: i64,
    expires_at: Option<i64>,
}

fn state_acquire_lock(
    &self,
    run: &ApiRunId,
    cell: &str,
    holder_id: &str,
    ttl_ms: Option<u64>,
) -> StrataResult<Option<LockGrant>>;  // None if already held

fn state_release_lock(
    &self,
    run: &ApiRunId,
    cell: &str,
    holder_id: &str,
) -> StrataResult<bool>;

fn state_refresh_lock(
    &self,
    run: &ApiRunId,
    cell: &str,
    holder_id: &str,
    ttl_ms: u64,
) -> StrataResult<Option<LockGrant>>;
```

#### Problem It Solves

**The Paused Client Problem**:

```
Timeline:
─────────────────────────────────────────────────────────────────────────►

Client A: [Acquire lock] ───────[GC PAUSE 30s]─────────► [Thinks it has lock!]
                         │                               │
Lock:     [A holds]──────┼──[TTL expires]──[B acquires]──┼──[B holds]
                         │                               │
Client B:                                [Acquire lock]──┼──► [Has lock]
                                                         │
                                                    DATA CORRUPTION
                                                    (Both write!)
```

Without fencing tokens, Client A wakes up thinking it still has the lock and writes to the shared resource, corrupting Client B's work.

**With Fencing Tokens**:

```rust
// Client A acquires lock
let grant_a = substrate.state_acquire_lock(&run, "resource", "client-a", Some(10000))?;
// grant_a.fence_token = 42

// Client A pauses (GC, network, etc.)...

// Lock expires, Client B acquires
let grant_b = substrate.state_acquire_lock(&run, "resource", "client-b", Some(10000))?;
// grant_b.fence_token = 43  (ALWAYS higher than previous)

// Client A wakes up, tries to write to storage
storage.write_with_fence(data, grant_a.fence_token);  // fence=42
// Storage REJECTS: "Fence token 42 < current fence 43"
```

The storage system (or any downstream service) remembers the highest fence token it has seen and rejects operations with lower tokens.

#### Use Cases

1. **Leader election**: Only the true leader can write
2. **Distributed locks**: Safe mutual exclusion
3. **Job scheduling**: Prevent duplicate job execution
4. **Resource allocation**: Ensure single writer to shared state

#### Implementation Complexity

**Medium-High**:
- Need global fence token counter (not per-cell)
- Need to track lock holder + expiration
- Need background task for TTL expiration
- Need to expose fence token to downstream systems

#### Industry Precedent

- **Google Chubby**: Sequence numbers on locks
- **ZooKeeper**: zxid on ephemeral nodes
- **etcd**: Revision numbers with leases
- **Martin Kleppmann**: Chapter 8 of "Designing Data-Intensive Applications"

#### Recommendation

**Defer to M5+** - Critical for production distributed systems, but requires TTL infrastructure and careful design. Not needed for single-process use cases.

---

### 2.2 Multi-Cell Transactions - Atomic Cross-Cell Operations

#### What It Does

Atomically read and write multiple cells in a single operation.

```rust
// Conditional transaction (etcd-style)
struct CellCondition {
    cell: String,
    condition: Condition,
}

enum Condition {
    Exists,
    NotExists,
    VersionEquals(u64),
    ValueEquals(Value),
}

struct CellOp {
    cell: String,
    op: Operation,
}

enum Operation {
    Get,
    Set(Value),
    Delete,
    Cas { expected: u64, value: Value },
}

fn state_txn(
    &self,
    run: &ApiRunId,
    conditions: Vec<CellCondition>,  // If ALL true...
    success_ops: Vec<CellOp>,        // ...execute these
    failure_ops: Vec<CellOp>,        // ...else execute these
) -> StrataResult<TxnResult>;
```

#### Problem It Solves

**Atomic Multi-Cell Updates**:

```rust
// WITHOUT multi-cell transactions (UNSAFE):
let vote1 = substrate.state_get(&run, "node1/vote")?;
let vote2 = substrate.state_get(&run, "node2/vote")?;
// ⚠️ RACE: votes could change between reads!
if all_committed(&[vote1, vote2]) {
    substrate.state_set(&run, "decision", Value::String("COMMIT"))?;
    // ⚠️ RACE: crash here = inconsistent state
    substrate.state_set(&run, "node1/status", Value::String("COMMITTED"))?;
    substrate.state_set(&run, "node2/status", Value::String("COMMITTED"))?;
}

// WITH multi-cell transactions (SAFE):
let result = substrate.state_txn(
    &run,
    vec![
        CellCondition { cell: "node1/vote".into(), condition: Condition::ValueEquals(Value::String("YES".into())) },
        CellCondition { cell: "node2/vote".into(), condition: Condition::ValueEquals(Value::String("YES".into())) },
    ],
    vec![  // If all YES
        CellOp { cell: "decision".into(), op: Operation::Set(Value::String("COMMIT".into())) },
        CellOp { cell: "node1/status".into(), op: Operation::Set(Value::String("COMMITTED".into())) },
        CellOp { cell: "node2/status".into(), op: Operation::Set(Value::String("COMMITTED".into())) },
    ],
    vec![],  // Else do nothing
)?;
```

#### Use Cases

1. **Two-phase commit**: Coordinate distributed transactions
2. **Workflow orchestration**: Update multiple state machines atomically
3. **Resource allocation**: Reserve multiple resources or none
4. **Configuration updates**: Update related config keys together

#### Implementation Complexity

**High**:
- Need to hold locks on multiple cells simultaneously
- Need to handle deadlock detection/prevention
- Need atomic commit across cells
- Complex API surface

#### Industry Precedent

- **etcd**: Txn API with if/then/else
- **ZooKeeper**: multi() operation
- **DynamoDB**: TransactWriteItems
- **Redis**: MULTI/EXEC

#### Recommendation

**Defer to M5+** - Very powerful but complex. Current users can use the existing Database transaction API which already supports multi-primitive operations within a run.

**Note**: Strata already has cross-primitive transactions via `db.transaction()`:
```rust
db.transaction(run_id, |txn| {
    txn.state_cas("cell1", 1, Value::Int(1))?;
    txn.state_cas("cell2", 1, Value::Int(2))?;
    txn.kv_put("key", Value::String("value"))?;
    Ok(())
})?;
```

This covers most use cases. The `state_txn` API would add conditional logic (if/then/else) which is a nice-to-have.

---

### 2.3 Watch/Subscribe - Change Notification

#### What It Does

Subscribe to changes on a cell and receive notifications when it changes.

```rust
// Blocking watch (waits for change)
fn state_watch(
    &self,
    run: &ApiRunId,
    cell: &str,
    from_version: Option<u64>,  // Watch for changes after this version
    timeout_ms: u64,
) -> StrataResult<Option<WatchEvent>>;

struct WatchEvent {
    cell: String,
    old_value: Option<Value>,
    new_value: Option<Value>,  // None = deleted
    version: u64,
    timestamp: i64,
}

// Streaming watch (async)
fn state_watch_stream(
    &self,
    run: &ApiRunId,
    cell: &str,
    from_version: Option<u64>,
) -> StrataResult<impl Stream<Item = WatchEvent>>;
```

#### Problem It Solves

**Efficient Change Detection**:

```rust
// WITHOUT watch (polling - INEFFICIENT):
loop {
    let current = substrate.state_get(&run, "config")?;
    if current.version != last_seen_version {
        handle_config_change(current);
        last_seen_version = current.version;
    }
    thread::sleep(Duration::from_millis(100));  // Wastes resources!
}

// WITH watch (event-driven - EFFICIENT):
loop {
    match substrate.state_watch(&run, "config", Some(last_version), 30000)? {
        Some(event) => {
            handle_config_change(event.new_value);
            last_version = event.version;
        }
        None => {
            // Timeout, no changes - just retry
        }
    }
}
```

#### Use Cases

1. **Configuration updates**: React to config changes immediately
2. **Leader election**: Know instantly when leadership changes
3. **Job queue**: Wake worker when new job arrives
4. **Cache invalidation**: Invalidate cache when source changes
5. **UI updates**: Push changes to connected clients

#### Implementation Complexity

**High**:
- Need notification infrastructure (channels, callbacks)
- Need to track watchers per cell
- Need to handle watcher cleanup on disconnect
- Need to handle high fanout (many watchers on one cell)
- Streaming API requires async runtime integration

#### Industry Precedent

- **ZooKeeper**: Watches (one-time triggers)
- **etcd**: Watch API (streaming)
- **Consul**: Blocking queries
- **Redis**: Pub/Sub, Keyspace notifications

#### Recommendation

**Defer to M5+** - Requires significant infrastructure. Current users can poll with `state_get` which works for many use cases. Watch becomes important at scale or for real-time requirements.

---

### 2.4 TTL/Lease - Ephemeral Cells

#### What It Does

Cells that automatically expire after a time-to-live (TTL) or when a lease expires.

```rust
// TTL-based expiration
fn state_set_with_ttl(
    &self,
    run: &ApiRunId,
    cell: &str,
    value: Value,
    ttl_ms: u64,
) -> StrataResult<Version>;

fn state_refresh_ttl(
    &self,
    run: &ApiRunId,
    cell: &str,
    ttl_ms: u64,
) -> StrataResult<bool>;  // false if cell doesn't exist

// Lease-based expiration (more powerful)
fn state_create_lease(
    &self,
    run: &ApiRunId,
    ttl_ms: u64,
) -> StrataResult<LeaseId>;

fn state_refresh_lease(
    &self,
    run: &ApiRunId,
    lease: &LeaseId,
) -> StrataResult<()>;

fn state_set_with_lease(
    &self,
    run: &ApiRunId,
    cell: &str,
    value: Value,
    lease: &LeaseId,
) -> StrataResult<Version>;
// Cell auto-deleted when lease expires or is revoked
```

#### Problem It Solves

**Automatic Cleanup on Failure**:

```rust
// WITHOUT TTL (manual cleanup required):
fn hold_lock(substrate: &impl StateCell, run: &ApiRunId) {
    substrate.state_set(&run, "lock", Value::String("holder-1".into()))?;

    // Do work...

    substrate.state_delete(&run, "lock")?;  // Must remember to cleanup!
    // ⚠️ If process crashes, lock is held FOREVER
}

// WITH TTL (automatic cleanup):
fn hold_lock(substrate: &impl StateCell, run: &ApiRunId) {
    substrate.state_set_with_ttl(&run, "lock", Value::String("holder-1".into()), 30000)?;

    // Refresh TTL periodically while working
    loop {
        if !do_some_work() { break; }
        substrate.state_refresh_ttl(&run, "lock", 30000)?;
    }

    substrate.state_delete(&run, "lock")?;
    // Even if we crash, lock expires in 30 seconds
}
```

#### Use Cases

1. **Ephemeral locks**: Locks that auto-release on holder crash
2. **Session management**: Sessions that expire on client disconnect
3. **Heartbeat patterns**: "I'm alive" signals that expire
4. **Temporary data**: Cache entries, rate limit windows
5. **Leader election**: Leadership that transfers on failure

#### Implementation Complexity

**Medium-High**:
- Need background expiration task
- Need efficient TTL index (sorted by expiration time)
- Need to handle clock skew
- Lease API needs lease tracking and revocation

#### Industry Precedent

- **ZooKeeper**: Ephemeral nodes (session-bound)
- **etcd**: Leases with TTL
- **Redis**: EXPIRE, TTL commands
- **Consul**: Session-bound KV entries

#### Recommendation

**Defer to M5+** - While TTL appears simple, it introduces fundamental complexity:

1. **Time as correctness input**: TTL makes time a first-class concern for correctness
2. **Background infrastructure**: Requires schedulers for expiration tasks
3. **Determinism implications**: How does TTL interact with replay? Event sourcing?
4. **Design coupling**: TTL and fencing tokens share time-based semantics and should be designed together

Per user feedback: "TTL looks simple, but it quietly introduces time as a first-class correctness input, background schedulers, expiration races vs concurrent updates, clock skew questions, and interaction with replay semantics."

TTL will be designed alongside fencing tokens in M5+ to ensure a coherent approach to time-based coordination.

---

## Part 3: Summary & Recommendations

### MVP Status

| API | Effort | Value | Status |
|-----|--------|-------|--------|
| `state_get_or_init` | Low | High | ✅ **IMPLEMENTED** (lazy default) |
| `state_info` | Low | Low | ❌ Deferred (current API sufficient) |
| Fencing Tokens | High | High | ❌ Deferred to M5+ |
| Multi-Cell Txn | High | Medium | ❌ Deferred (existing txn API covers most cases) |
| Watch/Subscribe | High | High | ❌ Deferred to M5+ |
| TTL (basic) | Medium | High | ❌ **Deferred** (see note below) |

### TTL Decision: NOT in MVP

Per user feedback, TTL is **intentionally excluded** from MVP because it introduces:

1. **Time as first-class correctness input** - Complicates deterministic behavior
2. **Background schedulers** - Requires infrastructure for expiration tasks
3. **Expiration races** - Complex interaction with concurrent updates
4. **Clock skew questions** - Distributed time synchronization issues
5. **Replay semantics** - How does TTL interact with event replay?

TTL should be designed **alongside fencing tokens**, not before. Both involve time-based coordination semantics and should share a coherent design.

### Implementation Priority

**Phase 1 (MVP) - COMPLETE**:
1. ✅ `state_get_or_init` with lazy default

**Phase 2 (M5+)**:
- Watch/Subscribe
- Fencing Tokens + TTL/Leases (designed together)
- Full Lease API
- Multi-Cell Transactions

### Design Philosophy

StateCell is a **run-scoped, OCC-protected, deterministic coordination primitive** - NOT a distributed lock service like etcd or ZooKeeper.

The MVP focus is on:
- **Correctness**: OCC transitions with automatic retry
- **Simplicity**: Clean API without time-based complexity
- **Composability**: Works well with existing Database transactions

Advanced features (TTL, fencing, watches) will be designed holistically in M5+ to ensure they don't compromise these core properties.

---

## Appendix: Workarounds for Deferred Features

### TTL (without dedicated API)
```rust
// Store expiration in value
let expires_at = now() + ttl_ms;
substrate.state_set(&run, cell, json!({
    "value": actual_value,
    "expires_at": expires_at,
}))?;

// Check on read
let stored = substrate.state_get(&run, cell)?;
if stored.expires_at < now() {
    substrate.state_delete(&run, cell)?;
    return None;
}
```

### Watch (without dedicated API)
```rust
// Polling loop
let mut last_version = 0;
loop {
    let current = substrate.state_get(&run, cell)?;
    if let Some(v) = current {
        if v.version != last_version {
            handle_change(v);
            last_version = v.version;
        }
    }
    thread::sleep(poll_interval);
}
```
