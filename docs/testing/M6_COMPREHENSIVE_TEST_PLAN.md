# M6 Comprehensive Test Plan

**Version**: 1.0
**Status**: Planning
**Date**: 2026-01-16

---

## Overview

This document defines the comprehensive test suite for M6 Retrieval Surfaces, **separate from the unit and integration tests written during development**.

The goal is to create a battery of tests that:
1. **Lock in semantic invariants** - Prevent accidental breakage in future refactors
2. **Validate the six architectural rules** - Every rule becomes executable
3. **Verify search correctness** - Same inputs always produce same outputs
4. **Test scoring accuracy** - BM25-lite produces reasonable rankings
5. **Verify fusion determinism** - RRF fusion is repeatable and correct
6. **Ensure snapshot consistency** - Search sees consistent data across primitives
7. **Test indexing correctness** - Index results match scan results
8. **Prevent regressions** - M4/M5 performance and semantics are maintained

---

## Test Structure

```
tests/
└── m6_comprehensive/
    ├── main.rs                           # Test harness and utilities
    │
    │   # Tier 1: Architectural Rule Invariants (MOST IMPORTANT)
    ├── docref_invariants.rs              # 1.1 No data movement (DocRef only)
    ├── primitive_search_invariants.rs    # 1.2 Primitive search first-class
    ├── composite_orchestration_tests.rs  # 1.3 Composite orchestrates
    ├── snapshot_search_invariants.rs     # 1.4 Snapshot-consistent search
    ├── zero_overhead_tests.rs            # 1.5 Zero overhead when disabled
    ├── algorithm_swappable_tests.rs      # 1.6 Scorer/Fuser are traits
    │
    │   # Tier 2: Search Correctness
    ├── search_determinism_tests.rs       # 2.1 Same inputs = same outputs
    ├── search_exhaustiveness_tests.rs    # 2.2 Exhaustiveness under various conditions
    ├── search_filter_tests.rs            # 2.3 Primitive filter correctness
    │
    │   # Tier 3: Budget Semantics (NOT Performance)
    ├── budget_truncation_tests.rs        # 3.1 Budget truncates, not corrupts
    ├── budget_ordering_tests.rs          # 3.2 Budget never changes prefix ordering
    ├── budget_isolation_tests.rs         # 3.3 Budget never violates snapshot isolation
    │
    │   # Tier 4: Scoring Accuracy
    ├── bm25_scoring_tests.rs             # 4.1 BM25-lite correctness
    ├── tokenizer_tests.rs                # 4.2 Tokenizer behavior
    ├── idf_calculation_tests.rs          # 4.3 IDF calculation
    │
    │   # Tier 5: Fusion Correctness
    ├── rrf_fusion_tests.rs               # 5.1 RRF algorithm correctness
    ├── fusion_determinism_tests.rs       # 5.2 Fusion is deterministic
    ├── tiebreak_tests.rs                 # 5.3 Tie-breaking is stable
    │
    │   # Tier 6: Cross-Primitive Identity
    ├── docref_identity_policy_tests.rs   # 6.1 Identity policy across primitives
    ├── deduplication_policy_tests.rs     # 6.2 Deduplication rules
    │
    │   # Tier 7: Index Consistency
    ├── index_scan_equivalence.rs         # 7.1 Index matches scan
    ├── index_update_tests.rs             # 7.2 Index tracks writes
    ├── watermark_tests.rs                # 7.3 Watermark correctness
    ├── stale_index_fallback_tests.rs     # 7.4 Fallback to scan
    │
    │   # Tier 8: Cross-Primitive Search
    ├── hybrid_search_tests.rs            # 8.1 Hybrid orchestration
    ├── multi_primitive_ranking.rs        # 8.2 Cross-primitive fusion
    │
    │   # Tier 9: Result Explainability (Future-Proofing)
    ├── result_provenance_tests.rs        # 9.1 Which primitive contributed
    ├── score_explanation_tests.rs        # 9.2 Which tokens matched, why this score
    ├── rank_contribution_tests.rs        # 9.3 Which rank sources contributed
    │
    │   # Tier 10: Property-Based/Fuzzing
    ├── search_fuzzing_tests.rs           # 10. Random query/data fuzzing
    │
    │   # Tier 11: Stress & Scale
    ├── search_stress_tests.rs            # 11. Large datasets, many queries
    │
    │   # Tier 12: Non-Regression
    ├── m4_m5_regression_tests.rs         # 12. M4/M5 targets maintained
    │
    │   # Tier 13: Spec Conformance
    └── spec_conformance_tests.rs         # 13. Direct spec-to-test mapping
```

---

## Tier 1: Architectural Rule Invariants (HIGHEST PRIORITY)

These tests ensure you **never accidentally violate the M6 contract** in future refactors.
They directly correspond to the six architectural rules.

### 1.1 No Data Movement (`docref_invariants.rs`)

**Rule 1**: DocRef references only, no content copying.

```rust
#[test]
fn test_search_returns_docref_not_data() {
    // Given: KV store with key "test_key" and value "test_value"
    // When: search("test") is called
    // Then: SearchHit contains DocRef, not the actual value
    //       DocRef.primitive_kind() == PrimitiveKind::Kv
    //       DocRef can be dereferenced to get actual data
}

#[test]
fn test_docref_is_lightweight() {
    // DocRef should be small (< 64 bytes typically)
    // Contains: primitive_kind, run_id, document_id
    assert!(std::mem::size_of::<DocRef>() <= 64);
}

#[test]
fn test_search_response_does_not_clone_content() {
    // Given: Large document (1MB)
    // When: search returns 100 hits
    // Then: Memory used by SearchResponse << 100MB
    //       (proves no content cloning)
}

#[test]
fn test_docref_dereference_requires_database() {
    // DocRef alone cannot access data
    // Must call db.deref_hit(&hit) to get actual content
}

#[test]
fn test_docref_preserves_identity_across_searches() {
    // Same document returns same DocRef in repeated searches
    let r1 = db.kv.search(&req);
    let r2 = db.kv.search(&req);
    assert_eq!(r1.hits[0].doc_ref, r2.hits[0].doc_ref);
}
```

### 1.2 Primitive Search First-Class (`primitive_search_invariants.rs`)

**Rule 2**: Every primitive has `.search()`.

```rust
#[test]
fn test_all_primitives_implement_searchable() {
    // All 6 primitives must have search()
    let _: fn(&KvStore, &SearchRequest) -> SearchResult = KvStore::search;
    let _: fn(&JsonStore, &SearchRequest) -> SearchResult = JsonStore::search;
    let _: fn(&EventLog, &SearchRequest) -> SearchResult = EventLog::search;
    let _: fn(&StateCell, &SearchRequest) -> SearchResult = StateCell::search;
    let _: fn(&TraceStore, &SearchRequest) -> SearchResult = TraceStore::search;
    let _: fn(&RunIndex, &SearchRequest) -> SearchResult = RunIndex::search;
}

#[test]
fn test_primitive_search_respects_run_id() {
    // Search only returns results from specified run_id
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

#[test]
fn test_primitive_search_returns_correct_primitive_kind() {
    // KV search returns PrimitiveKind::Kv
    // JSON search returns PrimitiveKind::Json
    // etc.
}

#[test]
fn test_primitive_search_works_independently() {
    // Can search one primitive without touching others
    // No cross-primitive coupling in single-primitive search
}

#[test]
fn test_kv_search_indexes_keys_and_values() {
    db.kv.put(&run_id, "user_name", "alice").unwrap();

    // Can search by key
    let by_key = db.kv.search(&SearchRequest::new(run_id, "user_name")).unwrap();
    assert!(!by_key.hits.is_empty());

    // Can search by value
    let by_val = db.kv.search(&SearchRequest::new(run_id, "alice")).unwrap();
    assert!(!by_val.hits.is_empty());
}

#[test]
fn test_json_search_indexes_all_string_values() {
    db.json.create(&run_id, json!({"name": "alice", "nested": {"city": "boston"}})).unwrap();

    // Can search nested values
    let result = db.json.search(&SearchRequest::new(run_id, "boston")).unwrap();
    assert!(!result.hits.is_empty());
}

#[test]
fn test_event_search_indexes_event_type_and_payload() {
    db.event.append(&run_id, "user.login", json!({"user": "alice"})).unwrap();

    // Can search event type
    let by_type = db.event.search(&SearchRequest::new(run_id, "login")).unwrap();
    assert!(!by_type.hits.is_empty());

    // Can search payload
    let by_payload = db.event.search(&SearchRequest::new(run_id, "alice")).unwrap();
    assert!(!by_payload.hits.is_empty());
}
```

### 1.3 Composite Orchestrates (`composite_orchestration_tests.rs`)

**Rule 3**: Composite orchestrates, doesn't replace.

```rust
#[test]
fn test_hybrid_delegates_to_primitives() {
    // db.hybrid().search() internally calls each primitive's search()
    // Verify by checking that results from hybrid contain all primitive types

    db.kv.put(&run_id, "key", "test").unwrap();
    db.json.create(&run_id, json!({"x": "test"})).unwrap();
    db.event.append(&run_id, "test_event", json!({})).unwrap();

    let response = db.hybrid().search(&SearchRequest::new(run_id, "test")).unwrap();

    let primitive_kinds: HashSet<_> = response.hits.iter()
        .map(|h| h.doc_ref.primitive_kind())
        .collect();

    assert!(primitive_kinds.contains(&PrimitiveKind::Kv));
    assert!(primitive_kinds.contains(&PrimitiveKind::Json));
    assert!(primitive_kinds.contains(&PrimitiveKind::Event));
}

#[test]
fn test_hybrid_search_results_superset_of_primitive_search() {
    // Results from db.hybrid().search() should be union of individual primitive searches
    // (before ranking/truncation)
}

#[test]
fn test_hybrid_does_not_duplicate_primitive_logic() {
    // db.hybrid() should not re-implement search
    // It should only orchestrate and fuse results
    // (This is more of a code review check, but test by verifying
    //  that disabling a primitive's search disables it in hybrid too)
}

#[test]
fn test_primitive_search_still_works_without_hybrid() {
    // Can call db.kv.search() directly without going through hybrid
    // Primitives are self-sufficient
}
```

### 1.4 Snapshot-Consistent Search (`snapshot_search_invariants.rs`)

**Rule 4**: Search sees consistent data across primitives.

```rust
#[test]
fn test_hybrid_search_uses_single_snapshot() {
    // All primitives in a hybrid search see the same snapshot
    // No partial views where KV sees write but Event doesn't
}

#[test]
fn test_search_does_not_see_concurrent_writes() {
    // Given: Snapshot taken at T1
    // When: Write happens at T2 > T1
    // Then: Search (using T1 snapshot) does not see T2 write
}

#[test]
fn test_search_snapshot_isolation() {
    let db = test_db();
    db.kv.put(&run_id, "key1", "original").unwrap();

    let snapshot = db.snapshot();

    db.kv.put(&run_id, "key2", "concurrent").unwrap();

    let req = SearchRequest::new(run_id, "concurrent");
    let response = db.kv.search_with_snapshot(&req, &snapshot).unwrap();

    assert!(response.is_empty()); // Snapshot doesn't see concurrent write
}

#[test]
fn test_repeated_search_same_snapshot_same_results() {
    // Same query on same snapshot always returns same results
    // Deterministic and isolated
}

#[test]
fn test_hybrid_snapshot_atomic() {
    // Start hybrid search
    // During search, another thread writes
    // Search results are consistent (all from before or all from after)
}
```

### 1.5 Zero Overhead When Disabled (`zero_overhead_tests.rs`)

**Rule 5**: No allocations when indexing is off.

```rust
#[test]
fn test_no_index_allocation_when_disabled() {
    let db = test_db();
    // Index NOT enabled

    // Write operations should not allocate index structures
    let alloc_before = get_allocator_stats();

    for i in 0..1000 {
        db.kv.put(&run_id, &format!("key_{}", i), "value").unwrap();
    }

    let alloc_after = get_allocator_stats();

    // Should not see InvertedIndex, PostingList allocations
    assert!(!allocs_contain_index_structures(alloc_before, alloc_after));
}

#[test]
fn test_search_works_via_scan_when_index_disabled() {
    let db = test_db();
    // Index NOT enabled

    db.kv.put(&run_id, "key", "searchable value").unwrap();

    let req = SearchRequest::new(run_id, "searchable");
    let response = db.kv.search(&req).unwrap();

    // Still works via fallback scan
    assert!(!response.hits.is_empty());
}

#[test]
fn test_write_latency_unchanged_when_index_disabled() {
    let db = test_db();
    // Index NOT enabled

    // Measure write latency
    let latencies = (0..1000).map(|i| {
        let start = Instant::now();
        db.kv.put(&run_id, &format!("key_{}", i), "value").unwrap();
        start.elapsed()
    }).collect::<Vec<_>>();

    // Should be within M4 targets (< 8µs)
    let mean = latencies.iter().map(|d| d.as_nanos()).sum::<u128>() / 1000;
    assert!(mean < 8000); // 8µs
}

#[test]
fn test_enable_index_allocates_structures() {
    let db = test_db();

    db.enable_search_index(PrimitiveKind::Kv).unwrap();

    // Now index structures should exist
    assert!(db.kv.has_index());
}

#[test]
fn test_disable_index_frees_structures() {
    let db = test_db();

    db.enable_search_index(PrimitiveKind::Kv).unwrap();
    db.disable_search_index(PrimitiveKind::Kv).unwrap();

    // Index structures should be freed
    assert!(!db.kv.has_index());
}
```

### 1.6 Algorithm Swappable (`algorithm_swappable_tests.rs`)

**Rule 6**: Scorer and Fuser are traits, not hardcoded.

```rust
#[test]
fn test_scorer_is_trait() {
    // Scorer must be a trait, allowing custom implementations
    struct CustomScorer;
    impl Scorer for CustomScorer {
        fn score(&self, doc: &SearchDoc, query: &str, ctx: &ScorerContext) -> f32 {
            1.0 // Constant score
        }
        fn name(&self) -> &str { "custom" }
    }

    // Can use custom scorer
    let scorer = CustomScorer;
    let score = scorer.score(&doc, "query", &ctx);
    assert_eq!(score, 1.0);
}

#[test]
fn test_fuser_is_trait() {
    // Fuser must be a trait, allowing custom implementations
    struct CustomFuser;
    impl Fuser for CustomFuser {
        fn fuse(&self, results: Vec<PrimitiveSearchResult>) -> Vec<SearchHit> {
            // Custom fusion logic
            vec![]
        }
        fn name(&self) -> &str { "custom" }
    }
}

#[test]
fn test_can_swap_scorer_at_runtime() {
    let db = test_db();

    // Use BM25LiteScorer (default)
    let bm25_results = db.search_with_scorer(&req, &BM25LiteScorer::default()).unwrap();

    // Use custom scorer
    let custom_results = db.search_with_scorer(&req, &ConstantScorer(1.0)).unwrap();

    // Results have different scores
    assert_ne!(bm25_results.hits[0].score, custom_results.hits[0].score);
}

#[test]
fn test_can_swap_fuser_at_runtime() {
    let db = test_db();

    // Use RRFFuser (default)
    let rrf_results = db.hybrid().search_with_fuser(&req, &RRFFuser::default()).unwrap();

    // Use custom fuser
    let custom_results = db.hybrid().search_with_fuser(&req, &InterleavingFuser).unwrap();

    // Different fusion produces different ranking
}

#[test]
fn test_default_scorer_is_bm25_lite() {
    let default_scorer = default_scorer();
    assert_eq!(default_scorer.name(), "bm25-lite");
}

#[test]
fn test_default_fuser_is_rrf() {
    let default_fuser = default_fuser();
    assert_eq!(default_fuser.name(), "rrf");
}
```

---

## Tier 2: Search Correctness

### 2.1 Search Determinism (`search_determinism_tests.rs`)

```rust
#[test]
fn test_same_query_same_results() {
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

#[test]
fn test_deterministic_across_sessions() {
    // Create DB, populate, close
    // Reopen DB
    // Same query produces same results
}

#[test]
fn test_deterministic_with_equal_scores() {
    // Documents with identical scores
    // Tie-breaker produces consistent ordering
}

#[test]
fn test_ordering_is_deterministic() {
    // Even with budget truncation, the order of returned results
    // must be identical across repeated calls
}
```

### 2.2 Search Exhaustiveness (`search_exhaustiveness_tests.rs`)

**IMPORTANT**: "Exhaustiveness" has different meanings depending on context.
These tests formalize each definition explicitly.

```rust
/// DEFINITION: Exhaustiveness under unlimited budget
/// With unlimited budget, search MUST return ALL matching documents.
/// This is the ground truth definition.
#[test]
fn test_exhaustive_unlimited_budget_finds_all() {
    let db = test_db();

    // Create 100 documents with "needle" in them
    for i in 0..100 {
        db.kv.put(&run_id, &format!("key_{}", i), &format!("haystack needle {}", i)).unwrap();
    }

    let req = SearchRequest::new(run_id, "needle")
        .with_budget(SearchBudget::unlimited()); // CRITICAL: No truncation

    let response = db.kv.search(&req).unwrap();

    assert_eq!(response.hits.len(), 100, "Unlimited budget must find ALL matches");
    assert!(!response.truncated, "Unlimited budget must not truncate");
}

/// DEFINITION: Exhaustiveness with index enabled
/// Index search with unlimited budget MUST return same documents as scan.
#[test]
fn test_exhaustive_index_matches_scan() {
    let db = test_db();
    populate_test_data(&db);

    let req = SearchRequest::new(run_id, "test")
        .with_budget(SearchBudget::unlimited());

    // Scan (no index)
    let scan_result = db.kv.search(&req).unwrap();

    // Enable index
    db.enable_search_index(PrimitiveKind::Kv).unwrap();
    db.rebuild_search_index(PrimitiveKind::Kv).unwrap();

    // Index search
    let index_result = db.kv.search(&req).unwrap();

    let scan_refs: HashSet<_> = scan_result.hits.iter().map(|h| &h.doc_ref).collect();
    let index_refs: HashSet<_> = index_result.hits.iter().map(|h| &h.doc_ref).collect();

    assert_eq!(scan_refs, index_refs, "Index must find exactly what scan finds");
}

/// DEFINITION: Exhaustiveness under truncation
/// Truncation DOES NOT mean incorrect results.
/// The returned prefix must be a valid prefix of the full result set.
#[test]
fn test_exhaustive_truncation_returns_valid_prefix() {
    let db = test_db();

    // Create 100 documents
    for i in 0..100 {
        db.kv.put(&run_id, &format!("key_{}", i), "searchable").unwrap();
    }

    // Get full results first
    let full_req = SearchRequest::new(run_id, "searchable")
        .with_budget(SearchBudget::unlimited());
    let full_result = db.kv.search(&full_req).unwrap();

    // Get truncated results
    let truncated_req = SearchRequest::new(run_id, "searchable")
        .with_budget(SearchBudget::default().with_max_results(10));
    let truncated_result = db.kv.search(&truncated_req).unwrap();

    // Truncated results must be prefix of full results
    for (i, hit) in truncated_result.hits.iter().enumerate() {
        assert_eq!(hit.doc_ref, full_result.hits[i].doc_ref,
            "Truncated result at position {} must match full result", i);
    }
}

/// DEFINITION: Truncation is explicit
/// When results are truncated, the response MUST indicate this.
#[test]
fn test_truncation_is_explicit() {
    let db = test_db();

    for i in 0..100 {
        db.kv.put(&run_id, &format!("key_{}", i), "searchable").unwrap();
    }

    let req = SearchRequest::new(run_id, "searchable")
        .with_budget(SearchBudget::default().with_max_results(10));

    let response = db.kv.search(&req).unwrap();

    assert!(response.truncated, "Response MUST indicate truncation occurred");
    assert!(response.hits.len() <= 10, "Truncation must respect limit");
}

/// DEFINITION: Non-truncation is explicit
/// When all results are returned, truncated must be false.
#[test]
fn test_non_truncation_is_explicit() {
    let db = test_db();

    for i in 0..5 {
        db.kv.put(&run_id, &format!("key_{}", i), "searchable").unwrap();
    }

    let req = SearchRequest::new(run_id, "searchable")
        .with_budget(SearchBudget::default().with_max_results(100)); // More than exist

    let response = db.kv.search(&req).unwrap();

    assert!(!response.truncated, "Response MUST indicate no truncation when all returned");
    assert_eq!(response.hits.len(), 5);
}

#[test]
fn test_search_case_insensitive() {
    db.kv.put(&run_id, "key", "UPPERCASE Value").unwrap();

    let result = db.kv.search(&SearchRequest::new(run_id, "uppercase")).unwrap();
    assert!(!result.hits.is_empty());
}
```

### 2.3 Primitive Filter (`search_filter_tests.rs`)

```rust
#[test]
fn test_primitive_filter_limits_scope() {
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

#[test]
fn test_empty_filter_returns_nothing() {
    let req = SearchRequest::new(run_id, "test")
        .with_primitive_filter(vec![]);

    let response = db.hybrid().search(&req).unwrap();
    assert!(response.hits.is_empty());
}

#[test]
fn test_multiple_primitives_in_filter() {
    let req = SearchRequest::new(run_id, "test")
        .with_primitive_filter(vec![PrimitiveKind::Kv, PrimitiveKind::Event]);

    let response = db.hybrid().search(&req).unwrap();

    for hit in &response.hits {
        assert!(hit.doc_ref.primitive_kind() == PrimitiveKind::Kv ||
                hit.doc_ref.primitive_kind() == PrimitiveKind::Event);
    }
}
```

---

## Tier 3: Budget Semantics (NOT Performance)

**CRITICAL**: These are SEMANTIC tests, not performance tests.
Budget is a resource cap, not a correctness condition.
These tests ensure budget enforcement never corrupts results.

### 3.1 Budget Truncation Semantics (`budget_truncation_tests.rs`)

```rust
/// Budget is a SOFT CAP, not a correctness condition.
/// Truncation means "we stopped early", not "we returned wrong results".
#[test]
fn test_budget_truncates_not_corrupts() {
    let db = test_db();

    for i in 0..1000 {
        db.kv.put(&run_id, &format!("key_{}", i), "searchable term").unwrap();
    }

    // Get full results
    let full_req = SearchRequest::new(run_id, "searchable")
        .with_budget(SearchBudget::unlimited());
    let full_result = db.kv.search(&full_req).unwrap();

    // Get truncated results
    let truncated_req = SearchRequest::new(run_id, "searchable")
        .with_budget(SearchBudget::default().with_max_results(10));
    let truncated_result = db.kv.search(&truncated_req).unwrap();

    // Every hit in truncated result must exist in full result
    for hit in &truncated_result.hits {
        assert!(full_result.hits.iter().any(|h| h.doc_ref == hit.doc_ref),
            "Truncated result contains hit not in full result - CORRUPTION!");
    }
}

/// Budget never introduces phantom results.
#[test]
fn test_budget_never_introduces_phantoms() {
    let db = test_db();

    for i in 0..100 {
        db.kv.put(&run_id, &format!("key_{}", i), "searchable").unwrap();
    }

    let req = SearchRequest::new(run_id, "searchable")
        .with_budget(SearchBudget::default().with_max_results(50));

    let response = db.kv.search(&req).unwrap();

    // Every returned doc must actually match the query
    for hit in &response.hits {
        let doc = db.deref_hit(&hit).unwrap();
        assert!(doc.contains("searchable"), "Result does not match query - phantom!");
    }
}

/// Budget never introduces duplicates.
#[test]
fn test_budget_never_introduces_duplicates() {
    let db = test_db();

    for i in 0..100 {
        db.kv.put(&run_id, &format!("key_{}", i), "searchable").unwrap();
    }

    let req = SearchRequest::new(run_id, "searchable")
        .with_budget(SearchBudget::default().with_max_results(50));

    let response = db.kv.search(&req).unwrap();

    let unique_refs: HashSet<_> = response.hits.iter().map(|h| &h.doc_ref).collect();
    assert_eq!(unique_refs.len(), response.hits.len(),
        "Budget introduced duplicate results!");
}
```

### 3.2 Budget Ordering Semantics (`budget_ordering_tests.rs`)

```rust
/// Budget NEVER changes the ordering of returned results.
/// The truncated result must be a PREFIX of the full result.
#[test]
fn test_budget_preserves_ordering() {
    let db = test_db();

    for i in 0..100 {
        db.kv.put(&run_id, &format!("key_{}", i), &format!("searchable term {}", i)).unwrap();
    }

    // Get full results
    let full_req = SearchRequest::new(run_id, "searchable")
        .with_budget(SearchBudget::unlimited());
    let full_result = db.kv.search(&full_req).unwrap();

    // Get truncated results at various limits
    for limit in [5, 10, 20, 50] {
        let truncated_req = SearchRequest::new(run_id, "searchable")
            .with_budget(SearchBudget::default().with_max_results(limit));
        let truncated_result = db.kv.search(&truncated_req).unwrap();

        // Truncated must be exact prefix
        for (i, hit) in truncated_result.hits.iter().enumerate() {
            assert_eq!(hit.doc_ref, full_result.hits[i].doc_ref,
                "Budget changed ordering at position {}!", i);
            assert_eq!(hit.rank, full_result.hits[i].rank,
                "Budget changed rank at position {}!", i);
        }
    }
}

/// Budget never changes scores of returned results.
#[test]
fn test_budget_preserves_scores() {
    let db = test_db();

    for i in 0..100 {
        db.kv.put(&run_id, &format!("key_{}", i), "searchable").unwrap();
    }

    let full_req = SearchRequest::new(run_id, "searchable")
        .with_budget(SearchBudget::unlimited());
    let full_result = db.kv.search(&full_req).unwrap();

    let truncated_req = SearchRequest::new(run_id, "searchable")
        .with_budget(SearchBudget::default().with_max_results(10));
    let truncated_result = db.kv.search(&truncated_req).unwrap();

    for (i, hit) in truncated_result.hits.iter().enumerate() {
        assert!((hit.score - full_result.hits[i].score).abs() < 0.0001,
            "Budget changed score at position {}!", i);
    }
}

/// Different budget limits produce consistent prefixes.
#[test]
fn test_budget_limits_are_nested_prefixes() {
    let db = test_db();

    for i in 0..100 {
        db.kv.put(&run_id, &format!("key_{}", i), "searchable").unwrap();
    }

    let result_10 = db.kv.search(&SearchRequest::new(run_id, "searchable")
        .with_budget(SearchBudget::default().with_max_results(10))).unwrap();

    let result_20 = db.kv.search(&SearchRequest::new(run_id, "searchable")
        .with_budget(SearchBudget::default().with_max_results(20))).unwrap();

    // result_10 must be prefix of result_20
    for (i, hit) in result_10.hits.iter().enumerate() {
        assert_eq!(hit.doc_ref, result_20.hits[i].doc_ref,
            "Smaller budget not prefix of larger budget at {}!", i);
    }
}
```

### 3.3 Budget Isolation Semantics (`budget_isolation_tests.rs`)

```rust
/// Budget NEVER violates snapshot isolation.
/// Even under budget pressure, we never see partial snapshots.
#[test]
fn test_budget_respects_snapshot_isolation() {
    let db = test_db();

    db.kv.put(&run_id, "key1", "searchable").unwrap();

    let snapshot = db.snapshot();

    // Write after snapshot
    db.kv.put(&run_id, "key2", "searchable concurrent").unwrap();

    // Search with budget using snapshot
    let req = SearchRequest::new(run_id, "searchable")
        .with_budget(SearchBudget::default().with_max_results(100));

    let response = db.kv.search_with_snapshot(&req, &snapshot).unwrap();

    // Should NOT see key2
    for hit in &response.hits {
        let key = hit.doc_ref.key();
        assert_ne!(key, "key2", "Budget violated snapshot isolation!");
    }
}

/// Budget under time pressure still respects snapshot.
#[test]
fn test_time_budget_respects_snapshot() {
    let db = test_db();

    for i in 0..10000 {
        db.kv.put(&run_id, &format!("key_{}", i), "searchable").unwrap();
    }

    let snapshot = db.snapshot();

    // Write many more after snapshot
    for i in 10000..20000 {
        db.kv.put(&run_id, &format!("key_{}", i), "searchable concurrent").unwrap();
    }

    // Search with tight time budget
    let req = SearchRequest::new(run_id, "searchable")
        .with_budget(SearchBudget::default().with_time_micros(1000)); // 1ms

    let response = db.kv.search_with_snapshot(&req, &snapshot).unwrap();

    // All results must be from snapshot (key < 10000)
    for hit in &response.hits {
        let key = hit.doc_ref.key();
        let num: usize = key.strip_prefix("key_").unwrap().parse().unwrap();
        assert!(num < 10000, "Time budget violated snapshot isolation!");
    }
}

/// Budget never produces inconsistent cross-primitive views.
#[test]
fn test_budget_cross_primitive_consistency() {
    let db = test_db();

    db.kv.put(&run_id, "key1", "searchable").unwrap();
    db.event.append(&run_id, "test.event", json!({"data": "searchable"})).unwrap();

    let snapshot = db.snapshot();

    // Concurrent writes
    db.kv.put(&run_id, "key2", "searchable new").unwrap();
    db.event.append(&run_id, "test.event2", json!({"data": "searchable new"})).unwrap();

    let req = SearchRequest::new(run_id, "searchable")
        .with_budget(SearchBudget::default().with_max_results(100));

    let response = db.hybrid().search_with_snapshot(&req, &snapshot).unwrap();

    // All results from all primitives must be from same snapshot
    // (Can't see key2 from KV but old event from Event)
}
```

---

## Tier 4: Scoring Accuracy

### 4.1 BM25-Lite Scoring (`bm25_scoring_tests.rs`)

```rust
#[test]
fn test_bm25_prefers_rare_terms() {
    // Rare term should have higher IDF → higher score
    // "xyzzy" (rare) should score higher than "the" (common)
}

#[test]
fn test_bm25_prefers_term_frequency() {
    // Document with term appearing 5 times scores higher than
    // document with term appearing 1 time
}

#[test]
fn test_bm25_length_normalization() {
    // Shorter documents with same term count score higher
    // (BM25 normalizes by document length)
}

#[test]
fn test_bm25_multi_term_query() {
    // Query "quick brown fox"
    // Document with all 3 terms scores higher than
    // document with only 1 term
}

#[test]
fn test_bm25_no_match_returns_zero() {
    let scorer = BM25LiteScorer::default();
    let doc = SearchDoc::new("hello world".into());
    let ctx = ScorerContext::default();

    let score = scorer.score(&doc, "banana", &ctx);
    assert_eq!(score, 0.0);
}

#[test]
fn test_bm25_title_boost() {
    let scorer = BM25LiteScorer::default();
    let doc_with_title = SearchDoc::new("content".into())
        .with_title("query term in title".into());
    let doc_without = SearchDoc::new("query term in content only".into());
    let ctx = ScorerContext::default();

    let score_with = scorer.score(&doc_with_title, "query", &ctx);
    let score_without = scorer.score(&doc_without, "query", &ctx);

    assert!(score_with > score_without); // Title match gets boost
}
```

### 4.2 Tokenizer Behavior (`tokenizer_tests.rs`)

```rust
#[test]
fn test_tokenizer_lowercases() {
    let tokens = tokenize("Hello WORLD");
    assert_eq!(tokens, vec!["hello", "world"]);
}

#[test]
fn test_tokenizer_splits_on_non_alphanumeric() {
    let tokens = tokenize("hello-world_test.case");
    assert_eq!(tokens, vec!["hello", "world", "test", "case"]);
}

#[test]
fn test_tokenizer_filters_short_tokens() {
    let tokens = tokenize("I am a test");
    assert_eq!(tokens, vec!["am", "test"]); // Filters single chars
}

#[test]
fn test_tokenizer_handles_numbers() {
    let tokens = tokenize("user123 test456");
    assert_eq!(tokens, vec!["user123", "test456"]);
}

#[test]
fn test_tokenizer_empty_string() {
    let tokens = tokenize("");
    assert!(tokens.is_empty());
}

#[test]
fn test_tokenize_unique_deduplicates() {
    let tokens = tokenize_unique("test test TEST");
    assert_eq!(tokens, vec!["test"]);
}
```

### 4.3 IDF Calculation (`idf_calculation_tests.rs`)

```rust
#[test]
fn test_idf_rare_term_high() {
    let ctx = ScorerContext {
        total_docs: 1000,
        doc_freqs: [("rare".into(), 1)].into_iter().collect(),
        ..Default::default()
    };

    let idf = ctx.idf("rare");
    assert!(idf > 5.0); // High IDF for rare term
}

#[test]
fn test_idf_common_term_low() {
    let ctx = ScorerContext {
        total_docs: 1000,
        doc_freqs: [("common".into(), 900)].into_iter().collect(),
        ..Default::default()
    };

    let idf = ctx.idf("common");
    assert!(idf < 1.0); // Low IDF for common term
}

#[test]
fn test_idf_unknown_term() {
    let ctx = ScorerContext::new(1000);

    let idf = ctx.idf("unknown");
    // Unknown term should have highest IDF (most rare)
}
```

---

## Tier 5: Fusion Correctness

### 5.1 RRF Algorithm (`rrf_fusion_tests.rs`)

```rust
#[test]
fn test_rrf_formula_correct() {
    // RRF score = sum of 1/(k + rank) for each result list
    // k is typically 60

    // Doc at rank 1 in list A: 1/(60+1) = 0.0164
    // Doc at rank 1 in list B: 1/(60+1) = 0.0164
    // Combined: 0.0328
}

#[test]
fn test_rrf_prefers_multi_list_presence() {
    // Doc appearing in 3 lists ranks higher than
    // doc appearing in 1 list (even if rank 1)
}

#[test]
fn test_rrf_rank_position_matters() {
    // Doc at rank 1 in one list beats
    // doc at rank 10 in same list
}

#[test]
fn test_rrf_handles_single_source() {
    // When only one primitive returns results
    // RRF still produces valid ranking
}

#[test]
fn test_rrf_empty_sources() {
    // When all primitives return empty
    // RRF returns empty result
}
```

### 5.2 Fusion Determinism (`fusion_determinism_tests.rs`)

```rust
#[test]
fn test_fusion_deterministic() {
    let fuser = RRFFuser::default();

    let sources = vec![
        PrimitiveSearchResult { primitive: PrimitiveKind::Kv, hits: kv_hits },
        PrimitiveSearchResult { primitive: PrimitiveKind::Json, hits: json_hits },
    ];

    let r1 = fuser.fuse(sources.clone());
    let r2 = fuser.fuse(sources.clone());

    assert_eq!(r1, r2);
}

#[test]
fn test_fusion_order_independent() {
    // [KV results, JSON results] produces same fusion as
    // [JSON results, KV results]
    // (RRF is order-independent by design)
}
```

### 5.3 Tie-Breaking (`tiebreak_tests.rs`)

```rust
#[test]
fn test_tiebreak_is_deterministic() {
    // Documents with equal RRF scores
    // Always sort in same order
}

#[test]
fn test_tiebreak_by_docref() {
    // When scores are equal, use DocRef as tiebreaker
    // (Ensures determinism)
}

#[test]
fn test_tiebreak_stable_across_sessions() {
    // Same ties broken same way after restart
}
```

---

## Tier 6: Cross-Primitive Identity

**CRITICAL POLICY DECISIONS**: This tier documents and tests the identity policy.
These tests encode design decisions that MUST be explicit.

### 6.1 DocRef Identity Policy (`docref_identity_policy_tests.rs`)

```rust
/// POLICY: What does it mean for two DocRefs to be "the same entity"?
///
/// M6 POLICY: DocRefs are NEVER considered equal across primitives.
/// - KV DocRef("key1") != JSON DocRef(doc_id)
/// - Even if they store "the same" logical data
///
/// RATIONALE: Primitives have different semantics. KV is key-value,
/// JSON is document-oriented. They are structurally different.
///
/// FUTURE: If cross-primitive identity is needed, it must be
/// implemented via explicit linking (e.g., foreign keys), not
/// implicit deduplication.

#[test]
fn test_docrefs_from_different_primitives_are_never_equal() {
    let kv_ref = DocRef::new(PrimitiveKind::Kv, run_id, "key1");
    let json_ref = DocRef::new(PrimitiveKind::Json, run_id, doc_id);

    // POLICY: These are NEVER equal, even if they reference "same" data
    assert_ne!(kv_ref, json_ref);
}

#[test]
fn test_docref_equality_requires_same_primitive() {
    let ref1 = DocRef::new(PrimitiveKind::Kv, run_id, "key1");
    let ref2 = DocRef::new(PrimitiveKind::Kv, run_id, "key1");
    let ref3 = DocRef::new(PrimitiveKind::Kv, run_id, "key2");

    assert_eq!(ref1, ref2, "Same primitive + same key = equal");
    assert_ne!(ref1, ref3, "Same primitive + different key = not equal");
}

#[test]
fn test_docref_equality_requires_same_run() {
    let run1 = RunId::new();
    let run2 = RunId::new();

    let ref1 = DocRef::new(PrimitiveKind::Kv, run1, "key1");
    let ref2 = DocRef::new(PrimitiveKind::Kv, run2, "key1");

    assert_ne!(ref1, ref2, "Different run = never equal");
}
```

### 6.2 Deduplication Policy (`deduplication_policy_tests.rs`)

```rust
/// POLICY: When does deduplication occur?
///
/// M6 POLICY:
/// 1. Within a single primitive: NEVER duplicates (guaranteed by primitive)
/// 2. Across primitives: NO deduplication (they are different entities)
///
/// RATIONALE: Cross-primitive dedup would require defining what "same"
/// means across structurally different data types. This is application
/// logic, not search infrastructure.

#[test]
fn test_within_primitive_never_duplicates() {
    let db = test_db();

    // Same key, updated multiple times
    db.kv.put(&run_id, "key1", "version1").unwrap();
    db.kv.put(&run_id, "key1", "version2").unwrap();

    let result = db.kv.search(&SearchRequest::new(run_id, "version")).unwrap();

    // Only ONE result for key1 (latest value)
    let key1_hits: Vec<_> = result.hits.iter()
        .filter(|h| h.doc_ref.key() == "key1")
        .collect();
    assert_eq!(key1_hits.len(), 1, "Within-primitive must never duplicate");
}

#[test]
fn test_across_primitives_no_dedup() {
    let db = test_db();

    // Store "same" data in KV and JSON
    db.kv.put(&run_id, "user_alice", "alice data").unwrap();
    db.json.create(&run_id, json!({"user": "alice", "data": "alice data"})).unwrap();

    let result = db.hybrid().search(&SearchRequest::new(run_id, "alice")).unwrap();

    // POLICY: Both appear - NO cross-primitive dedup
    let kv_hits = result.hits.iter().filter(|h| h.doc_ref.primitive_kind() == PrimitiveKind::Kv).count();
    let json_hits = result.hits.iter().filter(|h| h.doc_ref.primitive_kind() == PrimitiveKind::Json).count();

    assert!(kv_hits >= 1, "KV result must appear");
    assert!(json_hits >= 1, "JSON result must appear");
    // Total >= 2 because no cross-primitive dedup
}

#[test]
fn test_runindex_special_case() {
    // POLICY CLARIFICATION: RunIndex references runs, not documents.
    // A run can have data in multiple primitives.
    // RunIndex search returns the run, not the individual primitive data.

    let db = test_db();
    let run_id = db.run_index.create_run("test run with searchable name").unwrap();

    db.kv.put(&run_id, "key1", "searchable").unwrap();
    db.json.create(&run_id, json!({"data": "searchable"})).unwrap();

    // Search RunIndex returns the RUN, not KV/JSON hits
    let run_result = db.run_index.search(&SearchRequest::new(run_id, "searchable")).unwrap();

    for hit in &run_result.hits {
        assert_eq!(hit.doc_ref.primitive_kind(), PrimitiveKind::Run);
    }
}

/// POLICY: What happens when the same entity logically exists
/// in multiple places?
///
/// ANSWER: Application layer responsibility. M6 does not deduplicate.
#[test]
fn test_logical_duplicates_not_hidden() {
    let db = test_db();

    // User stores same logical entity in two places (their choice)
    db.kv.put(&run_id, "config.timeout", "30").unwrap();
    db.json.create(&run_id, json!({"config": {"timeout": 30}})).unwrap();

    let result = db.hybrid().search(&SearchRequest::new(run_id, "timeout")).unwrap();

    // M6 returns BOTH - it's the application's job to resolve
    assert!(result.hits.len() >= 2, "Logical duplicates must both appear");
}
```

---

## Tier 7: Index Consistency

### 7.1 Index-Scan Equivalence (`index_scan_equivalence.rs`)

```rust
#[test]
fn test_index_matches_scan() {
    let db = test_db();
    populate_test_data(&db);

    let req = SearchRequest::new(run_id, "test");

    // Search without index (scan)
    let scan_result = db.kv.search(&req).unwrap();

    // Enable and build index
    db.enable_search_index(PrimitiveKind::Kv).unwrap();
    db.rebuild_search_index(PrimitiveKind::Kv).unwrap();

    // Search with index
    let index_result = db.kv.search(&req).unwrap();

    // Results should contain same documents
    let scan_refs: HashSet<_> = scan_result.hits.iter().map(|h| &h.doc_ref).collect();
    let index_refs: HashSet<_> = index_result.hits.iter().map(|h| &h.doc_ref).collect();

    assert_eq!(scan_refs, index_refs);
}

#[test]
fn test_index_never_misses_document() {
    // Exhaustive test: every document found by scan is found by index
}

#[test]
fn test_index_never_returns_phantom() {
    // Every document returned by index actually exists
}
```

### 7.2 Index Update Tests (`index_update_tests.rs`)

```rust
#[test]
fn test_index_updated_on_write() {
    let db = test_db();
    db.enable_search_index(PrimitiveKind::Kv).unwrap();

    db.kv.put(&run_id, "key", "searchable").unwrap();

    let result = db.kv.search(&SearchRequest::new(run_id, "searchable")).unwrap();
    assert!(!result.hits.is_empty());
}

#[test]
fn test_index_updated_on_delete() {
    let db = test_db();
    db.enable_search_index(PrimitiveKind::Kv).unwrap();

    db.kv.put(&run_id, "key", "searchable").unwrap();
    db.kv.delete(&run_id, "key").unwrap();

    let result = db.kv.search(&SearchRequest::new(run_id, "searchable")).unwrap();
    assert!(result.hits.is_empty());
}

#[test]
fn test_index_updated_on_update() {
    let db = test_db();
    db.enable_search_index(PrimitiveKind::Kv).unwrap();

    db.kv.put(&run_id, "key", "original").unwrap();
    db.kv.put(&run_id, "key", "updated").unwrap();

    let original_search = db.kv.search(&SearchRequest::new(run_id, "original")).unwrap();
    let updated_search = db.kv.search(&SearchRequest::new(run_id, "updated")).unwrap();

    assert!(original_search.hits.is_empty()); // Old value not found
    assert!(!updated_search.hits.is_empty()); // New value found
}
```

### 7.3 Watermark Tests (`watermark_tests.rs`)

```rust
#[test]
fn test_watermark_tracks_index_freshness() {
    let db = test_db();
    db.enable_search_index(PrimitiveKind::Kv).unwrap();

    let w1 = db.kv.index_watermark();

    db.kv.put(&run_id, "key", "value").unwrap();

    let w2 = db.kv.index_watermark();

    assert!(w2 > w1); // Watermark advanced
}

#[test]
fn test_watermark_detects_stale_index() {
    // When index watermark < storage watermark
    // Index is stale
}
```

### 7.4 Stale Index Fallback (`stale_index_fallback_tests.rs`)

```rust
#[test]
fn test_stale_index_falls_back_to_scan() {
    let db = test_db();
    db.enable_search_index(PrimitiveKind::Kv).unwrap();

    // Write without updating index (simulated stale)
    db.kv.put_bypass_index(&run_id, "key", "needle").unwrap();

    let result = db.kv.search(&SearchRequest::new(run_id, "needle")).unwrap();

    // Should still find via fallback scan
    assert!(!result.hits.is_empty());
}

#[test]
fn test_fallback_indicated_in_stats() {
    // When fallback is used, stats.used_index == false
}
```

---

## Tier 8: Cross-Primitive Search

### 8.1 Hybrid Search Tests (`hybrid_search_tests.rs`)

```rust
#[test]
fn test_hybrid_searches_all_enabled_primitives() {
    // db.hybrid().search() touches KV, JSON, Event, State, Trace, Run
}

#[test]
fn test_hybrid_respects_budget_across_primitives() {
    // Time budget shared across all primitives
    // Not per-primitive
}

#[test]
fn test_hybrid_parallel_execution() {
    // Primitives searched concurrently (or at least efficiently)
    // Total time < sum of individual times
}
```

### 8.2 Multi-Primitive Ranking (`multi_primitive_ranking.rs`)

```rust
#[test]
fn test_ranking_across_primitive_types() {
    // KV hit can outrank JSON hit (or vice versa)
    // Ranking based on score, not primitive type
}

#[test]
fn test_ranking_uses_fusion() {
    // RRF properly combines results from different primitives
}
```

---

## Tier 9: Result Explainability (Future-Proofing)

**NOTE**: This tier documents test scaffolding for future debugging needs.
Not all tests may be implemented in M6, but the scaffolding enables
debugging agent memory issues in future milestones.

### 9.1 Result Provenance (`result_provenance_tests.rs`)

```rust
/// REQUIREMENT: Every SearchHit must know which primitive contributed it.
#[test]
fn test_hit_knows_source_primitive() {
    let db = test_db();

    db.kv.put(&run_id, "key", "searchable").unwrap();
    db.json.create(&run_id, json!({"data": "searchable"})).unwrap();

    let result = db.hybrid().search(&SearchRequest::new(run_id, "searchable")).unwrap();

    for hit in &result.hits {
        // Every hit must have primitive_kind set
        let kind = hit.doc_ref.primitive_kind();
        assert!(matches!(kind,
            PrimitiveKind::Kv | PrimitiveKind::Json |
            PrimitiveKind::Event | PrimitiveKind::State |
            PrimitiveKind::Trace | PrimitiveKind::Run
        ));
    }
}

#[test]
fn test_stats_show_primitive_contributions() {
    // SearchStats should break down:
    // - How many candidates from each primitive
    // - How many hits from each primitive
    // - Time spent in each primitive
}

#[test]
fn test_can_filter_to_explain_primitive_contribution() {
    // Run hybrid search
    // Then run single-primitive search with same query
    // Results from single-primitive should appear in hybrid results
}
```

### 9.2 Score Explanation (`score_explanation_tests.rs`)

```rust
/// FUTURE: Score explanation for debugging ranking issues.
/// These tests document the expected explainability interface.

#[test]
fn test_score_breakdown_available() {
    // Each SearchHit should (optionally) provide:
    // - Term match details (which query terms matched)
    // - IDF contribution per term
    // - TF contribution per term
    // - Length normalization factor
    // - Any boosts applied (title, recency, etc.)
}

#[test]
fn test_can_explain_why_doc_ranked_here() {
    let db = test_db();

    db.kv.put(&run_id, "doc1", "quick brown fox").unwrap();
    db.kv.put(&run_id, "doc2", "quick dog").unwrap();

    let result = db.kv.search(&SearchRequest::new(run_id, "quick fox")).unwrap();

    // Should be able to explain:
    // doc1 ranked higher because it matched "fox" (rare term, high IDF)
    // doc2 only matched "quick" (common term, low IDF)

    // For now, just verify scores are different and doc1 is first
    assert!(result.hits.len() >= 2);
    assert!(result.hits[0].score >= result.hits[1].score);
}

#[test]
fn test_token_matches_visible() {
    // Should be able to see which tokens matched in each document
    // Useful for debugging "why didn't this doc match?"
}
```

### 9.3 Rank Contribution (`rank_contribution_tests.rs`)

```rust
/// FUTURE: For hybrid search, explain how fusion affected ranking.

#[test]
fn test_fusion_contribution_visible() {
    // For each hit in hybrid results:
    // - Which primitive lists contained this doc?
    // - What was its rank in each list?
    // - How did RRF combine these ranks?
}

#[test]
fn test_can_trace_rank_through_fusion() {
    let db = test_db();

    db.kv.put(&run_id, "doc1", "searchable").unwrap();
    db.json.create(&run_id, json!({"data": "searchable"})).unwrap();

    let result = db.hybrid().search(&SearchRequest::new(run_id, "searchable")).unwrap();

    // For debugging, should be able to ask:
    // "This doc is at rank 3. Why?"
    // "It was rank 1 in KV, rank 5 in JSON. RRF combined to rank 3."
}

#[test]
fn test_primitive_rank_vs_final_rank() {
    // Verify that we can compare:
    // - Rank within each primitive
    // - Final rank after fusion
    // This helps debug "my doc was first in KV, why is it third in hybrid?"
}
```

---

## Tier 10: Property-Based / Fuzzing (`search_fuzzing_tests.rs`)

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn fuzz_search_never_panics(
        query in "[a-z]{1,20}",
        data in prop::collection::vec("[a-z ]{1,100}", 1..100),
    ) {
        let db = test_db();
        for (i, value) in data.iter().enumerate() {
            db.kv.put(&run_id, &format!("key_{}", i), value).unwrap();
        }

        let req = SearchRequest::new(run_id, &query);
        let _ = db.kv.search(&req); // Should not panic
    }

    #[test]
    fn fuzz_search_deterministic(
        query in "[a-z]{1,10}",
        data in prop::collection::vec("[a-z ]{1,50}", 1..50),
    ) {
        let db = test_db();
        for (i, value) in data.iter().enumerate() {
            db.kv.put(&run_id, &format!("key_{}", i), value).unwrap();
        }

        let req = SearchRequest::new(run_id, &query);
        let r1 = db.kv.search(&req).unwrap();
        let r2 = db.kv.search(&req).unwrap();

        prop_assert_eq!(r1.hits.len(), r2.hits.len());
    }

    #[test]
    fn fuzz_index_scan_equivalence(
        query in "[a-z]{2,10}",
        data in prop::collection::vec("[a-z ]{1,50}", 1..50),
    ) {
        let db = test_db();
        for (i, value) in data.iter().enumerate() {
            db.kv.put(&run_id, &format!("key_{}", i), value).unwrap();
        }

        let req = SearchRequest::new(run_id, &query);
        let scan_result = db.kv.search(&req).unwrap();

        db.enable_search_index(PrimitiveKind::Kv).unwrap();
        db.rebuild_search_index(PrimitiveKind::Kv).unwrap();

        let index_result = db.kv.search(&req).unwrap();

        let scan_refs: HashSet<_> = scan_result.hits.iter().map(|h| &h.doc_ref).collect();
        let index_refs: HashSet<_> = index_result.hits.iter().map(|h| &h.doc_ref).collect();

        prop_assert_eq!(scan_refs, index_refs);
    }
}
```

---

## Tier 11: Stress & Scale Tests (`search_stress_tests.rs`)

```rust
#[test]
#[ignore] // Slow, opt-in
fn test_search_100k_documents() {
    let db = test_db();
    db.enable_search_index(PrimitiveKind::Kv).unwrap();

    for i in 0..100_000 {
        db.kv.put(&run_id, &format!("key_{}", i), &format!("value with common term {}", i)).unwrap();
    }

    let req = SearchRequest::new(run_id, "common")
        .with_budget(SearchBudget::default().with_max_results(100));

    let start = Instant::now();
    let result = db.kv.search(&req).unwrap();
    let elapsed = start.elapsed();

    assert_eq!(result.hits.len(), 100);
    assert!(elapsed < Duration::from_secs(1)); // Should be fast with index
}

#[test]
#[ignore]
fn test_concurrent_search_and_write() {
    // Multiple threads searching while others are writing
    // No deadlocks, no data corruption
}

#[test]
#[ignore]
fn test_many_concurrent_searches() {
    // 100 concurrent searches
    // All complete correctly
}

#[test]
#[ignore]
fn test_search_deep_json_documents() {
    // JSON with 50 levels of nesting
    // Search still works
}
```

---

## Tier 12: Non-Regression Tests (`m4_m5_regression_tests.rs`)

```rust
#[test]
fn test_m4_kv_get_latency_maintained() {
    let db = test_db();

    for i in 0..1000 {
        db.kv.put(&run_id, &format!("key_{}", i), "value").unwrap();
    }

    let latencies: Vec<_> = (0..1000).map(|i| {
        let start = Instant::now();
        db.kv.get(&run_id, &format!("key_{}", i)).unwrap();
        start.elapsed()
    }).collect();

    let mean_ns = latencies.iter().map(|d| d.as_nanos()).sum::<u128>() / 1000;
    assert!(mean_ns < 5000); // < 5µs
}

#[test]
fn test_m4_kv_put_latency_maintained() {
    let db = test_db();

    let latencies: Vec<_> = (0..1000).map(|i| {
        let start = Instant::now();
        db.kv.put(&run_id, &format!("key_{}", i), "value").unwrap();
        start.elapsed()
    }).collect();

    let mean_ns = latencies.iter().map(|d| d.as_nanos()).sum::<u128>() / 1000;
    assert!(mean_ns < 8000); // < 8µs
}

#[test]
fn test_m5_json_get_latency_maintained() {
    let db = test_db();
    let doc_id = db.json.create(&run_id, json!({"x": 1})).unwrap();

    let latencies: Vec<_> = (0..1000).map(|_| {
        let start = Instant::now();
        db.json.get(&run_id, &doc_id, &JsonPath::root()).unwrap();
        start.elapsed()
    }).collect();

    let mean_ns = latencies.iter().map(|d| d.as_nanos()).sum::<u128>() / 1000;
    assert!(mean_ns < 50_000); // < 50µs
}

#[test]
fn test_m4_red_flags_still_pass() {
    // Re-run M4 red flag tests
    // All must still pass
}
```

---

## Tier 13: Spec Conformance Tests (`spec_conformance_tests.rs`)

```rust
// From M6_ARCHITECTURE.md

// Section 3: Core Search Types

#[test]
fn test_spec_searchrequest_contains_run_id() {
    // SearchRequest must have run_id field
}

#[test]
fn test_spec_searchresponse_contains_hits_and_stats() {
    // SearchResponse must have hits: Vec<SearchHit> and stats: SearchStats
}

#[test]
fn test_spec_searchhit_contains_docref_score_rank() {
    // SearchHit must have doc_ref, score, rank
}

// Section 4: Primitive Search

#[test]
fn test_spec_all_primitives_searchable() {
    // KV, JSON, Event, State, Trace, Run all implement Searchable
}

// Section 5: Scoring

#[test]
fn test_spec_scorer_is_trait() {
    // Scorer must be a trait for pluggability
}

#[test]
fn test_spec_bm25lite_is_default() {
    // BM25LiteScorer is the default Scorer
}

// Section 6: Composite Search

#[test]
fn test_spec_hybrid_orchestrates_primitives() {
    // db.hybrid() delegates to primitive search()
}

// Section 7: Fusion

#[test]
fn test_spec_fuser_is_trait() {
    // Fuser must be a trait for pluggability
}

#[test]
fn test_spec_rrf_is_default() {
    // RRFFuser is the default Fuser
}

// Section 8: Indexing

#[test]
fn test_spec_indexing_is_optional() {
    // Can enable/disable per primitive
}

#[test]
fn test_spec_no_overhead_when_disabled() {
    // Zero allocations when index disabled
}
```

---

## Test Utilities (`main.rs`)

```rust
//! M6 Comprehensive Test Suite
//!
//! Tests for the Retrieval Surfaces semantic guarantees.
//!
//! ## Test Tier Structure
//!
//! - **Tier 1: Architectural Rule Invariants** (sacred, must never break)
//! - **Tier 2: Search Correctness** (determinism, exhaustiveness, filters)
//! - **Tier 3: Budget Semantics** (truncation, ordering, isolation)
//! - **Tier 4: Scoring Accuracy** (BM25-lite correctness)
//! - **Tier 5: Fusion Correctness** (RRF, determinism, tiebreak)
//! - **Tier 6: Cross-Primitive Identity** (DocRef policy, deduplication policy)
//! - **Tier 7: Index Consistency** (index matches scan)
//! - **Tier 8: Cross-Primitive Search** (hybrid orchestration)
//! - **Tier 9: Result Explainability** (provenance, score explanation)
//! - **Tier 10: Property-Based/Fuzzing** (catch edge cases)
//! - **Tier 11: Stress/Scale** (correctness under load)
//! - **Tier 12: Non-Regression** (M4/M5 targets maintained)
//! - **Tier 13: Spec Conformance** (spec → test)
//!
//! ## Running Tests
//!
//! ```bash
//! # Run all M6 comprehensive tests
//! cargo test --test m6_comprehensive
//!
//! # Run only architectural invariants (fastest)
//! cargo test --test m6_comprehensive invariant
//!
//! # Run property-based tests
//! cargo test --test m6_comprehensive fuzz
//!
//! # Run stress tests (slow, opt-in)
//! cargo test --test m6_comprehensive stress -- --ignored
//! ```

// Utilities
mod test_utils;

// Tier 1: Architectural Rule Invariants
mod docref_invariants;
mod primitive_search_invariants;
mod composite_orchestration_tests;
mod snapshot_search_invariants;
mod zero_overhead_tests;
mod algorithm_swappable_tests;

// Tier 2: Search Correctness
mod search_determinism_tests;
mod search_exhaustiveness_tests;
mod search_filter_tests;

// Tier 3: Budget Semantics (NOT Performance)
mod budget_truncation_tests;
mod budget_ordering_tests;
mod budget_isolation_tests;

// Tier 4: Scoring Accuracy
mod bm25_scoring_tests;
mod tokenizer_tests;
mod idf_calculation_tests;

// Tier 5: Fusion Correctness
mod rrf_fusion_tests;
mod fusion_determinism_tests;
mod tiebreak_tests;

// Tier 6: Cross-Primitive Identity
mod docref_identity_policy_tests;
mod deduplication_policy_tests;

// Tier 7: Index Consistency
mod index_scan_equivalence;
mod index_update_tests;
mod watermark_tests;
mod stale_index_fallback_tests;

// Tier 8: Cross-Primitive Search
mod hybrid_search_tests;
mod multi_primitive_ranking;

// Tier 9: Result Explainability (Future-Proofing)
mod result_provenance_tests;
mod score_explanation_tests;
mod rank_contribution_tests;

// Tier 10: Property-Based/Fuzzing
mod search_fuzzing_tests;

// Tier 11: Stress & Scale (use #[ignore])
mod search_stress_tests;

// Tier 12: Non-Regression
mod m4_m5_regression_tests;

// Tier 13: Spec Conformance
mod spec_conformance_tests;
```

---

## Test Utilities (`test_utils.rs`)

```rust
use in_mem_core::types::RunId;
use in_mem_engine::Database;
use in_mem_primitives::*;
use in_mem_search::*;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Create a test database with InMemory durability
pub fn create_test_db() -> Arc<Database> {
    Arc::new(
        Database::builder()
            .durability(DurabilityMode::InMemory)
            .open_temp()
            .expect("Failed to create test database")
    )
}

/// Create test run ID
pub fn test_run_id() -> RunId {
    RunId::new()
}

/// Populate test data across all primitives
pub fn populate_test_data(db: &Database) {
    let run_id = test_run_id();

    // KV data
    for i in 0..100 {
        db.kv.put(&run_id, &format!("key_{}", i), &format!("value test {}", i)).unwrap();
    }

    // JSON data
    for i in 0..50 {
        db.json.create(&run_id, serde_json::json!({
            "name": format!("item_{}", i),
            "description": format!("test item number {}", i)
        })).unwrap();
    }

    // Event data
    for i in 0..50 {
        db.event.append(&run_id, &format!("test.event.{}", i % 5),
            serde_json::json!({"data": format!("event data {}", i)})).unwrap();
    }
}

/// Populate large dataset for stress testing
pub fn populate_large_dataset(db: &Database, count: usize) {
    let run_id = test_run_id();
    for i in 0..count {
        db.kv.put(&run_id, &format!("key_{}", i), &format!("value with common searchable term {}", i)).unwrap();
    }
}

/// Assert search returns expected number of hits
pub fn assert_hit_count(response: &SearchResponse, expected: usize) {
    assert_eq!(response.hits.len(), expected,
        "Expected {} hits, got {}", expected, response.hits.len());
}

/// Assert all hits are from specified primitive
pub fn assert_all_from_primitive(response: &SearchResponse, kind: PrimitiveKind) {
    for hit in &response.hits {
        assert_eq!(hit.doc_ref.primitive_kind(), kind,
            "Expected all hits from {:?}, found {:?}", kind, hit.doc_ref.primitive_kind());
    }
}

/// Measure operation latency
pub fn measure_latency<F, T>(op: F) -> (T, Duration)
where
    F: FnOnce() -> T,
{
    let start = Instant::now();
    let result = op();
    (result, start.elapsed())
}

/// Assert latency is within target
pub fn assert_latency_under(actual: Duration, target_micros: u64) {
    assert!(actual.as_micros() < target_micros as u128,
        "Latency {} µs exceeds target {} µs", actual.as_micros(), target_micros);
}
```

---

## Implementation Priority

| Priority | Tier | Estimated Tests | Rationale |
|----------|------|-----------------|-----------|
| **P0** | Tier 1: Architectural Rules | ~25 | Lock in the contract |
| **P0** | Tier 2: Search Correctness | ~12 | Core determinism & exhaustiveness |
| **P0** | Tier 3: Budget Semantics | ~10 | Budget never corrupts |
| **P0** | Tier 6: Cross-Primitive Identity | ~8 | Identity policy must be explicit |
| **P0** | Tier 7: Index Consistency | ~10 | Index must match scan |
| **P1** | Tier 4: Scoring Accuracy | ~15 | Ranking quality |
| **P1** | Tier 5: Fusion Correctness | ~10 | Multi-source merging |
| **P1** | Tier 10: Fuzzing | ~5 | Catches edge cases |
| **P2** | Tier 8: Cross-Primitive Search | ~10 | Hybrid orchestration |
| **P2** | Tier 9: Result Explainability | ~10 | Future debugging support |
| **P2** | Tier 12: Non-Regression | ~10 | M4/M5 maintained |
| **P2** | Tier 13: Spec Conformance | ~15 | Spec coverage |
| **P3** | Tier 11: Stress & Scale | ~10 | Scale verification |

**Total: ~150 new tests**

---

## Dependencies

```toml
[dev-dependencies]
proptest = "1.4"          # Property-based testing
criterion = "0.5"         # Benchmarking
tempfile = "3.10"         # Temporary directories
```

---

## Success Criteria

1. **All Tier 1 tests pass** - Six architectural rules locked
2. **Search is deterministic** - Same query always returns same results
3. **Budget never corrupts** - Truncation preserves ordering and snapshot isolation
4. **Identity policy explicit** - DocRefs follow documented equality semantics
5. **Index matches scan** - No phantom or missing results
6. **Fuzzing finds no violations** - 10,000+ random cases pass
7. **M4/M5 latency targets maintained** - No regressions
8. **Spec coverage > 95%** - Every spec statement has a test

---

## Notes

- These tests are **separate from unit tests** - they test public API behavior
- Tests should read like **English specifications**, not implementation details
- **Six architectural rules are sacred** - Tier 1 tests must never fail
- **Budget is semantic, not performance** - Tier 3 tests correctness, not speed
- **Identity policy is a design decision** - Tier 6 documents and locks policy
- **Index-scan equivalence is mandatory** - Index cannot return different results
- **Explainability is future-proofing** - Tier 9 scaffolding for debugging
- **Fuzzing is mandatory** - Property-based tests catch what humans miss
- Run stress tests **before every release** - Find rare bugs early

---

*End of M6 Comprehensive Test Plan*
