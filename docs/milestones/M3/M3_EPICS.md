# M3 Epics and User Stories: Primitives

**Milestone**: M3 - Primitives
**Goal**: Implement all five MVP primitives as stateless facades over M2's transactional engine
**Estimated Duration**: 1 week (Week 4)

---

## Overview

M3 implements the five high-level primitives that agents will use directly:
1. **KVStore** - General-purpose key-value storage
2. **EventLog** - Immutable append-only event stream with causal hash chaining
3. **StateCell** - CAS-based versioned cells for coordination
4. **TraceStore** - Structured reasoning traces with indexing
5. **RunIndex** - First-class run lifecycle management

All primitives are logically stateful but operationally stateless - they hold no in-memory state and delegate to the Database engine.

---

## Epic 13: Primitives Foundation (GitHub #159)

**Goal**: Core infrastructure and common patterns for all primitives

### Scope
- Primitives crate structure with re-exports
- TypeTag extensions for new primitive types
- Common primitive trait/pattern (stateless facade pattern)
- Key construction helpers per TypeTag
- Transaction extension trait infrastructure

### Critical First Story
**Story #166: Primitives Crate Setup & TypeTag Extensions**
- **BLOCKS ALL M3 IMPLEMENTATION**
- Creates crate structure, TypeTag values, key construction patterns
- Must be reviewed and approved before primitive implementation

### Success Criteria
- [ ] `crates/primitives` crate created with proper dependencies
- [ ] TypeTag enum extended: KV=0x01, Event=0x02, State=0x03, Trace=0x04, Run=0x05
- [ ] Key construction helpers: `Key::new_kv()`, `Key::new_event()`, etc.
- [ ] Transaction extension trait pattern documented and scaffolded
- [ ] All primitives re-exported from `lib.rs`
- [ ] Unit tests for key construction pass

### Dependencies
- M2 complete (Database, TransactionContext, transactions work)

### Estimated Effort
1 day with 2 Claudes in parallel (after Story #65)

### Risks
- **Risk**: TypeTag collisions with future primitives
- **Mitigation**: Reserve range 0x10+ for post-MVP primitives

### User Stories
- **#166**: Primitives Crate Setup & TypeTag Extensions (3 hours) 🔴 FOUNDATION
- **#167**: Key Construction Helpers (3 hours)
- **#168**: Transaction Extension Trait Infrastructure (4 hours)

### Parallelization
After #166, stories #167-168 can run in parallel (2 Claudes)

---

## Epic 14: KVStore Primitive (GitHub #160)

**Goal**: General-purpose key-value storage with run isolation

### Scope
- KVStore struct as stateless facade
- Single-operation API (implicit transactions): get, put, put_with_ttl, delete, list
- Multi-operation API (explicit transactions): KVTransaction
- List with prefix filtering
- Transaction extension trait: KVStoreExt

### Success Criteria
- [ ] KVStore struct implemented with Arc<Database> reference
- [ ] `get()` returns Option<Value> for key within run namespace
- [ ] `put()` stores value with TypeTag::KV prefix
- [ ] `put_with_ttl()` stores value with expiration metadata
- [ ] `delete()` removes key
- [ ] `list()` and `list_with_values()` support prefix filtering
- [ ] KVTransaction for multi-operation atomicity
- [ ] KVStoreExt trait for cross-primitive transactions
- [ ] Run isolation verified (different runs don't see each other's data)
- [ ] All unit tests pass (>95% coverage)

### Dependencies
- Epic 13 (Primitives Foundation) complete

### Estimated Effort
1 day with 2 Claudes in parallel (after Story #68)

### Risks
- **Risk**: TTL implementation complexity
- **Mitigation**: TTL metadata stored, actual cleanup deferred to M4 background tasks

### User Stories
- **#169**: KVStore Core Structure (3 hours) 🔴 FOUNDATION
- **#170**: KVStore Single-Operation API (4 hours)
- **#171**: KVStore Multi-Operation API (3 hours)
- **#172**: KVStore List Operations (3 hours)
- **#173**: KVStoreExt Transaction Extension (3 hours)

### Parallelization
After #169, stories #170-172 can run in parallel (3 Claudes)

---

## Epic 15: EventLog Primitive (GitHub #161)

**Goal**: Immutable append-only event stream with causal hash chaining

### Scope
- EventLog struct as stateless facade
- Event structure with sequence, type, payload, timestamp, hashes
- Append operation with automatic sequence assignment and hash chaining
- Read operations: single event, range, head, length
- Chain verification
- Query by event type
- EventLog metadata key for sequence/head tracking

### Success Criteria
- [ ] EventLog struct implemented with Arc<Database> reference
- [ ] Event struct with all fields (sequence, event_type, payload, timestamp, prev_hash, hash)
- [ ] `append()` atomically increments sequence and chains hash
- [ ] `read()` and `read_range()` return events by sequence
- [ ] `head()` returns latest event
- [ ] `len()` returns event count
- [ ] `verify_chain()` validates hash chain integrity
- [ ] `read_by_type()` filters events by type
- [ ] EventIterator for streaming reads
- [ ] Append-only invariant enforced (no update/delete methods)
- [ ] Single-writer-ordered per run (CAS on metadata key)
- [ ] All unit tests pass (>95% coverage)

### Dependencies
- Epic 13 (Primitives Foundation) complete

### Estimated Effort
1.5 days with 2 Claudes in parallel (after Story #73)

### Risks
- **Risk**: Hash chaining logic bugs create invalid chains
- **Mitigation**: Comprehensive tests for chain verification
- **Risk**: Sequence gaps on transaction failure
- **Mitigation**: Sequence assignment and event write in same transaction (atomic)

### User Stories
- **#174**: EventLog Core & Event Structure (4 hours) 🔴 FOUNDATION
- **#175**: EventLog Append with Hash Chaining (5 hours)
- **#176**: EventLog Read Operations (4 hours)
- **#177**: EventLog Chain Verification (4 hours)
- **#178**: EventLog Query by Type (3 hours)
- **#179**: EventLogExt Transaction Extension (3 hours)

### Parallelization
After #174-175 (sequential), stories #176-178 can run in parallel (3 Claudes)

---

## Epic 16: StateCell Primitive (GitHub #162)

**Goal**: CAS-based versioned cells for coordination

### Scope
- StateCell struct as stateless facade
- State structure with value, version, updated_at
- Init, read, CAS, set, delete operations
- Transition closure pattern with automatic retry
- Purity requirement documentation and enforcement

### Success Criteria
- [ ] StateCell struct implemented with Arc<Database> reference
- [ ] State struct with value, version, updated_at fields
- [ ] `init()` creates cell only if not exists
- [ ] `read()` returns current state with version
- [ ] `cas()` atomically updates only if version matches
- [ ] `set()` unconditionally updates (force write)
- [ ] `delete()` removes cell
- [ ] `list()` returns all cell names in run
- [ ] `exists()` checks cell existence
- [ ] `transition()` closure pattern with automatic OCC retry
- [ ] Version monotonicity enforced
- [ ] StateCellExt transaction extension trait
- [ ] All unit tests pass (>95% coverage)

### Dependencies
- Epic 13 (Primitives Foundation) complete

### Estimated Effort
1 day with 2 Claudes in parallel (after Story #79)

### Risks
- **Risk**: Transition closure impurity causes subtle bugs
- **Mitigation**: Document purity requirement, can't enforce at compile time
- **Risk**: Version overflow on long-running cells
- **Mitigation**: u64 version supports 584 billion years at 1 update/second

### User Stories
- **#180**: StateCell Core & State Structure (3 hours) 🔴 FOUNDATION
- **#181**: StateCell Read/Init/Delete Operations (3 hours)
- **#182**: StateCell CAS & Set Operations (4 hours)
- **#183**: StateCell Transition Closure Pattern (4 hours)
- **#184**: StateCellExt Transaction Extension (3 hours)

### Parallelization
After #180, stories #181-183 can run in parallel (3 Claudes)

---

## Epic 17: TraceStore Primitive (GitHub #163)

**Goal**: Structured reasoning traces with indexing

### Scope
- TraceStore struct as stateless facade
- Trace and TraceType structures (ToolCall, Decision, Query, Thought, Error, Custom)
- Record operations with ID generation
- Parent-child relationships for nested traces
- Secondary indices: by-type, by-tag, by-parent, by-time
- Query operations using indices
- Tree reconstruction

### Success Criteria
- [ ] TraceStore struct implemented with Arc<Database> reference
- [ ] TraceType enum with all variants (ToolCall, Decision, Query, Thought, Error, Custom)
- [ ] Trace struct with id, parent_id, trace_type, timestamp, tags, metadata
- [ ] `record()` stores trace with auto-generated ID
- [ ] `record_child()` stores trace with parent reference
- [ ] `record_with_options()` supports custom ID and tags
- [ ] `get()` retrieves trace by ID
- [ ] `query_by_type()` uses type index
- [ ] `query_by_tag()` uses tag index
- [ ] `query_by_time()` uses time index
- [ ] `get_children()` returns child traces
- [ ] `get_tree()` recursively builds TraceTree
- [ ] Secondary indices written atomically with trace
- [ ] Parent existence validated for child traces
- [ ] TraceStoreExt transaction extension trait
- [ ] All unit tests pass (>95% coverage)

### Dependencies
- Epic 13 (Primitives Foundation) complete

### Estimated Effort
1.5 days with 3 Claudes in parallel (after Story #84)

### Risks
- **Risk**: Index explosion with many traces
- **Mitigation**: Performance warning in docs, designed for tens-hundreds per run
- **Risk**: Orphaned indices on partial failure
- **Mitigation**: All index writes in same transaction as trace write

### User Stories
- **#185**: TraceStore Core & TraceType Structures (4 hours) 🔴 FOUNDATION
- **#186**: TraceStore Record Operations (4 hours)
- **#187**: TraceStore Secondary Indices (5 hours)
- **#188**: TraceStore Query Operations (4 hours)
- **#189**: TraceStore Tree Reconstruction (4 hours)
- **#190**: TraceStoreExt Transaction Extension (3 hours)

### Parallelization
After #185, stories #186-187 can run in parallel. After #187, stories #188-189 can run.

---

## Epic 18: RunIndex Primitive (GitHub #164)

**Goal**: First-class run lifecycle management

### Scope
- RunIndex struct as stateless facade
- RunMetadata and RunStatus structures
- Run lifecycle: create, get, update_status, complete, fail
- Status transition validation (no resurrection, archived is terminal)
- Query operations with filters
- Cascading delete and soft archive
- Secondary indices: by-status, by-tag, by-parent

### Success Criteria
- [ ] RunIndex struct implemented with Arc<Database> reference
- [ ] RunStatus enum: Active, Completed, Failed, Cancelled, Paused, Archived
- [ ] RunMetadata struct with all fields
- [ ] `create_run()` creates new run with Active status
- [ ] `create_run_with_options()` supports parent, tags, metadata
- [ ] `get_run()` retrieves run metadata
- [ ] `update_status()` validates transition and updates
- [ ] `complete_run()` transitions to Completed
- [ ] `fail_run()` transitions to Failed with error message
- [ ] `add_tags()` updates run tags
- [ ] `update_metadata()` updates custom metadata
- [ ] `query_runs()` supports status, tag, time, parent filters
- [ ] `list_runs()` returns all run IDs
- [ ] `get_child_runs()` returns forked runs
- [ ] `delete_run()` performs cascading hard delete
- [ ] `archive_run()` performs soft delete (status change)
- [ ] Status transition validation enforced (is_valid_transition)
- [ ] Secondary indices for efficient queries
- [ ] All unit tests pass (>95% coverage)

### Dependencies
- Epic 13 (Primitives Foundation) complete

### Estimated Effort
1.5 days with 3 Claudes in parallel (after Story #90)

### Risks
- **Risk**: Cascading delete misses some keys
- **Mitigation**: Comprehensive integration test with all primitive types
- **Risk**: Status transition bugs allow invalid states
- **Mitigation**: Explicit transition validation function with exhaustive match

### User Stories
- **#191**: RunIndex Core & RunMetadata Structures (4 hours) 🔴 FOUNDATION
- **#192**: RunIndex Create & Get Operations (4 hours)
- **#193**: RunIndex Status Update & Transition Validation (5 hours)
- **#194**: RunIndex Query Operations & Indices (4 hours)
- **#195**: RunIndex Delete & Archive Operations (5 hours)
- **#196**: RunIndex Integration with Other Primitives (4 hours)

### Parallelization
After #191, stories #192-194 can run in parallel (3 Claudes)

---

## Epic 19: Integration & Validation (GitHub #165)

**Goal**: Cross-primitive transactions and M3 completion validation

### Scope
- Cross-primitive transaction tests
- All extension traits working together
- Run isolation verification across all primitives
- Recovery tests (primitives survive crash + WAL replay)
- M3 completion checklist validation
- Performance benchmarks

### Success Criteria
- [ ] Cross-primitive transactions work (KV + Event + State + Trace atomic)
- [ ] Extension traits compose correctly in transactions
- [ ] Run isolation verified across all 5 primitives
- [ ] Recovery preserves all primitive data (events, traces, state)
- [ ] Hash chains valid after recovery
- [ ] Secondary indices consistent after recovery
- [ ] Status transitions preserved after recovery
- [ ] Performance: >10K single-primitive ops/sec
- [ ] Performance: >5K cross-primitive txn/sec
- [ ] All M3 success criteria verified
- [ ] Integration test coverage >90%

### Dependencies
- Epics 14-18 complete (all primitives implemented)

### Estimated Effort
1.5 days with 3 Claudes in parallel

### Risks
- **Risk**: Cross-primitive edge cases missed
- **Mitigation**: Systematic test matrix covering all combinations
- **Risk**: Performance regression from primitive overhead
- **Mitigation**: Benchmark and profile, primitives are thin wrappers

### User Stories
- **#197**: Cross-Primitive Transaction Tests (5 hours)
- **#198**: Run Isolation Integration Tests (4 hours)
- **#199**: Primitive Recovery Tests (5 hours)
- **#200**: Primitive Performance Benchmarks (4 hours)
- **#201**: M3 Completion Validation (3 hours)

### Parallelization
Stories #197-200 can run in parallel (3 Claudes). Story #201 runs last.

---

## Story Dependency Graph

```
Epic 13: Foundation (GitHub #159)
  #166 (crate setup) ──┬──> #167 (key helpers)
                       └──> #168 (extension traits)

Epic 14: KVStore (GitHub #160)
  #169 (core) ──┬──> #170 (single-op API)
                ├──> #171 (multi-op API)
                └──> #172 (list ops)
                     └──> #173 (extension)

Epic 15: EventLog (GitHub #161)
  #174 (core) ──> #175 (append/hash) ──┬──> #176 (read ops)
                                       ├──> #177 (verify chain)
                                       └──> #178 (query by type)
                                            └──> #179 (extension)

Epic 16: StateCell (GitHub #162)
  #180 (core) ──┬──> #181 (read/init/delete)
                ├──> #182 (CAS/set)
                └──> #183 (transition)
                     └──> #184 (extension)

Epic 17: TraceStore (GitHub #163)
  #185 (core) ──┬──> #186 (record ops)
                └──> #187 (indices) ──┬──> #188 (query ops)
                                      └──> #189 (tree)
                                           └──> #190 (extension)

Epic 18: RunIndex (GitHub #164)
  #191 (core) ──┬──> #192 (create/get)
                ├──> #193 (status transitions)
                └──> #194 (query ops)
                     └──> #195 (delete/archive)
                          └──> #196 (integration)

Epic 19: Integration (GitHub #165)
  All Epics ──> #197, #198, #199, #200 (parallel) ──> #201 (validation)
```

---

## Parallelization Strategy

### Phase 1: Foundation (Day 1)
- **Claude 1**: Story #166 (crate setup)
- After #166: Claude 1 → #167, Claude 2 → #168

### Phase 2: Primitives (Days 2-4)
After Epic 13 complete, all primitive epics can start in parallel:

- **Claude 1**: Epic 14 (KVStore) - #169 → #170, #171, #172 → #173
- **Claude 2**: Epic 15 (EventLog) - #174 → #175 → #176, #177, #178 → #179
- **Claude 3**: Epic 16 (StateCell) - #180 → #181, #182, #183 → #184
- **Claude 4**: Epic 17 (TraceStore) - #185 → #186, #187 → #188, #189 → #190
- **Claude 5**: Epic 18 (RunIndex) - #191 → #192, #193, #194 → #195 → #196

### Phase 3: Integration (Day 5)
- **Claude 1**: Story #197 (cross-primitive tests)
- **Claude 2**: Story #198 (run isolation tests)
- **Claude 3**: Story #199 (recovery tests)
- **All**: Story #200 (benchmarks)
- **All**: Story #201 (completion validation)

---

## Story Count Summary

| Epic | Stories | Effort |
|------|---------|--------|
| Epic 13: Foundation | 3 | 10 hours |
| Epic 14: KVStore | 5 | 16 hours |
| Epic 15: EventLog | 6 | 23 hours |
| Epic 16: StateCell | 5 | 17 hours |
| Epic 17: TraceStore | 6 | 24 hours |
| Epic 18: RunIndex | 6 | 26 hours |
| Epic 19: Integration | 5 | 21 hours |
| **Total** | **36** | **137 hours** |

With 5 Claudes working in parallel: ~28 hours elapsed = ~4 working days

---

## Success Criteria (M3 Complete)

- [ ] All 5 primitives implemented and tested
- [ ] KV store: get, put, delete, list working
- [ ] Event log: append, read, chain verification working
- [ ] StateCell: read, CAS, transitions working
- [ ] Trace store: record, query by type/tag/time working
- [ ] Run Index: create, update, query, lifecycle working
- [ ] All primitives enforce their invariants
- [ ] Cross-primitive transactions work atomically
- [ ] Run isolation verified across all primitives
- [ ] Recovery preserves all primitive data
- [ ] Status transition validation enforced
- [ ] Integration test coverage >90%
- [ ] Performance >10K ops/sec

---

**Document Version**: 1.0
**Created**: 2026-01-14
**Status**: Planning
