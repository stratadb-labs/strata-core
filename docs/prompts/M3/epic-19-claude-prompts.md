# Epic 19: Integration & Validation - Claude Prompts

**Copy the M3_PROMPT_HEADER.md content to the top of this file before using any prompt.**

---

## Epic Overview

| Field | Value |
|-------|-------|
| Epic | 19 - Integration & Validation |
| GitHub Issue | [#165](https://github.com/anibjoshi/in-mem/issues/165) |
| Stories | #197-#201 (5 stories) |
| Goal | Cross-primitive transactions and M3 completion validation |
| Dependencies | Epics 14-18 (all primitives) complete |
| Branch | `epic-19-integration-validation` |

**Deliverables**:
- Cross-primitive transaction tests
- Run isolation verification
- Recovery tests
- Performance benchmarks
- M3 completion report

---

## Story Dependency Graph

```
All Primitives Complete (Epics 14-18)
                ↓
    +-----------+-----------+
    |           |           |
Story #197  Story #198  Story #199
(Cross-     (Run        (Recovery
Primitive)  Isolation)   Tests)
    |           |           |
    +-----------+-----------+
                ↓
           Story #200
         (Benchmarks)
                ↓
           Story #201
         (Completion)
```

---

## Story #197: Cross-Primitive Transaction Tests

### Metadata
| Field | Value |
|-------|-------|
| Story | #197 |
| Branch | `epic-19-story-197-cross-primitive-tests` |
| File | `crates/primitives/tests/cross_primitive_tests.rs` |
| Depends On | Epics 14-18 complete |

### Prompt

```
## Context

You are implementing Story #197 for the in-mem agent database M3 milestone.

**CRITICAL**: Read docs/architecture/M3_ARCHITECTURE.md Section 10 (Transaction Integration) before starting.

## Your Task

Create comprehensive tests for cross-primitive atomic transactions.

## File to Create

`crates/primitives/tests/cross_primitive_tests.rs`

## Tests to Implement

### 1. test_kv_event_state_atomic
```rust
#[test]
fn test_kv_event_state_atomic() {
    // Setup database and primitives
    // Use db.transaction() to atomically:
    //   1. txn.kv_put("task/status", "running")
    //   2. txn.event_append("task_started", payload)
    //   3. txn.state_cas("workflow", 0, "step1")
    //   4. txn.trace_record(TraceType::Thought {...})
    // Verify ALL operations succeeded together
}
```

### 2. test_cross_primitive_rollback
```rust
#[test]
fn test_cross_primitive_rollback() {
    // Setup: create StateCell with version 1
    // Transaction:
    //   1. KV put (should succeed alone)
    //   2. StateCell CAS with WRONG version (should fail)
    // Verify: KV was NOT written (rollback affected all)
}
```

### 3. test_all_extension_traits_compose
```rust
#[test]
fn test_all_extension_traits_compose() {
    // Use all 4 extension traits in single transaction:
    //   - KVStoreExt::kv_put()
    //   - EventLogExt::event_append()
    //   - StateCellExt::state_set()
    //   - TraceStoreExt::trace_record()
    // All must work together
}
```

### 4. test_partial_failure_full_rollback
```rust
#[test]
fn test_partial_failure_full_rollback() {
    // Write to 3 primitives successfully
    // Fail on 4th primitive operation
    // Verify ALL 4 operations rolled back
}
```

### 5. test_nested_primitive_operations
```rust
#[test]
fn test_nested_primitive_operations() {
    // Within single transaction:
    //   - Read KV -> use value in Event payload
    //   - Append Event -> use sequence in StateCell
    //   - Read StateCell -> record in Trace
    // Verify causal chain works
}
```

## Extension Trait Imports

```rust
use primitives::{
    KVStoreExt,
    EventLogExt,
    StateCellExt,
    TraceStoreExt,
};
```

## Acceptance Criteria

- [ ] All 4 primitives work in single transaction
- [ ] Rollback affects all primitives atomically
- [ ] Extension traits compose correctly
- [ ] Partial failures cause full rollback
- [ ] Tests pass: `cargo test --package primitives cross_primitive`

## Validation Commands

```bash
# Run cross-primitive tests
~/.cargo/bin/cargo test --package primitives cross_primitive -- --nocapture

# Verify no warnings
~/.cargo/bin/cargo clippy --package primitives -- -D warnings
```

## Reference

- M3_ARCHITECTURE.md Section 10: Transaction Integration
- Extension traits in primitives/src/extensions.rs
```

---

## Story #198: Run Isolation Integration Tests

### Metadata
| Field | Value |
|-------|-------|
| Story | #198 |
| Branch | `epic-19-story-198-run-isolation-tests` |
| File | `crates/primitives/tests/run_isolation_tests.rs` |
| Depends On | Epics 14-18 complete |

### Prompt

```
## Context

You are implementing Story #198 for the in-mem agent database M3 milestone.

**CRITICAL**: Read docs/architecture/M3_ARCHITECTURE.md Section 2 (Run Isolation - Key Prefix Isolation) before starting.

## Your Task

Create comprehensive tests verifying run isolation across ALL primitives.

## File to Create

`crates/primitives/tests/run_isolation_tests.rs`

## Tests to Implement

### 1. test_kv_isolation
```rust
#[test]
fn test_kv_isolation() {
    let run1 = RunId::new();
    let run2 = RunId::new();

    kv.put(run1, "key", Value::I64(1))?;
    kv.put(run2, "key", Value::I64(2))?;

    // Each run sees ONLY its own data
    assert_eq!(kv.get(run1, "key")?, Some(Value::I64(1)));
    assert_eq!(kv.get(run2, "key")?, Some(Value::I64(2)));

    // List shows only own keys
    assert!(!kv.list(run1, None)?.iter().any(|k| /* belongs to run2 */));
}
```

### 2. test_event_log_isolation
```rust
#[test]
fn test_event_log_isolation() {
    let run1 = RunId::new();
    let run2 = RunId::new();

    // Both runs start at sequence 1
    let (seq1, _) = event_log.append(run1, "event", payload)?;
    let (seq2, _) = event_log.append(run2, "event", payload)?;

    assert_eq!(seq1, 1); // Independent sequence
    assert_eq!(seq2, 1); // Also 1, not 2

    // Chain verification is per-run
    assert!(event_log.verify_chain(run1)?.is_valid);
    assert!(event_log.verify_chain(run2)?.is_valid);
}
```

### 3. test_state_cell_isolation
```rust
#[test]
fn test_state_cell_isolation() {
    let run1 = RunId::new();
    let run2 = RunId::new();

    // Same cell name, different runs
    state_cell.init(run1, "counter", Value::I64(0))?;
    state_cell.init(run2, "counter", Value::I64(100))?;

    assert_eq!(state_cell.read(run1, "counter")?.unwrap().value, Value::I64(0));
    assert_eq!(state_cell.read(run2, "counter")?.unwrap().value, Value::I64(100));
}
```

### 4. test_trace_store_isolation
```rust
#[test]
fn test_trace_store_isolation() {
    let run1 = RunId::new();
    let run2 = RunId::new();

    trace_store.record(run1, TraceType::Thought { content: "run1".into(), confidence: None })?;
    trace_store.record(run2, TraceType::Thought { content: "run2".into(), confidence: None })?;

    // Queries respect run boundaries
    let run1_traces = trace_store.query_by_type(run1, "Thought")?;
    assert!(run1_traces.iter().all(|t| /* content contains "run1" */));

    let run2_traces = trace_store.query_by_type(run2, "Thought")?;
    assert!(run2_traces.iter().all(|t| /* content contains "run2" */));
}
```

### 5. test_cross_run_query_isolation
```rust
#[test]
fn test_cross_run_query_isolation() {
    // Create data in run1
    // Create data in run2

    // Query in run1 context NEVER returns run2 data
    // Query in run2 context NEVER returns run1 data
}
```

### 6. test_run_delete_isolation
```rust
#[test]
fn test_run_delete_isolation() {
    let run1 = RunId::new();
    let run2 = RunId::new();

    // Write to both runs
    kv.put(run1, "key", value)?;
    kv.put(run2, "key", value)?;

    // Delete run1
    run_index.delete_run(run1)?;

    // run2 data is UNTOUCHED
    assert!(kv.get(run2, "key")?.is_some());
}
```

## Isolation Guarantee

From M3_ARCHITECTURE.md Section 2.1:
> Run Isolation (Key Prefix Isolation)
> - Each primitive operation is scoped to a RunId
> - Data from different runs never mixes
> - Key prefixing ensures namespace isolation

## Acceptance Criteria

- [ ] KV isolation verified (same key, different runs)
- [ ] EventLog isolation verified (independent sequences)
- [ ] StateCell isolation verified (independent versions)
- [ ] TraceStore isolation verified (queries respect boundaries)
- [ ] Run deletion only affects target run

## Validation Commands

```bash
~/.cargo/bin/cargo test --package primitives run_isolation -- --nocapture
~/.cargo/bin/cargo clippy --package primitives -- -D warnings
```
```

---

## Story #199: Primitive Recovery Tests

### Metadata
| Field | Value |
|-------|-------|
| Story | #199 |
| Branch | `epic-19-story-199-recovery-tests` |
| File | `crates/primitives/tests/recovery_tests.rs` |
| Depends On | Epics 14-18 complete |

### Prompt

```
## Context

You are implementing Story #199 for the in-mem agent database M3 milestone.

**CRITICAL**: Read docs/architecture/M3_ARCHITECTURE.md Section 11 (Failure Model and Recovery) before starting.

## Your Task

Create comprehensive tests verifying ALL primitives survive crash + WAL replay.

## File to Create

`crates/primitives/tests/recovery_tests.rs`

## Recovery Contract (from M3_ARCHITECTURE.md)

> WAL replay reconstructs all state including indices.
> After crash + WAL replay:
> - Sequence numbers: Preserved
> - Secondary indices: Replayed, not rebuilt
> - Derived keys (hashes): Stored, not recomputed

## Tests to Implement

### 1. test_kv_survives_recovery
```rust
#[test]
fn test_kv_survives_recovery() {
    let path = temp_dir();
    let db = Database::open(&path)?;
    let kv = KVStore::new(db.clone());
    let run_id = create_run(&db);

    kv.put(run_id, "key1", Value::String("value1".into()))?;
    kv.put(run_id, "key2", Value::I64(42))?;

    // Simulate crash
    drop(kv);
    drop(db);

    // Recovery
    let db = Database::open(&path)?;
    let kv = KVStore::new(db.clone());

    // Data survived
    assert_eq!(kv.get(run_id, "key1")?, Some(Value::String("value1".into())));
    assert_eq!(kv.get(run_id, "key2")?, Some(Value::I64(42)));
}
```

### 2. test_event_log_chain_survives_recovery
```rust
#[test]
fn test_event_log_chain_survives_recovery() {
    // Append multiple events
    event_log.append(run_id, "event1", payload1)?;
    event_log.append(run_id, "event2", payload2)?;
    let (seq3, hash3) = event_log.append(run_id, "event3", payload3)?;

    // Crash + recovery
    drop(db);
    let db = Database::open(path)?;
    let event_log = EventLog::new(db.clone());

    // Chain is intact
    assert!(event_log.verify_chain(run_id)?.is_valid);
    assert_eq!(event_log.len(run_id)?, 3);

    // Sequence continues correctly
    let (seq4, _) = event_log.append(run_id, "event4", payload4)?;
    assert_eq!(seq4, 4);  // Not 1 (restarted)

    // Hashes preserved
    let event3 = event_log.read(run_id, seq3)?.unwrap();
    assert_eq!(event3.hash, hash3);
}
```

### 3. test_state_cell_version_survives_recovery
```rust
#[test]
fn test_state_cell_version_survives_recovery() {
    state_cell.init(run_id, "cell", Value::I64(0))?;
    state_cell.cas(run_id, "cell", 1, Value::I64(1))?;
    state_cell.cas(run_id, "cell", 2, Value::I64(2))?;

    // Crash + recovery
    let state_cell = recover_state_cell(path);

    // Version is correct (3, not 1)
    let state = state_cell.read(run_id, "cell")?.unwrap();
    assert_eq!(state.version, 3);
    assert_eq!(state.value, Value::I64(2));

    // CAS works with correct version
    state_cell.cas(run_id, "cell", 3, Value::I64(3))?; // Should succeed
}
```

### 4. test_trace_indices_survive_recovery
```rust
#[test]
fn test_trace_indices_survive_recovery() {
    let id1 = trace_store.record(run_id, TraceType::ToolCall { ... })?;
    let id2 = trace_store.record(run_id, TraceType::Decision { ... })?;
    trace_store.record_child(run_id, &id1, TraceType::Thought { ... })?;

    // Crash + recovery
    let trace_store = recover_trace_store(path);

    // Primary data accessible
    assert!(trace_store.get(run_id, &id1)?.is_some());

    // Indices work (query_by_type uses index)
    let tool_calls = trace_store.query_by_type(run_id, "ToolCall")?;
    assert_eq!(tool_calls.len(), 1);

    // Parent-child index works
    let children = trace_store.get_children(run_id, &id1)?;
    assert_eq!(children.len(), 1);
}
```

### 5. test_run_status_survives_recovery
```rust
#[test]
fn test_run_status_survives_recovery() {
    let run_meta = run_index.create_run(&ns)?;
    run_index.update_status(run_meta.run_id, RunStatus::Completed)?;
    run_index.add_tags(run_meta.run_id, &["important".into()])?;

    // Crash + recovery
    let run_index = recover_run_index(path);

    // Status preserved
    let recovered = run_index.get_run(run_meta.run_id)?.unwrap();
    assert_eq!(recovered.status, RunStatus::Completed);
    assert!(recovered.tags.contains(&"important".into()));

    // Indices work
    let completed = run_index.query_runs(RunQuery {
        status: Some(RunStatus::Completed),
        ..Default::default()
    })?;
    assert!(completed.iter().any(|r| r.run_id == run_meta.run_id));
}
```

### 6. test_uncommitted_transaction_lost_on_recovery
```rust
#[test]
fn test_uncommitted_transaction_lost_on_recovery() {
    kv.put(run_id, "committed", Value::I64(1))?;

    // Start but don't commit
    let txn = db.begin_transaction(run_id)?;
    txn.put(Key::new_kv(...), Value::I64(999))?;
    // Crash without commit
    drop(txn);
    drop(db);

    // Recovery
    let db = Database::open(path)?;

    // Committed survives
    assert!(kv.get(run_id, "committed")?.is_some());
    // Uncommitted lost
    assert!(kv.get(run_id, "uncommitted")?.is_none());
}
```

## Acceptance Criteria

- [ ] KV data preserved after recovery
- [ ] EventLog chain valid after recovery (verify_chain passes)
- [ ] EventLog sequences continue correctly (no restart)
- [ ] StateCell versions correct after recovery
- [ ] TraceStore indices functional after recovery
- [ ] RunIndex status and tags preserved after recovery
- [ ] Uncommitted transactions are lost (correct behavior)

## Validation Commands

```bash
~/.cargo/bin/cargo test --package primitives recovery -- --nocapture
~/.cargo/bin/cargo clippy --package primitives -- -D warnings
```
```

---

## Story #200: Primitive Performance Benchmarks

### Metadata
| Field | Value |
|-------|-------|
| Story | #200 |
| Branch | `epic-19-story-200-performance-benchmarks` |
| File | `crates/primitives/benches/primitive_benchmarks.rs` |
| Depends On | Epics 14-18 complete, Cargo.toml benchmark config |

### Prompt

```
## Context

You are implementing Story #200 for the in-mem agent database M3 milestone.

**CRITICAL**: Read docs/architecture/M3_ARCHITECTURE.md Section 14 (Performance Characteristics) for targets.

## Your Task

Create comprehensive benchmarks for all primitive operations.

## Performance Targets (from M3_ARCHITECTURE.md)

| Operation | Target |
|-----------|--------|
| KV put | >10K ops/sec |
| KV get | >20K ops/sec |
| EventLog append | >5K ops/sec |
| StateCell CAS | >5K ops/sec |
| TraceStore record | >2K ops/sec (index overhead) |
| Cross-primitive txn | >1K ops/sec |

## File to Create

`crates/primitives/benches/primitive_benchmarks.rs`

## Setup

Ensure Cargo.toml has:
```toml
[[bench]]
name = "primitive_benchmarks"
harness = false

[dev-dependencies]
criterion = "0.5"
```

## Benchmarks to Implement

```rust
use criterion::{criterion_group, criterion_main, Criterion, Throughput};

fn bench_kv_put(c: &mut Criterion) {
    let db = setup_db();
    let kv = KVStore::new(db.clone());
    let run_id = create_run(&db);

    let mut group = c.benchmark_group("kv");
    group.throughput(Throughput::Elements(1));

    let mut i = 0;
    group.bench_function("put", |b| {
        b.iter(|| {
            i += 1;
            kv.put(run_id, &format!("key{}", i), Value::I64(i as i64))
        })
    });
    group.finish();
}

fn bench_kv_get(c: &mut Criterion) {
    let db = setup_db();
    let kv = KVStore::new(db.clone());
    let run_id = create_run(&db);

    // Pre-populate keys
    for i in 0..1000 {
        kv.put(run_id, &format!("key{}", i), Value::I64(i))?;
    }

    let mut group = c.benchmark_group("kv");
    group.throughput(Throughput::Elements(1));

    let mut i = 0;
    group.bench_function("get", |b| {
        b.iter(|| {
            i = (i + 1) % 1000;
            kv.get(run_id, &format!("key{}", i))
        })
    });
    group.finish();
}

fn bench_event_append(c: &mut Criterion) {
    let db = setup_db();
    let event_log = EventLog::new(db.clone());
    let run_id = create_run(&db);

    let mut group = c.benchmark_group("event_log");
    group.throughput(Throughput::Elements(1));

    group.bench_function("append", |b| {
        b.iter(|| {
            event_log.append(run_id, "benchmark_event", json!({"data": "test"}))
        })
    });
    group.finish();
}

fn bench_state_cas(c: &mut Criterion) {
    let db = setup_db();
    let state_cell = StateCell::new(db.clone());
    let run_id = create_run(&db);

    state_cell.init(run_id, "bench_cell", Value::I64(0))?;

    let mut group = c.benchmark_group("state_cell");
    group.throughput(Throughput::Elements(1));

    // CAS increments value
    group.bench_function("cas", |b| {
        b.iter(|| {
            state_cell.transition(run_id, "bench_cell", |state| {
                let val = state.value.as_i64().unwrap_or(0);
                Ok((Value::I64(val + 1), val + 1))
            })
        })
    });
    group.finish();
}

fn bench_trace_record(c: &mut Criterion) {
    let db = setup_db();
    let trace_store = TraceStore::new(db.clone());
    let run_id = create_run(&db);

    let mut group = c.benchmark_group("trace_store");
    group.throughput(Throughput::Elements(1));

    group.bench_function("record", |b| {
        b.iter(|| {
            trace_store.record(run_id, TraceType::Thought {
                content: "benchmark thought".into(),
                confidence: Some(0.95),
            })
        })
    });
    group.finish();
}

fn bench_cross_primitive_transaction(c: &mut Criterion) {
    let db = setup_db();
    let run_id = create_run(&db);

    // Initialize state cell
    StateCell::new(db.clone()).init(run_id, "txn_cell", Value::I64(0))?;

    let mut group = c.benchmark_group("cross_primitive");
    group.throughput(Throughput::Elements(1));

    let mut counter = 0;
    group.bench_function("4_primitive_txn", |b| {
        b.iter(|| {
            counter += 1;
            db.transaction(run_id, |txn| {
                txn.kv_put(&format!("key{}", counter), Value::I64(counter))?;
                txn.event_append("txn_event", json!({"n": counter}))?;
                txn.state_set("txn_cell", Value::I64(counter))?;
                txn.trace_record(TraceType::Thought {
                    content: format!("txn {}", counter),
                    confidence: None,
                })?;
                Ok(())
            })
        })
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_kv_put,
    bench_kv_get,
    bench_event_append,
    bench_state_cas,
    bench_trace_record,
    bench_cross_primitive_transaction,
);
criterion_main!(benches);
```

## Acceptance Criteria

- [ ] KV put benchmark: >10K ops/sec
- [ ] KV get benchmark: >20K ops/sec
- [ ] EventLog append benchmark: >5K ops/sec
- [ ] StateCell CAS benchmark: >5K ops/sec
- [ ] TraceStore record benchmark: >2K ops/sec
- [ ] Cross-primitive transaction benchmark: >1K ops/sec
- [ ] Benchmarks are reproducible

## Validation Commands

```bash
# Run all benchmarks
~/.cargo/bin/cargo bench --package primitives

# Run specific benchmark
~/.cargo/bin/cargo bench --package primitives -- kv/put

# Generate HTML report
~/.cargo/bin/cargo bench --package primitives -- --save-baseline m3_complete
```

## Output Format

Document results in this format:
```
## M3 Benchmark Results

| Operation | Target | Actual | Status |
|-----------|--------|--------|--------|
| KV put | >10K/s | XXX/s | PASS/FAIL |
| KV get | >20K/s | XXX/s | PASS/FAIL |
| EventLog append | >5K/s | XXX/s | PASS/FAIL |
| StateCell CAS | >5K/s | XXX/s | PASS/FAIL |
| TraceStore record | >2K/s | XXX/s | PASS/FAIL |
| Cross-primitive txn | >1K/s | XXX/s | PASS/FAIL |
```
```

---

## Story #201: M3 Completion Validation

### Metadata
| Field | Value |
|-------|-------|
| Story | #201 |
| Branch | `epic-19-story-201-completion-validation` |
| File | `docs/milestones/M3_COMPLETION_REPORT.md` |
| Depends On | All M3 stories complete (#166-#200) |

### Prompt

```
## Context

You are implementing Story #201 - the final story of M3 milestone.

## Your Task

Create comprehensive M3 completion validation and report.

## File to Create

`docs/milestones/M3_COMPLETION_REPORT.md`

## Verification Steps

### 1. Epic/Story Status Verification

Run:
```bash
# Check all stories are closed
/opt/homebrew/bin/gh issue list --state closed --label "M3" | wc -l
# Should be 36

# Check all PRs merged
/opt/homebrew/bin/gh pr list --state merged --label "M3" | wc -l
```

### 2. Test Coverage Verification

Run:
```bash
# All unit tests pass
~/.cargo/bin/cargo test --workspace

# Specific primitive tests
~/.cargo/bin/cargo test --package primitives

# Integration tests
~/.cargo/bin/cargo test --package primitives --test cross_primitive_tests
~/.cargo/bin/cargo test --package primitives --test run_isolation_tests
~/.cargo/bin/cargo test --package primitives --test recovery_tests
```

### 3. Benchmark Verification

Run:
```bash
~/.cargo/bin/cargo bench --package primitives
```

Verify targets met:
| Operation | Target | Actual |
|-----------|--------|--------|
| KV put | >10K/s | ___ |
| KV get | >20K/s | ___ |
| EventLog append | >5K/s | ___ |
| StateCell CAS | >5K/s | ___ |
| TraceStore record | >2K/s | ___ |
| Cross-primitive txn | >1K/s | ___ |

### 4. Documentation Verification

Check these files exist and are complete:
- [ ] `docs/architecture/M3_ARCHITECTURE.md`
- [ ] `docs/milestones/M3_EPICS.md`
- [ ] `docs/milestones/M3_IMPLEMENTATION_PLAN.md`
- [ ] `docs/milestones/M3_PROJECT_STATUS.md`

## Report Template

```markdown
# M3 Completion Report

**Date**: YYYY-MM-DD
**Status**: COMPLETE / INCOMPLETE

## Executive Summary

M3 implemented 5 high-level primitives over the M1-M2 transactional engine:
- KVStore: General-purpose key-value storage
- EventLog: Append-only events with causal hash chaining
- StateCell: CAS-based versioned cells
- TraceStore: Structured reasoning traces
- RunIndex: First-class run lifecycle management

## Epic/Story Summary

| Epic | Stories | Status |
|------|---------|--------|
| Epic 13: Foundation | #166-#168 | COMPLETE |
| Epic 14: KVStore | #169-#173 | COMPLETE |
| Epic 15: EventLog | #174-#179 | COMPLETE |
| Epic 16: StateCell | #180-#184 | COMPLETE |
| Epic 17: TraceStore | #185-#190 | COMPLETE |
| Epic 18: RunIndex | #191-#196 | COMPLETE |
| Epic 19: Integration | #197-#201 | COMPLETE |

**Total**: 36 stories delivered

## Test Results

### Unit Tests
```
cargo test --workspace
[PASTE OUTPUT]
```

### Integration Tests
- Cross-primitive transactions: PASS
- Run isolation: PASS
- Recovery: PASS

## Benchmark Results

| Operation | Target | Actual | Status |
|-----------|--------|--------|--------|
| KV put | >10K/s | ___/s | PASS/FAIL |
| KV get | >20K/s | ___/s | PASS/FAIL |
| EventLog append | >5K/s | ___/s | PASS/FAIL |
| StateCell CAS | >5K/s | ___/s | PASS/FAIL |
| TraceStore record | >2K/s | ___/s | PASS/FAIL |
| Cross-primitive txn | >1K/s | ___/s | PASS/FAIL |

## Architecture Compliance

- [x] Primitives are stateless facades (hold only Arc<Database>)
- [x] All operations scoped to RunId
- [x] TypeTag separation enforced
- [x] Transaction integration via extension traits
- [x] Invariants enforced by primitives

## Known Limitations (Documented)

Per M3_ARCHITECTURE.md Section 16:
- Non-cryptographic hash in EventLog (upgrade path: SHA-256 in M4+)
- Linear trace queries (upgrade path: B-tree indices in M4+)
- No vector search (M6)
- No run forking (M9)

## Lessons Learned

1. [Document any implementation insights]
2. [Document any spec clarifications needed]
3. [Document any performance discoveries]

## M4 Preparation Notes

M4 scope (from M3_ARCHITECTURE.md Section 17.1):
- Snapshot + WAL rotation
- Point-in-time recovery
- Configurable retention

Recommended prep:
1. [Any tech debt to address]
2. [Any interface changes needed]
3. [Any documentation updates]

---

**Sign-off**: M3 is COMPLETE and ready for M4.
```

## Acceptance Criteria

- [ ] All 7 epics marked complete
- [ ] All 36 stories delivered and closed
- [ ] All unit tests pass (0 failures)
- [ ] All integration tests pass
- [ ] All benchmarks meet targets
- [ ] Documentation complete
- [ ] Completion report created

## Validation Commands

```bash
# Final validation suite
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace -- -D warnings
~/.cargo/bin/cargo fmt --check
~/.cargo/bin/cargo bench --package primitives
```
```

---

## Epic Completion Checklist

After all stories are complete:

```bash
# 1. Verify all stories merged
/opt/homebrew/bin/gh pr list --state merged --label "epic-19"

# 2. Run full test suite
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace -- -D warnings

# 3. Run benchmarks
~/.cargo/bin/cargo bench --package primitives

# 4. Merge epic branch to develop
git checkout develop
git merge --no-ff epic-19-integration-validation -m "Epic 19: Integration & Validation complete"
git push origin develop

# 5. Close epic issue
/opt/homebrew/bin/gh issue close 165 --comment "Epic 19 complete. All 5 stories delivered."
```

---

## M3 Milestone Completion

After Epic 19 is complete, M3 is done:

```bash
# 1. Verify all M3 epics complete
/opt/homebrew/bin/gh issue list --state closed --label "M3" --json number,title

# 2. Merge develop to main
git checkout main
git merge --no-ff develop -m "M3: Primitives milestone complete

Delivered:
- KVStore primitive
- EventLog primitive
- StateCell primitive
- TraceStore primitive
- RunIndex primitive

36 stories across 7 epics."

git push origin main

# 3. Tag release
git tag -a v0.3.0 -m "M3: Primitives"
git push origin v0.3.0
```

---

*End of Epic 19 Claude Prompts*
