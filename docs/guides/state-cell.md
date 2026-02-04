# State Cell Guide

State cells provide mutable, named values with **compare-and-swap (CAS)** for safe concurrent coordination. Use them for counters, locks, state machines, and any value that multiple writers need to update safely.

## API Overview

| Method | Signature | Returns |
|--------|-----------|---------|
| `state_set` | `(cell: &str, value: impl Into<Value>) -> Result<u64>` | Version number |
| `state_read` | `(cell: &str) -> Result<Option<Value>>` | Current value, or None |
| `state_init` | `(cell: &str, value: impl Into<Value>) -> Result<u64>` | Version number |
| `state_cas` | `(cell: &str, expected_counter: Option<u64>, value: impl Into<Value>) -> Result<Option<u64>>` | New version, or None on mismatch |

## Set (Unconditional Write)

`state_set` overwrites the cell value regardless of its current state:

```rust
let db = Strata::cache()?;

db.state_set("status", "active")?;
db.state_set("counter", 0i64)?;

let status = db.state_read("status")?;
assert_eq!(status, Some(Value::String("active".into())));
```

## Read

`state_read` returns the current value, or `None` if the cell doesn't exist:

```rust
let db = Strata::cache()?;

assert_eq!(db.state_read("missing")?, None);

db.state_set("cell", 42i64)?;
assert_eq!(db.state_read("cell")?, Some(Value::Int(42)));
```

## Init (Create If Absent)

`state_init` sets the value only if the cell does not already exist. This is idempotent — calling it multiple times with different values has no effect after the first call:

```rust
let db = Strata::cache()?;

// First init creates the cell
db.state_init("status", "idle")?;
assert_eq!(db.state_read("status")?, Some(Value::String("idle".into())));

// Second init is a no-op — value unchanged
db.state_init("status", "should-not-overwrite")?;
assert_eq!(db.state_read("status")?, Some(Value::String("idle".into())));
```

## Compare-and-Swap (CAS)

`state_cas` updates a cell only if the current version counter matches the expected value. This prevents lost updates when multiple writers are competing:

```rust
let db = Strata::cache()?;

// Create the cell (version 1)
let v1 = db.state_set("lock", "free")?;

// CAS: update only if at version v1
let new_version = db.state_cas("lock", Some(v1), "acquired")?;
assert!(new_version.is_some()); // Succeeded

// CAS with stale version fails
let failed = db.state_cas("lock", Some(v1), "stolen")?;
// The cell is now at a newer version, so the old version doesn't match
```

### CAS for Create-If-Absent

Pass `None` as the expected counter to create a cell only if it doesn't exist:

```rust
let db = Strata::cache()?;

// Creates the cell because it doesn't exist
let version = db.state_cas("new-cell", None, "initial")?;
assert!(version.is_some());
```

## Common Patterns

### State Machine

```rust
let db = Strata::cache()?;

// Initialize state
let v = db.state_set("task:status", "pending")?;

// Transition: pending → running (only if still pending)
let v = db.state_cas("task:status", Some(v), "running")?;

// Transition: running → completed
if let Some(v) = v {
    db.state_cas("task:status", Some(v), "completed")?;
}
```

### Simple Lock

```rust
let db = Strata::cache()?;

// Try to acquire lock (create-if-absent)
let result = db.state_cas("lock:resource", None, "owner-1")?;
if result.is_some() {
    println!("Lock acquired");
    // ... do work ...
    // Release by setting to a known "free" value
    db.state_set("lock:resource", "free")?;
}
```

### Counter

```rust
let db = Strata::cache()?;

db.state_set("counter", 0i64)?;

// Increment with CAS loop
let current = db.state_read("counter")?.unwrap();
let count = current.as_int().unwrap();
// In a real application, you'd retry on CAS failure
```

## Branch Isolation

State cells are isolated by branch, like all primitives.

## Space Isolation

Within a branch, state cells are scoped to the current space:

```rust
let mut db = Strata::cache()?;

db.state_set("status", "active")?;

db.set_space("other")?;
assert_eq!(db.state_read("status")?, None); // separate state per space
```

See [Spaces](spaces.md) for the full guide.

## Transactions

State cell operations (read, init, CAS) participate in transactions.

See [Sessions and Transactions](sessions-and-transactions.md) for details.

## Next

- [JSON Store](json-store.md) — structured documents
- [Cookbook: Multi-Agent Coordination](../cookbook/multi-agent-coordination.md) — CAS patterns for agent coordination
