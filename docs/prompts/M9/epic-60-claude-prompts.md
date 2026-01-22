# Epic 60: Core Types - Implementation Prompts

**Epic Goal**: Define universal types that express the seven invariants

**GitHub Issue**: [#464](https://github.com/anibjoshi/in-mem/issues/464)
**Status**: Ready to begin
**Dependencies**: M8 complete
**Phase**: 1 (Foundation)

---

## NAMING CONVENTION - CRITICAL

> **NEVER use "M9" or "Strata" in the actual codebase or comments.**
>
> - "M9" is an internal milestone tracker only - do not use it in code, comments, or user-facing text
> - All existing crates refer to the database as "in-mem" - use this name consistently
> - Do not use "Strata" anywhere in the codebase
> - This applies to: code, comments, docstrings, error messages, log messages, test names
>
> **CORRECT**: `//! Universal entity reference for any in-mem entity`
> **WRONG**: `//! Universal entity reference for any Strata entity`

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M9_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M9_ARCHITECTURE.md`
2. **Primitive Contract**: `docs/architecture/PRIMITIVE_CONTRACT.md`
3. **Epic Spec**: `docs/milestones/M9/EPIC_60_CORE_TYPES.md`
4. **Prompt Header**: `docs/prompts/M9/M9_PROMPT_HEADER.md` for the 4 architectural rules

**The architecture spec is LAW.** Epic docs provide implementation details but MUST NOT contradict the architecture spec.

---

## Epic 60 Overview

### Scope
- EntityRef enum with variants for all 7 primitives
- Versioned<T> wrapper type for versioned reads
- Version enum (TxnId, Sequence, Counter)
- Timestamp type for temporal tracking
- PrimitiveType enum
- **RunName + RunId dual identity model** (semantic names for users, internal UUIDs for storage)

### Key Rule: Types Express Invariants

> These types are the API expression of the seven invariants from PRIMITIVE_CONTRACT.md.
> Every type has a specific purpose tied to an invariant.

| Type | Invariant | Notes |
|------|-----------|-------|
| EntityRef | Invariant 1: Everything is Addressable | 7 variants for 7 primitives |
| Versioned<T> | Invariant 2: Everything is Versioned | **Canonical versioned type** |
| VersionedValue | Invariant 2: Everything is Versioned | Type alias: `Versioned<Value>` |
| Version | Invariant 2: Everything is Versioned | TxnId, Sequence, Counter |
| RunName | Invariant 5: Semantic user-facing run identity | Human-readable names |
| RunId | Invariant 5: Internal storage identity (UUID) | Unchanged from current |
| PrimitiveType | Invariant 6: Everything is Introspectable | 7 primitive types |
| Timestamp | Invariant 2: Everything is Versioned (temporal) | Microseconds since epoch |

### Success Criteria
- [ ] `EntityRef` enum with variants for all 7 primitives
- [ ] `EntityRef::run_id()` method returns the run for any entity
- [ ] `EntityRef::primitive_type()` method returns `PrimitiveType`
- [ ] `Versioned<T>` with value, version, timestamp fields
- [ ] `Versioned<T>::map()` for transforming inner value
- [ ] `Version` enum: TxnId(u64), Sequence(u64), Counter(u64)
- [ ] `Version::as_u64()` for numeric comparison
- [ ] `Timestamp` type with `now()` constructor
- [ ] `RunName` newtype for semantic user-facing identity
- [ ] `RunId` kept as `RunId(Uuid)` for internal storage
- [ ] Database mapping: `RunName ↔ RunId` bidirectional
- [ ] `db.run(name)` API accepts `RunName` or `&str`
- [ ] All types implement Debug, Clone; IDs implement Hash, Eq

### Component Breakdown
- **Story #469**: EntityRef Enum Implementation - FOUNDATION
- **Story #470**: Versioned<T> Wrapper Type - FOUNDATION
- **Story #471**: Version Enum - FOUNDATION
- **Story #472**: Timestamp Type - FOUNDATION
- **Story #473**: PrimitiveType Enum - HIGH
- **Story #474**: RunName + RunId Dual Identity Model - FOUNDATION

---

## File Organization: The Contract Module

> **Critical**: All Epic 60 types go in `crates/core/src/contract/`, NOT scattered across files.
> These types define the semantic contract of the system - they must live together.

### Directory Structure

Create this structure FIRST before implementing any stories:

```bash
mkdir -p crates/core/src/contract
touch crates/core/src/contract/mod.rs
```

**Target structure**:
```
crates/core/src/
├── contract/              # ALL M9 contract types live here
│   ├── mod.rs             # Module exports
│   ├── entity_ref.rs      # EntityRef, PrimitiveType
│   ├── versioned.rs       # Versioned<T>
│   ├── version.rs         # Version enum
│   ├── timestamp.rs       # Timestamp type
│   └── run_name.rs        # RunName type
├── types.rs               # Internal types (RunId stays here)
├── value.rs               # Value enum (+ VersionedValue alias)
├── search_types.rs        # Search types (+ DocRef, PrimitiveKind aliases)
└── lib.rs                 # Re-exports contract module prominently
```

### contract/mod.rs Template

```rust
//! Contract types for the in-mem database
//!
//! This module defines the semantic contract of the system.
//! These types encode the seven invariants and define what it
//! means to interact with any entity in the database.
//!
//! ## What Belongs Here
//!
//! Types that are part of the universal mental model:
//! - EntityRef, PrimitiveType (Invariant 1: Addressable)
//! - Versioned<T>, Version, Timestamp (Invariant 2: Versioned)
//! - RunName (Invariant 5: Run-scoped)
//!
//! ## What Does NOT Belong Here
//!
//! Implementation details:
//! - RunId(Uuid) - internal storage identity (stays in types.rs)
//! - Key, Namespace - internal addressing (stays in types.rs)
//! - ShardedStore internals, WAL formats, etc.

// === Invariant 1: Everything is Addressable ===
mod entity_ref;
pub use entity_ref::{EntityRef, PrimitiveType};

// === Invariant 2: Everything is Versioned ===
mod versioned;
mod version;
mod timestamp;
pub use versioned::Versioned;
pub use version::Version;
pub use timestamp::Timestamp;

// === Invariant 5: Everything is Run-Scoped ===
mod run_name;
pub use run_name::RunName;
```

### Updated lib.rs Exports

After creating the contract module, update `crates/core/src/lib.rs`:

```rust
//! Core types for in-mem
//!
//! ## Contract Types (M9)
//!
//! These types define the semantic contract of the system:
//! - [`EntityRef`] - Universal addressing for any entity
//! - [`Versioned<T>`] - Wrapper for versioned reads
//! - [`Version`] - Version identifier
//! - [`Timestamp`] - Temporal tracking
//! - [`RunName`] - Semantic run identity
//! - [`PrimitiveType`] - Primitive discriminator

// Contract module (M9) - THE semantic contract
pub mod contract;
pub use contract::{
    EntityRef, PrimitiveType,
    Versioned, Version, Timestamp,
    RunName,
};

// ... existing exports ...
```

---

## Dependency Graph

```
Story #469 (EntityRef) ────┬──> All other types depend on EntityRef
                           │
Story #471 (Version) ──────┼──> Story #470 (Versioned<T>)
                           │
Story #472 (Timestamp) ────┘

Story #473 (PrimitiveType) ──> Story #469 (EntityRef::primitive_type)

Story #474 (RunId) ──> Story #469 (EntityRef uses RunId)
```

**Recommended Order**: #474 (RunId) → #473 (PrimitiveType) → #471 (Version) → #472 (Timestamp) → #469 (EntityRef) → #470 (Versioned<T>)

---

## Story #469: EntityRef Enum Implementation

**GitHub Issue**: [#469](https://github.com/anibjoshi/in-mem/issues/469)
**Estimated Time**: 3 hours
**Dependencies**: Stories #473, #474
**Blocks**: All Epic 61, 62, 63 stories

### Critical: Unification with DocRef

> **The Semantic Problem**: The codebase already has `DocRef` in `crates/core/src/search_types.rs`.
> M9 cannot introduce a competing `EntityRef` - that creates parallel identity systems.
>
> **Solution**: `EntityRef` is the canonical type. `DocRef` becomes a type alias.

**Current DocRef** (in `crates/core/src/search_types.rs`):
```rust
pub enum DocRef {
    Kv { key: Key },                              // run_id inside Key.namespace
    Json { key: Key, doc_id: JsonDocId },         // run_id inside Key.namespace
    Event { log_key: Key, seq: u64 },             // run_id inside Key.namespace
    State { key: Key },                           // run_id inside Key.namespace
    Trace { key: Key, span_id: u64 },             // run_id inside Key.namespace
    Run { run_id: RunId },                        // explicit
    Vector { collection: String, key: String, run_id: RunId }, // explicit
}
```

**M9 EntityRef** makes run_id explicit and top-level:
```rust
pub enum EntityRef {
    Kv { run_id: RunId, key: String },
    Event { run_id: RunId, sequence: u64 },
    State { run_id: RunId, name: String },
    Trace { run_id: RunId, trace_id: TraceId },
    Run { run_id: RunId },
    Json { run_id: RunId, doc_id: JsonDocId },
    Vector { run_id: RunId, collection: String, vector_id: VectorId },
}

// Type alias for backwards compatibility
pub type DocRef = EntityRef;
pub type PrimitiveKind = PrimitiveType;
```

### Start Story

```bash
gh issue view 469
./scripts/start-story.sh 60 469 entity-ref
```

### Implementation Steps

#### Step 1: Create entity_ref.rs in contract module

Create `crates/core/src/contract/entity_ref.rs`:

```rust
//! Universal entity reference for any in-mem entity
//!
//! This type expresses Invariant 1: Everything is Addressable.
//! Every entity in the database has a stable identity that can be:
//! - Referenced
//! - Stored
//! - Passed between systems
//! - Used to retrieve the entity later
//!
//! NOTE: This type unifies with the existing DocRef from search_types.
//! After M9, `DocRef = EntityRef`.

use crate::{RunId, PrimitiveType};

// Import ID types from primitives (adjust paths as needed)
use crate::TraceId;
use crate::JsonDocId;
use crate::VectorId;

/// Universal entity reference for any in-mem entity
///
/// Every entity can be uniquely identified by an EntityRef.
/// This enables uniform addressing across all primitives.
///
/// ## Unification with DocRef
///
/// This type replaces the previous `DocRef` enum from search_types.
/// The old type becomes a type alias: `pub type DocRef = EntityRef;`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EntityRef {
    /// KV entry: run + key
    Kv { run_id: RunId, key: String },

    /// Event: run + sequence number
    Event { run_id: RunId, sequence: u64 },

    /// State cell: run + name
    State { run_id: RunId, name: String },

    /// Trace: run + trace_id
    Trace { run_id: RunId, trace_id: TraceId },

    /// Run metadata
    Run { run_id: RunId },

    /// JSON document: run + doc_id
    Json { run_id: RunId, doc_id: JsonDocId },

    /// Vector: run + collection + vector_id
    Vector { run_id: RunId, collection: String, vector_id: VectorId },
}

impl EntityRef {
    // =========================================================================
    // Constructors
    // =========================================================================

    /// Create a KV entity reference
    pub fn kv(run_id: RunId, key: impl Into<String>) -> Self {
        EntityRef::Kv { run_id, key: key.into() }
    }

    /// Create an Event entity reference
    pub fn event(run_id: RunId, sequence: u64) -> Self {
        EntityRef::Event { run_id, sequence }
    }

    /// Create a State entity reference
    pub fn state(run_id: RunId, name: impl Into<String>) -> Self {
        EntityRef::State { run_id, name: name.into() }
    }

    /// Create a Trace entity reference
    pub fn trace(run_id: RunId, trace_id: TraceId) -> Self {
        EntityRef::Trace { run_id, trace_id }
    }

    /// Create a Run entity reference
    pub fn run(run_id: RunId) -> Self {
        EntityRef::Run { run_id }
    }

    /// Create a Json entity reference
    pub fn json(run_id: RunId, doc_id: JsonDocId) -> Self {
        EntityRef::Json { run_id, doc_id }
    }

    /// Create a Vector entity reference
    pub fn vector(run_id: RunId, collection: impl Into<String>, vector_id: VectorId) -> Self {
        EntityRef::Vector { run_id, collection: collection.into(), vector_id }
    }

    // =========================================================================
    // Accessors
    // =========================================================================

    /// Get the run this entity belongs to
    ///
    /// All entities belong to exactly one run (Invariant 5).
    pub fn run_id(&self) -> &RunId {
        match self {
            EntityRef::Kv { run_id, .. } => run_id,
            EntityRef::Event { run_id, .. } => run_id,
            EntityRef::State { run_id, .. } => run_id,
            EntityRef::Trace { run_id, .. } => run_id,
            EntityRef::Run { run_id } => run_id,
            EntityRef::Json { run_id, .. } => run_id,
            EntityRef::Vector { run_id, .. } => run_id,
        }
    }

    /// Get the primitive type for this entity
    pub fn primitive_type(&self) -> PrimitiveType {
        match self {
            EntityRef::Kv { .. } => PrimitiveType::Kv,
            EntityRef::Event { .. } => PrimitiveType::Event,
            EntityRef::State { .. } => PrimitiveType::State,
            EntityRef::Trace { .. } => PrimitiveType::Trace,
            EntityRef::Run { .. } => PrimitiveType::Run,
            EntityRef::Json { .. } => PrimitiveType::Json,
            EntityRef::Vector { .. } => PrimitiveType::Vector,
        }
    }
}

impl std::fmt::Display for EntityRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntityRef::Kv { run_id, key } => write!(f, "kv:{}:{}", run_id, key),
            EntityRef::Event { run_id, sequence } => write!(f, "event:{}:{}", run_id, sequence),
            EntityRef::State { run_id, name } => write!(f, "state:{}:{}", run_id, name),
            EntityRef::Trace { run_id, trace_id } => write!(f, "trace:{}:{}", run_id, trace_id),
            EntityRef::Run { run_id } => write!(f, "run:{}", run_id),
            EntityRef::Json { run_id, doc_id } => write!(f, "json:{}:{}", run_id, doc_id),
            EntityRef::Vector { run_id, collection, vector_id } => {
                write!(f, "vector:{}:{}:{}", run_id, collection, vector_id)
            }
        }
    }
}
```

#### Step 2: Update contract/mod.rs

```rust
mod entity_ref;
pub use entity_ref::{EntityRef, PrimitiveType};
```

#### Step 3: Update search_types.rs to use type alias

After creating `entity_ref.rs`, update `crates/core/src/search_types.rs`:

```rust
// At the top of search_types.rs, add:
use crate::{EntityRef, PrimitiveType};

// Replace the DocRef enum with a type alias:
/// Document reference for search results
///
/// This is now a type alias for `EntityRef`.
/// The original enum has been unified into the universal `EntityRef` type.
pub type DocRef = EntityRef;

// Replace the PrimitiveKind enum with a type alias:
/// Primitive kind discriminator
///
/// This is now a type alias for `PrimitiveType`.
pub type PrimitiveKind = PrimitiveType;

// REMOVE the old DocRef and PrimitiveKind enum definitions.
// The methods are now provided by EntityRef and PrimitiveType.
```

### Compatibility Note

The following DocRef usage patterns continue to work:

```rust
// Old code (still works via type alias)
let doc_ref = DocRef::Kv { run_id, key: "mykey".to_string() };
assert_eq!(doc_ref.primitive_kind(), PrimitiveKind::Kv);

// New code (preferred)
let entity_ref: EntityRef = EntityRef::kv(run_id, "mykey");
assert_eq!(entity_ref.primitive_type(), PrimitiveType::Kv);
```

### Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_ref_kv() {
        let run_id = RunId::new("test-run");
        let entity_ref = EntityRef::kv(run_id.clone(), "my-key");

        assert_eq!(entity_ref.run_id(), &run_id);
        assert_eq!(entity_ref.primitive_type(), PrimitiveType::Kv);
    }

    #[test]
    fn test_entity_ref_event() {
        let run_id = RunId::new("test-run");
        let entity_ref = EntityRef::event(run_id.clone(), 42);

        assert_eq!(entity_ref.run_id(), &run_id);
        assert_eq!(entity_ref.primitive_type(), PrimitiveType::Event);
    }

    #[test]
    fn test_entity_ref_display() {
        let run_id = RunId::new("r1");
        let entity_ref = EntityRef::kv(run_id, "key");
        assert_eq!(format!("{}", entity_ref), "kv:r1:key");
    }

    #[test]
    fn test_entity_ref_equality() {
        let run_id = RunId::new("test");
        let ref1 = EntityRef::kv(run_id.clone(), "key");
        let ref2 = EntityRef::kv(run_id.clone(), "key");
        let ref3 = EntityRef::kv(run_id.clone(), "other");

        assert_eq!(ref1, ref2);
        assert_ne!(ref1, ref3);
    }

    #[test]
    fn test_entity_ref_hash() {
        use std::collections::HashSet;

        let run_id = RunId::new("test");
        let ref1 = EntityRef::kv(run_id.clone(), "key");
        let ref2 = EntityRef::kv(run_id.clone(), "key");

        let mut set = HashSet::new();
        set.insert(ref1);
        assert!(set.contains(&ref2));
    }

    // === DocRef Type Alias Compatibility Tests ===

    #[test]
    fn test_doc_ref_type_alias() {
        // This test verifies the type alias works correctly
        use crate::search_types::DocRef;

        let run_id = RunId::new("test");
        let doc_ref: DocRef = EntityRef::kv(run_id.clone(), "key");

        // DocRef is just EntityRef, so all methods work
        assert_eq!(doc_ref.run_id(), &run_id);
        assert_eq!(doc_ref.primitive_type(), PrimitiveType::Kv);
    }

    #[test]
    fn test_primitive_kind_type_alias() {
        // This test verifies the PrimitiveKind alias works
        use crate::search_types::PrimitiveKind;

        let kind: PrimitiveKind = PrimitiveType::Kv;
        assert_eq!(kind.name(), "KV");
    }
}
```

### Validation

```bash
~/.cargo/bin/cargo test -p in-mem-core -- entity_ref
~/.cargo/bin/cargo clippy -p in-mem-core -- -D warnings
```

### Complete Story

```bash
./scripts/complete-story.sh 469
```

---

## Story #470: Versioned<T> Wrapper Type

**GitHub Issue**: [#470](https://github.com/anibjoshi/in-mem/issues/470)
**Estimated Time**: 2 hours
**Dependencies**: Stories #471, #472
**Blocks**: All Epic 61 stories

### Critical: Unification with VersionedValue

> **The Semantic Problem**: The codebase already has `VersionedValue` in `crates/core/src/value.rs`.
> M9 cannot introduce a competing `Versioned<T>` - that creates semantic duplication.
>
> **Solution**: `Versioned<T>` is the canonical type. `VersionedValue` becomes a type alias.

**Current VersionedValue** (in `crates/core/src/value.rs`):
```rust
pub struct VersionedValue {
    pub value: Value,
    pub version: u64,
    pub timestamp: Timestamp,  // Note: i64 (Unix seconds)
    pub ttl: Option<Duration>,
}
```

**M9 Versioned<T>** must be compatible:
```rust
pub struct Versioned<T> {
    pub value: T,
    pub version: Version,      // Wrapper around u64
    pub timestamp: Timestamp,  // M9 Timestamp (microseconds)
    pub ttl: Option<Duration>, // Added for compatibility
}

// Then in value.rs:
pub type VersionedValue = Versioned<Value>;
```

### Start Story

```bash
gh issue view 470
./scripts/start-story.sh 60 470 versioned-wrapper
```

### Implementation

Create `crates/core/src/contract/versioned.rs`:

```rust
//! Versioned value wrapper
//!
//! This type expresses Invariant 2: Everything is Versioned.
//! When you read an entity, you know:
//! 1. Which version you are looking at
//! 2. When that version came into existence
//!
//! NOTE: This type unifies with the existing VersionedValue.
//! After M9, `VersionedValue = Versioned<Value>`.

use crate::{Version, Timestamp};
use std::time::Duration;

/// A value with its version and timestamp
///
/// This wrapper ensures version information is never lost when reading entities.
/// All read operations return `Versioned<T>`.
///
/// ## Unification with VersionedValue
///
/// This type replaces the previous `VersionedValue` struct. The old type
/// becomes a type alias: `pub type VersionedValue = Versioned<Value>;`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Versioned<T> {
    /// The actual value
    pub value: T,
    /// The version of this value
    pub version: Version,
    /// When this version was created
    pub timestamp: Timestamp,
    /// Optional time-to-live (for primitives that support TTL)
    pub ttl: Option<Duration>,
}

impl<T> Versioned<T> {
    /// Create a new versioned value (TTL defaults to None)
    pub fn new(value: T, version: Version, timestamp: Timestamp) -> Self {
        Self { value, version, timestamp, ttl: None }
    }

    /// Create a versioned value with TTL
    pub fn with_ttl(value: T, version: Version, timestamp: Timestamp, ttl: Option<Duration>) -> Self {
        Self { value, version, timestamp, ttl }
    }

    /// Transform the inner value, preserving version info
    ///
    /// Useful for converting between value types while keeping version metadata.
    pub fn map<U, F>(self, f: F) -> Versioned<U>
    where
        F: FnOnce(T) -> U,
    {
        Versioned {
            value: f(self.value),
            version: self.version,
            timestamp: self.timestamp,
            ttl: self.ttl,
        }
    }

    /// Get a reference to the inner value with version info
    pub fn as_ref(&self) -> Versioned<&T> {
        Versioned {
            value: &self.value,
            version: self.version.clone(),
            timestamp: self.timestamp,
            ttl: self.ttl,
        }
    }

    /// Extract just the value, discarding version info
    ///
    /// **Deprecated**: Prefer using the full Versioned<T> to preserve version info.
    /// This method exists only for migration from old APIs.
    #[deprecated(note = "Prefer using Versioned<T> to preserve version info")]
    pub fn into_value(self) -> T {
        self.value
    }

    /// Get a reference to the value
    pub fn value(&self) -> &T {
        &self.value
    }

    /// Get the version
    pub fn version(&self) -> &Version {
        &self.version
    }

    /// Get the timestamp
    pub fn timestamp(&self) -> Timestamp {
        self.timestamp
    }

    /// Check if the value has expired based on TTL
    ///
    /// This method provides compatibility with VersionedValue::is_expired().
    pub fn is_expired(&self) -> bool {
        if let Some(ttl) = self.ttl {
            let now = Timestamp::now();
            let elapsed_micros = now.as_micros().saturating_sub(self.timestamp.as_micros());
            elapsed_micros >= ttl.as_micros() as u64
        } else {
            false
        }
    }
}

impl<T: Default> Default for Versioned<T> {
    fn default() -> Self {
        Self {
            value: T::default(),
            version: Version::TxnId(0),
            timestamp: Timestamp::EPOCH,
            ttl: None,
        }
    }
}
```

### Step 2: Update value.rs to use type alias

After creating `versioned.rs`, update `crates/core/src/value.rs`:

```rust
// At the top of value.rs, add:
use crate::Versioned;

// Replace the VersionedValue struct with a type alias:
/// Versioned value with metadata
///
/// This is now a type alias for `Versioned<Value>`.
/// The original struct has been unified into the universal `Versioned<T>` type.
pub type VersionedValue = Versioned<Value>;

// REMOVE the old struct definition and impl block.
// The methods are now provided by Versioned<T>.
```

### Compatibility Note

The following VersionedValue usage patterns continue to work:

```rust
// Old code (still works via type alias)
let vv = VersionedValue::new(value, Version::TxnId(1), Timestamp::now());
assert_eq!(vv.value, value);
assert!(!vv.is_expired());

// New code (preferred)
let v: Versioned<Value> = Versioned::new(value, Version::TxnId(1), Timestamp::now());
```

### Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_versioned_new() {
        let versioned = Versioned::new(
            "hello".to_string(),
            Version::TxnId(42),
            Timestamp::from_micros(1000),
        );

        assert_eq!(versioned.value, "hello");
        assert_eq!(versioned.version, Version::TxnId(42));
        assert_eq!(versioned.timestamp.as_micros(), 1000);
        assert!(versioned.ttl.is_none());
    }

    #[test]
    fn test_versioned_with_ttl() {
        let ttl = Duration::from_secs(60);
        let versioned = Versioned::with_ttl(
            "data".to_string(),
            Version::TxnId(1),
            Timestamp::from_micros(1000),
            Some(ttl),
        );

        assert_eq!(versioned.ttl, Some(ttl));
        assert!(!versioned.is_expired());
    }

    #[test]
    fn test_versioned_map_preserves_ttl() {
        let ttl = Duration::from_secs(30);
        let versioned = Versioned::with_ttl(
            42i32,
            Version::TxnId(1),
            Timestamp::from_micros(1000),
            Some(ttl),
        );

        let mapped = versioned.map(|v| v.to_string());

        assert_eq!(mapped.value, "42");
        assert_eq!(mapped.version, Version::TxnId(1));
        assert_eq!(mapped.ttl, Some(ttl));
    }

    #[test]
    fn test_versioned_as_ref() {
        let versioned = Versioned::new(
            vec![1, 2, 3],
            Version::Sequence(5),
            Timestamp::from_micros(2000),
        );

        let versioned_ref = versioned.as_ref();

        assert_eq!(versioned_ref.value, &vec![1, 2, 3]);
        assert_eq!(versioned_ref.version, Version::Sequence(5));
    }

    #[test]
    fn test_versioned_accessors() {
        let versioned = Versioned::new(
            100,
            Version::Counter(3),
            Timestamp::from_micros(5000),
        );

        assert_eq!(*versioned.value(), 100);
        assert_eq!(*versioned.version(), Version::Counter(3));
        assert_eq!(versioned.timestamp().as_micros(), 5000);
    }

    #[test]
    fn test_versioned_is_expired() {
        let ttl = Duration::from_secs(1);
        let old_timestamp = Timestamp::from_micros(0); // Unix epoch

        let versioned = Versioned::with_ttl(
            "expired",
            Version::TxnId(1),
            old_timestamp,
            Some(ttl),
        );

        // Should be expired (timestamp is from 1970)
        assert!(versioned.is_expired());

        // No TTL = never expires
        let no_ttl = Versioned::new("forever", Version::TxnId(1), old_timestamp);
        assert!(!no_ttl.is_expired());
    }

    // === VersionedValue Type Alias Compatibility Tests ===

    #[test]
    fn test_versioned_value_type_alias() {
        // This test verifies the type alias works correctly
        use crate::Value;

        let value = Value::String("test".to_string());
        let vv: VersionedValue = Versioned::new(
            value.clone(),
            Version::TxnId(1),
            Timestamp::now(),
        );

        assert_eq!(vv.value, value);
    }
}
```

### Validation

```bash
~/.cargo/bin/cargo test -p in-mem-core -- versioned
~/.cargo/bin/cargo clippy -p in-mem-core -- -D warnings
```

### Complete Story

```bash
./scripts/complete-story.sh 470
```

---

## Story #471: Version Enum

**GitHub Issue**: [#471](https://github.com/anibjoshi/in-mem/issues/471)
**Estimated Time**: 1 hour
**Dependencies**: None
**Blocks**: Story #470

### Start Story

```bash
gh issue view 471
./scripts/start-story.sh 60 471 version-enum
```

### Implementation

Create `crates/core/src/contract/version.rs`:

```rust
//! Version identifier types
//!
//! Different primitives use different versioning schemes:
//! - Mutable primitives (KV, State, Json, Vector): TxnId
//! - Append-only primitives (Event, Trace): Sequence
//! - CAS operations: Counter

/// Version identifier for an entity
///
/// This type provides a unified version abstraction while
/// preserving the semantic meaning of each primitive's versioning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Version {
    /// Transaction ID - for mutable primitives (KV, Json, Vector, Run)
    TxnId(u64),
    /// Sequence number - for append-only primitives (Event, Trace)
    Sequence(u64),
    /// Counter - for CAS operations (State)
    Counter(u64),
}

impl Version {
    /// Get the numeric value for comparison
    ///
    /// Within the same primitive, versions are ordered.
    /// Comparison across primitives is NOT meaningful.
    pub fn as_u64(&self) -> u64 {
        match self {
            Version::TxnId(v) => *v,
            Version::Sequence(v) => *v,
            Version::Counter(v) => *v,
        }
    }

    /// Check if this is a transaction ID version
    pub fn is_txn_id(&self) -> bool {
        matches!(self, Version::TxnId(_))
    }

    /// Check if this is a sequence version
    pub fn is_sequence(&self) -> bool {
        matches!(self, Version::Sequence(_))
    }

    /// Check if this is a counter version
    pub fn is_counter(&self) -> bool {
        matches!(self, Version::Counter(_))
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        // Only compare versions of the same type
        match (self, other) {
            (Version::TxnId(a), Version::TxnId(b)) => Some(a.cmp(b)),
            (Version::Sequence(a), Version::Sequence(b)) => Some(a.cmp(b)),
            (Version::Counter(a), Version::Counter(b)) => Some(a.cmp(b)),
            _ => None, // Different version types are not comparable
        }
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Version::TxnId(v) => write!(f, "txn:{}", v),
            Version::Sequence(v) => write!(f, "seq:{}", v),
            Version::Counter(v) => write!(f, "cnt:{}", v),
        }
    }
}
```

### Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_as_u64() {
        assert_eq!(Version::TxnId(42).as_u64(), 42);
        assert_eq!(Version::Sequence(100).as_u64(), 100);
        assert_eq!(Version::Counter(5).as_u64(), 5);
    }

    #[test]
    fn test_version_type_checks() {
        let txn = Version::TxnId(1);
        let seq = Version::Sequence(1);
        let cnt = Version::Counter(1);

        assert!(txn.is_txn_id());
        assert!(!txn.is_sequence());
        assert!(!txn.is_counter());

        assert!(!seq.is_txn_id());
        assert!(seq.is_sequence());

        assert!(cnt.is_counter());
    }

    #[test]
    fn test_version_partial_ord_same_type() {
        assert!(Version::TxnId(1) < Version::TxnId(2));
        assert!(Version::Sequence(10) > Version::Sequence(5));
        assert!(Version::Counter(3) == Version::Counter(3));
    }

    #[test]
    fn test_version_partial_ord_different_types() {
        // Different types are not comparable
        assert_eq!(Version::TxnId(1).partial_cmp(&Version::Sequence(1)), None);
        assert_eq!(Version::Sequence(1).partial_cmp(&Version::Counter(1)), None);
    }

    #[test]
    fn test_version_display() {
        assert_eq!(format!("{}", Version::TxnId(42)), "txn:42");
        assert_eq!(format!("{}", Version::Sequence(100)), "seq:100");
        assert_eq!(format!("{}", Version::Counter(5)), "cnt:5");
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 471
```

---

## Story #472: Timestamp Type

**GitHub Issue**: [#472](https://github.com/anibjoshi/in-mem/issues/472)
**Estimated Time**: 1 hour
**Dependencies**: None
**Blocks**: Story #470

### Start Story

```bash
gh issue view 472
./scripts/start-story.sh 60 472 timestamp-type
```

### Implementation

Create `crates/core/src/contract/timestamp.rs`:

```rust
//! Microsecond-precision timestamp type

use std::time::{SystemTime, UNIX_EPOCH};

/// Microsecond-precision timestamp
///
/// Used to track when versions are created.
/// Stored as microseconds since Unix epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(u64);

impl Timestamp {
    /// Create a timestamp for the current moment
    pub fn now() -> Self {
        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards");
        Self(duration.as_micros() as u64)
    }

    /// Create a timestamp from microseconds since epoch
    pub fn from_micros(micros: u64) -> Self {
        Self(micros)
    }

    /// Get microseconds since Unix epoch
    pub fn as_micros(&self) -> u64 {
        self.0
    }

    /// Get milliseconds since Unix epoch
    pub fn as_millis(&self) -> u64 {
        self.0 / 1000
    }

    /// Get seconds since Unix epoch
    pub fn as_secs(&self) -> u64 {
        self.0 / 1_000_000
    }

    /// Zero timestamp (Unix epoch)
    pub const EPOCH: Timestamp = Timestamp(0);
}

impl Default for Timestamp {
    fn default() -> Self {
        Self::now()
    }
}

impl std::fmt::Display for Timestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}µs", self.0)
    }
}

impl From<u64> for Timestamp {
    fn from(micros: u64) -> Self {
        Self::from_micros(micros)
    }
}
```

### Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timestamp_now() {
        let ts = Timestamp::now();
        assert!(ts.as_micros() > 0);
    }

    #[test]
    fn test_timestamp_from_micros() {
        let ts = Timestamp::from_micros(1_000_000);
        assert_eq!(ts.as_micros(), 1_000_000);
        assert_eq!(ts.as_millis(), 1_000);
        assert_eq!(ts.as_secs(), 1);
    }

    #[test]
    fn test_timestamp_epoch() {
        assert_eq!(Timestamp::EPOCH.as_micros(), 0);
    }

    #[test]
    fn test_timestamp_ordering() {
        let t1 = Timestamp::from_micros(100);
        let t2 = Timestamp::from_micros(200);

        assert!(t1 < t2);
        assert!(t2 > t1);
    }

    #[test]
    fn test_timestamp_display() {
        let ts = Timestamp::from_micros(1234567);
        assert_eq!(format!("{}", ts), "1234567µs");
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 472
```

---

## Story #473: PrimitiveType Enum

**GitHub Issue**: [#473](https://github.com/anibjoshi/in-mem/issues/473)
**Estimated Time**: 1 hour
**Dependencies**: None
**Blocks**: Story #469

### Start Story

```bash
gh issue view 473
./scripts/start-story.sh 60 473 primitive-type
```

### Implementation

Create `crates/core/src/contract/primitive_type.rs` (or include in entity_ref.rs):

```rust
//! Primitive type enumeration

/// The seven Strata primitives
///
/// Used for introspection (Invariant 6) and type-safe dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrimitiveType {
    /// Key-Value store
    Kv,
    /// Append-only event log
    Event,
    /// Named state cells with CAS
    State,
    /// Structured trace recording
    Trace,
    /// Run lifecycle management
    Run,
    /// JSON document store
    Json,
    /// Vector similarity search
    Vector,
}

impl PrimitiveType {
    /// Human-readable name
    pub fn name(&self) -> &'static str {
        match self {
            PrimitiveType::Kv => "KVStore",
            PrimitiveType::Event => "EventLog",
            PrimitiveType::State => "StateCell",
            PrimitiveType::Trace => "TraceStore",
            PrimitiveType::Run => "RunIndex",
            PrimitiveType::Json => "JsonStore",
            PrimitiveType::Vector => "VectorStore",
        }
    }

    /// Short identifier for serialization
    pub fn id(&self) -> &'static str {
        match self {
            PrimitiveType::Kv => "kv",
            PrimitiveType::Event => "event",
            PrimitiveType::State => "state",
            PrimitiveType::Trace => "trace",
            PrimitiveType::Run => "run",
            PrimitiveType::Json => "json",
            PrimitiveType::Vector => "vector",
        }
    }

    /// All primitive types
    pub const ALL: [PrimitiveType; 7] = [
        PrimitiveType::Kv,
        PrimitiveType::Event,
        PrimitiveType::State,
        PrimitiveType::Trace,
        PrimitiveType::Run,
        PrimitiveType::Json,
        PrimitiveType::Vector,
    ];
}

impl std::fmt::Display for PrimitiveType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}
```

### Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_primitive_type_names() {
        assert_eq!(PrimitiveType::Kv.name(), "KVStore");
        assert_eq!(PrimitiveType::Event.name(), "EventLog");
        assert_eq!(PrimitiveType::Vector.name(), "VectorStore");
    }

    #[test]
    fn test_primitive_type_ids() {
        assert_eq!(PrimitiveType::Kv.id(), "kv");
        assert_eq!(PrimitiveType::Event.id(), "event");
        assert_eq!(PrimitiveType::Vector.id(), "vector");
    }

    #[test]
    fn test_primitive_type_all() {
        assert_eq!(PrimitiveType::ALL.len(), 7);

        // Verify all unique
        let mut seen = std::collections::HashSet::new();
        for pt in PrimitiveType::ALL {
            assert!(seen.insert(pt));
        }
    }

    #[test]
    fn test_primitive_type_display() {
        assert_eq!(format!("{}", PrimitiveType::Json), "JsonStore");
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 473
```

---

## Story #474: RunName + RunId Dual Identity Model

**GitHub Issue**: [#474](https://github.com/anibjoshi/in-mem/issues/474)
**Estimated Time**: 3 hours
**Dependencies**: None
**Blocks**: Story #469

### The Semantic Question

> **Who owns identity - the system or the user?**
>
> In a reasoning substrate, runs are not just storage buckets. They are:
> - **Reasoning contexts** - conceptual containers for cognition
> - **Memory boundaries** - isolation units for agent state
> - **Replayable timelines** - debuggable execution histories
> - **Semantic namespaces** - user-meaningful identifiers
>
> This requires **user-owned semantic identity** (RunName), not just machine-generated tokens (RunId).

### Start Story

```bash
gh issue view 474
./scripts/start-story.sh 60 474 run-identity
```

### Implementation

#### Part 1: RunName (NEW) - User-Facing Semantic Identity

Create `crates/core/src/contract/run_name.rs`:

```rust
//! Run name type - user-facing semantic identity

use serde::{Deserialize, Serialize};

/// User-facing name for a run (semantic identity)
///
/// RunName is what users think about, reference, and script against.
/// It is:
/// - Human-readable ("my-experiment-v2")
/// - Stable (same name = same conceptual run)
/// - Scriptable (can be used in CLI, prompts, logs)
///
/// This type expresses the semantic aspect of Invariant 5.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunName(String);

impl RunName {
    /// Create a new run name
    ///
    /// Names should be meaningful to humans:
    /// - "chat-session-alice-2026-01"
    /// - "experiment-transformer-v3"
    /// - "debug-replay-issue-42"
    ///
    /// # Panics
    /// Panics if name is empty.
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        assert!(!name.is_empty(), "RunName cannot be empty");
        Self(name)
    }

    /// Get the name as a string slice
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Check if this is a valid run name
    ///
    /// Run names must be non-empty and contain only:
    /// alphanumeric, dash, underscore, or dot characters.
    pub fn is_valid(&self) -> bool {
        !self.0.is_empty() && self.0.chars().all(|c| {
            c.is_alphanumeric() || c == '-' || c == '_' || c == '.'
        })
    }
}

impl std::fmt::Display for RunName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for RunName {
    fn from(s: &str) -> Self {
        RunName::new(s)
    }
}

impl From<String> for RunName {
    fn from(s: String) -> Self {
        RunName::new(s)
    }
}

impl AsRef<str> for RunName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
```

#### Part 2: Keep Existing RunId - Internal Storage Identity

The existing `RunId(Uuid)` in `crates/core/src/types.rs` is **unchanged**.
It remains the internal storage identity used by:
- Storage layer (ShardedStore keys)
- WAL entries
- EntityRef (internal addressing)

```rust
// crates/core/src/types.rs - KEEP EXISTING, add documentation

/// Internal identifier for a run (storage identity)
///
/// RunId is what the storage layer uses for indexing and references.
/// It is:
/// - Globally unique (UUID v4)
/// - Compact (16 bytes)
/// - Collision-free
///
/// NOTE: Users should not see or use RunIds directly in public APIs.
/// Public APIs accept RunName, and the system manages the mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunId(Uuid);
// ... existing implementation unchanged
```

#### Part 3: Name-to-ID Mapping

The mapping will be added to Database in a later story (Epic 62).
For now, document the interface:

```rust
// This will be implemented in Epic 62 (Transaction Unification)
// For now, we define the types and their relationship

/// The Database will manage bidirectional mapping:
/// - RunName → RunId (lookup)
/// - RunId → RunName (reverse lookup)
///
/// Public API:
/// - db.run("my-name") - get or create run by name
/// - db.create_run("my-name") - explicit creation
/// - db.resolve_run(&name) - lookup only
```

### Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // === RunName Tests ===

    #[test]
    fn test_run_name_new() {
        let name = RunName::new("my-experiment");
        assert_eq!(name.as_str(), "my-experiment");
    }

    #[test]
    #[should_panic(expected = "RunName cannot be empty")]
    fn test_run_name_empty_panics() {
        RunName::new("");
    }

    #[test]
    fn test_run_name_display() {
        let name = RunName::new("test-123");
        assert_eq!(format!("{}", name), "test-123");
    }

    #[test]
    fn test_run_name_from_str() {
        let name: RunName = "from-str".into();
        assert_eq!(name.as_str(), "from-str");
    }

    #[test]
    fn test_run_name_from_string() {
        let name: RunName = String::from("from-string").into();
        assert_eq!(name.as_str(), "from-string");
    }

    #[test]
    fn test_run_name_equality() {
        let n1 = RunName::new("same");
        let n2 = RunName::new("same");
        let n3 = RunName::new("different");

        assert_eq!(n1, n2);
        assert_ne!(n1, n3);
    }

    #[test]
    fn test_run_name_hash() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        set.insert(RunName::new("run1"));
        set.insert(RunName::new("run2"));

        assert!(set.contains(&RunName::new("run1")));
        assert!(!set.contains(&RunName::new("run3")));
    }

    #[test]
    fn test_run_name_validation() {
        assert!(RunName::new("valid-name_123.test").is_valid());
        assert!(RunName::new("simple").is_valid());

        // These would be invalid if we enforced at construction
        // For now, is_valid() is advisory
        let with_space = RunName::new("has space");
        assert!(!with_space.is_valid());
    }

    #[test]
    fn test_run_name_serialization() {
        let name = RunName::new("serialize-me");
        let json = serde_json::to_string(&name).unwrap();
        let restored: RunName = serde_json::from_str(&json).unwrap();
        assert_eq!(name, restored);
    }
}
```

### Why This Matters

**Without semantic identity:**
```
[2026-01-19 10:23:45] Processing run 550e8400-e29b-41d4-a716-446655440000
[2026-01-19 10:23:46] Error in run 550e8400-e29b-41d4-a716-446655440000: key not found
```

**With semantic identity:**
```
[2026-01-19 10:23:45] Processing run 'experiment-transformer-v3'
[2026-01-19 10:23:46] Error in run 'experiment-transformer-v3': key not found
```

### Acceptance Criteria

- [ ] `RunName` newtype over String in `crates/core/src/run_name.rs`
- [ ] `RunName::new()` constructor with empty check
- [ ] `RunName::as_str()` accessor
- [ ] `RunName::is_valid()` validation method
- [ ] `RunName` implements: Debug, Clone, PartialEq, Eq, Hash, Display
- [ ] `RunName` implements: From<&str>, From<String>, AsRef<str>
- [ ] `RunName` implements: Serialize, Deserialize
- [ ] Existing `RunId(Uuid)` unchanged (documentation updated)
- [ ] Export `RunName` from `crates/core/src/lib.rs`
- [ ] All tests passing

### Complete Story

```bash
./scripts/complete-story.sh 474
```

---

## Epic 60 Completion Checklist

### 1. Final Validation

```bash
~/.cargo/bin/cargo test -p in-mem-core
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### 2. Verify Deliverables

- [ ] EntityRef enum with all 7 variants
- [ ] EntityRef::run_id() and primitive_type() methods
- [ ] Versioned<T> with map() and accessors
- [ ] Version enum with TxnId, Sequence, Counter
- [ ] Timestamp type with now(), from_micros()
- [ ] PrimitiveType enum with ALL constant
- [ ] RunName newtype for semantic identity
- [ ] RunId kept as internal UUID (unchanged)

### 3. Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-60-core-types -m "Epic 60: Core Types complete

Delivered:
- EntityRef universal addressing
- Versioned<T> wrapper
- Version enum
- Timestamp type
- PrimitiveType enum
- RunName + RunId dual identity model

Stories: #469, #470, #471, #472, #473, #474
"
git push origin develop
gh issue close 464 --comment "Epic 60: Core Types - COMPLETE"
```

---

## Summary

Epic 60 establishes the foundation types that express the seven invariants. These types are the building blocks for all subsequent M9 work:

- **EntityRef** enables universal addressing (Invariant 1)
- **Versioned<T>** ensures version info is never lost (Invariant 2)
- **Version** captures different versioning schemes (Invariant 2)
- **Timestamp** tracks temporal information (Invariant 2)
- **PrimitiveType** enables introspection (Invariant 6)
- **RunName** provides semantic user-facing identity (Invariant 5)
- **RunId** provides internal storage identity (Invariant 5)
