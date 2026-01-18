# Epic End Validation Plan

**Run this validation at the end of every epic before merging to develop.**

---

## Overview

Epic-end validation ensures:
1. All stories in the epic are complete and correct
2. Code quality meets standards
3. Implementation matches architecture spec (M3, M4, M5, or M6)
4. Tests are comprehensive and passing
5. No regressions introduced

**Note**: This document covers validation for M3 (Epics 13-19), M4 (Epics 20-25), M5 (Epics 26-32), and M6 (Epics 33-39).
For M4-specific validation, see [Phase 6b: M4 Performance Validation](#phase-6b-m4-performance-validation-m4-only).
For M5-specific validation, see [Phase 6c: M5 JSON Primitive Validation](#phase-6c-m5-json-primitive-validation-m5-only).
For M6-specific validation, see [Phase 6d: M6 Retrieval Surfaces Validation](#phase-6d-m6-retrieval-surfaces-validation-m6-only).

---

## Phase 1: Automated Checks (5 minutes)

### 1.1 Build & Test Suite

```bash
# Full workspace build
~/.cargo/bin/cargo build --workspace

# Full test suite
~/.cargo/bin/cargo test --workspace

# Release mode tests (catches optimization-related issues)
~/.cargo/bin/cargo test --workspace --release
```

### 1.2 Code Quality

```bash
# Clippy with strict warnings
~/.cargo/bin/cargo clippy --workspace -- -D warnings

# Format check
~/.cargo/bin/cargo fmt --check

# Documentation builds without warnings
~/.cargo/bin/cargo doc --workspace --no-deps
```

### 1.3 Automated Check Summary

| Check | Command | Pass Criteria |
|-------|---------|---------------|
| Build | `cargo build --workspace` | Zero errors |
| Tests | `cargo test --workspace` | All pass |
| Release Tests | `cargo test --workspace --release` | All pass |
| Clippy | `cargo clippy --workspace -- -D warnings` | Zero warnings |
| Format | `cargo fmt --check` | No changes needed |
| Docs | `cargo doc --workspace --no-deps` | Builds without warnings |

---

## Phase 2: Story Completion Verification (10 minutes)

### 2.1 Story Checklist

For EACH story in the epic, verify:

| Story | Files Created/Modified | Tests Added | Acceptance Criteria Met |
|-------|------------------------|-------------|-------------------------|
| #XXX | [ ] | [ ] | [ ] |
| #XXX | [ ] | [ ] | [ ] |
| ... | | | |

### 2.2 Verify Story Deliverables

```bash
# Check that expected files exist
ls -la crates/primitives/src/<expected_files>

# Check test count for the epic's module
~/.cargo/bin/cargo test --package primitives <module_name> -- --list 2>/dev/null | grep -c "test"
```

### 2.3 PR Status Check

```bash
# Verify all story PRs are merged to epic branch
/opt/homebrew/bin/gh pr list --state merged --base epic-<N>-<name> --json number,title

# Should match the number of stories in the epic
```

---

## Phase 3: Spec Compliance Review (15 minutes)

### 3.1 Architecture Spec Compliance

**For M3 (Epics 13-19)**: Open `docs/architecture/M3_ARCHITECTURE.md`
**For M4 (Epics 20-25)**: Open `docs/architecture/M4_ARCHITECTURE.md`
**For M5 (Epics 26-32)**: Open `docs/architecture/M5_ARCHITECTURE.md` (THE AUTHORITATIVE SPEC)

Verify implementation matches:

#### M3 Spec Compliance (Epics 13-19)

| Section | Requirement | Implemented Correctly |
|---------|-------------|----------------------|
| Section 3 | Stateless facades | [ ] Primitives hold only `Arc<Database>` |
| Section 4-8 | Primitive APIs | [ ] All methods match spec signatures |
| Section 9 | Key design | [ ] TypeTags and key formats correct |
| Section 10 | Transaction integration | [ ] Extension traits work |
| Section 12 | Invariant enforcement | [ ] Primitives enforce their invariants |

#### M4 Spec Compliance (Epics 20-25)

| Section | Requirement | Implemented Correctly |
|---------|-------------|----------------------|
| Section 2 | Key Design Decisions | [ ] DurabilityMode enum, DashMap + HashMap, pooling |
| Section 3 | Durability Modes | [ ] InMemory, Buffered, Strict implementations |
| Section 4 | Sharded Storage | [ ] ShardedStore, per-RunId sharding |
| Section 5 | Transaction Pooling | [ ] Thread-local pools, reset() method |
| Section 6 | Read Path Optimization | [ ] Fast path reads, snapshot-based |
| Section 7 | Red Flag Thresholds | [ ] All hard stops respected |

#### M5 Spec Compliance (Epics 26-32)

| Rule | Requirement | Implemented Correctly |
|------|-------------|----------------------|
| Rule 1 | JSON in ShardedStore | [ ] No separate DashMap, uses `Key::new_json()` |
| Rule 2 | Stateless Facade | [ ] JsonStore holds only `Arc<Database>` |
| Rule 3 | Extension Trait | [ ] JsonStoreExt on TransactionContext, no separate type |
| Rule 4 | Path Semantics | [ ] Storage sees whole docs, path logic in API layer |
| Rule 5 | Unified WAL | [ ] Entry types 0x20-0x23 in existing WALEntry enum |
| Rule 6 | Consistent API | [ ] Same patterns as KVStore, EventLog, etc. |

### 3.2 Spec Deviation Check

Search for potential deviations:

```bash
# Look for TODOs or FIXMEs that might indicate spec deviations
grep -r "TODO\|FIXME\|HACK\|XXX" crates/primitives/src/

# Look for unwrap/expect that might indicate incomplete error handling
grep -r "\.unwrap()\|\.expect(" crates/primitives/src/ | wc -l
```

**Rule**: If ANY deviation from spec is found:
1. Document why
2. Get explicit approval
3. Create follow-up issue if needed

---

## Phase 4: Code Review Checklist (20 minutes)

### 4.1 Structural Review

| Item | Check | Status |
|------|-------|--------|
| **Module organization** | Files in correct locations | [ ] |
| **Public API** | Only intended items are `pub` | [ ] |
| **Dependencies** | No unnecessary dependencies added | [ ] |
| **Re-exports** | lib.rs exports what users need | [ ] |

### 4.2 Code Quality Review

| Item | Check | Status |
|------|-------|--------|
| **Error handling** | Uses `Result<T, Error>`, no panics in library code | [ ] |
| **Naming** | Follows Rust conventions (snake_case, CamelCase) | [ ] |
| **Documentation** | Public items have doc comments | [ ] |
| **No dead code** | No unused functions, structs, or imports | [ ] |
| **No debug code** | No `println!`, `dbg!`, or debug logging left in | [ ] |

### 4.3 Safety Review

| Item | Check | Status |
|------|-------|--------|
| **No unsafe** | No `unsafe` blocks (unless justified and documented) | [ ] |
| **No unwrap on user input** | All user-provided data validated | [ ] |
| **No panic paths** | Library code returns errors, doesn't panic | [ ] |
| **Thread safety** | Primitives are `Send + Sync` | [ ] |

### 4.4 Test Quality Review

| Item | Check | Status |
|------|-------|--------|
| **Happy path tested** | Normal operations work | [ ] |
| **Error cases tested** | Invalid inputs return appropriate errors | [ ] |
| **Edge cases tested** | Empty inputs, max values, boundaries | [ ] |
| **Concurrent access** | Thread safety verified where applicable | [ ] |
| **Integration tests** | Cross-component interactions tested | [ ] |

---

## Phase 5: Best Practices Verification (10 minutes)

### 5.1 Rust Best Practices

| Practice | Verified |
|----------|----------|
| Use `&str` for input, `String` for owned data | [ ] |
| Prefer iterators over manual loops | [ ] |
| Use `?` operator for error propagation | [ ] |
| Derive traits where appropriate (`Debug`, `Clone`, etc.) | [ ] |
| Use `#[must_use]` for functions with important return values | [ ] |

### 5.2 Project-Specific Best Practices

| Practice | Verified |
|----------|----------|
| Primitives are stateless (only hold `Arc<Database>`) | [ ] |
| All operations scoped to `RunId` | [ ] |
| Keys use correct `TypeTag` | [ ] |
| Extension traits delegate to primitive internals | [ ] |
| Tests follow TDD - written before implementation | [ ] |

### 5.3 Performance Considerations

| Item | Verified |
|------|----------|
| No unnecessary allocations in hot paths | [ ] |
| No holding locks across await points | [ ] |
| Efficient key construction | [ ] |
| Batch operations where appropriate | [ ] |

---

## Phase 6: Epic-Specific Validation

### For Each M3 Epic:

#### Epic 13: Foundation
```bash
# Verify Key helpers work correctly
~/.cargo/bin/cargo test --package primitives key_

# Verify TypeTags are correct
grep -r "TypeTag::" crates/primitives/src/
```

#### Epic 14: KVStore
```bash
# Verify CRUD operations
~/.cargo/bin/cargo test --package primitives kv_

# Verify list with prefix
~/.cargo/bin/cargo test --package primitives test_kv_list
```

#### Epic 15: EventLog
```bash
# Verify chain integrity
~/.cargo/bin/cargo test --package primitives test_event_chain

# Verify append-only (no update/delete)
grep -r "fn update\|fn delete" crates/primitives/src/event_log.rs
# Should show methods that return InvalidOperation error
```

#### Epic 16: StateCell
```bash
# Verify CAS semantics
~/.cargo/bin/cargo test --package primitives test_state_cas

# Verify transition purity documented
grep -r "purity\|pure\|Purity" crates/primitives/src/state_cell.rs
```

#### Epic 17: TraceStore
```bash
# Verify indices work
~/.cargo/bin/cargo test --package primitives test_trace_query

# Verify parent-child relationships
~/.cargo/bin/cargo test --package primitives test_trace_tree
```

#### Epic 18: RunIndex
```bash
# Verify status transitions
~/.cargo/bin/cargo test --package primitives test_status_transition

# Verify cascading delete
~/.cargo/bin/cargo test --package primitives test_delete_run
```

#### Epic 19: Integration
```bash
# Run all integration tests
~/.cargo/bin/cargo test --package primitives --test cross_primitive_tests
~/.cargo/bin/cargo test --package primitives --test run_isolation_tests
~/.cargo/bin/cargo test --package primitives --test recovery_tests

# Run benchmarks
~/.cargo/bin/cargo bench --package primitives
```

---

## Phase 6b: M4 Performance Validation (M4 Only)

**This phase is REQUIRED for all M4 epics (20-25).**

### 6b.1 Performance Benchmarks

```bash
# Run M4 performance benchmarks
~/.cargo/bin/cargo bench --bench m4_performance

# Run facade tax benchmarks
~/.cargo/bin/cargo bench --bench m4_facade_tax

# Expected results (InMemory mode):
# - engine/put_direct: < 3µs (red flag: > 10µs)
# - kvstore/put: < 8µs (red flag: > 20µs)
# - kvstore/get: < 5µs (red flag: > 10µs)
```

### 6b.2 Red Flag Tests

```bash
# Run red flag verification tests
~/.cargo/bin/cargo test --test m4_red_flags

# All red flag thresholds must pass:
# - Snapshot acquisition: < 2µs
# - A1/A0 ratio: < 20×
# - B/A1 ratio: < 8×
# - Disjoint scaling (4T): > 2.5×
# - p99 latency: < 20× mean
# - Hot-path allocations: 0
```

**CRITICAL**: If ANY red flag test fails, STOP and REDESIGN before proceeding.

### 6b.3 Facade Tax Verification

| Layer | Description | Target | Red Flag |
|-------|-------------|--------|----------|
| A0 | Raw HashMap | baseline | - |
| A1 | Engine/Storage | < 10× A0 | > 20× A0 |
| B | Primitive facade | < 5× A1 | > 8× A1 |

```bash
# Verify facade tax is within limits
~/.cargo/bin/cargo bench --bench m4_facade_tax -- --save-baseline m4-facade

# Expected output should show:
# A1/A0: < 10× (target), definitely < 20× (red flag)
# B/A1: < 5× (target), definitely < 8× (red flag)
```

### 6b.4 Scaling Verification

```bash
# Run scaling tests
~/.cargo/bin/cargo test --test m4_scaling

# Verify disjoint workload scaling:
# - 1 thread: baseline
# - 2 threads: > 1.8× improvement
# - 4 threads: > 2.5× improvement (red flag: < 2.5×)
```

### 6b.5 Durability Mode Verification (Epic 21+)

```bash
# Test each durability mode
~/.cargo/bin/cargo test --test m4_durability_modes

# Verify latency targets:
# - InMemory: < 3µs (no persistence)
# - Buffered: < 30µs (async fsync)
# - Strict: ~2ms (sync fsync, matches M3)
```

### 6b.6 M4 Epic-Specific Validation

#### Epic 20: Performance Foundation
```bash
# Verify baseline tag exists
git tag -l | grep "m3_baseline_perf"

# Verify DurabilityMode enum (in durability crate, not core)
grep -r "enum DurabilityMode" crates/durability/src/

# Verify instrumentation (in engine crate, not core)
~/.cargo/bin/cargo test --package in-mem-engine --features perf-trace perf
```

#### Epic 21: Durability Modes
```bash
# Verify all three modes work (durability crate)
~/.cargo/bin/cargo test --package in-mem-durability durability

# Verify graceful shutdown (engine crate)
~/.cargo/bin/cargo test --package in-mem-engine test_shutdown

# Verify per-operation override (engine crate)
~/.cargo/bin/cargo test --package in-mem-engine test_override
```

#### Epic 22: Sharded Storage
```bash
# Verify ShardedStore implementation (storage crate)
~/.cargo/bin/cargo test --package in-mem-storage sharded

# Verify snapshot acquisition time < 2µs
~/.cargo/bin/cargo bench --bench m4_performance -- snapshot

# Verify migration from old storage (storage crate)
~/.cargo/bin/cargo test --package in-mem-storage test_migration
```

#### Epic 23: Transaction Pooling
```bash
# Verify pool implementation (concurrency crate)
~/.cargo/bin/cargo test --package in-mem-concurrency transaction_pool

# Verify reset() preserves capacity (concurrency crate)
~/.cargo/bin/cargo test --package in-mem-concurrency test_reset_capacity

# Verify no pool exhaustion under load (concurrency crate)
~/.cargo/bin/cargo test --package in-mem-concurrency test_pool_stress
```

#### Epic 24: Read Path Optimization
```bash
# Verify fast path reads
~/.cargo/bin/cargo test --package primitives fast_path

# Verify observational equivalence
~/.cargo/bin/cargo test --test m4_fast_path_equivalence

# Verify batch operations use single snapshot
~/.cargo/bin/cargo test --package primitives test_get_many
```

#### Epic 25: Validation & Red Flags
```bash
# Run complete validation suite
~/.cargo/bin/cargo bench --bench m4_performance
~/.cargo/bin/cargo test --test m4_red_flags
~/.cargo/bin/cargo test --test m4_contention

# Generate performance report
~/.cargo/bin/cargo bench --bench m4_performance -- --save-baseline m4-final
```

### 6b.7 M4 Performance Checklist

| Metric | Target | Red Flag | Actual | Status |
|--------|--------|----------|--------|--------|
| `engine/put_direct` (InMemory) | < 3µs | > 10µs | ___ | [ ] |
| `kvstore/put` (InMemory) | < 8µs | > 20µs | ___ | [ ] |
| `kvstore/get` | < 5µs | > 10µs | ___ | [ ] |
| Throughput (1T InMemory) | 250K ops/sec | < 100K | ___ | [ ] |
| Throughput (4T disjoint) | 800K ops/sec | < 400K | ___ | [ ] |
| Snapshot acquisition | < 500ns | > 2µs | ___ | [ ] |
| A1/A0 ratio | < 10× | > 20× | ___ | [ ] |
| B/A1 ratio | < 5× | > 8× | ___ | [ ] |
| Disjoint scaling (4T) | > 3× | < 2.5× | ___ | [ ] |
| p99/mean latency | < 10× | > 20× | ___ | [ ] |

---

## Phase 6c: M5 JSON Primitive Validation (M5 Only)

**This phase is REQUIRED for all M5 epics (26-32).**

### 6c.1 The Six Architectural Rules Verification

```bash
# Rule 1: JSON in ShardedStore (no separate DashMap)
grep -r "DashMap.*Json" crates/primitives/src/
# Should return NO results - JSON uses existing storage

# Rule 2: Stateless facade
grep -A5 "pub struct JsonStore" crates/primitives/src/json_store.rs
# Should show ONLY Arc<Database>

# Rule 3: Extension trait (not separate type)
grep -r "impl JsonStoreExt for TransactionContext" crates/
# Should find the impl

# Rule 4: Storage sees whole documents
grep -r "storage.put_at_path\|storage.get_at_path" crates/
# Should return NO results - path logic in API layer only

# Rule 5: Unified WAL
grep -r "JsonCreate\|JsonSet\|JsonDelete\|JsonDestroy" crates/durability/src/wal.rs
# Should find entry types 0x20-0x23 in WALEntry enum

# Rule 6: Consistent API
grep -r "fn create\|fn get\|fn set\|fn destroy" crates/primitives/src/json_store.rs
# Should match other primitive patterns
```

### 6c.2 Core Types Verification (Epic 26)

```bash
# Verify JsonDocId, JsonValue, JsonPath, JsonPatch exist
~/.cargo/bin/cargo test --package in-mem-core json_

# Verify TypeTag::Json = 0x11
grep -r "Json = 0x11" crates/core/src/

# Verify JsonPath parsing
~/.cargo/bin/cargo test --package in-mem-core test_json_path
```

### 6c.3 Path Operations Verification (Epic 27)

```bash
# Verify path operations
~/.cargo/bin/cargo test --package in-mem-core test_get_at_path
~/.cargo/bin/cargo test --package in-mem-core test_set_at_path
~/.cargo/bin/cargo test --package in-mem-core test_delete_at_path
~/.cargo/bin/cargo test --package in-mem-core test_apply_patches
```

### 6c.4 JsonStore Verification (Epic 28)

```bash
# Verify CRUD operations
~/.cargo/bin/cargo test --package primitives json_store

# Verify fast path reads (no transaction overhead)
~/.cargo/bin/cargo test --package primitives test_json_fast_path

# Verify stateless facade pattern
~/.cargo/bin/cargo test --package primitives test_json_store_stateless
```

### 6c.5 WAL Integration Verification (Epic 29)

```bash
# Verify WAL entry types
~/.cargo/bin/cargo test --package in-mem-durability json_wal

# Verify serialization/deserialization
~/.cargo/bin/cargo test --package in-mem-durability test_json_entry_roundtrip

# Verify recovery
~/.cargo/bin/cargo test --package in-mem-durability test_json_recovery
```

### 6c.6 Transaction Integration Verification (Epic 30)

```bash
# Verify JsonStoreExt trait
~/.cargo/bin/cargo test --package in-mem-concurrency json_transaction

# Verify cross-primitive atomicity
~/.cargo/bin/cargo test --package primitives test_json_cross_primitive

# Verify lazy allocation (zero overhead when not using JSON)
~/.cargo/bin/cargo test --package in-mem-concurrency test_lazy_json_state
```

### 6c.7 Conflict Detection Verification (Epic 31)

```bash
# Verify path overlap detection
~/.cargo/bin/cargo test --package in-mem-core test_path_overlap

# Verify read-write conflict detection
~/.cargo/bin/cargo test --package in-mem-concurrency test_read_write_conflict

# Verify write-write conflict detection
~/.cargo/bin/cargo test --package in-mem-concurrency test_write_write_conflict

# Verify conflict aborts entire transaction
~/.cargo/bin/cargo test --package in-mem-concurrency test_conflict_abort
```

### 6c.8 Performance Benchmarks (Epic 32)

```bash
# Run M5 JSON benchmarks
~/.cargo/bin/cargo bench --bench m5_json_performance

# Expected results:
# - JSON create (1KB): < 1ms
# - JSON get at path (1KB): < 100µs
# - JSON set at path (1KB): < 1ms
# - JSON delete at path (1KB): < 500µs
```

### 6c.9 Non-Regression Verification

```bash
# Verify M4 performance targets are maintained
~/.cargo/bin/cargo bench --bench m4_performance

# Run M4 red flag tests (must still pass)
~/.cargo/bin/cargo test --test m4_red_flags

# Verify existing primitives unaffected
~/.cargo/bin/cargo test --package primitives kv_store
~/.cargo/bin/cargo test --package primitives event_log
~/.cargo/bin/cargo test --package primitives state_cell
~/.cargo/bin/cargo test --package primitives trace_store
```

### 6c.10 M5 Epic-Specific Validation

#### Epic 26: Core Types
```bash
~/.cargo/bin/cargo test --package in-mem-core json_
grep -r "TypeTag::Json" crates/core/src/
```

#### Epic 27: Path Operations
```bash
~/.cargo/bin/cargo test --package in-mem-core path_
~/.cargo/bin/cargo test --package in-mem-core patch_
```

#### Epic 28: JsonStore Core
```bash
~/.cargo/bin/cargo test --package primitives json_store
# Verify no internal state
grep -A10 "pub struct JsonStore" crates/primitives/src/json_store.rs
```

#### Epic 29: WAL Integration
```bash
~/.cargo/bin/cargo test --package in-mem-durability json_
grep -r "0x2" crates/durability/src/wal.rs
```

#### Epic 30: Transaction Integration
```bash
~/.cargo/bin/cargo test --package in-mem-concurrency json_
~/.cargo/bin/cargo test --package primitives test_json_transaction
```

#### Epic 31: Conflict Detection
```bash
~/.cargo/bin/cargo test --package in-mem-concurrency json_conflict
~/.cargo/bin/cargo test --package in-mem-concurrency test_overlap
```

#### Epic 32: Validation
```bash
# Full validation suite
~/.cargo/bin/cargo bench --bench m5_json_performance
~/.cargo/bin/cargo test --test m5_json_integration
~/.cargo/bin/cargo test --test m4_red_flags
```

### 6c.11 M5 Performance Checklist

| Metric | Target | Red Flag | Actual | Status |
|--------|--------|----------|--------|--------|
| JSON create (1KB) | < 1ms | > 5ms | ___ | [ ] |
| JSON get at path (1KB) | < 100µs | > 500µs | ___ | [ ] |
| JSON set at path (1KB) | < 1ms | > 5ms | ___ | [ ] |
| JSON delete at path (1KB) | < 500µs | > 2ms | ___ | [ ] |
| KV put (no regression) | < 3µs | > 10µs | ___ | [ ] |
| KV get (no regression) | < 5µs | > 10µs | ___ | [ ] |
| Event append (no regression) | < 10µs | > 30µs | ___ | [ ] |
| State read (no regression) | < 5µs | > 10µs | ___ | [ ] |

---

## Phase 6d: M6 Retrieval Surfaces Validation (M6 Only)

**This phase is REQUIRED for all M6 epics (33-39).**

### 6d.1 The Six Architectural Rules Verification

```bash
# Rule 1: No Data Movement (DocRef references only)
grep -r "fn search" crates/search/src/
# Should return SearchResponse with DocRef, not cloned data

# Rule 2: Primitive Search is First-Class
grep -r "impl Searchable for" crates/primitives/src/
# Should find implementations for all 6 primitives

# Rule 3: Composite Orchestrates, Doesn't Replace
grep -A10 "pub fn hybrid" crates/primitives/src/
# Should delegate to primitive search() methods

# Rule 4: Snapshot-Consistent Search
grep -r "search_with_snapshot\|SearchSnapshot" crates/search/src/
# Should find snapshot-based search methods

# Rule 5: Zero Overhead When Disabled
grep -r "if.*index_enabled\|Option<InvertedIndex>" crates/search/src/
# Should find conditional indexing logic

# Rule 6: Algorithm Swappable (Scorer and Fuser are traits)
grep -r "trait Scorer\|trait Fuser" crates/search/src/
# Should find trait definitions, not hardcoded implementations
```

### 6d.2 Core Types Verification (Epic 33)

```bash
# Verify SearchRequest, SearchResponse, SearchHit exist
~/.cargo/bin/cargo test --package in-mem-search search_types

# Verify DocRef and PrimitiveKind
~/.cargo/bin/cargo test --package in-mem-search doc_ref
~/.cargo/bin/cargo test --package in-mem-search primitive_kind

# Verify SearchBudget
~/.cargo/bin/cargo test --package in-mem-search budget
```

### 6d.3 Primitive Search Surface Verification (Epic 34)

```bash
# Verify Searchable trait
~/.cargo/bin/cargo test --package in-mem-search searchable

# Verify search() on each primitive
~/.cargo/bin/cargo test --package primitives kv_search
~/.cargo/bin/cargo test --package primitives json_search
~/.cargo/bin/cargo test --package primitives event_search
~/.cargo/bin/cargo test --package primitives state_search
~/.cargo/bin/cargo test --package primitives trace_search
~/.cargo/bin/cargo test --package primitives run_search
```

### 6d.4 Scoring Infrastructure Verification (Epic 35)

```bash
# Verify Scorer trait
~/.cargo/bin/cargo test --package in-mem-search scorer

# Verify ScorerContext
~/.cargo/bin/cargo test --package in-mem-search scorer_context

# Verify BM25LiteScorer
~/.cargo/bin/cargo test --package in-mem-search bm25

# Verify Tokenizer
~/.cargo/bin/cargo test --package in-mem-search tokenizer
```

### 6d.5 Composite Search Verification (Epic 36)

```bash
# Verify HybridSearch
~/.cargo/bin/cargo test --package in-mem-search hybrid

# Verify db.hybrid() accessor
~/.cargo/bin/cargo test --package primitives hybrid_accessor

# Verify primitive filter
~/.cargo/bin/cargo test --package in-mem-search primitive_filter

# Verify budget allocation
~/.cargo/bin/cargo test --package in-mem-search budget_allocation
```

### 6d.6 Fusion Infrastructure Verification (Epic 37)

```bash
# Verify Fuser trait
~/.cargo/bin/cargo test --package in-mem-search fuser

# Verify RRFFuser (Reciprocal Rank Fusion)
~/.cargo/bin/cargo test --package in-mem-search rrf

# Verify tie-breaking
~/.cargo/bin/cargo test --package in-mem-search tiebreak

# Verify deduplication
~/.cargo/bin/cargo test --package in-mem-search dedup
```

### 6d.7 Optional Indexing Verification (Epic 38)

```bash
# Verify InvertedIndex
~/.cargo/bin/cargo test --package in-mem-search inverted_index

# Verify enable/disable indexing
~/.cargo/bin/cargo test --package in-mem-search enable_index
~/.cargo/bin/cargo test --package in-mem-search disable_index

# Verify index updates on write
~/.cargo/bin/cargo test --package in-mem-search index_update

# Verify accelerated search
~/.cargo/bin/cargo test --package in-mem-search accelerated_search

# Verify watermark tracking
~/.cargo/bin/cargo test --package in-mem-search watermark
```

### 6d.8 Validation & Non-Regression (Epic 39)

```bash
# Run API contract tests
~/.cargo/bin/cargo test --package in-mem-search api_contracts

# Run non-regression benchmarks
~/.cargo/bin/cargo bench --bench m6_non_regression

# Run determinism tests
~/.cargo/bin/cargo test --package in-mem-search determinism

# Run snapshot consistency tests
~/.cargo/bin/cargo test --package in-mem-search snapshot_consistency
```

### 6d.9 Performance Benchmarks

```bash
# Run M6 search benchmarks
~/.cargo/bin/cargo bench --bench m6_non_regression

# Expected results:
# - KV get (no regression): < 5µs
# - KV put (no regression): < 8µs
# - JSON get (no regression): 30-50µs
# - Search scan (1K docs, no index): < 50ms
# - Search indexed (1K docs): < 10ms
```

### 6d.10 Non-Regression Verification

```bash
# Verify M4 performance targets are maintained
~/.cargo/bin/cargo bench --bench m4_performance

# Verify M5 performance targets are maintained
~/.cargo/bin/cargo bench --bench m5_json_performance

# Run M4 red flag tests (must still pass)
~/.cargo/bin/cargo test --test m4_red_flags

# Verify existing primitives unaffected
~/.cargo/bin/cargo test --package primitives kv_store
~/.cargo/bin/cargo test --package primitives json_store
~/.cargo/bin/cargo test --package primitives event_log
~/.cargo/bin/cargo test --package primitives state_cell
~/.cargo/bin/cargo test --package primitives trace_store
```

### 6d.11 M6 Performance Checklist

| Metric | Target | Red Flag | Actual | Status |
|--------|--------|----------|--------|--------|
| KV get (no regression) | < 5µs | > 10µs | ___ | [ ] |
| KV put (no regression) | < 8µs | > 20µs | ___ | [ ] |
| JSON get (no regression) | 30-50µs | > 100µs | ___ | [ ] |
| JSON set (no regression) | 100-200µs | > 500µs | ___ | [ ] |
| Search scan (1K, no index) | < 50ms | > 200ms | ___ | [ ] |
| Search indexed (1K) | < 10ms | > 50ms | ___ | [ ] |
| Index update overhead | < 5× write | > 10× write | ___ | [ ] |
| Budget enforcement | Works | N/A | ___ | [ ] |

---

## Phase 7: Final Sign-Off

### 7.1 Completion Checklist

| Item | Status |
|------|--------|
| All automated checks pass (Phase 1) | [ ] |
| All stories verified complete (Phase 2) | [ ] |
| Spec compliance confirmed (Phase 3) | [ ] |
| Code review complete (Phase 4) | [ ] |
| Best practices verified (Phase 5) | [ ] |
| Epic-specific validation done (Phase 6) | [ ] |
| **M4 Only**: Performance validation (Phase 6b) | [ ] |
| **M4 Only**: All red flags pass | [ ] |
| **M5 Only**: JSON primitive validation (Phase 6c) | [ ] |
| **M5 Only**: Six architectural rules verified | [ ] |
| **M5 Only**: Non-regression tests pass | [ ] |
| **M6 Only**: Retrieval surfaces validation (Phase 6d) | [ ] |
| **M6 Only**: Six architectural rules verified | [ ] |
| **M6 Only**: Non-regression tests pass (M4/M5 targets maintained) | [ ] |

### 7.2 Sign-Off

```
Epic: ___
Reviewer: ___
Date: ___

[ ] This epic is APPROVED for merge to develop

Notes:
_______________________
```

---

## Post-Validation: Merge to Develop

After all phases pass:

```bash
# 1. Ensure epic branch is up to date
git checkout epic-<N>-<name>
git pull origin epic-<N>-<name>

# 2. Final test run
~/.cargo/bin/cargo test --workspace

# 3. Merge to develop
git checkout develop
git pull origin develop
git merge --no-ff epic-<N>-<name> -m "Epic <N>: <Epic Name> complete

Delivered:
- Story #XXX: Description
- Story #XXX: Description
...

All validation phases passed."

# 4. Push
git push origin develop

# 5. Close epic issue
/opt/homebrew/bin/gh issue close <epic-issue-number> --comment "Epic complete. All stories delivered and validated."
```

---

## Validation Prompt Template

### M3 Epic Validation (Epics 13-19)

Use this prompt to run epic-end validation:

```
## Task: Epic End Validation

Run the complete epic-end validation for Epic <N>: <Epic Name>.

**Steps**:
1. Run Phase 1 automated checks
2. Verify all <X> stories are complete (Phase 2)
3. Verify spec compliance against M3_ARCHITECTURE.md (Phase 3)
4. Perform code review checklist (Phase 4)
5. Verify best practices (Phase 5)
6. Run epic-specific validation (Phase 6)
7. Provide final sign-off summary (Phase 7)

**Expected Output**:
- Pass/fail status for each phase
- Any issues found with recommendations
- Final sign-off or list of blockers

**Reference**:
- Epic prompt: docs/prompts/epic-<N>-claude-prompts.md
- M3 Architecture: docs/architecture/M3_ARCHITECTURE.md
- Stories: #XXX - #XXX
```

### M4 Epic Validation (Epics 20-25)

Use this prompt for M4 performance milestone validation:

```
## Task: M4 Epic End Validation

Run the complete epic-end validation for Epic <N>: <Epic Name>.

**Steps**:
1. Run Phase 1 automated checks
2. Verify all <X> stories are complete (Phase 2)
3. Verify spec compliance against M4_ARCHITECTURE.md (Phase 3)
4. Perform code review checklist (Phase 4)
5. Verify best practices (Phase 5)
6. Run epic-specific validation (Phase 6)
7. Run M4 performance validation (Phase 6b) - CRITICAL
8. Verify ALL red flag thresholds pass
9. Provide final sign-off summary (Phase 7)

**Expected Output**:
- Pass/fail status for each phase
- Performance benchmark results vs targets
- Red flag test results (ALL must pass)
- Any issues found with recommendations
- Final sign-off or list of blockers

**Reference**:
- Epic prompt: docs/prompts/epic-<N>-claude-prompts.md
- M4 Architecture: docs/architecture/M4_ARCHITECTURE.md
- M4 Prompt Header: docs/prompts/M4_PROMPT_HEADER.md
- Stories: #XXX - #XXX

**CRITICAL**: If ANY red flag fails, STOP and REDESIGN. Do NOT proceed.
```

### M5 Epic Validation (Epics 26-32)

Use this prompt for M5 JSON primitive validation:

```
## Task: M5 Epic End Validation

Run the complete epic-end validation for Epic <N>: <Epic Name>.

**Steps**:
1. Run Phase 1 automated checks
2. Verify all <X> stories are complete (Phase 2)
3. Verify spec compliance against M5_ARCHITECTURE.md (Phase 3)
4. Perform code review checklist (Phase 4)
5. Verify best practices (Phase 5)
6. Run epic-specific validation (Phase 6)
7. Run M5 JSON validation (Phase 6c) - CRITICAL
8. Verify ALL six architectural rules
9. Run non-regression tests (M4 performance targets must be maintained)
10. Provide final sign-off summary (Phase 7)

**Expected Output**:
- Pass/fail status for each phase
- Six architectural rules verification results
- JSON performance benchmark results vs targets
- Non-regression test results (M4 targets maintained)
- Any issues found with recommendations
- Final sign-off or list of blockers

**Reference**:
- **M5 Architecture (AUTHORITATIVE)**: docs/architecture/M5_ARCHITECTURE.md
- Epic prompt: docs/prompts/M5/epic-<N>-claude-prompts.md
- M5 Implementation Plan: docs/milestones/M5/M5_IMPLEMENTATION_PLAN.md
- M5 Prompt Header: docs/prompts/M5/M5_PROMPT_HEADER.md
- Epic Spec: docs/milestones/M5/EPIC_<N>_*.md
- Stories: #XXX - #XXX

**CRITICAL**: The six architectural rules are NON-NEGOTIABLE. Any violation is a blocking issue.
```

### M6 Epic Validation (Epics 33-39)

Use this prompt for M6 retrieval surfaces validation:

```
## Task: M6 Epic End Validation

Run the complete epic-end validation for Epic <N>: <Epic Name>.

**Steps**:
1. Run Phase 1 automated checks
2. Verify all <X> stories are complete (Phase 2)
3. Verify spec compliance against M6_ARCHITECTURE.md (Phase 3)
4. Perform code review checklist (Phase 4)
5. Verify best practices (Phase 5)
6. Run epic-specific validation (Phase 6)
7. Run M6 retrieval surfaces validation (Phase 6d) - CRITICAL
8. Verify ALL six architectural rules
9. Run non-regression tests (M4 and M5 performance targets must be maintained)
10. Provide final sign-off summary (Phase 7)

**Expected Output**:
- Pass/fail status for each phase
- Six architectural rules verification results
- Search performance benchmark results vs targets
- Non-regression test results (M4/M5 targets maintained)
- Any issues found with recommendations
- Final sign-off or list of blockers

**Reference**:
- **M6 Architecture (AUTHORITATIVE)**: docs/architecture/M6_ARCHITECTURE.md
- Epic prompt: docs/prompts/M6/epic-<N>-claude-prompts.md
- M6 Prompt Header: docs/prompts/M6/M6_PROMPT_HEADER.md
- Epic Spec: docs/milestones/M6/EPIC_<N>_*.md
- Stories: #XXX - #XXX

**CRITICAL**: The six architectural rules are NON-NEGOTIABLE:
1. No Data Movement (DocRef references only)
2. Primitive Search is First-Class (every primitive has .search())
3. Composite Orchestrates, Doesn't Replace (db.hybrid() delegates)
4. Snapshot-Consistent Search (single snapshot for all primitives)
5. Zero Overhead When Disabled (no allocations when indexing off)
6. Algorithm Swappable (Scorer and Fuser are traits)
```

---

## Quick Reference: Validation Commands

### M3 Quick Validation

```bash
# One-liner for Phase 1
~/.cargo/bin/cargo build --workspace && \
~/.cargo/bin/cargo test --workspace && \
~/.cargo/bin/cargo clippy --workspace -- -D warnings && \
~/.cargo/bin/cargo fmt --check && \
echo "Phase 1: PASS"

# Count tests in primitives
~/.cargo/bin/cargo test --package primitives -- --list 2>/dev/null | grep -c "test"

# Check for spec deviations
grep -r "TODO\|FIXME\|HACK" crates/primitives/src/
```

### M4 Quick Validation

```bash
# One-liner for Phase 1 + M4 performance
~/.cargo/bin/cargo build --workspace && \
~/.cargo/bin/cargo test --workspace && \
~/.cargo/bin/cargo clippy --workspace -- -D warnings && \
~/.cargo/bin/cargo fmt --check && \
~/.cargo/bin/cargo bench --bench m4_performance && \
~/.cargo/bin/cargo test --test m4_red_flags && \
echo "Phase 1 + M4 Performance: PASS"

# Run all M4 benchmarks
~/.cargo/bin/cargo bench --bench m4_performance
~/.cargo/bin/cargo bench --bench m4_facade_tax

# Run all M4 tests
~/.cargo/bin/cargo test --test m4_red_flags
~/.cargo/bin/cargo test --test m4_fast_path_equivalence
~/.cargo/bin/cargo test --test m4_scaling
~/.cargo/bin/cargo test --test m4_contention

# Check red flag compliance
~/.cargo/bin/cargo test --test m4_red_flags -- --nocapture
```

### M4 Red Flag Quick Check

```bash
# Quick red flag verification (run after any M4 change)
~/.cargo/bin/cargo test --test m4_red_flags -- \
  test_snapshot_acquisition_time \
  test_facade_tax_a1_a0 \
  test_facade_tax_b_a1 \
  test_disjoint_scaling \
  test_p99_latency \
  test_zero_allocations
```

### M5 Quick Validation

```bash
# One-liner for Phase 1 + M5 JSON validation
~/.cargo/bin/cargo build --workspace && \
~/.cargo/bin/cargo test --workspace && \
~/.cargo/bin/cargo clippy --workspace -- -D warnings && \
~/.cargo/bin/cargo fmt --check && \
~/.cargo/bin/cargo bench --bench m5_json_performance && \
~/.cargo/bin/cargo test --test m4_red_flags && \
echo "Phase 1 + M5 Validation: PASS"

# Run all M5 JSON tests
~/.cargo/bin/cargo test --package in-mem-core json_
~/.cargo/bin/cargo test --package primitives json_store
~/.cargo/bin/cargo test --package in-mem-concurrency json_
~/.cargo/bin/cargo test --package in-mem-durability json_

# Run M5 benchmarks
~/.cargo/bin/cargo bench --bench m5_json_performance

# Run non-regression tests
~/.cargo/bin/cargo bench --bench m4_performance
~/.cargo/bin/cargo test --test m4_red_flags
```

### M5 Six Rules Quick Check

```bash
# Quick architectural rules verification (run after any M5 change)
echo "Rule 1: No separate DashMap for JSON"
grep -r "DashMap.*Json" crates/primitives/src/ && echo "FAIL: Found separate DashMap" || echo "PASS"

echo "Rule 2: Stateless facade"
grep -A5 "pub struct JsonStore" crates/primitives/src/json_store.rs

echo "Rule 3: Extension trait"
grep -r "impl JsonStoreExt for TransactionContext" crates/ && echo "PASS" || echo "FAIL: Missing impl"

echo "Rule 4: No path in storage"
grep -r "storage.put_at_path\|storage.get_at_path" crates/ && echo "FAIL: Found path in storage" || echo "PASS"

echo "Rule 5: Unified WAL"
grep -r "JsonCreate\|JsonSet" crates/durability/src/wal.rs && echo "PASS" || echo "FAIL: Missing JSON WAL entries"

echo "Rule 6: Consistent API"
grep -r "fn create\|fn get\|fn set" crates/primitives/src/json_store.rs
```

### M6 Quick Validation

```bash
# One-liner for Phase 1 + M6 retrieval surfaces validation
~/.cargo/bin/cargo build --workspace && \
~/.cargo/bin/cargo test --workspace && \
~/.cargo/bin/cargo clippy --workspace -- -D warnings && \
~/.cargo/bin/cargo fmt --check && \
~/.cargo/bin/cargo bench --bench m6_non_regression && \
~/.cargo/bin/cargo test --test m4_red_flags && \
echo "Phase 1 + M6 Validation: PASS"

# Run all M6 search tests
~/.cargo/bin/cargo test --package in-mem-search
~/.cargo/bin/cargo test --package primitives search

# Run M6 benchmarks
~/.cargo/bin/cargo bench --bench m6_non_regression

# Run non-regression tests
~/.cargo/bin/cargo bench --bench m4_performance
~/.cargo/bin/cargo bench --bench m5_json_performance
~/.cargo/bin/cargo test --test m4_red_flags
```

### M6 Six Rules Quick Check

```bash
# Quick architectural rules verification (run after any M6 change)
echo "Rule 1: No Data Movement (DocRef references only)"
grep -r "fn search.*SearchResponse" crates/search/src/ && echo "PASS" || echo "FAIL"
grep -r "SearchHit.*doc_ref" crates/search/src/ && echo "PASS: DocRef found" || echo "FAIL: No DocRef"

echo "Rule 2: Primitive Search is First-Class"
grep -r "impl Searchable for" crates/primitives/src/ | wc -l
# Should return 6 (one per primitive)

echo "Rule 3: Composite Orchestrates, Doesn't Replace"
grep -A10 "HybridSearch" crates/search/src/ | grep -r "primitive.*search\|delegate" && echo "PASS" || echo "CHECK MANUALLY"

echo "Rule 4: Snapshot-Consistent Search"
grep -r "search_with_snapshot\|SearchSnapshot" crates/search/src/ && echo "PASS" || echo "FAIL: No snapshot search"

echo "Rule 5: Zero Overhead When Disabled"
grep -r "if.*index_enabled\|Option<InvertedIndex>\|index.is_none()" crates/search/src/ && echo "PASS" || echo "CHECK: May have overhead"

echo "Rule 6: Algorithm Swappable"
grep -r "trait Scorer" crates/search/src/ && echo "PASS: Scorer trait" || echo "FAIL: No Scorer trait"
grep -r "trait Fuser" crates/search/src/ && echo "PASS: Fuser trait" || echo "FAIL: No Fuser trait"
```

---

## M4 Milestone Completion Checklist

**Use this checklist when completing ALL M4 epics (Epic 25 final validation):**

| Category | Requirement | Status |
|----------|-------------|--------|
| **Durability** | | |
| | InMemory mode: < 3µs | [ ] |
| | Buffered mode: < 30µs | [ ] |
| | Strict mode: ~2ms (M3 compatible) | [ ] |
| | Per-operation override works | [ ] |
| | Graceful shutdown flushes all | [ ] |
| **Sharding** | | |
| | ShardedStore with DashMap | [ ] |
| | Snapshot acquisition < 2µs | [ ] |
| | Zero-allocation snapshots | [ ] |
| **Pooling** | | |
| | Thread-local transaction pools | [ ] |
| | reset() preserves capacity | [ ] |
| | No pool exhaustion | [ ] |
| **Read Path** | | |
| | Fast path get() < 5µs | [ ] |
| | Batch get uses single snapshot | [ ] |
| | Observational equivalence | [ ] |
| **Red Flags** | | |
| | Snapshot: < 2µs | [ ] |
| | A1/A0: < 20× | [ ] |
| | B/A1: < 8× | [ ] |
| | Scaling (4T): > 2.5× | [ ] |
| | p99: < 20× mean | [ ] |
| | Hot-path allocations: 0 | [ ] |
| **Documentation** | | |
| | M4_ARCHITECTURE.md complete | [ ] |
| | All story PRs merged | [ ] |
| | Benchmark results documented | [ ] |

---

## M5 Milestone Completion Checklist

**Use this checklist when completing ALL M5 epics (Epic 32 final validation):**

| Category | Requirement | Status |
|----------|-------------|--------|
| **Core Types (Epic 26)** | | |
| | JsonDocId with UUID | [ ] |
| | JsonValue enum (Null, Bool, Number, String, Array, Object) | [ ] |
| | JsonPath parsing and manipulation | [ ] |
| | JsonPatch operations (Set, Delete, ArrayPush, etc.) | [ ] |
| | TypeTag::Json = 0x11 | [ ] |
| **Path Operations (Epic 27)** | | |
| | get_at_path() | [ ] |
| | set_at_path() | [ ] |
| | delete_at_path() | [ ] |
| | apply_patches() | [ ] |
| | Path overlap detection | [ ] |
| **JsonStore (Epic 28)** | | |
| | Stateless facade (Arc<Database> only) | [ ] |
| | create() | [ ] |
| | get() fast path (no transaction) | [ ] |
| | set() | [ ] |
| | delete_at_path() | [ ] |
| | destroy() | [ ] |
| **WAL Integration (Epic 29)** | | |
| | JsonCreate (0x20) | [ ] |
| | JsonSet (0x21) | [ ] |
| | JsonDelete (0x22) | [ ] |
| | JsonDestroy (0x23) | [ ] |
| | Serialization/deserialization | [ ] |
| | Recovery from WAL | [ ] |
| **Transaction Integration (Epic 30)** | | |
| | JsonStoreExt trait | [ ] |
| | txn.json_get() | [ ] |
| | txn.json_set() | [ ] |
| | Cross-primitive atomicity | [ ] |
| | Lazy state allocation | [ ] |
| **Conflict Detection (Epic 31)** | | |
| | Path overlap detection | [ ] |
| | Read-write conflict check | [ ] |
| | Write-write conflict check | [ ] |
| | Version mismatch detection | [ ] |
| | Conflict aborts entire transaction | [ ] |
| **Validation (Epic 32)** | | |
| | Unit tests comprehensive | [ ] |
| | Integration tests pass | [ ] |
| | Benchmarks meet targets | [ ] |
| | M4 non-regression verified | [ ] |
| **Six Architectural Rules** | | |
| | Rule 1: JSON in ShardedStore | [ ] |
| | Rule 2: Stateless facade | [ ] |
| | Rule 3: Extension trait (not separate type) | [ ] |
| | Rule 4: Path semantics in API layer | [ ] |
| | Rule 5: Unified WAL | [ ] |
| | Rule 6: Consistent API pattern | [ ] |
| **Performance** | | |
| | JSON create (1KB) < 1ms | [ ] |
| | JSON get at path (1KB) < 100µs | [ ] |
| | JSON set at path (1KB) < 1ms | [ ] |
| | JSON delete at path (1KB) < 500µs | [ ] |
| | No regression in M4 targets | [ ] |
| **Documentation** | | |
| | M5_ARCHITECTURE.md authoritative | [ ] |
| | M5_IMPLEMENTATION_PLAN.md aligned | [ ] |
| | All EPIC_*.md specs complete | [ ] |
| | All story PRs merged | [ ] |
| | Benchmark results documented | [ ] |

---

## M6 Milestone Completion Checklist

**Use this checklist when completing ALL M6 epics (Epic 39 final validation):**

| Category | Requirement | Status |
|----------|-------------|--------|
| **Core Types (Epic 33)** | | |
| | SearchRequest with run_id, query, filters | [ ] |
| | SearchBudget with time and candidate limits | [ ] |
| | SearchResponse with hits and stats | [ ] |
| | SearchHit with doc_ref, score, rank | [ ] |
| | SearchStats with timing and counts | [ ] |
| | DocRef references documents by primitive+id | [ ] |
| | PrimitiveKind enum (Kv, Json, Event, State, Trace, Run) | [ ] |
| **Primitive Search (Epic 34)** | | |
| | Searchable trait defined | [ ] |
| | KvStore.search() | [ ] |
| | JsonStore.search() | [ ] |
| | EventLog.search() | [ ] |
| | StateCell.search() | [ ] |
| | TraceStore.search() | [ ] |
| | RunIndex.search() | [ ] |
| | Text extraction for each primitive | [ ] |
| **Scoring Infrastructure (Epic 35)** | | |
| | Scorer trait (pluggable) | [ ] |
| | ScorerContext with IDF and corpus stats | [ ] |
| | BM25LiteScorer default implementation | [ ] |
| | Tokenizer (lowercase, split on non-alphanumeric) | [ ] |
| | Extensions field for future signals | [ ] |
| **Composite Search (Epic 36)** | | |
| | HybridSearch struct | [ ] |
| | db.hybrid() accessor method | [ ] |
| | Primitive filter (limit to subset) | [ ] |
| | Budget allocation across primitives | [ ] |
| | Parallel primitive orchestration | [ ] |
| **Fusion Infrastructure (Epic 37)** | | |
| | Fuser trait (pluggable) | [ ] |
| | RRFFuser (Reciprocal Rank Fusion) | [ ] |
| | Deterministic tie-breaking | [ ] |
| | Cross-primitive deduplication | [ ] |
| **Optional Indexing (Epic 38)** | | |
| | InvertedIndex with PostingList | [ ] |
| | Enable/disable per-primitive | [ ] |
| | Automatic index updates on write | [ ] |
| | Accelerated search when index available | [ ] |
| | Watermark tracking for staleness | [ ] |
| **Validation (Epic 39)** | | |
| | API contract tests for all 6 primitives | [ ] |
| | Non-regression benchmark suite | [ ] |
| | Determinism tests (same inputs = same outputs) | [ ] |
| | Snapshot consistency tests | [ ] |
| | Index vs scan consistency | [ ] |
| **Six Architectural Rules** | | |
| | Rule 1: No Data Movement (DocRef only) | [ ] |
| | Rule 2: Primitive Search First-Class | [ ] |
| | Rule 3: Composite Orchestrates | [ ] |
| | Rule 4: Snapshot-Consistent Search | [ ] |
| | Rule 5: Zero Overhead When Disabled | [ ] |
| | Rule 6: Algorithm Swappable | [ ] |
| **Performance** | | |
| | KV get (no regression) < 5µs | [ ] |
| | KV put (no regression) < 8µs | [ ] |
| | JSON get (no regression) 30-50µs | [ ] |
| | Search scan (1K, no index) < 50ms | [ ] |
| | Search indexed (1K) < 10ms | [ ] |
| | Index update overhead < 5× write | [ ] |
| | M4 red flags still pass | [ ] |
| **Documentation** | | |
| | M6_ARCHITECTURE.md authoritative | [ ] |
| | All EPIC_*.md specs complete | [ ] |
| | All story PRs merged | [ ] |
| | Benchmark results documented | [ ] |

---

*End of Epic End Validation Plan*
