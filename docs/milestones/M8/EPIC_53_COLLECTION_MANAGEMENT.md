# Epic 53: Collection Management

**Goal**: Implement collection CRUD operations

**Dependencies**: Epic 51 (Vector Heap)

---

## Scope

- CollectionInfo and CollectionId types
- create_collection() with config validation
- delete_collection() with cascade deletion
- list_collections() and get_collection() for discovery
- Collection config persistence in KV store

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #346 | CollectionInfo and CollectionId Types | FOUNDATION |
| #347 | create_collection() Implementation | CRITICAL |
| #348 | delete_collection() Implementation | CRITICAL |
| #349 | list_collections() and get_collection() | HIGH |
| #350 | Collection Config Persistence | HIGH |

---

## Story #346: CollectionInfo and CollectionId Types

**File**: `crates/primitives/src/vector/collection.rs` (NEW)

**Deliverable**: Collection management types

### Implementation

```rust
use crate::vector::{VectorConfig, VectorError};
use crate::core::RunId;

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

/// Collection name validation
pub fn validate_collection_name(name: &str) -> Result<(), VectorError> {
    if name.is_empty() {
        return Err(VectorError::InvalidCollectionName {
            name: name.to_string(),
            reason: "Collection name cannot be empty".to_string(),
        });
    }

    if name.len() > 256 {
        return Err(VectorError::InvalidCollectionName {
            name: name.to_string(),
            reason: "Collection name cannot exceed 256 characters".to_string(),
        });
    }

    // Forbidden characters that could cause key parsing issues
    if name.contains('/') {
        return Err(VectorError::InvalidCollectionName {
            name: name.to_string(),
            reason: "Collection name cannot contain '/'".to_string(),
        });
    }

    if name.contains('\0') {
        return Err(VectorError::InvalidCollectionName {
            name: name.to_string(),
            reason: "Collection name cannot contain null bytes".to_string(),
        });
    }

    // Names starting with underscore are reserved for system use
    if name.starts_with('_') {
        return Err(VectorError::InvalidCollectionName {
            name: name.to_string(),
            reason: "Collection names starting with '_' are reserved".to_string(),
        });
    }

    Ok(())
}

/// Vector key validation
pub fn validate_vector_key(key: &str) -> Result<(), VectorError> {
    if key.is_empty() {
        return Err(VectorError::InvalidKey {
            key: key.to_string(),
            reason: "Vector key cannot be empty".to_string(),
        });
    }

    if key.len() > 1024 {
        return Err(VectorError::InvalidKey {
            key: key.to_string(),
            reason: "Vector key cannot exceed 1024 characters".to_string(),
        });
    }

    if key.contains('\0') {
        return Err(VectorError::InvalidKey {
            key: key.to_string(),
            reason: "Vector key cannot contain null bytes".to_string(),
        });
    }

    Ok(())
}
```

### Acceptance Criteria

- [ ] CollectionId with run_id and name fields
- [ ] CollectionInfo with name, config, count, created_at
- [ ] validate_collection_name() checks empty, length, forbidden chars
- [ ] validate_vector_key() checks empty, length, null bytes
- [ ] Names starting with '_' reserved for system use
- [ ] '/' forbidden in collection names (used as key separator)

---

## Story #347: create_collection() Implementation

**File**: `crates/primitives/src/vector/store.rs` (partial)

**Deliverable**: Collection creation with validation

### Implementation

```rust
impl VectorStore {
    /// Create a new collection
    ///
    /// Creates a collection with the specified configuration.
    /// The configuration (dimension, metric, dtype) is immutable after creation.
    ///
    /// # Errors
    /// - CollectionAlreadyExists if a collection with this name exists
    /// - InvalidCollectionName if name is invalid
    /// - InvalidDimension if dimension is 0
    pub fn create_collection(
        &self,
        run_id: RunId,
        name: &str,
        config: VectorConfig,
    ) -> Result<CollectionInfo, VectorError> {
        // Validate name
        validate_collection_name(name)?;

        // Validate config
        if config.dimension == 0 {
            return Err(VectorError::InvalidDimension {
                dimension: config.dimension,
            });
        }

        let collection_id = CollectionId::new(run_id, name);

        // Check if collection already exists
        if self.collection_exists(&collection_id)? {
            return Err(VectorError::CollectionAlreadyExists {
                name: name.to_string(),
            });
        }

        let now = crate::util::now_micros();

        // Create collection record
        let record = CollectionRecord {
            config: VectorConfigSerde::from(&config),
            created_at: now,
        };

        // Store config in KV
        let config_key = Key::new_vector_config(
            Namespace::from_run_id(run_id),
            name,
        );
        let config_bytes = record.to_bytes()?;
        self.db.kv_put(&config_key, &config_bytes)?;

        // Write WAL entry
        self.write_wal_collection_create(run_id, name, &config)?;

        // Initialize in-memory backend
        self.init_backend(&collection_id, &config)?;

        Ok(CollectionInfo {
            name: name.to_string(),
            config,
            count: 0,
            created_at: now,
        })
    }

    /// Check if a collection exists
    fn collection_exists(&self, id: &CollectionId) -> Result<bool, VectorError> {
        let config_key = Key::new_vector_config(
            Namespace::from_run_id(id.run_id),
            &id.name,
        );
        Ok(self.db.kv_get(&config_key)?.is_some())
    }

    /// Initialize the index backend for a collection
    fn init_backend(
        &self,
        id: &CollectionId,
        config: &VectorConfig,
    ) -> Result<(), VectorError> {
        let backend = self.backend_factory.create(config);
        self.backends.write().unwrap().insert(id.clone(), backend);
        Ok(())
    }

    /// Write WAL entry for collection creation
    fn write_wal_collection_create(
        &self,
        run_id: RunId,
        name: &str,
        config: &VectorConfig,
    ) -> Result<(), VectorError> {
        let payload = WalVectorCollectionCreate {
            run_id,
            collection: name.to_string(),
            config: VectorConfigSerde::from(config),
            timestamp: crate::util::now_micros(),
        };

        self.db.write_wal_entry(
            WalEntryType::VectorCollectionCreate,
            &payload.to_bytes()?,
        )?;

        Ok(())
    }
}
```

### Acceptance Criteria

- [ ] Validates collection name
- [ ] Validates dimension > 0
- [ ] Returns error if collection already exists
- [ ] Stores config in KV via TypeTag::VectorConfig
- [ ] Writes WAL_VECTOR_COLLECTION_CREATE entry
- [ ] Initializes in-memory backend
- [ ] Returns CollectionInfo on success

---

## Story #348: delete_collection() Implementation

**File**: `crates/primitives/src/vector/store.rs` (partial)

**Deliverable**: Collection deletion with cascade

### Implementation

```rust
impl VectorStore {
    /// Delete a collection and all its vectors
    ///
    /// This is a destructive operation that:
    /// 1. Deletes all vectors in the collection
    /// 2. Deletes the collection configuration
    /// 3. Removes the in-memory backend
    ///
    /// # Errors
    /// - CollectionNotFound if collection doesn't exist
    pub fn delete_collection(
        &self,
        run_id: RunId,
        name: &str,
    ) -> Result<(), VectorError> {
        let collection_id = CollectionId::new(run_id, name);

        // Check if collection exists
        if !self.collection_exists(&collection_id)? {
            return Err(VectorError::CollectionNotFound {
                name: name.to_string(),
            });
        }

        // Delete all vectors in the collection
        self.delete_all_vectors(&collection_id)?;

        // Delete config from KV
        let config_key = Key::new_vector_config(
            Namespace::from_run_id(run_id),
            name,
        );
        self.db.kv_delete(&config_key)?;

        // Write WAL entry
        self.write_wal_collection_delete(run_id, name)?;

        // Remove in-memory backend
        self.backends.write().unwrap().remove(&collection_id);

        Ok(())
    }

    /// Delete all vectors in a collection
    fn delete_all_vectors(&self, id: &CollectionId) -> Result<(), VectorError> {
        let namespace = Namespace::from_run_id(id.run_id);
        let prefix = Key::vector_collection_prefix(namespace, &id.name);

        // Scan all vector keys in this collection
        let keys: Vec<Key> = self.db.scan_keys_with_prefix(&prefix)?;

        // Delete each vector
        for key in keys {
            self.db.kv_delete(&key)?;
        }

        Ok(())
    }

    /// Write WAL entry for collection deletion
    fn write_wal_collection_delete(
        &self,
        run_id: RunId,
        name: &str,
    ) -> Result<(), VectorError> {
        let payload = WalVectorCollectionDelete {
            run_id,
            collection: name.to_string(),
            timestamp: crate::util::now_micros(),
        };

        self.db.write_wal_entry(
            WalEntryType::VectorCollectionDelete,
            &payload.to_bytes()?,
        )?;

        Ok(())
    }
}
```

### Acceptance Criteria

- [ ] Returns error if collection doesn't exist
- [ ] Deletes all vectors in the collection
- [ ] Deletes collection config from KV
- [ ] Writes WAL_VECTOR_COLLECTION_DELETE entry
- [ ] Removes in-memory backend
- [ ] Operation is atomic (via transaction)

---

## Story #349: list_collections() and get_collection()

**File**: `crates/primitives/src/vector/store.rs` (partial)

**Deliverable**: Collection discovery operations

### Implementation

```rust
impl VectorStore {
    /// List all collections for a run
    ///
    /// Returns CollectionInfo for each collection, including current vector count.
    pub fn list_collections(&self, run_id: RunId) -> Result<Vec<CollectionInfo>, VectorError> {
        let namespace = Namespace::from_run_id(run_id);

        // Scan all collection configs
        let prefix = Key::new(namespace, TypeTag::VectorConfig, "".to_string());
        let entries: Vec<(Key, Vec<u8>)> = self.db.scan_with_prefix(&prefix)?;

        let mut collections = Vec::new();

        for (key, value) in entries {
            let name = key.user_key().to_string();
            let record = CollectionRecord::from_bytes(&value)?;
            let config = VectorConfig::try_from(record.config)?;

            // Get current count from backend
            let collection_id = CollectionId::new(run_id, &name);
            let count = self.get_collection_count(&collection_id)?;

            collections.push(CollectionInfo {
                name,
                config,
                count,
                created_at: record.created_at,
            });
        }

        // Sort by name for deterministic ordering
        collections.sort_by(|a, b| a.name.cmp(&b.name));

        Ok(collections)
    }

    /// Get a single collection's info
    ///
    /// Returns None if collection doesn't exist.
    pub fn get_collection(
        &self,
        run_id: RunId,
        name: &str,
    ) -> Result<Option<CollectionInfo>, VectorError> {
        let config_key = Key::new_vector_config(
            Namespace::from_run_id(run_id),
            name,
        );

        let Some(value) = self.db.kv_get(&config_key)? else {
            return Ok(None);
        };

        let record = CollectionRecord::from_bytes(&value)?;
        let config = VectorConfig::try_from(record.config)?;

        let collection_id = CollectionId::new(run_id, name);
        let count = self.get_collection_count(&collection_id)?;

        Ok(Some(CollectionInfo {
            name: name.to_string(),
            config,
            count,
            created_at: record.created_at,
        }))
    }

    /// Get the current vector count for a collection
    fn get_collection_count(&self, id: &CollectionId) -> Result<usize, VectorError> {
        let backends = self.backends.read().unwrap();
        if let Some(backend) = backends.get(id) {
            Ok(backend.len())
        } else {
            // Backend not loaded - count from KV
            let namespace = Namespace::from_run_id(id.run_id);
            let prefix = Key::vector_collection_prefix(namespace, &id.name);
            let count = self.db.count_keys_with_prefix(&prefix)?;
            Ok(count)
        }
    }
}
```

### Acceptance Criteria

- [ ] list_collections() returns all collections for run_id
- [ ] Results sorted by name for determinism
- [ ] Each CollectionInfo includes current count
- [ ] get_collection() returns Option for single collection
- [ ] Returns None if collection doesn't exist
- [ ] Count accurate even if backend not loaded

---

## Story #350: Collection Config Persistence

**File**: `crates/primitives/src/vector/store.rs` (partial)

**Deliverable**: Reliable config persistence and loading

### Implementation

```rust
use serde::{Deserialize, Serialize};

/// Collection record stored in KV
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionRecord {
    /// Serializable configuration
    pub config: VectorConfigSerde,

    /// Creation timestamp (microseconds)
    pub created_at: u64,
}

impl CollectionRecord {
    pub fn new(config: &VectorConfig) -> Self {
        CollectionRecord {
            config: VectorConfigSerde::from(config),
            created_at: crate::util::now_micros(),
        }
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, VectorError> {
        rmp_serde::to_vec(self)
            .map_err(|e| VectorError::Serialization(e.to_string()))
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, VectorError> {
        rmp_serde::from_slice(data)
            .map_err(|e| VectorError::Serialization(e.to_string()))
    }
}

/// Serializable VectorConfig for KV storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorConfigSerde {
    pub dimension: usize,
    pub metric: u8,
    pub storage_dtype: u8,
}

impl From<&VectorConfig> for VectorConfigSerde {
    fn from(config: &VectorConfig) -> Self {
        VectorConfigSerde {
            dimension: config.dimension,
            metric: config.metric.to_byte(),
            storage_dtype: 0, // F32 = 0
        }
    }
}

impl TryFrom<VectorConfigSerde> for VectorConfig {
    type Error = VectorError;

    fn try_from(serde: VectorConfigSerde) -> Result<Self, Self::Error> {
        let metric = DistanceMetric::from_byte(serde.metric)
            .ok_or_else(|| VectorError::Serialization(
                format!("Invalid metric byte: {}", serde.metric)
            ))?;

        Ok(VectorConfig {
            dimension: serde.dimension,
            metric,
            storage_dtype: StorageDtype::F32,
        })
    }
}

impl VectorStore {
    /// Load collection config from KV
    fn load_collection_config(
        &self,
        run_id: RunId,
        name: &str,
    ) -> Result<Option<VectorConfig>, VectorError> {
        let config_key = Key::new_vector_config(
            Namespace::from_run_id(run_id),
            name,
        );

        let Some(value) = self.db.kv_get(&config_key)? else {
            return Ok(None);
        };

        let record = CollectionRecord::from_bytes(&value)?;
        let config = VectorConfig::try_from(record.config)?;
        Ok(Some(config))
    }

    /// Ensure collection is loaded into memory
    ///
    /// If the collection exists in KV but not in memory (after recovery),
    /// this loads it and initializes the backend.
    fn ensure_collection_loaded(
        &self,
        run_id: RunId,
        name: &str,
    ) -> Result<(), VectorError> {
        let collection_id = CollectionId::new(run_id, name);

        // Already loaded?
        if self.backends.read().unwrap().contains_key(&collection_id) {
            return Ok(());
        }

        // Load from KV
        let config = self.load_collection_config(run_id, name)?
            .ok_or_else(|| VectorError::CollectionNotFound {
                name: name.to_string(),
            })?;

        // Initialize backend
        self.init_backend(&collection_id, &config)?;

        // Load vectors into backend
        self.load_vectors_into_backend(&collection_id)?;

        Ok(())
    }

    /// Load all vectors for a collection into its backend
    fn load_vectors_into_backend(
        &self,
        id: &CollectionId,
    ) -> Result<(), VectorError> {
        let namespace = Namespace::from_run_id(id.run_id);
        let prefix = Key::vector_collection_prefix(namespace, &id.name);

        let entries: Vec<(Key, Vec<u8>)> = self.db.scan_with_prefix(&prefix)?;

        let mut backends = self.backends.write().unwrap();
        let backend = backends.get_mut(id)
            .ok_or_else(|| VectorError::Internal(
                "Backend not initialized".to_string()
            ))?;

        for (_key, value) in entries {
            let record = VectorRecord::from_bytes(&value)?;
            // Note: Embedding is loaded from snapshot, not KV
            // This method is for loading metadata only
        }

        Ok(())
    }
}
```

### Acceptance Criteria

- [ ] CollectionRecord serializes to/from MessagePack
- [ ] VectorConfigSerde handles metric/dtype byte conversion
- [ ] load_collection_config() retrieves from KV
- [ ] ensure_collection_loaded() lazy-loads collections
- [ ] Config survives restart via KV persistence
- [ ] Roundtrip: create → restart → get returns same config

---

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collection_lifecycle() {
        let db = test_db();
        let store = VectorStore::new(db);
        let run_id = RunId::new();

        // Create
        let config = VectorConfig::for_minilm();
        let info = store.create_collection(run_id, "test", config.clone()).unwrap();
        assert_eq!(info.name, "test");
        assert_eq!(info.count, 0);

        // Get
        let info = store.get_collection(run_id, "test").unwrap().unwrap();
        assert_eq!(info.config.dimension, 384);

        // List
        let collections = store.list_collections(run_id).unwrap();
        assert_eq!(collections.len(), 1);

        // Delete
        store.delete_collection(run_id, "test").unwrap();
        assert!(store.get_collection(run_id, "test").unwrap().is_none());
    }

    #[test]
    fn test_collection_already_exists() {
        let db = test_db();
        let store = VectorStore::new(db);
        let run_id = RunId::new();

        let config = VectorConfig::for_minilm();
        store.create_collection(run_id, "test", config.clone()).unwrap();

        // Second create should fail
        let result = store.create_collection(run_id, "test", config);
        assert!(matches!(result, Err(VectorError::CollectionAlreadyExists { .. })));
    }

    #[test]
    fn test_collection_not_found() {
        let db = test_db();
        let store = VectorStore::new(db);
        let run_id = RunId::new();

        let result = store.delete_collection(run_id, "nonexistent");
        assert!(matches!(result, Err(VectorError::CollectionNotFound { .. })));
    }

    #[test]
    fn test_collection_name_validation() {
        assert!(validate_collection_name("valid_name").is_ok());
        assert!(validate_collection_name("").is_err());
        assert!(validate_collection_name("has/slash").is_err());
        assert!(validate_collection_name("_reserved").is_err());
    }

    #[test]
    fn test_run_isolation() {
        let db = test_db();
        let store = VectorStore::new(db);

        let run1 = RunId::new();
        let run2 = RunId::new();

        let config = VectorConfig::for_minilm();

        // Create same-named collection in different runs
        store.create_collection(run1, "shared_name", config.clone()).unwrap();
        store.create_collection(run2, "shared_name", config).unwrap();

        // Each run sees only its own collection
        let list1 = store.list_collections(run1).unwrap();
        let list2 = store.list_collections(run2).unwrap();

        assert_eq!(list1.len(), 1);
        assert_eq!(list2.len(), 1);

        // Deleting from one run doesn't affect the other
        store.delete_collection(run1, "shared_name").unwrap();
        assert!(store.get_collection(run2, "shared_name").unwrap().is_some());
    }

    #[test]
    fn test_collection_config_persistence() {
        let config = VectorConfig::new(768, DistanceMetric::Euclidean).unwrap();
        let record = CollectionRecord::new(&config);

        // Serialize and deserialize
        let bytes = record.to_bytes().unwrap();
        let restored = CollectionRecord::from_bytes(&bytes).unwrap();

        let restored_config = VectorConfig::try_from(restored.config).unwrap();
        assert_eq!(restored_config.dimension, 768);
        assert_eq!(restored_config.metric, DistanceMetric::Euclidean);
    }
}
```

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/primitives/src/vector/collection.rs` | CREATE - Collection types and validation |
| `crates/primitives/src/vector/store.rs` | MODIFY - Add collection management methods |
| `crates/primitives/src/vector/mod.rs` | MODIFY - Export collection module |
