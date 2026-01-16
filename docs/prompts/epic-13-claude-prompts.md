# Epic 13: Primitives Foundation - Implementation Prompts

**Epic Goal**: Core infrastructure and common patterns for all M3 primitives.

**GitHub Issue**: [#159](https://github.com/anibjoshi/in-mem/issues/159)
**Status**: Ready to begin
**Dependencies**: M2 complete (Database, TransactionContext, transactions work)

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M3_ARCHITECTURE.md` is the GOSPEL for ALL M3 implementation.**

Before starting ANY story in this epic:
```bash
cat docs/architecture/M3_ARCHITECTURE.md
cat docs/milestones/M3_IMPLEMENTATION_PLAN.md
```

See `docs/prompts/M3_PROMPT_HEADER.md` for complete guidelines.

---

## Epic 13 Overview

### Scope
- Primitives crate structure with re-exports
- TypeTag extensions for new primitive types (KV=0x01, Event=0x02, State=0x03, Trace=0x04, Run=0x05)
- Key construction helpers per TypeTag
- Transaction extension trait infrastructure

### Success Criteria
- [ ] `crates/primitives` crate created with proper dependencies
- [ ] TypeTag enum extended with all 5 primitive values
- [ ] Key construction helpers: `Key::new_kv()`, `Key::new_event()`, etc.
- [ ] Transaction extension trait pattern documented and scaffolded
- [ ] All primitives re-exported from `lib.rs`
- [ ] Unit tests for key construction pass

### Component Breakdown
- **Story #166**: Primitives Crate Setup & TypeTag Extensions - BLOCKS ALL M3
- **Story #167**: Key Construction Helpers
- **Story #168**: Transaction Extension Trait Infrastructure

---

## Dependency Graph

```
Phase 1 (Sequential - CRITICAL):
  Story #166 (Primitives Crate Setup)
    └─> BLOCKS #167, #168, AND ALL OTHER M3 STORIES

Phase 2 (Parallel - 2 Claudes after #166):
  Story #167 (Key Construction Helpers)
  Story #168 (Transaction Extension Traits)
    └─> Both depend on #166
    └─> Independent of each other
```

---

## Parallelization Strategy

### Optimal Execution (2 Claudes)

| Phase | Duration | Claude 1 | Claude 2 |
|-------|----------|----------|----------|
| 1 | 3 hours | #166 Crate Setup | - |
| 2 | 3-4 hours | #167 Key Helpers | #168 Extension Traits |

**Total Wall Time**: ~7 hours (vs. ~10 hours sequential)

---

## Story #166: Primitives Crate Setup & TypeTag Extensions

**GitHub Issue**: [#166](https://github.com/anibjoshi/in-mem/issues/166)
**Estimated Time**: 3 hours
**Dependencies**: M2 complete
**Blocks**: ALL other M3 stories

### PREREQUISITE: Read the Architecture Spec

Before writing ANY code, read these sections of `docs/architecture/M3_ARCHITECTURE.md`:
- Section 3: Primitives Overview
- Section 9: Key Design (especially 9.1 TypeTag)
- Section 12: Invariant Enforcement

### Start Story

```bash
/opt/homebrew/bin/gh issue view 166
./scripts/start-story.sh 13 166 crate-setup
```

### Implementation Steps

#### Step 1: Create primitives crate structure

```bash
mkdir -p crates/primitives/src
```

#### Step 2: Create Cargo.toml

Create `crates/primitives/Cargo.toml`:

```toml
[package]
name = "in-mem-primitives"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
authors.workspace = true
license.workspace = true
repository.workspace = true
description = "High-level primitives for in-mem agent database"

[dependencies]
in-mem-core = { path = "../core" }
in-mem-engine = { path = "../engine" }
in-mem-concurrency = { path = "../concurrency" }
serde = { workspace = true }
serde_json = { workspace = true }

[dev-dependencies]
tempfile = { workspace = true }
```

#### Step 3: Update workspace Cargo.toml

Add to workspace members:
```toml
[workspace]
members = [
    # ... existing members
    "crates/primitives",
]
```

#### Step 4: Add TypeTag variants to core/src/types.rs

```rust
/// Type tags for different primitive types
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TypeTag {
    // M3 primitives
    KV = 0x01,
    Event = 0x02,
    State = 0x03,
    Trace = 0x04,
    Run = 0x05,

    // Reserved for future (M6+)
    Vector = 0x10,
}

impl TypeTag {
    /// Convert to byte representation
    pub fn as_byte(&self) -> u8 {
        *self as u8
    }

    /// Try to create from byte
    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0x01 => Some(TypeTag::KV),
            0x02 => Some(TypeTag::Event),
            0x03 => Some(TypeTag::State),
            0x04 => Some(TypeTag::Trace),
            0x05 => Some(TypeTag::Run),
            0x10 => Some(TypeTag::Vector),
            _ => None,
        }
    }
}
```

#### Step 5: Create lib.rs scaffold

Create `crates/primitives/src/lib.rs`:

```rust
//! Primitives layer for in-mem
//!
//! Provides five high-level primitives as stateless facades over the Database engine:
//! - **KVStore**: General-purpose key-value storage
//! - **EventLog**: Immutable append-only event stream with causal hash chaining
//! - **StateCell**: CAS-based versioned cells for coordination
//! - **TraceStore**: Structured reasoning traces with indexing
//! - **RunIndex**: Run lifecycle management
//!
//! ## Design Principle: Stateless Facades
//!
//! All primitives are logically stateful but operationally stateless.
//! They hold only an `Arc<Database>` reference and delegate all operations
//! to the transactional engine.
//!
//! ## Run Isolation
//!
//! Every operation is scoped to a `run_id`. Different runs cannot see
//! each other's data. This is enforced through key prefix isolation.

pub mod kv;
pub mod event_log;
pub mod state_cell;
pub mod trace;
pub mod run_index;
pub mod extensions;

// Re-exports will be added as primitives are implemented
// pub use kv::KVStore;
// pub use event_log::{EventLog, Event};
// pub use state_cell::{StateCell, State};
// pub use trace::{TraceStore, Trace, TraceType};
// pub use run_index::{RunIndex, RunMetadata, RunStatus};
```

#### Step 6: Create placeholder modules

Create empty placeholder files:
- `crates/primitives/src/kv.rs`
- `crates/primitives/src/event_log.rs`
- `crates/primitives/src/state_cell.rs`
- `crates/primitives/src/trace.rs`
- `crates/primitives/src/run_index.rs`
- `crates/primitives/src/extensions.rs`

Each with:
```rust
//! [Primitive Name] primitive implementation
//!
//! TODO: Implement in Story #XXX
```

#### Step 7: Write TypeTag tests

Add to `crates/core/src/types.rs` tests:

```rust
#[cfg(test)]
mod type_tag_tests {
    use super::*;

    #[test]
    fn test_type_tag_values() {
        assert_eq!(TypeTag::KV.as_byte(), 0x01);
        assert_eq!(TypeTag::Event.as_byte(), 0x02);
        assert_eq!(TypeTag::State.as_byte(), 0x03);
        assert_eq!(TypeTag::Trace.as_byte(), 0x04);
        assert_eq!(TypeTag::Run.as_byte(), 0x05);
        assert_eq!(TypeTag::Vector.as_byte(), 0x10);
    }

    #[test]
    fn test_type_tag_from_byte() {
        assert_eq!(TypeTag::from_byte(0x01), Some(TypeTag::KV));
        assert_eq!(TypeTag::from_byte(0x02), Some(TypeTag::Event));
        assert_eq!(TypeTag::from_byte(0x03), Some(TypeTag::State));
        assert_eq!(TypeTag::from_byte(0x04), Some(TypeTag::Trace));
        assert_eq!(TypeTag::from_byte(0x05), Some(TypeTag::Run));
        assert_eq!(TypeTag::from_byte(0x10), Some(TypeTag::Vector));
        assert_eq!(TypeTag::from_byte(0xFF), None);
    }

    #[test]
    fn test_type_tag_no_collisions() {
        // Ensure all TypeTag values are unique
        let tags = [
            TypeTag::KV,
            TypeTag::Event,
            TypeTag::State,
            TypeTag::Trace,
            TypeTag::Run,
            TypeTag::Vector,
        ];
        let bytes: Vec<u8> = tags.iter().map(|t| t.as_byte()).collect();
        let unique: std::collections::HashSet<u8> = bytes.iter().cloned().collect();
        assert_eq!(bytes.len(), unique.len(), "TypeTag values must be unique");
    }
}
```

### Validation

```bash
# Build primitives crate
~/.cargo/bin/cargo build -p in-mem-primitives

# Run core tests (TypeTag)
~/.cargo/bin/cargo test -p in-mem-core

# Build all
~/.cargo/bin/cargo build --all

# Check clippy
~/.cargo/bin/cargo clippy --all -- -D warnings

# Check formatting
~/.cargo/bin/cargo fmt --check
```

### Complete Story

```bash
./scripts/complete-story.sh 166
```

---

## Story #167: Key Construction Helpers

**GitHub Issue**: [#167](https://github.com/anibjoshi/in-mem/issues/167)
**Estimated Time**: 3 hours
**Dependencies**: Story #166

### Start Story

```bash
/opt/homebrew/bin/gh issue view 167
./scripts/start-story.sh 13 167 key-helpers
```

### Implementation Steps

#### Step 1: Add Key construction methods to core/src/types.rs

```rust
impl Key {
    /// Create KV store key
    pub fn new_kv(namespace: Namespace, user_key: &str) -> Self {
        Self::new(namespace, TypeTag::KV, user_key.as_bytes())
    }

    /// Create Event log key (sequence number as big-endian bytes)
    pub fn new_event(namespace: Namespace, sequence: u64) -> Self {
        Self::new(namespace, TypeTag::Event, &sequence.to_be_bytes())
    }

    /// Create Event log metadata key
    pub fn new_event_meta(namespace: Namespace) -> Self {
        Self::new(namespace, TypeTag::Event, b"__meta__")
    }

    /// Create State cell key
    pub fn new_state(namespace: Namespace, cell_name: &str) -> Self {
        Self::new(namespace, TypeTag::State, cell_name.as_bytes())
    }

    /// Create Trace store key
    pub fn new_trace(namespace: Namespace, trace_id: &str) -> Self {
        Self::new(namespace, TypeTag::Trace, trace_id.as_bytes())
    }

    /// Create Trace index key
    pub fn new_trace_index(
        namespace: Namespace,
        index_type: &str,
        index_value: &str,
        trace_id: &str,
    ) -> Self {
        let key_data = format!("__idx_{}__{}__{}", index_type, index_value, trace_id);
        Self::new(namespace, TypeTag::Trace, key_data.as_bytes())
    }

    /// Create Run index key
    pub fn new_run(namespace: Namespace, run_id: &str) -> Self {
        Self::new(namespace, TypeTag::Run, run_id.as_bytes())
    }

    /// Create Run index secondary index key
    pub fn new_run_index(
        namespace: Namespace,
        index_type: &str,
        index_value: &str,
        run_id: &str,
    ) -> Self {
        let key_data = format!("__idx_{}__{}__{}", index_type, index_value, run_id);
        Self::new(namespace, TypeTag::Run, key_data.as_bytes())
    }

    /// Extract user key as string (if valid UTF-8)
    pub fn user_key_string(&self) -> Option<String> {
        String::from_utf8(self.user_key.clone()).ok()
    }
}
```

#### Step 2: Write comprehensive tests

```rust
#[cfg(test)]
mod key_construction_tests {
    use super::*;

    fn test_namespace() -> Namespace {
        Namespace::new("tenant", "app", "agent", "run123")
    }

    #[test]
    fn test_new_kv() {
        let ns = test_namespace();
        let key = Key::new_kv(ns.clone(), "my-key");
        assert_eq!(key.type_tag, TypeTag::KV);
        assert_eq!(key.user_key, b"my-key");
        assert_eq!(key.namespace, ns);
    }

    #[test]
    fn test_new_event() {
        let ns = test_namespace();
        let key = Key::new_event(ns.clone(), 42);
        assert_eq!(key.type_tag, TypeTag::Event);
        assert_eq!(key.user_key, 42u64.to_be_bytes());
    }

    #[test]
    fn test_new_event_meta() {
        let ns = test_namespace();
        let key = Key::new_event_meta(ns);
        assert_eq!(key.type_tag, TypeTag::Event);
        assert_eq!(key.user_key, b"__meta__");
    }

    #[test]
    fn test_new_state() {
        let ns = test_namespace();
        let key = Key::new_state(ns, "workflow/status");
        assert_eq!(key.type_tag, TypeTag::State);
        assert_eq!(key.user_key_string(), Some("workflow/status".to_string()));
    }

    #[test]
    fn test_new_trace() {
        let ns = test_namespace();
        let key = Key::new_trace(ns, "trace-abc123");
        assert_eq!(key.type_tag, TypeTag::Trace);
        assert_eq!(key.user_key_string(), Some("trace-abc123".to_string()));
    }

    #[test]
    fn test_new_trace_index() {
        let ns = test_namespace();
        let key = Key::new_trace_index(ns, "by-type", "ToolCall", "trace-123");
        assert_eq!(key.type_tag, TypeTag::Trace);
        assert!(key.user_key_string().unwrap().contains("__idx_by-type__ToolCall__trace-123"));
    }

    #[test]
    fn test_new_run() {
        let ns = test_namespace();
        let key = Key::new_run(ns, "run-xyz");
        assert_eq!(key.type_tag, TypeTag::Run);
        assert_eq!(key.user_key_string(), Some("run-xyz".to_string()));
    }

    #[test]
    fn test_event_keys_sort_by_sequence() {
        let ns = test_namespace();
        let key1 = Key::new_event(ns.clone(), 1);
        let key2 = Key::new_event(ns.clone(), 10);
        let key3 = Key::new_event(ns.clone(), 100);

        // Big-endian encoding ensures lexicographic sort = numeric sort
        assert!(key1 < key2);
        assert!(key2 < key3);
    }

    #[test]
    fn test_keys_with_same_inputs_are_equal() {
        let ns1 = test_namespace();
        let ns2 = test_namespace();
        let key1 = Key::new_kv(ns1, "same-key");
        let key2 = Key::new_kv(ns2, "same-key");
        assert_eq!(key1, key2);
    }
}
```

### Validation

```bash
~/.cargo/bin/cargo test -p in-mem-core -- key_construction
~/.cargo/bin/cargo clippy --all -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### Complete Story

```bash
./scripts/complete-story.sh 167
```

---

## Story #168: Transaction Extension Trait Infrastructure

**GitHub Issue**: [#168](https://github.com/anibjoshi/in-mem/issues/168)
**Estimated Time**: 4 hours
**Dependencies**: Story #166

### Start Story

```bash
/opt/homebrew/bin/gh issue view 168
./scripts/start-story.sh 13 168 extension-traits
```

### Implementation Steps

#### Step 1: Create extensions module

Create `crates/primitives/src/extensions.rs`:

```rust
//! Transaction extension traits for cross-primitive operations
//!
//! ## Design Principle
//!
//! Extension traits allow multiple primitives to participate in a single
//! transaction. Each trait provides domain-specific methods that operate
//! on a `TransactionContext`.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use in_mem_primitives::extensions::*;
//!
//! db.transaction(run_id, |txn| {
//!     // KV operation
//!     txn.kv_put("key", value)?;
//!
//!     // Event operation
//!     txn.event_append("type", payload)?;
//!
//!     // State operation
//!     txn.state_cas("cell", version, new_value)?;
//!
//!     Ok(())
//! })?;
//! ```
//!
//! ## Implementation Note
//!
//! Extension traits DELEGATE to primitive internals - they do NOT
//! reimplement logic. Each trait implementation calls the same
//! internal functions used by the standalone primitive API.

use in_mem_core::{Result, Value};

// Forward declarations - traits are defined here, implementations
// are added in their respective primitive modules.

/// KV operations within a transaction
///
/// Implemented in `kv.rs` (Story #173)
pub trait KVStoreExt {
    /// Get a value by key
    fn kv_get(&mut self, key: &str) -> Result<Option<Value>>;

    /// Put a value
    fn kv_put(&mut self, key: &str, value: Value) -> Result<()>;

    /// Delete a key
    fn kv_delete(&mut self, key: &str) -> Result<()>;
}

/// Event log operations within a transaction
///
/// Implemented in `event_log.rs` (Story #179)
pub trait EventLogExt {
    /// Append an event and return sequence number
    fn event_append(&mut self, event_type: &str, payload: Value) -> Result<u64>;

    /// Read an event by sequence number
    fn event_read(&mut self, sequence: u64) -> Result<Option<Value>>;
}

/// State cell operations within a transaction
///
/// Implemented in `state_cell.rs` (Story #184)
pub trait StateCellExt {
    /// Read current state
    fn state_read(&mut self, name: &str) -> Result<Option<Value>>;

    /// Compare-and-swap update
    fn state_cas(&mut self, name: &str, expected_version: u64, new_value: Value) -> Result<u64>;

    /// Unconditional set
    fn state_set(&mut self, name: &str, value: Value) -> Result<u64>;
}

/// Trace store operations within a transaction
///
/// Implemented in `trace.rs` (Story #190)
pub trait TraceStoreExt {
    /// Record a trace and return trace ID
    fn trace_record(&mut self, trace_type: &str, metadata: Value) -> Result<String>;

    /// Record a child trace
    fn trace_record_child(
        &mut self,
        parent_id: &str,
        trace_type: &str,
        metadata: Value,
    ) -> Result<String>;
}

// Note: RunIndex does not have an extension trait because run operations
// are typically done outside of cross-primitive transactions.
```

#### Step 2: Update lib.rs to export extensions

Update `crates/primitives/src/lib.rs`:

```rust
pub mod extensions;
pub use extensions::*;
```

#### Step 3: Write trait compilation tests

Add to `crates/primitives/src/extensions.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // These tests verify trait definitions compile correctly.
    // Implementation tests will be in their respective primitive stories.

    #[test]
    fn test_traits_are_object_safe() {
        // Verify traits can be used as trait objects if needed
        fn _accepts_kv_ext(_ext: &dyn KVStoreExt) {}
        fn _accepts_event_ext(_ext: &dyn EventLogExt) {}
        fn _accepts_state_ext(_ext: &dyn StateCellExt) {}
        fn _accepts_trace_ext(_ext: &dyn TraceStoreExt) {}
    }
}
```

### Validation

```bash
~/.cargo/bin/cargo build -p in-mem-primitives
~/.cargo/bin/cargo test -p in-mem-primitives
~/.cargo/bin/cargo clippy --all -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### Complete Story

```bash
./scripts/complete-story.sh 168
```

---

## Epic 13 Completion Checklist

Once ALL 3 stories are complete and merged to `epic-13-primitives-foundation`:

### 1. Final Validation

```bash
# All tests pass
~/.cargo/bin/cargo test --all

# Release build clean
~/.cargo/bin/cargo build --release --all

# No clippy warnings
~/.cargo/bin/cargo clippy --all -- -D warnings

# Formatting clean
~/.cargo/bin/cargo fmt --check
```

### 2. Verify Deliverables

- [ ] `crates/primitives/Cargo.toml` exists
- [ ] `crates/primitives/src/lib.rs` has all module declarations
- [ ] TypeTag enum has KV, Event, State, Trace, Run values
- [ ] Key construction helpers work for all primitive types
- [ ] Extension traits are defined and compile
- [ ] All unit tests pass

### 3. Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-13-primitives-foundation -m "Epic 13: Primitives Foundation

Complete:
- Primitives crate structure
- TypeTag extensions (KV=0x01, Event=0x02, State=0x03, Trace=0x04, Run=0x05)
- Key construction helpers
- Transaction extension trait infrastructure

Stories:
- #166: Primitives Crate Setup & TypeTag Extensions
- #167: Key Construction Helpers
- #168: Transaction Extension Trait Infrastructure

This unblocks ALL other M3 epics (14-19).
"

git push origin develop
```

### 4. Close Epic Issue

```bash
/opt/homebrew/bin/gh issue close 159 --comment "Epic 13: Primitives Foundation - COMPLETE

All 3 stories completed:
- #166: Primitives Crate Setup & TypeTag Extensions
- #167: Key Construction Helpers
- #168: Transaction Extension Trait Infrastructure

Epics 14-18 are now unblocked for parallel implementation.
"
```

---

## Critical Notes

### This Epic Blocks Everything

Story #166 MUST be completed before ANY other M3 work can begin. It establishes:
- The primitives crate structure
- TypeTag values that ALL primitives use
- The foundation for key construction

### TypeTag Values Are Fixed

Once M3 is released, these TypeTag values become part of the on-disk format:
- KV = 0x01
- Event = 0x02
- State = 0x03
- Trace = 0x04
- Run = 0x05
- Vector = 0x10 (reserved for M6)

Do NOT change these values after implementation begins.

---

## Summary

Epic 13 establishes the foundation for all M3 primitives:
- Creates the primitives crate structure
- Defines TypeTag values for primitive type discrimination
- Provides key construction helpers for all primitive types
- Scaffolds transaction extension traits for cross-primitive transactions

**After Epic 13**: Epics 14-18 can begin in parallel, each implementing a specific primitive.
