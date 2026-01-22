# Epic 60: Core Types

**Goal**: Define universal types that express the seven invariants

**Dependencies**: M8 complete

---

## Scope

- EntityRef enum for universal addressing (Invariant 1)
- Versioned<T> wrapper for versioned reads (Invariant 2)
- Version enum for write returns (Invariant 2)
- Timestamp type for temporal tracking
- PrimitiveType enum for type discrimination
- **RunName + RunId dual identity model** (Invariant 5) - semantic user-facing names + internal UUIDs
- **Contract module organization** - single cohesive home for all M9 contract types

---

## File Organization: The Contract Module

> **Critical**: M9 introduces types that define the **semantic contract** of the system.
> These are not implementation details - they encode the seven invariants.
> They must live in a single, discoverable, stable location.

All Epic 60 types are created in `crates/core/src/contract/`:

```
crates/core/src/
├── contract/              # NEW: M9 contract types module
│   ├── mod.rs             # Module exports
│   ├── entity_ref.rs      # Story #469: EntityRef, PrimitiveType
│   ├── versioned.rs       # Story #470: Versioned<T>
│   ├── version.rs         # Story #471: Version enum
│   ├── timestamp.rs       # Story #472: Timestamp type
│   └── run_name.rs        # Story #474: RunName type
├── types.rs               # Internal types (RunId stays here)
├── value.rs               # Value enum (+ VersionedValue alias)
├── search_types.rs        # Search types (+ DocRef, PrimitiveKind aliases)
└── lib.rs                 # Re-exports contract types prominently
```

**Why a contract module?**
- Single source of truth for the system's semantics
- Clear API stability boundary
- Easy documentation and SDK generation
- Prevents semantic drift
- Clean mental model for contributors

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #469 | EntityRef Enum Implementation | FOUNDATION |
| #470 | Versioned<T> Wrapper Type | FOUNDATION |
| #471 | Version Enum (TxnId, Sequence, Counter) | FOUNDATION |
| #472 | Timestamp Type | FOUNDATION |
| #473 | PrimitiveType Enum | HIGH |
| #474 | RunName + RunId Dual Identity Model | FOUNDATION |

---

## Story #469: EntityRef Enum Implementation

**File**: `crates/core/src/entity_ref.rs` (NEW)

**Deliverable**: Universal addressing type for all entities

### Unification with Existing DocRef

> **Critical Design Decision**: The codebase already has `DocRef` in `crates/core/src/search_types.rs`.
> M9 cannot introduce a competing `EntityRef` - that creates parallel identity systems.
>
> **Solution**: `EntityRef` is the canonical type. `DocRef` becomes a type alias.

The current `DocRef` (in `search_types.rs`):
```rust
pub enum DocRef {
    Kv { key: Key },
    Json { key: Key, doc_id: JsonDocId },
    Event { log_key: Key, seq: u64 },
    State { key: Key },
    Trace { key: Key, span_id: u64 },
    Run { run_id: RunId },
    Vector { collection: String, key: String, run_id: RunId },
}
```

Must unify with M9's `EntityRef`:
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

// Type alias for backwards compatibility with search layer
pub type DocRef = EntityRef;
```

**Key Insight**: DocRef embeds run_id inside the `Key` type (via `Key.namespace.run_id`).
EntityRef makes run_id explicit and top-level. This is cleaner and enables better APIs.

### Implementation

```rust
use crate::{RunId, TraceId, JsonDocId, VectorId};

/// Reference to any entity in the database
///
/// This type expresses Invariant 1: Everything is Addressable.
/// Every entity has a stable identity that can be:
/// - Referenced
/// - Stored
/// - Passed between systems
/// - Used to retrieve the entity later
///
/// IMPORTANT: This enum covers all 7 primitives. When adding a new
/// primitive, a new variant MUST be added here.
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

    /// Run metadata (the run itself)
    Run { run_id: RunId },

    /// JSON document: run + document ID
    Json { run_id: RunId, doc_id: JsonDocId },

    /// Vector: run + collection + vector ID
    Vector {
        run_id: RunId,
        collection: String,
        vector_id: VectorId,
    },
}

impl EntityRef {
    // === Constructors ===

    /// Create a KV entity reference
    pub fn kv(run_id: RunId, key: impl Into<String>) -> Self {
        EntityRef::Kv {
            run_id,
            key: key.into(),
        }
    }

    /// Create an Event entity reference
    pub fn event(run_id: RunId, sequence: u64) -> Self {
        EntityRef::Event { run_id, sequence }
    }

    /// Create a State entity reference
    pub fn state(run_id: RunId, name: impl Into<String>) -> Self {
        EntityRef::State {
            run_id,
            name: name.into(),
        }
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
        EntityRef::Vector {
            run_id,
            collection: collection.into(),
            vector_id,
        }
    }

    // === Accessors ===

    /// Returns the run this entity belongs to
    ///
    /// All entities belong to exactly one run (Invariant 5).
    /// This method never fails.
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

    /// Returns the primitive type of this entity
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

    /// Returns a human-readable description of the entity
    pub fn description(&self) -> String {
        match self {
            EntityRef::Kv { run_id, key } => {
                format!("KV[{}:{}]", run_id, key)
            }
            EntityRef::Event { run_id, sequence } => {
                format!("Event[{}:{}]", run_id, sequence)
            }
            EntityRef::State { run_id, name } => {
                format!("State[{}:{}]", run_id, name)
            }
            EntityRef::Trace { run_id, trace_id } => {
                format!("Trace[{}:{:?}]", run_id, trace_id)
            }
            EntityRef::Run { run_id } => {
                format!("Run[{}]", run_id)
            }
            EntityRef::Json { run_id, doc_id } => {
                format!("Json[{}:{:?}]", run_id, doc_id)
            }
            EntityRef::Vector { run_id, collection, vector_id } => {
                format!("Vector[{}:{}:{:?}]", run_id, collection, vector_id)
            }
        }
    }
}

impl std::fmt::Display for EntityRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.description())
    }
}

/// Primitive type discriminator
///
/// Used for type-level operations without the full EntityRef data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrimitiveType {
    Kv,
    Event,
    State,
    Trace,
    Run,
    Json,
    Vector,
}

impl PrimitiveType {
    /// Human-readable name
    pub fn name(&self) -> &'static str {
        match self {
            PrimitiveType::Kv => "KV",
            PrimitiveType::Event => "Event",
            PrimitiveType::State => "State",
            PrimitiveType::Trace => "Trace",
            PrimitiveType::Run => "Run",
            PrimitiveType::Json => "Json",
            PrimitiveType::Vector => "Vector",
        }
    }

    /// All primitive types (for iteration)
    pub fn all() -> &'static [PrimitiveType] {
        &[
            PrimitiveType::Kv,
            PrimitiveType::Event,
            PrimitiveType::State,
            PrimitiveType::Trace,
            PrimitiveType::Run,
            PrimitiveType::Json,
            PrimitiveType::Vector,
        ]
    }
}

impl std::fmt::Display for PrimitiveType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}
```

### Acceptance Criteria

- [ ] EntityRef enum with 7 variants (one per primitive)
- [ ] Constructor methods for each variant
- [ ] `run_id()` returns the run for any entity
- [ ] `primitive_type()` returns PrimitiveType
- [ ] `description()` for human-readable output
- [ ] Display impl for error messages
- [ ] Implements Debug, Clone, PartialEq, Eq, Hash
- [ ] **Type alias**: `pub type DocRef = EntityRef;` in search_types.rs
- [ ] **Conversions**: `From<DocRef> for EntityRef` (if needed during transition)
- [ ] **Migration**: Existing search code continues to work via type alias

---

## Story #470: Versioned<T> Wrapper Type

**File**: `crates/core/src/versioned.rs` (NEW)

**Deliverable**: Universal wrapper for versioned read results

### Unification with Existing VersionedValue

> **Critical Design Decision**: The codebase already has `VersionedValue` in `crates/core/src/value.rs`.
> M9 cannot introduce a competing `Versioned<T>` abstraction - that creates semantic duplication.
>
> **Solution**: `Versioned<T>` is the canonical type. `VersionedValue` becomes a type alias.

The current `VersionedValue`:
```rust
// crates/core/src/value.rs (CURRENT)
pub struct VersionedValue {
    pub value: Value,
    pub version: u64,
    pub timestamp: Timestamp,
    pub ttl: Option<Duration>,
}
```

Must unify with M9's `Versioned<T>`:
```rust
// After M9
pub struct Versioned<T> {
    pub value: T,
    pub version: Version,
    pub timestamp: Timestamp,
    pub ttl: Option<Duration>,  // Added for compatibility
}

// Type alias for backwards compatibility
pub type VersionedValue = Versioned<Value>;
```

### Implementation

```rust
use crate::{Version, Timestamp};
use std::time::Duration;

/// Wrapper for any value read from the database
///
/// This type expresses Invariant 2: Everything is Versioned.
/// Every read returns version information. Version information
/// is NEVER optional.
///
/// IMPORTANT: There is no "read without version" API. If you read
/// something, you get its version. You can ignore it, but it's
/// always there.
///
/// NOTE: This type unifies with the existing VersionedValue.
/// After M9, `VersionedValue = Versioned<Value>`.
#[derive(Debug, Clone)]
pub struct Versioned<T> {
    /// The actual value
    pub value: T,

    /// Version identifier
    pub version: Version,

    /// When this version was created
    pub timestamp: Timestamp,

    /// Optional time-to-live (for primitives that support TTL)
    pub ttl: Option<Duration>,
}

impl<T> Versioned<T> {
    /// Create a new Versioned wrapper
    pub fn new(value: T, version: Version, timestamp: Timestamp) -> Self {
        Self {
            value,
            version,
            timestamp,
            ttl: None,
        }
    }

    /// Create with TTL
    pub fn with_ttl(value: T, version: Version, timestamp: Timestamp, ttl: Option<Duration>) -> Self {
        Self {
            value,
            version,
            timestamp,
            ttl,
        }
    }

    /// Create with current timestamp
    pub fn now(value: T, version: Version) -> Self {
        Self {
            value,
            version,
            timestamp: Timestamp::now(),
            ttl: None,
        }
    }

    /// Map the inner value while preserving version info
    ///
    /// Useful for transforming the value without losing metadata.
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Versioned<U> {
        Versioned {
            value: f(self.value),
            version: self.version,
            timestamp: self.timestamp,
            ttl: self.ttl,
        }
    }

    /// Map with a fallible function
    pub fn try_map<U, E, F: FnOnce(T) -> Result<U, E>>(self, f: F) -> Result<Versioned<U>, E> {
        Ok(Versioned {
            value: f(self.value)?,
            version: self.version,
            timestamp: self.timestamp,
            ttl: self.ttl,
        })
    }

    /// Get a reference to the inner value
    pub fn as_ref(&self) -> Versioned<&T> {
        Versioned {
            value: &self.value,
            version: self.version,
            timestamp: self.timestamp,
            ttl: self.ttl,
        }
    }

    /// Extract just the value, discarding version info
    ///
    /// DEPRECATED: Use versioned returns for new code.
    /// This method exists for migration compatibility.
    #[deprecated(
        since = "0.9.0",
        note = "Use versioned returns directly. This method discards important version information."
    )]
    pub fn into_value(self) -> T {
        self.value
    }

    /// Check if this version is newer than another
    pub fn is_newer_than(&self, other: &Versioned<T>) -> bool {
        self.version > other.version
    }

    /// Check if the value has expired based on TTL
    ///
    /// Compatibility method matching VersionedValue::is_expired()
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

impl<T: PartialEq> PartialEq for Versioned<T> {
    fn eq(&self, other: &Self) -> bool {
        // Two versioned values are equal if their values and versions match
        self.value == other.value && self.version == other.version
    }
}

impl<T: Eq> Eq for Versioned<T> {}

impl<T: Default> Default for Versioned<T> {
    fn default() -> Self {
        Versioned {
            value: T::default(),
            version: Version::TxnId(0),
            timestamp: Timestamp::EPOCH,
        }
    }
}

// Convenience: Versioned<Option<T>> operations
impl<T> Versioned<Option<T>> {
    /// Check if the inner option is Some
    pub fn is_some(&self) -> bool {
        self.value.is_some()
    }

    /// Check if the inner option is None
    pub fn is_none(&self) -> bool {
        self.value.is_none()
    }

    /// Transpose Versioned<Option<T>> to Option<Versioned<T>>
    pub fn transpose(self) -> Option<Versioned<T>> {
        self.value.map(|v| Versioned {
            value: v,
            version: self.version,
            timestamp: self.timestamp,
        })
    }
}
```

### Acceptance Criteria

- [ ] Versioned<T> with value, version, timestamp, ttl fields
- [ ] `new()` and `now()` constructors (ttl defaults to None)
- [ ] `with_ttl()` constructor for TTL support
- [ ] `map()` for value transformation (preserves TTL)
- [ ] `try_map()` for fallible transformation
- [ ] `as_ref()` for borrowing
- [ ] `into_value()` deprecated but available for migration
- [ ] `is_newer_than()` for version comparison
- [ ] `is_expired()` for TTL checking (VersionedValue compatibility)
- [ ] PartialEq, Eq implementations
- [ ] Transpose for Option inner types
- [ ] **Type alias**: `pub type VersionedValue = Versioned<Value>;` in value.rs
- [ ] **Migration**: VersionedValue::new() still works via type alias

---

## Story #471: Version Enum

**File**: `crates/core/src/version.rs` (NEW)

**Deliverable**: Universal version type for all primitives

### Implementation

```rust
/// Version identifier
///
/// Versions are comparable within the same entity.
/// Versions may not be comparable across entities or across primitives.
///
/// Different primitives use different versioning schemes:
/// - KV, Trace, Run, Json, Vector: Transaction-based (TxnId)
/// - EventLog: Sequence-based (Sequence)
/// - StateCell: Counter-based (Counter)
///
/// This is an implementation detail. Users should not depend on
/// the specific variant; they should only use the ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Version {
    /// Transaction-based version (KV, Trace, Run, Json, Vector)
    ///
    /// Represents a global transaction ID. Multiple entities
    /// modified in the same transaction share this version.
    TxnId(u64),

    /// Sequence-based version (EventLog)
    ///
    /// Represents position in an append-only log.
    /// Unique within a run's event log.
    Sequence(u64),

    /// Counter-based version (StateCell)
    ///
    /// Represents a per-entity mutation counter.
    /// Increments on each modification.
    Counter(u64),
}

impl Version {
    /// Returns the numeric value for comparison
    ///
    /// NOTE: Only compare versions of the same variant.
    /// Cross-variant comparison is not meaningful.
    pub fn as_u64(&self) -> u64 {
        match self {
            Version::TxnId(v) => *v,
            Version::Sequence(v) => *v,
            Version::Counter(v) => *v,
        }
    }

    /// Check if this is a transaction-based version
    pub fn is_txn(&self) -> bool {
        matches!(self, Version::TxnId(_))
    }

    /// Check if this is a sequence-based version
    pub fn is_sequence(&self) -> bool {
        matches!(self, Version::Sequence(_))
    }

    /// Check if this is a counter-based version
    pub fn is_counter(&self) -> bool {
        matches!(self, Version::Counter(_))
    }

    /// Increment the version (returns new version)
    ///
    /// Used internally for mutations.
    pub fn increment(&self) -> Self {
        match self {
            Version::TxnId(v) => Version::TxnId(v + 1),
            Version::Sequence(v) => Version::Sequence(v + 1),
            Version::Counter(v) => Version::Counter(v + 1),
        }
    }

    /// Zero/initial version for each variant
    pub fn zero_txn() -> Self {
        Version::TxnId(0)
    }

    pub fn zero_sequence() -> Self {
        Version::Sequence(0)
    }

    pub fn zero_counter() -> Self {
        Version::Counter(0)
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        // Only compare same variants
        match (self, other) {
            (Version::TxnId(a), Version::TxnId(b)) => Some(a.cmp(b)),
            (Version::Sequence(a), Version::Sequence(b)) => Some(a.cmp(b)),
            (Version::Counter(a), Version::Counter(b)) => Some(a.cmp(b)),
            // Cross-variant comparison is undefined
            _ => None,
        }
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // For Ord, we need total ordering. Fall back to numeric comparison.
        // This is used for sorting; semantic comparison should use partial_cmp.
        self.as_u64().cmp(&other.as_u64())
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

// Convenient From implementations
impl From<u64> for Version {
    /// Default conversion uses TxnId (most common)
    fn from(v: u64) -> Self {
        Version::TxnId(v)
    }
}
```

### Acceptance Criteria

- [ ] Version enum with TxnId, Sequence, Counter variants
- [ ] `as_u64()` for numeric access
- [ ] Variant type checks: `is_txn()`, `is_sequence()`, `is_counter()`
- [ ] `increment()` for mutation tracking
- [ ] Zero constructors for each variant
- [ ] PartialOrd only compares same variants
- [ ] Ord provides total ordering for sorting
- [ ] Display impl for debugging

---

## Story #463: Timestamp Type

**File**: `crates/core/src/timestamp.rs` (NEW)

**Deliverable**: Timestamp type for version temporal tracking

### Implementation

```rust
use std::time::{SystemTime, UNIX_EPOCH};

/// Timestamp for version creation
///
/// Represents microseconds since Unix epoch.
/// Used for temporal ordering and debugging.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(pub u64);

impl Timestamp {
    /// Unix epoch (1970-01-01 00:00:00 UTC)
    pub const EPOCH: Timestamp = Timestamp(0);

    /// Maximum timestamp value
    pub const MAX: Timestamp = Timestamp(u64::MAX);

    /// Create a timestamp from microseconds since epoch
    pub const fn from_micros(micros: u64) -> Self {
        Timestamp(micros)
    }

    /// Create a timestamp from milliseconds since epoch
    pub const fn from_millis(millis: u64) -> Self {
        Timestamp(millis * 1000)
    }

    /// Create a timestamp from seconds since epoch
    pub const fn from_secs(secs: u64) -> Self {
        Timestamp(secs * 1_000_000)
    }

    /// Get current timestamp
    pub fn now() -> Self {
        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards");
        Timestamp(duration.as_micros() as u64)
    }

    /// Get microseconds since epoch
    pub const fn as_micros(&self) -> u64 {
        self.0
    }

    /// Get milliseconds since epoch
    pub const fn as_millis(&self) -> u64 {
        self.0 / 1000
    }

    /// Get seconds since epoch
    pub const fn as_secs(&self) -> u64 {
        self.0 / 1_000_000
    }

    /// Duration since another timestamp
    pub fn duration_since(&self, earlier: Timestamp) -> Option<std::time::Duration> {
        if self.0 >= earlier.0 {
            Some(std::time::Duration::from_micros(self.0 - earlier.0))
        } else {
            None
        }
    }

    /// Add duration to timestamp
    pub fn add(&self, duration: std::time::Duration) -> Self {
        Timestamp(self.0.saturating_add(duration.as_micros() as u64))
    }

    /// Subtract duration from timestamp
    pub fn sub(&self, duration: std::time::Duration) -> Self {
        Timestamp(self.0.saturating_sub(duration.as_micros() as u64))
    }
}

impl Default for Timestamp {
    fn default() -> Self {
        Timestamp::EPOCH
    }
}

impl std::fmt::Display for Timestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Format as ISO-like for readability
        let secs = self.0 / 1_000_000;
        let micros = self.0 % 1_000_000;
        write!(f, "{}.{:06}", secs, micros)
    }
}

impl From<u64> for Timestamp {
    fn from(micros: u64) -> Self {
        Timestamp(micros)
    }
}

impl From<Timestamp> for u64 {
    fn from(ts: Timestamp) -> Self {
        ts.0
    }
}

#[cfg(feature = "chrono")]
impl From<chrono::DateTime<chrono::Utc>> for Timestamp {
    fn from(dt: chrono::DateTime<chrono::Utc>) -> Self {
        Timestamp(dt.timestamp_micros() as u64)
    }
}
```

### Acceptance Criteria

- [ ] Timestamp newtype over u64 (microseconds)
- [ ] Constants: EPOCH, MAX
- [ ] Constructors: `from_micros`, `from_millis`, `from_secs`, `now`
- [ ] Accessors: `as_micros`, `as_millis`, `as_secs`
- [ ] Duration operations: `duration_since`, `add`, `sub`
- [ ] Implements Ord, PartialOrd, Eq, PartialEq, Hash
- [ ] Display impl for debugging

---

## Story #464: PrimitiveType Enum

**File**: `crates/core/src/entity_ref.rs` (included in #460)

**Deliverable**: Type discriminator for primitives

### Implementation

(Included in Story #460 EntityRef implementation)

### Acceptance Criteria

- [ ] PrimitiveType enum with 7 variants
- [ ] `name()` returns human-readable name
- [ ] `all()` returns slice of all types
- [ ] Display impl

---

## Story #474: RunName + RunId Dual Identity Model

**Files**:
- `crates/core/src/run_name.rs` (NEW)
- `crates/core/src/types.rs` (MODIFY - keep existing RunId(Uuid))

**Deliverable**: Dual identity model - semantic RunName for users, internal RunId for storage

### The Semantic Question

> **Who owns identity - the system or the user?**
>
> In a reasoning substrate, runs are not just storage buckets. They are:
> - Reasoning contexts (conceptual containers for cognition)
> - Memory boundaries (isolation units for agent state)
> - Replayable timelines (debuggable execution histories)
> - Semantic namespaces (user-meaningful identifiers)
>
> This requires user-owned semantic identity, not just machine-generated tokens.

### Implementation

#### RunName: User-Facing Semantic Identity

```rust
// crates/core/src/run_name.rs (NEW)

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
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        debug_assert!(!name.is_empty(), "RunName cannot be empty");
        Self(name)
    }

    /// Get the name as a string slice
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Check if this is a valid run name
    ///
    /// Run names must be non-empty and contain only valid characters.
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

#### RunId: Internal Storage Identity (Keep Existing)

```rust
// crates/core/src/types.rs (KEEP EXISTING - internal use only)

/// Internal identifier for a run (storage identity)
///
/// RunId is what the storage layer uses for indexing and references.
/// It is:
/// - Globally unique (UUID v4)
/// - Compact (16 bytes)
/// - Collision-free
///
/// NOTE: Users should not see or use RunIds directly. Public APIs
/// accept RunName, and the system manages the mapping internally.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunId(Uuid);

impl RunId {
    /// Create a new random RunId (internal use only)
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create from bytes (for deserialization/WAL recovery)
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(Uuid::from_bytes(bytes))
    }

    /// Get bytes (for serialization/WAL)
    pub fn as_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }
}
```

#### Name-to-ID Mapping in Database

```rust
// The Database manages the bidirectional RunName ↔ RunId mapping

impl Database {
    /// Create a new run with a user-provided name
    ///
    /// Returns error if name already exists.
    pub fn create_run(&self, name: impl Into<RunName>) -> Result<RunId> {
        let name = name.into();

        // Check if name already exists
        if self.run_name_to_id.contains_key(&name) {
            return Err(Error::RunNameExists { name });
        }

        // Generate new internal ID
        let run_id = RunId::new();

        // Store bidirectional mapping (persisted)
        self.run_name_to_id.insert(name.clone(), run_id);
        self.run_id_to_name.insert(run_id, name);

        Ok(run_id)
    }

    /// Get or create a run by name
    ///
    /// If name exists, returns existing RunId.
    /// If not, creates new run with that name.
    pub fn get_or_create_run(&self, name: impl Into<RunName>) -> RunId {
        let name = name.into();
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

    /// Get a run handle by name (primary public API)
    pub fn run(&self, name: impl Into<RunName>) -> RunHandle {
        let name = name.into();
        let run_id = self.get_or_create_run(name.clone());
        RunHandle { name, run_id, db: self.clone() }
    }
}
```

#### Public API Uses RunName

```rust
// CORRECT: User code is semantic and readable
let run = db.run("my-experiment-v2");
run.kv_put("config", json!({"learning_rate": 0.01})).await?;

// Logs are meaningful
log::info!("Processing run '{}'", run.name()); // "Processing run 'my-experiment-v2'"

// Scripts can reference runs by name
let replay = db.run("debug-replay-issue-42");
for event in replay.events().range(0..100)? {
    println!("{:?}", event);
}
```

### Why This Matters

**Without semantic identity:**
- Logs are full of opaque UUIDs
- CLI commands require copy-pasting tokens
- Scripts can't reference runs meaningfully
- Debugging is cognitively hostile
- The system feels like infrastructure, not a reasoning tool

**With semantic identity:**
- `db.run("debug-issue-42")` is self-documenting
- Logs say "Run 'experiment-v3'" not "Run 550e8400..."
- Scripts use meaningful names
- Humans can reason about runs conceptually
- The substrate aligns with cognition

### Migration Strategy

1. **Keep existing RunId(Uuid)** - no storage changes
2. **Add RunName type** - new semantic layer
3. **Add mapping table** - persisted bidirectional mapping
4. **Update public APIs** - accept RunName, return RunHandle
5. **Internal code unchanged** - still uses RunId for storage

### Acceptance Criteria

- [ ] RunName newtype over String
- [ ] RunName validation (non-empty, valid characters)
- [ ] RunName Display, From<&str>, From<String>
- [ ] Keep existing RunId(Uuid) unchanged
- [ ] Bidirectional mapping: RunName ↔ RunId
- [ ] `db.run(name)` returns RunHandle
- [ ] `db.create_run(name)` for explicit creation
- [ ] `db.resolve_run(name)` returns Option<RunId>
- [ ] Mapping persisted across restarts
- [ ] EntityRef uses internal RunId (no change)
- [ ] Error on duplicate RunName creation

---

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // === EntityRef Tests ===

    #[test]
    fn test_entity_ref_constructors() {
        let run_id = RunId::new("test-run");

        let kv_ref = EntityRef::kv(run_id.clone(), "my-key");
        assert_eq!(kv_ref.run_id(), &run_id);
        assert_eq!(kv_ref.primitive_type(), PrimitiveType::Kv);

        let event_ref = EntityRef::event(run_id.clone(), 42);
        assert_eq!(event_ref.primitive_type(), PrimitiveType::Event);

        let state_ref = EntityRef::state(run_id.clone(), "my-state");
        assert_eq!(state_ref.primitive_type(), PrimitiveType::State);
    }

    #[test]
    fn test_entity_ref_description() {
        let run_id = RunId::new("run-123");
        let kv_ref = EntityRef::kv(run_id, "key-456");

        let desc = kv_ref.description();
        assert!(desc.contains("run-123"));
        assert!(desc.contains("key-456"));
    }

    #[test]
    fn test_primitive_type_all() {
        let all = PrimitiveType::all();
        assert_eq!(all.len(), 7);
        assert!(all.contains(&PrimitiveType::Kv));
        assert!(all.contains(&PrimitiveType::Vector));
    }

    // === Versioned Tests ===

    #[test]
    fn test_versioned_map() {
        let v = Versioned::new(42, Version::TxnId(1), Timestamp::now());
        let mapped = v.map(|x| x * 2);

        assert_eq!(mapped.value, 84);
        assert_eq!(mapped.version, Version::TxnId(1));
    }

    #[test]
    fn test_versioned_transpose() {
        let v: Versioned<Option<i32>> = Versioned::new(
            Some(42),
            Version::TxnId(1),
            Timestamp::now(),
        );

        let transposed = v.transpose();
        assert!(transposed.is_some());
        assert_eq!(transposed.unwrap().value, 42);

        let v_none: Versioned<Option<i32>> = Versioned::new(
            None,
            Version::TxnId(1),
            Timestamp::now(),
        );
        assert!(v_none.transpose().is_none());
    }

    #[test]
    fn test_versioned_is_newer_than() {
        let v1 = Versioned::new(1, Version::TxnId(1), Timestamp::now());
        let v2 = Versioned::new(2, Version::TxnId(2), Timestamp::now());

        assert!(v2.is_newer_than(&v1));
        assert!(!v1.is_newer_than(&v2));
    }

    // === Version Tests ===

    #[test]
    fn test_version_comparison() {
        let v1 = Version::TxnId(1);
        let v2 = Version::TxnId(2);
        let v3 = Version::Sequence(1);

        // Same variant comparison
        assert!(v1 < v2);
        assert!(v2 > v1);

        // Cross-variant comparison via partial_cmp
        assert!(v1.partial_cmp(&v3).is_none());
    }

    #[test]
    fn test_version_increment() {
        let v = Version::TxnId(1);
        let incremented = v.increment();
        assert_eq!(incremented, Version::TxnId(2));

        let s = Version::Sequence(10);
        assert_eq!(s.increment(), Version::Sequence(11));
    }

    #[test]
    fn test_version_display() {
        assert_eq!(Version::TxnId(42).to_string(), "txn:42");
        assert_eq!(Version::Sequence(100).to_string(), "seq:100");
        assert_eq!(Version::Counter(5).to_string(), "cnt:5");
    }

    // === Timestamp Tests ===

    #[test]
    fn test_timestamp_constructors() {
        let ts = Timestamp::from_secs(1000);
        assert_eq!(ts.as_secs(), 1000);
        assert_eq!(ts.as_millis(), 1_000_000);
        assert_eq!(ts.as_micros(), 1_000_000_000);

        let ts2 = Timestamp::from_millis(5000);
        assert_eq!(ts2.as_millis(), 5000);
    }

    #[test]
    fn test_timestamp_now() {
        let before = Timestamp::now();
        std::thread::sleep(std::time::Duration::from_millis(1));
        let after = Timestamp::now();

        assert!(after > before);
    }

    #[test]
    fn test_timestamp_duration() {
        let t1 = Timestamp::from_micros(1000);
        let t2 = Timestamp::from_micros(2000);

        let duration = t2.duration_since(t1).unwrap();
        assert_eq!(duration.as_micros(), 1000);

        // Earlier timestamp returns None
        assert!(t1.duration_since(t2).is_none());
    }

    // === RunId Tests ===

    #[test]
    fn test_run_id_creation() {
        let run_id = RunId::new("test-run-123");
        assert_eq!(run_id.as_str(), "test-run-123");
        assert!(run_id.is_valid());
    }

    #[test]
    fn test_run_id_from_string() {
        let run_id: RunId = "my-run".into();
        assert_eq!(run_id.as_str(), "my-run");

        let run_id2: RunId = String::from("another-run").into();
        assert_eq!(run_id2.as_str(), "another-run");
    }

    #[test]
    fn test_run_id_display() {
        let run_id = RunId::new("display-test");
        assert_eq!(format!("{}", run_id), "display-test");
    }
}
```

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/core/src/contract/mod.rs` | CREATE - Contract module exports |
| `crates/core/src/contract/entity_ref.rs` | CREATE - EntityRef enum and PrimitiveType |
| `crates/core/src/contract/versioned.rs` | CREATE - Versioned<T> wrapper |
| `crates/core/src/contract/version.rs` | CREATE - Version enum |
| `crates/core/src/contract/timestamp.rs` | CREATE - Timestamp type |
| `crates/core/src/contract/run_name.rs` | CREATE - RunName type |
| `crates/core/src/types.rs` | MODIFY - Update RunId docs (internal identity) |
| `crates/core/src/value.rs` | MODIFY - Add `type VersionedValue = Versioned<Value>` |
| `crates/core/src/search_types.rs` | MODIFY - Add `type DocRef = EntityRef`, `type PrimitiveKind = PrimitiveType` |
| `crates/core/src/lib.rs` | MODIFY - Export contract module prominently |
