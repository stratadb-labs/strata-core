//! Vector Index Backend trait
//!
//! Defines the interface for swappable vector index implementations.
//! BruteForceBackend (O(n) search)
//! HnswBackend (O(log n) search) - reserved

use crate::primitives::vector::{DistanceMetric, VectorConfig, VectorError, VectorId};

/// Trait for swappable vector index implementations
///
/// BruteForceBackend (O(n) search)
/// HnswBackend (O(log n) search)
///
/// IMPORTANT: This trait is designed to work for BOTH brute-force and HNSW.
/// Do NOT add methods that assume brute-force semantics (like get_all_vectors).
/// See Evolution Warning A in architecture documentation.
pub trait VectorIndexBackend: Send + Sync {
    /// Allocate a new VectorId (monotonically increasing, per-collection)
    ///
    /// Each collection has its own ID counter. IDs are never reused.
    /// This counter is persisted in snapshots via `snapshot_state()`.
    ///
    /// CRITICAL: This is per-collection, not global. Two separate databases
    /// doing identical operations MUST get identical VectorIds.
    fn allocate_id(&mut self) -> VectorId;

    /// Insert a vector (upsert semantics)
    ///
    /// If the VectorId already exists, updates the embedding.
    /// The VectorId is assigned externally and passed in.
    fn insert(&mut self, id: VectorId, embedding: &[f32]) -> Result<(), VectorError>;

    /// Insert with specific VectorId (for WAL replay)
    ///
    /// Used during recovery to replay WAL entries with their original IDs.
    /// Updates next_id if necessary to maintain monotonicity (Invariant T4).
    ///
    /// IMPORTANT: This method MUST ensure that future ID allocations
    /// don't reuse IDs from replayed entries.
    fn insert_with_id(&mut self, id: VectorId, embedding: &[f32]) -> Result<(), VectorError>;

    /// Delete a vector
    ///
    /// Returns true if the vector existed and was deleted.
    fn delete(&mut self, id: VectorId) -> Result<bool, VectorError>;

    /// Search for k nearest neighbors
    ///
    /// Returns (VectorId, score) pairs.
    /// Scores are normalized to "higher = more similar" (Invariant R2).
    /// Results are sorted by (score desc, VectorId asc) for determinism (Invariant R4).
    fn search(&self, query: &[f32], k: usize) -> Vec<(VectorId, f32)>;

    /// Get number of indexed vectors
    fn len(&self) -> usize;

    /// Check if empty
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get embedding dimension
    fn dimension(&self) -> usize;

    /// Get distance metric
    fn metric(&self) -> DistanceMetric;

    /// Get full collection config (Issue #452: for config validation during replay)
    fn config(&self) -> VectorConfig;

    /// Get a vector by ID (for metadata lookups after search)
    fn get(&self, id: VectorId) -> Option<&[f32]>;

    /// Check if a vector exists
    fn contains(&self, id: VectorId) -> bool;

    // ========================================================================
    // Snapshot Methods
    // ========================================================================

    /// Get all VectorIds in deterministic order
    ///
    /// Returns VectorIds sorted ascending for deterministic snapshot serialization.
    fn vector_ids(&self) -> Vec<VectorId>;

    /// Get snapshot state for serialization
    ///
    /// Returns (next_id, free_slots) for snapshot header.
    /// CRITICAL: next_id and free_slots MUST be persisted to maintain
    /// VectorId uniqueness across restarts (Invariant T4).
    fn snapshot_state(&self) -> (u64, Vec<usize>);

    /// Restore snapshot state after deserialization
    ///
    /// Called after all vectors have been inserted with insert_with_id()
    /// to restore the exact next_id and free_slots from the snapshot.
    fn restore_snapshot_state(&mut self, next_id: u64, free_slots: Vec<usize>);
}

/// Factory for creating index backends
///
/// This abstraction allows switching between BruteForce and HNSW
/// without changing the VectorStore code.
#[derive(Clone, Default)]
pub enum IndexBackendFactory {
    /// Brute-force O(n) search
    #[default]
    BruteForce,
    // Hnsw(HnswConfig),  // Reserved for future use
}

impl IndexBackendFactory {
    /// Create a new backend instance
    pub fn create(&self, config: &VectorConfig) -> Box<dyn VectorIndexBackend> {
        match self {
            IndexBackendFactory::BruteForce => {
                Box::new(super::brute_force::BruteForceBackend::new(config))
            }
        }
    }
}
