# Spaces Guide

Spaces are an organizational layer within branches. Each branch contains one or more spaces, and each space has its own independent instance of every primitive (KV, Event, State, JSON, Vector). Think of spaces like schemas in PostgreSQL — they organize data within a database (branch) without creating full isolation boundaries.

## API Overview

| Method | Signature | Returns |
|--------|-----------|---------|
| `set_space` | `(name: &str) -> Result<()>` | Switches current space |
| `current_space` | `() -> &str` | Current space name |
| `list_spaces` | `() -> Result<Vec<String>>` | All space names in current branch |
| `delete_space` | `(name: &str) -> Result<()>` | Deletes empty space |
| `delete_space_force` | `(name: &str) -> Result<()>` | Deletes space and all its data |

## Default Space

Every branch starts with a `default` space. When you open a database, all operations target this space automatically. You never need to create or switch to it explicitly.

```rust
let db = Strata::cache()?;

// These go to the "default" space
db.kv_put("key", "value")?;
db.event_append("log", serde_json::json!({"msg": "hello"}).into())?;

assert_eq!(db.current_space(), "default");
```

The `default` space cannot be deleted.

## Creating and Switching Spaces

Use `set_space` to switch to a space. Spaces are auto-registered on first write — no explicit create step is needed:

```rust
let mut db = Strata::cache()?;

// Switch to a new space (auto-created on first data write)
db.set_space("conversations")?;
db.kv_put("msg_001", "hello")?;     // creates "conversations" space

// Switch to another space
db.set_space("tool-results")?;
db.kv_put("run_42", "done")?;       // creates "tool-results" space

// List all spaces
let spaces = db.list_spaces()?;
// → ["conversations", "default", "tool-results"]
```

You can also create a space explicitly using the `SpaceCreate` command without writing data.

## Data Isolation Between Spaces

Each space has its own independent data. The same key in different spaces refers to different values:

```rust
let mut db = Strata::cache()?;

// Write in default space
db.kv_put("config", "default-config")?;

// Switch to another space
db.set_space("experiments")?;
db.kv_put("config", "experiment-config")?;

// Data is separate
db.set_space("default")?;
assert_eq!(db.kv_get("config")?, Some(Value::String("default-config".into())));

db.set_space("experiments")?;
assert_eq!(db.kv_get("config")?, Some(Value::String("experiment-config".into())));
```

This applies to all primitives — events, state cells, JSON documents, and vector collections are all space-scoped.

## Cross-Space Transactions

Transactions can span multiple spaces within the same branch. This is useful when you need atomic operations across organizational boundaries:

```rust
let session = db.session();
session.begin()?;

session.set_space("billing")?;
session.kv_put("credits", credits - 1)?;

session.set_space("api-logs")?;
session.event_append("api_call", serde_json::json!({"endpoint": "/search"}).into())?;

session.commit()?; // atomic across both spaces
```

## Space Naming Rules

Space names follow these conventions:

| Rule | Details |
|------|---------|
| **Start with** | Lowercase letter `[a-z]` |
| **Allowed characters** | Lowercase letters, digits, hyphens, underscores `[a-z0-9_-]` |
| **Max length** | 64 characters |
| **Reserved prefix** | `_system_` (reserved for internal use) |
| **Reserved name** | `default` (cannot be deleted) |

```rust
// Valid names
db.set_space("conversations")?;       // ✓
db.set_space("tool-results")?;        // ✓
db.set_space("agent_memory_v2")?;     // ✓

// Invalid names
db.set_space("Conversations")?;       // ✗ uppercase
db.set_space("123-invalid")?;         // ✗ starts with digit
db.set_space("")?;                    // ✗ empty
db.set_space("_system_internal")?;    // ✗ reserved prefix
```

## Deleting Spaces

Delete a space with `delete_space` (must be empty) or `delete_space_force` (deletes all data):

```rust
let mut db = Strata::cache()?;

// Create and populate a space
db.set_space("temp")?;
db.kv_put("key", "value")?;

// Can't delete non-empty space without force
db.set_space("default")?;
// db.delete_space("temp")?;           // Error: space is non-empty

// Force delete removes all data
db.delete_space_force("temp")?;        // OK — deletes space and all its data

// Cannot delete the default space
// db.delete_space("default")?;        // Error: cannot delete default space
```

## Multi-Threaded Usage

Each handle tracks its own current space. Use `new_handle()` for independent space context per thread:

```rust
let db = Strata::open("/data/app")?;

let handle_a = db.new_handle()?;
let handle_b = db.new_handle()?;

std::thread::spawn(move || {
    handle_a.set_space("conversations").unwrap();
    handle_a.kv_put("msg_001", "hello").unwrap();
});

std::thread::spawn(move || {
    handle_b.set_space("tool-results").unwrap();
    handle_b.kv_put("run_42", "done").unwrap();
});
// No interference — each handle has its own active space.
```

New handles always start on the `default` space.

## Common Patterns

### Agent Memory Organization

```rust
let mut db = Strata::open("/data/agent")?;

// Conversation history
db.set_space("conversations")?;
db.event_append("user_message", serde_json::json!({"content": "What's the weather?"}).into())?;
db.event_append("tool_call", serde_json::json!({"tool": "weather_api"}).into())?;

// Tool results
db.set_space("tool-results")?;
db.kv_put("weather_api:run_1", serde_json::json!({"temp": 72, "conditions": "sunny"}))?;

// User preferences
db.set_space("user-context")?;
db.state_set("preferences", serde_json::json!({"units": "fahrenheit"}))?;
```

### Multi-Tenant Data

```rust
let mut db = Strata::open("/data/app")?;

for tenant in ["acme-corp", "globex", "initech"] {
    db.set_space(tenant)?;
    db.kv_put("config", serde_json::json!({"plan": "enterprise"}))?;
    db.vector_create_collection("docs", 384, DistanceMetric::Cosine)?;
}

let spaces = db.list_spaces()?;
// → ["acme-corp", "default", "globex", "initech"]
```

### Experiment Tracking

```rust
let mut db = Strata::open("/data/ml")?;

// Baseline parameters
db.set_space("hyperparams")?;
db.kv_put("config", serde_json::json!({"lr": 0.001, "epochs": 10}))?;

// Results per experiment run
db.set_space("run-001")?;
db.kv_put("metrics", serde_json::json!({"loss": 0.42, "accuracy": 0.87}))?;

db.set_space("run-002")?;
db.kv_put("metrics", serde_json::json!({"loss": 0.38, "accuracy": 0.89}))?;
```

## Spaces vs Branches

| | Branches | Spaces |
|--|----------|--------|
| **Purpose** | Isolation | Organization |
| **Data visibility** | Fully isolated | Fully visible within branch |
| **Transactions** | Cannot span branches | Can span spaces |
| **Analogy** | Git branches | PostgreSQL schemas |
| **Use case** | Separate experiments, sessions | Organize data within a session |

Use branches when you need full data isolation. Use spaces when you need to organize related data within a single branch.

## Next

- [KV Store](kv-store.md) — key-value operations
- [Branch Management](branch-management.md) — branch isolation
- [Sessions and Transactions](sessions-and-transactions.md) — cross-space atomicity
