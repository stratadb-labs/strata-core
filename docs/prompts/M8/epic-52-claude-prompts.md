# Epic 52: Index Backend Abstraction - Implementation Prompts

**Epic Goal**: Implement VectorIndexBackend trait and BruteForceBackend

**GitHub Issue**: [#390](https://github.com/anibjoshi/in-mem/issues/390)
**Status**: Ready after Epic 51
**Dependencies**: Epic 51 (Vector Heap)

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M8_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

### IMPORTANT: Naming Convention

**Do NOT use "M8" or "m8" in the codebase or comments.** M8 is an internal milestone indicator only. In code, use "Vector" prefix instead:
- Module names: `vector`, `backend`, `brute_force`, `distance`
- Type names: `VectorIndexBackend`, `BruteForceBackend`, `DistanceMetric`
- Test names: `test_vector_*`, `test_brute_force_*`, not `test_m8_*`
- Comments: "Vector backend" not "M8 backend"

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M8_ARCHITECTURE.md`
2. **Epic Spec**: `docs/milestones/M8/EPIC_52_INDEX_BACKEND.md`
3. **Prompt Header**: `docs/prompts/M8/M8_PROMPT_HEADER.md` for the 7 architectural rules

---

## Epic 52 Overview

### Scope
- VectorIndexBackend trait for swappable implementations
- BruteForceBackend with O(n) search
- Distance metric calculations (cosine, euclidean, dot product)
- Score normalization ("higher is better")
- Deterministic search ordering (score desc, VectorId asc)

### Critical Invariants

| Invariant | Description |
|-----------|-------------|
| **R2** | All metrics normalized to "higher = more similar" |
| **R4** | Backend tie-break: score desc, VectorId asc |
| **R8** | Single-threaded search (no parallelism in M8) |

### Component Breakdown
- **Story #405**: VectorIndexBackend Trait Definition - CRITICAL
- **Story #406**: BruteForceBackend Implementation - CRITICAL
- **Story #407**: Distance Metric Calculations - CRITICAL
- **Story #408**: Deterministic Search Ordering - CRITICAL
- **Story #409**: Score Normalization - HIGH

---

## Story #405: VectorIndexBackend Trait Definition

**GitHub Issue**: [#405](https://github.com/anibjoshi/in-mem/issues/405)
**Estimated Time**: 1.5 hours
**Dependencies**: Epic 51
**Blocks**: #406

### Start Story

```bash
gh issue view 405
./scripts/start-story.sh 52 405 backend-trait
```

### Implementation

Create `crates/primitives/src/vector/backend.rs`:

```rust
//! Vector index backend abstraction
//!
//! This trait allows swapping between BruteForce (M8) and HNSW (M9)
//! without changing the VectorStore code.

use crate::vector::{DistanceMetric, VectorError, VectorId, VectorResult};

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
    fn insert(&mut self, id: VectorId, embedding: &[f32]) -> VectorResult<()>;

    /// Delete a vector
    ///
    /// Returns true if the vector existed and was deleted.
    fn delete(&mut self, id: VectorId) -> VectorResult<bool>;

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
#[derive(Debug, Clone, Default)]
pub enum IndexBackendFactory {
    #[default]
    BruteForce,
    // Hnsw(HnswConfig),  // M9
}

impl IndexBackendFactory {
    /// Create a new backend instance
    pub fn create(
        &self,
        dimension: usize,
        metric: DistanceMetric,
    ) -> Box<dyn VectorIndexBackend> {
        match self {
            IndexBackendFactory::BruteForce => {
                Box::new(super::brute_force::BruteForceBackend::new(dimension, metric))
            }
            // IndexBackendFactory::Hnsw(config) => { ... }  // M9
        }
    }
}
```

### Acceptance Criteria

- [ ] Trait with insert/delete/search/get operations
- [ ] Send + Sync bounds for thread safety
- [ ] Documentation for score normalization contract
- [ ] Documentation for deterministic ordering contract
- [ ] IndexBackendFactory for backend creation

### Complete Story

```bash
~/.cargo/bin/cargo test --workspace
./scripts/complete-story.sh 405
```

---

## Story #406: BruteForceBackend Implementation

**GitHub Issue**: [#406](https://github.com/anibjoshi/in-mem/issues/406)
**Estimated Time**: 2.5 hours
**Dependencies**: #405, Epic 51
**Blocks**: Epic 54

### Start Story

```bash
gh issue view 406
./scripts/start-story.sh 52 406 brute-force
```

### Implementation

Create `crates/primitives/src/vector/brute_force.rs`:

```rust
//! Brute-force vector index implementation
//!
//! O(n) linear scan search, suitable for small collections (<50K vectors).
//! This is the baseline implementation for M8; HNSW is added in M9.

use std::cmp::Ordering;

use crate::vector::{
    DistanceMetric, VectorError, VectorId, VectorResult,
    VectorIndexBackend, VectorHeap,
};
use super::distance::compute_similarity;

/// Brute-force vector index for small collections (<50K vectors)
///
/// O(n) search complexity, but simple and correct.
/// Sufficient for most agent memory workloads in M8.
pub struct BruteForceBackend {
    /// Underlying vector heap storage
    heap: VectorHeap,
}

impl BruteForceBackend {
    /// Create a new brute-force backend
    pub fn new(dimension: usize, metric: DistanceMetric) -> Self {
        use crate::vector::VectorConfig;
        let config = VectorConfig::new(dimension, metric)
            .expect("dimension validated by caller");
        BruteForceBackend {
            heap: VectorHeap::new(config),
        }
    }

    /// Create from an existing heap (for recovery)
    pub fn from_heap(heap: VectorHeap) -> Self {
        BruteForceBackend { heap }
    }

    /// Get reference to underlying heap (for snapshotting)
    pub fn heap(&self) -> &VectorHeap {
        &self.heap
    }

    /// Get mutable reference to heap (for recovery)
    pub fn heap_mut(&mut self) -> &mut VectorHeap {
        &mut self.heap
    }
}

impl VectorIndexBackend for BruteForceBackend {
    fn insert(&mut self, id: VectorId, embedding: &[f32]) -> VectorResult<()> {
        self.heap.upsert_with_id(id, embedding)
    }

    fn delete(&mut self, id: VectorId) -> VectorResult<bool> {
        Ok(self.heap.delete(id))
    }

    fn search(&self, query: &[f32], k: usize) -> Vec<(VectorId, f32)> {
        if k == 0 || self.heap.is_empty() {
            return Vec::new();
        }

        // Validate dimension (return empty on mismatch rather than panic)
        if query.len() != self.heap.dimension() {
            return Vec::new();
        }

        let metric = self.heap.config().metric;

        // Compute similarity for all vectors
        let mut results: Vec<(VectorId, f32)> = self.heap
            .iter()
            .map(|(id, embedding)| {
                let score = compute_similarity(metric, query, embedding);
                (id, score)
            })
            .collect();

        // Sort: score descending, VectorId ascending for tie-breaks
        // This ensures deterministic ordering (Invariant R4)
        results.sort_by(|a, b| {
            // Primary: score descending
            let score_cmp = b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal);

            if score_cmp == Ordering::Equal {
                // Secondary: VectorId ascending (tie-breaker)
                a.0.cmp(&b.0)
            } else {
                score_cmp
            }
        });

        // Take top k
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
        self.heap.config().metric
    }

    fn get(&self, id: VectorId) -> Option<&[f32]> {
        self.heap.get(id)
    }

    fn contains(&self, id: VectorId) -> bool {
        self.heap.contains(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_backend() -> BruteForceBackend {
        BruteForceBackend::new(4, DistanceMetric::Cosine)
    }

    #[test]
    fn test_insert_and_search() {
        let mut backend = make_backend();

        backend.insert(VectorId::new(0), &[1.0, 0.0, 0.0, 0.0]).unwrap();
        backend.insert(VectorId::new(1), &[0.9, 0.1, 0.0, 0.0]).unwrap();
        backend.insert(VectorId::new(2), &[0.0, 1.0, 0.0, 0.0]).unwrap();

        let results = backend.search(&[1.0, 0.0, 0.0, 0.0], 3);

        assert_eq!(results.len(), 3);
        // First result should be exact match (id=0)
        assert_eq!(results[0].0, VectorId::new(0));
        // Second should be close match (id=1)
        assert_eq!(results[1].0, VectorId::new(1));
        // Third should be orthogonal (id=2)
        assert_eq!(results[2].0, VectorId::new(2));
    }

    #[test]
    fn test_delete() {
        let mut backend = make_backend();

        backend.insert(VectorId::new(0), &[1.0, 0.0, 0.0, 0.0]).unwrap();
        backend.insert(VectorId::new(1), &[0.0, 1.0, 0.0, 0.0]).unwrap();

        assert_eq!(backend.len(), 2);

        let deleted = backend.delete(VectorId::new(0)).unwrap();
        assert!(deleted);
        assert_eq!(backend.len(), 1);

        let results = backend.search(&[1.0, 0.0, 0.0, 0.0], 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, VectorId::new(1));
    }

    #[test]
    fn test_search_empty() {
        let backend = make_backend();
        let results = backend.search(&[1.0, 0.0, 0.0, 0.0], 10);
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_k_zero() {
        let mut backend = make_backend();
        backend.insert(VectorId::new(0), &[1.0, 0.0, 0.0, 0.0]).unwrap();

        let results = backend.search(&[1.0, 0.0, 0.0, 0.0], 0);
        assert!(results.is_empty());
    }
}
```

### Acceptance Criteria

- [ ] Implements VectorIndexBackend trait
- [ ] Linear scan through all vectors
- [ ] Uses VectorHeap internally
- [ ] Correct sorting: (score desc, VectorId asc)
- [ ] Truncates to k results
- [ ] Performance acceptable for <50K vectors

### Complete Story

```bash
~/.cargo/bin/cargo test --workspace
./scripts/complete-story.sh 406
```

---

## Story #407: Distance Metric Calculations

**GitHub Issue**: [#407](https://github.com/anibjoshi/in-mem/issues/407)
**Estimated Time**: 2 hours
**Dependencies**: #395 (DistanceMetric)
**Blocks**: #406

### Start Story

```bash
gh issue view 407
./scripts/start-story.sh 52 407 distance-calc
```

### Implementation

Create `crates/primitives/src/vector/distance.rs`:

```rust
//! Distance metric calculations for vector similarity
//!
//! All metrics are normalized to "higher = more similar" (Invariant R2).

use crate::vector::DistanceMetric;

/// Compute similarity score based on metric
///
/// All scores are normalized to "higher = more similar".
/// This is a core invariant (R2) that must be maintained.
pub fn compute_similarity(metric: DistanceMetric, a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len(), "Vectors must have same dimension");

    match metric {
        DistanceMetric::Cosine => cosine_similarity(a, b),
        DistanceMetric::Euclidean => euclidean_similarity(a, b),
        DistanceMetric::DotProduct => dot_product(a, b),
    }
}

/// Cosine similarity: dot(a,b) / (||a|| * ||b||)
///
/// Range: [-1, 1], higher = more similar
/// Returns 0 if either vector has zero magnitude (handles edge case safely)
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;

    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }

    let magnitude = (norm_a * norm_b).sqrt();
    if magnitude < f32::EPSILON {
        0.0 // Handle zero vectors safely
    } else {
        dot / magnitude
    }
}

/// Euclidean similarity: 1 / (1 + distance)
///
/// Range: (0, 1], higher = more similar
/// Distance 0 -> similarity 1, distance infinity -> similarity 0
fn euclidean_similarity(a: &[f32], b: &[f32]) -> f32 {
    let mut sum_sq = 0.0f32;

    for (x, y) in a.iter().zip(b.iter()) {
        let diff = x - y;
        sum_sq += diff * diff;
    }

    let distance = sum_sq.sqrt();
    1.0 / (1.0 + distance)
}

/// Dot product (raw value)
///
/// Range: unbounded, higher = more similar
/// WARNING: For non-normalized vectors, scores can be very large or negative
fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_identical() {
        let v = [1.0, 0.0, 0.0, 0.0];
        let score = cosine_similarity(&v, &v);
        assert!((score - 1.0).abs() < 1e-6, "Identical vectors should have cosine = 1");
    }

    #[test]
    fn test_cosine_opposite() {
        let v1 = [1.0, 0.0, 0.0, 0.0];
        let v2 = [-1.0, 0.0, 0.0, 0.0];
        let score = cosine_similarity(&v1, &v2);
        assert!((score - (-1.0)).abs() < 1e-6, "Opposite vectors should have cosine = -1");
    }

    #[test]
    fn test_cosine_orthogonal() {
        let v1 = [1.0, 0.0, 0.0, 0.0];
        let v2 = [0.0, 1.0, 0.0, 0.0];
        let score = cosine_similarity(&v1, &v2);
        assert!(score.abs() < 1e-6, "Orthogonal vectors should have cosine = 0");
    }

    #[test]
    fn test_cosine_zero_vector() {
        let v1 = [1.0, 0.0, 0.0, 0.0];
        let v2 = [0.0, 0.0, 0.0, 0.0];
        let score = cosine_similarity(&v1, &v2);
        assert_eq!(score, 0.0, "Zero vector should return 0");
    }

    #[test]
    fn test_euclidean_identical() {
        let v = [1.0, 2.0, 3.0, 4.0];
        let score = euclidean_similarity(&v, &v);
        assert!((score - 1.0).abs() < 1e-6, "Identical vectors should have euclidean sim = 1");
    }

    #[test]
    fn test_euclidean_different() {
        let v1 = [0.0, 0.0, 0.0, 0.0];
        let v2 = [1.0, 0.0, 0.0, 0.0];
        let score = euclidean_similarity(&v1, &v2);
        // distance = 1, similarity = 1/(1+1) = 0.5
        assert!((score - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_euclidean_range() {
        let v1 = [0.0, 0.0, 0.0, 0.0];
        let v2 = [1000.0, 1000.0, 1000.0, 1000.0];
        let score = euclidean_similarity(&v1, &v2);
        assert!(score > 0.0, "Euclidean similarity should be > 0");
        assert!(score <= 1.0, "Euclidean similarity should be <= 1");
    }

    #[test]
    fn test_dot_product() {
        let v1 = [1.0, 2.0, 3.0, 4.0];
        let v2 = [1.0, 1.0, 1.0, 1.0];
        let score = dot_product(&v1, &v2);
        assert!((score - 10.0).abs() < 1e-6);
    }

    #[test]
    fn test_dot_product_orthogonal() {
        let v1 = [1.0, 0.0, 0.0, 0.0];
        let v2 = [0.0, 1.0, 0.0, 0.0];
        let score = dot_product(&v1, &v2);
        assert!(score.abs() < 1e-6);
    }

    #[test]
    fn test_compute_similarity_dispatches() {
        let v1 = [1.0, 0.0, 0.0, 0.0];
        let v2 = [1.0, 0.0, 0.0, 0.0];

        let cosine = compute_similarity(DistanceMetric::Cosine, &v1, &v2);
        let euclidean = compute_similarity(DistanceMetric::Euclidean, &v1, &v2);
        let dot = compute_similarity(DistanceMetric::DotProduct, &v1, &v2);

        assert!((cosine - 1.0).abs() < 1e-6);
        assert!((euclidean - 1.0).abs() < 1e-6);
        assert!((dot - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_higher_is_more_similar() {
        // For all metrics, closer vectors should have higher scores
        let query = [1.0, 0.0, 0.0, 0.0];
        let close = [0.9, 0.1, 0.0, 0.0];
        let far = [0.0, 1.0, 0.0, 0.0];

        for metric in [DistanceMetric::Cosine, DistanceMetric::Euclidean, DistanceMetric::DotProduct] {
            let score_close = compute_similarity(metric, &query, &close);
            let score_far = compute_similarity(metric, &query, &far);

            assert!(
                score_close > score_far,
                "{:?}: close score ({}) should be > far score ({})",
                metric, score_close, score_far
            );
        }
    }
}
```

### Acceptance Criteria

- [ ] Cosine similarity: dot(a,b) / (||a|| * ||b||)
- [ ] Euclidean similarity: 1 / (1 + distance)
- [ ] Dot product: raw value
- [ ] All return "higher = more similar"
- [ ] Handles zero vectors safely (epsilon)
- [ ] Unit tests for each metric

### Complete Story

```bash
~/.cargo/bin/cargo test --workspace
./scripts/complete-story.sh 407
```

---

## Story #408: Deterministic Search Ordering

**GitHub Issue**: [#408](https://github.com/anibjoshi/in-mem/issues/408)
**Estimated Time**: 1.5 hours
**Dependencies**: #406
**Blocks**: None

### Start Story

```bash
gh issue view 408
./scripts/start-story.sh 52 408 deterministic-order
```

### Implementation

Add comprehensive determinism tests to `brute_force.rs`:

```rust
#[cfg(test)]
mod determinism_tests {
    use super::*;

    #[test]
    fn test_deterministic_tie_breaking() {
        // Create vectors with identical scores (orthogonal to query)
        let mut backend = BruteForceBackend::new(4, DistanceMetric::Cosine);

        // All these are orthogonal to the query, so same cosine score
        backend.insert(VectorId::new(5), &[0.0, 1.0, 0.0, 0.0]).unwrap();
        backend.insert(VectorId::new(2), &[0.0, 0.0, 1.0, 0.0]).unwrap();
        backend.insert(VectorId::new(8), &[0.0, 0.0, 0.0, 1.0]).unwrap();

        let query = [1.0, 0.0, 0.0, 0.0];
        let results = backend.search(&query, 3);

        // With identical scores, should be sorted by VectorId ascending
        assert_eq!(results[0].0, VectorId::new(2));
        assert_eq!(results[1].0, VectorId::new(5));
        assert_eq!(results[2].0, VectorId::new(8));
    }

    #[test]
    fn test_deterministic_across_runs() {
        fn run_search() -> Vec<(VectorId, f32)> {
            let mut backend = BruteForceBackend::new(4, DistanceMetric::Cosine);

            // Insert in "random" order
            backend.insert(VectorId::new(10), &[0.5, 0.5, 0.0, 0.0]).unwrap();
            backend.insert(VectorId::new(3), &[0.8, 0.2, 0.0, 0.0]).unwrap();
            backend.insert(VectorId::new(7), &[0.9, 0.1, 0.0, 0.0]).unwrap();
            backend.insert(VectorId::new(1), &[1.0, 0.0, 0.0, 0.0]).unwrap();

            backend.search(&[1.0, 0.0, 0.0, 0.0], 4)
        }

        let results1 = run_search();
        let results2 = run_search();
        let results3 = run_search();

        // All runs should produce identical results
        assert_eq!(results1, results2);
        assert_eq!(results2, results3);
    }

    #[test]
    fn test_insertion_order_does_not_affect_results() {
        let query = [1.0, 0.0, 0.0, 0.0];

        let embeddings = vec![
            (VectorId::new(1), [0.9, 0.1, 0.0, 0.0]),
            (VectorId::new(2), [0.8, 0.2, 0.0, 0.0]),
            (VectorId::new(3), [0.7, 0.3, 0.0, 0.0]),
        ];

        // Insert in order 1, 2, 3
        let mut backend1 = BruteForceBackend::new(4, DistanceMetric::Cosine);
        for (id, emb) in &embeddings {
            backend1.insert(*id, emb).unwrap();
        }

        // Insert in order 3, 1, 2
        let mut backend2 = BruteForceBackend::new(4, DistanceMetric::Cosine);
        backend2.insert(embeddings[2].0, &embeddings[2].1).unwrap();
        backend2.insert(embeddings[0].0, &embeddings[0].1).unwrap();
        backend2.insert(embeddings[1].0, &embeddings[1].1).unwrap();

        let results1 = backend1.search(&query, 3);
        let results2 = backend2.search(&query, 3);

        assert_eq!(results1, results2);
    }

    #[test]
    fn test_btreemap_iteration_order() {
        // Verify BTreeMap provides sorted iteration
        use std::collections::BTreeMap;

        let mut map = BTreeMap::new();
        map.insert(VectorId::new(5), ());
        map.insert(VectorId::new(2), ());
        map.insert(VectorId::new(8), ());
        map.insert(VectorId::new(1), ());

        let ids: Vec<_> = map.keys().collect();
        assert_eq!(
            ids,
            vec![
                &VectorId::new(1),
                &VectorId::new(2),
                &VectorId::new(5),
                &VectorId::new(8),
            ]
        );
    }
}
```

### Acceptance Criteria

- [ ] Sort by score descending (primary)
- [ ] Tie-break by VectorId ascending
- [ ] Handles NaN scores gracefully
- [ ] Test: same query returns identical order on repeated runs
- [ ] Test: deterministic regardless of insertion order
- [ ] BTreeMap iteration ensures deterministic source order

### Complete Story

```bash
~/.cargo/bin/cargo test --workspace
./scripts/complete-story.sh 408
```

---

## Story #409: Score Normalization

**GitHub Issue**: [#409](https://github.com/anibjoshi/in-mem/issues/409)
**Estimated Time**: 1.5 hours
**Dependencies**: #407
**Blocks**: None

### Start Story

```bash
gh issue view 409
./scripts/start-story.sh 52 409 score-normalization
```

### Implementation

Add normalization tests to `distance.rs`:

```rust
#[cfg(test)]
mod normalization_tests {
    use super::*;

    /// Document the score normalization contract (Invariant R2)
    ///
    /// Metric         | Raw Output      | Normalized Output
    /// ---------------|-----------------|------------------
    /// Cosine         | [-1, 1]         | [-1, 1] (unchanged)
    /// Euclidean      | [0, ∞)          | (0, 1] via 1/(1+d)
    /// DotProduct     | (-∞, ∞)         | (-∞, ∞) (unchanged)

    #[test]
    fn test_cosine_range() {
        // Cosine should be in [-1, 1]
        let identical = cosine_similarity(&[1.0, 0.0], &[1.0, 0.0]);
        let opposite = cosine_similarity(&[1.0, 0.0], &[-1.0, 0.0]);
        let orthogonal = cosine_similarity(&[1.0, 0.0], &[0.0, 1.0]);

        assert!((identical - 1.0).abs() < 1e-6);
        assert!((opposite - (-1.0)).abs() < 1e-6);
        assert!(orthogonal.abs() < 1e-6);

        assert!(identical >= -1.0 && identical <= 1.0);
        assert!(opposite >= -1.0 && opposite <= 1.0);
        assert!(orthogonal >= -1.0 && orthogonal <= 1.0);
    }

    #[test]
    fn test_euclidean_range() {
        // Euclidean similarity should be in (0, 1]
        let identical = euclidean_similarity(&[1.0, 2.0], &[1.0, 2.0]);
        let close = euclidean_similarity(&[0.0, 0.0], &[0.1, 0.1]);
        let far = euclidean_similarity(&[0.0, 0.0], &[1000.0, 1000.0]);

        assert!((identical - 1.0).abs() < 1e-6, "Identical should be 1.0");
        assert!(close > 0.0 && close < 1.0);
        assert!(far > 0.0 && far < 1.0);

        // Far should be smaller than close
        assert!(far < close, "Farther should have lower similarity");
    }

    #[test]
    fn test_all_metrics_higher_is_better() {
        // Core invariant: for ALL metrics, more similar = higher score
        let query = [1.0, 0.0, 0.0];
        let very_similar = [0.99, 0.01, 0.0];
        let somewhat_similar = [0.7, 0.3, 0.0];
        let not_similar = [0.0, 1.0, 0.0];

        for metric in [
            DistanceMetric::Cosine,
            DistanceMetric::Euclidean,
            DistanceMetric::DotProduct,
        ] {
            let score_very = compute_similarity(metric, &query, &very_similar);
            let score_somewhat = compute_similarity(metric, &query, &somewhat_similar);
            let score_not = compute_similarity(metric, &query, &not_similar);

            assert!(
                score_very > score_somewhat,
                "{:?}: very_similar ({}) should be > somewhat_similar ({})",
                metric, score_very, score_somewhat
            );
            assert!(
                score_somewhat > score_not,
                "{:?}: somewhat_similar ({}) should be > not_similar ({})",
                metric, score_somewhat, score_not
            );
        }
    }

    #[test]
    fn test_normalized_embeddings_consistent() {
        // For normalized embeddings, cosine and dot product should give same ranking
        fn normalize(v: &[f32]) -> Vec<f32> {
            let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            v.iter().map(|x| x / norm).collect()
        }

        let query = normalize(&[1.0, 0.5, 0.0]);
        let v1 = normalize(&[1.0, 0.4, 0.0]);
        let v2 = normalize(&[0.5, 0.5, 0.0]);

        let cosine_1 = compute_similarity(DistanceMetric::Cosine, &query, &v1);
        let cosine_2 = compute_similarity(DistanceMetric::Cosine, &query, &v2);

        let dot_1 = compute_similarity(DistanceMetric::DotProduct, &query, &v1);
        let dot_2 = compute_similarity(DistanceMetric::DotProduct, &query, &v2);

        // For normalized vectors, ranking should be the same
        assert_eq!(
            cosine_1 > cosine_2,
            dot_1 > dot_2,
            "For normalized vectors, cosine and dot should agree on ranking"
        );
    }
}
```

### Acceptance Criteria

- [ ] Cosine: identical vectors -> ~1.0
- [ ] Euclidean: zero distance -> 1.0
- [ ] All metrics: closer vectors -> higher scores
- [ ] Document normalization formulas
- [ ] Test edge cases (zero vectors, identical vectors)

### Complete Story

```bash
~/.cargo/bin/cargo test --workspace
./scripts/complete-story.sh 409
```

---

## Epic 52 Completion Checklist

### Validation

```bash
# Full test suite
~/.cargo/bin/cargo test --workspace

# Run backend-specific tests
~/.cargo/bin/cargo test vector::backend
~/.cargo/bin/cargo test vector::brute_force
~/.cargo/bin/cargo test vector::distance

# Run determinism tests
~/.cargo/bin/cargo test deterministic

# Clippy and format
~/.cargo/bin/cargo clippy --workspace -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### Performance Baseline

```rust
#[test]
#[ignore] // Run manually: cargo test --release -- --ignored
fn bench_brute_force_search() {
    use std::time::Instant;

    let mut backend = BruteForceBackend::new(384, DistanceMetric::Cosine);

    // Insert 10K vectors
    for i in 0..10_000 {
        let embedding: Vec<f32> = (0..384).map(|j| ((i * 384 + j) % 100) as f32 / 100.0).collect();
        backend.insert(VectorId::new(i), &embedding).unwrap();
    }

    let query: Vec<f32> = (0..384).map(|i| (i % 100) as f32 / 100.0).collect();

    let start = Instant::now();
    for _ in 0..100 {
        let _ = backend.search(&query, 10);
    }
    let elapsed = start.elapsed();

    println!("10K vectors, 100 searches: {:?} ({:?} per search)",
             elapsed, elapsed / 100);

    // M8 target: < 50ms per search for 10K vectors
    assert!(elapsed / 100 < std::time::Duration::from_millis(50));
}
```

### Epic Merge

```bash
git checkout develop
git merge --no-ff epic-52-index-backend -m "Epic 52: Index Backend Abstraction complete"
git push origin develop

gh issue close 390 --comment "Epic 52 complete. All 5 stories merged and validated."
```

---

*End of Epic 52 Prompts*
