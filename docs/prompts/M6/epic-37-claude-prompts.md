# Epic 37: Fusion Infrastructure - Implementation Prompts

**Epic Goal**: Implement pluggable result fusion with RRF default

**GitHub Issue**: [#299](https://github.com/anibjoshi/in-mem/issues/299)
**Status**: Ready after Epic 36
**Dependencies**: Epic 36 (Composite Search)

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M6_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M6_ARCHITECTURE.md`
2. **Epic Spec**: `docs/milestones/M6/EPIC_37_FUSION.md`
3. **Prompt Header**: `docs/prompts/M6/M6_PROMPT_HEADER.md` for the 6 architectural rules

**CRITICAL**: Rule 6 - Algorithm Swappable. Fuser is a TRAIT, not hardcoded.

---

## Epic 37 Overview

### Scope
- Fuser trait for pluggable fusion algorithms
- RRFFuser (Reciprocal Rank Fusion) default implementation
- Tie-breaking for determinism
- Result deduplication

### Success Criteria
- [ ] Fuser trait defined
- [ ] RRFFuser implements the RRF formula correctly
- [ ] Same inputs always produce same outputs (determinism)
- [ ] Duplicate DocRefs are merged, not duplicated

### Component Breakdown
- **Story #280 (GitHub #325)**: Fuser Trait Definition - FOUNDATION
- **Story #281 (GitHub #326)**: RRFFuser Implementation - CRITICAL
- **Story #282 (GitHub #327)**: Tie-Breaking for Determinism - HIGH
- **Story #283 (GitHub #328)**: Result Deduplication - HIGH

---

## Dependency Graph

```
Story #325 (Fuser Trait) ──> Story #326 (RRFFuser)
                                    │
Story #327 (Tie-Breaking) ──────────┤
                                    │
Story #328 (Deduplication) ─────────┘
```

---

## Parallelization Strategy

### Optimal Execution (2 Claudes)

| Phase | Duration | Claude 1 | Claude 2 |
|-------|----------|----------|----------|
| 1 | 2 hours | #325 Fuser Trait | - |
| 2 | 3 hours | #326 RRFFuser | #327 Tie-Breaking |
| 3 | 2 hours | #328 Deduplication | - |

**Total Wall Time**: ~7 hours (vs. ~10 hours sequential)

---

## Story #325: Fuser Trait Definition

**GitHub Issue**: [#325](https://github.com/anibjoshi/in-mem/issues/325)
**Estimated Time**: 2 hours
**Dependencies**: Epic 36 complete
**Blocks**: Story #326

### Start Story

```bash
gh issue view 325
./scripts/start-story.sh 37 325 fuser-trait
```

### Implementation

Create `crates/search/src/fuser.rs`:

```rust
use crate::search_types::{SearchResponse, PrimitiveKind};

/// Pluggable fusion interface
///
/// Fusers take ranked result lists from multiple primitives
/// and combine them into a single ranked list.
pub trait Fuser: Send + Sync {
    /// Fuse results from multiple primitives
    fn fuse(
        &self,
        results: Vec<(PrimitiveKind, SearchResponse)>,
        k: usize,
    ) -> SearchResponse;

    /// Name for debugging and logging
    fn name(&self) -> &str;
}
```

### Complete Story

```bash
./scripts/complete-story.sh 325
```

---

## Story #326: RRFFuser Implementation

**GitHub Issue**: [#326](https://github.com/anibjoshi/in-mem/issues/326)
**Estimated Time**: 3 hours
**Dependencies**: Story #325

### Start Story

```bash
gh issue view 326
./scripts/start-story.sh 37 326 rrf-fuser
```

### Implementation

Add to `crates/search/src/fuser.rs`:

```rust
use std::collections::HashMap;
use std::cmp::Ordering;

/// Reciprocal Rank Fusion (RRF)
///
/// RRF Score = sum(1 / (k + rank)) across all lists
/// Where k is a smoothing constant (default 60).
pub struct RRFFuser {
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
        let mut rrf_scores: HashMap<DocRef, f32> = HashMap::new();
        let mut hit_data: HashMap<DocRef, SearchHit> = HashMap::new();

        for (_primitive, response) in results {
            for hit in response.hits {
                // RRF contribution: 1 / (k + rank)
                let rrf_contribution = 1.0 / (self.k_rrf as f32 + hit.rank as f32);
                *rrf_scores.entry(hit.doc_ref.clone()).or_insert(0.0) += rrf_contribution;

                // Keep first occurrence of hit data
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
                hit.rank = (i + 1) as u32;
                hit
            })
            .collect();

        SearchResponse {
            hits,
            truncated: false,
            stats: SearchStats::default(),
        }
    }

    fn name(&self) -> &str {
        "rrf"
    }
}
```

### RRF Algorithm Explanation

```
Given:
  - List A: [doc1@rank1, doc2@rank2, doc3@rank3]
  - List B: [doc2@rank1, doc4@rank2, doc1@rank3]
  - k_rrf = 60

RRF scores:
  doc1: 1/(60+1) + 1/(60+3) = 0.0164 + 0.0159 = 0.0323
  doc2: 1/(60+2) + 1/(60+1) = 0.0161 + 0.0164 = 0.0325  <- highest
  doc3: 1/(60+3) = 0.0159
  doc4: 1/(60+2) = 0.0161

Final ranking: [doc2, doc1, doc4, doc3]
```

### Tests

```rust
#[test]
fn test_rrf_fusion_basic() {
    let fuser = RRFFuser::default();

    let results = vec![
        (PrimitiveKind::Kv, make_response(&["A", "B", "C"])),
        (PrimitiveKind::Json, make_response(&["B", "D", "A"])),
    ];

    let fused = fuser.fuse(results, 10);

    // A and B appear in both lists, should rank higher
    let top_refs: Vec<_> = fused.hits.iter().take(2).map(|h| &h.doc_ref).collect();
    assert!(top_refs.iter().any(|r| is_doc_ref(r, "A")));
    assert!(top_refs.iter().any(|r| is_doc_ref(r, "B")));
}
```

### Complete Story

```bash
./scripts/complete-story.sh 326
```

---

## Story #327: Tie-Breaking for Determinism

**GitHub Issue**: [#327](https://github.com/anibjoshi/in-mem/issues/327)
**Estimated Time**: 2 hours
**Dependencies**: Story #326

### Start Story

```bash
gh issue view 327
./scripts/start-story.sh 37 327 tie-breaking
```

### Implementation

Update the sort in `RRFFuser::fuse()`:

```rust
// Sort with tie-breaking for determinism
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
```

Add to DocRef:

```rust
impl DocRef {
    fn stable_hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }
}
```

### Tests

```rust
#[test]
fn test_rrf_deterministic() {
    let fuser = RRFFuser::default();

    let results = || vec![
        (PrimitiveKind::Kv, make_response(&["A", "B"])),
        (PrimitiveKind::Json, make_response(&["C", "D"])),
    ];

    let fused1 = fuser.fuse(results(), 10);
    let fused2 = fuser.fuse(results(), 10);

    // Same inputs -> same output order
    for (h1, h2) in fused1.hits.iter().zip(fused2.hits.iter()) {
        assert_eq!(h1.doc_ref, h2.doc_ref);
        assert_eq!(h1.rank, h2.rank);
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 327
```

---

## Story #328: Result Deduplication

**GitHub Issue**: [#328](https://github.com/anibjoshi/in-mem/issues/328)
**Estimated Time**: 2 hours
**Dependencies**: Story #326

### Start Story

```bash
gh issue view 328
./scripts/start-story.sh 37 328 deduplication
```

### Implementation

The HashMap-based approach in RRFFuser already handles deduplication:

```rust
// HashMap naturally deduplicates by DocRef
// When same DocRef appears in multiple lists:
// - RRF scores are summed
// - First hit's metadata (snippet, etc.) is kept

let mut rrf_scores: HashMap<DocRef, f32> = HashMap::new();
let mut hit_data: HashMap<DocRef, SearchHit> = HashMap::new();

for (primitive, response) in results {
    for hit in response.hits {
        // Sum RRF contributions for same DocRef
        let rrf_contribution = 1.0 / (self.k_rrf as f32 + hit.rank as f32);
        *rrf_scores.entry(hit.doc_ref.clone()).or_insert(0.0) += rrf_contribution;

        // Keep first occurrence (has the original primitive's snippet)
        hit_data.entry(hit.doc_ref.clone()).or_insert(hit);
    }
}
```

### Tests

```rust
#[test]
fn test_rrf_deduplication() {
    let fuser = RRFFuser::default();

    let results = vec![
        (PrimitiveKind::Kv, make_response(&["A", "B"])),
        (PrimitiveKind::Json, make_response(&["A", "C"])),
    ];

    let fused = fuser.fuse(results, 10);

    // A should appear only once despite being in both lists
    let a_count = fused.hits.iter()
        .filter(|h| is_doc_ref(&h.doc_ref, "A"))
        .count();
    assert_eq!(a_count, 1);
}
```

### Complete Story

```bash
./scripts/complete-story.sh 328
```

---

## Epic 37 Completion Checklist

### 1. Final Validation

```bash
~/.cargo/bin/cargo test -p search -- fuser
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### 2. Verify Deliverables

- [ ] Fuser trait is Send + Sync
- [ ] RRFFuser implements RRF correctly
- [ ] Same inputs produce same outputs
- [ ] Duplicates are merged, not repeated

### 3. Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-37-fusion -m "Epic 37: Fusion Infrastructure complete

Delivered:
- Fuser trait for pluggable fusion
- RRFFuser with k=60 default
- Deterministic tie-breaking
- Automatic result deduplication

Stories: #325-#328
"
git push origin develop
gh issue close 299 --comment "Epic 37: Fusion Infrastructure - COMPLETE"
```
