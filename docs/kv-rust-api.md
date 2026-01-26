# KV Store Rust API Guide

This document describes the KV (Key-Value) store lifecycle and how to use the Rust API.

## Overview

The KV store is a versioned key-value storage primitive. Every write operation creates a new version, enabling:
- Time-travel queries (read historical values)
- Optimistic concurrency control (CAS operations)
- Full audit trail via version history

## Setup

```rust
use strata_executor::Strata;
use strata_core::Value;
use strata_engine::Database;
use strata_api::substrate::SubstrateImpl;
use std::sync::Arc;

// Create database and API wrapper
let db = Arc::new(Database::open("./my-data").unwrap());
let substrate = Arc::new(SubstrateImpl::new(db));
let strata = Strata::new(substrate);
```

## Basic Operations

### Put (Write)

Store a value at a key. Returns the version number assigned to this write.

```rust
// Store a string
let version = strata.kv_put("default", "user:123", Value::String("Alice".into()))?;
println!("Stored at version {}", version);  // e.g., "Stored at version 1"

// Store an integer
strata.kv_put("default", "counter", Value::Int(0))?;

// Store a complex object
let user = Value::Object([
    ("name".to_string(), Value::String("Bob".into())),
    ("age".to_string(), Value::Int(30)),
    ("active".to_string(), Value::Bool(true)),
].into_iter().collect());
strata.kv_put("default", "user:456", user)?;
```

### Get (Read)

Retrieve the current value at a key. Returns `None` if key doesn't exist.

```rust
// Get returns Option<VersionedValue>
match strata.kv_get("default", "user:123")? {
    Some(versioned) => {
        println!("Value: {:?}", versioned.value);      // The actual value
        println!("Version: {}", versioned.version);     // Version number
        println!("Timestamp: {}", versioned.timestamp); // Write timestamp (nanos)
    }
    None => println!("Key not found"),
}

// Pattern: extract just the value
let name = strata.kv_get("default", "user:123")?
    .map(|v| v.value);
```

### Delete

Remove a key. Returns `true` if key existed, `false` otherwise.

```rust
let existed = strata.kv_delete("default", "user:123")?;
if existed {
    println!("Key deleted");
} else {
    println!("Key didn't exist");
}
```

### Exists

Check if a key exists without retrieving its value.

```rust
if strata.kv_exists("default", "user:123")? {
    println!("Key exists");
}
```

## Atomic Operations

### Increment

Atomically increment an integer value. Creates the key with value `delta` if it doesn't exist.

```rust
// Initialize counter
strata.kv_put("default", "page_views", Value::Int(0))?;

// Increment by 1
let new_value = strata.kv_incr("default", "page_views", 1)?;
println!("Page views: {}", new_value);  // 1

// Increment by arbitrary amount
let new_value = strata.kv_incr("default", "page_views", 100)?;
println!("Page views: {}", new_value);  // 101

// Decrement (negative delta)
let new_value = strata.kv_incr("default", "page_views", -50)?;
println!("Page views: {}", new_value);  // 51
```

### Compare-and-Swap (CAS)

Update a value only if its current version matches expected. Essential for optimistic concurrency.

```rust
// Read current value
let current = strata.kv_get("default", "balance")?.unwrap();
let current_version = current.version;
let current_balance = match current.value {
    Value::Int(n) => n,
    _ => panic!("Expected int"),
};

// Compute new value
let new_balance = current_balance + 100;

// Write only if version hasn't changed (via Command directly)
use strata_executor::{Command, Output};

let result = strata.executor().execute(Command::KvCasVersion {
    run: "default".into(),
    key: "balance".to_string(),
    expected_version: Some(current_version),
    new_value: Value::Int(new_balance),
})?;

match result {
    Output::MaybeVersion(Some(new_version)) => {
        println!("Updated to version {}", new_version);
    }
    Output::MaybeVersion(None) => {
        println!("Conflict! Someone else modified the value");
        // Retry logic here
    }
    _ => unreachable!(),
}
```

## Batch Operations

### Multi-Get

Retrieve multiple keys in a single call.

```rust
let keys = vec![
    "user:1".to_string(),
    "user:2".to_string(),
    "user:3".to_string(),
];

let results = strata.kv_mget("default", keys)?;
// Results are in same order as keys
for (i, maybe_value) in results.iter().enumerate() {
    match maybe_value {
        Some(v) => println!("user:{} = {:?}", i + 1, v.value),
        None => println!("user:{} not found", i + 1),
    }
}
```

### Multi-Put

Write multiple key-value pairs atomically.

```rust
let entries = vec![
    ("config:timeout".to_string(), Value::Int(30)),
    ("config:retries".to_string(), Value::Int(3)),
    ("config:debug".to_string(), Value::Bool(false)),
];

let version = strata.kv_mput("default", entries)?;
println!("All keys written at version {}", version);
```

## Advanced Operations (via Executor)

Some operations require using the `Executor` directly:

### Get at Specific Version (Time Travel)

```rust
use strata_executor::{Command, Output};

let result = strata.executor().execute(Command::KvGetAt {
    run: "default".into(),
    key: "user:123".to_string(),
    version: 5,  // Get value as it was at version 5
})?;

match result {
    Output::Versioned(v) => println!("Value at v5: {:?}", v.value),
    _ => println!("Not found at that version"),
}
```

### Version History

Get the history of all versions for a key.

```rust
let result = strata.executor().execute(Command::KvHistory {
    run: "default".into(),
    key: "user:123".to_string(),
    limit: Some(10),   // Get last 10 versions
    before: None,      // No cursor (start from latest)
})?;

match result {
    Output::VersionedValues(history) => {
        for entry in history {
            println!("v{}: {:?} (at {})",
                entry.version,
                entry.value,
                entry.timestamp
            );
        }
    }
    _ => unreachable!(),
}
```

### Key Listing and Scanning

```rust
// List all keys matching a prefix
let result = strata.executor().execute(Command::KvKeys {
    run: "default".into(),
    prefix: Some("user:".to_string()),
    limit: Some(100),
    cursor: None,
})?;

match result {
    Output::Keys(keys) => {
        for key in keys {
            println!("Found key: {}", key);
        }
    }
    _ => unreachable!(),
}

// Scan with key-value pairs and pagination
let result = strata.executor().execute(Command::KvScan {
    run: "default".into(),
    prefix: Some("user:".to_string()),
    limit: Some(10),
    cursor: None,
})?;

match result {
    Output::KvScanResult { entries, cursor } => {
        for (key, value) in entries {
            println!("{}: {:?}", key, value.value);
        }
        if let Some(next_cursor) = cursor {
            println!("More results available, cursor: {}", next_cursor);
        }
    }
    _ => unreachable!(),
}
```

## Run Isolation

All KV operations are scoped to a "run" - an isolated namespace. The `"default"` run always exists.

```rust
// Create a new isolated run
let (run_info, _) = strata.run_create(
    Some("experiment-1".to_string()),
    None,
)?;

// Write to the new run
strata.kv_put("experiment-1", "key", Value::Int(42))?;

// Data is isolated - won't affect default run
assert!(strata.kv_get("default", "key")?.is_none());
assert!(strata.kv_get("experiment-1", "key")?.is_some());
```

## Value Types

The `Value` enum supports:

```rust
use strata_core::Value;

// Primitives
Value::Null
Value::Bool(true)
Value::Int(42)           // i64
Value::Float(3.14)       // f64
Value::String("hello".into())

// Binary data
Value::Bytes(vec![0, 1, 2, 255])

// Collections
Value::Array(vec![Value::Int(1), Value::Int(2)])
Value::Object(/* BTreeMap<String, Value> */)
```

## Error Handling

```rust
use strata_executor::Error;

match strata.kv_put("nonexistent-run", "key", Value::Int(1)) {
    Ok(version) => println!("Success"),
    Err(Error::NotFound { entity, key }) => {
        println!("Run '{}' not found", key.unwrap_or_default());
    }
    Err(Error::InvalidInput { reason }) => {
        println!("Bad input: {}", reason);
    }
    Err(Error::WrongType { expected, actual }) => {
        println!("Type error: expected {}, got {}", expected, actual);
    }
    Err(e) => println!("Other error: {:?}", e),
}
```

## Complete Example: User Session Store

```rust
use strata_executor::Strata;
use strata_core::Value;
use std::collections::BTreeMap;

fn create_session(strata: &Strata, user_id: &str, token: &str) -> Result<u64, Error> {
    let session = Value::Object(BTreeMap::from([
        ("user_id".into(), Value::String(user_id.into())),
        ("token".into(), Value::String(token.into())),
        ("created_at".into(), Value::Int(timestamp_now())),
        ("expires_at".into(), Value::Int(timestamp_now() + 3600)),
    ]));

    strata.kv_put("default", &format!("session:{}", token), session)
}

fn get_session(strata: &Strata, token: &str) -> Result<Option<Value>, Error> {
    Ok(strata.kv_get("default", &format!("session:{}", token))?
        .map(|v| v.value))
}

fn extend_session(strata: &Strata, token: &str) -> Result<(), Error> {
    let key = format!("session:{}", token);

    // Get current session
    let current = strata.kv_get("default", &key)?
        .ok_or(Error::NotFound { entity: "session".into(), key: Some(token.into()) })?;

    // Update expiry
    if let Value::Object(mut map) = current.value {
        map.insert("expires_at".into(), Value::Int(timestamp_now() + 3600));
        strata.kv_put("default", &key, Value::Object(map))?;
    }

    Ok(())
}

fn delete_session(strata: &Strata, token: &str) -> Result<bool, Error> {
    strata.kv_delete("default", &format!("session:{}", token))
}

fn timestamp_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
```

## Summary

| Operation | Method | Returns |
|-----------|--------|---------|
| Write | `kv_put(run, key, value)` | `u64` (version) |
| Read | `kv_get(run, key)` | `Option<VersionedValue>` |
| Delete | `kv_delete(run, key)` | `bool` (existed) |
| Exists | `kv_exists(run, key)` | `bool` |
| Increment | `kv_incr(run, key, delta)` | `i64` (new value) |
| Multi-Get | `kv_mget(run, keys)` | `Vec<Option<VersionedValue>>` |
| Multi-Put | `kv_mput(run, entries)` | `u64` (version) |
