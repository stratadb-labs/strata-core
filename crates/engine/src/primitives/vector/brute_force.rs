//! Brute-Force Vector Search Backend
//!
//! Simple O(n) brute-force implementation.
//! Sufficient for datasets < 10K vectors.
//! Performance degrades linearly with dataset size.
//!
//! Switch threshold: P95 > 100ms at 50K vectors triggers HNSW priority.

use std::cmp::Ordering;

use crate::primitives::vector::backend::VectorIndexBackend;
use crate::primitives::vector::{DistanceMetric, VectorConfig, VectorError, VectorHeap, VectorId};

/// Brute-force vector search backend
///
/// Simple O(n) brute-force implementation.
/// Sufficient for datasets < 10K vectors.
/// Performance degrades linearly with dataset size.
///
/// Switch threshold: P95 > 100ms at 50K vectors triggers HNSW priority.
pub struct BruteForceBackend {
    /// Vector heap (contiguous storage)
    heap: VectorHeap,
}

impl BruteForceBackend {
    /// Create a new brute-force backend
    pub fn new(config: &VectorConfig) -> Self {
        BruteForceBackend {
            heap: VectorHeap::new(config.clone()),
        }
    }

    /// Create from existing heap (for recovery)
    pub fn from_heap(heap: VectorHeap) -> Self {
        BruteForceBackend { heap }
    }

    /// Get mutable access to heap (for recovery)
    pub fn heap_mut(&mut self) -> &mut VectorHeap {
        &mut self.heap
    }

    /// Get read access to heap (for snapshot)
    pub fn heap(&self) -> &VectorHeap {
        &self.heap
    }

    /// Search with explicit determinism guarantees
    ///
    /// This method documents and enforces the determinism contract:
    /// 1. Iterate vectors in VectorId order (BTreeMap iteration)
    /// 2. Compute scores (single-threaded, deterministic)
    /// 3. Sort by (score desc, VectorId asc)
    /// 4. Truncate to k
    ///
    /// INVARIANTS SATISFIED:
    /// - R3: Deterministic order (same query = same results)
    /// - R4: Backend tie-break (VectorId asc)
    /// - R8: Single-threaded (no parallel computation)
    /// - R10: Search is read-only (no mutation)
    #[allow(dead_code)]
    pub fn search_deterministic(&self, query: &[f32], k: usize) -> Vec<(VectorId, f32)> {
        // This is the same as search(), but with explicit documentation
        <Self as VectorIndexBackend>::search(self, query, k)
    }
}

impl VectorIndexBackend for BruteForceBackend {
    fn allocate_id(&mut self) -> VectorId {
        // Delegate to heap's per-collection counter
        self.heap.allocate_id()
    }

    fn insert(&mut self, id: VectorId, embedding: &[f32]) -> Result<(), VectorError> {
        self.heap.upsert(id, embedding)
    }

    fn insert_with_id(&mut self, id: VectorId, embedding: &[f32]) -> Result<(), VectorError> {
        // Use heap's insert_with_id which updates next_id for monotonicity
        self.heap.insert_with_id(id, embedding)
    }

    fn delete(&mut self, id: VectorId) -> Result<bool, VectorError> {
        Ok(self.heap.delete(id))
    }

    fn search(&self, query: &[f32], k: usize) -> Vec<(VectorId, f32)> {
        if k == 0 || self.heap.is_empty() {
            return Vec::new();
        }

        // Validate query dimension
        if query.len() != self.heap.dimension() {
            // Return empty on dimension mismatch (validated at facade level)
            return Vec::new();
        }

        let metric = self.heap.metric();

        // Compute similarities for all vectors
        // IMPORTANT: heap.iter() returns vectors in VectorId order (BTreeMap)
        // This ensures deterministic iteration before scoring
        let mut results: Vec<(VectorId, f32)> = self
            .heap
            .iter()
            .map(|(id, embedding)| {
                let score = compute_similarity(query, embedding, metric);
                (id, score)
            })
            .collect();

        // Sort by (score desc, VectorId asc) for determinism
        // CRITICAL: VectorId tie-break ensures identical results across runs
        // This satisfies Invariant R4 (Backend tie-break)
        results.sort_by(|(id_a, score_a), (id_b, score_b)| {
            // Primary: score descending (higher = better)
            score_b
                .partial_cmp(score_a)
                .unwrap_or(Ordering::Equal)
                // Secondary: VectorId ascending (deterministic tie-break)
                .then_with(|| id_a.cmp(id_b))
        });

        results.truncate(k);
        results
    }

    fn len(&self) -> usize {
        self.heap.len()
    }

    fn dimension(&self) -> usize {
        self.heap.dimension()
    }

    fn metric(&self) -> DistanceMetric {
        self.heap.metric()
    }

    fn config(&self) -> VectorConfig {
        self.heap.config().clone()
    }

    fn get(&self, id: VectorId) -> Option<&[f32]> {
        self.heap.get(id)
    }

    fn contains(&self, id: VectorId) -> bool {
        self.heap.contains(id)
    }

    fn vector_ids(&self) -> Vec<VectorId> {
        self.heap.ids().collect()
    }

    fn snapshot_state(&self) -> (u64, Vec<usize>) {
        (self.heap.next_id_value(), self.heap.free_slots().to_vec())
    }

    fn restore_snapshot_state(&mut self, next_id: u64, free_slots: Vec<usize>) {
        self.heap.restore_snapshot_state(next_id, free_slots);
    }
}

// ============================================================================
// Distance Metric Calculations
// ============================================================================

/// Compute similarity score between two vectors
///
/// All scores are normalized to "higher = more similar" (Invariant R2).
/// This function is single-threaded for determinism (Invariant R8).
///
/// IMPORTANT: No implicit normalization of vectors (Invariant R9).
/// Vectors are used as-is.
fn compute_similarity(a: &[f32], b: &[f32], metric: DistanceMetric) -> f32 {
    debug_assert_eq!(
        a.len(),
        b.len(),
        "Dimension mismatch in similarity computation"
    );

    match metric {
        DistanceMetric::Cosine => cosine_similarity(a, b),
        DistanceMetric::Euclidean => euclidean_similarity(a, b),
        DistanceMetric::DotProduct => dot_product(a, b),
    }
}

/// Cosine similarity: dot(a,b) / (||a|| * ||b||)
///
/// Range: [-1, 1], higher = more similar
/// Returns 0.0 if either vector has zero norm (avoids division by zero)
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot = dot_product(a, b);
    let norm_a = l2_norm(a);
    let norm_b = l2_norm(b);

    if norm_a == 0.0 || norm_b == 0.0 {
        // Zero vectors have undefined cosine similarity
        // Return 0.0 as neutral value
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

/// Euclidean similarity: 1 / (1 + l2_distance)
///
/// Range: (0, 1], higher = more similar
/// Transforms distance to similarity (inversely related)
fn euclidean_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dist = euclidean_distance(a, b);
    // Transform to similarity: 1 / (1 + dist)
    // When dist=0, similarity=1 (identical)
    // As dist→∞, similarity→0
    1.0 / (1.0 + dist)
}

/// Dot product (inner product)
///
/// Range: unbounded, higher = more similar
/// Assumes vectors are pre-normalized for meaningful comparison
fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// L2 norm (Euclidean length)
fn l2_norm(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

/// Euclidean distance (L2 distance)
fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
}

/// Squared Euclidean distance (optimization for comparison)
///
/// Since sqrt is monotonic, we can compare squared distances
/// when we only need relative ordering, not actual distances.
#[allow(dead_code)]
fn euclidean_distance_squared(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| (x - y).powi(2)).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================
    // Distance Metric Tests
    // ========================================

    #[test]
    fn test_cosine_identical_vectors() {
        let v = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_opposite_vectors() {
        let v1 = vec![1.0, 0.0];
        let v2 = vec![-1.0, 0.0];
        let sim = cosine_similarity(&v1, &v2);
        assert!((sim - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn test_cosine_orthogonal_vectors() {
        let v1 = vec![1.0, 0.0];
        let v2 = vec![0.0, 1.0];
        let sim = cosine_similarity(&v1, &v2);
        assert!(sim.abs() < 1e-6);
    }

    #[test]
    fn test_euclidean_identical_vectors() {
        let v = vec![1.0, 2.0, 3.0];
        let sim = euclidean_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_euclidean_distant_vectors() {
        let v1 = vec![0.0, 0.0];
        let v2 = vec![100.0, 0.0];
        let sim = euclidean_similarity(&v1, &v2);
        assert!(sim < 0.01); // Very low similarity
        assert!(sim > 0.0); // But still positive
        assert!(sim <= 1.0);
    }

    #[test]
    fn test_dot_product_unit_vectors() {
        let v = vec![1.0, 0.0];
        assert!((dot_product(&v, &v) - 1.0).abs() < 1e-6);

        let v1 = vec![1.0, 0.0];
        let v2 = vec![0.0, 1.0];
        assert!(dot_product(&v1, &v2).abs() < 1e-6);
    }

    #[test]
    fn test_zero_vector_handling() {
        let zero = vec![0.0, 0.0, 0.0];
        let nonzero = vec![1.0, 2.0, 3.0];

        // Cosine with zero vector returns 0.0 (not NaN)
        assert_eq!(cosine_similarity(&zero, &nonzero), 0.0);
        assert_eq!(cosine_similarity(&nonzero, &zero), 0.0);
        assert_eq!(cosine_similarity(&zero, &zero), 0.0);

        // Euclidean with zero vector works normally
        let sim = euclidean_similarity(&zero, &nonzero);
        assert!(sim > 0.0 && sim <= 1.0);
    }

    // ========================================
    // Backend Tests
    // ========================================

    #[test]
    fn test_backend_basic_operations() {
        let config = VectorConfig::for_minilm();
        let mut backend = BruteForceBackend::new(&config);

        // Insert
        let e1 = vec![0.1; 384];
        let e2 = vec![0.2; 384];
        let e3 = vec![0.3; 384];

        backend.insert(VectorId::new(1), &e1).unwrap();
        backend.insert(VectorId::new(2), &e2).unwrap();
        backend.insert(VectorId::new(3), &e3).unwrap();

        assert_eq!(backend.len(), 3);
        assert!(!backend.is_empty());

        // Search
        let query = vec![0.25; 384];
        let results = backend.search(&query, 2);
        assert_eq!(results.len(), 2);

        // Delete
        backend.delete(VectorId::new(2)).unwrap();
        assert_eq!(backend.len(), 2);
        assert!(!backend.contains(VectorId::new(2)));

        // Update (upsert)
        let e1_new = vec![0.15; 384];
        backend.insert(VectorId::new(1), &e1_new).unwrap();
        assert_eq!(backend.len(), 2); // Count unchanged

        let retrieved = backend.get(VectorId::new(1)).unwrap();
        assert!((retrieved[0] - 0.15).abs() < f32::EPSILON);
    }

    #[test]
    fn test_search_determinism() {
        let config = VectorConfig::for_minilm();
        let mut backend = BruteForceBackend::new(&config);

        // Insert vectors with known embeddings
        for i in 0..100 {
            let embedding: Vec<f32> = (0..384).map(|j| ((i * 384 + j) as f32).sin()).collect();
            let id = VectorId::new(i);
            backend.insert(id, &embedding).unwrap();
        }

        // Query vector
        let query: Vec<f32> = (0..384).map(|i| (i as f32).cos()).collect();

        // Run search multiple times
        let result1 = backend.search(&query, 10);
        let result2 = backend.search(&query, 10);
        let result3 = backend.search(&query, 10);

        // All results must be identical
        assert_eq!(result1, result2);
        assert_eq!(result2, result3);
    }

    #[test]
    fn test_score_tie_breaking() {
        let config = VectorConfig::new(3, DistanceMetric::DotProduct).unwrap();
        let mut backend = BruteForceBackend::new(&config);

        // Insert vectors that will have identical scores
        let embedding = vec![1.0, 0.0, 0.0];
        backend.insert(VectorId::new(5), &embedding).unwrap();
        backend.insert(VectorId::new(2), &embedding).unwrap();
        backend.insert(VectorId::new(8), &embedding).unwrap();
        backend.insert(VectorId::new(1), &embedding).unwrap();

        // Query that produces identical scores for all vectors
        let query = vec![1.0, 0.0, 0.0];
        let results = backend.search(&query, 10);

        // All scores should be equal (dot product = 1.0)
        for (_, score) in &results {
            assert!((score - 1.0).abs() < f32::EPSILON);
        }

        // With equal scores, should be sorted by VectorId ascending
        let ids: Vec<u64> = results.iter().map(|(id, _)| id.as_u64()).collect();
        assert_eq!(ids, vec![1, 2, 5, 8]);
    }

    #[test]
    fn test_higher_is_better_contract() {
        let config = VectorConfig::new(3, DistanceMetric::Cosine).unwrap();
        let mut backend = BruteForceBackend::new(&config);

        // Insert a "close" vector and a "far" vector relative to query
        let query = vec![1.0, 0.0, 0.0];
        let close = vec![0.9, 0.1, 0.0]; // Similar to query
        let far = vec![0.0, 0.0, 1.0]; // Orthogonal to query

        backend.insert(VectorId::new(1), &close).unwrap();
        backend.insert(VectorId::new(2), &far).unwrap();

        let results = backend.search(&query, 2);

        // Close vector should have HIGHER score (rank first)
        assert_eq!(results[0].0, VectorId::new(1));
        assert!(results[0].1 > results[1].1);
    }

    #[test]
    fn test_large_scale_search() {
        let config = VectorConfig::new(128, DistanceMetric::Cosine).unwrap();
        let mut backend = BruteForceBackend::new(&config);

        // Insert 1000 vectors
        for i in 0..1000 {
            let embedding: Vec<f32> = (0..128)
                .map(|j| ((i * 128 + j) as f32 / 1000.0).sin())
                .collect();
            backend.insert(VectorId::new(i), &embedding).unwrap();
        }

        // Search should complete in reasonable time
        let query: Vec<f32> = (0..128).map(|i| (i as f32 / 100.0).cos()).collect();
        let start = std::time::Instant::now();
        let results = backend.search(&query, 10);
        let elapsed = start.elapsed();

        assert_eq!(results.len(), 10);
        assert!(
            elapsed.as_millis() < 100,
            "Search took too long: {:?}",
            elapsed
        );

        // Verify ordering
        for i in 1..results.len() {
            assert!(
                results[i - 1].1 >= results[i].1,
                "Results not sorted by score"
            );
        }
    }

    #[test]
    fn test_empty_search() {
        let config = VectorConfig::for_minilm();
        let backend = BruteForceBackend::new(&config);

        let query = vec![0.1; 384];
        let results = backend.search(&query, 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_k_zero() {
        let config = VectorConfig::for_minilm();
        let mut backend = BruteForceBackend::new(&config);

        backend.insert(VectorId::new(1), &vec![0.1; 384]).unwrap();

        let query = vec![0.1; 384];
        let results = backend.search(&query, 0);
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_dimension_mismatch() {
        let config = VectorConfig::for_minilm();
        let mut backend = BruteForceBackend::new(&config);

        backend.insert(VectorId::new(1), &vec![0.1; 384]).unwrap();

        let wrong_dim_query = vec![0.1; 256];
        let results = backend.search(&wrong_dim_query, 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_from_heap() {
        let config = VectorConfig::for_minilm();
        let mut heap = VectorHeap::new(config.clone());

        // Add some vectors to heap
        heap.insert(&vec![0.1; 384]).unwrap();
        heap.insert(&vec![0.2; 384]).unwrap();

        // Create backend from heap
        let backend = BruteForceBackend::from_heap(heap);
        assert_eq!(backend.len(), 2);
    }

    #[test]
    fn test_accessors() {
        let config = VectorConfig::for_minilm();
        let backend = BruteForceBackend::new(&config);

        assert_eq!(backend.dimension(), 384);
        assert_eq!(backend.metric(), DistanceMetric::Cosine);
        assert!(backend.is_empty());
    }
}
