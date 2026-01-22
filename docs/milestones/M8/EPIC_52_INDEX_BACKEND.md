# Epic 52: Index Backend Abstraction

**Goal**: Implement VectorIndexBackend trait and BruteForceBackend

**Dependencies**: Epic 51 (Vector Heap)

---

## Scope

- VectorIndexBackend trait for swappable implementations
- BruteForceBackend with O(n) search
- Distance metric calculations (cosine, euclidean, dot product)
- Score normalization ("higher is better")
- Deterministic search ordering (score desc, VectorId asc)

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #341 | VectorIndexBackend Trait Definition | CRITICAL |
| #342 | BruteForceBackend Implementation | CRITICAL |
| #343 | Distance Metric Calculations | CRITICAL |
| #344 | Deterministic Search Ordering | CRITICAL |
| #345 | Score Normalization | HIGH |

---

## Story #341: VectorIndexBackend Trait Definition

**File**: `crates/primitives/src/vector/backend.rs` (NEW)

**Deliverable**: Trait for swappable vector index implementations

### Implementation

```rust
use crate::vector::{DistanceMetric, VectorId, VectorError};

/// Trait for swappable vector index implementations
///
/// M8: BruteForceBackend (O(n) search)
/// M9: HnswBackend (O(log n) search)
///
/// IMPORTANT: This trait is designed to work for BOTH brute-force and HNSW.
/// Do NOT add methods that assume brute-force semantics (like get_all_vectors).
/// See Evolution Warning A in M8_ARCHITECTURE.md.
pub trait VectorIndexBackend: Send + Sync {
    /// Insert a vector (upsert semantics)
    ///
    /// If the VectorId already exists, updates the embedding.
    /// The VectorId is assigned externally and passed in.
    fn insert(&mut self, id: VectorId, embedding: &[f32]) -> Result<(), VectorError>;

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

    /// Get a vector by ID (for metadata lookups after search)
    fn get(&self, id: VectorId) -> Option<&[f32]>;

    /// Check if a vector exists
    fn contains(&self, id: VectorId) -> bool;
}

/// Factory for creating index backends
///
/// This abstraction allows switching between BruteForce (M8) and HNSW (M9)
/// without changing the VectorStore code.
pub enum IndexBackendFactory {
    BruteForce,
    // Hnsw(HnswConfig),  // M9
}

impl IndexBackendFactory {
    /// Create a new backend instance
    pub fn create(&self, config: &VectorConfig) -> Box<dyn VectorIndexBackend> {
        match self {
            IndexBackendFactory::BruteForce => {
                Box::new(BruteForceBackend::new(config))
            }
            // IndexBackendFactory::Hnsw(hnsw_config) => {
            //     Box::new(HnswBackend::new(config, hnsw_config))
            // }
        }
    }
}

impl Default for IndexBackendFactory {
    fn default() -> Self {
        IndexBackendFactory::BruteForce
    }
}
```

### Acceptance Criteria

- [ ] Trait with insert, delete, search, len, dimension, metric, get, contains
- [ ] Trait is `Send + Sync` for future concurrency
- [ ] search() returns Vec<(VectorId, f32)> with deterministic ordering
- [ ] No methods that assume brute-force semantics
- [ ] IndexBackendFactory for backend selection
- [ ] Default factory creates BruteForceBackend

---

## Story #342: BruteForceBackend Implementation

**File**: `crates/primitives/src/vector/brute_force.rs` (NEW)

**Deliverable**: O(n) brute-force search implementation

### Implementation

```rust
use crate::vector::{
    VectorConfig, VectorHeap, VectorId, VectorError, DistanceMetric,
    VectorIndexBackend,
};
use std::cmp::Ordering;

/// Brute-force vector search backend
///
/// Simple O(n) implementation for M8.
/// Sufficient for datasets < 10K vectors.
/// Performance degrades linearly with dataset size.
///
/// Switch threshold: P95 > 100ms at 50K vectors triggers M9/HNSW priority.
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
}

impl VectorIndexBackend for BruteForceBackend {
    fn insert(&mut self, id: VectorId, embedding: &[f32]) -> Result<(), VectorError> {
        self.heap.upsert(id, embedding)
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
        let mut results: Vec<(VectorId, f32)> = self.heap
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
            score_b.partial_cmp(score_a)
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

    fn get(&self, id: VectorId) -> Option<&[f32]> {
        self.heap.get(id)
    }

    fn contains(&self, id: VectorId) -> bool {
        self.heap.contains(id)
    }
}
```

### Acceptance Criteria

- [ ] BruteForceBackend wraps VectorHeap
- [ ] Implements all VectorIndexBackend methods
- [ ] search() iterates heap in deterministic order
- [ ] search() sorts by (score desc, VectorId asc)
- [ ] Returns empty Vec for k=0 or empty heap
- [ ] from_heap() for recovery

---

## Story #343: Distance Metric Calculations

**File**: `crates/primitives/src/vector/brute_force.rs`

**Deliverable**: Distance metric implementations

### Implementation

```rust
/// Compute similarity score between two vectors
///
/// All scores are normalized to "higher = more similar" (Invariant R2).
/// This function is single-threaded for determinism (Invariant R8).
///
/// IMPORTANT: No implicit normalization of vectors (Invariant R9).
/// Vectors are used as-is.
fn compute_similarity(a: &[f32], b: &[f32], metric: DistanceMetric) -> f32 {
    debug_assert_eq!(a.len(), b.len(), "Dimension mismatch in similarity computation");

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
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| x * y)
        .sum()
}

/// L2 norm (Euclidean length)
fn l2_norm(v: &[f32]) -> f32 {
    v.iter()
        .map(|x| x * x)
        .sum::<f32>()
        .sqrt()
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
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum()
}
```

### Acceptance Criteria

- [ ] cosine_similarity() returns dot/(norm_a * norm_b)
- [ ] Handles zero-norm vectors gracefully (returns 0.0)
- [ ] euclidean_similarity() returns 1/(1+distance)
- [ ] dot_product() returns raw sum of element products
- [ ] l2_norm() computes sqrt(sum of squares)
- [ ] euclidean_distance() computes sqrt(sum of squared differences)
- [ ] All functions are pure (no side effects) for determinism

---

## Story #344: Deterministic Search Ordering

**File**: `crates/primitives/src/vector/brute_force.rs`

**Deliverable**: Guaranteed deterministic result ordering

### Implementation

```rust
impl BruteForceBackend {
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
    pub fn search_deterministic(&self, query: &[f32], k: usize) -> Vec<(VectorId, f32)> {
        // This is the same as search(), but with explicit documentation
        self.search(query, k)
    }
}

#[cfg(test)]
mod determinism_tests {
    use super::*;

    /// Test that search produces identical results on repeated calls
    #[test]
    fn test_search_determinism() {
        let config = VectorConfig::for_minilm();
        let mut backend = BruteForceBackend::new(&config);

        // Insert vectors with known embeddings
        for i in 0..100 {
            let embedding: Vec<f32> = (0..384)
                .map(|j| ((i * 384 + j) as f32).sin())
                .collect();
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

    /// Test tie-breaking with identical scores
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
}
```

### Acceptance Criteria

- [ ] Search always returns same results for same inputs
- [ ] Score ties broken by VectorId ascending
- [ ] BTreeMap iteration guarantees VectorId order before scoring
- [ ] No parallel computation (single-threaded)
- [ ] Tests verify determinism across multiple calls
- [ ] Tests verify tie-breaking behavior

---

## Story #345: Score Normalization

**File**: `crates/primitives/src/vector/brute_force.rs`

**Deliverable**: All metrics normalized to "higher is better"

### Implementation

```rust
/// Score normalization documentation and tests
///
/// All distance metrics are normalized so that:
/// - Higher scores indicate MORE similar vectors
/// - This is the OPPOSITE of distance (lower distance = higher similarity)
///
/// Metric normalization formulas:
/// - Cosine: raw cosine similarity [-1, 1], no transformation needed
/// - Euclidean: 1 / (1 + distance), transforms (0, ∞) → (0, 1]
/// - DotProduct: raw dot product, no transformation (assumes normalized vectors)

#[cfg(test)]
mod normalization_tests {
    use super::*;

    #[test]
    fn test_cosine_normalization() {
        // Identical vectors: similarity = 1.0
        let v = vec![1.0, 2.0, 3.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-6);

        // Opposite vectors: similarity = -1.0
        let v1 = vec![1.0, 0.0];
        let v2 = vec![-1.0, 0.0];
        assert!((cosine_similarity(&v1, &v2) - (-1.0)).abs() < 1e-6);

        // Orthogonal vectors: similarity = 0.0
        let v1 = vec![1.0, 0.0];
        let v2 = vec![0.0, 1.0];
        assert!(cosine_similarity(&v1, &v2).abs() < 1e-6);
    }

    #[test]
    fn test_euclidean_normalization() {
        // Identical vectors: distance = 0, similarity = 1.0
        let v = vec![1.0, 2.0, 3.0];
        assert!((euclidean_similarity(&v, &v) - 1.0).abs() < 1e-6);

        // Distant vectors: low similarity
        let v1 = vec![0.0, 0.0];
        let v2 = vec![100.0, 0.0];
        let sim = euclidean_similarity(&v1, &v2);
        assert!(sim < 0.01); // Very low similarity
        assert!(sim > 0.0);  // But still positive

        // Similarity is always in (0, 1]
        assert!(sim > 0.0 && sim <= 1.0);
    }

    #[test]
    fn test_dot_product_normalization() {
        // Identical unit vectors: dot = 1.0
        let v = vec![1.0, 0.0];
        assert!((dot_product(&v, &v) - 1.0).abs() < 1e-6);

        // Orthogonal unit vectors: dot = 0.0
        let v1 = vec![1.0, 0.0];
        let v2 = vec![0.0, 1.0];
        assert!(dot_product(&v1, &v2).abs() < 1e-6);

        // Note: dot product can be negative for opposing vectors
        // and unbounded for non-normalized vectors
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

    #[test]
    fn test_higher_is_better_contract() {
        let config = VectorConfig::new(3, DistanceMetric::Cosine).unwrap();
        let mut backend = BruteForceBackend::new(&config);

        // Insert a "close" vector and a "far" vector relative to query
        let query = vec![1.0, 0.0, 0.0];
        let close = vec![0.9, 0.1, 0.0]; // Similar to query
        let far = vec![0.0, 0.0, 1.0];   // Orthogonal to query

        backend.insert(VectorId::new(1), &close).unwrap();
        backend.insert(VectorId::new(2), &far).unwrap();

        let results = backend.search(&query, 2);

        // Close vector should have HIGHER score (rank first)
        assert_eq!(results[0].0, VectorId::new(1));
        assert!(results[0].1 > results[1].1);
    }
}
```

### Acceptance Criteria

- [ ] Cosine returns similarity in [-1, 1], higher = more similar
- [ ] Euclidean returns 1/(1+dist) in (0, 1], higher = more similar
- [ ] DotProduct returns raw value, higher = more similar
- [ ] Zero vectors handled without NaN/Inf
- [ ] Tests verify "higher is better" contract
- [ ] Search results ordered by score descending

---

## Testing

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_backend_end_to_end() {
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

        // Search
        let query = vec![0.25; 384];
        let results = backend.search(&query, 2);

        assert_eq!(results.len(), 2);
        // e2 (0.2) should be closest to query (0.25)

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
        assert!(elapsed.as_millis() < 100, "Search took too long: {:?}", elapsed);

        // Verify ordering
        for i in 1..results.len() {
            assert!(results[i-1].1 >= results[i].1, "Results not sorted by score");
        }
    }
}
```

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/primitives/src/vector/backend.rs` | CREATE - VectorIndexBackend trait |
| `crates/primitives/src/vector/brute_force.rs` | CREATE - BruteForceBackend implementation |
| `crates/primitives/src/vector/mod.rs` | MODIFY - Export backend module |
