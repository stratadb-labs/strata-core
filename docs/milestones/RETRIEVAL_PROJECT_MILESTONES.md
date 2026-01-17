# Retrieval Project Milestones

This document tracks the retrieval-specific roadmap that builds on top of the core M6 Retrieval Surfaces milestone.

---

## Milestone 1: Retrieval Surfaces ✅

**Goal**: Add primitive-native search APIs and a composite planner surface

**Deliverable**: `kv.search()`, `json.search()`, etc. plus `db.hybrid.search()` that fuses results

**Status**: Complete (corresponds to main project M6)

**Success Criteria**:
- [x] SearchRequest/SearchResponse/DocRef types finalized
- [x] Each primitive implements `search()` (scan fallback acceptable)
- [x] Composite `hybrid.search()` orchestrates and fuses
- [x] Snapshot-consistent search execution
- [x] Budget enforcement works
- [x] Deterministic ordering and stable pointers

**Risk**: Over-abstracting and accidentally forcing data movement or expensive normalization. ✅ Mitigated

---

## Milestone 2: Keyword Indexing Foundation ✅

**Goal**: Make keyword search fast enough to feel native

**Deliverable**: Lightweight inverted index option for primitives with commit-consistent watermarking

**Status**: Complete (implemented as part of M6)

**Success Criteria**:
- [x] Tokenizer + postings format implemented
- [x] Incremental updates on commit for opt-in primitives
- [x] Query-time top-k with bounded candidate work
- [x] Index can be disabled with scan fallback still correct
- [x] Benchmarks show big win vs scan for medium corpora

**Risk**: Write amplification and index consistency bugs under crash/recovery. ✅ Mitigated

---

## Milestone 3: Hello World Hybrid Retrieval ✅

**Goal**: Validate fusion and multi-primitive retrieval end-to-end

**Deliverable**: KeywordTopK per primitive + RRF fusion in composite search

**Status**: Complete (implemented as part of M6)

**Success Criteria**:
- [x] RRF implementation with deterministic tie-breaking
- [x] Primitive weighting knobs (light-touch)
- [x] Simple demo workload shows cross-primitive retrieval works
- [x] Debug traces explain which primitive contributed each hit

**Risk**: If this is slow or opaque, iteration speed on future algorithms collapses. ✅ Mitigated

---

## Milestone 4: Evaluation Harness and Ground Truth

**Goal**: Turn retrieval into a measurable research loop

**Deliverable**: Bench harness + datasets + offline metrics + regression gates

**Status**: Planned

**Success Criteria**:
- [ ] Synthetic "agent memory" datasets for all primitives
- [ ] Query sets with expected hits
- [ ] Metrics: Recall@K, MRR, nDCG, latency p50/p95
- [ ] Automated regression tests for relevance and latency

**Risk**: Without a harness, "blazing fast" and "good retrieval" become vibes.

---

## Milestone 5: Vector Retrieval

**Goal**: Add semantic retrieval as an additional retriever, not a replacement

**Deliverable**: Per-primitive vector search where relevant, plus composite fusion with keyword

**Status**: Planned (corresponds to main project M9)

**Success Criteria**:
- [ ] Vector index primitive or module integrated cleanly
- [ ] Composite can fuse keyword + vector
- [ ] Budgets and determinism maintained
- [ ] Evaluation shows clear wins on semantic queries

**Risk**: Vectors can dominate engineering effort and derail the planner-centric design.

---

## Milestone 6: Reranking and Multi-Step Retrieval

**Goal**: Human-like retrieval: generate candidates fast, rerank intelligently, iterate

**Deliverable**: Optional reranker hooks (model-based or heuristic) and multi-hop retrieval loops

**Status**: Planned

**Success Criteria**:
- [ ] Rerank interface that accepts top-N candidates
- [ ] Multi-step query expansion experiments
- [ ] Reranker runs under strict time budget and is optional
- [ ] Evaluation proves improved relevance

**Risk**: Latency blowups and non-determinism.

---

## Milestone 7: Production Hardening

**Goal**: Make retrieval reliable at scale

**Deliverable**: Robust crash handling, index rebuild, observability, and tuning knobs

**Status**: Planned

**Success Criteria**:
- [ ] Index rebuild and verification tools
- [ ] Observability: per-stage timings, candidate counts, truncation reasons
- [ ] Backpressure and memory caps
- [ ] Stable behavior across durability modes

**Risk**: Index corruption and silent relevance regressions.

---

## Timeline

```
Completed:
- R1 (Retrieval Surfaces)       ✅ (= M6)
- R2 (Keyword Indexing)         ✅ (= M6)
- R3 (Hello World Hybrid)       ✅ (= M6)

Next:
- R4 (Evaluation Harness)       ← Next retrieval work

Future:
- R5 (Vector Retrieval)         (= M9)
- R6 (Reranking)
- R7 (Production Hardening)
```

---

## Mapping to Main Project Milestones

| Retrieval Milestone | Main Project Milestone |
|---------------------|------------------------|
| R1: Retrieval Surfaces | M6: Retrieval Surfaces ✅ |
| R2: Keyword Indexing | M6: Retrieval Surfaces ✅ |
| R3: Hello World Hybrid | M6: Retrieval Surfaces ✅ |
| R4: Evaluation Harness | Post-MVP |
| R5: Vector Retrieval | M9: Vector Store |
| R6: Reranking | Post-MVP |
| R7: Production Hardening | Post-MVP |

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | Initial | Original retrieval roadmap |
| 2.0 | 2026-01-17 | R1-R3 complete (M6 delivered); formatted for consistency |
