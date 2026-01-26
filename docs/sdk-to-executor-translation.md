# SDK to Executor Translation Guide

This document explains how end-user SDK calls translate through the layers to executor Commands, and how responses flow back.

## Architecture Layers

```
┌─────────────────────────────────────────────────────────────┐
│  End User Code                                              │
│  kv.get("user:123")                                         │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  Rust SDK (strata-sdk crate)                                │
│  - Ergonomic API                                            │
│  - Type conversions                                         │
│  - Error wrapping                                           │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  Executor                                                   │
│  Command::KvGet { run, key } → Output::MaybeVersioned(...)  │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│  Substrate (storage layer)                                  │
└─────────────────────────────────────────────────────────────┘
```

## Example: Simple KV Get

### What the End User Writes

```rust
use strata_sdk::{Client, Value};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Connect to database
    let client = Client::open("./my-data")?;

    // Simple get - returns Option<Value>
    let name = client.kv().get("user:123")?;

    match name {
        Some(value) => println!("Found: {}", value.as_str().unwrap()),
        None => println!("Not found"),
    }

    Ok(())
}
```

### What the SDK Does Internally

```rust
// Inside strata-sdk/src/kv.rs

impl KV<'_> {
    /// Get a value by key.
    ///
    /// Returns `None` if key doesn't exist.
    pub fn get(&self, key: &str) -> Result<Option<Value>> {
        // 1. Build the Command
        let command = Command::KvGet {
            run: self.run_id.clone(),
            key: key.to_string(),
        };

        // 2. Execute via executor
        let output = self.executor.execute(command)?;

        // 3. Translate Output back to user-friendly type
        match output {
            Output::MaybeVersioned(Some(versioned)) => Ok(Some(versioned.value)),
            Output::MaybeVersioned(None) => Ok(None),
            _ => Err(Error::unexpected_output("MaybeVersioned", output)),
        }
    }
}
```

### The Full Translation Flow

```
USER CODE                    SDK LAYER                      EXECUTOR/SUBSTRATE
─────────                    ─────────                      ──────────────────

client.kv().get("user:123")
         │
         │    ┌────────────────────────────┐
         └───►│ Build Command::KvGet {     │
              │   run: "default",          │
              │   key: "user:123"          │
              │ }                          │
              └────────────────────────────┘
                            │
                            │    ┌────────────────────────────┐
                            └───►│ executor.execute(cmd)      │
                                 │   → substrate.kv_get(...)  │
                                 │   → Output::MaybeVersioned │
                                 └────────────────────────────┘
                            │
              ┌─────────────┘
              ▼
         ┌────────────────────────────┐
         │ Match Output variant       │
         │ Extract versioned.value    │
         │ Return Option<Value>       │
         └────────────────────────────┘
              │
              ▼
    Some(Value::String("Alice"))
```

## Example: KV Put

### What the End User Writes

```rust
// Simple put
client.kv().put("user:123", "Alice")?;

// Put with explicit Value type
client.kv().put("counter", Value::Int(0))?;

// Put returns the version number
let version = client.kv().put("user:123", "Bob")?;
println!("Stored at version {}", version);
```

### SDK Translation

```rust
impl KV<'_> {
    pub fn put(&self, key: &str, value: impl Into<Value>) -> Result<u64> {
        let command = Command::KvPut {
            run: self.run_id.clone(),
            key: key.to_string(),
            value: value.into(),  // SDK provides Into<Value> for common types
        };

        let output = self.executor.execute(command)?;

        match output {
            Output::Version(v) => Ok(v),
            _ => Err(Error::unexpected_output("Version", output)),
        }
    }
}
```

### Into<Value> Conversions (SDK provides these)

```rust
// In strata-sdk/src/value.rs

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::String(s.to_string())
    }
}

impl From<i64> for Value {
    fn from(n: i64) -> Self {
        Value::Int(n)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Bool(b)
    }
}

// etc. for String, f64, Vec<u8>, HashMap, Vec<Value>, ...
```

## Example: Batch Operations (mget)

### What the End User Writes

```rust
// Get multiple keys at once
let results = client.kv().mget(&["user:1", "user:2", "user:3"])?;

for (key, value) in ["user:1", "user:2", "user:3"].iter().zip(results) {
    match value {
        Some(v) => println!("{}: {}", key, v),
        None => println!("{}: not found", key),
    }
}
```

### SDK Translation

```rust
impl KV<'_> {
    pub fn mget(&self, keys: &[&str]) -> Result<Vec<Option<Value>>> {
        let command = Command::KvMget {
            run: self.run_id.clone(),
            keys: keys.iter().map(|k| k.to_string()).collect(),
        };

        let output = self.executor.execute(command)?;

        match output {
            Output::MaybeVersionedValues(values) => {
                Ok(values.into_iter()
                    .map(|opt| opt.map(|v| v.value))
                    .collect())
            }
            _ => Err(Error::unexpected_output("MaybeVersionedValues", output)),
        }
    }
}
```

## Example: Atomic Increment

### What the End User Writes

```rust
// Initialize
client.kv().put("page_views", 0)?;

// Increment - returns new value
let count = client.kv().incr("page_views", 1)?;
println!("Page views: {}", count);  // 1

// Increment by 10
let count = client.kv().incr("page_views", 10)?;
println!("Page views: {}", count);  // 11
```

### SDK Translation

```rust
impl KV<'_> {
    pub fn incr(&self, key: &str, delta: i64) -> Result<i64> {
        let command = Command::KvIncr {
            run: self.run_id.clone(),
            key: key.to_string(),
            delta,
        };

        let output = self.executor.execute(command)?;

        match output {
            Output::Int(n) => Ok(n),
            _ => Err(Error::unexpected_output("Int", output)),
        }
    }
}
```

## Example: Compare-and-Swap (CAS)

### What the End User Writes

```rust
// Read-modify-write with optimistic concurrency
loop {
    // Get current value with version
    let current = client.kv().get_versioned("balance")?
        .ok_or("balance not found")?;

    let current_balance = current.value.as_int().unwrap();
    let new_balance = current_balance + 100;

    // Try to update - only succeeds if version matches
    match client.kv().cas("balance", current.version, new_balance)? {
        Some(new_version) => {
            println!("Updated to version {}", new_version);
            break;
        }
        None => {
            println!("Conflict, retrying...");
            continue;
        }
    }
}
```

### SDK Translation

```rust
impl KV<'_> {
    /// Get value with version metadata (for CAS operations)
    pub fn get_versioned(&self, key: &str) -> Result<Option<VersionedValue>> {
        let command = Command::KvGet {
            run: self.run_id.clone(),
            key: key.to_string(),
        };

        let output = self.executor.execute(command)?;

        match output {
            Output::MaybeVersioned(v) => Ok(v),
            _ => Err(Error::unexpected_output("MaybeVersioned", output)),
        }
    }

    /// Compare-and-swap: update only if current version matches expected
    pub fn cas(
        &self,
        key: &str,
        expected_version: u64,
        new_value: impl Into<Value>,
    ) -> Result<Option<u64>> {
        let command = Command::KvCasVersion {
            run: self.run_id.clone(),
            key: key.to_string(),
            expected_version: Some(expected_version),
            new_value: new_value.into(),
        };

        let output = self.executor.execute(command)?;

        match output {
            Output::MaybeVersion(v) => Ok(v),
            _ => Err(Error::unexpected_output("MaybeVersion", output)),
        }
    }
}
```

## Command → Output Mapping Reference

| SDK Method | Command Variant | Output Variant | SDK Return Type |
|------------|-----------------|----------------|-----------------|
| `kv.get(key)` | `KvGet` | `MaybeVersioned` | `Option<Value>` |
| `kv.get_versioned(key)` | `KvGet` | `MaybeVersioned` | `Option<VersionedValue>` |
| `kv.put(key, value)` | `KvPut` | `Version` | `u64` |
| `kv.delete(key)` | `KvDelete` | `Bool` | `bool` |
| `kv.exists(key)` | `KvExists` | `Bool` | `bool` |
| `kv.incr(key, delta)` | `KvIncr` | `Int` | `i64` |
| `kv.mget(keys)` | `KvMget` | `MaybeVersionedValues` | `Vec<Option<Value>>` |
| `kv.mput(entries)` | `KvMput` | `Version` | `u64` |
| `kv.cas(key, ver, val)` | `KvCasVersion` | `MaybeVersion` | `Option<u64>` |
| `kv.keys(prefix)` | `KvKeys` | `Keys` | `Vec<String>` |
| `kv.scan(prefix)` | `KvScan` | `KvScanResult` | `ScanResult` |

## SDK Client Structure

```rust
// strata-sdk/src/lib.rs

pub struct Client {
    executor: Executor,
    default_run: RunId,
}

impl Client {
    /// Open a database at the given path
    pub fn open(path: &str) -> Result<Self> {
        let db = Arc::new(Database::open(path)?);
        let substrate = Arc::new(SubstrateImpl::new(db));
        let executor = Executor::new(substrate);

        Ok(Self {
            executor,
            default_run: RunId::from("default"),
        })
    }

    /// Get KV operations handle (uses default run)
    pub fn kv(&self) -> KV<'_> {
        KV {
            executor: &self.executor,
            run_id: self.default_run.clone(),
        }
    }

    /// Get KV operations for a specific run
    pub fn kv_for_run(&self, run: &str) -> KV<'_> {
        KV {
            executor: &self.executor,
            run_id: RunId::from(run),
        }
    }

    /// Create a new isolated run
    pub fn create_run(&self, name: Option<&str>) -> Result<RunInfo> {
        // ...
    }
}

pub struct KV<'a> {
    executor: &'a Executor,
    run_id: RunId,
}
```

## Error Translation

```rust
// Executor errors map to SDK errors

// Executor error
Error::NotFound { entity, key }

// Becomes SDK error
SdkError::KeyNotFound { key: String }

// Executor error
Error::WrongType { expected, actual }

// Becomes SDK error
SdkError::TypeError {
    expected: String,
    actual: String,
    key: Option<String>,
}
```

## Summary

The SDK provides:
1. **Ergonomic API** - Simple method calls instead of building Command structs
2. **Type conversions** - `Into<Value>` for common types, extractors for reading
3. **Output translation** - Match on Output variants, return user-friendly types
4. **Error wrapping** - Convert executor errors to user-friendly SDK errors
5. **Connection management** - Handle database/executor lifecycle

The Executor provides:
1. **Uniform dispatch** - Single `execute(Command) -> Output` interface
2. **Serializable protocol** - Commands/Outputs can cross process boundaries
3. **Batch execution** - `execute_many` for multiple commands
4. **Deterministic behavior** - Same command always produces same output type
