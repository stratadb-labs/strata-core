# Milestone 11b: Primitives Quality & API Completion

> **Status**: Planning
> **Created**: 2026-01-22
> **Scope**: All 7 Primitives

## Objective

Systematically fix all documented defects, implement missing substrate APIs, and write comprehensive test suites for all 7 primitives.

## Scope

**7 Primitives**: KVStore, EventLog, StateCell, JsonStore, VectorStore, TraceStore, RunIndex

**135+ documented issues** across:
- `docs/defects/KV_DEFECTS.md` (14 issues)
- `docs/defects/EVENTLOG_DEFECTS.md` (20 issues)
- `docs/defects/STATECELL_DEFECTS.md` (19 issues)
- `docs/defects/JSONSTORE_DEFECTS.md` (18 issues)
- `docs/defects/VECTORSTORE_DEFECTS.md` (20 issues)
- `docs/defects/TRACESTORE_DEFECTS.md` (23 issues)
- `docs/defects/RUNINDEX_DEFECTS.md` (21 issues)
- `docs/defects/FOUNDATIONAL_CAPABILITIES_AUDIT.md` (cross-cutting)

---

## Execution Order

### Phase 1: Fix P0 Bugs & Expose Hidden Features (2-3 days)

**Goal**: Fix critical bugs and expose already-implemented primitive features at Substrate level.

#### 1.1 KVStore P0 Fixes
- [ ] Add key validation (empty, NUL bytes, `_strata/` prefix) in `crates/api/src/substrate/kv.rs`
- [ ] Fix `kv_incr` overflow with `checked_add()` at `crates/api/src/substrate/kv.rs:372`

#### 1.2 EventLog P0 Fixes
- [ ] Add payload type validation (must be Object) in `event_append`
- [ ] Add stream name validation (non-empty) in `event_append`
- [ ] Expose `event_rev_range` (primitive has `xrevrange`)
- [ ] Expose `event_streams` (primitive has `event_types()`)
- [ ] Expose `event_head` (primitive has `head()`)
- [ ] Expose `event_verify_chain` (primitive has `verify_chain()`)

#### 1.3 StateCell P0 Fixes
- [ ] Expose `state_transition` / `state_transition_or_init` (primitive has both)
- [ ] Expose `state_list` (primitive has `list()`)
- [ ] Expose `state_init` (primitive has `init()`)

#### 1.4 JsonStore P0 Fixes
- [ ] Expose `json_search` (primitive has `search()`)
- [ ] Expose `json_exists` (primitive has `exists()`)
- [ ] Expose `json_get_version` (primitive has `get_version()`)

#### 1.5 VectorStore P0 Fixes
- [ ] **CRITICAL**: Fix SearchFilter silently ignored - wire to primitive or return error
- [ ] **CRITICAL**: Fix vector data always empty in search results
- [ ] Expose `vector_list_collections` (primitive has `list_collections()`)
- [ ] Expose `vector_exists` (primitive has `collection_exists()`)

#### 1.6 TraceStore P0 Fixes
- [ ] Expose `trace_query_by_time` (primitive has `query_by_time()`)
- [ ] Expose `trace_query_by_tag` (primitive has `query_by_tag()`)
- [ ] Expose `trace_count` (primitive has `count()`)
- [ ] Expose `trace_search` (primitive has `search()`)

#### 1.7 RunIndex P0 Fixes
- [ ] Expose full RunStatus enum (6 states, not 2)
- [ ] Expose `run_pause` / `run_resume` (primitive has both)
- [ ] Expose `run_fail` with error (primitive has `fail_run()`)
- [ ] Expose `run_delete` with cascade (primitive has `delete_run()`)
- [ ] Expose `run_query_by_status` (primitive has it)
- [ ] Add error field to RunInfo

---

### Phase 2: Implement Stubbed APIs (3-4 days)

**Goal**: Implement APIs that exist in trait but return stubs/placeholders.

#### 2.1 History APIs (Cross-Primitive)
- [ ] Implement `kv_history()` - currently returns `vec![]`
- [ ] Implement `kv_get_at()` - currently only checks current version
- [ ] Implement `state_history()` - currently returns `vec![]`
- [ ] Implement `json_history()` - currently returns `vec![]`

#### 2.2 Version Tracking (Cross-Primitive)
- [ ] Fix VectorStore to return real versions (not `Version::Txn(0)`)
- [ ] Fix TraceStore queries to preserve versions
- [ ] Fix RunIndex to return real versions

#### 2.3 Missing Table Stakes APIs
- [ ] KVStore: Add `kv_keys` and `kv_scan` operations
- [ ] EventLog: Add `event_append_batch`, `event_stream_info`, `event_range_by_time`
- [ ] StateCell: Add `state_get_or_init`, `state_info`
- [ ] JsonStore: Add `json_keys`/`json_list`, `json_create`, `json_destroy`
- [ ] VectorStore: Add batch operations, `vector_list`/`vector_scan`
- [ ] TraceStore: Add `trace_info`, `trace_list_tags`
- [ ] RunIndex: Add tag management (`run_add_tags`, `run_remove_tags`), `run_count`

#### 2.4 Retention System
- [ ] Implement `run_set_retention` (currently no-op)
- [ ] Implement `run_get_retention` (currently returns KeepAll)

---

### Phase 3: Comprehensive Test Suites (5-7 days)

**Goal**: Create comprehensive test suites for all 6 remaining primitives following the KV pattern.

#### Test Suite Structure (per primitive)
```
tests/substrate_api_comprehensive/<primitive>/
├── mod.rs                      # Module declaration
├── basic_ops.rs               # CRUD operations
├── <domain_specific>.rs       # Domain-specific tests
├── durability.rs              # Crash recovery
├── concurrency.rs             # Thread safety
├── recovery_invariants.rs     # R1-R6 guarantees
└── edge_cases.rs              # Validation & boundaries

testdata/
├── <primitive>_test_data.jsonl      # 2000+ entries
└── <primitive>_edge_cases.jsonl     # Categorized edge cases
```

#### 3.1 EventLog Test Suite (~100 tests)
- [ ] Create `testdata/eventlog_test_data.jsonl`
- [ ] Create `testdata/eventlog_edge_cases.jsonl`
- [ ] Implement test modules:
  - `basic_ops.rs`: append, get, range, len, latest_sequence
  - `streams.rs`: multi-stream isolation, stream enumeration
  - `immutability.rs`: append-only verification
  - `durability.rs`: crash recovery across modes
  - `concurrency.rs`: thread safety, ordering
  - `recovery_invariants.rs`: R1-R6 + E1-E3
  - `edge_cases.rs`: payload validation, stream names, sequences

#### 3.2 StateCell Test Suite (~80 tests)
- [ ] Create `testdata/statecell_test_data.jsonl`
- [ ] Create `testdata/statecell_edge_cases.jsonl`
- [ ] Implement test modules:
  - `basic_ops.rs`: set, get, delete, exists
  - `transitions.rs`: transition closures, retry behavior
  - `cas_ops.rs`: compare-and-swap semantics
  - `durability.rs`: crash recovery
  - `concurrency.rs`: contention, atomic updates
  - `recovery_invariants.rs`: R1-R6
  - `edge_cases.rs`: cell names, value limits

#### 3.3 JsonStore Test Suite (~90 tests)
- [ ] Create `testdata/jsonstore_test_data.jsonl`
- [ ] Create `testdata/jsonstore_edge_cases.jsonl`
- [ ] Implement test modules:
  - `basic_ops.rs`: create, get, set, delete
  - `path_ops.rs`: path navigation, nested updates
  - `merge_ops.rs`: JSON merge patch
  - `durability.rs`: crash recovery
  - `concurrency.rs`: concurrent path updates
  - `recovery_invariants.rs`: R1-R6
  - `edge_cases.rs`: path validation, depth limits, size limits

#### 3.4 VectorStore Test Suite (~80 tests)
- [ ] Create `testdata/vectorstore_test_data.jsonl`
- [ ] Create `testdata/vectorstore_edge_cases.jsonl`
- [ ] Implement test modules:
  - `basic_ops.rs`: upsert, get, delete
  - `collections.rs`: create, drop, info
  - `search.rs`: similarity search, filtering, metrics
  - `durability.rs`: crash recovery, WAL replay
  - `concurrency.rs`: concurrent inserts/searches
  - `recovery_invariants.rs`: R1-R6
  - `edge_cases.rs`: dimension validation, key validation

#### 3.5 TraceStore Test Suite (~90 tests)
- [ ] Create `testdata/tracestore_test_data.jsonl`
- [ ] Create `testdata/tracestore_edge_cases.jsonl`
- [ ] Implement test modules:
  - `basic_ops.rs`: create, get, list
  - `hierarchy.rs`: parent-child, tree reconstruction
  - `queries.rs`: by-type, by-tag, by-time
  - `durability.rs`: crash recovery
  - `concurrency.rs`: concurrent trace recording
  - `recovery_invariants.rs`: R1-R6
  - `edge_cases.rs`: trace types, tag limits

#### 3.6 RunIndex Test Suite (~80 tests)
- [ ] Create `testdata/runindex_test_data.jsonl`
- [ ] Create `testdata/runindex_edge_cases.jsonl`
- [ ] Implement test modules:
  - `basic_ops.rs`: create, get, list
  - `lifecycle.rs`: status transitions, pause/resume/fail/cancel
  - `hierarchy.rs`: parent-child runs
  - `queries.rs`: by-status, by-tag
  - `durability.rs`: crash recovery
  - `recovery_invariants.rs`: R1-R6
  - `edge_cases.rs`: metadata limits, tag limits

---

## Key Files to Modify

### Substrate API (Phase 1-2)
- `crates/api/src/substrate/kv.rs`
- `crates/api/src/substrate/event.rs`
- `crates/api/src/substrate/state.rs`
- `crates/api/src/substrate/json.rs`
- `crates/api/src/substrate/vector.rs`
- `crates/api/src/substrate/trace.rs`
- `crates/api/src/substrate/run.rs`
- `crates/api/src/substrate/retention.rs`

### Primitive Layer (if needed)
- `crates/primitives/src/kv.rs`
- `crates/primitives/src/event_log.rs`
- `crates/primitives/src/state_cell.rs`
- `crates/primitives/src/json_store.rs`
- `crates/primitives/src/vector/store.rs`
- `crates/primitives/src/trace.rs`
- `crates/primitives/src/run_index.rs`

### Test Infrastructure (Phase 3)
- `tests/substrate_api_comprehensive/main.rs` (shared utilities)
- `tests/substrate_api_comprehensive/test_data.rs` (data loaders)
- `tests/substrate_api_comprehensive/testdata/*.jsonl` (test data files)

---

## Verification

### After Phase 1
```bash
cargo test --test substrate_api_comprehensive
# Expected: ~163 KV tests + newly enabled tests pass
```

### After Phase 2
```bash
cargo test --test substrate_api_comprehensive
# Expected: Previously failing/ignored tests now pass
```

### After Phase 3
```bash
cargo test --test substrate_api_comprehensive
# Expected: ~700+ tests across all primitives
# Each primitive: ~80-100 tests
```

### Final Verification
```bash
# All tests pass
cargo test --test substrate_api_comprehensive

# No ignored tests (except known limitations)
cargo test --test substrate_api_comprehensive -- --ignored
# Should only run overflow test and any documented limitations
```

---

## Success Criteria

1. **All P0 bugs fixed** - Key validation, overflow, SearchFilter, version tracking
2. **All hidden features exposed** - Primitive capabilities available at Substrate
3. **All stubbed APIs implemented** - History, retention, listing operations
4. **Comprehensive test coverage** - ~700+ tests across 7 primitives
5. **Documentation updated** - Defect docs marked as resolved
6. **Zero regressions** - Existing tests continue to pass

---

## Dependencies

- KV test suite already complete (163 tests)
- Defect documentation complete for all primitives
- Test infrastructure (`TestDb`, `test_across_modes`) already in place

---

## Risk Mitigation

1. **Scope creep**: Focus on P0/P1 issues only; P2/world-class features deferred
2. **History implementation complexity**: Storage layer has `VersionChain`, but may need primitive changes
3. **Test data generation**: Reuse KV patterns; generate programmatically where possible

---

## Timeline Estimate

| Phase | Duration | Deliverables |
|-------|----------|--------------|
| Phase 1 | 2-3 days | P0 bugs fixed, hidden features exposed |
| Phase 2 | 3-4 days | Stubbed APIs implemented |
| Phase 3 | 5-7 days | 6 new test suites (~520 tests) |
| **Total** | **10-14 days** | 700+ tests, 135+ issues resolved |

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-22 | Initial plan based on defect documentation |
