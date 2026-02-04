# KV Store Guide

The KV Store is StrataDB's most general-purpose primitive. It maps string keys to arbitrary values with simple put/get/delete semantics.

## API Overview

| Method | Signature | Returns |
|--------|-----------|---------|
| `kv_put` | `(key: &str, value: impl Into<Value>) -> Result<u64>` | Version number |
| `kv_get` | `(key: &str) -> Result<Option<Value>>` | The value, or None |
| `kv_delete` | `(key: &str) -> Result<bool>` | Whether the key existed |
| `kv_list` | `(prefix: Option<&str>) -> Result<Vec<String>>` | Matching key names |

## Put

`kv_put` creates or overwrites a key. It returns the version number of the write.

```rust
let db = Strata::cache()?;

// Pass any type that implements Into<Value>
db.kv_put("name", "Alice")?;           // &str
db.kv_put("age", 30i64)?;              // i64
db.kv_put("score", 99.5)?;             // f64
db.kv_put("active", true)?;            // bool
db.kv_put("data", vec![1u8, 2, 3])?;   // Vec<u8> → Bytes

// Overwriting returns a new version
let v1 = db.kv_put("counter", 1i64)?;
let v2 = db.kv_put("counter", 2i64)?;
assert!(v2 > v1);
```

## Get

`kv_get` returns the latest value for a key, or `None` if the key doesn't exist.

```rust
let db = Strata::cache()?;
db.kv_put("key", "value")?;

let result = db.kv_get("key")?;
assert_eq!(result, Some(Value::String("value".into())));

let missing = db.kv_get("nonexistent")?;
assert_eq!(missing, None);
```

### Extracting Typed Values

Use the `as_*()` accessors on `Value`:

```rust
if let Some(value) = db.kv_get("name")? {
    let name: &str = value.as_str().expect("expected a string");
    println!("Name: {}", name);
}

if let Some(value) = db.kv_get("age")? {
    let age: i64 = value.as_int().expect("expected an integer");
    println!("Age: {}", age);
}
```

## Delete

`kv_delete` removes a key and returns whether it existed.

```rust
let db = Strata::cache()?;

db.kv_put("key", "value")?;
assert!(db.kv_delete("key")?);   // true — existed
assert!(!db.kv_delete("key")?);  // false — already gone
```

## List Keys

`kv_list` returns all keys, optionally filtered by prefix.

```rust
let db = Strata::cache()?;

db.kv_put("user:1", "Alice")?;
db.kv_put("user:2", "Bob")?;
db.kv_put("task:1", "Review")?;

// All keys
let all = db.kv_list(None)?;
assert_eq!(all.len(), 3);

// Keys with prefix
let users = db.kv_list(Some("user:"))?;
assert_eq!(users.len(), 2);
```

## Key Naming Conventions

Use colon-separated namespaces for organized key spaces:

```rust
// Group by entity type
db.kv_put("user:123:name", "Alice")?;
db.kv_put("user:123:email", "alice@example.com")?;
db.kv_put("config:model", "gpt-4")?;
db.kv_put("config:temperature", 0.7)?;

// Then list by prefix
let user_keys = db.kv_list(Some("user:123:"))?;
let config_keys = db.kv_list(Some("config:"))?;
```

## Branch Isolation

KV data is isolated by branch. See [Branches](../concepts/branches.md) for details.

```rust
let mut db = Strata::cache()?;
db.kv_put("key", "default-value")?;

db.create_branch("other")?;
db.set_branch("other")?;
assert!(db.kv_get("key")?.is_none()); // Not visible in other branch
```

## Space Isolation

Within a branch, KV data is further organized by space. Each space has independent keys:

```rust
let mut db = Strata::cache()?;

db.kv_put("config", "default-value")?;

// Switch to a different space
db.set_space("experiments")?;
assert!(db.kv_get("config")?.is_none()); // not visible in this space

db.kv_put("config", "experiment-value")?;

// Switch back — original data is untouched
db.set_space("default")?;
assert_eq!(db.kv_get("config")?, Some(Value::String("default-value".into())));
```

See [Spaces](spaces.md) for the full guide.

## Transactions

KV operations participate in transactions. Within a `Session`, reads and writes are atomic:

```rust
session.execute(Command::TxnBegin { branch: None, options: None })?;
session.execute(Command::KvPut { branch: None, key: "a".into(), value: Value::Int(1) })?;
session.execute(Command::KvPut { branch: None, key: "b".into(), value: Value::Int(2) })?;
session.execute(Command::TxnCommit)?;
// Both writes are visible atomically
```

See [Sessions and Transactions](sessions-and-transactions.md) for the full guide.

## Next

- [Event Log](event-log.md) — append-only event streams
- [API Quick Reference](../reference/api-quick-reference.md) — all methods at a glance
