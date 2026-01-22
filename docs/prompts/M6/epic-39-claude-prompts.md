# Epic 39: Validation & Non-Regression - Implementation Prompts

**Epic Goal**: Ensure correctness and maintain M5 performance

**GitHub Issue**: [#301](https://github.com/anibjoshi/in-mem/issues/301)
**Status**: Ready after all M6 implementation
**Dependencies**: All other M6 epics

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M6_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M6_ARCHITECTURE.md`
2. **Epic Spec**: `docs/milestones/M6/EPIC_39_VALIDATION.md`
3. **Prompt Header**: `docs/prompts/M6/M6_PROMPT_HEADER.md` for the 6 architectural rules

---

## Epic 39 Overview

### Scope
- Search API contract tests
- Non-regression benchmark suite
- Determinism and snapshot consistency tests

### Success Criteria
- [ ] All 6 primitive search() methods tested
- [ ] M4/M5 latency targets maintained
- [ ] Search is deterministic (same inputs = same outputs)
- [ ] Snapshot consistency verified
- [ ] Index results consistent with scan results

### Component Breakdown
- **Story #289 (GitHub #334)**: Search API Contract Tests - CRITICAL
- **Story #290 (GitHub #335)**: Non-Regression Benchmark Suite - CRITICAL
- **Story #291 (GitHub #336)**: Determinism and Snapshot Consistency Tests - CRITICAL

---

## Story #334: Search API Contract Tests

**GitHub Issue**: [#334](https://github.com/anibjoshi/in-mem/issues/334)
**Estimated Time**: 4 hours
**Dependencies**: All M6 implementation complete

### Start Story

```bash
gh issue view 334
./scripts/start-story.sh 39 334 api-contracts
```

### Implementation

Create `crates/search/tests/api_contracts.rs`:

```rust
//! Search API Contract Tests
//!
//! Validates all search API contracts across primitives.

use in_mem_core::*;
use in_mem_search::*;
use std::collections::HashSet;

/// Each primitive returns SearchResponse with valid DocRefs
#[test]
fn test_primitive_search_returns_valid_docref() {
    let db = test_db();
    populate_test_data(&db);

    for primitive in PrimitiveKind::all() {
        let req = SearchRequest::new(run_id, "test");
        let response = match primitive {
            PrimitiveKind::Kv => db.kv.search(&req),
            PrimitiveKind::Json => db.json.search(&req),
            PrimitiveKind::Event => db.event.search(&req),
            PrimitiveKind::State => db.state.search(&req),
            PrimitiveKind::Trace => db.trace.search(&req),
            PrimitiveKind::Run => db.run_index.search(&req),
        }.unwrap();

        for hit in &response.hits {
            // DocRef must match primitive
            assert_eq!(hit.doc_ref.primitive_kind(), *primitive);

            // DocRef must be dereferenceable
            let data = db.deref_hit(&hit).unwrap();
            assert!(data.is_some());
        }
    }
}

/// Search respects run_id filter
#[test]
fn test_search_respects_run_id() {
    let db = test_db();

    let run1 = RunId::new();
    let run2 = RunId::new();

    db.kv.put(&run1, "key1", "shared term").unwrap();
    db.kv.put(&run2, "key2", "shared term").unwrap();

    let req = SearchRequest::new(run1, "shared");
    let response = db.kv.search(&req).unwrap();

    for hit in &response.hits {
        assert_eq!(hit.doc_ref.run_id(), run1);
    }
}

/// Composite search orchestrates across primitives
#[test]
fn test_hybrid_search_orchestrates() {
    let db = test_db();

    db.kv.put(&run_id, "key1", "test value").unwrap();
    db.json.create(&run_id, json!({"field": "test data"})).unwrap();
    db.event.append(&run_id, "test_event", json!({"msg": "test"})).unwrap();

    let req = SearchRequest::new(run_id, "test");
    let response = db.hybrid().search(&req).unwrap();

    let primitives: HashSet<_> = response.hits.iter()
        .map(|h| h.doc_ref.primitive_kind())
        .collect();

    assert!(primitives.len() >= 2);
}

/// Primitive filter limits search scope
#[test]
fn test_hybrid_search_respects_filter() {
    let db = test_db();

    db.kv.put(&run_id, "key1", "test").unwrap();
    db.json.create(&run_id, json!({"x": "test"})).unwrap();

    let req = SearchRequest::new(run_id, "test")
        .with_primitive_filter(vec![PrimitiveKind::Kv]);

    let response = db.hybrid().search(&req).unwrap();

    for hit in &response.hits {
        assert_eq!(hit.doc_ref.primitive_kind(), PrimitiveKind::Kv);
    }
}

/// Time budget causes early termination
#[test]
fn test_time_budget_enforced() {
    let db = test_db();
    populate_large_dataset(&db, 100_000);

    let req = SearchRequest::new(run_id, "common")
        .with_budget(SearchBudget::default().with_time(10_000));  // 10ms

    let start = Instant::now();
    let response = db.hybrid().search(&req).unwrap();
    let elapsed = start.elapsed();

    assert!(elapsed.as_micros() < 50_000);  // 50ms max
    assert!(response.truncated);
}

/// Candidate budget causes early termination
#[test]
fn test_candidate_budget_enforced() {
    let db = test_db();
    populate_large_dataset(&db, 10_000);

    let req = SearchRequest::new(run_id, "common")
        .with_budget(SearchBudget::default().with_candidates(100));

    let response = db.hybrid().search(&req).unwrap();

    assert!(response.stats.candidates_considered <= 100 * 6);
    assert!(response.truncated);
}
```

### Complete Story

```bash
./scripts/complete-story.sh 334
```

---

## Story #335: Non-Regression Benchmark Suite

**GitHub Issue**: [#335](https://github.com/anibjoshi/in-mem/issues/335)
**Estimated Time**: 4 hours
**Dependencies**: All M6 implementation complete

### Start Story

```bash
gh issue view 335
./scripts/start-story.sh 39 335 benchmarks
```

### Implementation

Create `crates/benchmarks/benches/m6_non_regression.rs`:

```rust
//! M6 Non-Regression Benchmarks
//!
//! Ensures M4/M5 performance targets are maintained.

use criterion::{criterion_group, criterion_main, Criterion};

/// KV get latency - must stay < 5µs
fn bench_kv_get_latency(c: &mut Criterion) {
    let db = setup_db_with_data();
    let run_id = test_run_id();

    c.bench_function("m6/kv_get_latency", |b| {
        b.iter(|| {
            db.kv.get(&run_id, "key_500").unwrap()
        })
    });
}

/// KV put latency - must stay < 8µs
fn bench_kv_put_latency(c: &mut Criterion) {
    let db = setup_db();
    let run_id = test_run_id();
    let mut i = 0;

    c.bench_function("m6/kv_put_latency", |b| {
        b.iter(|| {
            i += 1;
            db.kv.put(&run_id, &format!("key_{}", i), "value").unwrap()
        })
    });
}

/// JSON get latency - must stay < 50µs
fn bench_json_get_latency(c: &mut Criterion) {
    let db = setup_db_with_json();
    let run_id = test_run_id();
    let doc_id = test_doc_id();

    c.bench_function("m6/json_get_latency", |b| {
        b.iter(|| {
            db.json.get(&run_id, &doc_id, &JsonPath::root()).unwrap()
        })
    });
}

/// Verify no overhead when search is not used
fn bench_kv_put_without_search(c: &mut Criterion) {
    let db = setup_db();
    // Search index NOT enabled

    c.bench_function("m6/kv_put_no_search", |b| {
        b.iter(|| {
            db.kv.put(&run_id, "key", "value").unwrap()
        })
    });
}

/// Compare: KV put with search index enabled
fn bench_kv_put_with_search_index(c: &mut Criterion) {
    let db = setup_db();
    db.enable_search_index(PrimitiveKind::Kv).unwrap();

    c.bench_function("m6/kv_put_with_index", |b| {
        b.iter(|| {
            db.kv.put(&run_id, "key", "value").unwrap()
        })
    });
}

/// Search without index - establish baseline
fn bench_search_scan_1k_docs(c: &mut Criterion) {
    let db = setup_db_with_kv(1000);
    let req = SearchRequest::new(run_id, "test");

    c.bench_function("m6/search_scan_1k", |b| {
        b.iter(|| {
            db.kv.search(&req).unwrap()
        })
    });
}

/// Search with index - should be faster
fn bench_search_index_1k_docs(c: &mut Criterion) {
    let db = setup_db_with_kv(1000);
    db.enable_search_index(PrimitiveKind::Kv).unwrap();
    db.rebuild_search_index(PrimitiveKind::Kv).unwrap();

    let req = SearchRequest::new(run_id, "test");

    c.bench_function("m6/search_index_1k", |b| {
        b.iter(|| {
            db.kv.search(&req).unwrap()
        })
    });
}

criterion_group!(
    benches,
    bench_kv_get_latency,
    bench_kv_put_latency,
    bench_json_get_latency,
    bench_kv_put_without_search,
    bench_kv_put_with_search_index,
    bench_search_scan_1k_docs,
    bench_search_index_1k_docs,
);
criterion_main!(benches);
```

### Performance Targets

| Operation | M5 Target | M6 Requirement |
|-----------|-----------|----------------|
| KV get | < 5 µs | < 5 µs |
| KV put | < 8 µs | < 8 µs |
| JSON get | 30-50 µs | 30-50 µs |
| JSON set | 100-200 µs | 100-200 µs |
| Search (1K, no index) | N/A | < 50 ms |
| Search (1K, indexed) | N/A | < 10 ms |

### Complete Story

```bash
./scripts/complete-story.sh 335
```

---

## Story #336: Determinism and Snapshot Consistency Tests

**GitHub Issue**: [#336](https://github.com/anibjoshi/in-mem/issues/336)
**Estimated Time**: 4 hours
**Dependencies**: All M6 implementation complete

### Start Story

```bash
gh issue view 336
./scripts/start-story.sh 39 336 determinism
```

### Implementation

Create `crates/search/tests/determinism.rs`:

```rust
//! Determinism and Snapshot Consistency Tests

/// Same request produces identical results
#[test]
fn test_search_deterministic() {
    let db = test_db();
    populate_test_data(&db);

    let req = SearchRequest::new(run_id, "test");

    let r1 = db.hybrid().search(&req).unwrap();
    let r2 = db.hybrid().search(&req).unwrap();

    assert_eq!(r1.hits.len(), r2.hits.len());

    for (h1, h2) in r1.hits.iter().zip(r2.hits.iter()) {
        assert_eq!(h1.doc_ref, h2.doc_ref);
        assert_eq!(h1.rank, h2.rank);
        assert!((h1.score - h2.score).abs() < 0.0001);
    }
}

/// Fusion is deterministic even with equal scores
#[test]
fn test_fusion_deterministic_tiebreak() {
    let db = test_db();

    db.kv.put(&run_id, "a", "test").unwrap();
    db.kv.put(&run_id, "b", "test").unwrap();
    db.kv.put(&run_id, "c", "test").unwrap();

    let req = SearchRequest::new(run_id, "test");

    let r1 = db.kv.search(&req).unwrap();
    let r2 = db.kv.search(&req).unwrap();

    let order1: Vec<_> = r1.hits.iter().map(|h| &h.doc_ref).collect();
    let order2: Vec<_> = r2.hits.iter().map(|h| &h.doc_ref).collect();
    assert_eq!(order1, order2);
}

/// Search doesn't see concurrent writes
#[test]
fn test_search_snapshot_isolation() {
    let db = test_db();
    db.kv.put(&run_id, "key1", "original").unwrap();

    let snapshot = db.snapshot();

    db.kv.put(&run_id, "key2", "concurrent").unwrap();

    let req = SearchRequest::new(run_id, "concurrent");
    let response = db.kv.search_with_snapshot(&req, &snapshot).unwrap();

    assert!(response.is_empty());
}

/// Index-based search is consistent with scan
#[test]
fn test_index_consistent_with_scan() {
    let db = test_db();
    populate_test_data(&db);

    let req = SearchRequest::new(run_id, "test");
    let scan_result = db.kv.search(&req).unwrap();

    db.enable_search_index(PrimitiveKind::Kv).unwrap();
    db.rebuild_search_index(PrimitiveKind::Kv).unwrap();
    let index_result = db.kv.search(&req).unwrap();

    let scan_refs: HashSet<_> = scan_result.hits.iter().map(|h| &h.doc_ref).collect();
    let index_refs: HashSet<_> = index_result.hits.iter().map(|h| &h.doc_ref).collect();

    assert_eq!(scan_refs, index_refs);
}

/// Stale index falls back to scan
#[test]
fn test_stale_index_fallback() {
    let db = test_db();
    db.enable_search_index(PrimitiveKind::Kv).unwrap();

    db.kv.put(&run_id, "key1", "test").unwrap();

    let req = SearchRequest::new(run_id, "test");
    let response = db.kv.search(&req).unwrap();

    assert!(!response.is_empty());
}
```

### Complete Story

```bash
./scripts/complete-story.sh 336
```

---

## Epic 39 Completion Checklist

### 1. Final Validation

```bash
# Run all M6 tests
~/.cargo/bin/cargo test --workspace

# Run M6 benchmarks
~/.cargo/bin/cargo bench --bench m6_non_regression

# Run M4/M5 non-regression
~/.cargo/bin/cargo bench --bench m4_performance
~/.cargo/bin/cargo bench --bench m5_json_performance

# Full lint
~/.cargo/bin/cargo clippy --workspace -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### 2. Verify Deliverables

- [ ] All 6 primitive search() methods tested
- [ ] Budget enforcement tested
- [ ] Determinism verified
- [ ] Snapshot consistency verified
- [ ] Index consistency verified
- [ ] M4/M5 targets maintained

### 3. Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-39-validation -m "Epic 39: Validation & Non-Regression complete

Delivered:
- Search API contract tests
- Non-regression benchmark suite
- Determinism tests
- Snapshot consistency tests
- Index consistency tests

Stories: #334-#336
"
git push origin develop
gh issue close 301 --comment "Epic 39: Validation & Non-Regression - COMPLETE"
```

---

## M6 Milestone Completion

After Epic 39 passes:

```bash
# Tag M6 completion
git tag -a m6-complete -m "M6: Retrieval Surfaces complete"
git push origin m6-complete

# Merge develop to main
git checkout main
git merge --no-ff develop -m "M6: Retrieval Surfaces - Complete

Milestones:
- Core Search Types (Epic 33)
- Primitive Search Surface (Epic 34)
- Scoring Infrastructure (Epic 35)
- Composite Search (Epic 36)
- Fusion Infrastructure (Epic 37)
- Optional Indexing (Epic 38)
- Validation & Non-Regression (Epic 39)

All tests passing. M4/M5 performance maintained.
"
git push origin main
```
