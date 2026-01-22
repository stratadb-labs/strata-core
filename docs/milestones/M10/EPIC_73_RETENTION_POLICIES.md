# Epic 73: Retention Policies

**Goal**: Implement user-configurable retention policies as database entries

**Dependencies**: Epic 72 (Recovery)

---

## Scope

- RetentionPolicy type definition (KeepAll, KeepLast, KeepFor, Composite)
- System namespace for policy storage
- Retention policy CRUD API
- Retention policy enforcement
- HistoryTrimmed error type

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #519 | RetentionPolicy Type Definition | FOUNDATION |
| #520 | System Namespace for Policies | CRITICAL |
| #521 | Retention Policy CRUD API | CRITICAL |
| #522 | Retention Policy Enforcement | CRITICAL |
| #523 | HistoryTrimmed Error Type | HIGH |

---

## Story #519: RetentionPolicy Type Definition

**File**: `crates/storage/src/retention/policy.rs` (NEW)

**Deliverable**: Retention policy enum with all variants

### Design

Retention policies control how much history is retained:
- **KeepAll**: Never delete anything (default, safest)
- **KeepLast(N)**: Keep only the last N versions
- **KeepFor(Duration)**: Keep versions newer than Duration
- **Composite**: Different policies for different primitive types

### Implementation

```rust
use std::time::Duration;
use std::collections::HashMap;

/// Retention policy for a run
///
/// Controls how much version history is retained.
/// Policies are stored as database entries and are themselves versioned.
#[derive(Debug, Clone, PartialEq)]
pub enum RetentionPolicy {
    /// Keep all versions forever (default)
    ///
    /// This is the safest policy. No data is ever deleted.
    /// Use this when you need complete audit trails or debugging capability.
    KeepAll,

    /// Keep only the last N versions
    ///
    /// When a new version is created, versions beyond the Nth oldest
    /// become eligible for removal during compaction.
    ///
    /// Example: KeepLast(10) keeps the 10 most recent versions.
    KeepLast(usize),

    /// Keep versions newer than the specified duration
    ///
    /// Versions older than `now - duration` become eligible for
    /// removal during compaction.
    ///
    /// Example: KeepFor(Duration::from_secs(86400 * 7)) keeps 7 days.
    KeepFor(Duration),

    /// Different policies for different primitive types
    ///
    /// Allows fine-grained control, e.g., keep all events but only
    /// the last 10 KV versions.
    Composite {
        /// Default policy for primitives not explicitly listed
        default: Box<RetentionPolicy>,

        /// Override policies per primitive type
        overrides: HashMap<PrimitiveType, Box<RetentionPolicy>>,
    },
}

impl RetentionPolicy {
    /// Create a KeepAll policy (recommended default)
    pub fn keep_all() -> Self {
        RetentionPolicy::KeepAll
    }

    /// Create a KeepLast policy
    pub fn keep_last(n: usize) -> Self {
        assert!(n > 0, "KeepLast(n) requires n > 0");
        RetentionPolicy::KeepLast(n)
    }

    /// Create a KeepFor policy
    pub fn keep_for(duration: Duration) -> Self {
        assert!(!duration.is_zero(), "KeepFor requires non-zero duration");
        RetentionPolicy::KeepFor(duration)
    }

    /// Create a Composite policy
    pub fn composite(default: RetentionPolicy) -> CompositeBuilder {
        CompositeBuilder {
            default: Box::new(default),
            overrides: HashMap::new(),
        }
    }

    /// Check if a version should be retained
    ///
    /// Returns true if the version should be kept, false if eligible for removal.
    pub fn should_retain(
        &self,
        version: u64,
        timestamp: u64,
        version_count: usize,
        current_time: u64,
        primitive_type: PrimitiveType,
    ) -> bool {
        match self {
            RetentionPolicy::KeepAll => true,

            RetentionPolicy::KeepLast(n) => {
                // Keep if within the last N versions
                version_count <= *n
            }

            RetentionPolicy::KeepFor(duration) => {
                // Keep if timestamp is within duration from now
                let cutoff = current_time.saturating_sub(duration.as_micros() as u64);
                timestamp >= cutoff
            }

            RetentionPolicy::Composite { default, overrides } => {
                if let Some(override_policy) = overrides.get(&primitive_type) {
                    override_policy.should_retain(
                        version,
                        timestamp,
                        version_count,
                        current_time,
                        primitive_type,
                    )
                } else {
                    default.should_retain(
                        version,
                        timestamp,
                        version_count,
                        current_time,
                        primitive_type,
                    )
                }
            }
        }
    }

    /// Serialize policy to bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        match self {
            RetentionPolicy::KeepAll => {
                bytes.push(0x01);
            }
            RetentionPolicy::KeepLast(n) => {
                bytes.push(0x02);
                bytes.extend_from_slice(&(*n as u64).to_le_bytes());
            }
            RetentionPolicy::KeepFor(duration) => {
                bytes.push(0x03);
                bytes.extend_from_slice(&duration.as_micros().to_le_bytes());
            }
            RetentionPolicy::Composite { default, overrides } => {
                bytes.push(0x04);

                // Serialize default
                let default_bytes = default.to_bytes();
                bytes.extend_from_slice(&(default_bytes.len() as u32).to_le_bytes());
                bytes.extend_from_slice(&default_bytes);

                // Serialize overrides
                bytes.extend_from_slice(&(overrides.len() as u32).to_le_bytes());
                for (ptype, policy) in overrides {
                    bytes.push(ptype.to_byte());
                    let policy_bytes = policy.to_bytes();
                    bytes.extend_from_slice(&(policy_bytes.len() as u32).to_le_bytes());
                    bytes.extend_from_slice(&policy_bytes);
                }
            }
        }

        bytes
    }

    /// Deserialize policy from bytes
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, RetentionPolicyError> {
        if bytes.is_empty() {
            return Err(RetentionPolicyError::Empty);
        }

        match bytes[0] {
            0x01 => Ok(RetentionPolicy::KeepAll),

            0x02 => {
                if bytes.len() < 9 {
                    return Err(RetentionPolicyError::InsufficientData);
                }
                let n = u64::from_le_bytes(bytes[1..9].try_into().unwrap()) as usize;
                Ok(RetentionPolicy::KeepLast(n))
            }

            0x03 => {
                if bytes.len() < 17 {
                    return Err(RetentionPolicyError::InsufficientData);
                }
                let micros = u128::from_le_bytes(bytes[1..17].try_into().unwrap());
                Ok(RetentionPolicy::KeepFor(Duration::from_micros(micros as u64)))
            }

            0x04 => {
                let mut cursor = 1;

                // Read default
                let default_len = u32::from_le_bytes(
                    bytes[cursor..cursor + 4].try_into().unwrap()
                ) as usize;
                cursor += 4;

                let default = RetentionPolicy::from_bytes(&bytes[cursor..cursor + default_len])?;
                cursor += default_len;

                // Read overrides
                let override_count = u32::from_le_bytes(
                    bytes[cursor..cursor + 4].try_into().unwrap()
                ) as usize;
                cursor += 4;

                let mut overrides = HashMap::new();
                for _ in 0..override_count {
                    let ptype = PrimitiveType::from_byte(bytes[cursor])
                        .ok_or(RetentionPolicyError::InvalidPrimitiveType)?;
                    cursor += 1;

                    let policy_len = u32::from_le_bytes(
                        bytes[cursor..cursor + 4].try_into().unwrap()
                    ) as usize;
                    cursor += 4;

                    let policy = RetentionPolicy::from_bytes(&bytes[cursor..cursor + policy_len])?;
                    cursor += policy_len;

                    overrides.insert(ptype, Box::new(policy));
                }

                Ok(RetentionPolicy::Composite {
                    default: Box::new(default),
                    overrides,
                })
            }

            tag => Err(RetentionPolicyError::InvalidTag(tag)),
        }
    }
}

impl Default for RetentionPolicy {
    /// Default is KeepAll - no data loss by default
    fn default() -> Self {
        RetentionPolicy::KeepAll
    }
}

/// Builder for composite policies
pub struct CompositeBuilder {
    default: Box<RetentionPolicy>,
    overrides: HashMap<PrimitiveType, Box<RetentionPolicy>>,
}

impl CompositeBuilder {
    /// Set policy for a specific primitive type
    pub fn with_override(mut self, ptype: PrimitiveType, policy: RetentionPolicy) -> Self {
        self.overrides.insert(ptype, Box::new(policy));
        self
    }

    /// Build the composite policy
    pub fn build(self) -> RetentionPolicy {
        RetentionPolicy::Composite {
            default: self.default,
            overrides: self.overrides,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RetentionPolicyError {
    #[error("Empty policy data")]
    Empty,

    #[error("Insufficient data")]
    InsufficientData,

    #[error("Invalid tag: {0}")]
    InvalidTag(u8),

    #[error("Invalid primitive type")]
    InvalidPrimitiveType,
}
```

### Acceptance Criteria

- [ ] `RetentionPolicy` enum with KeepAll, KeepLast, KeepFor, Composite
- [ ] Constructors: `keep_all()`, `keep_last(n)`, `keep_for(duration)`
- [ ] `composite()` builder pattern
- [ ] `should_retain()` for retention checking
- [ ] Serialization/deserialization
- [ ] Default is KeepAll (no data loss)

---

## Story #520: System Namespace for Policies

**File**: `crates/storage/src/retention/mod.rs`

**Deliverable**: System namespace for storing retention policies

### Design

Retention policies are stored in the system namespace (`_strata/`):

> **Storage Location**: Retention policies are stored in the system namespace (`_strata/`) as versioned entities, not in user-visible KV space.
>
> This design:
> - Prevents users from accidentally deleting retention policies
> - Keeps system metadata separate from user data
> - Allows system namespace to have different access controls (future)
> - Makes policies discoverable via standard introspection

### Implementation

```rust
/// System namespace for internal storage
pub mod system_namespace {
    /// Prefix for all system keys
    pub const PREFIX: &str = "_strata/";

    /// Key for retention policy
    pub const RETENTION_POLICY: &str = "_strata/retention_policy";

    /// Check if a key is in the system namespace
    pub fn is_system_key(key: &str) -> bool {
        key.starts_with(PREFIX)
    }

    /// Generate retention policy key for a run
    pub fn retention_policy_key(run_id: &[u8; 16]) -> String {
        format!("_strata/retention_policy/{}", hex::encode(run_id))
    }
}

/// Retention policy storage
pub struct RetentionPolicyStore {
    /// Database reference
    db: Arc<Database>,
}

impl RetentionPolicyStore {
    pub fn new(db: Arc<Database>) -> Self {
        RetentionPolicyStore { db }
    }

    /// Get retention policy for a run
    pub fn get_policy(&self, run_id: &[u8; 16]) -> Result<Option<Versioned<RetentionPolicy>>, StorageError> {
        let key = system_namespace::retention_policy_key(run_id);

        match self.db.system_kv_get(&key)? {
            Some(versioned) => {
                let policy = RetentionPolicy::from_bytes(&versioned.value)
                    .map_err(|e| StorageError::PolicyError(e.to_string()))?;

                Ok(Some(Versioned {
                    value: policy,
                    version: versioned.version,
                    timestamp: versioned.timestamp,
                    ttl: None,
                }))
            }
            None => Ok(None),
        }
    }

    /// Set retention policy for a run
    pub fn set_policy(
        &self,
        run_id: &[u8; 16],
        policy: RetentionPolicy,
    ) -> Result<Version, StorageError> {
        let key = system_namespace::retention_policy_key(run_id);
        let value = policy.to_bytes();

        self.db.system_kv_put(&key, &value)
    }

    /// Delete retention policy for a run (revert to default)
    pub fn delete_policy(&self, run_id: &[u8; 16]) -> Result<(), StorageError> {
        let key = system_namespace::retention_policy_key(run_id);
        self.db.system_kv_delete(&key)
    }

    /// Get effective policy (stored or default)
    pub fn effective_policy(&self, run_id: &[u8; 16]) -> Result<RetentionPolicy, StorageError> {
        match self.get_policy(run_id)? {
            Some(versioned) => Ok(versioned.value),
            None => Ok(RetentionPolicy::default()),
        }
    }
}
```

### Acceptance Criteria

- [ ] System namespace prefix: `_strata/`
- [ ] Retention policy key format: `_strata/retention_policy/{run_id_hex}`
- [ ] `is_system_key()` check
- [ ] Policies stored as versioned KV entries
- [ ] System namespace hidden from user queries

---

## Story #521: Retention Policy CRUD API

**File**: `crates/storage/src/retention/mod.rs`

**Deliverable**: Public API for managing retention policies

### Implementation

```rust
impl Database {
    /// Set retention policy for a run
    ///
    /// The policy controls which versions are retained during compaction.
    /// Policy changes are versioned and transactional.
    ///
    /// # Example
    /// ```
    /// // Keep only the last 10 versions
    /// db.set_retention_policy(run_id, RetentionPolicy::keep_last(10))?;
    ///
    /// // Keep versions from the last 7 days
    /// db.set_retention_policy(
    ///     run_id,
    ///     RetentionPolicy::keep_for(Duration::from_secs(7 * 24 * 3600))
    /// )?;
    ///
    /// // Different policies per primitive
    /// db.set_retention_policy(
    ///     run_id,
    ///     RetentionPolicy::composite(RetentionPolicy::keep_all())
    ///         .with_override(PrimitiveType::Kv, RetentionPolicy::keep_last(100))
    ///         .with_override(PrimitiveType::Event, RetentionPolicy::keep_for(Duration::from_secs(86400)))
    ///         .build()
    /// )?;
    /// ```
    pub fn set_retention_policy(
        &self,
        run_id: RunId,
        policy: RetentionPolicy,
    ) -> Result<Version, StorageError> {
        // Validate run exists
        if !self.run_exists(run_id)? {
            return Err(StorageError::RunNotFound { run_id });
        }

        // Store policy
        self.retention_store.set_policy(run_id.as_bytes(), policy)
    }

    /// Get retention policy for a run
    ///
    /// Returns the configured policy with version information,
    /// or None if using the default (KeepAll).
    pub fn get_retention_policy(
        &self,
        run_id: RunId,
    ) -> Result<Option<Versioned<RetentionPolicy>>, StorageError> {
        self.retention_store.get_policy(run_id.as_bytes())
    }

    /// Delete retention policy for a run
    ///
    /// Reverts to the default policy (KeepAll).
    pub fn delete_retention_policy(&self, run_id: RunId) -> Result<(), StorageError> {
        self.retention_store.delete_policy(run_id.as_bytes())
    }

    /// Get effective retention policy (stored or default)
    pub fn effective_retention_policy(
        &self,
        run_id: RunId,
    ) -> Result<RetentionPolicy, StorageError> {
        self.retention_store.effective_policy(run_id.as_bytes())
    }
}
```

### Acceptance Criteria

- [ ] `set_retention_policy(run_id, policy)` returns Version
- [ ] `get_retention_policy(run_id)` returns Option<Versioned<RetentionPolicy>>
- [ ] `delete_retention_policy(run_id)` reverts to default
- [ ] `effective_retention_policy(run_id)` returns stored or default
- [ ] Validates run exists before setting policy
- [ ] Policies are versioned and transactional

---

## Story #522: Retention Policy Enforcement

**File**: `crates/storage/src/retention/enforcement.rs` (NEW)

**Deliverable**: Enforcement logic for retention policies

### Implementation

```rust
/// Retention enforcer
pub struct RetentionEnforcer {
    policy_store: Arc<RetentionPolicyStore>,
}

impl RetentionEnforcer {
    pub fn new(policy_store: Arc<RetentionPolicyStore>) -> Self {
        RetentionEnforcer { policy_store }
    }

    /// Compute which versions should be removed for a run
    ///
    /// Returns a list of (EntityRef, Version) pairs that are eligible
    /// for removal according to the retention policy.
    pub fn compute_removable_versions(
        &self,
        run_id: &[u8; 16],
        version_index: &VersionIndex,
        current_time: u64,
    ) -> Result<Vec<(EntityRef, u64)>, StorageError> {
        let policy = self.policy_store.effective_policy(run_id)?;

        let mut removable = Vec::new();

        // Iterate over all entities in the run
        for (entity_ref, versions) in version_index.entities_for_run(run_id) {
            let version_count = versions.len();

            for (idx, (version, timestamp)) in versions.iter().enumerate() {
                // Version index is from oldest to newest
                let versions_remaining = version_count - idx;

                if !policy.should_retain(
                    *version,
                    *timestamp,
                    versions_remaining,
                    current_time,
                    entity_ref.primitive_type(),
                ) {
                    removable.push((entity_ref.clone(), *version));
                }
            }
        }

        Ok(removable)
    }

    /// Check if a specific version is retained
    pub fn is_retained(
        &self,
        run_id: &[u8; 16],
        entity_ref: &EntityRef,
        version: u64,
        timestamp: u64,
        version_count: usize,
        current_time: u64,
    ) -> Result<bool, StorageError> {
        let policy = self.policy_store.effective_policy(run_id)?;

        Ok(policy.should_retain(
            version,
            timestamp,
            version_count,
            current_time,
            entity_ref.primitive_type(),
        ))
    }

    /// Get the earliest retained version for an entity
    ///
    /// Returns None if all versions are retained (KeepAll policy).
    pub fn earliest_retained_version(
        &self,
        run_id: &[u8; 16],
        entity_ref: &EntityRef,
        versions: &[(u64, u64)], // (version, timestamp) pairs, oldest first
        current_time: u64,
    ) -> Result<Option<u64>, StorageError> {
        let policy = self.policy_store.effective_policy(run_id)?;

        if matches!(policy, RetentionPolicy::KeepAll) {
            return Ok(None); // All retained
        }

        let version_count = versions.len();

        for (idx, (version, timestamp)) in versions.iter().enumerate() {
            let versions_remaining = version_count - idx;

            if policy.should_retain(
                *version,
                *timestamp,
                versions_remaining,
                current_time,
                entity_ref.primitive_type(),
            ) {
                return Ok(Some(*version));
            }
        }

        // All versions trimmed (shouldn't happen with valid data)
        Ok(versions.first().map(|(v, _)| *v))
    }
}

/// Version index interface (implemented by engine)
pub trait VersionIndex {
    /// Get all entities for a run with their version histories
    fn entities_for_run(&self, run_id: &[u8; 16]) -> impl Iterator<Item = (EntityRef, Vec<(u64, u64)>)>;
}
```

### Acceptance Criteria

- [ ] `compute_removable_versions()` returns versions eligible for removal
- [ ] `is_retained()` checks single version retention
- [ ] `earliest_retained_version()` for error messages
- [ ] Respects Composite policy overrides
- [ ] Never removes versions that should be retained

---

## Story #523: HistoryTrimmed Error Type

**File**: `crates/core/src/error.rs`

**Deliverable**: Explicit error for trimmed history access

### Design

When a user requests a version that has been trimmed by retention policy, we must:
1. Return an explicit error (not silent fallback)
2. Include helpful metadata (requested version, earliest retained)
3. Suggest alternatives

### Implementation

```rust
/// Error returned when accessing trimmed history
#[derive(Debug, Clone)]
pub struct HistoryTrimmed {
    /// The version that was requested
    pub requested_version: u64,

    /// The earliest version still retained
    pub earliest_retained: u64,

    /// Entity reference
    pub entity_ref: EntityRef,

    /// Retention policy in effect
    pub policy_summary: String,
}

impl std::fmt::Display for HistoryTrimmed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Version {} of {} has been trimmed. Earliest retained: {}. Policy: {}",
            self.requested_version,
            self.entity_ref,
            self.earliest_retained,
            self.policy_summary,
        )
    }
}

impl std::error::Error for HistoryTrimmed {}

/// Storage error variants
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    // ... existing variants ...

    /// Requested version has been trimmed by retention policy
    #[error("{0}")]
    HistoryTrimmed(HistoryTrimmed),
}

impl StorageError {
    /// Create a HistoryTrimmed error
    pub fn history_trimmed(
        requested_version: u64,
        earliest_retained: u64,
        entity_ref: EntityRef,
        policy: &RetentionPolicy,
    ) -> Self {
        let policy_summary = match policy {
            RetentionPolicy::KeepAll => "KeepAll".to_string(),
            RetentionPolicy::KeepLast(n) => format!("KeepLast({})", n),
            RetentionPolicy::KeepFor(d) => format!("KeepFor({:?})", d),
            RetentionPolicy::Composite { .. } => "Composite".to_string(),
        };

        StorageError::HistoryTrimmed(HistoryTrimmed {
            requested_version,
            earliest_retained,
            entity_ref,
            policy_summary,
        })
    }

    /// Check if this error is a HistoryTrimmed error
    pub fn is_history_trimmed(&self) -> bool {
        matches!(self, StorageError::HistoryTrimmed(_))
    }

    /// Get HistoryTrimmed details if applicable
    pub fn as_history_trimmed(&self) -> Option<&HistoryTrimmed> {
        match self {
            StorageError::HistoryTrimmed(h) => Some(h),
            _ => None,
        }
    }
}
```

### Integration

```rust
impl Database {
    /// Get a specific version of a KV entry
    pub fn kv_get_version(
        &self,
        run_id: RunId,
        key: &str,
        version: u64,
    ) -> Result<Option<Versioned<Vec<u8>>>, StorageError> {
        let entity_ref = EntityRef::kv(run_id, key.to_string());

        // Check if version exists
        if let Some(value) = self.engine.kv_get_version(run_id, key, version)? {
            return Ok(Some(value));
        }

        // Version not found - check if it was trimmed
        let enforcer = self.retention_enforcer();
        let policy = enforcer.policy_store.effective_policy(run_id.as_bytes())?;

        let version_history = self.engine.kv_version_history(run_id, key)?;
        if let Some(earliest) = enforcer.earliest_retained_version(
            run_id.as_bytes(),
            &entity_ref,
            &version_history,
            Timestamp::now().as_micros(),
        )? {
            if version < earliest {
                return Err(StorageError::history_trimmed(
                    version,
                    earliest,
                    entity_ref,
                    &policy,
                ));
            }
        }

        // Version never existed
        Ok(None)
    }
}
```

### Acceptance Criteria

- [ ] `HistoryTrimmed` error with requested, earliest_retained, entity_ref, policy_summary
- [ ] `StorageError::HistoryTrimmed` variant
- [ ] `is_history_trimmed()` helper
- [ ] `as_history_trimmed()` for accessing details
- [ ] No silent fallback to nearest version
- [ ] Clear, actionable error message

---

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retention_policy_keep_all() {
        let policy = RetentionPolicy::keep_all();

        // All versions retained
        for i in 0..100 {
            assert!(policy.should_retain(i, 0, 100, 1000000, PrimitiveType::Kv));
        }
    }

    #[test]
    fn test_retention_policy_keep_last() {
        let policy = RetentionPolicy::keep_last(5);

        // Only last 5 retained
        assert!(!policy.should_retain(1, 0, 10, 0, PrimitiveType::Kv)); // 10th oldest
        assert!(!policy.should_retain(5, 0, 6, 0, PrimitiveType::Kv));  // 6th oldest
        assert!(policy.should_retain(6, 0, 5, 0, PrimitiveType::Kv));   // 5th oldest
        assert!(policy.should_retain(10, 0, 1, 0, PrimitiveType::Kv));  // newest
    }

    #[test]
    fn test_retention_policy_keep_for() {
        let policy = RetentionPolicy::keep_for(Duration::from_secs(3600)); // 1 hour
        let now = 1000000000; // Current time in micros

        // Within duration - retained
        let recent = now - 1800000000; // 30 minutes ago
        assert!(policy.should_retain(1, recent, 1, now, PrimitiveType::Kv));

        // Outside duration - not retained
        let old = now - 7200000000; // 2 hours ago
        assert!(!policy.should_retain(1, old, 1, now, PrimitiveType::Kv));
    }

    #[test]
    fn test_retention_policy_composite() {
        let policy = RetentionPolicy::composite(RetentionPolicy::keep_all())
            .with_override(PrimitiveType::Kv, RetentionPolicy::keep_last(10))
            .with_override(PrimitiveType::Event, RetentionPolicy::keep_last(1000))
            .build();

        // KV uses KeepLast(10)
        assert!(!policy.should_retain(1, 0, 20, 0, PrimitiveType::Kv));
        assert!(policy.should_retain(1, 0, 5, 0, PrimitiveType::Kv));

        // Event uses KeepLast(1000)
        assert!(policy.should_retain(1, 0, 500, 0, PrimitiveType::Event));
        assert!(!policy.should_retain(1, 0, 1500, 0, PrimitiveType::Event));

        // State uses default (KeepAll)
        assert!(policy.should_retain(1, 0, 10000, 0, PrimitiveType::State));
    }

    #[test]
    fn test_retention_policy_serialization() {
        let policies = vec![
            RetentionPolicy::keep_all(),
            RetentionPolicy::keep_last(42),
            RetentionPolicy::keep_for(Duration::from_secs(3600)),
            RetentionPolicy::composite(RetentionPolicy::keep_last(10))
                .with_override(PrimitiveType::Event, RetentionPolicy::keep_all())
                .build(),
        ];

        for policy in policies {
            let bytes = policy.to_bytes();
            let parsed = RetentionPolicy::from_bytes(&bytes).unwrap();
            assert_eq!(policy, parsed);
        }
    }

    #[test]
    fn test_history_trimmed_error() {
        let entity_ref = EntityRef::kv(RunId::new(), "test-key".to_string());

        let error = StorageError::history_trimmed(
            5,
            10,
            entity_ref.clone(),
            &RetentionPolicy::keep_last(10),
        );

        assert!(error.is_history_trimmed());

        let details = error.as_history_trimmed().unwrap();
        assert_eq!(details.requested_version, 5);
        assert_eq!(details.earliest_retained, 10);
        assert!(details.policy_summary.contains("KeepLast"));
    }

    #[test]
    fn test_system_namespace() {
        assert!(system_namespace::is_system_key("_strata/retention_policy"));
        assert!(system_namespace::is_system_key("_strata/anything"));
        assert!(!system_namespace::is_system_key("user-key"));
        assert!(!system_namespace::is_system_key("strata/not-system"));
    }
}
```

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/storage/src/retention/mod.rs` | CREATE - Retention module |
| `crates/storage/src/retention/policy.rs` | CREATE - RetentionPolicy types |
| `crates/storage/src/retention/enforcement.rs` | CREATE - Enforcement logic |
| `crates/core/src/error.rs` | MODIFY - Add HistoryTrimmed error |
