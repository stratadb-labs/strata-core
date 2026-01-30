# Ignored Tests Registry

**Total: 64 ignored tests across 6 suites**

This document tracks every `#[ignore]` test so none are forgotten. Tests fall into
three categories: **API gaps** (need new methods), **known bugs** (need fixes), and
**stress tests** (opt-in, slow).

---

## API Gap Tests (29 tests)

These are architectural specifications disguised as tests. Each one validates a
principle the database should support. When you implement the missing method,
remove the `#[ignore]` and the test is ready.

### RunIndex Lifecycle — 14 tests

**File:** `tests/engine/primitives/runindex.rs`
**Requires:** `RunIndex::complete_run`, `fail_run`, `cancel_run`, `pause_run`,
`resume_run`, `archive_run`, `add_tags`, `remove_tags`, `update_metadata`,
`query_by_status`, `query_by_tag`, `RunStatus::is_terminal`, `is_finished`,
`can_transition_to`

| Test | What It Validates |
|------|-------------------|
| `complete_run` | Mark a run as successfully completed |
| `fail_run` | Mark a run as failed |
| `cancel_run` | Cancel an in-progress run |
| `pause_and_resume_run` | Suspend and resume execution |
| `archive_completed_run` | Archive finished runs for cold storage |
| `terminal_states_cannot_transition_to_active` | State machine: completed/failed/cancelled are final |
| `add_tags` | Attach string tags to runs for organization |
| `remove_tags` | Remove previously added tags |
| `query_by_tag` | Find runs matching specific tags |
| `update_metadata` | Update run metadata after creation |
| `query_by_status` | Filter runs by current status |
| `status_is_terminal_check` | `RunStatus::is_terminal()` returns true for completed/failed/cancelled |
| `status_is_finished_check` | `RunStatus::is_finished()` returns true for completed/archived |
| `status_can_transition_to` | Valid state transitions enforced (e.g., Active->Paused ok, Completed->Active rejected) |

**Why this matters:** Agent runs need lifecycle management. Without status
transitions, there's no way to mark runs as done, failed, or archived. Without
tags, there's no way to organize and query runs.

### StateCell — 6 tests

**File:** `tests/engine/primitives/statecell.rs`

| Test | Requires | What It Validates |
|------|----------|-------------------|
| `delete_removes_cell` | `StateCell::delete` | Remove a state cell entirely |
| `delete_nonexistent_returns_false` | `StateCell::delete` | Delete of missing cell returns false (not error) |
| `transition_reads_transforms_writes` | `StateCell::transition` | Atomic read-modify-write: read current, apply function, write result |
| `transition_or_init_initializes_if_missing` | `StateCell::transition_or_init` | Atomic init-or-transform for idempotent state setup |
| `list_returns_all_cells` | `StateCell::list` | Enumerate all cells in a run |
| `list_empty_run_returns_empty` | `StateCell::list` | Empty run returns empty list |

**Why this matters:** `transition()` is the fundamental state machine primitive.
Without it, every read-modify-write requires manual `read()` + `cas()` which is
error-prone and not atomic under contention. `delete()` is needed for cleanup.

### JsonStore — 5 tests

**File:** `tests/engine/primitives/jsonstore.rs`

| Test | Requires | What It Validates |
|------|----------|-------------------|
| `merge_adds_new_fields` | `JsonStore::merge` | RFC 7396 JSON Merge Patch: add fields to document |
| `merge_overwrites_existing_fields` | `JsonStore::merge` | Merge replaces existing fields |
| `merge_null_removes_field` | `JsonStore::merge` | Merge with null value deletes the field |
| `cas_succeeds_with_correct_version` | `JsonStore::cas` | Optimistic concurrency on JSON docs |
| `cas_fails_with_wrong_version` | `JsonStore::cas` | Stale version rejected |

**Why this matters:** `merge()` enables partial document updates without
read-modify-write cycles. `cas()` prevents lost updates when multiple writers
modify the same document.

### EventLog — 3 tests

**File:** `tests/engine/primitives/eventlog.rs`

| Test | Requires | What It Validates |
|------|----------|-------------------|
| `verify_chain_valid_for_empty_log` | `EventLog::verify_chain` | Empty event log has valid (trivial) hash chain |
| `verify_chain_valid_after_appends` | `EventLog::verify_chain` | Hash chain integrity after writes — detects tampering/corruption |
| `append_batch_returns_sequence_numbers` | `EventLog::append_batch` | Atomic multi-event append with sequential IDs |

**Why this matters:** `verify_chain()` is the audit trail guarantee — it proves
no events were inserted, deleted, or reordered after the fact. `append_batch()`
ensures related events are written atomically.

### Run Forking — 1 test

**File:** `tests/integration/branching.rs`

| Test | Requires | What It Validates |
|------|----------|-------------------|
| `child_run_should_inherit_parent_data` | `RunCreateChild` (issue #780) | Fork a run: child starts with a copy of parent's data |

**Why this matters:** Run forking enables branching agent execution — try
different strategies from the same starting state without re-running setup.

---

## Known Bug Tests (1 test)

### Run Deletion Data Cleanup

**File:** `tests/executor/run_invariants.rs`

| Test | Bug | What It Validates |
|------|-----|-------------------|
| `run_delete_removes_all_data` | Issue #781 | Deleting a run should cascade-delete all KV, State, and Event data |

**Current behavior:** `RunDelete` removes the run metadata but data persists.
Recreating a run with the same name sees stale data. See
`run_delete_currently_does_not_remove_data` (not ignored) which documents this.

---

## Implementation-Pending Tests (2 tests)

### Session Transaction Gaps

**File:** `tests/executor/session_transactions.rs`

| Test | Issue | What It Validates |
|------|-------|-------------------|
| `read_your_writes_event` | EventAppend in transactions pending review | Events written in a transaction are readable before commit |
| `cross_primitive_transaction` | State/Event serialization issues | Single transaction spans KV + State + Event atomically |

---

## Stress Tests (31 tests)

Run manually: `cargo test --test <suite> stress -- --ignored`

These are slow, high-workload tests that validate correctness under contention.
They pass but are skipped in CI to keep builds fast.

### Concurrency Stress (6 tests)

**File:** `tests/concurrency/stress.rs`
**Run:** `cargo test --test concurrency stress -- --ignored`

| Test | Workload |
|------|----------|
| `stress_concurrent_read_write` | 16 threads mixed read-write on shared keys |
| `stress_transaction_throughput` | Rapid transaction commit rate measurement |
| `stress_large_transaction` | 10K operations in a single transaction |
| `stress_many_runs` | 100 concurrent runs with transactions |
| `stress_long_running_transaction` | Long transaction with concurrent modifications |
| `stress_sustained_workload` | Sustained mixed workload over extended period |

### Durability Stress (9 tests)

**File:** `tests/durability/stress.rs`
**Run:** `cargo test --test durability stress -- --ignored`

| Test | Workload |
|------|----------|
| `stress_large_wal_recovery` | 10K key write + recovery from WAL |
| `stress_concurrent_writes` | 8 threads x 1000 writes to same DB |
| `stress_concurrent_read_write` | Readers and writers on same DB simultaneously |
| `stress_many_small_writes` | 100K small value writes throughput |
| `stress_large_values` | 1MB value writes and recovery |
| `stress_mixed_operations` | Mixed put/delete/reopen under load |
| `stress_recovery_after_churn` | Recovery after high-volume write+delete churn |
| `stress_repeated_reopen` | 20 reopen cycles with writes between each |
| `stress_all_primitives_sustained` | All 6 primitives under sustained load |

### Engine Stress (7 tests)

**File:** `tests/engine/stress.rs`
**Run:** `cargo test --test engine stress -- --ignored`

| Test | Workload |
|------|----------|
| `stress_concurrent_kv_operations` | High concurrency KV read/write |
| `stress_transaction_throughput` | Sustained transaction commit rate |
| `stress_large_batch_kv` | Large batch KV operations |
| `stress_many_concurrent_runs` | Many parallel runs with data |
| `stress_cross_primitive_transactions` | Transactions spanning all primitives |
| `stress_vector_search` | Vector search under write contention |
| `stress_eventlog_append` | High-rate event append |

### Storage Stress (6 tests)

**File:** `tests/storage/stress.rs`
**Run:** `cargo test --test storage stress -- --ignored`

| Test | Workload |
|------|----------|
| `stress_concurrent_writers_readers` | High-concurrency writers and readers |
| `stress_rapid_snapshot_creation` | Rapid snapshot acquisition |
| `stress_version_chain_growth` | Deep version chain growth per key |
| `stress_ttl_expiration_cleanup` | TTL expiration under load |
| `stress_many_runs_concurrent` | Many runs with concurrent access |
| `stress_sustained_throughput` | Sustained throughput measurement |

### Integration Scale (4 tests)

**File:** `tests/integration/scale.rs`
**Run:** `cargo test --test integration scale -- --ignored`

| Test | Workload |
|------|----------|
| `kv_scale_100k` | 100K key-value pairs |
| `event_scale_100k` | 100K events |
| `json_scale_100k` | 100K JSON documents |
| `vector_scale_100k` | 100K vectors |

---

## Deferred: Intelligence Tests (489 compile errors)

**Directory:** `tests/intelligence/` (31 files, `main.rs` renamed to `main.rs.deferred`)

The entire intelligence test suite needs a full rewrite to target the current
search API in `strata_engine::search`. The test *concepts* are valuable:

- Budget enforcement (search respects time/resource budgets)
- Score normalization consistency
- RRF fusion correctness (Reciprocal Rank Fusion)
- Hybrid search orchestration (keyword + vector combined)
- Deterministic ordering (same query produces same order)
- Snapshot consistency during search
- Regression: `issue_018_search_overfetch`

To restore: rename `main.rs.deferred` back to `main.rs` and rewrite imports from
`strata_core::search_types` to `strata_engine::search`, create the missing
`tests/intelligence/common.rs` module, and update helper function signatures.

---

## Quick Reference

Find all ignored tests:
```bash
cargo test --test concurrency --test durability --test engine --test executor --test integration --test storage -- --ignored --list
```

Run all ignored tests (slow):
```bash
cargo test --test concurrency --test durability --test engine --test executor --test integration --test storage -- --ignored
```

Search for ignored tests by required method:
```bash
grep -r '#\[ignore' tests/ --include='*.rs' | grep 'requires:'
```
