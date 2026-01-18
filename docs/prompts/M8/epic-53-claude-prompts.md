# Epic 53: Collection Management - Implementation Prompts

**Epic Goal**: Implement collection CRUD operations

**GitHub Issue**: [#391](https://github.com/anibjoshi/in-mem/issues/391)
**Status**: Ready after Epic 51
**Dependencies**: Epic 51 (Vector Heap)

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M8_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

### IMPORTANT: Naming Convention

**Do NOT use "M8" or "m8" in the codebase or comments.** M8 is an internal milestone indicator only. In code, use "Vector" prefix instead:
- Module names: `vector`, `collection`, `store`
- Type names: `CollectionId`, `CollectionInfo`, `VectorStore`
- Test names: `test_collection_*`, `test_vector_*`, not `test_m8_*`
- Comments: "Vector collection" not "M8 collection"

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M8_ARCHITECTURE.md`
2. **Epic Spec**: `docs/milestones/M8/EPIC_53_COLLECTION_MANAGEMENT.md`
3. **Prompt Header**: `docs/prompts/M8/M8_PROMPT_HEADER.md` for the 7 architectural rules

---

## Epic 53 Overview

### Scope
- CollectionInfo and CollectionId types
- create_collection() with config validation
- delete_collection() with cascade deletion
- list_collections() and get_collection() for discovery
- Collection config persistence in KV store

### Key Rules

- **Rule 2**: Collections are scoped to RunId. Different runs cannot see each other's collections.

### Component Breakdown
- **Story #410**: CollectionInfo and CollectionId Types - FOUNDATION
- **Story #411**: create_collection() Implementation - CRITICAL
- **Story #412**: delete_collection() Implementation - CRITICAL
- **Story #413**: list_collections() and get_collection() - HIGH
- **Story #414**: Collection Config Persistence - HIGH

---

## Story #410: CollectionInfo and CollectionId Types

**GitHub Issue**: [#410](https://github.com/anibjoshi/in-mem/issues/410)
**Estimated Time**: 1 hour
**Dependencies**: Epic 50
**Blocks**: #411, #412, #413

### Start Story

```bash
gh issue view 410
./scripts/start-story.sh 53 410 collection-types
```

### Implementation

Create `crates/primitives/src/vector/collection.rs`:

```rust
//! Collection management types and validation

use crate::core::RunId;
use crate::vector::{VectorConfig, VectorError, VectorResult};

/// Unique identifier for a collection within a run
///
/// Collections are scoped to RunId per Rule 2 (Collections Per RunId).
/// Different runs cannot see each other's collections.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CollectionId {
    pub run_id: RunId,
    pub name: String,
}

impl CollectionId {
    pub fn new(run_id: RunId, name: impl Into<String>) -> Self {
        CollectionId {
            run_id,
            name: name.into(),
        }
    }

    /// Create a string key for internal storage lookups
    pub fn to_key_string(&self) -> String {
        format!("{}:{}", self.run_id, self.name)
    }
}

/// Collection metadata returned by list/get operations
#[derive(Debug, Clone)]
pub struct CollectionInfo {
    /// Collection name (unique within run)
    pub name: String,

    /// Immutable configuration
    pub config: VectorConfig,

    /// Current vector count
    pub count: usize,

    /// Creation timestamp (microseconds since epoch)
    pub created_at: u64,
}

impl CollectionInfo {
    pub fn new(name: String, config: VectorConfig, count: usize, created_at: u64) -> Self {
        CollectionInfo {
            name,
            config,
            count,
            created_at,
        }
    }
}

/// Validate collection name
///
/// Rules:
/// - Not empty
/// - Max 256 characters
/// - Only alphanumeric, underscore, hyphen
/// - Cannot start with underscore (reserved for system)
pub fn validate_collection_name(name: &str) -> VectorResult<()> {
    if name.is_empty() {
        return Err(VectorError::InvalidCollectionName {
            name: name.to_string(),
            reason: "Collection name cannot be empty".to_string(),
        });
    }

    if name.len() > 256 {
        return Err(VectorError::InvalidCollectionName {
            name: name.to_string(),
            reason: "Collection name too long (max 256 characters)".to_string(),
        });
    }

    if name.starts_with('_') {
        return Err(VectorError::InvalidCollectionName {
            name: name.to_string(),
            reason: "Collection name cannot start with underscore (reserved)".to_string(),
        });
    }

    for c in name.chars() {
        if !c.is_alphanumeric() && c != '_' && c != '-' {
            return Err(VectorError::InvalidCollectionName {
                name: name.to_string(),
                reason: format!("Invalid character '{}' (only alphanumeric, _, - allowed)", c),
            });
        }
    }

    Ok(())
}

/// Validate vector key
pub fn validate_vector_key(key: &str) -> VectorResult<()> {
    if key.is_empty() {
        return Err(VectorError::InvalidKey {
            key: key.to_string(),
            reason: "Key cannot be empty".to_string(),
        });
    }

    if key.len() > 1024 {
        return Err(VectorError::InvalidKey {
            key: key.to_string(),
            reason: "Key too long (max 1024 characters)".to_string(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::DistanceMetric;

    #[test]
    fn test_collection_id_to_key() {
        let id = CollectionId::new(RunId::new(), "embeddings");
        let key = id.to_key_string();
        assert!(key.contains("embeddings"));
        assert!(key.contains(":"));
    }

    #[test]
    fn test_validate_collection_name_valid() {
        assert!(validate_collection_name("embeddings").is_ok());
        assert!(validate_collection_name("my-collection").is_ok());
        assert!(validate_collection_name("test_123").is_ok());
        assert!(validate_collection_name("A").is_ok());
    }

    #[test]
    fn test_validate_collection_name_invalid() {
        assert!(validate_collection_name("").is_err());
        assert!(validate_collection_name("_reserved").is_err());
        assert!(validate_collection_name("has space").is_err());
        assert!(validate_collection_name("has.dot").is_err());

        let long_name = "a".repeat(257);
        assert!(validate_collection_name(&long_name).is_err());
    }

    #[test]
    fn test_validate_vector_key() {
        assert!(validate_vector_key("doc_123").is_ok());
        assert!(validate_vector_key("").is_err());

        let long_key = "a".repeat(1025);
        assert!(validate_vector_key(&long_key).is_err());
    }
}
```

### Acceptance Criteria

- [ ] CollectionId with run_id and name
- [ ] `to_key_string()` for internal storage
- [ ] CollectionInfo with name, config, count, timestamp
- [ ] validate_collection_name() with all rules
- [ ] validate_vector_key() with length limits

### Complete Story

```bash
~/.cargo/bin/cargo test --workspace
./scripts/complete-story.sh 410
```

---

## Story #411: create_collection() Implementation

**GitHub Issue**: [#411](https://github.com/anibjoshi/in-mem/issues/411)
**Estimated Time**: 2.5 hours
**Dependencies**: #410, Epic 51, Epic 52
**Blocks**: Epic 54

### Start Story

```bash
gh issue view 411
./scripts/start-story.sh 53 411 create-collection
```

### Implementation

Add to `crates/primitives/src/vector/store.rs`:

```rust
//! VectorStore implementation

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::core::{Database, RunId};
use crate::vector::{
    CollectionId, CollectionInfo, VectorConfig, VectorError, VectorResult,
    IndexBackendFactory, VectorIndexBackend,
    validate_collection_name,
};

/// Stateless facade for vector operations
///
/// IMPORTANT: This struct follows Rule 1 (Stateless Facade Pattern).
/// All persistent state lives in Database. The backends map is an
/// in-memory cache that can be reconstructed from Database state.
///
/// Multiple VectorStore instances pointing to the same Database are safe.
pub struct VectorStore {
    /// Reference to the database
    db: Arc<Database>,

    /// In-memory index backends (cache, reconstructible from DB)
    /// Key: CollectionId, Value: Box<dyn VectorIndexBackend>
    backends: RwLock<HashMap<CollectionId, Box<dyn VectorIndexBackend>>>,

    /// Factory for creating index backends
    backend_factory: IndexBackendFactory,
}

impl VectorStore {
    /// Create a new VectorStore facade
    pub fn new(db: Arc<Database>) -> Self {
        VectorStore {
            db,
            backends: RwLock::new(HashMap::new()),
            backend_factory: IndexBackendFactory::default(),
        }
    }

    /// Create with a specific backend factory
    pub fn with_factory(db: Arc<Database>, factory: IndexBackendFactory) -> Self {
        VectorStore {
            db,
            backends: RwLock::new(HashMap::new()),
            backend_factory: factory,
        }
    }

    /// Create a new vector collection
    ///
    /// Returns error if collection already exists.
    /// WAL entry: COLLECTION_CREATE
    pub fn create_collection(
        &self,
        run_id: RunId,
        name: &str,
        config: VectorConfig,
    ) -> VectorResult<CollectionInfo> {
        // Validate collection name
        validate_collection_name(name)?;

        let collection_id = CollectionId::new(run_id.clone(), name);

        // Check if already exists
        if self.collection_exists(&collection_id)? {
            return Err(VectorError::CollectionAlreadyExists {
                name: name.to_string(),
            });
        }

        // Get current timestamp
        let created_at = Self::now_micros();

        // Create stored config
        let stored = StoredCollectionConfig {
            config: config.clone(),
            created_at,
        };

        // Store in KV with TypeTag::VectorCollection
        self.db.kv_put_with_tag(
            TypeTag::VectorCollection,
            &collection_id.to_key_string(),
            &stored.to_bytes()?,
        )?;

        // Create and cache backend
        let backend = self.backend_factory.create(config.dimension, config.metric);
        {
            let mut backends = self.backends.write().unwrap();
            backends.insert(collection_id.clone(), backend);
        }

        // Log WAL entry (handled by Database transaction)
        // self.db.log_wal_entry(WalEntryType::VectorCollectionCreate, ...)?;

        Ok(CollectionInfo::new(name.to_string(), config, 0, created_at))
    }

    /// Check if a collection exists
    pub fn collection_exists(&self, collection_id: &CollectionId) -> VectorResult<bool> {
        // Check cache first
        {
            let backends = self.backends.read().unwrap();
            if backends.contains_key(collection_id) {
                return Ok(true);
            }
        }

        // Check KV store
        let key = collection_id.to_key_string();
        Ok(self.db.kv_get_with_tag(TypeTag::VectorCollection, &key)?.is_some())
    }

    fn now_micros() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_micros() as u64)
            .unwrap_or(0)
    }
}

/// Stored collection configuration
#[derive(Debug, Clone)]
struct StoredCollectionConfig {
    config: VectorConfig,
    created_at: u64,
}

impl StoredCollectionConfig {
    fn to_bytes(&self) -> VectorResult<Vec<u8>> {
        let mut buf = Vec::new();

        // Version byte
        buf.push(0x01);

        // dimension (4 bytes)
        buf.extend_from_slice(&(self.config.dimension as u32).to_le_bytes());

        // metric (1 byte)
        buf.push(self.config.metric.to_byte());

        // storage_dtype (1 byte)
        buf.push(0); // F32 = 0

        // created_at (8 bytes)
        buf.extend_from_slice(&self.created_at.to_le_bytes());

        Ok(buf)
    }

    fn from_bytes(bytes: &[u8]) -> VectorResult<Self> {
        if bytes.len() < 15 {
            return Err(VectorError::Serialization("Truncated config".into()));
        }

        let version = bytes[0];
        if version != 0x01 {
            return Err(VectorError::Serialization(
                format!("Unknown config version: {}", version)
            ));
        }

        let dimension = u32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]) as usize;
        let metric = DistanceMetric::from_byte(bytes[5])
            .ok_or_else(|| VectorError::Serialization("Invalid metric".into()))?;
        let _dtype = bytes[6]; // Currently ignored, always F32

        let created_at = u64::from_le_bytes(
            bytes[7..15].try_into().unwrap()
        );

        Ok(StoredCollectionConfig {
            config: VectorConfig::new(dimension, metric)?,
            created_at,
        })
    }
}

#[cfg(test)]
mod create_tests {
    use super::*;
    use crate::vector::DistanceMetric;

    // Note: These tests require a mock Database or integration test setup

    #[test]
    fn test_stored_config_roundtrip() {
        let config = VectorConfig::new(768, DistanceMetric::Cosine).unwrap();
        let stored = StoredCollectionConfig {
            config: config.clone(),
            created_at: 1234567890,
        };

        let bytes = stored.to_bytes().unwrap();
        let restored = StoredCollectionConfig::from_bytes(&bytes).unwrap();

        assert_eq!(restored.config.dimension, 768);
        assert_eq!(restored.config.metric, DistanceMetric::Cosine);
        assert_eq!(restored.created_at, 1234567890);
    }
}
```

### Acceptance Criteria

- [ ] Validates collection name (length, characters)
- [ ] Returns error if collection exists
- [ ] Stores config in KV with TypeTag::VectorCollection
- [ ] Initializes empty backend
- [ ] Returns CollectionInfo

### Complete Story

```bash
~/.cargo/bin/cargo test --workspace
./scripts/complete-story.sh 411
```

---

## Story #412: delete_collection() Implementation

**GitHub Issue**: [#412](https://github.com/anibjoshi/in-mem/issues/412)
**Estimated Time**: 2 hours
**Dependencies**: #411
**Blocks**: None

### Start Story

```bash
gh issue view 412
./scripts/start-story.sh 53 412 delete-collection
```

### Implementation

Add to `crates/primitives/src/vector/store.rs`:

```rust
impl VectorStore {
    /// Delete a collection and all its vectors
    ///
    /// Cascade deletion removes:
    /// - All vector records from KV
    /// - Backend from in-memory cache
    /// - Collection config from KV
    ///
    /// Returns true if collection existed and was deleted.
    /// WAL entry: COLLECTION_DELETE
    pub fn delete_collection(
        &self,
        run_id: RunId,
        name: &str,
    ) -> VectorResult<bool> {
        let collection_id = CollectionId::new(run_id.clone(), name);

        // Check exists
        if !self.collection_exists(&collection_id)? {
            return Ok(false);
        }

        // Delete all vector records (cascade)
        self.delete_all_vectors_in_collection(&collection_id)?;

        // Remove backend from cache
        {
            let mut backends = self.backends.write().unwrap();
            backends.remove(&collection_id);
        }

        // Delete collection config from KV
        self.db.kv_delete_with_tag(
            TypeTag::VectorCollection,
            &collection_id.to_key_string(),
        )?;

        // Log WAL entry (handled by Database transaction)
        // self.db.log_wal_entry(WalEntryType::VectorCollectionDelete, ...)?;

        Ok(true)
    }

    /// Delete all vectors in a collection
    fn delete_all_vectors_in_collection(&self, collection_id: &CollectionId) -> VectorResult<()> {
        // Get all vector keys in this collection
        let prefix = format!("{}:", collection_id.to_key_string());

        // Iterate and delete each vector record
        let keys = self.db.kv_scan_keys_with_tag(TypeTag::VectorRecord, &prefix)?;

        for key in keys {
            self.db.kv_delete_with_tag(TypeTag::VectorRecord, &key)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod delete_tests {
    use super::*;

    // Integration tests would go here
}
```

### Acceptance Criteria

- [ ] Returns false if collection doesn't exist
- [ ] Deletes all vector records (cascade)
- [ ] Removes in-memory backend
- [ ] Deletes collection config from KV

### Complete Story

```bash
~/.cargo/bin/cargo test --workspace
./scripts/complete-story.sh 412
```

---

## Story #413: list_collections() and get_collection()

**GitHub Issue**: [#413](https://github.com/anibjoshi/in-mem/issues/413)
**Estimated Time**: 1.5 hours
**Dependencies**: #411
**Blocks**: None

### Start Story

```bash
gh issue view 413
./scripts/start-story.sh 53 413 list-collections
```

### Implementation

Add to `crates/primitives/src/vector/store.rs`:

```rust
impl VectorStore {
    /// List all collections for a run
    pub fn list_collections(
        &self,
        run_id: RunId,
    ) -> VectorResult<Vec<CollectionInfo>> {
        let prefix = format!("{}:", run_id);

        // Scan KV for all collections with this run_id
        let entries = self.db.kv_scan_with_tag(TypeTag::VectorCollection, &prefix)?;

        let mut collections = Vec::new();

        for (key, value) in entries {
            // Parse collection name from key
            let name = key.strip_prefix(&prefix)
                .ok_or_else(|| VectorError::Internal("Invalid key format".into()))?
                .to_string();

            // Parse stored config
            let stored = StoredCollectionConfig::from_bytes(&value)?;

            // Get current count from backend
            let collection_id = CollectionId::new(run_id.clone(), &name);
            let count = self.get_collection_count(&collection_id)?;

            collections.push(CollectionInfo::new(
                name,
                stored.config,
                count,
                stored.created_at,
            ));
        }

        // Sort by name for deterministic ordering
        collections.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(collections)
    }

    /// Get collection info by name
    pub fn get_collection(
        &self,
        run_id: RunId,
        name: &str,
    ) -> VectorResult<Option<CollectionInfo>> {
        let collection_id = CollectionId::new(run_id.clone(), name);
        let key = collection_id.to_key_string();

        // Lookup in KV
        let value = self.db.kv_get_with_tag(TypeTag::VectorCollection, &key)?;

        let Some(value) = value else {
            return Ok(None);
        };

        let stored = StoredCollectionConfig::from_bytes(&value)?;
        let count = self.get_collection_count(&collection_id)?;

        Ok(Some(CollectionInfo::new(
            name.to_string(),
            stored.config,
            count,
            stored.created_at,
        )))
    }

    /// Get the vector count for a collection
    fn get_collection_count(&self, collection_id: &CollectionId) -> VectorResult<usize> {
        // Try cache first
        {
            let backends = self.backends.read().unwrap();
            if let Some(backend) = backends.get(collection_id) {
                return Ok(backend.len());
            }
        }

        // If not in cache, count from KV (slower but accurate)
        let prefix = format!("{}:", collection_id.to_key_string());
        let count = self.db.kv_count_with_tag(TypeTag::VectorRecord, &prefix)?;

        Ok(count)
    }
}

#[cfg(test)]
mod list_tests {
    use super::*;

    // Integration tests would go here
}
```

### Acceptance Criteria

- [ ] `list_collections()` returns all collections for run
- [ ] Results include current vector count
- [ ] `get_collection()` returns None if not found
- [ ] Results sorted by name for determinism
- [ ] Read operations do NOT write WAL (Invariant R10)

### Complete Story

```bash
~/.cargo/bin/cargo test --workspace
./scripts/complete-story.sh 413
```

---

## Story #414: Collection Config Persistence

**GitHub Issue**: [#414](https://github.com/anibjoshi/in-mem/issues/414)
**Estimated Time**: 1.5 hours
**Dependencies**: #411
**Blocks**: Epic 55

### Start Story

```bash
gh issue view 414
./scripts/start-story.sh 53 414 config-persistence
```

### Implementation

The `StoredCollectionConfig` is already implemented in Story #411. This story adds comprehensive tests and ensures forward compatibility.

```rust
#[cfg(test)]
mod persistence_tests {
    use super::*;
    use crate::vector::DistanceMetric;

    #[test]
    fn test_stored_config_format() {
        let config = VectorConfig::new(768, DistanceMetric::Euclidean).unwrap();
        let stored = StoredCollectionConfig {
            config,
            created_at: 1704067200000000, // 2024-01-01 00:00:00 UTC
        };

        let bytes = stored.to_bytes().unwrap();

        // Verify format
        assert_eq!(bytes[0], 0x01); // Version
        assert_eq!(u32::from_le_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]), 768); // Dimension
        assert_eq!(bytes[5], 1); // Euclidean = 1
        assert_eq!(bytes[6], 0); // F32 = 0
    }

    #[test]
    fn test_all_metrics_persist() {
        for metric in [
            DistanceMetric::Cosine,
            DistanceMetric::Euclidean,
            DistanceMetric::DotProduct,
        ] {
            let config = VectorConfig::new(384, metric).unwrap();
            let stored = StoredCollectionConfig {
                config: config.clone(),
                created_at: 0,
            };

            let bytes = stored.to_bytes().unwrap();
            let restored = StoredCollectionConfig::from_bytes(&bytes).unwrap();

            assert_eq!(restored.config.metric, metric);
        }
    }

    #[test]
    fn test_various_dimensions() {
        for dim in [128, 384, 768, 1536, 3072] {
            let config = VectorConfig::new(dim, DistanceMetric::Cosine).unwrap();
            let stored = StoredCollectionConfig {
                config: config.clone(),
                created_at: 0,
            };

            let bytes = stored.to_bytes().unwrap();
            let restored = StoredCollectionConfig::from_bytes(&bytes).unwrap();

            assert_eq!(restored.config.dimension, dim);
        }
    }

    #[test]
    fn test_invalid_version() {
        let mut bytes = vec![0x02, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let result = StoredCollectionConfig::from_bytes(&bytes);
        assert!(result.is_err());
    }

    #[test]
    fn test_truncated_data() {
        let bytes = vec![0x01, 0, 0]; // Too short
        let result = StoredCollectionConfig::from_bytes(&bytes);
        assert!(result.is_err());
    }
}
```

### Acceptance Criteria

- [ ] Compact binary serialization (no JSON overhead)
- [ ] Dimension stored as u32
- [ ] Metric stored as single byte
- [ ] Timestamp in microseconds
- [ ] Round-trip serialization tests
- [ ] Forward compatible (version byte)

### Complete Story

```bash
~/.cargo/bin/cargo test --workspace
./scripts/complete-story.sh 414
```

---

## Epic 53 Completion Checklist

### Validation

```bash
# Full test suite
~/.cargo/bin/cargo test --workspace

# Collection-specific tests
~/.cargo/bin/cargo test vector::collection
~/.cargo/bin/cargo test vector::store

# Clippy and format
~/.cargo/bin/cargo clippy --workspace -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### Run Isolation Test

```rust
#[test]
fn test_run_isolation() {
    // Create collections with same name in different runs
    let store = VectorStore::new(db);

    let run_a = RunId::new();
    let run_b = RunId::new();

    let config = VectorConfig::new(384, DistanceMetric::Cosine).unwrap();

    // Create "embeddings" in both runs
    store.create_collection(run_a.clone(), "embeddings", config.clone()).unwrap();
    store.create_collection(run_b.clone(), "embeddings", config.clone()).unwrap();

    // Each run sees only its own collection
    let list_a = store.list_collections(run_a.clone()).unwrap();
    let list_b = store.list_collections(run_b.clone()).unwrap();

    assert_eq!(list_a.len(), 1);
    assert_eq!(list_b.len(), 1);

    // Deleting from one run doesn't affect the other
    store.delete_collection(run_a, "embeddings").unwrap();

    let list_b_after = store.list_collections(run_b).unwrap();
    assert_eq!(list_b_after.len(), 1); // Still has its collection
}
```

### Epic Merge

```bash
git checkout develop
git merge --no-ff epic-53-collection-management -m "Epic 53: Collection Management complete"
git push origin develop

gh issue close 391 --comment "Epic 53 complete. All 5 stories merged and validated."
```

---

*End of Epic 53 Prompts*
