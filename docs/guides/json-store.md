# JSON Store Guide

The JSON Store holds structured documents that you can read and write at specific JSON paths. Instead of replacing an entire document to change one field, you can update just `$.config.temperature`.

## API Overview

| Method | Signature | Returns |
|--------|-----------|---------|
| `json_set` | `(key: &str, path: &str, value: impl Into<Value>) -> Result<u64>` | Version number |
| `json_get` | `(key: &str, path: &str) -> Result<Option<Value>>` | Value at path, or None |
| `json_delete` | `(key: &str, path: &str) -> Result<u64>` | Count deleted |
| `json_list` | `(prefix: Option<String>, cursor: Option<String>, limit: u64) -> Result<(Vec<String>, Option<String>)>` | Keys + next cursor |

## Creating Documents

Use `json_set` with the root path `"$"` to create a document:

```rust
let db = Strata::cache()?;

let doc: Value = serde_json::json!({
    "model": "gpt-4",
    "temperature": 0.7,
    "settings": {
        "max_tokens": 1000,
        "stream": true
    }
}).into();
db.json_set("config", "$", doc)?;
```

## Reading Documents

### Read the Whole Document

```rust
let doc = db.json_get("config", "$")?;
if let Some(value) = doc {
    println!("Config: {:?}", value);
}
```

### Read a Nested Path

```rust
let temp = db.json_get("config", "$.temperature")?;
assert_eq!(temp, Some(Value::Float(0.7)));

let max_tokens = db.json_get("config", "$.settings.max_tokens")?;
assert_eq!(max_tokens, Some(Value::Int(1000)));
```

### Path Syntax

Paths use a simple dot notation starting with `$`:

| Path | Selects |
|------|---------|
| `$` | The root document |
| `$.name` | Top-level field "name" |
| `$.settings.max_tokens` | Nested field |

## Updating Fields

Update a specific path without touching the rest of the document:

```rust
// Change temperature
db.json_set("config", "$.temperature", 0.9)?;

// Add a new field
db.json_set("config", "$.version", "2.0")?;

// Update nested field
db.json_set("config", "$.settings.stream", false)?;
```

## Deleting

### Delete a Field

```rust
db.json_delete("config", "$.deprecated_field")?;
```

### Delete the Entire Document

```rust
db.json_delete("config", "$")?;
assert!(db.json_get("config", "$")?.is_none());
```

## Listing Documents

List documents with cursor-based pagination:

```rust
let db = Strata::cache()?;

db.json_set("user:1", "$", serde_json::json!({"name": "Alice"}).into())?;
db.json_set("user:2", "$", serde_json::json!({"name": "Bob"}).into())?;
db.json_set("config", "$", serde_json::json!({"debug": true}).into())?;

// List all
let (keys, _cursor) = db.json_list(None, None, 100)?;
assert_eq!(keys.len(), 3);

// List with prefix
let (user_keys, _cursor) = db.json_list(Some("user:".into()), None, 100)?;
assert_eq!(user_keys.len(), 2);
```

### Pagination

When there are more results than `limit`, the returned cursor is `Some`. Pass it to the next call:

```rust
let (first_page, cursor) = db.json_list(None, None, 10)?;
if let Some(c) = cursor {
    let (second_page, _) = db.json_list(None, Some(c), 10)?;
}
```

## Common Patterns

### Conversation History

```rust
let conv: Value = serde_json::json!({
    "messages": [
        {"role": "user", "content": "Hello"},
        {"role": "assistant", "content": "Hi there!"}
    ],
    "metadata": {
        "model": "gpt-4",
        "created_at": 1234567890
    }
}).into();
db.json_set("conversation:001", "$", conv)?;
```

### Agent Configuration

```rust
let config: Value = serde_json::json!({
    "agent_id": "agent-001",
    "model": "gpt-4",
    "tools": ["search", "calculator", "code_interpreter"],
    "constraints": {
        "max_steps": 10,
        "timeout_seconds": 30
    }
}).into();
db.json_set("agent:001:config", "$", config)?;

// Update just the model
db.json_set("agent:001:config", "$.model", "gpt-4-turbo")?;
```

## Branch Isolation

JSON documents are isolated by branch, like all primitives.

## Space Isolation

Within a branch, JSON documents are scoped to the current space:

```rust
let mut db = Strata::cache()?;

db.json_set("config", "$", serde_json::json!({"debug": true}).into())?;

db.set_space("other")?;
assert!(db.json_get("config", "$")?.is_none()); // separate documents per space
```

See [Spaces](spaces.md) for the full guide.

## Transactions

JSON set, get, and delete operations participate in transactions. Path-level updates to different fields of the same document can be made by concurrent transactions — sibling paths don't conflict.

See [Sessions and Transactions](sessions-and-transactions.md) for details.

## Next

- [Vector Store](vector-store.md) — embeddings and similarity search
- [Value Types](../concepts/value-types.md) — the 8-variant type system
