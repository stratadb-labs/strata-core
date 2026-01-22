# Epic 51: Vector Heap & Storage - Implementation Prompts

**Epic Goal**: Implement hybrid storage model (vector heap + KV metadata)

**GitHub Issue**: [#389](https://github.com/anibjoshi/in-mem/issues/389)
**Status**: Ready after Epic 50
**Dependencies**: Epic 50 (Core Types)

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M8_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

### IMPORTANT: Naming Convention

**Do NOT use "M8" or "m8" in the codebase or comments.** M8 is an internal milestone indicator only. In code, use "Vector" prefix instead:
- Module names: `vector`, `vector_heap`, `vector_store`
- Type names: `VectorHeap`, `VectorRecord`, `VectorId`
- Test names: `test_vector_*`, not `test_m8_*`
- Comments: "Vector heap" not "M8 heap"

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M8_ARCHITECTURE.md`
2. **Epic Spec**: `docs/milestones/M8/EPIC_51_VECTOR_HEAP.md`
3. **Prompt Header**: `docs/prompts/M8/M8_PROMPT_HEADER.md` for the 7 architectural rules

---

## Epic 51 Overview

### Scope
- VectorHeap with contiguous Vec<f32> storage
- BTreeMap for deterministic iteration (NOT HashMap)
- Free slot management for storage reuse
- Monotonically increasing VectorId (never recycled)
- VectorRecord KV metadata structure
- TypeTag extensions for Vector primitive

### Critical Invariants

| Invariant | Description |
|-----------|-------------|
| **S4** | VectorIds are NEVER reused (storage slots may be reused) |
| **S7** | id_to_offset BTreeMap is the SOLE source of truth for active vectors |
| **T4** | next_id MUST be persisted in snapshots for crash recovery |

### Key Rules

- **Rule 5**: Use BTreeMap (NOT HashMap) for all ID-to-data mappings
- **Rule 6**: VectorId is never reused

### Component Breakdown
- **Story #399**: VectorHeap Data Structure - CRITICAL
- **Story #400**: VectorHeap Insert/Upsert - CRITICAL
- **Story #401**: VectorHeap Delete with Slot Reuse - CRITICAL
- **Story #402**: VectorHeap Get and Iteration - CRITICAL
- **Story #403**: VectorRecord KV Metadata - HIGH
- **Story #404**: TypeTag Extensions - FOUNDATION

---

## Dependency Graph

```
Story #399 (Data Structure) ──> Story #400 (Insert) ──> Story #401 (Delete)
                                     │
                                     └──> Story #402 (Get/Iterate)

Story #403 (VectorRecord) ──> (independent, parallel with #400-402)

Story #404 (TypeTag) ──> (foundation, can be done first)
```

---

## Story #399: VectorHeap Data Structure

**GitHub Issue**: [#399](https://github.com/anibjoshi/in-mem/issues/399)
**Estimated Time**: 2.5 hours
**Dependencies**: Epic 50 complete
**Blocks**: Stories #400, #401, #402

### Start Story

```bash
gh issue view 399
./scripts/start-story.sh 51 399 vector-heap-struct
```

### Implementation

Create `crates/primitives/src/vector/heap.rs`:

```rust
//! Vector heap for contiguous embedding storage
//!
//! The VectorHeap stores embeddings in a contiguous Vec<f32> for
//! cache-friendly similarity computation. It uses BTreeMap for
//! deterministic iteration order.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::vector::{VectorConfig, VectorError, VectorId, VectorResult};

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
pub struct VectorHeap {
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

    /// Create a heap for recovery (with restored next_id and free_slots)
    ///
    /// CRITICAL: This must be used during recovery to restore the
    /// monotonically increasing VectorId counter. Without this,
    /// newly inserted vectors could reuse IDs from before the crash.
    pub fn for_recovery(
        config: VectorConfig,
        next_id: u64,
        free_slots: Vec<usize>,
    ) -> Self {
        VectorHeap {
            config,
            data: Vec::new(),
            id_to_offset: BTreeMap::new(),
            free_slots,
            next_id: AtomicU64::new(next_id),
            version: AtomicU64::new(0),
        }
    }

    /// Get the configuration
    pub fn config(&self) -> &VectorConfig {
        &self.config
    }

    /// Get the dimension
    pub fn dimension(&self) -> usize {
        self.config.dimension
    }

    /// Get the number of active vectors
    pub fn len(&self) -> usize {
        self.id_to_offset.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.id_to_offset.is_empty()
    }

    /// Get the next_id value (for snapshotting)
    pub fn next_id(&self) -> u64 {
        self.next_id.load(Ordering::SeqCst)
    }

    /// Get the free_slots (for snapshotting)
    pub fn free_slots(&self) -> &[usize] {
        &self.free_slots
    }

    /// Get the current version
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::SeqCst)
    }

    /// Increment version and return new value
    fn bump_version(&self) -> u64 {
        self.version.fetch_add(1, Ordering::SeqCst) + 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::DistanceMetric;

    fn test_config() -> VectorConfig {
        VectorConfig::new(4, DistanceMetric::Cosine).unwrap()
    }

    #[test]
    fn test_new_heap() {
        let heap = VectorHeap::new(test_config());
        assert_eq!(heap.len(), 0);
        assert!(heap.is_empty());
        assert_eq!(heap.dimension(), 4);
        assert_eq!(heap.next_id(), 0);
    }

    #[test]
    fn test_recovery_heap() {
        let free = vec![0, 16, 32];
        let heap = VectorHeap::for_recovery(test_config(), 100, free.clone());
        assert_eq!(heap.next_id(), 100);
        assert_eq!(heap.free_slots(), &free);
    }
}
```

### Acceptance Criteria

- [ ] VectorHeap with BTreeMap (NOT HashMap) for deterministic iteration
- [ ] Contiguous Vec<f32> storage for cache-friendly computation
- [ ] next_id is monotonically increasing
- [ ] free_slots for storage slot reuse
- [ ] Thread-safe with atomic counters
- [ ] `new()` constructor with VectorConfig
- [ ] `for_recovery()` constructor for crash recovery

### Complete Story

```bash
~/.cargo/bin/cargo test --workspace
./scripts/complete-story.sh 399
```

---

## Story #400: VectorHeap Insert/Upsert

**GitHub Issue**: [#400](https://github.com/anibjoshi/in-mem/issues/400)
**Estimated Time**: 2 hours
**Dependencies**: #399
**Blocks**: Epic 52

### Start Story

```bash
gh issue view 400
./scripts/start-story.sh 51 400 heap-insert
```

### Implementation

Add to `crates/primitives/src/vector/heap.rs`:

```rust
impl VectorHeap {
    /// Insert a new vector, returns assigned VectorId
    ///
    /// Reuses free storage slots if available, otherwise appends.
    /// VectorId is monotonically increasing and NEVER reused.
    pub fn insert(&mut self, embedding: &[f32]) -> VectorResult<VectorId> {
        // Validate dimension
        if embedding.len() != self.config.dimension {
            return Err(VectorError::DimensionMismatch {
                expected: self.config.dimension,
                got: embedding.len(),
            });
        }

        // Allocate new VectorId (monotonically increasing, NEVER reused)
        let id = VectorId::new(self.next_id.fetch_add(1, Ordering::SeqCst));

        // Find or create storage slot
        let offset = if let Some(slot) = self.free_slots.pop() {
            // Reuse existing slot
            slot
        } else {
            // Allocate new slot at end
            let offset = self.data.len();
            self.data.resize(offset + self.config.dimension, 0.0);
            offset
        };

        // Copy embedding data
        self.data[offset..offset + self.config.dimension].copy_from_slice(embedding);

        // Record mapping
        self.id_to_offset.insert(id, offset);

        self.bump_version();
        Ok(id)
    }

    /// Insert with a specific VectorId (for WAL replay)
    ///
    /// CRITICAL: Only use during recovery. The caller must ensure
    /// the VectorId is valid and next_id is updated accordingly.
    pub fn insert_with_id(&mut self, id: VectorId, embedding: &[f32]) -> VectorResult<()> {
        if embedding.len() != self.config.dimension {
            return Err(VectorError::DimensionMismatch {
                expected: self.config.dimension,
                got: embedding.len(),
            });
        }

        // Find or create storage slot
        let offset = if let Some(slot) = self.free_slots.pop() {
            slot
        } else {
            let offset = self.data.len();
            self.data.resize(offset + self.config.dimension, 0.0);
            offset
        };

        self.data[offset..offset + self.config.dimension].copy_from_slice(embedding);
        self.id_to_offset.insert(id, offset);

        self.bump_version();
        Ok(())
    }

    /// Update an existing vector's embedding
    pub fn update(&mut self, id: VectorId, embedding: &[f32]) -> VectorResult<()> {
        if embedding.len() != self.config.dimension {
            return Err(VectorError::DimensionMismatch {
                expected: self.config.dimension,
                got: embedding.len(),
            });
        }

        let offset = self.id_to_offset.get(&id)
            .ok_or_else(|| VectorError::Internal(format!("VectorId {} not found", id)))?;

        self.data[*offset..*offset + self.config.dimension].copy_from_slice(embedding);

        self.bump_version();
        Ok(())
    }

    /// Upsert: update existing or insert new (with specific ID for replay)
    pub fn upsert_with_id(&mut self, id: VectorId, embedding: &[f32]) -> VectorResult<()> {
        if self.id_to_offset.contains_key(&id) {
            self.update(id, embedding)
        } else {
            self.insert_with_id(id, embedding)
        }
    }
}

#[cfg(test)]
mod insert_tests {
    use super::*;

    #[test]
    fn test_insert_assigns_monotonic_ids() {
        let mut heap = VectorHeap::new(test_config());

        let id1 = heap.insert(&[1.0, 2.0, 3.0, 4.0]).unwrap();
        let id2 = heap.insert(&[5.0, 6.0, 7.0, 8.0]).unwrap();
        let id3 = heap.insert(&[9.0, 10.0, 11.0, 12.0]).unwrap();

        assert_eq!(id1.as_u64(), 0);
        assert_eq!(id2.as_u64(), 1);
        assert_eq!(id3.as_u64(), 2);
    }

    #[test]
    fn test_insert_dimension_mismatch() {
        let mut heap = VectorHeap::new(test_config());

        let result = heap.insert(&[1.0, 2.0, 3.0]); // 3 instead of 4
        assert!(matches!(result, Err(VectorError::DimensionMismatch { .. })));
    }

    #[test]
    fn test_insert_stores_data() {
        let mut heap = VectorHeap::new(test_config());
        let embedding = [1.0, 2.0, 3.0, 4.0];

        let id = heap.insert(&embedding).unwrap();
        let stored = heap.get(id).unwrap();

        assert_eq!(stored, &embedding);
    }
}
```

### Acceptance Criteria

- [ ] `insert()` allocates new VectorId and returns it
- [ ] Reuses free slots from deleted vectors
- [ ] Falls back to append when no free slots
- [ ] Validates embedding dimension matches config
- [ ] `insert_with_id()` for WAL replay
- [ ] VectorId never reused (Invariant S4)

### Complete Story

```bash
~/.cargo/bin/cargo test --workspace
./scripts/complete-story.sh 400
```

---

## Story #401: VectorHeap Delete with Slot Reuse

**GitHub Issue**: [#401](https://github.com/anibjoshi/in-mem/issues/401)
**Estimated Time**: 1.5 hours
**Dependencies**: #400
**Blocks**: None

### Start Story

```bash
gh issue view 401
./scripts/start-story.sh 51 401 heap-delete
```

### Implementation

Add to `crates/primitives/src/vector/heap.rs`:

```rust
impl VectorHeap {
    /// Delete a vector by ID
    ///
    /// Returns true if the vector existed and was deleted.
    /// The storage slot is added to free_slots for reuse.
    /// The VectorId is NOT recycled (Invariant S4).
    pub fn delete(&mut self, id: VectorId) -> bool {
        if let Some(offset) = self.id_to_offset.remove(&id) {
            // Zero out the embedding data (optional security measure)
            let end = offset + self.config.dimension;
            self.data[offset..end].fill(0.0);

            // Add slot to free list for reuse
            // NOTE: We're reusing the SLOT, not the ID!
            self.free_slots.push(offset);

            self.bump_version();
            true
        } else {
            false
        }
    }

    /// Delete a vector by ID, returning the old embedding if it existed
    pub fn delete_and_get(&mut self, id: VectorId) -> Option<Vec<f32>> {
        if let Some(offset) = self.id_to_offset.remove(&id) {
            let end = offset + self.config.dimension;
            let embedding = self.data[offset..end].to_vec();

            // Zero out and recycle slot
            self.data[offset..end].fill(0.0);
            self.free_slots.push(offset);

            self.bump_version();
            Some(embedding)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod delete_tests {
    use super::*;

    #[test]
    fn test_delete_returns_true_if_exists() {
        let mut heap = VectorHeap::new(test_config());
        let id = heap.insert(&[1.0, 2.0, 3.0, 4.0]).unwrap();

        assert!(heap.delete(id));
        assert!(!heap.contains(id));
    }

    #[test]
    fn test_delete_returns_false_if_not_exists() {
        let mut heap = VectorHeap::new(test_config());
        assert!(!heap.delete(VectorId::new(999)));
    }

    #[test]
    fn test_delete_adds_slot_to_free_list() {
        let mut heap = VectorHeap::new(test_config());
        let id = heap.insert(&[1.0, 2.0, 3.0, 4.0]).unwrap();

        assert!(heap.free_slots().is_empty());
        heap.delete(id);
        assert_eq!(heap.free_slots().len(), 1);
    }

    #[test]
    fn test_slot_reuse_after_delete() {
        let mut heap = VectorHeap::new(test_config());

        // Insert three vectors
        let id1 = heap.insert(&[1.0, 2.0, 3.0, 4.0]).unwrap();
        let _id2 = heap.insert(&[5.0, 6.0, 7.0, 8.0]).unwrap();

        // Delete first
        heap.delete(id1);

        // Insert new - should reuse slot
        let id3 = heap.insert(&[9.0, 10.0, 11.0, 12.0]).unwrap();

        // IDs are NEVER reused
        assert_ne!(id1, id3);
        assert_eq!(id3.as_u64(), 2); // Monotonic

        // But slot was reused (free_slots should be empty again)
        assert!(heap.free_slots().is_empty());
    }

    #[test]
    fn test_vector_id_never_reused() {
        let mut heap = VectorHeap::new(test_config());

        // Insert, delete, insert pattern
        let id1 = heap.insert(&[1.0, 2.0, 3.0, 4.0]).unwrap();
        heap.delete(id1);
        let id2 = heap.insert(&[5.0, 6.0, 7.0, 8.0]).unwrap();

        // ID must be different (monotonically increasing)
        assert!(id2.as_u64() > id1.as_u64());
    }
}
```

### Acceptance Criteria

- [ ] `delete()` removes from id_to_offset map
- [ ] Storage slot added to free_slots for reuse
- [ ] Returns bool indicating if vector existed
- [ ] Optionally zeros embedding data
- [ ] Increments version counter
- [ ] VectorId NOT added to any reuse pool (IDs never recycled)

### Complete Story

```bash
~/.cargo/bin/cargo test --workspace
./scripts/complete-story.sh 401
```

---

## Story #402: VectorHeap Get and Iteration

**GitHub Issue**: [#402](https://github.com/anibjoshi/in-mem/issues/402)
**Estimated Time**: 1.5 hours
**Dependencies**: #399
**Blocks**: Epic 52

### Start Story

```bash
gh issue view 402
./scripts/start-story.sh 51 402 heap-get-iter
```

### Implementation

Add to `crates/primitives/src/vector/heap.rs`:

```rust
impl VectorHeap {
    /// Get embedding by VectorId
    pub fn get(&self, id: VectorId) -> Option<&[f32]> {
        self.id_to_offset.get(&id).map(|&offset| {
            &self.data[offset..offset + self.config.dimension]
        })
    }

    /// Check if vector exists
    pub fn contains(&self, id: VectorId) -> bool {
        self.id_to_offset.contains_key(&id)
    }

    /// Iterate all vectors in deterministic order (by VectorId)
    ///
    /// BTreeMap guarantees sorted iteration by VectorId.
    /// This is CRITICAL for deterministic search results.
    pub fn iter(&self) -> impl Iterator<Item = (VectorId, &[f32])> {
        self.id_to_offset.iter().map(|(&id, &offset)| {
            (id, &self.data[offset..offset + self.config.dimension])
        })
    }

    /// Get all VectorIds in sorted order
    pub fn ids(&self) -> impl Iterator<Item = VectorId> + '_ {
        self.id_to_offset.keys().copied()
    }
}

#[cfg(test)]
mod get_iter_tests {
    use super::*;

    #[test]
    fn test_get_existing() {
        let mut heap = VectorHeap::new(test_config());
        let embedding = [1.0, 2.0, 3.0, 4.0];
        let id = heap.insert(&embedding).unwrap();

        assert_eq!(heap.get(id), Some(embedding.as_slice()));
    }

    #[test]
    fn test_get_nonexistent() {
        let heap = VectorHeap::new(test_config());
        assert_eq!(heap.get(VectorId::new(999)), None);
    }

    #[test]
    fn test_contains() {
        let mut heap = VectorHeap::new(test_config());
        let id = heap.insert(&[1.0, 2.0, 3.0, 4.0]).unwrap();

        assert!(heap.contains(id));
        assert!(!heap.contains(VectorId::new(999)));
    }

    #[test]
    fn test_iter_deterministic_order() {
        let mut heap = VectorHeap::new(test_config());

        // Insert in arbitrary order
        let id3 = heap.insert(&[9.0, 10.0, 11.0, 12.0]).unwrap();
        let id1 = heap.insert(&[1.0, 2.0, 3.0, 4.0]).unwrap();
        let id2 = heap.insert(&[5.0, 6.0, 7.0, 8.0]).unwrap();

        // Delete middle one
        heap.delete(id1);

        // Iteration should be in VectorId order (0, 2) since 1 was deleted
        // But wait - IDs are assigned in insertion order: 0, 1, 2
        // So after deleting id1 (which is 1), we have 0 and 2
        let ids: Vec<_> = heap.iter().map(|(id, _)| id).collect();

        // Should be sorted by VectorId
        for window in ids.windows(2) {
            assert!(window[0] < window[1], "IDs not in sorted order");
        }
    }

    #[test]
    fn test_iter_returns_correct_embeddings() {
        let mut heap = VectorHeap::new(test_config());

        let e1 = [1.0, 2.0, 3.0, 4.0];
        let e2 = [5.0, 6.0, 7.0, 8.0];

        let id1 = heap.insert(&e1).unwrap();
        let id2 = heap.insert(&e2).unwrap();

        let results: Vec<_> = heap.iter().collect();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0], (id1, e1.as_slice()));
        assert_eq!(results[1], (id2, e2.as_slice()));
    }
}
```

### Acceptance Criteria

- [ ] `get()` returns slice reference to embedding
- [ ] `contains()` checks existence
- [ ] `iter()` returns deterministic order (BTreeMap sorted by VectorId)
- [ ] `ids()` returns sorted VectorIds
- [ ] Zero-copy slice references (no cloning)

### Complete Story

```bash
~/.cargo/bin/cargo test --workspace
./scripts/complete-story.sh 402
```

---

## Story #403: VectorRecord KV Metadata

**GitHub Issue**: [#403](https://github.com/anibjoshi/in-mem/issues/403)
**Estimated Time**: 2 hours
**Dependencies**: Epic 50
**Blocks**: Epic 53, Epic 54

### Start Story

```bash
gh issue view 403
./scripts/start-story.sh 51 403 vector-record
```

### Implementation

Create `crates/primitives/src/vector/record.rs`:

```rust
//! KV-backed metadata for vectors
//!
//! VectorRecord stores metadata in the KV store, while the actual
//! embedding data is stored in VectorHeap.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::vector::{VectorError, VectorId, VectorResult};

/// Metadata stored in KV for each vector
///
/// Embedding data is in VectorHeap, not here.
/// This separation allows efficient similarity search (contiguous embeddings)
/// while still supporting rich metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorRecord {
    /// User-provided key (unique within collection)
    pub key: String,

    /// Internal ID (for heap lookup)
    pub vector_id: VectorId,

    /// Optional JSON metadata
    #[serde(skip_serializing_if = "Option::is_none")]
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
    pub fn new(key: String, vector_id: VectorId, metadata: Option<JsonValue>) -> Self {
        let now = Self::now_micros();
        VectorRecord {
            key,
            vector_id,
            metadata,
            version: 1,
            created_at: now,
            updated_at: now,
        }
    }

    /// Update the record (increments version, updates timestamp)
    pub fn update(&mut self, metadata: Option<JsonValue>) {
        self.metadata = metadata;
        self.version += 1;
        self.updated_at = Self::now_micros();
    }

    /// Serialize for KV storage
    pub fn to_bytes(&self) -> VectorResult<Vec<u8>> {
        // Use a compact binary format
        let mut buf = Vec::new();

        // Version byte for format evolution
        buf.push(0x01);

        // key length (u16) + key bytes
        let key_bytes = self.key.as_bytes();
        if key_bytes.len() > u16::MAX as usize {
            return Err(VectorError::InvalidKey {
                key: self.key.clone(),
                reason: "Key too long".to_string(),
            });
        }
        buf.extend_from_slice(&(key_bytes.len() as u16).to_le_bytes());
        buf.extend_from_slice(key_bytes);

        // vector_id (u64)
        buf.extend_from_slice(&self.vector_id.as_u64().to_le_bytes());

        // version (u64)
        buf.extend_from_slice(&self.version.to_le_bytes());

        // created_at (u64)
        buf.extend_from_slice(&self.created_at.to_le_bytes());

        // updated_at (u64)
        buf.extend_from_slice(&self.updated_at.to_le_bytes());

        // metadata: has_metadata (u8) + optional JSON bytes
        if let Some(ref meta) = self.metadata {
            buf.push(1);
            let meta_bytes = serde_json::to_vec(meta)
                .map_err(|e| VectorError::Serialization(e.to_string()))?;
            buf.extend_from_slice(&(meta_bytes.len() as u32).to_le_bytes());
            buf.extend_from_slice(&meta_bytes);
        } else {
            buf.push(0);
        }

        Ok(buf)
    }

    /// Deserialize from KV storage
    pub fn from_bytes(bytes: &[u8]) -> VectorResult<Self> {
        if bytes.is_empty() {
            return Err(VectorError::Serialization("Empty bytes".to_string()));
        }

        let mut pos = 0;

        // Version byte
        let version_byte = bytes[pos];
        pos += 1;

        if version_byte != 0x01 {
            return Err(VectorError::Serialization(
                format!("Unknown format version: {}", version_byte)
            ));
        }

        // Key
        if pos + 2 > bytes.len() {
            return Err(VectorError::Serialization("Truncated key length".to_string()));
        }
        let key_len = u16::from_le_bytes([bytes[pos], bytes[pos + 1]]) as usize;
        pos += 2;

        if pos + key_len > bytes.len() {
            return Err(VectorError::Serialization("Truncated key".to_string()));
        }
        let key = String::from_utf8(bytes[pos..pos + key_len].to_vec())
            .map_err(|e| VectorError::Serialization(e.to_string()))?;
        pos += key_len;

        // Vector ID
        if pos + 8 > bytes.len() {
            return Err(VectorError::Serialization("Truncated vector_id".to_string()));
        }
        let vector_id = VectorId::new(u64::from_le_bytes(
            bytes[pos..pos + 8].try_into().unwrap()
        ));
        pos += 8;

        // Version
        if pos + 8 > bytes.len() {
            return Err(VectorError::Serialization("Truncated version".to_string()));
        }
        let version = u64::from_le_bytes(bytes[pos..pos + 8].try_into().unwrap());
        pos += 8;

        // Created at
        if pos + 8 > bytes.len() {
            return Err(VectorError::Serialization("Truncated created_at".to_string()));
        }
        let created_at = u64::from_le_bytes(bytes[pos..pos + 8].try_into().unwrap());
        pos += 8;

        // Updated at
        if pos + 8 > bytes.len() {
            return Err(VectorError::Serialization("Truncated updated_at".to_string()));
        }
        let updated_at = u64::from_le_bytes(bytes[pos..pos + 8].try_into().unwrap());
        pos += 8;

        // Metadata
        if pos >= bytes.len() {
            return Err(VectorError::Serialization("Truncated has_metadata".to_string()));
        }
        let has_metadata = bytes[pos] != 0;
        pos += 1;

        let metadata = if has_metadata {
            if pos + 4 > bytes.len() {
                return Err(VectorError::Serialization("Truncated metadata length".to_string()));
            }
            let meta_len = u32::from_le_bytes(
                bytes[pos..pos + 4].try_into().unwrap()
            ) as usize;
            pos += 4;

            if pos + meta_len > bytes.len() {
                return Err(VectorError::Serialization("Truncated metadata".to_string()));
            }
            let meta: JsonValue = serde_json::from_slice(&bytes[pos..pos + meta_len])
                .map_err(|e| VectorError::Serialization(e.to_string()))?;
            Some(meta)
        } else {
            None
        };

        Ok(VectorRecord {
            key,
            vector_id,
            metadata,
            version,
            created_at,
            updated_at,
        })
    }

    fn now_micros() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_micros() as u64)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_record_roundtrip_no_metadata() {
        let record = VectorRecord::new(
            "test_key".to_string(),
            VectorId::new(42),
            None,
        );

        let bytes = record.to_bytes().unwrap();
        let restored = VectorRecord::from_bytes(&bytes).unwrap();

        assert_eq!(restored.key, "test_key");
        assert_eq!(restored.vector_id, VectorId::new(42));
        assert!(restored.metadata.is_none());
        assert_eq!(restored.version, 1);
    }

    #[test]
    fn test_record_roundtrip_with_metadata() {
        let metadata = json!({
            "category": "document",
            "year": 2024,
            "tags": ["ai", "ml"]
        });

        let record = VectorRecord::new(
            "doc_123".to_string(),
            VectorId::new(100),
            Some(metadata.clone()),
        );

        let bytes = record.to_bytes().unwrap();
        let restored = VectorRecord::from_bytes(&bytes).unwrap();

        assert_eq!(restored.key, "doc_123");
        assert_eq!(restored.metadata, Some(metadata));
    }

    #[test]
    fn test_record_update() {
        let mut record = VectorRecord::new(
            "key".to_string(),
            VectorId::new(1),
            None,
        );

        let original_created = record.created_at;
        let original_updated = record.updated_at;

        std::thread::sleep(std::time::Duration::from_millis(1));
        record.update(Some(json!({"new": "data"})));

        assert_eq!(record.version, 2);
        assert_eq!(record.created_at, original_created);
        assert!(record.updated_at >= original_updated);
    }
}
```

### Acceptance Criteria

- [ ] VectorRecord with key, vector_id, metadata, version, timestamps
- [ ] Serialization to/from bytes for KV storage
- [ ] Compact binary format
- [ ] Version field for optimistic concurrency
- [ ] Timestamps in microseconds
- [ ] Round-trip tests

### Complete Story

```bash
~/.cargo/bin/cargo test --workspace
./scripts/complete-story.sh 403
```

---

## Story #404: TypeTag Extensions

**GitHub Issue**: [#404](https://github.com/anibjoshi/in-mem/issues/404)
**Estimated Time**: 1 hour
**Dependencies**: None
**Blocks**: Epic 53

### Start Story

```bash
gh issue view 404
./scripts/start-story.sh 51 404 type-tag
```

### Implementation

Modify `crates/primitives/src/core/type_tag.rs` (or equivalent):

```rust
/// Type tags for internal storage keys
///
/// Tags in 0x50-0x5F range are reserved for Vector primitive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum TypeTag {
    // ... existing tags ...

    /// Vector collection config
    VectorCollection = 0x50,

    /// Vector record (key -> VectorRecord)
    VectorRecord = 0x51,

    /// Vector collection index (for listing)
    VectorCollectionIndex = 0x52,
}

impl TypeTag {
    /// Check if this tag is for Vector primitive
    pub fn is_vector(&self) -> bool {
        matches!(
            self,
            TypeTag::VectorCollection |
            TypeTag::VectorRecord |
            TypeTag::VectorCollectionIndex
        )
    }

    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            // ... existing ...
            0x50 => Some(TypeTag::VectorCollection),
            0x51 => Some(TypeTag::VectorRecord),
            0x52 => Some(TypeTag::VectorCollectionIndex),
            _ => None,
        }
    }

    pub fn to_byte(&self) -> u8 {
        *self as u8
    }
}
```

### Acceptance Criteria

- [ ] TypeTag::VectorCollection for collection configs
- [ ] TypeTag::VectorRecord for vector metadata
- [ ] TypeTag::VectorCollectionIndex for collection listing
- [ ] `is_vector()` helper method
- [ ] Tags in 0x50-0x5F range (reserved for Vector)

### Complete Story

```bash
~/.cargo/bin/cargo test --workspace
./scripts/complete-story.sh 404
```

---

## Epic 51 Completion Checklist

### Validation

```bash
# Full test suite
~/.cargo/bin/cargo test --workspace

# Run heap-specific tests
~/.cargo/bin/cargo test vector::heap

# Verify BTreeMap determinism
~/.cargo/bin/cargo test iter_deterministic

# Clippy and format
~/.cargo/bin/cargo clippy --workspace -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### Critical Invariant Tests

```rust
#[test]
fn test_invariant_s4_vector_id_never_reused() {
    let mut heap = VectorHeap::new(test_config());
    let mut all_ids = Vec::new();

    for _ in 0..100 {
        let id = heap.insert(&[1.0, 2.0, 3.0, 4.0]).unwrap();
        all_ids.push(id);
    }

    // Delete half
    for id in all_ids.iter().take(50) {
        heap.delete(*id);
    }

    // Insert more
    for _ in 0..50 {
        let id = heap.insert(&[5.0, 6.0, 7.0, 8.0]).unwrap();
        // Must not match any previous ID
        assert!(!all_ids.contains(&id));
        all_ids.push(id);
    }
}

#[test]
fn test_invariant_t4_next_id_persisted() {
    let config = test_config();

    // Simulate: insert 100, get next_id
    let (next_id, free_slots) = {
        let mut heap = VectorHeap::new(config.clone());
        for _ in 0..100 {
            heap.insert(&[1.0, 2.0, 3.0, 4.0]).unwrap();
        }
        // Delete some
        heap.delete(VectorId::new(10));
        heap.delete(VectorId::new(20));

        (heap.next_id(), heap.free_slots().to_vec())
    };

    // Recovery: restore with persisted values
    let mut heap = VectorHeap::for_recovery(config, next_id, free_slots);

    // New insert must have ID >= next_id
    let new_id = heap.insert(&[9.0, 10.0, 11.0, 12.0]).unwrap();
    assert!(new_id.as_u64() >= next_id);
}
```

### Epic Merge

```bash
git checkout develop
git merge --no-ff epic-51-vector-heap -m "Epic 51: Vector Heap & Storage complete"
git push origin develop

gh issue close 389 --comment "Epic 51 complete. All 6 stories merged and validated."
```

---

*End of Epic 51 Prompts*
