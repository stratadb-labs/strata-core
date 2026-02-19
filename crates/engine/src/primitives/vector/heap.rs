//! Vector Heap - Contiguous embedding storage
//!
//! VectorHeap stores embeddings in a contiguous Vec<f32> for cache-friendly
//! similarity computation. Uses BTreeMap for deterministic iteration order.
//!
//! # Critical Invariants
//!
//! - **S4**: VectorIds are NEVER reused, only storage slots are reused
//! - **S7**: id_to_offset is the SOLE source of truth for active vectors
//! - **T4**: next_id is monotonically increasing and MUST be persisted in snapshots
//! - **R3**: BTreeMap guarantees deterministic iteration order

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::primitives::vector::error::{VectorError, VectorResult};
use crate::primitives::vector::mmap::{self, MmapVectorData};
use crate::primitives::vector::types::{DistanceMetric, VectorConfig, VectorId};

/// Backing storage for vector embeddings.
///
/// - `InMemory`: Heap-allocated `Vec<f32>` (default, mutable).
/// - `Mmap`: Memory-mapped file (read-only, OS-paged).
pub(crate) enum VectorData {
    InMemory(Vec<f32>),
    Mmap(MmapVectorData),
}

/// Per-collection vector heap
///
/// Stores embeddings in a contiguous Vec<f32> for cache-friendly
/// similarity computation. Uses BTreeMap for deterministic iteration.
///
/// # Critical Invariants
///
/// - id_to_offset is the SOLE source of truth for active vectors (S7)
/// - VectorIds are NEVER reused, only storage slots are reused (S4)
/// - next_id is monotonically increasing and MUST be persisted in snapshots (T4)
/// - free_slots MUST be persisted in snapshots for correct recovery
pub struct VectorHeap {
    /// Collection configuration
    config: VectorConfig,

    /// Embedding storage — either heap-allocated or memory-mapped.
    ///
    /// Layout (InMemory): [v0_dim0, v0_dim1, ..., v0_dimN, v1_dim0, ...]
    /// Each vector occupies `config.dimension` consecutive f32 values.
    /// Mmap variant is read-only; mutating methods panic on it.
    data: VectorData,

    /// VectorId -> offset in data (in floats, not bytes)
    ///
    /// IMPORTANT: Use BTreeMap for deterministic iteration order.
    /// HashMap would cause nondeterministic search results.
    /// This is the SOLE source of truth for active vectors.
    ///
    /// For the Mmap variant this mirrors the mmap's id_to_offset.
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
    ///
    /// # Memory Ordering
    ///
    /// Uses Relaxed ordering for fetch_add because:
    /// 1. fetch_add is atomic and guarantees each caller gets a unique value
    /// 2. No other memory operations are synchronized by this counter
    /// 3. The uniqueness guarantee comes from the atomic operation, not ordering
    next_id: AtomicU64,

    /// Version counter for snapshot consistency
    version: AtomicU64,
}

impl VectorHeap {
    /// Create a new vector heap with the given configuration
    ///
    /// Note: next_id starts at 1, not 0, to match expected VectorId semantics
    /// where IDs are positive integers.
    pub fn new(config: VectorConfig) -> Self {
        VectorHeap {
            config,
            data: VectorData::InMemory(Vec::new()),
            id_to_offset: BTreeMap::new(),
            free_slots: Vec::new(),
            next_id: AtomicU64::new(1),
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
            data: VectorData::InMemory(data),
            id_to_offset,
            free_slots,
            next_id: AtomicU64::new(next_id),
            version: AtomicU64::new(0),
        }
    }

    /// Open from an existing mmap file.
    ///
    /// The heap is read-only — mutating methods (`upsert`, `delete`, etc.) will panic.
    /// Use this for disk-backed databases on startup when the `.vec` cache file exists.
    pub fn from_mmap(path: &Path, config: VectorConfig) -> Result<Self, VectorError> {
        let mmap_data = MmapVectorData::open(path, config.dimension)?;
        let id_to_offset = mmap_data.id_to_offset().clone();
        let free_slots = mmap_data.free_slots().to_vec();
        let next_id = mmap_data.next_id();
        Ok(VectorHeap {
            config,
            data: VectorData::Mmap(mmap_data),
            id_to_offset,
            free_slots,
            next_id: AtomicU64::new(next_id),
            version: AtomicU64::new(0),
        })
    }

    /// Get the dimension of vectors in this heap
    pub fn dimension(&self) -> usize {
        self.config.dimension
    }

    /// Get the distance metric
    pub fn metric(&self) -> DistanceMetric {
        self.config.metric
    }

    /// Get the config
    pub fn config(&self) -> &VectorConfig {
        &self.config
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

    /// Restore snapshot state (for recovery)
    ///
    /// Called after all vectors have been inserted with insert_with_id()
    /// to restore the exact next_id and free_slots from the snapshot.
    ///
    /// CRITICAL: This ensures VectorId uniqueness across restarts (T4).
    pub fn restore_snapshot_state(&mut self, next_id: u64, free_slots: Vec<usize>) {
        self.next_id.store(next_id, Ordering::Relaxed);
        self.free_slots = free_slots;
    }

    /// Allocate a new VectorId (monotonically increasing)
    ///
    /// This NEVER returns a previously used ID, even after deletions.
    /// This is the per-collection counter that ensures deterministic
    /// VectorId assignment across separate databases.
    pub fn allocate_id(&self) -> VectorId {
        VectorId::new(self.next_id.fetch_add(1, Ordering::Relaxed))
    }

    // ========================================================================
    // Insert/Upsert Operations
    // ========================================================================

    /// Insert or update a vector (upsert semantics)
    ///
    /// If the VectorId already exists, updates in place.
    /// If new, allocates a slot (reusing deleted slots if available).
    ///
    /// IMPORTANT: When reusing a slot, MUST copy embedding into that slot.
    pub fn upsert(&mut self, id: VectorId, embedding: &[f32]) -> VectorResult<()> {
        // Validate dimension
        let dim = self.config.dimension;
        if embedding.len() != dim {
            return Err(VectorError::DimensionMismatch {
                expected: dim,
                got: embedding.len(),
            });
        }

        let vec = match &mut self.data {
            VectorData::InMemory(v) => v,
            VectorData::Mmap(_) => panic!("cannot mutate mmap-backed VectorHeap"),
        };

        if let Some(&offset) = self.id_to_offset.get(&id) {
            // Update existing vector in place
            vec[offset..offset + dim].copy_from_slice(embedding);
        } else {
            // Insert new vector
            let offset = if let Some(slot) = self.free_slots.pop() {
                // Reuse deleted slot
                // CRITICAL: Must copy embedding into the reused slot
                vec[slot..slot + dim].copy_from_slice(embedding);
                slot
            } else {
                // Append to end
                let offset = vec.len();
                vec.extend_from_slice(embedding);
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
    pub fn insert(&mut self, embedding: &[f32]) -> VectorResult<VectorId> {
        let id = self.allocate_id();
        self.upsert(id, embedding)?;
        Ok(id)
    }

    /// Insert with a specific VectorId (for WAL replay)
    ///
    /// Used during recovery to replay WAL entries with their original IDs.
    /// Updates next_id if necessary to maintain monotonicity.
    pub fn insert_with_id(&mut self, id: VectorId, embedding: &[f32]) -> VectorResult<()> {
        // Ensure next_id stays ahead of all assigned IDs
        let id_val = id.as_u64();
        loop {
            let current = self.next_id.load(Ordering::Relaxed);
            if current > id_val {
                break;
            }
            if self
                .next_id
                .compare_exchange(current, id_val + 1, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
            {
                break;
            }
        }
        self.upsert(id, embedding)
    }

    // ========================================================================
    // Delete Operations
    // ========================================================================

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
            // For mmap-backed heaps the pages are read-only; skip zeroing.
            // The mmap file will be re-frozen without the deleted vector.
            if let VectorData::InMemory(v) = &mut self.data {
                let dim = self.config.dimension;
                v[offset..offset + dim].fill(0.0);
            }

            self.version.fetch_add(1, Ordering::Release);
            true
        } else {
            false
        }
    }

    /// Delete a vector by ID (for WAL replay)
    ///
    /// Same as delete(), but explicitly named for WAL replay context.
    pub fn delete_replay(&mut self, id: VectorId) -> bool {
        self.delete(id)
    }

    /// Clear all vectors (for testing or collection deletion)
    pub fn clear(&mut self) {
        self.data = VectorData::InMemory(Vec::new());
        self.id_to_offset.clear();
        self.free_slots.clear();
        // Note: next_id is NOT reset - IDs are never reused
        self.version.fetch_add(1, Ordering::Release);
    }

    // ========================================================================
    // Read Operations
    // ========================================================================

    /// Get embedding by VectorId
    ///
    /// Returns None if the vector doesn't exist.
    /// Works for both InMemory and Mmap backing.
    pub fn get(&self, id: VectorId) -> Option<&[f32]> {
        // id_to_offset is the sole source of truth for active vectors (S7).
        // For mmap-backed heaps, a vector may have been deleted at runtime
        // (removed from id_to_offset) while still present in the mmap file.
        if !self.id_to_offset.contains_key(&id) {
            return None;
        }
        match &self.data {
            VectorData::InMemory(vec) => {
                let offset = *self.id_to_offset.get(&id)?;
                let start = offset;
                let end = offset + self.config.dimension;
                Some(&vec[start..end])
            }
            VectorData::Mmap(mmap) => mmap.get(id),
        }
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
        let data = &self.data;
        let dim = self.config.dimension;
        // BTreeMap iterates in key order (VectorId ascending)
        self.id_to_offset.iter().map(move |(&id, &offset)| {
            let embedding = match data {
                VectorData::InMemory(vec) => &vec[offset..offset + dim],
                VectorData::Mmap(mmap) => mmap.get(id).expect("id_to_offset has stale entry"),
            };
            (id, embedding)
        })
    }

    /// Get all VectorIds in deterministic order
    pub fn ids(&self) -> impl Iterator<Item = VectorId> + '_ {
        self.id_to_offset.keys().copied()
    }

    /// Get raw data slice (for snapshot serialization)
    ///
    /// Only available for InMemory heaps. Panics on Mmap variant.
    pub fn raw_data(&self) -> &[f32] {
        match &self.data {
            VectorData::InMemory(vec) => vec,
            VectorData::Mmap(_) => panic!("raw_data() not available on mmap-backed heap"),
        }
    }

    /// Get id_to_offset map (for snapshot serialization)
    pub fn id_to_offset_map(&self) -> &BTreeMap<VectorId, usize> {
        &self.id_to_offset
    }

    /// Check whether this heap is backed by a memory-mapped file.
    pub fn is_mmap(&self) -> bool {
        matches!(&self.data, VectorData::Mmap(_))
    }

    /// Write the current in-memory heap to a `.vec` mmap file.
    ///
    /// This creates a disk cache that can be opened with `from_mmap()` on
    /// subsequent startups, avoiding the cost of rebuilding from KV.
    ///
    /// Only works on InMemory heaps. Returns `Ok(())` silently on Mmap heaps
    /// (already on disk).
    pub fn freeze_to_disk(&self, path: &Path) -> Result<(), VectorError> {
        match &self.data {
            VectorData::InMemory(vec) => {
                mmap::write_mmap_file(
                    path,
                    self.config.dimension,
                    self.next_id.load(Ordering::Relaxed),
                    &self.id_to_offset,
                    &self.free_slots,
                    vec,
                )
            }
            VectorData::Mmap(_) => Ok(()), // Already on disk
        }
    }

}

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

        // Insert multiple vectors
        let embedding = vec![0.1; 384];
        let _id1 = heap.insert(&embedding).unwrap();
        let _id2 = heap.insert(&embedding).unwrap();
        let _id3 = heap.insert(&embedding).unwrap();

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
        let mut restored =
            VectorHeap::from_snapshot(config, data, id_to_offset, free_slots, next_id);

        // Verify state
        assert!(restored.get(id1).is_none()); // Deleted
        assert!(restored.get(id2).is_some()); // Exists
        assert_eq!(restored.free_slots().len(), 1); // One free slot

        // New insert should get higher ID
        let id3 = restored.insert(&vec![0.3; 384]).unwrap();
        assert!(
            id3.as_u64() >= next_id,
            "ID must be >= next_id from snapshot"
        );
    }

    #[test]
    fn test_insert_with_id_for_wal_replay() {
        let config = VectorConfig::for_minilm();
        let mut heap = VectorHeap::new(config);

        let embedding = vec![0.1; 384];

        // Insert with specific ID (simulating WAL replay)
        let replay_id = VectorId::new(100);
        heap.insert_with_id(replay_id, &embedding).unwrap();

        // Verify it exists
        assert!(heap.get(replay_id).is_some());

        // next_id should be updated to be > 100
        assert!(heap.next_id_value() > 100);

        // New insert should get ID > 100
        let new_id = heap.insert(&embedding).unwrap();
        assert!(new_id.as_u64() > 100);
    }

    #[test]
    fn test_clear_preserves_next_id() {
        let config = VectorConfig::for_minilm();
        let mut heap = VectorHeap::new(config);

        let embedding = vec![0.1; 384];
        let _id1 = heap.insert(&embedding).unwrap();
        let _id2 = heap.insert(&embedding).unwrap();
        let next_id_before = heap.next_id_value();

        heap.clear();

        // next_id should NOT be reset
        assert_eq!(heap.next_id_value(), next_id_before);
        assert!(heap.is_empty());

        // New insert should continue with higher ID
        let id3 = heap.insert(&embedding).unwrap();
        assert!(id3.as_u64() >= next_id_before);
    }

    #[test]
    fn test_contains() {
        let config = VectorConfig::for_minilm();
        let mut heap = VectorHeap::new(config);

        let embedding = vec![0.1; 384];
        let id = heap.insert(&embedding).unwrap();

        assert!(heap.contains(id));
        heap.delete(id);
        assert!(!heap.contains(id));
    }

    #[test]
    fn test_version_increments() {
        let config = VectorConfig::for_minilm();
        let mut heap = VectorHeap::new(config);

        let initial_version = heap.version();
        let embedding = vec![0.1; 384];

        let id = heap.insert(&embedding).unwrap();
        assert!(heap.version() > initial_version);

        let v1 = heap.version();
        heap.upsert(id, &embedding).unwrap();
        assert!(heap.version() > v1);

        let v2 = heap.version();
        heap.delete(id);
        assert!(heap.version() > v2);
    }

    #[test]
    fn test_accessors() {
        let config = VectorConfig::for_minilm();
        let heap = VectorHeap::new(config.clone());

        assert_eq!(heap.dimension(), 384);
        assert_eq!(heap.metric(), DistanceMetric::Cosine);
        assert_eq!(heap.len(), 0);
        assert!(heap.is_empty());
    }

    #[test]
    fn test_deleted_data_is_zeroed() {
        let config = VectorConfig::for_minilm();
        let mut heap = VectorHeap::new(config);

        let embedding = vec![0.5; 384];
        let id = heap.insert(&embedding).unwrap();

        // Get the offset before deletion
        let offset = *heap.id_to_offset_map().get(&id).unwrap();

        heap.delete(id);

        // Check that the data at that offset is zeroed
        let data = heap.raw_data();
        for i in offset..offset + 384 {
            assert_eq!(data[i], 0.0, "Data should be zeroed after deletion");
        }
    }

    // ====================================================================
    // mmap integration tests
    // ====================================================================

    #[test]
    fn test_freeze_and_reopen_mmap() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let vec_path = temp_dir.path().join("test.vec");

        let config = VectorConfig::for_minilm();
        let mut heap = VectorHeap::new(config.clone());

        // Insert a few vectors
        let e1 = vec![0.1_f32; 384];
        let e2 = vec![0.2_f32; 384];
        let e3 = vec![0.3_f32; 384];
        let id1 = heap.insert(&e1).unwrap();
        let id2 = heap.insert(&e2).unwrap();
        let id3 = heap.insert(&e3).unwrap();
        heap.delete(id2); // Create a free slot

        // Freeze to disk
        heap.freeze_to_disk(&vec_path).unwrap();

        // Reopen from mmap
        let mmap_heap = VectorHeap::from_mmap(&vec_path, config).unwrap();

        // Verify all active vectors match
        assert_eq!(mmap_heap.len(), 2); // id2 was deleted
        assert!(mmap_heap.is_mmap());

        let emb1 = mmap_heap.get(id1).unwrap();
        assert_eq!(emb1.len(), 384);
        assert!((emb1[0] - 0.1).abs() < f32::EPSILON);

        assert!(mmap_heap.get(id2).is_none()); // Deleted

        let emb3 = mmap_heap.get(id3).unwrap();
        assert!((emb3[0] - 0.3).abs() < f32::EPSILON);

        // Verify metadata
        assert_eq!(mmap_heap.next_id_value(), heap.next_id_value());
        assert_eq!(mmap_heap.free_slots().len(), 1);
        assert!(mmap_heap.contains(id1));
        assert!(!mmap_heap.contains(id2));
    }

    #[test]
    fn test_mmap_iter_matches_in_memory() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let vec_path = temp_dir.path().join("iter.vec");

        let config = VectorConfig::for_minilm();
        let mut heap = VectorHeap::new(config.clone());

        let e1 = vec![1.0_f32; 384];
        let e2 = vec![2.0_f32; 384];
        let id1 = heap.insert(&e1).unwrap();
        let id2 = heap.insert(&e2).unwrap();

        // Collect in-memory iteration results
        let mem_entries: Vec<_> = heap.iter().collect();

        // Freeze and reopen
        heap.freeze_to_disk(&vec_path).unwrap();
        let mmap_heap = VectorHeap::from_mmap(&vec_path, config).unwrap();

        // Collect mmap iteration results
        let mmap_entries: Vec<_> = mmap_heap.iter().collect();

        assert_eq!(mem_entries.len(), mmap_entries.len());
        for ((mid, memb), (iid, iemb)) in mem_entries.iter().zip(mmap_entries.iter()) {
            assert_eq!(mid, iid);
            assert_eq!(*memb, *iemb);
        }
    }

    #[test]
    fn test_mmap_empty_heap() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let vec_path = temp_dir.path().join("empty.vec");

        let config = VectorConfig::for_minilm();
        let heap = VectorHeap::new(config.clone());

        heap.freeze_to_disk(&vec_path).unwrap();
        let mmap_heap = VectorHeap::from_mmap(&vec_path, config).unwrap();

        assert_eq!(mmap_heap.len(), 0);
        assert!(mmap_heap.is_empty());
        assert!(mmap_heap.is_mmap());
    }

    #[test]
    fn test_is_mmap_flag() {
        let config = VectorConfig::for_minilm();
        let heap = VectorHeap::new(config);
        assert!(!heap.is_mmap());
    }

    #[test]
    fn test_delete_on_mmap_heap_does_not_panic() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let vec_path = temp_dir.path().join("test.vec");

        let config = VectorConfig::new(3, DistanceMetric::Cosine).unwrap();
        let mut heap = VectorHeap::new(config.clone());
        heap.upsert(VectorId::new(1), &[1.0, 0.0, 0.0]).unwrap();
        heap.upsert(VectorId::new(2), &[0.0, 1.0, 0.0]).unwrap();
        heap.freeze_to_disk(&vec_path).unwrap();

        // Reopen as mmap
        let mut mmap_heap = VectorHeap::from_mmap(&vec_path, config).unwrap();
        assert!(mmap_heap.is_mmap());
        assert_eq!(mmap_heap.len(), 2);

        // Delete should succeed without panicking (skips zeroing on mmap)
        let deleted = mmap_heap.delete(VectorId::new(1));
        assert!(deleted);
        assert_eq!(mmap_heap.len(), 1);
        assert!(mmap_heap.get(VectorId::new(1)).is_none());
        assert!(mmap_heap.get(VectorId::new(2)).is_some());

        // Deleting nonexistent ID returns false
        assert!(!mmap_heap.delete(VectorId::new(99)));
    }
}
