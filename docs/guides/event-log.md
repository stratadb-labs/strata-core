# Event Log Guide

The Event Log is an append-only sequence of typed events. Events are immutable once written — you cannot update or delete individual events. This makes it ideal for audit trails, tool call history, and decision logs.

## API Overview

| Method | Signature | Returns |
|--------|-----------|---------|
| `event_append` | `(event_type: &str, payload: Value) -> Result<u64>` | Sequence number |
| `event_read` | `(sequence: u64) -> Result<Option<VersionedValue>>` | Event at sequence |
| `event_read_by_type` | `(event_type: &str) -> Result<Vec<VersionedValue>>` | All events of type |
| `event_len` | `() -> Result<u64>` | Total event count |

## Appending Events

Each event has a **type** (a string label) and a **payload** (must be `Value::Object`):

```rust
let db = Strata::cache()?;

// Append with serde_json for ergonomic Object construction
let payload: Value = serde_json::json!({
    "tool": "web_search",
    "query": "rust embedded database",
    "results": 10
}).into();
let seq = db.event_append("tool_call", payload)?;
println!("Event recorded at sequence {}", seq);
```

### Payload Must Be Object

Event payloads must be `Value::Object`. This ensures events are structured and queryable:

```rust
use std::collections::HashMap;

// Using HashMap directly
let mut payload = HashMap::new();
payload.insert("action".to_string(), Value::String("login".into()));
payload.insert("user_id".to_string(), Value::Int(42));
db.event_append("auth", Value::Object(payload))?;

// Using serde_json::json! (more ergonomic)
let payload: Value = serde_json::json!({"action": "login", "user_id": 42}).into();
db.event_append("auth", payload)?;
```

## Reading Events

### By Sequence Number

Each event gets a unique sequence number (starting from 1):

```rust
let db = Strata::cache()?;

let seq = db.event_append("log", serde_json::json!({"msg": "hello"}).into())?;

let event = db.event_read(seq)?;
if let Some(versioned) = event {
    println!("Payload: {:?}", versioned.value);
    println!("Version: {}", versioned.version);
}
```

### By Event Type

Retrieve all events with a specific type label:

```rust
let db = Strata::cache()?;

db.event_append("tool_call", serde_json::json!({"tool": "search"}).into())?;
db.event_append("decision", serde_json::json!({"choice": "A"}).into())?;
db.event_append("tool_call", serde_json::json!({"tool": "calculator"}).into())?;

let tool_calls = db.event_read_by_type("tool_call")?;
assert_eq!(tool_calls.len(), 2);

let decisions = db.event_read_by_type("decision")?;
assert_eq!(decisions.len(), 1);
```

## Event Count

Get the total number of events in the current branch:

```rust
let db = Strata::cache()?;
assert_eq!(db.event_len()?, 0);

db.event_append("log", serde_json::json!({"msg": "one"}).into())?;
db.event_append("log", serde_json::json!({"msg": "two"}).into())?;
assert_eq!(db.event_len()?, 2);
```

## Common Patterns

### Audit Trail

```rust
fn log_tool_call(db: &Strata, tool: &str, input: &str, output: &str) -> stratadb::Result<u64> {
    let payload: Value = serde_json::json!({
        "tool": tool,
        "input": input,
        "output": output,
    }).into();
    db.event_append("tool_call", payload)
}
```

### Decision Log

```rust
fn log_decision(db: &Strata, decision: &str, reason: &str, confidence: f64) -> stratadb::Result<u64> {
    let payload: Value = serde_json::json!({
        "decision": decision,
        "reason": reason,
        "confidence": confidence,
    }).into();
    db.event_append("decision", payload)
}
```

## Branch Isolation

Events are isolated by branch. `event_len()` returns 0 in a new branch even if other branches have events:

```rust
let mut db = Strata::cache()?;
db.event_append("log", serde_json::json!({"msg": "in default"}).into())?;

db.create_branch("other")?;
db.set_branch("other")?;
assert_eq!(db.event_len()?, 0); // Isolated
```

## Space Isolation

Within a branch, events are scoped to the current space:

```rust
let mut db = Strata::cache()?;

db.event_append("log", serde_json::json!({"msg": "in default space"}).into())?;
assert_eq!(db.event_len()?, 1);

db.set_space("other")?;
assert_eq!(db.event_len()?, 0); // separate event stream per space
```

See [Spaces](spaces.md) for the full guide.

## Transactions

Event append operations participate in transactions. Within a session, appended events are only visible after commit.

See [Sessions and Transactions](sessions-and-transactions.md) for details.

## Next

- [State Cell](state-cell.md) — mutable state with CAS
- [Cookbook: Agent State Management](../cookbook/agent-state-management.md) — combining events with other primitives
