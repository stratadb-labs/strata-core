# Versioning API Design

## Overview

Strata's core value proposition is that data is never overwritten - every write creates a new version. This document defines the versioning API.

## Design

Two methods for reading data:

| Method | Returns | Use Case |
|--------|---------|----------|
| `get()` | Latest value only | Fast reads, most common case |
| `getv()` | Full version history, indexable | When you need previous versions |

### API

```rust
/// Get latest value only (fast path)
pub fn get(&self, run_id: &RunId, key: &str) -> Result<Option<Value>>

/// Get full version history with indexing support
pub fn getv(&self, run_id: &RunId, key: &str) -> Result<Option<VersionedHistory<Value>>>
```

### VersionedHistory<T>

```rust
pub struct VersionedHistory<T> {
    versions: Vec<Versioned<T>>,  // Newest first
}

impl<T> VersionedHistory<T> {
    /// Get latest value (same as [0])
    pub fn value(&self) -> &T {
        &self.versions[0].value
    }

    /// Number of versions
    pub fn len(&self) -> usize {
        self.versions.len()
    }

    /// Latest version info
    pub fn version(&self) -> Version {
        self.versions[0].version
    }

    /// Latest timestamp
    pub fn timestamp(&self) -> Timestamp {
        self.versions[0].timestamp
    }
}

// Index by version offset: [0] = latest, [1] = previous, etc.
impl<T> std::ops::Index<usize> for VersionedHistory<T> {
    type Output = Versioned<T>;
    fn index(&self, index: usize) -> &Self::Output {
        &self.versions[index]
    }
}
```

### Usage

```rust
// Fast path - just need the current value
let name = db.kv.get("user:123")?;  // Option<Value>

// Need version history
let history = db.kv.getv("user:123")?;  // Option<VersionedHistory<Value>>
if let Some(h) = history {
    println!("Current: {:?}", h[0].value);   // Latest
    println!("Previous: {:?}", h[1].value);  // One before
    println!("Version count: {}", h.len());
}
```

## Current State

### Storage Layer (Correct)

The storage layer correctly preserves all versions:

```rust
// crates/storage/src/sharded.rs
pub struct VersionChain {
    versions: VecDeque<StoredValue>,  // Newest-first
}
```

### API Layer (Needs Update)

Current methods that need to be reconciled:

| Current Method | Status | Action |
|---------------|--------|--------|
| `get()` | Returns `Versioned<Value>` | Change to return `Value` only |
| `get_at(version)` | Point-in-time read | Keep as-is (different purpose) |
| `history()` | Returns `Vec<Versioned<Value>>` | Replace with `getv()` |

### Versioned<T> Struct (Keep)

```rust
// crates/core/src/contract/versioned.rs
pub struct Versioned<T> {
    pub value: T,
    pub version: Version,
    pub timestamp: Timestamp,
}
```

This struct is still useful as the element type within `VersionedHistory<T>`.

## Implementation Steps

1. Create `VersionedHistory<T>` type in `crates/core/src/contract/`
2. Add `getv()` / `readv()` methods to primitives (KV, StateCell, JsonStore)
3. Update `get()` / `read()` to return `Value` instead of `Versioned<Value>`
4. **All methods must use `Database.transaction()`** - no `db.storage()` bypasses
4. Deprecate `history()` in favor of `getv()`
5. Keep `get_at(version)` for point-in-time queries

## Method Summary by Primitive

### KVStore

| Method | Signature | Purpose |
|--------|-----------|---------|
| `get` | `get(run_id, key) -> Option<Value>` | Latest value |
| `getv` | `getv(run_id, key) -> Option<VersionedHistory<Value>>` | All versions |
| `get_at` | `get_at(run_id, key, version) -> Option<Versioned<Value>>` | Point-in-time |
| `put` | `put(run_id, key, value) -> Version` | Write |
| `delete` | `delete(run_id, key) -> bool` | Delete |
| `list` | `list(run_id, prefix) -> Vec<String>` | List keys |

### StateCell

| Method | Signature | Purpose |
|--------|-----------|---------|
| `read` | `read(run_id, name) -> Option<Value>` | Latest state |
| `readv` | `readv(run_id, name) -> Option<VersionedHistory<Value>>` | All states |
| `init` | `init(run_id, name, value) -> Version` | Create cell |
| `set` | `set(run_id, name, value) -> Version` | Update |
| `cas` | `cas(run_id, name, expected, value) -> Version` | Compare-and-swap |

### JsonStore

| Method | Signature | Purpose |
|--------|-----------|---------|
| `get` | `get(run_id, doc_id, path) -> Option<JsonValue>` | Latest value at path |
| `getv` | `getv(run_id, doc_id) -> Option<VersionedHistory<JsonValue>>` | All doc versions |
| `create` | `create(run_id, doc_id, value) -> Version` | Create document |
| `set` | `set(run_id, doc_id, path, value) -> Version` | Update at path |
| `delete` | `delete(run_id, doc_id) -> bool` | Delete document |
| `list` | `list(run_id, prefix, limit) -> Vec<String>` | List documents |
