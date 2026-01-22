# M9 Architecture Specification: API Stabilization & Universal Protocol

**Version**: 1.0
**Status**: Implementation Ready
**Last Updated**: 2026-01-19

---

## Executive Summary

This document specifies the architecture for **Milestone 9 (M9): API Stabilization & Universal Protocol** of the Strata database. M9 freezes the external API before server implementation (M10) and client development (M12), ensuring all seven primitives conform to a unified interaction model.

**THIS DOCUMENT IS AUTHORITATIVE.** All M9 implementation must conform to this specification.

**Related Documents**:
- [PRIMITIVE_CONTRACT.md](./PRIMITIVE_CONTRACT.md) - The seven invariants (constitutional)
- [CORE_API_SHAPE.md](./CORE_API_SHAPE.md) - API patterns and types
- [PRODUCT_SURFACES.md](./PRODUCT_SURFACES.md) - Features built on core
- [M8 Architecture](./M8_ARCHITECTURE.md) - Vector primitive
- [MILESTONES.md](../milestones/MILESTONES.md) - Project milestone tracking

**M9 Philosophy**:
> M9 is not about features. M9 is about **contracts**.
>
> Before building the server (M10), before adding Python clients (M12), the interface must be stable. M9 separates invariants from conveniences and substrate from product. "What is the universal way a user interacts with anything in Strata?" This milestone answers that question.

**M9 Goals** (Contract Guarantees):
- Seven invariants documented, enforced, and tested
- All seven primitives conform to all seven invariants
- Universal types (`EntityRef`, `Versioned<T>`, `Transaction`) implemented
- API consistency across all primitives audited and verified
- Migration path from current API documented

**M9 Non-Goals** (Deferred):
- Wire protocol (M10)
- Server implementation (M10)
- Performance optimization (M11)
- Python SDK (M12)
- New features or primitives

**Critical Constraint**:
> M9 is a stabilization milestone, not an expansion milestone. If a change adds new functionality, it is out of scope. If a change improves consistency without changing semantics, it is in scope.

**Built on M1-M8**:
- M1 provides: Storage (UnifiedStore), WAL, Recovery
- M2 provides: OCC transactions, Snapshot isolation, Conflict detection
- M3 provides: Five primitives (KVStore, EventLog, StateCell, TraceStore, RunIndex)
- M4 provides: Durability modes, ShardedStore
- M5 provides: JsonStore primitive
- M6 provides: Retrieval surface with search
- M7 provides: Snapshots, crash recovery, deterministic replay
- M8 provides: Vector primitive
- M9 adds: Universal Protocol, API stabilization, conformance testing

---

## Table of Contents

1. [Scope Boundaries](#1-scope-boundaries)
2. [THE FOUR ARCHITECTURAL RULES](#2-the-four-architectural-rules-non-negotiable)
3. [The Seven Invariants](#3-the-seven-invariants)
4. [Core Types](#4-core-types)
5. [Transaction Pattern](#5-transaction-pattern)
6. [Primitive Conformance](#6-primitive-conformance)
7. [API Consistency Audit](#7-api-consistency-audit)
8. [Error Handling](#8-error-handling)
9. [Migration Strategy](#9-migration-strategy)
10. [Testing Strategy](#10-testing-strategy)
11. [Known Limitations](#11-known-limitations)
12. [Future Extension Points](#12-future-extension-points)
13. [Success Criteria Checklist](#13-success-criteria-checklist)

---

## 1. Scope Boundaries

### 1.1 What M9 IS

M9 is an **API stabilization milestone**. It defines:

| Aspect | M9 Commits To |
|--------|---------------|
| **Invariants** | Seven invariants documented and enforced |
| **Universal types** | `EntityRef`, `Versioned<T>`, `Version`, `RunId` |
| **Transaction pattern** | Unified `Transaction` trait for all primitives |
| **Handle pattern** | `RunHandle` for scoped primitive access |
| **Error types** | Consistent `StrataError` across all primitives |
| **Conformance tests** | Tests verifying each primitive against each invariant |

### 1.2 What M9 is NOT

M9 is **not** a feature milestone. These are explicitly deferred:

| Deferred Item | Why Deferred | Target Milestone |
|---------------|--------------|------------------|
| Wire protocol | Transport, not semantics | M10 |
| Server binary | Deployment mode | M10 |
| Performance optimization | Separate concern | M11 |
| Python SDK | Language binding | M12 |
| New primitives | Contract first | Post-MVP |
| Advanced introspection (diff, history) | Feature, not invariant | Post-MVP |

### 1.3 The Risk We Are Avoiding

Without API stabilization:
- Server (M10) builds on shifting foundations
- Python SDK (M12) targets a moving API
- Each primitive has different patterns, confusing users
- Version information is inconsistent or missing
- Error handling is ad-hoc
- Migration becomes impossible

**M9 freezes the contract.** After M9, API changes require explicit migration paths.

### 1.4 Evolution Warnings

**These are explicit warnings about M9 design decisions:**

#### A. Invariants Are Constitutional

The seven invariants are not negotiable. If a feature requires violating an invariant, the feature is wrong. If you think an invariant needs changing, that requires an RFC process, not a code change.

#### B. API Shape May Still Evolve (With Migration)

M9 stabilizes the **patterns**, not every method signature. After M9:
- New methods can be added (additive change)
- Method signatures can change with migration paths
- The patterns (`Versioned<T>`, `Transaction`, etc.) are stable

#### C. EntityRef Representation Is Flexible

M9 commits to the **capability** (everything is addressable), not the exact representation. The `EntityRef` enum may evolve, but the ability to reference any entity must not regress.

---

## 2. THE FOUR ARCHITECTURAL RULES (NON-NEGOTIABLE)

**These rules MUST be followed in ALL M9 implementation. Violating any of these is a blocking issue.**

### Rule 1: Every Read Returns Versioned<T>

> **No read operation may return raw values without version information.**

```rust
// CORRECT: Read returns versioned
pub fn kv_get(&self, key: &str) -> Result<Option<Versioned<Value>>> {
    // Returns version with value
}

// WRONG: Read returns raw value
pub fn kv_get(&self, key: &str) -> Result<Option<Value>> {
    // Where's the version? NEVER DO THIS
}
```

**Why**: Invariant 2 requires "everything is versioned." If reads don't return versions, users cannot know what version they're looking at.

### Rule 2: Every Write Returns Version

> **Every mutation returns the version it created.**

```rust
// CORRECT: Write returns version
pub fn kv_put(&mut self, key: &str, value: Value) -> Result<Version> {
    // Returns the version created
}

// WRONG: Write returns nothing
pub fn kv_put(&mut self, key: &str, value: Value) -> Result<()> {
    // What version was created? NEVER DO THIS
}
```

**Why**: Invariant 2 requires "every mutation produces a version." If writes don't return versions, users cannot track what happened.

### Rule 3: Transaction Trait Covers All Primitives

> **Every primitive operation is accessible through the Transaction trait.**

```rust
// CORRECT: Transaction provides all primitives
pub trait TransactionOps {
    // KV
    fn kv_get(&self, key: &str) -> Result<Option<Versioned<Value>>>;
    fn kv_put(&mut self, key: &str, value: Value) -> Result<Version>;

    // Event
    fn event_append(&mut self, event_type: &str, payload: Value) -> Result<Version>;
    fn event_read(&self, sequence: u64) -> Result<Option<Versioned<Event>>>;

    // ... all other primitives
}

// WRONG: Some primitives outside transaction
pub trait TransactionOps {
    fn kv_get(&self, key: &str) -> Result<Option<Versioned<Value>>>;
    // No event operations? NEVER DO THIS
}
```

**Why**: Invariant 3 requires "every primitive can participate in a transaction." If a primitive isn't in the Transaction trait, cross-primitive atomicity breaks.

### Rule 4: Run Scope Is Always Explicit

> **The run is always known. No ambient run context.**

```rust
// CORRECT: Run in handle or parameter
pub struct RunHandle { run_id: RunId, db: Arc<Database> }
impl RunHandle {
    pub fn kv(&self) -> KvHandle { /* run_id from self */ }
}

// CORRECT: Run as parameter
pub fn kv_get(&self, run_id: &RunId, key: &str) -> Result<...>;

// WRONG: Ambient run context
thread_local! {
    static CURRENT_RUN: RefCell<Option<RunId>> = RefCell::new(None);
}
pub fn kv_get(&self, key: &str) -> Result<...> {
    let run_id = CURRENT_RUN.with(...);  // NEVER DO THIS
}
```

**Why**: Invariant 5 requires "everything exists within a run." If run scope is implicit, it's easy to accidentally cross run boundaries.

---

## 3. The Seven Invariants

M9 enforces and tests these invariants. See [PRIMITIVE_CONTRACT.md](./PRIMITIVE_CONTRACT.md) for full details.

### Summary

| # | Invariant | What It Means |
|---|-----------|---------------|
| 1 | Everything is Addressable | Every entity has a stable identity |
| 2 | Everything is Versioned | Every mutation produces a version |
| 3 | Everything is Transactional | All primitives participate in transactions |
| 4 | Everything Has a Lifecycle | Create, exist, evolve, destroy pattern |
| 5 | Everything Exists Within a Run | Run is the unit of isolation |
| 6 | Everything is Introspectable | Existence checks and state reads |
| 7 | Reads and Writes are Consistent | Reads never modify; writes produce versions |

### Conformance Requirements

Each primitive MUST demonstrate conformance to each invariant:

```rust
#[cfg(test)]
mod invariant_conformance {
    // Invariant 1: Addressable
    #[test]
    fn kv_has_stable_identity() {
        // Create entity, get its reference, use reference to retrieve
    }

    // Invariant 2: Versioned
    #[test]
    fn kv_reads_return_versions() {
        // Read returns Versioned<T>, not raw T
    }

    #[test]
    fn kv_writes_return_versions() {
        // Write returns Version
    }

    // Invariant 3: Transactional
    #[test]
    fn kv_participates_in_cross_primitive_transaction() {
        // KV + Event + StateCell in same transaction
    }

    // ... tests for each invariant for each primitive
}
```

---

## 4. Core Types

### 4.0 File Organization: The Contract Module

> **The Semantic Question**: Where does the meaning of the system live?
>
> M9 introduces types that are not implementation details - they are **contract types**.
> These types encode the semantic invariants of the system. They define what it means
> to interact with the database.
>
> If contract types are scattered across random files or buried in `types.rs`,
> you lose:
> - Semantic clarity
> - Discoverability
> - API stability boundaries
> - A single place to understand the system model
> - A clean mental map for contributors

**The Solution: A Dedicated Contract Module**

All M9 contract types live in a single cohesive module: `crates/core/src/contract.rs`

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
//! - TransactionOps (Invariant 3: Transactional)
//! - Lifecycle states (Invariant 4: Lifecycle)
//! - RunName, RunId (Invariant 5: Run-scoped)
//! - Introspection types (Invariant 6: Introspectable)
//!
//! ## What Does NOT Belong Here
//!
//! Implementation details that are internal to subsystems:
//! - ShardedStore internals
//! - WAL entry formats
//! - Index data structures
//! - Scorer implementations

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
// RunId stays in types.rs (internal storage identity)

// Type aliases for backwards compatibility
pub use crate::search_types::DocRef;       // = EntityRef
pub use crate::search_types::PrimitiveKind; // = PrimitiveType
pub use crate::value::VersionedValue;       // = Versioned<Value>
```

**Module Structure**:

```
crates/core/src/
├── contract.rs          # NEW: All M9 contract types
│   ├── entity_ref.rs    # EntityRef, PrimitiveType
│   ├── versioned.rs     # Versioned<T>
│   ├── version.rs       # Version enum
│   ├── timestamp.rs     # Timestamp type
│   └── run_name.rs      # RunName type
├── types.rs             # Internal types (RunId, Key, Namespace, etc.)
├── value.rs             # Value enum (+ VersionedValue alias)
├── search_types.rs      # Search types (+ DocRef alias)
├── error.rs             # Error types
└── lib.rs               # Re-exports contract types prominently
```

**Updated lib.rs Exports**:

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

// Backwards compatibility aliases
pub use contract::DocRef;        // = EntityRef
pub use contract::PrimitiveKind; // = PrimitiveType
pub use contract::VersionedValue; // = Versioned<Value>
```

**Why This Matters**:

| Without Contract Module | With Contract Module |
|------------------------|---------------------|
| Types scattered across files | Single source of truth |
| No clear API boundary | Clear stability boundary |
| Hard to document | Easy to document |
| Hard to generate SDKs | Clean SDK generation |
| Semantic drift over time | Semantic coherence |

### 4.1 EntityRef: Universal Addressing (Invariant 1)

```rust
/// Reference to any entity in Strata
///
/// This type expresses Invariant 1: Everything is Addressable.
/// Every entity has a reference that can identify it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EntityRef {
    /// KV entry: run + key
    Kv { run_id: RunId, key: String },

    /// Event: run + sequence number
    Event { run_id: RunId, sequence: u64 },

    /// State cell: run + cell name
    State { run_id: RunId, name: String },

    /// Trace: run + trace ID
    Trace { run_id: RunId, trace_id: TraceId },

    /// Run metadata
    Run { run_id: RunId },

    /// JSON document: run + document ID
    Json { run_id: RunId, doc_id: JsonDocId },

    /// Vector: run + collection + vector ID
    Vector { run_id: RunId, collection: CollectionId, vector_id: VectorId },
}

impl EntityRef {
    /// Returns the run this entity belongs to
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

    /// Returns the primitive type
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveType {
    Kv,
    Event,
    State,
    Trace,
    Run,
    Json,
    Vector,
}
```

#### 4.1.1 Unification with Existing DocRef and PrimitiveKind

> **The Semantic Question**: Can there be two parallel identity systems in a reasoning substrate?
>
> The current codebase has `DocRef` and `PrimitiveKind` in `crates/core/src/search_types.rs`:
> ```rust
> pub enum DocRef {
>     Kv { key: Key },
>     Json { key: Key, doc_id: JsonDocId },
>     Event { log_key: Key, seq: u64 },
>     State { key: Key },
>     Trace { key: Key, span_id: u64 },
>     Run { run_id: RunId },
>     Vector { collection: String, key: String, run_id: RunId },
> }
>
> pub enum PrimitiveKind { Kv, Json, Event, State, Trace, Run, Vector }
> ```
>
> M9 introduces `EntityRef` and `PrimitiveType` as universal contracts. Having both creates:
> - Two ways to refer to the same thing
> - Two mental models of identity
> - Conversion glue everywhere
> - Inconsistent APIs
> - Long-term semantic drift
>
> **Identity is foundational in a reasoning substrate. You cannot afford multiple notions of "what an entity is."**

**The Solution: Canonicalize EntityRef**

`EntityRef` becomes the **canonical** identity abstraction. `DocRef` becomes a type alias:

```rust
// In crates/core/src/search_types.rs (after M9)

/// Type alias for backwards compatibility with search layer
///
/// New code should use `EntityRef` directly.
/// This alias exists only for migration compatibility.
pub type DocRef = EntityRef;

/// Type alias for backwards compatibility
pub type PrimitiveKind = PrimitiveType;
```

**Key Differences and Reconciliation**:

| DocRef Field | EntityRef Field | Resolution |
|-------------|----------------|------------|
| `Kv { key: Key }` | `Kv { run_id: RunId, key: String }` | Extract run_id from Key.namespace, use user_key |
| `Json { key, doc_id }` | `Json { run_id, doc_id }` | Extract run_id from Key.namespace |
| `Event { log_key, seq }` | `Event { run_id, sequence }` | Extract run_id, rename seq→sequence |
| `State { key }` | `State { run_id, name }` | Extract run_id, use user_key as name |
| `Trace { key, span_id }` | `Trace { run_id, trace_id }` | Extract run_id, map span_id→trace_id |
| `Run { run_id }` | `Run { run_id }` | Same |
| `Vector { collection, key, run_id }` | `Vector { run_id, collection, vector_id }` | Reorder, rename key→vector_id |

**Migration Strategy**:

1. **Phase 1: Add EntityRef** - Implement `EntityRef` with canonical structure
2. **Phase 2: Add Conversions** - Implement `From<DocRef> for EntityRef` and reverse
3. **Phase 3: Add Type Aliases** - Make `DocRef = EntityRef`, `PrimitiveKind = PrimitiveType`
4. **Phase 4: Update Internal Code** - Migrate search layer to use `EntityRef`
5. **Phase 5: Deprecate DocRef** - Mark `DocRef` usage as deprecated

**Why This Matters for a Reasoning Substrate**:

Without unified identity:
- Search returns `DocRef`, core returns different types
- Error messages reference entities inconsistently
- Wire protocol needs multiple identity formats
- CLI/SDKs have confusing APIs
- Provenance tracking is fragmented

With unified identity:
- One way to reference any entity, everywhere
- Consistent error messages across all operations
- Single wire format for entity references
- Clean APIs for CLI, SDKs, and tools
- Unified provenance and history tracking

### 4.2 Versioned<T>: Universal Read Result (Invariant 2)

```rust
/// Wrapper for any value read from the database
///
/// This type expresses Invariant 2: Everything is Versioned.
/// Every read returns version information.
#[derive(Debug, Clone)]
pub struct Versioned<T> {
    /// The actual value
    pub value: T,

    /// Version identifier
    pub version: Version,

    /// When this version was created
    pub timestamp: Timestamp,

    /// Optional time-to-live (for primitives that support TTL)
    pub ttl: Option<std::time::Duration>,
}

impl<T> Versioned<T> {
    pub fn new(value: T, version: Version, timestamp: Timestamp) -> Self {
        Self { value, version, timestamp, ttl: None }
    }

    pub fn with_ttl(value: T, version: Version, timestamp: Timestamp, ttl: Option<std::time::Duration>) -> Self {
        Self { value, version, timestamp, ttl }
    }

    /// Map the inner value while preserving version info
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Versioned<U> {
        Versioned {
            value: f(self.value),
            version: self.version,
            timestamp: self.timestamp,
            ttl: self.ttl,
        }
    }
}
```

#### 4.2.1 Unification with Existing VersionedValue

> **The Semantic Question**: Can there be two competing notions of "versioned"?
>
> The current codebase has `VersionedValue` in `crates/core/src/value.rs`:
> ```rust
> pub struct VersionedValue {
>     pub value: Value,
>     pub version: u64,
>     pub timestamp: Timestamp,
>     pub ttl: Option<Duration>,
> }
> ```
>
> M9 introduces `Versioned<T>` as a universal contract. Having both creates semantic duplication:
> two types that mean the same thing but with different names and slightly different shapes.

**The Solution: Unify Around Versioned<T>**

`Versioned<T>` becomes the **canonical** versioned abstraction. `VersionedValue` becomes a type alias:

```rust
// In crates/core/src/value.rs (after M9)

/// Type alias for backwards compatibility
///
/// New code should use `Versioned<Value>` directly.
/// This alias exists only for migration compatibility.
pub type VersionedValue = Versioned<Value>;
```

**Migration Strategy**:

1. **Phase 1: Add Versioned<T>** - Implement `Versioned<T>` with TTL support
2. **Phase 2: Add Type Alias** - Make `VersionedValue = Versioned<Value>`
3. **Phase 3: Update Internal Code** - Migrate internal usage to `Versioned<T>`
4. **Phase 4: Deprecate Direct Usage** - Warn on direct `VersionedValue` construction

**Field Reconciliation**:

| VersionedValue Field | Versioned<T> Field | Resolution |
|---------------------|-------------------|------------|
| `value: Value` | `value: T` | `T = Value` for KV |
| `version: u64` | `version: Version` | Wrap in `Version::TxnId(u64)` |
| `timestamp: Timestamp` | `timestamp: Timestamp` | Same (use M9 Timestamp type) |
| `ttl: Option<Duration>` | `ttl: Option<Duration>` | Add to Versioned<T> |

**Why This Matters**:
- Single source of truth for "versioned data"
- API consistency: all reads return `Versioned<T>`
- No semantic duplication or confusion
- Existing code continues to work via type alias
```

### 4.3 Version: Universal Version Type

```rust
/// Version identifier
///
/// Versions are comparable within the same entity.
/// Versions may not be comparable across entities or primitives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Version {
    /// Transaction-based version (KV, Trace, Run, Vector, Json)
    TxnId(u64),

    /// Sequence-based version (EventLog)
    Sequence(u64),

    /// Counter-based version (StateCell)
    Counter(u64),
}

impl Version {
    /// Returns the numeric value for comparison
    pub fn as_u64(&self) -> u64 {
        match self {
            Version::TxnId(v) => *v,
            Version::Sequence(v) => *v,
            Version::Counter(v) => *v,
        }
    }
}
```

### 4.4 Run Identity: Dual Model (Invariant 5)

> **The Semantic Question**: Who owns identity - the system or the user?
>
> In a reasoning substrate, runs are not just storage buckets. They are:
> - **Reasoning contexts** - conceptual containers for cognition
> - **Memory boundaries** - isolation units for agent state
> - **Replayable timelines** - debuggable execution histories
> - **Semantic namespaces** - user-meaningful identifiers
>
> This requires **user-owned semantic identity**, not just machine-generated tokens.

#### The Dual Identity Model

```
┌─────────────────────────────────────────────────────────────┐
│                     RUN IDENTITY MODEL                      │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   User-Facing Layer (Semantic)                              │
│   ┌─────────────────────────────────────────────────────┐   │
│   │  RunName(String)                                    │   │
│   │  - "experiment-2026-01-19"                          │   │
│   │  - "chat-with-alice"                                │   │
│   │  - "reasoning-session-v3"                           │   │
│   │  Human-readable, scriptable, stable                 │   │
│   └─────────────────────────────────────────────────────┘   │
│                          │                                  │
│                          ▼ (mapping table)                  │
│                                                             │
│   Storage Layer (Mechanical)                                │
│   ┌─────────────────────────────────────────────────────┐   │
│   │  RunId(Uuid)                                        │   │
│   │  - 550e8400-e29b-41d4-a716-446655440000             │   │
│   │  Unique, compact, collision-free                    │   │
│   └─────────────────────────────────────────────────────┘   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

#### RunName: User-Facing Semantic Identity

```rust
/// User-facing name for a run (semantic identity)
///
/// RunName is what users think about, reference, and script against.
/// It is:
/// - Human-readable ("my-experiment-v2")
/// - Stable (same name = same conceptual run)
/// - Scriptable (can be used in CLI, prompts, logs)
///
/// This type expresses the semantic aspect of Invariant 5.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RunName(String);

impl RunName {
    /// Create a new run name
    ///
    /// Names should be meaningful to humans:
    /// - "chat-session-alice-2026-01"
    /// - "experiment-transformer-v3"
    /// - "debug-replay-issue-42"
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        debug_assert!(!name.is_empty(), "RunName cannot be empty");
        debug_assert!(
            name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '.'),
            "RunName must contain only alphanumeric, dash, underscore, or dot"
        );
        Self(name)
    }

    /// Get the name as a string slice
    pub fn as_str(&self) -> &str {
        &self.0
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
```

#### RunId: Internal Storage Identity

```rust
/// Internal identifier for a run (storage identity)
///
/// RunId is what the storage layer uses for indexing and references.
/// It is:
/// - Globally unique (UUID v4)
/// - Compact (16 bytes)
/// - Collision-free
///
/// Users should not need to see or use RunIds directly. All public
/// APIs accept RunName, and the system manages the mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RunId(Uuid);

impl RunId {
    /// Create a new random RunId
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create from bytes (for deserialization)
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(Uuid::from_bytes(bytes))
    }

    /// Get bytes (for serialization)
    pub fn as_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }
}
```

#### Name-to-ID Mapping

```rust
/// The Database manages the RunName → RunId mapping
impl Database {
    /// Create a new run with a user-provided name
    ///
    /// Returns error if name already exists.
    pub fn create_run(&self, name: RunName) -> Result<RunId> {
        // Check if name already exists
        if self.run_name_to_id.contains_key(&name) {
            return Err(Error::RunNameExists { name });
        }

        // Generate new internal ID
        let run_id = RunId::new();

        // Store bidirectional mapping
        self.run_name_to_id.insert(name.clone(), run_id);
        self.run_id_to_name.insert(run_id, name);

        Ok(run_id)
    }

    /// Get or create a run by name
    ///
    /// If name exists, returns existing RunId.
    /// If not, creates new run with that name.
    pub fn get_or_create_run(&self, name: RunName) -> RunId {
        if let Some(run_id) = self.run_name_to_id.get(&name) {
            return *run_id;
        }
        self.create_run(name).unwrap()
    }

    /// Resolve a RunName to its RunId
    pub fn resolve_run(&self, name: &RunName) -> Option<RunId> {
        self.run_name_to_id.get(name).copied()
    }

    /// Get the name for a RunId
    pub fn run_name(&self, run_id: RunId) -> Option<&RunName> {
        self.run_id_to_name.get(&run_id)
    }
}
```

#### Public API Uses RunName

```rust
// CORRECT: Public API uses RunName
impl Database {
    pub fn run(&self, name: impl Into<RunName>) -> RunHandle {
        let name = name.into();
        let run_id = self.get_or_create_run(name.clone());
        RunHandle { name, run_id, db: self.clone() }
    }
}

// User code is semantic and readable
let run = db.run("my-experiment-v2");
run.kv_put("config", json!({"learning_rate": 0.01})).await?;

// NOT this (opaque UUIDs)
let run = db.run("550e8400-e29b-41d4-a716-446655440000"); // Bad UX
```

#### Migration from Current RunId

The current codebase uses `RunId(Uuid)` directly in APIs. Migration path:

1. **Phase 1**: Add `RunName` type, keep existing `RunId(Uuid)` internal
2. **Phase 2**: Add mapping table to Database
3. **Phase 3**: Update public APIs to accept `RunName`
4. **Phase 4**: Deprecate direct `RunId` usage in public APIs

Internal storage and WAL continue using `RunId(Uuid)` - no changes needed there.

#### Why This Matters for a Reasoning Substrate

Without semantic identity:
- Logs are full of opaque UUIDs
- CLI commands require copy-pasting tokens
- Scripts can't reference runs meaningfully
- Debugging is cognitively hostile
- The system feels like infrastructure, not a reasoning tool

With semantic identity:
- `db.run("debug-issue-42")` is self-documenting
- Logs say "Run 'experiment-v3'" not "Run 550e8400..."
- Scripts use meaningful names
- Humans can reason about runs conceptually
- The substrate aligns with cognition

### 4.5 Timestamp

```rust
/// Timestamp for version creation
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Timestamp(pub u64);

impl Timestamp {
    pub fn now() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let duration = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
        Self(duration.as_micros() as u64)
    }
}
```

---

## 5. Transaction Pattern

### 5.1 Shape (Invariant 3)

```rust
impl Database {
    /// Execute operations atomically within a run
    pub fn transaction<F, T>(&self, run_id: &RunId, f: F) -> Result<T>
    where
        F: FnOnce(&mut Transaction) -> Result<T>;
}
```

### 5.2 Transaction Trait

```rust
/// Operations available within a transaction
///
/// This trait expresses Invariant 3: Everything is Transactional.
/// Every primitive's operations are accessible through this trait.
pub trait TransactionOps {
    // === KV Operations ===
    fn kv_get(&self, key: &str) -> Result<Option<Versioned<Value>>>;
    fn kv_put(&mut self, key: &str, value: Value) -> Result<Version>;
    fn kv_delete(&mut self, key: &str) -> Result<bool>;
    fn kv_exists(&self, key: &str) -> Result<bool>;

    // === Event Operations ===
    fn event_append(&mut self, event_type: &str, payload: Value) -> Result<Version>;
    fn event_read(&self, sequence: u64) -> Result<Option<Versioned<Event>>>;
    fn event_range(&self, start: u64, end: u64) -> Result<Vec<Versioned<Event>>>;

    // === State Operations ===
    fn state_read(&self, name: &str) -> Result<Option<Versioned<StateValue>>>;
    fn state_set(&mut self, name: &str, value: Value) -> Result<Version>;
    fn state_cas(&mut self, name: &str, expected: Version, value: Value) -> Result<Version>;
    fn state_delete(&mut self, name: &str) -> Result<bool>;
    fn state_exists(&self, name: &str) -> Result<bool>;

    // === Trace Operations ===
    fn trace_record(
        &mut self,
        trace_type: TraceType,
        content: Value,
        tags: Vec<String>
    ) -> Result<Versioned<TraceId>>;
    fn trace_read(&self, trace_id: &TraceId) -> Result<Option<Versioned<Trace>>>;

    // === Json Operations ===
    fn json_create(&mut self, doc_id: &JsonDocId, value: JsonValue) -> Result<Version>;
    fn json_get(&self, doc_id: &JsonDocId) -> Result<Option<Versioned<JsonValue>>>;
    fn json_get_path(&self, doc_id: &JsonDocId, path: &JsonPath) -> Result<Option<Versioned<JsonValue>>>;
    fn json_set(&mut self, doc_id: &JsonDocId, path: &JsonPath, value: JsonValue) -> Result<Version>;
    fn json_delete(&mut self, doc_id: &JsonDocId) -> Result<bool>;
    fn json_exists(&self, doc_id: &JsonDocId) -> Result<bool>;

    // === Vector Operations ===
    fn vector_upsert(
        &mut self,
        collection: &CollectionId,
        entries: Vec<VectorEntry>
    ) -> Result<Version>;
    fn vector_get(
        &self,
        collection: &CollectionId,
        id: &VectorId
    ) -> Result<Option<Versioned<VectorEntry>>>;
    fn vector_delete(&mut self, collection: &CollectionId, id: &VectorId) -> Result<bool>;
    fn vector_search(
        &self,
        collection: &CollectionId,
        query: &[f32],
        k: usize
    ) -> Result<Vec<VectorMatch>>;
}
```

### 5.3 Transaction Lifetime

```rust
/// Example: Cross-primitive transaction
fn example_transaction(db: &Database, run_id: &RunId) -> Result<()> {
    db.transaction(run_id, |txn| {
        // All operations in this closure are atomic

        // Read from KV
        let config = txn.kv_get("config")?;

        // Append to event log
        let event_version = txn.event_append("config_read", json!({
            "config_key": "config",
            "had_value": config.is_some()
        }))?;

        // Update state cell
        txn.state_set("last_event", Value::from(event_version.as_u64()))?;

        // Record trace
        txn.trace_record(TraceType::Action, json!({
            "action": "read_config"
        }), vec!["config".to_string()])?;

        Ok(())
    })
}
```

---

## 6. Primitive Conformance

### 6.1 Conformance Matrix

Each primitive must be verified against each invariant:

| Primitive | Addr | Ver | Txn | Life | Run | Intro | R/W |
|-----------|------|-----|-----|------|-----|-------|-----|
| KVStore | `EntityRef::Kv` | `Versioned<Value>` | `TransactionOps::kv_*` | CRUD | RunId param | `exists()` | get/put |
| EventLog | `EntityRef::Event` | `Versioned<Event>` | `TransactionOps::event_*` | CR | RunId param | `read()` | read/append |
| StateCell | `EntityRef::State` | `Versioned<StateValue>` | `TransactionOps::state_*` | CRUD | RunId param | `exists()` | read/set |
| TraceStore | `EntityRef::Trace` | `Versioned<Trace>` | `TransactionOps::trace_*` | CR | RunId param | `read()` | read/record |
| RunIndex | `EntityRef::Run` | `Versioned<RunMetadata>` | `TransactionOps::run_*` | CRUD | (meta) | `exists()` | get/create |
| JsonStore | `EntityRef::Json` | `Versioned<JsonValue>` | `TransactionOps::json_*` | CRUD | RunId param | `exists()` | get/set |
| VectorStore | `EntityRef::Vector` | `Versioned<VectorEntry>` | `TransactionOps::vector_*` | CRUD | RunId param | `get()` | get/upsert |

### 6.2 Per-Primitive Requirements

#### KVStore

```rust
// Invariant 1: Addressable
let ref = EntityRef::Kv { run_id: run_id.clone(), key: "my_key".to_string() };

// Invariant 2: Versioned reads
let result: Option<Versioned<Value>> = kv.get(run_id, "my_key")?;

// Invariant 2: Versioned writes
let version: Version = kv.put(run_id, "my_key", value)?;

// Invariant 3: Transactional
db.transaction(&run_id, |txn| {
    txn.kv_put("key", value)?;
    Ok(())
})?;

// Invariant 4: Lifecycle (CRUD)
kv.put(run_id, key, value)?;  // Create/Update
kv.get(run_id, key)?;          // Read
kv.delete(run_id, key)?;       // Delete

// Invariant 5: Run-scoped
kv.get(&run_id, "key")?;  // Run always explicit

// Invariant 6: Introspectable
kv.exists(&run_id, "key")?;

// Invariant 7: Consistent R/W
// get is read (no state change), put is write (returns version)
```

#### EventLog

```rust
// Invariant 1: Addressable
let ref = EntityRef::Event { run_id: run_id.clone(), sequence: 42 };

// Invariant 2: Versioned
let event: Option<Versioned<Event>> = events.read(run_id, 42)?;
let seq: Version = events.append(run_id, "type", payload)?;  // Version::Sequence

// Invariant 3: Transactional
db.transaction(&run_id, |txn| {
    txn.event_append("type", payload)?;
    Ok(())
})?;

// Invariant 4: Lifecycle (CR - append only)
events.append(run_id, "type", payload)?;  // Create
events.read(run_id, seq)?;                 // Read
// No Update, No Delete (immutable)

// Invariant 5: Run-scoped
events.read(&run_id, seq)?;

// Invariant 6: Introspectable
events.read(&run_id, seq)?;  // Returns Option

// Invariant 7: Consistent R/W
// read is read, append is write
```

*(Similar patterns for StateCell, TraceStore, RunIndex, JsonStore, VectorStore)*

---

## 7. API Consistency Audit

### 7.1 Audit Checklist

M9 must verify:

| Check | Description | Status |
|-------|-------------|--------|
| All reads return `Versioned<T>` | No raw value returns | [ ] |
| All writes return `Version` | No void writes | [ ] |
| All primitives in `TransactionOps` | Complete trait coverage | [ ] |
| All operations accept `RunId` | Explicit scope | [ ] |
| All primitives have `exists()` | Introspection support | [ ] |
| Consistent error types | `StrataError` everywhere | [ ] |
| No primitive-specific patterns | Same API shape | [ ] |

### 7.2 Breaking Changes Identified

Document any breaking changes from current API:

| Change | Current | M9 | Migration |
|--------|---------|-----|-----------|
| KV get return | `Option<Value>` | `Option<Versioned<Value>>` | Wrap existing returns |
| Event append return | `u64` | `Version` | Wrap in `Version::Sequence` |
| State set return | `()` | `Version` | Return version from write |
| ... | ... | ... | ... |

---

## 8. Error Handling

### 8.1 Unified Error Type

```rust
/// Errors from Strata operations
#[derive(Debug, thiserror::Error)]
pub enum StrataError {
    /// Entity not found
    #[error("entity not found: {entity_ref:?}")]
    NotFound { entity_ref: EntityRef },

    /// Version conflict (CAS failure, OCC conflict)
    #[error("version conflict on {entity_ref:?}: expected {expected:?}, got {actual:?}")]
    VersionConflict {
        entity_ref: EntityRef,
        expected: Version,
        actual: Version,
    },

    /// Transaction aborted
    #[error("transaction aborted: {reason}")]
    TransactionAborted { reason: String },

    /// Run not found
    #[error("run not found: {run_id}")]
    RunNotFound { run_id: RunId },

    /// Invalid operation
    #[error("invalid operation on {entity_ref:?}: {reason}")]
    InvalidOperation { entity_ref: EntityRef, reason: String },

    /// Dimension mismatch (vectors)
    #[error("dimension mismatch: expected {expected}, got {got}")]
    DimensionMismatch { expected: usize, got: usize },

    /// Storage error
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),

    /// Serialization error
    #[error("serialization error: {0}")]
    Serialization(String),
}
```

### 8.2 Error Conversion

All primitive-specific errors must convert to `StrataError`:

```rust
impl From<KvError> for StrataError {
    fn from(e: KvError) -> Self {
        match e {
            KvError::NotFound { key, run_id } => StrataError::NotFound {
                entity_ref: EntityRef::Kv { run_id, key },
            },
            // ...
        }
    }
}
```

---

## 9. Migration Strategy

### 9.1 Phased Migration

M9 migration happens in phases:

**Phase 1: Add Types**
- Add `EntityRef`, `Versioned<T>`, `Version`, unified errors
- Existing API continues to work

**Phase 2: Wrap Returns**
- Reads start returning `Versioned<T>`
- Writes start returning `Version`
- Deprecate old return types

**Phase 3: Update Callers**
- Update all internal code to use new types
- Update tests to expect new types

**Phase 4: Remove Deprecated**
- Remove old API surface
- Finalize documentation

### 9.2 Backwards Compatibility

During migration, provide compatibility helpers:

```rust
impl<T> Versioned<T> {
    /// Extract just the value, discarding version info
    ///
    /// DEPRECATED: Use versioned returns for new code
    pub fn into_value(self) -> T {
        self.value
    }
}
```

---

## 10. Testing Strategy

### 10.1 Conformance Tests

For each primitive P and invariant I, there must be a test:

```rust
#[cfg(test)]
mod conformance {
    mod kv {
        #[test]
        fn invariant_1_addressable() { /* KV is addressable */ }

        #[test]
        fn invariant_2_versioned_read() { /* KV reads return Versioned */ }

        #[test]
        fn invariant_2_versioned_write() { /* KV writes return Version */ }

        #[test]
        fn invariant_3_transactional() { /* KV participates in transactions */ }

        #[test]
        fn invariant_4_lifecycle() { /* KV has CRUD lifecycle */ }

        #[test]
        fn invariant_5_run_scoped() { /* KV is run-scoped */ }

        #[test]
        fn invariant_6_introspectable() { /* KV has exists() */ }

        #[test]
        fn invariant_7_read_write_consistency() { /* reads don't modify */ }
    }

    mod event { /* Same 7 tests */ }
    mod state { /* Same 7 tests */ }
    mod trace { /* Same 7 tests */ }
    mod run { /* Same 7 tests */ }
    mod json { /* Same 7 tests */ }
    mod vector { /* Same 7 tests */ }
}
```

Total: 7 primitives × 7 invariants = **49 conformance tests**

### 10.2 Cross-Primitive Tests

```rust
#[test]
fn cross_primitive_transaction_atomicity() {
    // KV + Event + State + Trace + Json + Vector in one transaction
    // Verify all-or-nothing semantics
}

#[test]
fn cross_primitive_isolation() {
    // Concurrent transactions on different primitives
    // Verify snapshot isolation
}
```

### 10.3 Migration Tests

```rust
#[test]
fn migration_versioned_wrapper_preserves_value() {
    // Versioned<T>.into_value() returns original value
}

#[test]
fn migration_old_code_still_works() {
    // Code pattern that worked before M9 still works
}
```

---

## 11. Known Limitations

### 11.1 EntityRef Limitations

- EntityRef is an enum, not a trait. Adding new primitives requires enum extension.
- EntityRef does not support sub-entity addressing (e.g., JSON path within document).
- EntityRef serialization format is not specified (deferred to M10 wire protocol).

### 11.2 Version Limitations

- Versions are not comparable across primitives (TxnId vs Sequence vs Counter).
- Version history is not queryable through core API (deferred to Magic APIs).
- Version does not include causal information.

### 11.3 Transaction Limitations

- Transactions are single-run only (no cross-run transactions).
- Transaction timeout is implementation-defined.
- Read-only transactions are not explicitly typed.

---

## 12. Future Extension Points

### 12.1 For M10 (Wire Protocol)

- `EntityRef` serialization format
- `Versioned<T>` wire representation
- `StrataError` error codes for wire

### 12.2 For Post-MVP (Magic APIs)

- Version history queries
- EntityRef sub-addressing
- Causal version tracking
- Cross-run references

---

## 13. Success Criteria Checklist

### Gate 1: Primitive Contract (Constitutional)

- [ ] Seven invariants documented in PRIMITIVE_CONTRACT.md
- [ ] All 7 primitives conform to all 7 invariants
- [ ] Conformance tests pass (49 tests)

### Gate 2: Core API Shape

- [ ] `EntityRef` type implemented
- [ ] `Versioned<T>` wrapper for all read operations
- [ ] `Version` type for all write returns
- [ ] Unified `TransactionOps` trait with all primitives
- [ ] `RunHandle` pattern implemented
- [ ] `StrataError` for all error cases

### Gate 3: API Consistency Audit

- [ ] All reads return `Versioned<T>`
- [ ] All writes return `Version`
- [ ] All primitives accessible through same patterns
- [ ] No primitive-specific special cases in core API
- [ ] Audit checklist complete

### Gate 4: Documentation

- [ ] PRIMITIVE_CONTRACT.md finalized
- [ ] CORE_API_SHAPE.md finalized
- [ ] PRODUCT_SURFACES.md documented
- [ ] Migration guide written

### Gate 5: Validation

- [ ] Example code works with new API
- [ ] Existing tests updated for `Versioned<T>` returns
- [ ] API review completed
- [ ] Cross-primitive transaction tests pass

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-19 | Initial M9 architecture specification |
