# Epic 39: Validation & Non-Regression

**Goal**: Ensure correctness and maintain M5 performance

**Dependencies**: All other M6 epics

---

## Scope

- Search API contract tests
- Non-regression benchmark suite
- Determinism and snapshot consistency tests

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #289 | Search API Contract Tests | CRITICAL |
| #290 | Non-Regression Benchmark Suite | CRITICAL |
| #291 | Determinism and Snapshot Consistency Tests | CRITICAL |

---

## Story #289: Search API Contract Tests

**File**: `crates/search/tests/api_contracts.rs` (NEW)

**Deliverable**: Tests validating all search API contracts

### Test Categories

#### 1. Primitive Search Contracts

```rust
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
        }?;

        for hit in &response.hits {
            // DocRef must match primitive
            assert_eq!(hit.doc_ref.primitive_kind(), *primitive);

            // DocRef must be dereferenceable
            let data = db.deref_hit(&hit)?;
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

    db.kv.put(&run1, "key1", "shared term")?;
    db.kv.put(&run2, "key2", "shared term")?;

    let req = SearchRequest::new(run1, "shared");
    let response = db.kv.search(&req)?;

    // Should only return results from run1
    for hit in &response.hits {
        assert_eq!(hit.doc_ref.run_id(), run1);
    }
}
```

#### 2. Composite Search Contracts

```rust
/// Composite search orchestrates across primitives
#[test]
fn test_hybrid_search_orchestrates() {
    let db = test_db();

    db.kv.put(&run_id, "key1", "test value")?;
    db.json.create(&run_id, json!({"field": "test data"}))?;
    db.event.append(&run_id, "test_event", json!({"msg": "test"}))?;

    let req = SearchRequest::new(run_id, "test");
    let response = db.hybrid().search(&req)?;

    // Should have results from multiple primitives
    let primitives: HashSet<_> = response.hits.iter()
        .map(|h| h.doc_ref.primitive_kind())
        .collect();

    assert!(primitives.len() >= 2);
}

/// Primitive filter limits search scope
#[test]
fn test_hybrid_search_respects_filter() {
    let db = test_db();

    db.kv.put(&run_id, "key1", "test")?;
    db.json.create(&run_id, json!({"x": "test"}))?;

    let req = SearchRequest::new(run_id, "test")
        .with_primitive_filter(vec![PrimitiveKind::Kv]);

    let response = db.hybrid().search(&req)?;

    for hit in &response.hits {
        assert_eq!(hit.doc_ref.primitive_kind(), PrimitiveKind::Kv);
    }
}
```

#### 3. Budget Enforcement Contracts

```rust
/// Time budget causes early termination
#[test]
fn test_time_budget_enforced() {
    let db = test_db();
    populate_large_dataset(&db, 100_000);

    let req = SearchRequest::new(run_id, "common")
        .with_budget(SearchBudget::default().with_time(10_000));  // 10ms

    let start = Instant::now();
    let response = db.hybrid().search(&req)?;
    let elapsed = start.elapsed();

    // Should complete within budget (with margin)
    assert!(elapsed.as_micros() < 50_000);  // 50ms max

    // Should be marked truncated
    assert!(response.truncated);
}

/// Candidate budget causes early termination
#[test]
fn test_candidate_budget_enforced() {
    let db = test_db();
    populate_large_dataset(&db, 10_000);

    let req = SearchRequest::new(run_id, "common")
        .with_budget(SearchBudget::default().with_candidates(100));

    let response = db.hybrid().search(&req)?;

    assert!(response.stats.candidates_considered <= 100 * 6);  // 100 per primitive
    assert!(response.truncated);
}
```

### Acceptance Criteria

- [ ] All 6 primitive search() methods tested
- [ ] DocRef dereferencing tested for all variants
- [ ] run_id filtering tested
- [ ] Composite search orchestration tested
- [ ] Primitive filter tested
- [ ] Time budget enforcement tested
- [ ] Candidate budget enforcement tested
- [ ] Truncation flag correctly set

---

## Story #290: Non-Regression Benchmark Suite

**File**: `crates/benchmarks/benches/m6_non_regression.rs` (NEW)

**Deliverable**: Benchmarks ensuring M5 performance maintained

### Benchmark Categories

#### 1. Baseline Operations (Must Not Regress)

```rust
/// KV get latency - must stay < 5µs
#[bench]
fn bench_kv_get_latency(b: &mut Bencher) {
    let db = setup_db_with_data();
    let run_id = test_run_id();

    b.iter(|| {
        db.kv.get(&run_id, "key_500").unwrap()
    });
}

/// KV put latency - must stay < 8µs
#[bench]
fn bench_kv_put_latency(b: &mut Bencher) {
    let db = setup_db();
    let run_id = test_run_id();
    let mut i = 0;

    b.iter(|| {
        i += 1;
        db.kv.put(&run_id, &format!("key_{}", i), "value").unwrap()
    });
}

/// JSON get latency - must stay < 50µs
#[bench]
fn bench_json_get_latency(b: &mut Bencher) {
    let db = setup_db_with_json();
    let run_id = test_run_id();
    let doc_id = test_doc_id();

    b.iter(|| {
        db.json.get(&run_id, &doc_id, &JsonPath::root()).unwrap()
    });
}
```

#### 2. Zero Overhead Verification

```rust
/// Verify no overhead when search is not used
#[bench]
fn bench_kv_put_without_search(b: &mut Bencher) {
    let db = setup_db();
    // Search index NOT enabled

    b.iter(|| {
        db.kv.put(&run_id, "key", "value").unwrap()
    });
}

/// Compare: KV put with search index enabled
#[bench]
fn bench_kv_put_with_search_index(b: &mut Bencher) {
    let db = setup_db();
    db.enable_search_index(PrimitiveKind::Kv).unwrap();

    b.iter(|| {
        db.kv.put(&run_id, "key", "value").unwrap()
    });
}

// Assert: bench_kv_put_without_search ≈ bench_kv_put_with_search_index
// When search is not USED (just enabled), overhead should be minimal
```

#### 3. Search Performance Baselines

```rust
/// Search without index - establish baseline
#[bench]
fn bench_search_scan_1k_docs(b: &mut Bencher) {
    let db = setup_db_with_kv(1000);
    // Index NOT enabled

    let req = SearchRequest::new(run_id, "test");

    b.iter(|| {
        db.kv.search(&req).unwrap()
    });
}

/// Search with index - should be faster
#[bench]
fn bench_search_index_1k_docs(b: &mut Bencher) {
    let db = setup_db_with_kv(1000);
    db.enable_search_index(PrimitiveKind::Kv).unwrap();
    db.rebuild_search_index(PrimitiveKind::Kv).unwrap();

    let req = SearchRequest::new(run_id, "test");

    b.iter(|| {
        db.kv.search(&req).unwrap()
    });
}
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

### Acceptance Criteria

- [ ] All M5 latency targets maintained
- [ ] Zero overhead verified for non-search operations
- [ ] Search baseline established (with and without index)
- [ ] Benchmarks run in CI

---

## Story #291: Determinism and Snapshot Consistency Tests

**File**: `crates/search/tests/determinism.rs` (NEW)

**Deliverable**: Tests verifying deterministic and consistent search

### Test Categories

#### 1. Determinism Tests

```rust
/// Same request produces identical results
#[test]
fn test_search_deterministic() {
    let db = test_db();
    populate_test_data(&db);

    let req = SearchRequest::new(run_id, "test");

    let r1 = db.hybrid().search(&req)?;
    let r2 = db.hybrid().search(&req)?;

    // Same number of hits
    assert_eq!(r1.hits.len(), r2.hits.len());

    // Same order and scores
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

    // Create docs that will have equal scores
    db.kv.put(&run_id, "a", "test")?;
    db.kv.put(&run_id, "b", "test")?;
    db.kv.put(&run_id, "c", "test")?;

    let req = SearchRequest::new(run_id, "test");

    let r1 = db.kv.search(&req)?;
    let r2 = db.kv.search(&req)?;

    // Order must be stable
    let order1: Vec<_> = r1.hits.iter().map(|h| &h.doc_ref).collect();
    let order2: Vec<_> = r2.hits.iter().map(|h| &h.doc_ref).collect();
    assert_eq!(order1, order2);
}
```

#### 2. Snapshot Consistency Tests

```rust
/// Search doesn't see concurrent writes
#[test]
fn test_search_snapshot_isolation() {
    let db = test_db();
    db.kv.put(&run_id, "key1", "original")?;

    // Start a search (takes snapshot internally)
    let snapshot = db.snapshot();

    // Concurrent write after snapshot
    db.kv.put(&run_id, "key2", "concurrent")?;

    // Search with the earlier snapshot
    let req = SearchRequest::new(run_id, "concurrent");
    let response = db.kv.search_with_snapshot(&req, &snapshot)?;

    // Should NOT see the concurrent write
    assert!(response.is_empty());
}

/// Composite search uses single snapshot
#[test]
fn test_hybrid_search_atomic_snapshot() {
    let db = test_db();
    db.kv.put(&run_id, "key1", "test")?;
    db.json.create(&run_id, json!({"x": "test"}))?;

    // Modify one primitive while searching
    let search_thread = std::thread::spawn({
        let db = db.clone();
        move || db.hybrid().search(&SearchRequest::new(run_id, "test"))
    });

    // Add more data during search
    db.kv.put(&run_id, "key2", "test")?;

    let response = search_thread.join().unwrap()?;

    // Results should be from consistent snapshot
    // (either sees both adds or neither, not just one)
}
```

#### 3. Index Consistency Tests

```rust
/// Index-based search is consistent with scan
#[test]
fn test_index_consistent_with_scan() {
    let db = test_db();
    populate_test_data(&db);

    // Search without index
    let req = SearchRequest::new(run_id, "test");
    let scan_result = db.kv.search(&req)?;

    // Enable index and search again
    db.enable_search_index(PrimitiveKind::Kv)?;
    db.rebuild_search_index(PrimitiveKind::Kv)?;
    let index_result = db.kv.search(&req)?;

    // Should return same docs (order may differ due to scoring)
    let scan_refs: HashSet<_> = scan_result.hits.iter().map(|h| &h.doc_ref).collect();
    let index_refs: HashSet<_> = index_result.hits.iter().map(|h| &h.doc_ref).collect();

    assert_eq!(scan_refs, index_refs);
}

/// Stale index falls back to scan
#[test]
fn test_stale_index_fallback() {
    let db = test_db();
    db.enable_search_index(PrimitiveKind::Kv)?;

    db.kv.put(&run_id, "key1", "test")?;

    // Simulate stale index by not updating version
    // (in practice this happens with concurrent modifications)

    let req = SearchRequest::new(run_id, "test");
    let response = db.kv.search(&req)?;

    // Should still find the document (via scan fallback)
    assert!(!response.is_empty());
}
```

### Acceptance Criteria

- [ ] Same request always produces identical results
- [ ] Tie-breaking is deterministic
- [ ] Search uses snapshot for isolation
- [ ] Concurrent writes not visible in ongoing search
- [ ] Index results consistent with scan results
- [ ] Stale index falls back gracefully

---

## Summary Test Matrix

| Test Category | Coverage |
|---------------|----------|
| Primitive Search API | All 6 primitives |
| DocRef Dereferencing | All 6 variants |
| Run ID Filtering | Single and cross-run |
| Composite Orchestration | All primitives |
| Primitive Filtering | Include/exclude |
| Time Budget | Truncation verified |
| Candidate Budget | Truncation verified |
| Determinism | Same inputs = same outputs |
| Snapshot Consistency | Isolation from writes |
| Index Consistency | Match scan results |
| Non-Regression | M5 latency targets |
| Zero Overhead | Search disabled case |

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/search/tests/api_contracts.rs` | CREATE - API contract tests |
| `crates/search/tests/determinism.rs` | CREATE - Determinism tests |
| `crates/benchmarks/benches/m6_non_regression.rs` | CREATE - Benchmarks |
