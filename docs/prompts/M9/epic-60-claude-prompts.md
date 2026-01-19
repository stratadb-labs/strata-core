# Epic 60: Core Types - Implementation Prompts

**Epic Goal**: Define universal types that express the seven invariants

**GitHub Issue**: [#464](https://github.com/anibjoshi/in-mem/issues/464)
**Status**: Ready to begin
**Dependencies**: M8 complete
**Phase**: 1 (Foundation)

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
- RunId standardization

### Key Rule: Types Express Invariants

> These types are the API expression of the seven invariants from PRIMITIVE_CONTRACT.md.
> Every type has a specific purpose tied to an invariant.

| Type | Invariant |
|------|-----------|
| EntityRef | Invariant 1: Everything is Addressable |
| Versioned<T> | Invariant 2: Everything is Versioned |
| Version | Invariant 2: Everything is Versioned |
| RunId | Invariant 5: Everything Exists Within a Run |
| PrimitiveType | Invariant 6: Everything is Introspectable |
| Timestamp | Invariant 2: Everything is Versioned (temporal) |

### Success Criteria
- [ ] `EntityRef` enum with variants for all 7 primitives
- [ ] `EntityRef::run_id()` method returns the run for any entity
- [ ] `EntityRef::primitive_type()` method returns `PrimitiveType`
- [ ] `Versioned<T>` with value, version, timestamp fields
- [ ] `Versioned<T>::map()` for transforming inner value
- [ ] `Version` enum: TxnId(u64), Sequence(u64), Counter(u64)
- [ ] `Version::as_u64()` for numeric comparison
- [ ] `Timestamp` type with `now()` constructor
- [ ] `RunId` newtype with `new()`, `as_str()`, Display impl
- [ ] All types implement Debug, Clone; IDs implement Hash, Eq

### Component Breakdown
- **Story #469**: EntityRef Enum Implementation - FOUNDATION
- **Story #470**: Versioned<T> Wrapper Type - FOUNDATION
- **Story #471**: Version Enum - FOUNDATION
- **Story #472**: Timestamp Type - FOUNDATION
- **Story #473**: PrimitiveType Enum - HIGH
- **Story #474**: RunId Standardization - FOUNDATION

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
**Estimated Time**: 2 hours
**Dependencies**: Stories #473, #474
**Blocks**: All Epic 61, 62, 63 stories

### Start Story

```bash
gh issue view 469
./scripts/start-story.sh 60 469 entity-ref
```

### Implementation Steps

#### Step 1: Create entity_ref.rs module

Create `crates/core/src/entity_ref.rs`:

```rust
//! Universal entity reference for any Strata entity
//!
//! This type expresses Invariant 1: Everything is Addressable.
//! Every entity in Strata has a stable identity that can be:
//! - Referenced
//! - Stored
//! - Passed between systems
//! - Used to retrieve the entity later

use crate::{RunId, PrimitiveType};

// Import ID types from primitives (adjust paths as needed)
use crate::TraceId;
use crate::JsonDocId;
use crate::VectorId;

/// Universal entity reference for any Strata entity
///
/// Every entity in Strata can be uniquely identified by an EntityRef.
/// This enables uniform addressing across all primitives.
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

#### Step 2: Update lib.rs

```rust
pub mod entity_ref;
pub use entity_ref::EntityRef;
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
**Estimated Time**: 1.5 hours
**Dependencies**: Stories #471, #472
**Blocks**: All Epic 61 stories

### Start Story

```bash
gh issue view 470
./scripts/start-story.sh 60 470 versioned-wrapper
```

### Implementation

Create `crates/core/src/versioned.rs`:

```rust
//! Versioned value wrapper
//!
//! This type expresses Invariant 2: Everything is Versioned.
//! When you read an entity, you know:
//! 1. Which version you are looking at
//! 2. When that version came into existence

use crate::{Version, Timestamp};

/// A value with its version and timestamp
///
/// This wrapper ensures version information is never lost when reading entities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Versioned<T> {
    /// The actual value
    pub value: T,
    /// The version of this value
    pub version: Version,
    /// When this version was created
    pub timestamp: Timestamp,
}

impl<T> Versioned<T> {
    /// Create a new versioned value
    pub fn new(value: T, version: Version, timestamp: Timestamp) -> Self {
        Self { value, version, timestamp }
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
        }
    }

    /// Get a reference to the inner value with version info
    pub fn as_ref(&self) -> Versioned<&T> {
        Versioned {
            value: &self.value,
            version: self.version.clone(),
            timestamp: self.timestamp,
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
}

impl<T: Default> Default for Versioned<T> {
    fn default() -> Self {
        Self {
            value: T::default(),
            version: Version::TxnId(0),
            timestamp: Timestamp::EPOCH,
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
    fn test_versioned_new() {
        let versioned = Versioned::new(
            "hello".to_string(),
            Version::TxnId(42),
            Timestamp::from_micros(1000),
        );

        assert_eq!(versioned.value, "hello");
        assert_eq!(versioned.version, Version::TxnId(42));
        assert_eq!(versioned.timestamp.as_micros(), 1000);
    }

    #[test]
    fn test_versioned_map() {
        let versioned = Versioned::new(
            42i32,
            Version::TxnId(1),
            Timestamp::from_micros(1000),
        );

        let mapped = versioned.map(|v| v.to_string());

        assert_eq!(mapped.value, "42");
        assert_eq!(mapped.version, Version::TxnId(1));
        assert_eq!(mapped.timestamp.as_micros(), 1000);
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
}
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

Create `crates/core/src/version.rs`:

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

Create `crates/core/src/timestamp.rs`:

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

Create `crates/core/src/primitive_type.rs`:

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

## Story #474: RunId Standardization

**GitHub Issue**: [#474](https://github.com/anibjoshi/in-mem/issues/474)
**Estimated Time**: 1 hour
**Dependencies**: None
**Blocks**: Story #469

### Start Story

```bash
gh issue view 474
./scripts/start-story.sh 60 474 run-id
```

### Implementation

Modify or create `crates/core/src/run_id.rs`:

```rust
//! Run identifier type

/// Unique identifier for a run (execution context)
///
/// All data in Strata is scoped to a run (Invariant 5).
/// A run is the unit of isolation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RunId(String);

impl RunId {
    /// Create a new RunId from a string
    ///
    /// # Panics
    /// Panics if the id is empty.
    pub fn new(id: impl Into<String>) -> Self {
        let id = id.into();
        assert!(!id.is_empty(), "RunId cannot be empty");
        Self(id)
    }

    /// Create a new RunId with a generated UUID
    #[cfg(feature = "uuid")]
    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// Get the string representation
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for RunId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for RunId {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl From<&str> for RunId {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl AsRef<str> for RunId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}
```

### Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_id_new() {
        let run_id = RunId::new("my-run");
        assert_eq!(run_id.as_str(), "my-run");
    }

    #[test]
    #[should_panic(expected = "RunId cannot be empty")]
    fn test_run_id_empty_panics() {
        RunId::new("");
    }

    #[test]
    fn test_run_id_display() {
        let run_id = RunId::new("test-123");
        assert_eq!(format!("{}", run_id), "test-123");
    }

    #[test]
    fn test_run_id_from_string() {
        let run_id: RunId = "from-string".into();
        assert_eq!(run_id.as_str(), "from-string");
    }

    #[test]
    fn test_run_id_equality() {
        let r1 = RunId::new("same");
        let r2 = RunId::new("same");
        let r3 = RunId::new("different");

        assert_eq!(r1, r2);
        assert_ne!(r1, r3);
    }

    #[test]
    fn test_run_id_hash() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        set.insert(RunId::new("run1"));
        set.insert(RunId::new("run2"));

        assert!(set.contains(&RunId::new("run1")));
        assert!(!set.contains(&RunId::new("run3")));
    }
}
```

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
- [ ] RunId newtype with validation

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
- RunId standardization

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
- **RunId** enforces run scoping (Invariant 5)
