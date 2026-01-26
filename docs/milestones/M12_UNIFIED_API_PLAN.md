# M12: Unified API Implementation Plan

> **Status**: Implementation Plan
> **Author**: Architecture Team
> **Date**: 2026-01-25
> **Prerequisites**: M11 (Primitives Complete), RunBundle MVP
> **Estimated Scope**: Major refactor

---

## Executive Summary

This milestone transforms Strata's API from a confusing two-layer system (Facade + Substrate) into a single, unified, intuitive API surface. The result will be a clean, production-ready API that matches best-in-class databases.

**Before:**
```rust
use strata_api::substrate::KVStore;
use strata_api::facade::KVFacade;

substrate.kv_put(&run, "key", value)?;
facade.set("key", value)?;
```

**After:**
```rust
use strata::prelude::*;

db.kv.set("key", value)?;           // Simple
db.kv.put(&run, "key", value)?;     // Full control
```

---

## Goals

1. **Single Entry Point**: `use strata::prelude::*`
2. **Unified Primitives**: `db.kv`, `db.json`, `db.events`, `db.state`, `db.vectors`, `db.runs`
3. **Clean Naming**: No redundant prefixes (`set` not `kv_set`)
4. **Progressive Disclosure**: Simple → Run-scoped → Full control
5. **Consistent Patterns**: Same conventions across all primitives
6. **Backward Compatibility**: Deprecation path for old API

---

## Reference Documents

| Document | Purpose |
|----------|---------|
| `UNIFIED_API_DESIGN.md` | Target API surface |
| `API_AUDIT_REPORT.md` | Issues to fix |
| `CAPABILITIES_AUDIT.md` | All capabilities to expose |
| `API_ENCAPSULATION.md` | Visibility strategy |

---

## Implementation Phases

### Phase 1: Create `strata` Crate (Foundation)

**Goal**: Create the public entry point crate that will be the only thing users depend on.

#### 1.1 Create Crate Structure

```
strata/
├── Cargo.toml
└── src/
    ├── lib.rs           # Main entry, re-exports
    ├── database.rs      # Strata struct (wraps Database)
    ├── primitives/
    │   ├── mod.rs
    │   ├── kv.rs        # KV primitive wrapper
    │   ├── json.rs      # Json primitive wrapper
    │   ├── events.rs    # Events primitive wrapper
    │   ├── state.rs     # State primitive wrapper
    │   ├── vectors.rs   # Vectors primitive wrapper
    │   └── runs.rs      # Runs primitive wrapper
    ├── types.rs         # Public types (RunId, Value, etc.)
    ├── error.rs         # Unified Error type
    └── prelude.rs       # Convenient imports
```

#### 1.2 Cargo.toml

```toml
[package]
name = "strata"
version = "0.12.0"
edition = "2021"
description = "Production-grade embedded database for AI agents"
# This is the ONLY publishable crate

[dependencies]
strata-api = { path = "../crates/api", version = "0.12.0" }
strata-engine = { path = "../crates/engine", version = "0.12.0" }
strata-core = { path = "../crates/core", version = "0.12.0" }

[features]
default = []
full = ["search", "vectors"]
search = ["strata-api/search"]
vectors = ["strata-api/vectors"]
```

#### 1.3 lib.rs Structure

```rust
//! # Strata
//!
//! Production-grade embedded database for AI agents.
//!
//! ## Quick Start
//!
//! ```rust
//! use strata::prelude::*;
//!
//! let db = Strata::open("./my-db")?;
//!
//! db.kv.set("key", "value")?;
//! let value = db.kv.get("key")?;
//!
//! db.shutdown()?;
//! ```

#![warn(missing_docs)]

mod database;
mod primitives;
mod types;
mod error;
pub mod prelude;

pub use database::{Strata, StrataBuilder};
pub use error::{Error, Result};
pub use types::*;
pub use primitives::{KV, Json, Events, State, Vectors, Runs};
```

#### 1.4 Tasks

- [ ] Create `strata/` directory at workspace root
- [ ] Create `Cargo.toml` with dependencies
- [ ] Create `src/lib.rs` with module structure
- [ ] Create `src/prelude.rs` with common imports
- [ ] Add `strata` to workspace `Cargo.toml`
- [ ] Verify crate compiles

---

### Phase 2: Implement Core Types

**Goal**: Create unified public types that hide internal complexity.

#### 2.1 Unified Error Type

```rust
// src/error.rs

/// All Strata errors
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("wrong type: expected {expected}, got {actual}")]
    WrongType { expected: String, actual: String },

    #[error("invalid key: {0}")]
    InvalidKey(String),

    #[error("invalid path: {0}")]
    InvalidPath(String),

    #[error("version mismatch: expected {expected}, got {actual}")]
    VersionMismatch { expected: u64, actual: u64 },

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("constraint violation: {0}")]
    ConstraintViolation(String),

    #[error("run closed: {0}")]
    RunClosed(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, Error>;

// Conversion from internal errors
impl From<strata_core::Error> for Error {
    fn from(e: strata_core::Error) -> Self {
        // Map internal errors to public errors
    }
}
```

#### 2.2 Public Types

```rust
// src/types.rs

/// Run identifier
pub struct RunId(pub(crate) strata_api::substrate::ApiRunId);

impl RunId {
    pub fn new() -> Self { ... }
    pub fn parse(s: &str) -> Result<Self> { ... }
    pub fn default() -> Self { ... }
    pub fn is_default(&self) -> bool { ... }
    pub fn as_str(&self) -> &str { ... }
}

// Re-export from core (these are already clean)
pub use strata_core::Value;
pub use strata_core::contract::{Version, Versioned, Timestamp};

/// Run state
pub use strata_api::substrate::RunState;

/// Run information
pub use strata_api::substrate::RunInfo;

/// Retention policy
pub use strata_api::substrate::RetentionPolicy;

/// Distance metric for vectors
pub use strata_api::substrate::DistanceMetric;

/// Search filter for vectors
pub use strata_api::substrate::SearchFilter;
```

#### 2.3 Tasks

- [ ] Create `src/error.rs` with unified Error enum
- [ ] Create `src/types.rs` with public type wrappers
- [ ] Implement `From` conversions for internal types
- [ ] Add `#[doc(hidden)]` to internal type access
- [ ] Write tests for type conversions

---

### Phase 3: Implement Strata Entry Point

**Goal**: Create the main `Strata` struct that provides access to all primitives.

#### 3.1 Strata Struct

```rust
// src/database.rs

use crate::primitives::*;
use crate::{Error, Result};

/// The Strata database
///
/// Main entry point for all database operations.
///
/// # Example
///
/// ```rust
/// use strata::prelude::*;
///
/// let db = Strata::open("./my-db")?;
///
/// // Access primitives
/// db.kv.set("key", "value")?;
/// db.json.set("doc", json!({"name": "Alice"}))?;
/// db.events.append("stream", json!({"action": "login"}))?;
///
/// db.shutdown()?;
/// ```
pub struct Strata {
    inner: Arc<strata_engine::Database>,

    /// Key-value operations
    pub kv: KV,

    /// JSON document operations
    pub json: Json,

    /// Event stream operations
    pub events: Events,

    /// State cell operations
    pub state: State,

    /// Vector similarity search
    pub vectors: Vectors,

    /// Run lifecycle management
    pub runs: Runs,
}

impl Strata {
    /// Open a database at the given path
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::builder().path(path).open()
    }

    /// Create a builder for database configuration
    pub fn builder() -> StrataBuilder {
        StrataBuilder::new()
    }

    /// Force flush to disk
    pub fn flush(&self) -> Result<()> {
        self.inner.flush().map_err(Into::into)
    }

    /// Graceful shutdown
    pub fn shutdown(&self) -> Result<()> {
        self.inner.shutdown().map_err(Into::into)
    }

    /// Check if database is open
    pub fn is_open(&self) -> bool {
        self.inner.is_open()
    }

    /// Get database info
    pub fn info(&self) -> DatabaseInfo {
        DatabaseInfo {
            path: self.inner.data_dir().to_path_buf(),
            durability_mode: self.inner.durability_mode(),
        }
    }

    /// Execute operations in a transaction
    pub fn transaction<F, T>(&self, run: &RunId, f: F) -> Result<T>
    where
        F: FnOnce(&Transaction) -> Result<T>,
    {
        // Create transaction context with primitive access
        let tx = Transaction::new(self.inner.clone(), run);
        let result = f(&tx)?;
        // Auto-commit handled by Transaction
        Ok(result)
    }

    /// Transaction with retry on conflict
    pub fn transaction_retry<F, T>(&self, run: &RunId, retries: usize, f: F) -> Result<T>
    where
        F: FnOnce(&Transaction) -> Result<T>,
    {
        let config = strata_engine::RetryConfig::new().with_max_retries(retries);
        // Delegate to engine with retry
        todo!()
    }
}

/// Builder for database configuration
pub struct StrataBuilder {
    inner: strata_engine::DatabaseBuilder,
}

impl StrataBuilder {
    pub fn new() -> Self {
        Self { inner: strata_engine::DatabaseBuilder::new() }
    }

    pub fn path(mut self, path: impl AsRef<Path>) -> Self {
        self.inner = self.inner.path(path);
        self
    }

    pub fn in_memory(mut self) -> Self {
        self.inner = self.inner.in_memory();
        self
    }

    pub fn buffered(mut self) -> Self {
        self.inner = self.inner.buffered();
        self
    }

    pub fn strict(mut self) -> Self {
        self.inner = self.inner.strict();
        self
    }

    pub fn open(self) -> Result<Strata> {
        let db = Arc::new(self.inner.open()?);
        Ok(Strata {
            kv: KV::new(db.clone()),
            json: Json::new(db.clone()),
            events: Events::new(db.clone()),
            state: State::new(db.clone()),
            vectors: Vectors::new(db.clone()),
            runs: Runs::new(db.clone()),
            inner: db,
        })
    }
}
```

#### 3.2 Tasks

- [ ] Create `src/database.rs` with Strata struct
- [ ] Implement StrataBuilder
- [ ] Implement database lifecycle methods
- [ ] Implement transaction methods
- [ ] Write integration tests

---

### Phase 4: Implement Primitive Wrappers

**Goal**: Create wrapper structs for each primitive with clean method names.

#### 4.1 KV Primitive

```rust
// src/primitives/kv.rs

use crate::{Error, Result, RunId, Value, Version, Versioned};

/// Key-value store operations
pub struct KV {
    db: Arc<strata_engine::Database>,
    substrate: strata_api::substrate::SubstrateImpl,
}

impl KV {
    pub(crate) fn new(db: Arc<strata_engine::Database>) -> Self {
        Self {
            substrate: strata_api::substrate::SubstrateImpl::new(db.clone()),
            db,
        }
    }

    // ========================================
    // SIMPLE OPERATIONS (default run)
    // ========================================

    /// Get a value by key
    pub fn get(&self, key: &str) -> Result<Option<Value>> {
        self.get_in(&RunId::default(), key)
    }

    /// Get a value with version info
    pub fn get_versioned(&self, key: &str) -> Result<Option<Versioned<Value>>> {
        self.substrate
            .kv_get(&RunId::default().0, key)
            .map_err(Into::into)
    }

    /// Set a value (default run, no version return)
    pub fn set(&self, key: &str, value: impl Into<Value>) -> Result<()> {
        self.set_in(&RunId::default(), key, value)
    }

    /// Delete a key
    pub fn delete(&self, key: &str) -> Result<bool> {
        self.delete_in(&RunId::default(), key)
    }

    /// Check if key exists
    pub fn exists(&self, key: &str) -> Result<bool> {
        self.substrate
            .kv_exists(&RunId::default().0, key)
            .map_err(Into::into)
    }

    /// Atomic increment
    pub fn incr(&self, key: &str) -> Result<i64> {
        self.incr_by(key, 1)
    }

    /// Atomic increment by delta
    pub fn incr_by(&self, key: &str, delta: i64) -> Result<i64> {
        self.substrate
            .kv_incr(&RunId::default().0, key, delta)
            .map_err(Into::into)
    }

    /// Set if not exists
    pub fn set_nx(&self, key: &str, value: impl Into<Value>) -> Result<bool> {
        self.substrate
            .kv_cas_version(&RunId::default().0, key, None, value.into())
            .map_err(Into::into)
    }

    // ========================================
    // RUN-SCOPED OPERATIONS
    // ========================================

    /// Get in specific run
    pub fn get_in(&self, run: &RunId, key: &str) -> Result<Option<Value>> {
        self.substrate
            .kv_get(&run.0, key)
            .map(|opt| opt.map(|v| v.into_value()))
            .map_err(Into::into)
    }

    /// Set in specific run
    pub fn set_in(&self, run: &RunId, key: &str, value: impl Into<Value>) -> Result<()> {
        self.substrate
            .kv_put(&run.0, key, value.into())
            .map(|_| ())
            .map_err(Into::into)
    }

    /// Delete in specific run
    pub fn delete_in(&self, run: &RunId, key: &str) -> Result<bool> {
        self.substrate
            .kv_delete(&run.0, key)
            .map_err(Into::into)
    }

    // ========================================
    // FULL CONTROL (returns version)
    // ========================================

    /// Put with version return
    pub fn put(&self, run: &RunId, key: &str, value: impl Into<Value>) -> Result<Version> {
        self.substrate
            .kv_put(&run.0, key, value.into())
            .map_err(Into::into)
    }

    /// Get at specific version
    pub fn get_at(&self, run: &RunId, key: &str, version: Version) -> Result<Versioned<Value>> {
        self.substrate
            .kv_get_at(&run.0, key, version)
            .map_err(Into::into)
    }

    /// Compare-and-swap
    pub fn cas(&self, run: &RunId, key: &str, expected: Version, value: impl Into<Value>) -> Result<bool> {
        self.substrate
            .kv_cas_version(&run.0, key, Some(expected), value.into())
            .map_err(Into::into)
    }

    // ========================================
    // HISTORY
    // ========================================

    /// Get version history
    pub fn history(&self, key: &str, limit: usize) -> Result<Vec<Versioned<Value>>> {
        self.substrate
            .kv_history(&RunId::default().0, key, Some(limit as u64), None)
            .map_err(Into::into)
    }

    // ========================================
    // BATCH OPERATIONS
    // ========================================

    /// Get multiple keys
    pub fn mget(&self, keys: &[&str]) -> Result<Vec<Option<Value>>> {
        self.substrate
            .kv_mget(&RunId::default().0, keys)
            .map(|v| v.into_iter().map(|opt| opt.map(|v| v.into_value())).collect())
            .map_err(Into::into)
    }

    /// Set multiple keys
    pub fn mset(&self, entries: &[(&str, Value)]) -> Result<()> {
        self.substrate
            .kv_mput(&RunId::default().0, entries)
            .map(|_| ())
            .map_err(Into::into)
    }

    /// Delete multiple keys
    pub fn mdelete(&self, keys: &[&str]) -> Result<u64> {
        self.substrate
            .kv_mdelete(&RunId::default().0, keys)
            .map_err(Into::into)
    }

    /// List keys with prefix
    pub fn keys(&self, prefix: &str) -> Result<Vec<String>> {
        self.substrate
            .kv_keys(&RunId::default().0, prefix, None)
            .map_err(Into::into)
    }
}
```

#### 4.2 Similar Pattern for Other Primitives

Each primitive follows the same pattern:
- Simple methods (default run, no version return)
- Run-scoped methods (`*_in`)
- Full control methods (`put`, `get_at`)
- Batch methods (`m*`)
- History methods

#### 4.3 Tasks

- [ ] Create `src/primitives/mod.rs`
- [ ] Implement `src/primitives/kv.rs`
- [ ] Implement `src/primitives/json.rs`
- [ ] Implement `src/primitives/events.rs`
- [ ] Implement `src/primitives/state.rs`
- [ ] Implement `src/primitives/vectors.rs`
- [ ] Implement `src/primitives/runs.rs`
- [ ] Write unit tests for each primitive
- [ ] Write integration tests

---

### Phase 5: Implement Missing Capabilities

**Goal**: Add capabilities identified as missing in the audit.

#### 5.1 Version/History Gaps

| Primitive | Missing | Implementation |
|-----------|---------|----------------|
| JSON | `get_at` | Add to JsonStore trait, implement |
| State | `get_at` | Add to StateCell trait, implement |
| All | `first_version` | Query history with limit=1, reverse |
| All | `current_version` | Get versioned, extract version |

#### 5.2 Storage Management

```rust
// Add to Strata
impl Strata {
    /// Get storage statistics
    pub fn storage_stats(&self) -> Result<StorageStats> {
        // Aggregate from storage layer
    }
}

pub struct StorageStats {
    pub total_size_bytes: u64,
    pub wal_size_bytes: u64,
    pub data_size_bytes: u64,
    pub version_count: u64,
    pub key_count: u64,
}
```

#### 5.3 Checkpoint API (P2)

```rust
impl Strata {
    /// Create a named checkpoint
    pub fn checkpoint(&self, name: &str) -> Result<()> { ... }

    /// List all checkpoints
    pub fn checkpoints(&self) -> Result<Vec<CheckpointInfo>> { ... }

    /// Read from a checkpoint
    pub fn at_checkpoint(&self, name: &str) -> Result<CheckpointView> { ... }

    /// Delete a checkpoint
    pub fn delete_checkpoint(&self, name: &str) -> Result<()> { ... }
}
```

#### 5.4 Tasks

- [ ] Add `json_get_at` to JsonStore
- [ ] Add `state_get_at` to StateCell
- [ ] Implement `storage_stats()`
- [ ] Implement `runs.size(&run)`
- [ ] Design checkpoint API (P2)
- [ ] Add missing convenience methods

---

### Phase 6: Deprecate Old API

**Goal**: Mark old API as deprecated.

#### 6.1 Add Deprecation Attributes

```rust
// In crates/api/src/substrate/mod.rs

#[deprecated(
    since = "0.12.0",
    note = "Use `strata::prelude::*` and `db.kv.set()` instead"
)]
pub use kv::KVStore;

#[deprecated(
    since = "0.12.0",
    note = "Use `strata::prelude::*` and `db.kv.set()` instead"
)]
pub use impl_::SubstrateImpl;
```

#### 6.2 Tasks

- [ ] Add `#[deprecated]` to all old API items
- [ ] Update README
- [ ] Update all examples
- [ ] Update all tests to use new API

---

### Phase 7: Internal Crate Cleanup

**Goal**: Mark internal crates as non-publishable and hide internals.

#### 7.1 Add `publish = false`

Update each internal crate's `Cargo.toml`:

```toml
# crates/primitives/Cargo.toml
[package]
name = "strata-primitives"
publish = false  # Internal crate
```

Crates to update:
- `strata-core`
- `strata-primitives`
- `strata-engine`
- `strata-api`
- `strata-storage`
- `strata-concurrency`
- `strata-durability`
- `strata-search`

#### 7.2 Add `#[doc(hidden)]`

For items that must be `pub` for sibling crate access but shouldn't be user-visible:

```rust
#[doc(hidden)]
pub struct InternalImplementation { ... }
```

#### 7.3 Tasks

- [ ] Add `publish = false` to all internal crates
- [ ] Add `#[doc(hidden)]` to internal-but-public items
- [ ] Verify `cargo doc` only shows public API
- [ ] Verify internal crates can't be depended on externally

---

### Phase 8: Documentation & Examples

**Goal**: Comprehensive documentation for the new API.

#### 8.1 Update README.md

```markdown
# Strata

Production-grade embedded database for AI agents.

## Quick Start

```rust
use strata::prelude::*;

let db = Strata::open("./my-db")?;

// Key-Value
db.kv.set("user:1", json!({"name": "Alice"}))?;
let user = db.kv.get("user:1")?;

// JSON Documents
db.json.set("profile", json!({"settings": {}}))?;
db.json.set_path("profile", "$.settings.theme", "dark")?;

// Events
db.events.append("activity", json!({"action": "login"}))?;

// Vectors
db.vectors.upsert("embeddings", "doc:1", vec![0.1, 0.2], json!({}))?;
let similar = db.vectors.search("embeddings", vec![0.1, 0.2], 10)?;

// Runs
let run = db.runs.create(json!({"agent": "my-agent"}))?;
db.kv.set_in(&run, "step", 1)?;
db.runs.close(&run)?;

db.shutdown()?;
```
```

#### 8.2 Create Examples

```
examples/
├── quickstart.rs        # Basic usage
├── transactions.rs      # Transaction patterns
├── versioning.rs        # Version history access
├── vectors.rs           # Vector similarity search
├── runs.rs              # Run lifecycle
├── retention.rs         # Retention policies
└── migration.rs         # Migrating from v0.11
```

#### 8.3 Tasks

- [ ] Update README.md
- [ ] Create example files
- [ ] Update rustdoc comments
- [ ] Generate and review docs
- [ ] Create getting started guide

---

### Phase 9: Testing & Validation

**Goal**: Comprehensive test coverage for new API.

#### 9.1 Test Structure

```
tests/
├── unified_api/
│   ├── kv_tests.rs
│   ├── json_tests.rs
│   ├── events_tests.rs
│   ├── state_tests.rs
│   ├── vectors_tests.rs
│   ├── runs_tests.rs
│   ├── transactions_tests.rs
│   └── versioning_tests.rs
├── migration/
│   └── compatibility_tests.rs
└── integration/
    └── full_workflow_tests.rs
```

#### 9.2 Test Categories

1. **Unit Tests**: Each method in isolation
2. **Integration Tests**: Multi-primitive workflows
3. **Migration Tests**: Old API still works
4. **Deprecation Tests**: Warnings are emitted
5. **Documentation Tests**: All doc examples compile

#### 9.3 Tasks

- [ ] Write unit tests for each primitive
- [ ] Write integration tests
- [ ] Write migration compatibility tests
- [ ] Verify all doc examples
- [ ] Run full test suite

---

## Implementation Order

### Sprint 1: Foundation (Phase 1-2)
- Create `strata` crate structure
- Implement core types and error handling
- Basic compilation verification

### Sprint 2: Primitives (Phase 3-4)
- Implement Strata entry point
- Implement all primitive wrappers
- Unit tests for each primitive

### Sprint 3: Gaps & Polish (Phase 5-6)
- Add missing capabilities
- Deprecate old API
- Write migration guide

### Sprint 4: Cleanup & Docs (Phase 7-8)
- Internal crate cleanup
- Documentation
- Examples

### Sprint 5: Validation (Phase 9)
- Comprehensive testing
- Performance validation
- Release preparation

---

## Success Criteria

1. **Single import**: `use strata::prelude::*` provides everything
2. **Clean naming**: No `kv_`, `json_`, etc. prefixes on methods
3. **Progressive disclosure**: Simple → run-scoped → full control works
4. **Backward compatible**: Old API works with deprecation warnings
5. **Documented**: All public items have rustdoc
6. **Tested**: 100% of new API has test coverage
7. **No leakage**: Internal types not visible to users

---

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Breaking changes | Deprecation path, not removal |
| Performance regression | Benchmark before/after |
| Missing edge cases | Comprehensive testing |
| Documentation gaps | Review all public items |

---

## Appendix: Method Count

### Before (v0.11)

```
Substrate traits: 11 (110 methods)
Facade traits: 9 (65 methods)
Total unique: ~90 methods
```

### After (v0.12)

```
Strata: 5 methods (open, builder, flush, shutdown, transaction)
KV: 20 methods
Json: 22 methods
Events: 12 methods
State: 12 methods
Vectors: 18 methods
Runs: 20 methods
Total: ~110 methods (but cleaner, no duplication)
```

The method count is similar, but:
- No duplication between layers
- Consistent naming
- Progressive disclosure
- Single entry point
