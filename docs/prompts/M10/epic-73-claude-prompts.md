# Epic 73: Retention Policies - Implementation Prompts

**Epic Goal**: Implement user-configurable retention policies as database entries

**GitHub Issue**: [#519](https://github.com/anibjoshi/in-mem/issues/519)
**Status**: Ready to begin
**Dependencies**: Epic 72 (Recovery)
**Phase**: 4 (Retention & Compaction)

---

## NAMING CONVENTION - CRITICAL

> **NEVER use "M10" or "Strata" in the actual codebase or comments.**
>
> - "M10" is an internal milestone tracker only - do not use it in code, comments, or user-facing text
> - All existing crates refer to the database as "in-mem" - use this name consistently
> - Do not use "Strata" anywhere in the codebase
> - This applies to: code, comments, docstrings, error messages, log messages, test names
>
> **CORRECT**: `//! Retention policies for version history management`
> **WRONG**: `//! M10 Retention for Strata database`

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M10_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M10_ARCHITECTURE.md`
2. **Implementation Plan**: `docs/milestones/M10/M10_IMPLEMENTATION_PLAN.md`
3. **Epic Spec**: `docs/milestones/M10/EPIC_73_RETENTION_POLICIES.md`
4. **Prompt Header**: `docs/prompts/M10/M10_PROMPT_HEADER.md` for the 8 architectural rules

**The architecture spec is LAW.** Epic docs provide implementation details but MUST NOT contradict the architecture spec.

---

## Epic 73 Overview

### Scope
- RetentionPolicy type definition (KeepAll, KeepLast, KeepFor, Composite)
- System namespace for policy storage
- Retention policy CRUD API
- Retention policy enforcement
- HistoryTrimmed error type

### Key Rules for Epic 73

1. **Retention policies are database entries** - Versioned and transactional
2. **Stored in system namespace** - `_strata/retention_policy/{run_id_hex}`
3. **Default is KeepAll** - No data loss by default
4. **Explicit errors for trimmed history** - Never silent fallback

### Success Criteria
- [ ] `RetentionPolicy` enum with KeepAll, KeepLast, KeepFor, Composite
- [ ] System namespace `_strata/` for policies
- [ ] CRUD API: set, get, delete retention policy
- [ ] `should_retain()` enforcement logic
- [ ] `HistoryTrimmed` error type
- [ ] All tests passing

### Component Breakdown
- **Story #519**: RetentionPolicy Type Definition - FOUNDATION
- **Story #520**: System Namespace for Policies - CRITICAL
- **Story #521**: Retention Policy CRUD API - CRITICAL
- **Story #522**: Retention Policy Enforcement - CRITICAL
- **Story #523**: HistoryTrimmed Error Type - HIGH

---

## File Organization

### Directory Structure

```bash
mkdir -p crates/storage/src/retention
```

**Target structure**:
```
crates/storage/src/
├── lib.rs
├── format/
│   └── ...
├── wal/
│   └── ...
├── snapshot/
│   └── ...
├── recovery/
│   └── ...
├── retention/                # NEW
│   ├── mod.rs
│   ├── policy.rs             # RetentionPolicy types
│   └── enforcement.rs        # Enforcement logic
└── codec/
    └── ...
```

---

## Dependency Graph

```
Story #519 (Policy Type) ──────> Story #521 (CRUD API)
                                       │
Story #520 (System Namespace) ─────────┘
                                       │
                              └──> Story #522 (Enforcement)
                                       │
Story #523 (HistoryTrimmed) ───────────┘
```

**Recommended Order**: #519 (Policy Type) → #520 (System Namespace) → #521 (CRUD) → #523 (HistoryTrimmed) → #522 (Enforcement)

---

## Story #519: RetentionPolicy Type Definition

**GitHub Issue**: [#519](https://github.com/anibjoshi/in-mem/issues/519)
**Estimated Time**: 3 hours
**Dependencies**: None
**Blocks**: Story #521

### Start Story

```bash
gh issue view 519
./scripts/start-story.sh 73 519 retention-policy-type
```

### Implementation

Create `crates/storage/src/retention/policy.rs`:

```rust
//! Retention policy types
//!
//! Controls how much version history is retained per run.

use std::time::Duration;
use std::collections::HashMap;
use crate::core::PrimitiveType;

/// Retention policy for a run
///
/// Controls how much version history is retained.
/// Policies are stored as database entries and are themselves versioned.
#[derive(Debug, Clone, PartialEq)]
pub enum RetentionPolicy {
    /// Keep all versions forever (default)
    ///
    /// Safest policy. No data is ever deleted.
    KeepAll,

    /// Keep only the last N versions
    ///
    /// Versions beyond the Nth oldest become eligible for removal.
    KeepLast(usize),

    /// Keep versions newer than the specified duration
    ///
    /// Versions older than `now - duration` become eligible for removal.
    KeepFor(Duration),

    /// Different policies for different primitive types
    ///
    /// Allows fine-grained control per primitive type.
    Composite {
        default: Box<RetentionPolicy>,
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

    /// Create a Composite policy builder
    pub fn composite(default: RetentionPolicy) -> CompositeBuilder {
        CompositeBuilder {
            default: Box::new(default),
            overrides: HashMap::new(),
        }
    }

    /// Check if a version should be retained
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

            RetentionPolicy::KeepLast(n) => version_count <= *n,

            RetentionPolicy::KeepFor(duration) => {
                let cutoff = current_time.saturating_sub(duration.as_micros() as u64);
                timestamp >= cutoff
            }

            RetentionPolicy::Composite { default, overrides } => {
                if let Some(override_policy) = overrides.get(&primitive_type) {
                    override_policy.should_retain(version, timestamp, version_count, current_time, primitive_type)
                } else {
                    default.should_retain(version, timestamp, version_count, current_time, primitive_type)
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

                let default_bytes = default.to_bytes();
                bytes.extend_from_slice(&(default_bytes.len() as u32).to_le_bytes());
                bytes.extend_from_slice(&default_bytes);

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

                let default_len = u32::from_le_bytes(
                    bytes[cursor..cursor + 4].try_into().unwrap()
                ) as usize;
                cursor += 4;

                let default = RetentionPolicy::from_bytes(&bytes[cursor..cursor + default_len])?;
                cursor += default_len;

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
    pub fn with_override(mut self, ptype: PrimitiveType, policy: RetentionPolicy) -> Self {
        self.overrides.insert(ptype, Box::new(policy));
        self
    }

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

### Complete Story

```bash
./scripts/complete-story.sh 519
```

---

## Story #520: System Namespace for Policies

**GitHub Issue**: [#520](https://github.com/anibjoshi/in-mem/issues/520)
**Estimated Time**: 2 hours
**Dependencies**: Story #519
**Blocks**: Story #521

### Start Story

```bash
gh issue view 520
./scripts/start-story.sh 73 520 system-namespace
```

### Implementation

Add to `crates/storage/src/retention/mod.rs`:

```rust
//! Retention policy storage in system namespace

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

### Complete Story

```bash
./scripts/complete-story.sh 520
```

---

## Story #521: Retention Policy CRUD API

**GitHub Issue**: [#521](https://github.com/anibjoshi/in-mem/issues/521)
**Estimated Time**: 2 hours
**Dependencies**: Stories #519, #520
**Blocks**: Story #522

### Start Story

```bash
gh issue view 521
./scripts/start-story.sh 73 521 retention-crud
```

### Implementation

Add to `crates/storage/src/database.rs`:

```rust
impl Database {
    /// Set retention policy for a run
    ///
    /// The policy controls which versions are retained during compaction.
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

### Complete Story

```bash
./scripts/complete-story.sh 521
```

---

## Story #522: Retention Policy Enforcement

**GitHub Issue**: [#522](https://github.com/anibjoshi/in-mem/issues/522)
**Estimated Time**: 3 hours
**Dependencies**: Stories #521, #523
**Blocks**: None

### Start Story

```bash
gh issue view 522
./scripts/start-story.sh 73 522 retention-enforcement
```

### Implementation

Create `crates/storage/src/retention/enforcement.rs`:

```rust
//! Retention policy enforcement

use std::sync::Arc;
use crate::core::{EntityRef, PrimitiveType};
use super::policy::RetentionPolicy;
use super::RetentionPolicyStore;

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
    /// Returns (EntityRef, version) pairs eligible for removal.
    pub fn compute_removable_versions(
        &self,
        run_id: &[u8; 16],
        version_index: &dyn VersionIndex,
        current_time: u64,
    ) -> Result<Vec<(EntityRef, u64)>, StorageError> {
        let policy = self.policy_store.effective_policy(run_id)?;

        let mut removable = Vec::new();

        for (entity_ref, versions) in version_index.entities_for_run(run_id) {
            let version_count = versions.len();

            for (idx, (version, timestamp)) in versions.iter().enumerate() {
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
            return Ok(None);
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

        Ok(versions.first().map(|(v, _)| *v))
    }
}

/// Version index interface (implemented by engine)
pub trait VersionIndex {
    fn entities_for_run(&self, run_id: &[u8; 16]) -> Box<dyn Iterator<Item = (EntityRef, Vec<(u64, u64)>)> + '_>;
}
```

### Acceptance Criteria

- [ ] `compute_removable_versions()` returns versions eligible for removal
- [ ] `is_retained()` checks single version retention
- [ ] `earliest_retained_version()` for error messages
- [ ] Respects Composite policy overrides
- [ ] Never removes versions that should be retained

### Complete Story

```bash
./scripts/complete-story.sh 522
```

---

## Story #523: HistoryTrimmed Error Type

**GitHub Issue**: [#523](https://github.com/anibjoshi/in-mem/issues/523)
**Estimated Time**: 2 hours
**Dependencies**: Story #519
**Blocks**: Story #522

### Start Story

```bash
gh issue view 523
./scripts/start-story.sh 73 523 history-trimmed-error
```

### Implementation

Add to `crates/core/src/error.rs`:

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

/// Add to StorageError enum
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

    pub fn is_history_trimmed(&self) -> bool {
        matches!(self, StorageError::HistoryTrimmed(_))
    }

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
        let policy = self.effective_retention_policy(run_id)?;
        let version_history = self.engine.kv_version_history(run_id, key)?;

        let enforcer = self.retention_enforcer();
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

### Complete Story

```bash
./scripts/complete-story.sh 523
```

---

## Epic 73 Completion Checklist

### 1. Final Validation

```bash
~/.cargo/bin/cargo build --workspace
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### 2. Verify Deliverables

- [ ] `RetentionPolicy` enum with all variants
- [ ] System namespace `_strata/`
- [ ] `RetentionPolicyStore` for storage
- [ ] CRUD API on Database
- [ ] `RetentionEnforcer` for enforcement
- [ ] `HistoryTrimmed` error type

### 3. Run Epic-End Validation

See `docs/prompts/EPIC_END_VALIDATION.md`

### 4. Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-73-retention-policies -m "Epic 73: Retention Policies complete

Delivered:
- RetentionPolicy type (KeepAll, KeepLast, KeepFor, Composite)
- System namespace for policy storage
- Retention policy CRUD API
- Retention policy enforcement
- HistoryTrimmed error type

Stories: #519, #520, #521, #522, #523
"
git push origin develop
gh issue close 519 --comment "Epic 73: Retention Policies - COMPLETE"
```

---

## Summary

Epic 73 establishes the retention policy system:

- **RetentionPolicy Type** defines retention semantics
- **System Namespace** provides isolated storage
- **CRUD API** exposes user-facing functionality
- **Enforcement Logic** determines removable versions
- **HistoryTrimmed Error** provides explicit feedback

This foundation enables Epic 74 (Compaction) to actually remove versions.
