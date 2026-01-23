# EventLog Defects and Gaps

> Consolidated from test suite analysis, architecture review, and API gap analysis.
> Source: `tests/substrate_api_comprehensive/eventlog/` and `crates/api/src/substrate/event.rs`

## Summary

| Category | Count | Priority |
|----------|-------|----------|
| Implementation Bugs | 4 | P0-P2 |
| Missing Table Stakes APIs | 6 | P0 |
| Missing Important APIs | 3 | P1 |
| API Design Issues | 3 | P1 |
| Known Limitations | 2 | N/A |
| Performance Issues | 2 | P1 |
| **Total Issues** | **20** | |

---

## Current Substrate API (5 methods)

```rust
// What exists today
fn event_append(run, stream, payload) -> Version;
fn event_get(run, stream, sequence) -> Option<Versioned<Value>>;
fn event_range(run, stream, start?, end?, limit?) -> Vec<Versioned<Value>>;
fn event_len(run, stream) -> u64;
fn event_latest_sequence(run, stream) -> Option<u64>;
```

---

## Part 1: Implementation Bugs

### Bug 1: Payload Type Validation Not Enforced

**GitHub Issue:** [#705](https://github.com/anibjoshi/in-mem/issues/705)

**Priority:** P0

**Tests:**
- `eventlog::edge_cases::test_payload_must_be_object_null_rejected`
- `eventlog::edge_cases::test_payload_must_be_object_bool_rejected`
- `eventlog::edge_cases::test_payload_must_be_object_int_rejected`
- `eventlog::edge_cases::test_payload_must_be_object_float_rejected`
- `eventlog::edge_cases::test_payload_must_be_object_string_rejected`
- `eventlog::edge_cases::test_payload_must_be_object_bytes_rejected`
- `eventlog::edge_cases::test_payload_must_be_object_array_rejected`

**Expected:** `event_append(&run, stream, non_object_value)` returns `ConstraintViolation`

**Actual:** Operation succeeds with any Value type

**Contract Reference:** `crates/api/src/substrate/event.rs` lines 30-43

**Fix:**
```rust
if !matches!(payload, Value::Object(_)) {
    return Err(StrataError::ConstraintViolation("Payload must be Object".into()));
}
```

---

### Bug 2: Empty Stream Name Not Rejected

**GitHub Issue:** [#706](https://github.com/anibjoshi/in-mem/issues/706)

**Priority:** P1

**Test:** `eventlog::edge_cases::test_stream_name_empty_rejected`

**Expected:** `event_append(&run, "", payload)` returns `InvalidKey`

**Actual:** Operation succeeds

**Fix:**
```rust
if stream.is_empty() {
    return Err(StrataError::InvalidKey("Stream name cannot be empty".into()));
}
```

---

### Bug 3: Sequences Start at 0 Instead of 1

**GitHub Issue:** [#707](https://github.com/anibjoshi/in-mem/issues/707)

**Priority:** P1

**Test:** `eventlog::basic_ops::test_append_returns_sequence_version`

**Expected:** First event has sequence >= 1 (typical database convention)

**Actual:** First event has sequence 0

**Options:**
1. Change to 1-based sequences (breaking change)
2. Document as 0-based (documentation fix)

---

### Bug 4: Float Special Values (NaN, Infinity) Fail on Read

**GitHub Issue:** [#708](https://github.com/anibjoshi/in-mem/issues/708)

**Priority:** P2

**Test:** `eventlog::edge_cases::test_float_special_values_in_payload`

**Expected:** Special floats rejected at append OR roundtrip correctly

**Actual:** Append succeeds, read fails with serialization error

**Root Cause:** JSON doesn't support NaN/Infinity natively

---

## Part 2: Missing Table Stakes APIs (P0)

### Gap 1: `event_append_batch` - Atomic Multi-Event Append

**Priority:** P0 - Critical for event sourcing

**Proposed API:**
```rust
fn event_append_batch(&self, run: &ApiRunId, events: Vec<(String, Value)>)
    -> StrataResult<Vec<Version>>;
```

**Why Critical:**
- Cannot atomically write related events (OrderCreated + InventoryReserved + PaymentInitiated)
- Every production event store has this:
  - Kafka: Batched produce
  - Redis: XADD with multiple entries
  - EventStoreDB: AppendToStream with multiple events

**Current Workaround:** Multiple separate appends (NOT atomic, can partially fail)

**Impact:** Cannot ensure event consistency for multi-event business operations

---

### Gap 2: `event_rev_range` - Reverse Range (Newest First)

**Priority:** P0 - Most common UI pattern

**Proposed API:**
```rust
fn event_rev_range(&self, run: &ApiRunId, stream: &str,
    start: Option<u64>, end: Option<u64>, limit: Option<u64>)
    -> StrataResult<Vec<Versioned<Value>>>;
```

**Why Critical:**
- "Show last N events" is the most common access pattern
- Activity feeds, audit logs, recent orders all need newest-first
- Redis: `XREVRANGE`

**Current Workaround:** Read all events, reverse in memory (O(n) always)

**Note:** Facade has `xrevrange` but Substrate doesn't expose it!

---

### Gap 3: `event_streams` - List All Streams

**Priority:** P0 - Required for administration

**Proposed API:**
```rust
fn event_streams(&self, run: &ApiRunId) -> StrataResult<Vec<String>>;
```

**Why Critical:**
- Cannot discover what streams exist
- Required for:
  - Admin/debugging tools
  - Data export/migration
  - Stream enumeration
  - Monitoring dashboards

**Primitive Has This:** `event_types()` exists but is not exposed

**Current Workaround:** None - must know stream names in advance

---

### Gap 4: `event_range_by_time` - Timestamp-Based Queries

**Priority:** P0 - Fundamental for operational queries

**Proposed API:**
```rust
fn event_range_by_time(&self, run: &ApiRunId, stream: &str,
    start_time: Option<i64>, end_time: Option<i64>, limit: Option<u64>)
    -> StrataResult<Vec<Versioned<Value>>>;
```

**Why Critical:**
- "Give me events from the last hour" is fundamental
- Events HAVE timestamps (primitive stores them)
- Time-based queries are essential for:
  - Debugging ("what happened at 3pm?")
  - Analytics ("events per hour")
  - Compliance ("show me yesterday's activity")

**Current Workaround:** Read all events, filter by timestamp in application (O(n))

---

### Gap 5: `event_stream_info` - Stream Metadata (O(1))

**Priority:** P0 - Performance critical

**Proposed API:**
```rust
struct StreamInfo {
    first_sequence: Option<u64>,
    last_sequence: Option<u64>,
    count: u64,
    first_timestamp: Option<i64>,
    last_timestamp: Option<i64>,
}

fn event_stream_info(&self, run: &ApiRunId, stream: &str)
    -> StrataResult<StreamInfo>;
```

**Why Critical:**
- Current `event_len` and `event_latest_sequence` are O(n)
- Production systems need O(1) metadata access
- Every database has this (Redis: XINFO STREAM, Kafka: describe topic)

**Current Workaround:** Call O(n) methods repeatedly

---

### Gap 6: `event_consumer_position` - Consumer Offset Tracking

**Priority:** P0 - Required for reliable consumers

**Proposed API:**
```rust
fn event_consumer_get_position(&self, run: &ApiRunId, stream: &str,
    consumer_id: &str) -> StrataResult<Option<u64>>;

fn event_consumer_set_position(&self, run: &ApiRunId, stream: &str,
    consumer_id: &str, position: u64) -> StrataResult<()>;
```

**Why Critical:**
- Consumers need "where did I leave off?" for:
  - Restart recovery
  - At-least-once processing
  - Consumer lag monitoring
- Every event system has this:
  - Kafka: Consumer offsets
  - Redis: Consumer groups with XACK
  - Pulsar: Subscription cursors

**Current Workaround:** Every consumer must implement position tracking in KV store

---

## Part 3: Missing Important APIs (P1)

### Gap 7: `event_wait` - Blocking Read / Long Poll

**Priority:** P1

**Proposed API:**
```rust
fn event_wait(&self, run: &ApiRunId, stream: &str,
    after_sequence: Option<u64>, timeout_ms: u64)
    -> StrataResult<Vec<Versioned<Value>>>;
```

**Why Important:**
- Without blocking reads, consumers must poll
- Higher latency, more CPU/network load
- Every major system has this:
  - Redis: `XREAD BLOCK`
  - Kafka: `consumer.poll(timeout)`

**Current Workaround:** Application-level polling with sleep

---

### Gap 8: `event_verify_chain` - Chain Integrity Verification

**Priority:** P1

**Proposed API:**
```rust
struct ChainVerification {
    is_valid: bool,
    length: u64,
    first_invalid: Option<u64>,
    error: Option<String>,
}

fn event_verify_chain(&self, run: &ApiRunId, stream: &str)
    -> StrataResult<ChainVerification>;
```

**Why Important:**
- Primitive has `verify_chain()` but not exposed
- Critical for compliance audits and tamper detection
- EventLog has hash chain specifically for this purpose

**Current Workaround:** None - cannot verify chain integrity

---

### Gap 9: `event_head` - Get Latest Event (Not Just Sequence)

**Priority:** P1

**Proposed API:**
```rust
fn event_head(&self, run: &ApiRunId, stream: &str)
    -> StrataResult<Option<Versioned<Value>>>;
```

**Why Important:**
- Get latest event directly without extra round trip
- Primitive has `head()` but Substrate only exposes sequence

**Current Workaround:** `event_get(stream, event_latest_sequence(stream)?)` - two calls

---

## Part 4: API Design Issues (P1)

### Design Issue 1: Timestamp Not Exposed in Return Type

**Current:**
```rust
Versioned<Value>  // Only: version + payload
```

**Should Be:**
```rust
struct EventEntry {
    sequence: u64,
    payload: Value,
    timestamp: i64,  // Events have timestamps!
}
```

**Impact:** Facade has `EventEntry` with timestamp, but Substrate hides it

---

### Design Issue 2: Hash Chain Completely Hidden

Events have `hash` and `prev_hash` for tamper-evidence, but no Substrate access.

**Should Expose:**
- `event_get_with_metadata()` - Return event with hash info
- `event_verify_chain()` - Verify integrity (see Gap 8)

---

### Design Issue 3: Inconsistency Between Substrate and Facade

| Feature | Substrate | Facade |
|---------|-----------|--------|
| Reverse range | No | Yes (`xrevrange`) |
| Timestamp in result | No | Yes |
| Stream info | No | No |

Facade has features that Substrate doesn't, which is backwards.

---

## Part 5: Known Limitations (Not Bugs)

### Limitation 1: Sequences Are Global, Not Per-Stream

**Behavior:** Sequences span all streams within a run

```rust
event_append(&run, "stream1", p1);  // seq = 0
event_append(&run, "stream2", p2);  // seq = 1  (not 0!)
event_append(&run, "stream1", p3);  // seq = 2  (not 1!)
```

**Root Cause:** Substrate maps `stream` → `event_type`. Primitive has single log per run.

**Status:** Documented; accepted for M11

---

### Limitation 2: Single-Writer Serialization

All appends serialize through CAS (200 retries). Parallel append from multiple threads will experience contention.

**Status:** By design for ordering guarantees

---

## Part 6: Performance Issues (P1)

### Perf 1: `event_len()` is O(n)

**Current Implementation:** Reads all events, filters by type, counts

**Should Be:** O(1) metadata lookup

**Fix:** Maintain per-stream count in metadata key

---

### Perf 2: `event_latest_sequence()` is O(n)

**Current Implementation:** Reverse scans until finding matching type

**Should Be:** O(1) metadata lookup

**Fix:** Maintain per-stream latest sequence in metadata key

---

## Priority Matrix

| ID | Issue | Priority | Effort | Category |
|----|-------|----------|--------|----------|
| Bug 1 | Payload validation | P0 | Low | Bug |
| Gap 1 | Batch append | P0 | Medium | Missing API |
| Gap 2 | Reverse range | P0 | Low | Missing API |
| Gap 3 | List streams | P0 | Low | Missing API |
| Gap 4 | Time-based queries | P0 | Medium | Missing API |
| Gap 5 | Stream info O(1) | P0 | Medium | Missing API |
| Gap 6 | Consumer position | P0 | Medium | Missing API |
| Bug 2 | Empty stream name | P1 | Low | Bug |
| Bug 3 | Sequence start | P1 | Low | Bug/Doc |
| Gap 7 | Blocking read | P1 | High | Missing API |
| Gap 8 | Chain verification | P1 | Low | Missing API |
| Gap 9 | Head event | P1 | Low | Missing API |
| Perf 1 | O(n) event_len | P1 | Medium | Performance |
| Perf 2 | O(n) latest_seq | P1 | Medium | Performance |
| Design 1 | Timestamp hidden | P1 | Low | Design |
| Design 2 | Hash chain hidden | P1 | Low | Design |
| Design 3 | Facade/Substrate mismatch | P1 | Medium | Design |
| Bug 4 | Float NaN/Infinity | P2 | Medium | Bug |

---

## Recommended Fix Order

### Phase 1: Quick Wins (Low Effort)
1. Add payload type validation (Bug 1)
2. Add stream name validation (Bug 2)
3. Expose `event_rev_range` (Gap 2) - Facade already has it
4. Expose `event_streams` (Gap 3) - Primitive already has `event_types()`
5. Expose `event_verify_chain` (Gap 8) - Primitive already has it
6. Expose `event_head` (Gap 9) - Primitive already has it
7. Add timestamp to Substrate return type (Design 1)

### Phase 2: Core Features (Medium Effort)
8. Implement `event_append_batch` (Gap 1)
9. Implement `event_range_by_time` (Gap 4)
10. Implement `event_stream_info` with O(1) metadata (Gap 5)
11. Implement `event_consumer_position` (Gap 6)
12. Fix O(n) performance issues (Perf 1, 2)

### Phase 3: Advanced Features (High Effort)
13. Implement `event_wait` / blocking reads (Gap 7)

---

## Test Coverage Summary

| Current | After Phase 1 | After Phase 2 |
|---------|---------------|---------------|
| 92 pass | ~110 pass | ~140 pass |
| 10 fail | 2 fail | 0 fail |
| 0 ignore | TBD ignore | 0 ignore |

---

## GitHub Issues

### Existing (Bugs)
| Issue | Title | Priority |
|-------|-------|----------|
| [#705](https://github.com/anibjoshi/in-mem/issues/705) | Payload type validation not enforced | P0 |
| [#706](https://github.com/anibjoshi/in-mem/issues/706) | Empty stream name not rejected | P1 |
| [#707](https://github.com/anibjoshi/in-mem/issues/707) | Sequences start at 0 instead of 1 | P1 |
| [#708](https://github.com/anibjoshi/in-mem/issues/708) | Float NaN/Infinity values fail on read | P2 |

### To Create (Missing APIs)
| Title | Priority |
|-------|----------|
| EventLog: Add batch append (`event_append_batch`) | P0 |
| EventLog: Add reverse range (`event_rev_range`) | P0 |
| EventLog: Add stream listing (`event_streams`) | P0 |
| EventLog: Add timestamp-based queries (`event_range_by_time`) | P0 |
| EventLog: Add O(1) stream info (`event_stream_info`) | P0 |
| EventLog: Add consumer position tracking | P0 |
| EventLog: Add blocking read (`event_wait`) | P1 |
| EventLog: Expose chain verification | P1 |
| EventLog: Add `event_head` method | P1 |
| EventLog: Expose timestamp in Substrate return type | P1 |
| EventLog: Fix O(n) performance for len/latest_sequence | P1 |

---

## Comparison with Industry Standards

| Feature | Strata EventLog | Redis Streams | Kafka | EventStoreDB |
|---------|-----------------|---------------|-------|--------------|
| Single append | ✅ | ✅ | ✅ | ✅ |
| Batch append | ❌ | ✅ | ✅ | ✅ |
| Point read | ✅ | ✅ | ❌ | ✅ |
| Range read | ✅ | ✅ | ✅ | ✅ |
| Reverse range | ❌ (Facade only) | ✅ | ❌ | ✅ |
| Time-based query | ❌ | ✅ | ✅ | ✅ |
| Stream listing | ❌ | ✅ | ✅ | ✅ |
| Stream info | ❌ | ✅ | ✅ | ✅ |
| Consumer groups | ❌ | ✅ | ✅ | ✅ |
| Consumer positions | ❌ | ✅ | ✅ | ✅ |
| Blocking read | ❌ | ✅ | ✅ | ✅ |
| Trimming | ❌ | ✅ | ✅ (retention) | ❌ |
| Hash chain | ✅ (hidden) | ❌ | ❌ | ❌ |

**Strata's Unique Strength:** Hash chain for tamper-evidence (but currently hidden!)

**Strata's Gaps:** Consumer position tracking, blocking reads, batch append
