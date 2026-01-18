# Epic 51: Vector Heap & Storage

**Goal**: Implement hybrid storage model (vector heap + KV metadata)

**Dependencies**: Epic 50 (Core Types)

---

## Scope

- VectorHeap with contiguous Vec<f32> storage
- BTreeMap for deterministic iteration (NOT HashMap)
- Free slot management for storage reuse
- Monotonically increasing VectorId (never recycled)
- VectorRecord KV metadata structure
- TypeTag extensions for Vector primitive

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #335 | VectorHeap Data Structure | CRITICAL |
| #336 | VectorHeap Insert/Upsert | CRITICAL |
| #337 | VectorHeap Delete with Slot Reuse | CRITICAL |
| #338 | VectorHeap Get and Iteration | CRITICAL |
| #339 | VectorRecord KV Metadata | HIGH |
| #340 | TypeTag Extensions | FOUNDATION |

---

## Story #335: VectorHeap Data Structure

**File**: `crates/primitives/src/vector/heap.rs` (NEW)

**Deliverable**: Core vector heap data structure

### Implementation

```rust
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// Per-collection vector heap
///
/// Stores embeddings in a contiguous Vec<f32> for cache-friendly
/// similarity computation. Uses BTreeMap for deterministic iteration.
///
/// CRITICAL INVARIANTS:
/// - id_to_offset is the SOLE source of truth for active vectors (S7)
/// - VectorIds are NEVER reused, only storage slots are reused (S4)
/// - next_id is monotonically increasing and MUST be persisted in snapshots (T4)
/// - free_slots MUST be persisted in snapshots for correct recovery
pub(crate) struct VectorHeap {
    /// Collection configuration
    config: VectorConfig,

    /// Contiguous embedding storage
    /// Layout: [v0_dim0, v0_dim1, ..., v0_dimN, v1_dim0, v1_dim1, ...]
    /// Each vector occupies `config.dimension` consecutive f32 values.
    data: Vec<f32>,

    /// VectorId -> offset in data (in floats, not bytes)
    ///
    /// IMPORTANT: Use BTreeMap for deterministic iteration order.
    /// HashMap would cause nondeterministic search results.
    /// This is the SOLE source of truth for active vectors.
    id_to_offset: BTreeMap<VectorId, usize>,

    /// Free list for deleted storage slots (enables slot reuse)
    ///
    /// When a vector is deleted, its storage slot offset is added here.
    /// New inserts can reuse these slots to avoid unbounded memory growth.
    ///
    /// NOTE: Storage slots are reused, but VectorId values are NEVER reused.
    /// This must be persisted in snapshots for correct recovery.
    free_slots: Vec<usize>,

    /// Next VectorId to allocate (monotonically increasing)
    ///
    /// This value is NEVER decremented, even after deletions.
    /// MUST be persisted in snapshots to maintain ID uniqueness across restarts.
    /// Without this, recovery could reuse IDs and break replay determinism.
    next_id: AtomicU64,

    /// Version counter for snapshot consistency
    version: AtomicU64,
}

impl VectorHeap {
    /// Create a new vector heap with the given configuration
    pub fn new(config: VectorConfig) -> Self {
        VectorHeap {
            config,
            data: Vec::new(),
            id_to_offset: BTreeMap::new(),
            free_slots: Vec::new(),
            next_id: AtomicU64::new(0),
            version: AtomicU64::new(0),
        }
    }

    /// Create from snapshot data (for recovery)
    ///
    /// CRITICAL: next_id and free_slots MUST be restored from snapshot
    /// to maintain invariants T4 (VectorId monotonicity across crashes).
    pub fn from_snapshot(
        config: VectorConfig,
        data: Vec<f32>,
        id_to_offset: BTreeMap<VectorId, usize>,
        free_slots: Vec<usize>,
        next_id: u64,
    ) -> Self {
        VectorHeap {
            config,
            data,
            id_to_offset,
            free_slots,
            next_id: AtomicU64::new(next_id),
            version: AtomicU64::new(0),
        }
    }

    /// Get the dimension of vectors in this heap
    pub fn dimension(&self) -> usize {
        self.config.dimension
    }

    /// Get the distance metric
    pub fn metric(&self) -> DistanceMetric {
        self.config.metric
    }

    /// Get the number of active vectors
    pub fn len(&self) -> usize {
        self.id_to_offset.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.id_to_offset.is_empty()
    }

    /// Get current version (for snapshot consistency)
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Acquire)
    }

    /// Get next_id value (for snapshot persistence)
    pub fn next_id_value(&self) -> u64 {
        self.next_id.load(Ordering::Relaxed)
    }

    /// Get free_slots (for snapshot persistence)
    pub fn free_slots(&self) -> &[usize] {
        &self.free_slots
    }

    /// Allocate a new VectorId (monotonically increasing)
    ///
    /// This NEVER returns a previously used ID, even after deletions.
    fn allocate_id(&self) -> VectorId {
        VectorId(self.next_id.fetch_add(1, Ordering::Relaxed))
    }
}
```

### Acceptance Criteria

- [ ] VectorHeap struct with config, data, id_to_offset, free_slots, next_id, version
- [ ] id_to_offset uses BTreeMap (NOT HashMap) for deterministic iteration
- [ ] next_id is AtomicU64, monotonically increasing
- [ ] from_snapshot() restores next_id and free_slots
- [ ] Accessors for dimension, metric, len, version, next_id_value, free_slots

---

## Story #336: VectorHeap Insert/Upsert

**File**: `crates/primitives/src/vector/heap.rs`

**Deliverable**: Upsert operation with slot reuse

### Implementation

```rust
impl VectorHeap {
    /// Insert or update a vector (upsert semantics)
    ///
    /// If the VectorId already exists, updates in place.
    /// If new, allocates a slot (reusing deleted slots if available).
    ///
    /// IMPORTANT: When reusing a slot, MUST copy embedding into that slot.
    /// This was a bug in early design - the embedding must be written to
    /// the reused slot, not appended to the end.
    pub fn upsert(&mut self, id: VectorId, embedding: &[f32]) -> Result<(), VectorError> {
        // Validate dimension
        if embedding.len() != self.config.dimension {
            return Err(VectorError::DimensionMismatch {
                expected: self.config.dimension,
                got: embedding.len(),
            });
        }

        if let Some(&offset) = self.id_to_offset.get(&id) {
            // Update existing vector in place
            let start = offset;
            let end = offset + self.config.dimension;
            self.data[start..end].copy_from_slice(embedding);
        } else {
            // Insert new vector
            let offset = if let Some(slot) = self.free_slots.pop() {
                // Reuse deleted slot
                // CRITICAL: Must copy embedding into the reused slot
                let start = slot;
                let end = slot + self.config.dimension;
                self.data[start..end].copy_from_slice(embedding);
                slot
            } else {
                // Append to end
                let offset = self.data.len();
                self.data.extend_from_slice(embedding);
                offset
            };
            self.id_to_offset.insert(id, offset);
        }

        self.version.fetch_add(1, Ordering::Release);
        Ok(())
    }

    /// Insert a new vector, allocating a new VectorId
    ///
    /// Returns the allocated VectorId.
    pub fn insert(&mut self, embedding: &[f32]) -> Result<VectorId, VectorError> {
        let id = self.allocate_id();
        self.upsert(id, embedding)?;
        Ok(id)
    }

    /// Insert with a specific VectorId (for WAL replay)
    ///
    /// Used during recovery to replay WAL entries with their original IDs.
    /// Updates next_id if necessary to maintain monotonicity.
    pub fn insert_with_id(&mut self, id: VectorId, embedding: &[f32]) -> Result<(), VectorError> {
        // Ensure next_id stays ahead of all assigned IDs
        let id_val = id.as_u64();
        loop {
            let current = self.next_id.load(Ordering::Relaxed);
            if current > id_val {
                break;
            }
            if self.next_id.compare_exchange(
                current,
                id_val + 1,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ).is_ok() {
                break;
            }
        }
        self.upsert(id, embedding)
    }
}
```

### Acceptance Criteria

- [ ] upsert() validates dimension matches config
- [ ] Updates in place if VectorId exists
- [ ] Reuses free slot if available, correctly copying embedding into slot
- [ ] Appends to data if no free slot
- [ ] insert() allocates new VectorId and calls upsert()
- [ ] insert_with_id() for WAL replay, updates next_id to maintain monotonicity
- [ ] Increments version on mutation

---

## Story #337: VectorHeap Delete with Slot Reuse

**File**: `crates/primitives/src/vector/heap.rs`

**Deliverable**: Delete operation with slot recycling

### Implementation

```rust
impl VectorHeap {
    /// Delete a vector by ID
    ///
    /// Returns true if the vector existed and was deleted.
    /// The storage slot is added to free_slots for reuse.
    /// The VectorId is NEVER reused (Invariant S4).
    ///
    /// Security note: Data is zeroed to prevent information leakage.
    pub fn delete(&mut self, id: VectorId) -> bool {
        if let Some(offset) = self.id_to_offset.remove(&id) {
            // Mark slot as free for reuse
            self.free_slots.push(offset);

            // Zero out data (security: prevent information leakage)
            let start = offset;
            let end = offset + self.config.dimension;
            self.data[start..end].fill(0.0);

            self.version.fetch_add(1, Ordering::Release);
            true
        } else {
            false
        }
    }

    /// Delete a vector by ID (for WAL replay, with specific slot handling)
    ///
    /// During WAL replay, we might delete a vector that was inserted
    /// in the same replay sequence. This handles that case correctly.
    pub fn delete_replay(&mut self, id: VectorId) -> bool {
        self.delete(id)
    }

    /// Clear all vectors (for testing or collection deletion)
    pub fn clear(&mut self) {
        self.data.clear();
        self.id_to_offset.clear();
        self.free_slots.clear();
        // Note: next_id is NOT reset - IDs are never reused
        self.version.fetch_add(1, Ordering::Release);
    }
}
```

### Acceptance Criteria

- [ ] delete() removes from id_to_offset
- [ ] Adds offset to free_slots for reuse
- [ ] Zeros data for security
- [ ] Returns true if deleted, false if not found
- [ ] VectorId is NOT added to any recycling pool (never reused)
- [ ] Increments version on mutation
- [ ] clear() clears all data but doesn't reset next_id

---

## Story #338: VectorHeap Get and Iteration

**File**: `crates/primitives/src/vector/heap.rs`

**Deliverable**: Read operations with deterministic ordering

### Implementation

```rust
impl VectorHeap {
    /// Get embedding by VectorId
    ///
    /// Returns None if the vector doesn't exist.
    pub fn get(&self, id: VectorId) -> Option<&[f32]> {
        let offset = *self.id_to_offset.get(&id)?;
        let start = offset;
        let end = offset + self.config.dimension;
        Some(&self.data[start..end])
    }

    /// Check if a vector exists
    pub fn contains(&self, id: VectorId) -> bool {
        self.id_to_offset.contains_key(&id)
    }

    /// Iterate all vectors in deterministic order (sorted by VectorId)
    ///
    /// IMPORTANT: This uses BTreeMap iteration which guarantees sorted order.
    /// This is critical for deterministic brute-force search (Invariant R3).
    /// HashMap iteration would be nondeterministic.
    pub fn iter(&self) -> impl Iterator<Item = (VectorId, &[f32])> {
        // BTreeMap iterates in key order (VectorId ascending)
        self.id_to_offset.iter().map(|(&id, &offset)| {
            let start = offset;
            let end = offset + self.config.dimension;
            (id, &self.data[start..end])
        })
    }

    /// Get all VectorIds in deterministic order
    pub fn ids(&self) -> impl Iterator<Item = VectorId> + '_ {
        self.id_to_offset.keys().copied()
    }

    /// Get raw data slice (for snapshot serialization)
    pub fn raw_data(&self) -> &[f32] {
        &self.data
    }

    /// Get id_to_offset map (for snapshot serialization)
    pub fn id_to_offset_map(&self) -> &BTreeMap<VectorId, usize> {
        &self.id_to_offset
    }
}
```

### Acceptance Criteria

- [ ] get() returns Option<&[f32]> by VectorId
- [ ] contains() checks existence
- [ ] iter() returns vectors in VectorId order (BTreeMap guarantee)
- [ ] ids() returns VectorIds in sorted order
- [ ] raw_data() for snapshot serialization
- [ ] id_to_offset_map() for snapshot serialization

---

## Story #339: VectorRecord KV Metadata

**File**: `crates/primitives/src/vector/types.rs`

**Deliverable**: KV-stored metadata for vectors

### Implementation

```rust
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Metadata stored in KV (MessagePack serialized)
///
/// This is stored separately from the embedding for:
/// 1. Transaction participation (KV has full tx support)
/// 2. Flexible schema (JSON metadata)
/// 3. WAL integration (reuses existing infrastructure)
///
/// The embedding is stored in VectorHeap for cache-friendly scanning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorRecord {
    /// Internal vector ID (maps to VectorHeap)
    pub vector_id: u64,

    /// User-provided metadata (optional)
    pub metadata: Option<JsonValue>,

    /// Version for optimistic concurrency
    pub version: u64,

    /// Creation timestamp (microseconds since epoch)
    pub created_at: u64,

    /// Last update timestamp (microseconds since epoch)
    pub updated_at: u64,
}

impl VectorRecord {
    /// Create a new VectorRecord
    pub fn new(vector_id: VectorId, metadata: Option<JsonValue>) -> Self {
        let now = crate::util::now_micros();
        VectorRecord {
            vector_id: vector_id.as_u64(),
            metadata,
            version: 1,
            created_at: now,
            updated_at: now,
        }
    }

    /// Update metadata and version
    pub fn update(&mut self, metadata: Option<JsonValue>) {
        self.metadata = metadata;
        self.version += 1;
        self.updated_at = crate::util::now_micros();
    }

    /// Get VectorId
    pub fn vector_id(&self) -> VectorId {
        VectorId(self.vector_id)
    }

    /// Serialize to bytes (MessagePack)
    pub fn to_bytes(&self) -> Result<Vec<u8>, VectorError> {
        rmp_serde::to_vec(self)
            .map_err(|e| VectorError::Serialization(e.to_string()))
    }

    /// Deserialize from bytes (MessagePack)
    pub fn from_bytes(data: &[u8]) -> Result<Self, VectorError> {
        rmp_serde::from_slice(data)
            .map_err(|e| VectorError::Serialization(e.to_string()))
    }
}

/// Collection configuration stored in KV
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionRecord {
    /// Collection configuration
    pub config: VectorConfigSerde,

    /// Creation timestamp
    pub created_at: u64,
}

/// Serializable version of VectorConfig
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorConfigSerde {
    pub dimension: usize,
    pub metric: u8,  // DistanceMetric::to_byte()
    pub storage_dtype: u8,  // StorageDtype enum value
}

impl From<&VectorConfig> for VectorConfigSerde {
    fn from(config: &VectorConfig) -> Self {
        VectorConfigSerde {
            dimension: config.dimension,
            metric: config.metric.to_byte(),
            storage_dtype: 0, // F32
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
```

### Acceptance Criteria

- [ ] VectorRecord with vector_id, metadata, version, timestamps
- [ ] Serialization via MessagePack (rmp_serde)
- [ ] new() and update() methods with timestamp handling
- [ ] CollectionRecord for config persistence
- [ ] VectorConfigSerde for serializable config
- [ ] Bidirectional conversion between VectorConfig and VectorConfigSerde

---

## Story #340: TypeTag Extensions

**File**: `crates/core/src/type_tag.rs`

**Deliverable**: TypeTag variants for Vector primitive

### Implementation

```rust
/// Type tag for key prefixing
///
/// Each primitive has a unique type tag that prefixes its keys
/// in the underlying storage, enabling efficient scans and
/// preventing key collisions between primitives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum TypeTag {
    // Existing tags...
    Kv = 0x10,
    Json = 0x20,
    Event = 0x30,
    State = 0x40,
    Trace = 0x50,
    Run = 0x60,

    // M8 additions
    /// Vector metadata (VectorRecord)
    Vector = 0x70,
    /// Vector collection configuration
    VectorConfig = 0x71,
}

impl TypeTag {
    /// Convert to byte for key prefixing
    pub fn to_byte(self) -> u8 {
        self as u8
    }

    /// Parse from byte
    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0x10 => Some(TypeTag::Kv),
            0x20 => Some(TypeTag::Json),
            0x30 => Some(TypeTag::Event),
            0x40 => Some(TypeTag::State),
            0x50 => Some(TypeTag::Trace),
            0x60 => Some(TypeTag::Run),
            0x70 => Some(TypeTag::Vector),
            0x71 => Some(TypeTag::VectorConfig),
            _ => None,
        }
    }
}
```

**File**: `crates/core/src/key.rs`

**Additional implementation**:

```rust
impl Key {
    /// Create key for vector metadata
    /// Format: namespace + TypeTag::Vector + collection_name + "/" + vector_key
    pub fn new_vector(namespace: Namespace, collection: &str, key: &str) -> Self {
        let user_key = format!("{}/{}", collection, key);
        Key::new(namespace, TypeTag::Vector, user_key)
    }

    /// Create key for collection configuration
    /// Format: namespace + TypeTag::VectorConfig + collection_name
    pub fn new_vector_config(namespace: Namespace, collection: &str) -> Self {
        Key::new(namespace, TypeTag::VectorConfig, collection.to_string())
    }

    /// Create prefix for scanning all vectors in a collection
    pub fn vector_collection_prefix(namespace: Namespace, collection: &str) -> Self {
        let user_key = format!("{}/", collection);
        Key::new(namespace, TypeTag::Vector, user_key)
    }
}
```

### Acceptance Criteria

- [ ] TypeTag::Vector = 0x70
- [ ] TypeTag::VectorConfig = 0x71
- [ ] Key::new_vector() for vector metadata keys
- [ ] Key::new_vector_config() for collection config keys
- [ ] Key::vector_collection_prefix() for collection scans
- [ ] from_byte() updated to parse new tags

---

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vector_heap_basic_operations() {
        let config = VectorConfig::for_minilm(); // 384 dims
        let mut heap = VectorHeap::new(config);

        // Insert
        let embedding = vec![0.1; 384];
        let id = heap.insert(&embedding).unwrap();

        // Get
        let retrieved = heap.get(id).unwrap();
        assert_eq!(retrieved.len(), 384);
        assert!((retrieved[0] - 0.1).abs() < f32::EPSILON);

        // Update (upsert)
        let new_embedding = vec![0.2; 384];
        heap.upsert(id, &new_embedding).unwrap();
        let retrieved = heap.get(id).unwrap();
        assert!((retrieved[0] - 0.2).abs() < f32::EPSILON);

        // Delete
        assert!(heap.delete(id));
        assert!(heap.get(id).is_none());
    }

    #[test]
    fn test_vector_id_never_reused() {
        let config = VectorConfig::for_minilm();
        let mut heap = VectorHeap::new(config);

        let embedding = vec![0.1; 384];

        // Insert and delete several times
        let id1 = heap.insert(&embedding).unwrap();
        heap.delete(id1);

        let id2 = heap.insert(&embedding).unwrap();
        heap.delete(id2);

        let id3 = heap.insert(&embedding).unwrap();

        // IDs should be monotonically increasing
        assert!(id1.as_u64() < id2.as_u64());
        assert!(id2.as_u64() < id3.as_u64());
    }

    #[test]
    fn test_slot_reuse() {
        let config = VectorConfig::for_minilm();
        let mut heap = VectorHeap::new(config);

        let embedding = vec![0.1; 384];

        // Insert, then delete to create free slot
        let id1 = heap.insert(&embedding).unwrap();
        let initial_len = heap.raw_data().len();
        heap.delete(id1);

        // Insert again - should reuse slot, not grow data
        let new_embedding = vec![0.2; 384];
        let id2 = heap.insert(&new_embedding).unwrap();

        // Data length should not have grown
        assert_eq!(heap.raw_data().len(), initial_len);

        // New ID should be different
        assert_ne!(id1, id2);

        // New embedding should be in reused slot
        let retrieved = heap.get(id2).unwrap();
        assert!((retrieved[0] - 0.2).abs() < f32::EPSILON);
    }

    #[test]
    fn test_deterministic_iteration() {
        let config = VectorConfig::for_minilm();
        let mut heap = VectorHeap::new(config);

        // Insert in arbitrary order
        let embedding = vec![0.1; 384];
        let id3 = heap.insert(&embedding).unwrap();
        let id1 = heap.insert(&embedding).unwrap();
        let id2 = heap.insert(&embedding).unwrap();

        // Iteration should be in VectorId order
        let ids: Vec<_> = heap.ids().collect();
        for i in 1..ids.len() {
            assert!(ids[i - 1] < ids[i], "IDs should be in sorted order");
        }
    }

    #[test]
    fn test_dimension_validation() {
        let config = VectorConfig::for_minilm(); // 384 dims
        let mut heap = VectorHeap::new(config);

        // Wrong dimension should fail
        let wrong_embedding = vec![0.1; 256];
        let result = heap.insert(&wrong_embedding);
        assert!(matches!(result, Err(VectorError::DimensionMismatch { .. })));
    }

    #[test]
    fn test_snapshot_restore() {
        let config = VectorConfig::for_minilm();
        let mut heap = VectorHeap::new(config.clone());

        // Insert some vectors
        let e1 = vec![0.1; 384];
        let e2 = vec![0.2; 384];
        let id1 = heap.insert(&e1).unwrap();
        let id2 = heap.insert(&e2).unwrap();
        heap.delete(id1); // Create a free slot

        // Capture state for snapshot
        let data = heap.raw_data().to_vec();
        let id_to_offset = heap.id_to_offset_map().clone();
        let free_slots = heap.free_slots().to_vec();
        let next_id = heap.next_id_value();

        // Restore from snapshot
        let restored = VectorHeap::from_snapshot(
            config,
            data,
            id_to_offset,
            free_slots,
            next_id,
        );

        // Verify state
        assert!(restored.get(id1).is_none()); // Deleted
        assert!(restored.get(id2).is_some()); // Exists
        assert_eq!(restored.free_slots().len(), 1); // One free slot

        // New insert should get higher ID
        let mut restored = restored;
        let id3 = restored.insert(&vec![0.3; 384]).unwrap();
        assert!(id3.as_u64() >= next_id, "ID must be >= next_id from snapshot");
    }
}
```

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/primitives/src/vector/heap.rs` | CREATE - VectorHeap implementation |
| `crates/primitives/src/vector/types.rs` | MODIFY - Add VectorRecord, CollectionRecord |
| `crates/core/src/type_tag.rs` | MODIFY - Add Vector, VectorConfig tags |
| `crates/core/src/key.rs` | MODIFY - Add vector key constructors |
