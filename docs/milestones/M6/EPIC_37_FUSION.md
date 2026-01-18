# Epic 37: Fusion Infrastructure

**Goal**: Implement pluggable result fusion with RRF default

**Dependencies**: Epic 36 (Composite Search)

---

## Scope

- Fuser trait for pluggable fusion algorithms
- RRFFuser (Reciprocal Rank Fusion) default implementation
- Tie-breaking for determinism
- Result deduplication

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #280 | Fuser Trait Definition | FOUNDATION |
| #281 | RRFFuser Implementation | CRITICAL |
| #282 | Tie-Breaking for Determinism | HIGH |
| #283 | Result Deduplication | HIGH |

---

## Story #280: Fuser Trait Definition

**File**: `crates/search/src/fuser.rs` (NEW)

**Deliverable**: Trait for pluggable fusion algorithms

### Implementation

```rust
use crate::search_types::{SearchResponse, PrimitiveKind};

/// Pluggable fusion interface
///
/// Fusers take ranked result lists from multiple primitives
/// and combine them into a single ranked list.
///
/// M6 ships with RRFFuser. Future can add:
/// - Weighted fusion
/// - Learning-to-rank fusion
/// - Max-score fusion
pub trait Fuser: Send + Sync {
    /// Fuse results from multiple primitives
    ///
    /// Input: Vec of (PrimitiveKind, SearchResponse) pairs
    /// Output: Single merged SearchResponse
    ///
    /// The fuser should:
    /// 1. Combine results from all primitives
    /// 2. Re-rank based on fusion algorithm
    /// 3. Deduplicate if same DocRef appears in multiple lists
    /// 4. Return top-k results
    fn fuse(
        &self,
        results: Vec<(PrimitiveKind, SearchResponse)>,
        k: usize,
    ) -> SearchResponse;

    /// Name for debugging and logging
    fn name(&self) -> &str;
}
```

### Acceptance Criteria

- [ ] Trait defined with fuse() method
- [ ] name() for debugging
- [ ] Send + Sync for thread safety

---

## Story #281: RRFFuser Implementation

**File**: `crates/search/src/fuser.rs`

**Deliverable**: Reciprocal Rank Fusion implementation

### Implementation

```rust
use std::collections::HashMap;
use std::cmp::Ordering;

/// Reciprocal Rank Fusion (RRF)
///
/// Classic fusion algorithm from "Reciprocal Rank Fusion outperforms
/// Condorcet and individual Rank Learning Methods" (Cormack et al., 2009)
///
/// RRF Score = sum(1 / (k + rank)) across all lists
///
/// Where k is a smoothing constant (default 60).
pub struct RRFFuser {
    /// RRF constant k (default 60)
    ///
    /// Higher k gives more weight to documents that appear in multiple lists.
    /// Lower k gives more weight to top-ranked documents in each list.
    k_rrf: u32,
}

impl Default for RRFFuser {
    fn default() -> Self {
        RRFFuser { k_rrf: 60 }
    }
}

impl RRFFuser {
    pub fn new(k_rrf: u32) -> Self {
        RRFFuser { k_rrf }
    }
}

impl Fuser for RRFFuser {
    fn fuse(
        &self,
        results: Vec<(PrimitiveKind, SearchResponse)>,
        k: usize,
    ) -> SearchResponse {
        // Collect all hits with their RRF contributions
        let mut rrf_scores: HashMap<DocRef, f32> = HashMap::new();
        let mut hit_data: HashMap<DocRef, SearchHit> = HashMap::new();

        for (_primitive, response) in results {
            for hit in response.hits {
                // RRF contribution: 1 / (k + rank)
                let rrf_contribution = 1.0 / (self.k_rrf as f32 + hit.rank as f32);
                *rrf_scores.entry(hit.doc_ref.clone()).or_insert(0.0) += rrf_contribution;

                // Keep first occurrence of hit data (for snippet, etc.)
                hit_data.entry(hit.doc_ref.clone()).or_insert(hit);
            }
        }

        // Sort by RRF score (descending)
        let mut scored: Vec<_> = rrf_scores.into_iter().collect();
        scored.sort_by(|a, b| {
            b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal)
        });

        // Build final ranked list
        let hits: Vec<SearchHit> = scored
            .into_iter()
            .take(k)
            .enumerate()
            .map(|(i, (doc_ref, rrf_score))| {
                let mut hit = hit_data.remove(&doc_ref).unwrap();
                hit.score = rrf_score;
                hit.rank = (i + 1) as u32;  // 1-indexed rank
                hit
            })
            .collect();

        SearchResponse {
            hits,
            truncated: false,  // Fusion itself doesn't truncate
            stats: SearchStats::default(),
        }
    }

    fn name(&self) -> &str {
        "rrf"
    }
}
```

### Acceptance Criteria

- [ ] RRF formula: sum(1 / (k + rank))
- [ ] k_rrf parameter configurable (default 60)
- [ ] Documents in multiple lists get higher scores
- [ ] Output is sorted by fused score

---

## Story #282: Tie-Breaking for Determinism

**File**: `crates/search/src/fuser.rs`

**Deliverable**: Deterministic ordering when scores are equal

### Implementation

```rust
impl RRFFuser {
    fn fuse_deterministic(
        &self,
        results: Vec<(PrimitiveKind, SearchResponse)>,
        k: usize,
    ) -> SearchResponse {
        // ... RRF calculation same as above ...

        // Sort with tie-breaking
        scored.sort_by(|a, b| {
            // Primary: RRF score (descending)
            match b.1.partial_cmp(&a.1) {
                Some(Ordering::Equal) | None => {
                    // Tie-breaker 1: original score from first occurrence
                    let orig_a = hit_data.get(&a.0).map(|h| h.score).unwrap_or(0.0);
                    let orig_b = hit_data.get(&b.0).map(|h| h.score).unwrap_or(0.0);
                    match orig_b.partial_cmp(&orig_a) {
                        Some(Ordering::Equal) | None => {
                            // Tie-breaker 2: DocRef hash (stable ordering)
                            a.0.stable_hash().cmp(&b.0.stable_hash())
                        }
                        Some(ord) => ord,
                    }
                }
                Some(ord) => ord,
            }
        });

        // ... rest same as above ...
    }
}

impl DocRef {
    /// Stable hash for deterministic tie-breaking
    fn stable_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }
}
```

### Acceptance Criteria

- [ ] Same inputs always produce same output order
- [ ] Primary sort by RRF score
- [ ] Secondary sort by original score
- [ ] Tertiary sort by stable DocRef hash

---

## Story #283: Result Deduplication

**File**: `crates/search/src/fuser.rs`

**Deliverable**: Handle same DocRef appearing in multiple result lists

### Implementation

```rust
impl Fuser for RRFFuser {
    fn fuse(&self, results: Vec<(PrimitiveKind, SearchResponse)>, k: usize) -> SearchResponse {
        // HashMap naturally deduplicates by DocRef
        // When same DocRef appears in multiple lists:
        // - RRF scores are summed
        // - First hit's metadata (snippet, etc.) is kept

        let mut rrf_scores: HashMap<DocRef, f32> = HashMap::new();
        let mut hit_data: HashMap<DocRef, SearchHit> = HashMap::new();
        let mut occurrence_count: HashMap<DocRef, usize> = HashMap::new();

        for (primitive, response) in results {
            for hit in response.hits {
                // Track occurrences
                *occurrence_count.entry(hit.doc_ref.clone()).or_insert(0) += 1;

                // Sum RRF contributions
                let rrf_contribution = 1.0 / (self.k_rrf as f32 + hit.rank as f32);
                *rrf_scores.entry(hit.doc_ref.clone()).or_insert(0.0) += rrf_contribution;

                // Keep first occurrence (has the original primitive's snippet)
                hit_data.entry(hit.doc_ref.clone()).or_insert(hit);
            }
        }

        // Deduplication is implicit: each DocRef appears once in output
        // ...
    }
}
```

### Acceptance Criteria

- [ ] Same DocRef from multiple primitives merged
- [ ] RRF scores summed across occurrences
- [ ] First occurrence's metadata preserved
- [ ] Output contains unique DocRefs only

---

## RRF Algorithm Explanation

```
Given:
  - List A: [doc1@rank1, doc2@rank2, doc3@rank3]
  - List B: [doc2@rank1, doc4@rank2, doc1@rank3]
  - k_rrf = 60

RRF scores:
  doc1: 1/(60+1) + 1/(60+3) = 0.0164 + 0.0159 = 0.0323
  doc2: 1/(60+2) + 1/(60+1) = 0.0161 + 0.0164 = 0.0325  <- highest (in both lists, high ranks)
  doc3: 1/(60+3) = 0.0159
  doc4: 1/(60+2) = 0.0161

Final ranking: [doc2, doc1, doc4, doc3]
```

---

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn make_hits(refs: &[&str]) -> Vec<SearchHit> {
        refs.iter().enumerate().map(|(i, r)| {
            SearchHit::new(
                DocRef::Kv { key: Key::test(r) },
                1.0 - (i as f32 * 0.1),  // Decreasing scores
                (i + 1) as u32,          // 1-indexed rank
            )
        }).collect()
    }

    #[test]
    fn test_rrf_fusion_basic() {
        let fuser = RRFFuser::default();

        let results = vec![
            (PrimitiveKind::Kv, SearchResponse::new(make_hits(&["A", "B", "C"]), false, Default::default())),
            (PrimitiveKind::Json, SearchResponse::new(make_hits(&["B", "D", "A"]), false, Default::default())),
        ];

        let fused = fuser.fuse(results, 10);

        // A and B appear in both lists, should rank higher
        let top_refs: Vec<_> = fused.hits.iter()
            .take(2)
            .map(|h| h.doc_ref.clone())
            .collect();

        // Both A and B should be in top 2
        assert!(top_refs.iter().any(|r| matches!(r, DocRef::Kv { key } if key.contains("A"))));
        assert!(top_refs.iter().any(|r| matches!(r, DocRef::Kv { key } if key.contains("B"))));
    }

    #[test]
    fn test_rrf_deduplication() {
        let fuser = RRFFuser::default();

        let results = vec![
            (PrimitiveKind::Kv, SearchResponse::new(make_hits(&["A", "B"]), false, Default::default())),
            (PrimitiveKind::Json, SearchResponse::new(make_hits(&["A", "C"]), false, Default::default())),
        ];

        let fused = fuser.fuse(results, 10);

        // A should appear only once despite being in both lists
        let a_count = fused.hits.iter()
            .filter(|h| matches!(&h.doc_ref, DocRef::Kv { key } if key.contains("A")))
            .count();
        assert_eq!(a_count, 1);
    }

    #[test]
    fn test_rrf_deterministic() {
        let fuser = RRFFuser::default();

        let results = || vec![
            (PrimitiveKind::Kv, SearchResponse::new(make_hits(&["A", "B"]), false, Default::default())),
            (PrimitiveKind::Json, SearchResponse::new(make_hits(&["C", "D"]), false, Default::default())),
        ];

        let fused1 = fuser.fuse(results(), 10);
        let fused2 = fuser.fuse(results(), 10);

        // Same inputs -> same output order
        for (h1, h2) in fused1.hits.iter().zip(fused2.hits.iter()) {
            assert_eq!(h1.doc_ref, h2.doc_ref);
            assert_eq!(h1.rank, h2.rank);
        }
    }
}
```

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/search/src/fuser.rs` | CREATE - Fuser trait, RRFFuser |
| `crates/search/src/lib.rs` | MODIFY - Export fuser module |
