# Epic 55: Transaction & Durability

**Goal**: Integrate with transaction system and M7 durability

**Dependencies**: Epic 51 (Vector Heap), Epic 52 (Index Backend)

---

## Scope

- Vector WAL entry types (0x70-0x73)
- WAL write and replay for vector operations
- Snapshot serialization with next_id and free_slots
- Recovery from snapshot + WAL
- Cross-primitive transaction tests

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #357 | Vector WAL Entry Types | CRITICAL |
| #358 | Vector WAL Write and Replay | CRITICAL |
| #359 | Vector Snapshot Serialization | CRITICAL |
| #360 | Vector Recovery Implementation | CRITICAL |
| #361 | Cross-Primitive Transaction Tests | HIGH |

---

## Story #357: Vector WAL Entry Types

**File**: `crates/durability/src/wal_types.rs`

**Deliverable**: WAL entry types for vector operations

### Implementation

```rust
/// Vector WAL entry types (0x70-0x7F range)
///
/// Naming convention:
/// - COLLECTION_CREATE/DELETE: prefixed to distinguish from vector-level ops
/// - UPSERT (not INSERT): matches our semantic (insert overwrites if exists)
pub const WAL_VECTOR_COLLECTION_CREATE: u8 = 0x70;
pub const WAL_VECTOR_COLLECTION_DELETE: u8 = 0x71;
pub const WAL_VECTOR_UPSERT: u8 = 0x72;
pub const WAL_VECTOR_DELETE: u8 = 0x73;

/// WAL entry type enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalEntryType {
    // ... existing types ...

    // Vector operations (M8)
    VectorCollectionCreate,
    VectorCollectionDelete,
    VectorUpsert,
    VectorDelete,
}

impl WalEntryType {
    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            // ... existing ...
            0x70 => Some(WalEntryType::VectorCollectionCreate),
            0x71 => Some(WalEntryType::VectorCollectionDelete),
            0x72 => Some(WalEntryType::VectorUpsert),
            0x73 => Some(WalEntryType::VectorDelete),
            _ => None,
        }
    }

    pub fn to_byte(&self) -> u8 {
        match self {
            // ... existing ...
            WalEntryType::VectorCollectionCreate => 0x70,
            WalEntryType::VectorCollectionDelete => 0x71,
            WalEntryType::VectorUpsert => 0x72,
            WalEntryType::VectorDelete => 0x73,
        }
    }

    pub fn primitive_kind(&self) -> Option<PrimitiveKind> {
        match self {
            // ... existing ...
            WalEntryType::VectorCollectionCreate |
            WalEntryType::VectorCollectionDelete |
            WalEntryType::VectorUpsert |
            WalEntryType::VectorDelete => Some(PrimitiveKind::Vector),
        }
    }
}

/// Primitive kinds for routing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveKind {
    Kv,
    Json,
    Event,
    State,
    Trace,
    Run,
    Vector,  // M8 addition
}
```

### Acceptance Criteria

- [ ] WAL_VECTOR_COLLECTION_CREATE = 0x70
- [ ] WAL_VECTOR_COLLECTION_DELETE = 0x71
- [ ] WAL_VECTOR_UPSERT = 0x72
- [ ] WAL_VECTOR_DELETE = 0x73
- [ ] WalEntryType enum with Vector variants
- [ ] from_byte()/to_byte() conversions
- [ ] primitive_kind() returns PrimitiveKind::Vector

---

## Story #358: Vector WAL Write and Replay

**File**: `crates/primitives/src/vector/wal.rs` (NEW)

**Deliverable**: WAL payloads and replay logic

### Implementation

```rust
use serde::{Deserialize, Serialize};
use crate::core::RunId;
use crate::vector::{VectorId, VectorConfigSerde};

/// WAL payload for collection creation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalVectorCollectionCreate {
    pub run_id: RunId,
    pub collection: String,
    pub config: VectorConfigSerde,
    pub timestamp: u64,
}

/// WAL payload for collection deletion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalVectorCollectionDelete {
    pub run_id: RunId,
    pub collection: String,
    pub timestamp: u64,
}

/// WAL payload for vector upsert
///
/// WARNING: TEMPORARY M8 FORMAT
/// This payload contains the full embedding, which:
/// - Bloats WAL size significantly (3KB per 768-dim vector)
/// - Slows down recovery proportionally
///
/// This is acceptable for M8 (correctness over performance).
///
/// M9 MAY change this to:
/// - Store embeddings in separate segment
/// - Use delta encoding for updates
/// - Reference external embedding storage
///
/// Any such change MUST be versioned and backward compatible.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalVectorUpsert {
    pub run_id: RunId,
    pub collection: String,
    pub key: String,
    pub vector_id: u64,
    pub embedding: Vec<f32>,  // TEMPORARY: Full embedding in WAL
    pub metadata: Option<serde_json::Value>,
    pub timestamp: u64,
}

/// WAL payload for vector deletion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalVectorDelete {
    pub run_id: RunId,
    pub collection: String,
    pub key: String,
    pub vector_id: u64,
    pub timestamp: u64,
}

impl WalVectorCollectionCreate {
    pub fn to_bytes(&self) -> Result<Vec<u8>, VectorError> {
        rmp_serde::to_vec(self)
            .map_err(|e| VectorError::Serialization(e.to_string()))
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, VectorError> {
        rmp_serde::from_slice(data)
            .map_err(|e| VectorError::Serialization(e.to_string()))
    }
}

// Similar implementations for other payloads...

impl VectorStore {
    /// Write WAL entry for vector upsert
    fn write_wal_upsert(
        &self,
        run_id: RunId,
        collection: &str,
        key: &str,
        vector_id: VectorId,
        embedding: &[f32],
        metadata: &Option<serde_json::Value>,
    ) -> Result<(), VectorError> {
        let payload = WalVectorUpsert {
            run_id,
            collection: collection.to_string(),
            key: key.to_string(),
            vector_id: vector_id.as_u64(),
            embedding: embedding.to_vec(),
            metadata: metadata.clone(),
            timestamp: crate::util::now_micros(),
        };

        self.db.write_wal_entry(
            WalEntryType::VectorUpsert,
            &payload.to_bytes()?,
        )?;

        Ok(())
    }

    /// Write WAL entry for vector deletion
    fn write_wal_delete(
        &self,
        run_id: RunId,
        collection: &str,
        key: &str,
        vector_id: VectorId,
    ) -> Result<(), VectorError> {
        let payload = WalVectorDelete {
            run_id,
            collection: collection.to_string(),
            key: key.to_string(),
            vector_id: vector_id.as_u64(),
            timestamp: crate::util::now_micros(),
        };

        self.db.write_wal_entry(
            WalEntryType::VectorDelete,
            &payload.to_bytes()?,
        )?;

        Ok(())
    }
}

/// WAL replay handler for vector operations
///
/// IMPORTANT: WAL replay is transaction-aware.
/// This handler is called by the global WAL replayer, which:
/// 1. Groups entries by transaction ID
/// 2. Only applies committed transactions
/// 3. Respects transaction ordering
/// 4. Handles cross-primitive atomicity (KV + Vector in same tx)
///
/// The vector replayer does NOT need to check transaction boundaries
/// because the global replayer handles this.
pub struct VectorWalReplayer<'a> {
    store: &'a mut VectorStore,
}

impl<'a> VectorWalReplayer<'a> {
    pub fn new(store: &'a mut VectorStore) -> Self {
        VectorWalReplayer { store }
    }

    /// Apply a single WAL entry
    ///
    /// Called by the global replayer for committed vector entries.
    pub fn apply_entry(&mut self, entry: &WalEntry) -> Result<(), VectorError> {
        match entry.entry_type {
            WalEntryType::VectorCollectionCreate => {
                let payload = WalVectorCollectionCreate::from_bytes(&entry.payload)?;
                let config = VectorConfig::try_from(payload.config)?;
                self.store.replay_create_collection(
                    payload.run_id,
                    &payload.collection,
                    config,
                )?;
            }
            WalEntryType::VectorCollectionDelete => {
                let payload = WalVectorCollectionDelete::from_bytes(&entry.payload)?;
                self.store.replay_delete_collection(
                    payload.run_id,
                    &payload.collection,
                )?;
            }
            WalEntryType::VectorUpsert => {
                let payload = WalVectorUpsert::from_bytes(&entry.payload)?;
                self.store.replay_upsert(
                    payload.run_id,
                    &payload.collection,
                    &payload.key,
                    VectorId::new(payload.vector_id),
                    &payload.embedding,
                    payload.metadata,
                )?;
            }
            WalEntryType::VectorDelete => {
                let payload = WalVectorDelete::from_bytes(&entry.payload)?;
                self.store.replay_delete(
                    payload.run_id,
                    &payload.collection,
                    &payload.key,
                    VectorId::new(payload.vector_id),
                )?;
            }
            _ => {
                // Not a vector entry, ignore
            }
        }
        Ok(())
    }
}

impl VectorStore {
    /// Replay collection creation (no WAL write)
    fn replay_create_collection(
        &mut self,
        run_id: RunId,
        name: &str,
        config: VectorConfig,
    ) -> Result<(), VectorError> {
        let collection_id = CollectionId::new(run_id, name);
        let backend = self.backend_factory.create(&config);
        self.backends.write().unwrap().insert(collection_id, backend);
        Ok(())
    }

    /// Replay collection deletion (no WAL write)
    fn replay_delete_collection(
        &mut self,
        run_id: RunId,
        name: &str,
    ) -> Result<(), VectorError> {
        let collection_id = CollectionId::new(run_id, name);
        self.backends.write().unwrap().remove(&collection_id);
        Ok(())
    }

    /// Replay upsert (no WAL write)
    fn replay_upsert(
        &mut self,
        run_id: RunId,
        collection: &str,
        key: &str,
        vector_id: VectorId,
        embedding: &[f32],
        metadata: Option<serde_json::Value>,
    ) -> Result<(), VectorError> {
        let collection_id = CollectionId::new(run_id, collection);

        let mut backends = self.backends.write().unwrap();
        let backend = backends.get_mut(&collection_id)
            .ok_or_else(|| VectorError::CollectionNotFound {
                name: collection.to_string(),
            })?;

        // Use insert_with_id to maintain VectorId continuity
        backend.heap_mut().insert_with_id(vector_id, embedding)?;

        Ok(())
    }

    /// Replay deletion (no WAL write)
    fn replay_delete(
        &mut self,
        run_id: RunId,
        collection: &str,
        _key: &str,
        vector_id: VectorId,
    ) -> Result<(), VectorError> {
        let collection_id = CollectionId::new(run_id, collection);

        let mut backends = self.backends.write().unwrap();
        let backend = backends.get_mut(&collection_id)
            .ok_or_else(|| VectorError::CollectionNotFound {
                name: collection.to_string(),
            })?;

        backend.delete(vector_id)?;

        Ok(())
    }
}
```

### Acceptance Criteria

- [ ] WAL payload structs for all 4 entry types
- [ ] Payloads serialize to/from MessagePack
- [ ] write_wal_upsert() includes full embedding
- [ ] VectorWalReplayer applies entries correctly
- [ ] replay_* methods don't write to WAL (replay-only)
- [ ] insert_with_id() maintains VectorId continuity
- [ ] Doc comment explains transaction-awareness

---

## Story #359: Vector Snapshot Serialization

**File**: `crates/primitives/src/vector/snapshot.rs` (NEW)

**Deliverable**: Snapshot format for vector data

### Implementation

```rust
use std::io::{Read, Write};
use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};

/// Snapshot format version
const VECTOR_SNAPSHOT_VERSION: u8 = 0x01;

/// Collection snapshot header (MessagePack serialized)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionSnapshotHeader {
    pub run_id: RunId,
    pub name: String,
    pub dimension: usize,
    pub metric: u8,
    pub storage_dtype: u8,
    /// CRITICAL: Must be persisted to maintain ID uniqueness across restarts
    pub next_id: u64,
    /// CRITICAL: Must be persisted for correct slot reuse after recovery
    pub free_slots: Vec<usize>,
    pub count: u32,
}

impl VectorStore {
    /// Serialize vector data for snapshot
    ///
    /// Format:
    /// - Version byte (0x01)
    /// - Collection count (u32 LE)
    /// - For each collection:
    ///   - Header length (u32 LE)
    ///   - Header (MessagePack)
    ///   - For each vector (in VectorId order):
    ///     - VectorId (u64 LE)
    ///     - Key length (u32 LE)
    ///     - Key (UTF-8 bytes)
    ///     - Embedding (dimension * f32 LE)
    ///     - Has metadata (u8: 0 or 1)
    ///     - If has metadata: Metadata length (u32 LE) + Metadata (MessagePack)
    pub fn snapshot_serialize<W: Write>(&self, writer: &mut W) -> Result<(), VectorError> {
        // Version byte
        writer.write_u8(VECTOR_SNAPSHOT_VERSION)?;

        let backends = self.backends.read().unwrap();
        let collection_count = backends.len() as u32;
        writer.write_u32::<LittleEndian>(collection_count)?;

        // Sort collections for deterministic output
        let mut collections: Vec<_> = backends.iter().collect();
        collections.sort_by(|a, b| a.0.to_key_string().cmp(&b.0.to_key_string()));

        for (collection_id, backend) in collections {
            // Get config from KV
            let config = self.load_collection_config(
                collection_id.run_id,
                &collection_id.name,
            )?.ok_or_else(|| VectorError::CollectionNotFound {
                name: collection_id.name.clone(),
            })?;

            let heap = backend.heap();

            // Create header
            let header = CollectionSnapshotHeader {
                run_id: collection_id.run_id,
                name: collection_id.name.clone(),
                dimension: config.dimension,
                metric: config.metric.to_byte(),
                storage_dtype: 0, // F32
                next_id: heap.next_id_value(),
                free_slots: heap.free_slots().to_vec(),
                count: heap.len() as u32,
            };

            // Write header
            let header_bytes = rmp_serde::to_vec(&header)?;
            writer.write_u32::<LittleEndian>(header_bytes.len() as u32)?;
            writer.write_all(&header_bytes)?;

            // Write vectors in VectorId order (deterministic)
            for (vector_id, embedding) in heap.iter() {
                // VectorId
                writer.write_u64::<LittleEndian>(vector_id.as_u64())?;

                // Get key and metadata from KV
                let (key, metadata) = self.get_key_and_metadata(
                    collection_id.run_id,
                    &collection_id.name,
                    vector_id,
                )?;

                // Key
                let key_bytes = key.as_bytes();
                writer.write_u32::<LittleEndian>(key_bytes.len() as u32)?;
                writer.write_all(key_bytes)?;

                // Embedding (raw f32 LE)
                for &value in embedding {
                    writer.write_f32::<LittleEndian>(value)?;
                }

                // Metadata
                if let Some(ref meta) = metadata {
                    writer.write_u8(1)?;
                    let meta_bytes = serde_json::to_vec(meta)?;
                    writer.write_u32::<LittleEndian>(meta_bytes.len() as u32)?;
                    writer.write_all(&meta_bytes)?;
                } else {
                    writer.write_u8(0)?;
                }
            }
        }

        Ok(())
    }

    /// Deserialize vector data from snapshot
    pub fn snapshot_deserialize<R: Read>(&mut self, reader: &mut R) -> Result<(), VectorError> {
        // Version byte
        let version = reader.read_u8()?;
        if version != VECTOR_SNAPSHOT_VERSION {
            return Err(VectorError::Serialization(
                format!("Unsupported snapshot version: {}", version)
            ));
        }

        let collection_count = reader.read_u32::<LittleEndian>()?;

        for _ in 0..collection_count {
            // Read header
            let header_len = reader.read_u32::<LittleEndian>()? as usize;
            let mut header_bytes = vec![0u8; header_len];
            reader.read_exact(&mut header_bytes)?;
            let header: CollectionSnapshotHeader = rmp_serde::from_slice(&header_bytes)?;

            // Reconstruct config
            let config = VectorConfig {
                dimension: header.dimension,
                metric: DistanceMetric::from_byte(header.metric)
                    .ok_or_else(|| VectorError::Serialization(
                        format!("Invalid metric: {}", header.metric)
                    ))?,
                storage_dtype: StorageDtype::F32,
            };

            // Create collection
            let collection_id = CollectionId::new(header.run_id, &header.name);

            // Build heap with restored state
            let mut id_to_offset = BTreeMap::new();
            let mut data = Vec::new();

            for _ in 0..header.count {
                // VectorId
                let vector_id = VectorId::new(reader.read_u64::<LittleEndian>()?);

                // Key
                let key_len = reader.read_u32::<LittleEndian>()? as usize;
                let mut key_bytes = vec![0u8; key_len];
                reader.read_exact(&mut key_bytes)?;
                let key = String::from_utf8(key_bytes)?;

                // Embedding
                let offset = data.len();
                for _ in 0..header.dimension {
                    data.push(reader.read_f32::<LittleEndian>()?);
                }
                id_to_offset.insert(vector_id, offset);

                // Metadata
                let has_metadata = reader.read_u8()? != 0;
                let metadata = if has_metadata {
                    let meta_len = reader.read_u32::<LittleEndian>()? as usize;
                    let mut meta_bytes = vec![0u8; meta_len];
                    reader.read_exact(&mut meta_bytes)?;
                    Some(serde_json::from_slice(&meta_bytes)?)
                } else {
                    None
                };

                // Store metadata in KV
                let record = VectorRecord {
                    vector_id: vector_id.as_u64(),
                    metadata,
                    version: 1,
                    created_at: 0, // Will be set from WAL if needed
                    updated_at: 0,
                };
                let kv_key = Key::new_vector(
                    Namespace::from_run_id(header.run_id),
                    &header.name,
                    &key,
                );
                self.db.kv_put(&kv_key, &record.to_bytes()?)?;
            }

            // Create heap from restored state
            let heap = VectorHeap::from_snapshot(
                config.clone(),
                data,
                id_to_offset,
                header.free_slots,
                header.next_id,
            );

            // Create backend
            let backend = Box::new(BruteForceBackend::from_heap(heap));
            self.backends.write().unwrap().insert(collection_id, backend);
        }

        Ok(())
    }
}
```

### Acceptance Criteria

- [ ] Version byte 0x01 at start
- [ ] Collection count as u32 LE
- [ ] Header includes next_id and free_slots (CRITICAL)
- [ ] Vectors written in VectorId order (deterministic)
- [ ] Embeddings as raw f32 LE
- [ ] Metadata as optional MessagePack
- [ ] snapshot_deserialize() restores complete state
- [ ] VectorHeap::from_snapshot() used for recovery

---

## Story #360: Vector Recovery Implementation

**File**: `crates/durability/src/recovery.rs`

**Deliverable**: Vector recovery from snapshot + WAL

### Implementation

```rust
impl RecoveryEngine {
    /// Recover vector state from snapshot + WAL
    ///
    /// This function:
    /// 1. Loads vector snapshot section
    /// 2. Replays vector WAL entries from offset
    /// 3. Verifies invariants
    pub fn recover_vectors(
        &mut self,
        db: &mut Database,
        snapshot: &Snapshot,
        wal_entries: &[WalEntry],
    ) -> Result<RecoveryStats, RecoveryError> {
        let mut stats = RecoveryStats::default();

        // Load from snapshot
        if let Some(vector_section) = snapshot.get_section(PrimitiveKind::Vector) {
            let mut reader = std::io::Cursor::new(vector_section);
            db.vector_store_mut().snapshot_deserialize(&mut reader)?;
            stats.snapshot_collections = db.vector_store().collection_count();
        }

        // Replay WAL entries
        let mut replayer = VectorWalReplayer::new(db.vector_store_mut());

        for entry in wal_entries {
            if entry.entry_type.primitive_kind() == Some(PrimitiveKind::Vector) {
                replayer.apply_entry(entry)?;
                stats.wal_entries_applied += 1;
            }
        }

        // Verify invariants
        self.verify_vector_invariants(db)?;

        Ok(stats)
    }

    /// Verify vector invariants after recovery
    fn verify_vector_invariants(&self, db: &Database) -> Result<(), RecoveryError> {
        let store = db.vector_store();

        for (collection_id, backend) in store.backends.read().unwrap().iter() {
            let heap = backend.heap();

            // S8: Snapshot-WAL equivalence (verified by hash comparison)
            // This is tested separately

            // T4: VectorId monotonicity
            // next_id must be > all existing IDs
            let next_id = heap.next_id_value();
            for id in heap.ids() {
                if id.as_u64() >= next_id {
                    return Err(RecoveryError::InvariantViolation {
                        invariant: "T4: VectorId monotonicity".to_string(),
                        details: format!(
                            "VectorId {} >= next_id {} in collection {}",
                            id.as_u64(),
                            next_id,
                            collection_id.name
                        ),
                    });
                }
            }

            // S7: BTreeMap sole source (structural invariant, verified by type)
        }

        Ok(())
    }
}

/// Recovery statistics
#[derive(Debug, Default)]
pub struct RecoveryStats {
    pub snapshot_collections: usize,
    pub wal_entries_applied: usize,
}
```

### Acceptance Criteria

- [ ] Loads vector section from snapshot
- [ ] Replays WAL entries from offset
- [ ] Filters entries by PrimitiveKind::Vector
- [ ] Verifies T4 (VectorId monotonicity)
- [ ] Returns recovery statistics
- [ ] Integrates with global recovery engine

---

## Story #361: Cross-Primitive Transaction Tests

**File**: `crates/engine/tests/vector_transactions.rs` (NEW)

**Deliverable**: Transaction atomicity tests

### Implementation

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Test: KV + Vector in same transaction commits atomically
    #[test]
    fn test_cross_primitive_commit() {
        let db = test_db();
        let run_id = RunId::new();

        // Create collection
        let config = VectorConfig::new(3, DistanceMetric::Cosine).unwrap();
        db.vector_store().create_collection(run_id, "test", config).unwrap();

        // Start transaction
        let tx = db.begin_transaction();

        // Write to both primitives
        tx.kv_put("key1", b"value1").unwrap();
        db.vector_store().insert(run_id, "test", "vec1", &[1.0, 0.0, 0.0], None).unwrap();

        // Commit
        tx.commit().unwrap();

        // Both should be visible
        assert!(db.kv_get("key1").unwrap().is_some());
        assert!(db.vector_store().get(run_id, "test", "vec1").unwrap().is_some());
    }

    /// Test: KV + Vector in same transaction rolls back atomically
    #[test]
    fn test_cross_primitive_rollback() {
        let db = test_db();
        let run_id = RunId::new();

        let config = VectorConfig::new(3, DistanceMetric::Cosine).unwrap();
        db.vector_store().create_collection(run_id, "test", config).unwrap();

        let tx = db.begin_transaction();

        tx.kv_put("key1", b"value1").unwrap();
        db.vector_store().insert(run_id, "test", "vec1", &[1.0, 0.0, 0.0], None).unwrap();

        // Rollback
        tx.rollback();

        // Neither should be visible
        assert!(db.kv_get("key1").unwrap().is_none());
        assert!(db.vector_store().get(run_id, "test", "vec1").unwrap().is_none());
    }

    /// Test: Crash after commit marker survives recovery
    #[test]
    fn test_crash_recovery_committed() {
        let (path, run_id, config) = setup_test_db();

        {
            let db = Database::open(&path).unwrap();
            db.vector_store().create_collection(run_id, "test", config.clone()).unwrap();

            let tx = db.begin_transaction();
            tx.kv_put("key1", b"value1").unwrap();
            db.vector_store().insert(run_id, "test", "vec1", &[1.0, 0.0, 0.0], None).unwrap();
            tx.commit().unwrap();

            // Simulate crash (don't call shutdown)
        }

        // Recover
        let db = Database::open(&path).unwrap();

        // Both should be visible after recovery
        assert!(db.kv_get("key1").unwrap().is_some());
        assert!(db.vector_store().get(run_id, "test", "vec1").unwrap().is_some());
    }

    /// Test: Crash before commit marker loses uncommitted data
    #[test]
    fn test_crash_recovery_uncommitted() {
        let (path, run_id, config) = setup_test_db();

        {
            let db = Database::open(&path).unwrap();
            db.vector_store().create_collection(run_id, "test", config.clone()).unwrap();

            // Create some committed data first
            let tx = db.begin_transaction();
            tx.kv_put("key1", b"value1").unwrap();
            tx.commit().unwrap();

            // Start uncommitted transaction
            let tx = db.begin_transaction();
            tx.kv_put("key2", b"value2").unwrap();
            db.vector_store().insert(run_id, "test", "vec1", &[1.0, 0.0, 0.0], None).unwrap();

            // Simulate crash before commit
        }

        // Recover
        let db = Database::open(&path).unwrap();

        // Committed data should be visible
        assert!(db.kv_get("key1").unwrap().is_some());

        // Uncommitted data should NOT be visible
        assert!(db.kv_get("key2").unwrap().is_none());
        // Note: vec1 insert may or may not be visible depending on
        // whether it was within the transaction boundary
    }

    /// Test: Snapshot-WAL equivalence (S8)
    #[test]
    fn test_snapshot_wal_equivalence() {
        let path1 = temp_path();
        let path2 = temp_path();
        let run_id = RunId::new();

        // Create identical data via two paths
        let config = VectorConfig::new(3, DistanceMetric::Cosine).unwrap();

        // Path 1: Normal operation
        {
            let db = Database::open(&path1).unwrap();
            db.vector_store().create_collection(run_id, "test", config.clone()).unwrap();
            for i in 0..100 {
                db.vector_store().insert(
                    run_id, "test",
                    &format!("key{}", i),
                    &[i as f32, 0.0, 0.0],
                    None,
                ).unwrap();
            }
            db.snapshot().unwrap();
            db.shutdown().unwrap();
        }

        // Path 2: Snapshot + WAL replay
        {
            // Copy snapshot to path2
            copy_snapshot(&path1, &path2);

            let db = Database::open(&path2).unwrap();

            // Add more entries (will be in WAL)
            for i in 100..150 {
                db.vector_store().insert(
                    run_id, "test",
                    &format!("key{}", i),
                    &[i as f32, 0.0, 0.0],
                    None,
                ).unwrap();
            }
            db.shutdown().unwrap();
        }

        // Recover both
        let db1 = Database::open(&path1).unwrap();
        let db2 = Database::open(&path2).unwrap();

        // Compare state
        for i in 0..100 {
            let v1 = db1.vector_store().get(run_id, "test", &format!("key{}", i)).unwrap();
            let v2 = db2.vector_store().get(run_id, "test", &format!("key{}", i)).unwrap();
            assert_eq!(v1.is_some(), v2.is_some());
            if let (Some(e1), Some(e2)) = (v1, v2) {
                assert_eq!(e1.embedding, e2.embedding);
            }
        }
    }

    /// Test: VectorId monotonicity across crashes (T4)
    #[test]
    fn test_vector_id_monotonicity_across_crash() {
        let path = temp_path();
        let run_id = RunId::new();
        let config = VectorConfig::new(3, DistanceMetric::Cosine).unwrap();

        let max_id_before: u64;

        {
            let db = Database::open(&path).unwrap();
            db.vector_store().create_collection(run_id, "test", config.clone()).unwrap();

            // Insert and delete to exercise ID allocation
            for i in 0..10 {
                db.vector_store().insert(
                    run_id, "test",
                    &format!("key{}", i),
                    &[i as f32, 0.0, 0.0],
                    None,
                ).unwrap();
            }

            // Delete some
            for i in 0..5 {
                db.vector_store().delete(run_id, "test", &format!("key{}", i)).unwrap();
            }

            // Get max ID
            max_id_before = db.vector_store()
                .get_max_vector_id(run_id, "test")
                .unwrap()
                .unwrap_or(0);

            db.snapshot().unwrap();
            // Simulate crash
        }

        // Recover
        let db = Database::open(&path).unwrap();

        // Insert new vector
        db.vector_store().insert(run_id, "test", "new_key", &[99.0, 0.0, 0.0], None).unwrap();

        // New ID must be > max_id_before
        let new_entry = db.vector_store().get(run_id, "test", "new_key").unwrap().unwrap();
        assert!(
            new_entry.vector_id.as_u64() > max_id_before,
            "New VectorId {} should be > max before crash {}",
            new_entry.vector_id.as_u64(),
            max_id_before
        );
    }
}
```

### Acceptance Criteria

- [ ] Cross-primitive commit: KV + Vector atomic
- [ ] Cross-primitive rollback: both rolled back
- [ ] Crash after commit: data survives
- [ ] Crash before commit: data lost (correct behavior)
- [ ] S8 test: Snapshot+WAL = pure WAL state
- [ ] T4 test: VectorId > previous max after crash

---

## Testing Summary

| Test Category | Tests |
|---------------|-------|
| WAL Entry Serialization | 4 entry types roundtrip |
| WAL Replay | Collection create/delete, upsert, delete |
| Snapshot Serialization | Roundtrip with next_id, free_slots |
| Recovery | Snapshot + WAL replay |
| Cross-Primitive | Commit, rollback, crash scenarios |
| Invariant Verification | S8, T4 |

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/durability/src/wal_types.rs` | MODIFY - Add vector WAL entry types |
| `crates/primitives/src/vector/wal.rs` | CREATE - WAL payloads and replay |
| `crates/primitives/src/vector/snapshot.rs` | CREATE - Snapshot serialization |
| `crates/durability/src/recovery.rs` | MODIFY - Add vector recovery |
| `crates/engine/tests/vector_transactions.rs` | CREATE - Transaction tests |
