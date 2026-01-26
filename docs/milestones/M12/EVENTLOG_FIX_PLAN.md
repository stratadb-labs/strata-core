# EventLog Comprehensive Fix Plan

> **Status**: APPROVED (Decisions Locked)
> **Date**: 2026-01-23
> **Scope**: Storage → Primitive → Substrate fixes for EventLog

---

## Executive Summary

EventLog has **25+ documented issues** across implementation bugs, missing APIs, performance problems, and architectural gaps. This plan consolidates findings from:

- `docs/defects/EVENTLOG_DEFECTS.md` (25 issues)
- `docs/architecture/translations/EVENTLOG_TRANSLATION.md` (semantic gaps)
- `docs/defects/FOUNDATIONAL_CAPABILITIES_AUDIT.md` (cross-primitive concerns)
- `docs/architecture/EVENTLOG_ROLE_AND_SPEC.md` (role definition)
- `docs/architecture/EVENTLOG_DECISIONS.md` (locked decisions)

## Architectural Commitments (LOCKED)

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Stream Sequences | **Global** | Run-level determinism > messaging semantics |
| Hash Algorithm | **SHA-256** | Determinism is non-negotiable |
| Payload Types | **Object Only** | Enforce existing contract |
| Replay Model | **Record/Replay** | EventLog = nondeterminism boundary |

## EventLog's Role

**EventLog is the determinism boundary recorder.**

- **Snapshot + WAL** = source of truth for state
- **EventLog** = source of truth for nondeterministic inputs

Streams are logical filters, not isolated partitions. Sequences are global within a run.

---

## Issue Inventory

### By Layer

| Layer | Issues | Priority Distribution |
|-------|--------|----------------------|
| **Primitive** | 12 | 4 P0, 5 P1, 3 P2 |
| **Substrate** | 8 | 3 P0, 4 P1, 1 P2 |
| **Cross-cutting** | 4 | 2 P0, 2 P1 |
| **Total** | 24 | 9 P0, 11 P1, 4 P2 |

### Critical Path (P0)

| ID | Issue | Layer | Effort |
|----|-------|-------|--------|
| P0-1 | Payload type validation not enforced | Substrate | Low |
| P0-2 | Batch append (`event_append_batch`) missing | Primitive+Substrate | Medium |
| P0-3 | Reverse range not exposed in Substrate | Substrate | Low |
| P0-4 | Stream listing not exposed | Substrate | Low |
| P0-5 | O(n) performance for `event_len`, `event_latest_sequence` | Primitive | Medium |
| P0-6 | Per-stream metadata (O(1) stream info) | Primitive | Medium |
| P0-7 | Consumer position tracking | Primitive+Substrate | Medium |
| P0-8 | Timestamp-based range queries | Primitive+Substrate | Medium |
| P0-9 | Transaction integration verification | Cross-cutting | Medium |

---

## Layer 1: Primitive Fixes

### Current Primitive API

```rust
// crates/primitives/src/event_log.rs
impl EventLog {
    fn append(&self, run_id, event_type, payload) -> Result<Version>;
    fn read(&self, run_id, sequence) -> Result<Option<Versioned<Event>>>;
    fn read_range(&self, run_id, start, end) -> Result<Vec<Versioned<Event>>>;
    fn head(&self, run_id) -> Result<Option<Versioned<Event>>>;
    fn len(&self, run_id) -> Result<u64>;
    fn is_empty(&self, run_id) -> Result<bool>;
    fn read_by_type(&self, run_id, event_type) -> Result<Vec<Versioned<Event>>>;
    fn verify_chain(&self, run_id) -> Result<ChainVerification>;
    fn event_types(&self, run_id) -> Result<Vec<String>>;
}
```

### Primitive Fix 1: Per-Stream Metadata Tracking (P0)

**Problem**: `event_len(stream)` and `event_latest_sequence(stream)` are O(n) because Substrate must read all events and filter.

**Solution**: Maintain per-stream metadata in addition to global metadata.

**Current Metadata Structure**:
```rust
struct EventLogMeta {
    next_sequence: u64,
    head_hash: [u8; 32],
}
```

**Proposed Metadata Structure**:
```rust
struct EventLogMeta {
    next_sequence: u64,
    head_hash: [u8; 32],
    // NEW: Per-stream tracking
    streams: HashMap<String, StreamMeta>,
}

struct StreamMeta {
    count: u64,
    first_sequence: u64,
    last_sequence: u64,
    first_timestamp: i64,
    last_timestamp: i64,
}
```

**New Primitive Methods**:
```rust
fn stream_info(&self, run_id: &RunId, event_type: &str) -> Result<Option<StreamInfo>>;
fn len_by_type(&self, run_id: &RunId, event_type: &str) -> Result<u64>;
fn latest_sequence_by_type(&self, run_id: &RunId, event_type: &str) -> Result<Option<u64>>;
```

**Impact**: O(1) stream metadata access instead of O(n).

---

### Primitive Fix 2: Batch Append (P0)

**Problem**: No atomic multi-event append. Cannot write OrderCreated + InventoryReserved atomically.

**Proposed API**:
```rust
fn append_batch(
    &self,
    run_id: &RunId,
    events: Vec<(String, Value)>,  // (event_type, payload)
) -> Result<Vec<Version>>;
```

**Semantics**:
- All events get consecutive sequence numbers
- Hash chain remains valid (each event hashes previous)
- Atomic: all succeed or all fail
- Returns vector of versions in order

**Implementation**:
- Single transaction with multiple event writes
- Update metadata once at end with final sequence/hash

---

### Primitive Fix 3: Timestamp Index (P0)

**Problem**: No way to query events by time range efficiently.

**Solution**: Add secondary index by timestamp.

**New Storage Keys**:
```
<namespace>:event:time:<timestamp_be_bytes>:<sequence_be_bytes>
```

**New Primitive Method**:
```rust
fn read_range_by_time(
    &self,
    run_id: &RunId,
    start_time: Option<i64>,
    end_time: Option<i64>,
    limit: Option<u64>,
) -> Result<Vec<Versioned<Event>>>;
```

---

### Primitive Fix 4: Consumer Position Tracking (P0)

**Problem**: No built-in consumer offset management.

**Storage Keys**:
```
<namespace>:event:consumer:<consumer_id>
```

**Value**:
```rust
struct ConsumerPosition {
    last_sequence: u64,
    updated_at: i64,
}
```

**New Primitive Methods**:
```rust
fn consumer_get_position(&self, run_id: &RunId, consumer_id: &str) -> Result<Option<u64>>;
fn consumer_set_position(&self, run_id: &RunId, consumer_id: &str, position: u64) -> Result<()>;
fn consumer_list(&self, run_id: &RunId) -> Result<Vec<String>>;
```

---

### Primitive Fix 5: Deterministic Hash (P0 - LOCKED)

**Problem**: `DefaultHasher` is explicitly NOT guaranteed stable across Rust versions.

**Decision**: Use SHA-256. Determinism is non-negotiable.

**Implementation**:
```rust
use sha2::{Sha256, Digest};

fn compute_event_hash(
    sequence: u64,
    stream: &str,  // event_type
    payload: &Value,
    timestamp: i64,
    prev_hash: &[u8; 32],
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    // Fixed field order - canonicalized
    hasher.update(&sequence.to_le_bytes());
    hasher.update(stream.as_bytes());
    hasher.update(&(stream.len() as u32).to_le_bytes());  // Length prefix
    hasher.update(&timestamp.to_le_bytes());
    hasher.update(&serde_json::to_vec(payload).unwrap_or_default());
    hasher.update(prev_hash);
    hasher.finalize().into()
}
```

**Migration**:
```rust
struct EventLogMeta {
    next_sequence: u64,
    head_hash: [u8; 32],
    hash_version: u8,  // 0 = DefaultHasher (legacy), 1 = SHA-256
}
```

- New events use SHA-256 (v1)
- Existing chains verified with appropriate algorithm based on version
- Add test vectors to ensure hash determinism

---

### Primitive Fix 6: Reverse Range (P1)

**Problem**: `read_range` only supports forward iteration.

**Proposed API**:
```rust
fn read_range_reverse(
    &self,
    run_id: &RunId,
    start: u64,  // inclusive, higher sequence
    end: u64,    // inclusive, lower sequence
) -> Result<Vec<Versioned<Event>>>;
```

**Implementation**: Use storage iterator in reverse direction.

---

### Primitive Fix 7: Head by Type (P1)

**Problem**: Getting latest event for a specific type requires O(n) scan.

**With per-stream metadata (Fix 1)**, this becomes O(1):
```rust
fn head_by_type(&self, run_id: &RunId, event_type: &str) -> Result<Option<Versioned<Event>>> {
    let info = self.stream_info(run_id, event_type)?;
    match info {
        Some(meta) => self.read(run_id, meta.last_sequence),
        None => Ok(None),
    }
}
```

---

### Primitive Summary

| Fix | Priority | Effort | Dependencies |
|-----|----------|--------|--------------|
| Per-stream metadata | P0 | Medium | None |
| Batch append | P0 | Medium | None |
| Timestamp index | P0 | Medium | None |
| Consumer positions | P0 | Medium | None |
| Deterministic hash | P1 | Low | Migration strategy |
| Reverse range | P1 | Low | None |
| Head by type | P1 | Low | Per-stream metadata |

---

## Layer 2: Substrate Fixes

### Current Substrate API

```rust
// crates/api/src/substrate/event.rs
trait EventLog {
    fn event_append(run, stream, payload) -> Version;
    fn event_range(run, stream, start?, end?, limit?) -> Vec<Versioned<Value>>;
    fn event_get(run, stream, sequence) -> Option<Versioned<Value>>;
    fn event_len(run, stream) -> u64;
    fn event_latest_sequence(run, stream) -> Option<u64>;
    fn event_rev_range(run, stream, start?, end?, limit?) -> Vec<Versioned<Value>>;
    fn event_streams(run) -> Vec<String>;
    fn event_head(run, stream) -> Option<Versioned<Value>>;
    fn event_verify_chain(run) -> ChainVerification;
}
```

### Substrate Fix 1: Payload Validation (P0)

**Problem**: Non-Object payloads accepted but documented as requiring Object.

**Location**: `crates/api/src/substrate/event.rs:244-248`

**Current**:
```rust
fn event_append(&self, run: &ApiRunId, stream: &str, payload: Value) -> StrataResult<Version> {
    validate_stream_name(stream)?;
    validate_event_payload(&payload)?;  // This exists but may not be working
    // ...
}
```

**Fix**: Ensure `validate_event_payload` properly enforces Object:
```rust
fn validate_event_payload(payload: &Value) -> StrataResult<()> {
    if !matches!(payload, Value::Object(_)) {
        return Err(StrataError::ConstraintViolation(
            "Event payload must be Object".into()
        ));
    }
    Ok(())
}
```

---

### Substrate Fix 2: Stream Name Validation (P1)

**Problem**: Empty stream names accepted.

**Fix**: Add to `validate_stream_name`:
```rust
fn validate_stream_name(stream: &str) -> StrataResult<()> {
    if stream.is_empty() {
        return Err(StrataError::InvalidKey("Stream name cannot be empty".into()));
    }
    if stream.contains('\0') {
        return Err(StrataError::InvalidKey("Stream name cannot contain NUL".into()));
    }
    if stream.len() > 1024 {
        return Err(StrataError::InvalidKey("Stream name exceeds 1024 bytes".into()));
    }
    Ok(())
}
```

---

### Substrate Fix 3: Use Primitive Methods (P0)

**Problem**: Substrate implements O(n) filtering when primitive could provide O(1).

**Current Implementation** (O(n)):
```rust
fn event_len(&self, run: &ApiRunId, stream: &str) -> StrataResult<u64> {
    let events = self.event().read_by_type(&run_id, stream)?;
    Ok(events.len() as u64)  // Loads ALL events!
}
```

**After Primitive Fix 1** (O(1)):
```rust
fn event_len(&self, run: &ApiRunId, stream: &str) -> StrataResult<u64> {
    self.event().len_by_type(&run.to_run_id(), stream)
}
```

Same pattern for:
- `event_latest_sequence` → `primitive.latest_sequence_by_type()`
- `event_head` → `primitive.head_by_type()`

---

### Substrate Fix 4: Expose Timestamp in Return Type (P1)

**Problem**: `Versioned<Value>` hides timestamp, but events have timestamps.

**Option A: Keep Versioned<Value>, include timestamp**

The `Versioned<T>` struct already has a `timestamp` field:
```rust
pub struct Versioned<T> {
    pub value: T,
    pub version: Version,
    pub timestamp: Timestamp,
}
```

Ensure the timestamp is populated correctly from the Event's timestamp field.

**Current** (line 276-280):
```rust
.map(|e| Versioned {
    value: e.value.payload.clone(),
    version: e.version,
    timestamp: strata_core::Timestamp::from_millis(e.value.timestamp as u64),
})
```

This looks correct. Verify the conversion is proper (`timestamp` in Event is `i64` microseconds or milliseconds?).

---

### Substrate Fix 5: Add Batch Append (P0)

**Proposed API**:
```rust
fn event_append_batch(
    &self,
    run: &ApiRunId,
    events: Vec<(&str, Value)>,  // (stream, payload)
) -> StrataResult<Vec<Version>>;
```

**Note**: All events in batch use same stream OR allow mixed streams.

**Implementation**: Delegate to `primitive.append_batch()`.

---

### Substrate Fix 6: Add Time-Based Range (P0)

**Proposed API**:
```rust
fn event_range_by_time(
    &self,
    run: &ApiRunId,
    stream: &str,
    start_time: Option<i64>,
    end_time: Option<i64>,
    limit: Option<u64>,
) -> StrataResult<Vec<Versioned<Value>>>;
```

**Implementation**: Delegate to `primitive.read_range_by_time()`.

---

### Substrate Fix 7: Add Consumer Position APIs (P0)

**Proposed API**:
```rust
fn event_consumer_get_position(
    &self,
    run: &ApiRunId,
    stream: &str,
    consumer_id: &str,
) -> StrataResult<Option<u64>>;

fn event_consumer_set_position(
    &self,
    run: &ApiRunId,
    stream: &str,
    consumer_id: &str,
    position: u64,
) -> StrataResult<()>;
```

---

### Substrate Fix 8: Add Stream Info (P0)

**Proposed API**:
```rust
pub struct StreamInfo {
    pub first_sequence: Option<u64>,
    pub last_sequence: Option<u64>,
    pub count: u64,
    pub first_timestamp: Option<i64>,
    pub last_timestamp: Option<i64>,
}

fn event_stream_info(&self, run: &ApiRunId, stream: &str) -> StrataResult<StreamInfo>;
```

---

### Substrate Summary

| Fix | Priority | Effort | Depends On |
|-----|----------|--------|------------|
| Payload validation | P0 | Low | None |
| Stream name validation | P1 | Low | None |
| Use primitive O(1) methods | P0 | Low | Primitive Fix 1 |
| Batch append | P0 | Low | Primitive Fix 2 |
| Time-based range | P0 | Low | Primitive Fix 3 |
| Consumer positions | P0 | Low | Primitive Fix 4 |
| Stream info | P0 | Low | Primitive Fix 1 |
| Timestamp exposure | P1 | Low | None |

---

## Layer 3: Cross-Cutting Concerns

### Cross-Cut 1: Transaction Integration (P0 - CRITICAL)

**Architectural Requirement**: EventLog append MUST be part of the same transaction as any state mutation it logically guards.

This is not optional. Without this, replay has a correctness hole: you could observe external input without atomically committing corresponding state transitions.

**Required Behavior**:
```rust
// CORRECT: EventLog append and state mutation in same transaction
db.transaction(run_id, |txn| {
    let response = external_api_call();
    txn.event_append("api_response", response.clone())?;  // Record nondeterminism
    txn.kv_put("result", process(response))?;             // Guarded state change
    Ok(())
})?;

// WRONG: EventLog append outside transaction boundary
event_log.append(run_id, "api_response", response)?;  // Committed
kv.put(run_id, "result", process(response))?;          // Might fail separately
```

**Required Verification**:
1. EventLog appends work within transactions
2. Rollback properly reverts appends
3. Cross-primitive transactions (KV + EventLog) work atomically
4. Crash between EventLog append and guarded mutation is impossible (same transaction)

**Test Cases Needed**:
```rust
#[test]
fn test_event_append_rollback() {
    // Begin transaction
    // Append event
    // Rollback
    // Verify event not visible
}

#[test]
fn test_event_kv_cross_primitive_atomicity() {
    // Begin transaction
    // KV put
    // Event append
    // Rollback
    // Verify BOTH operations reverted
}
```

---

### Cross-Cut 2: Retention Policy (P1)

**From Audit**: "Retention policy not integrated"

**Required**:
- EventLog respects run-level retention policy
- Old events can be trimmed based on retention
- `HistoryTrimmed` error returned for trimmed events

---

### Cross-Cut 3: Sequence Starting Point (P1)

**Issue**: Sequences start at 0, typical convention is 1.

**Decision Needed**:
1. Change to 1-based (breaking change)
2. Document as 0-based (documentation fix)

**Recommendation**: Document as 0-based. Changing would break existing data.

---

### Cross-Cut 4: Float Special Values (P2)

**Problem**: NaN/Infinity in payload succeed on append, fail on read.

**Options**:
1. Reject at append time (validate payload recursively)
2. Use JSON extension that supports special floats
3. Document as unsupported

**Recommendation**: Reject at append time with clear error message.

---

## Implementation Phases

### Phase 1: Quick Wins (No Primitive Changes)

**Effort**: 1-2 days

| Task | Priority | Layer |
|------|----------|-------|
| Payload type validation | P0 | Substrate |
| Stream name validation | P1 | Substrate |
| Document 0-based sequences | P1 | Docs |
| Float validation at append | P2 | Substrate |

**Deliverable**: Input validation complete, no new features.

---

### Phase 2: Primitive Performance Foundation

**Effort**: 3-5 days

| Task | Priority | Dependencies |
|------|----------|--------------|
| Per-stream metadata structure | P0 | None |
| `len_by_type()` method | P0 | Per-stream metadata |
| `latest_sequence_by_type()` | P0 | Per-stream metadata |
| `head_by_type()` | P1 | Per-stream metadata |
| Migrate existing data (if any) | P0 | New metadata |

**Deliverable**: O(1) stream metadata access at primitive level.

---

### Phase 3: Substrate API Completion

**Effort**: 2-3 days

| Task | Priority | Dependencies |
|------|----------|--------------|
| Wire substrate to new primitive methods | P0 | Phase 2 |
| Add `event_stream_info()` | P0 | Phase 2 |
| Verify timestamp exposure | P1 | None |

**Deliverable**: Substrate uses efficient primitive methods.

---

### Phase 4: New Primitive Features

**Effort**: 5-7 days

| Task | Priority | Dependencies |
|------|----------|--------------|
| Batch append | P0 | None |
| Timestamp index | P0 | None |
| Consumer positions | P0 | None |
| Reverse range iterator | P1 | None |

**Deliverable**: Core missing features at primitive level.

---

### Phase 5: New Substrate Features

**Effort**: 2-3 days

| Task | Priority | Dependencies |
|------|----------|--------------|
| `event_append_batch()` | P0 | Phase 4 batch |
| `event_range_by_time()` | P0 | Phase 4 timestamp |
| Consumer position APIs | P0 | Phase 4 consumer |

**Deliverable**: Full substrate API.

---

### Phase 6: Cross-Cutting & Testing

**Effort**: 3-5 days

| Task | Priority | Dependencies |
|------|----------|--------------|
| Transaction integration tests | P0 | None |
| Retention policy integration | P1 | Retention system |
| Deterministic hash migration | P1 | None |
| Comprehensive test suite | P0 | All previous |

**Deliverable**: Production-ready EventLog.

---

## Test Coverage Plan

### Current State
- 92 tests passing
- 10 tests failing (bugs)
- 0 ignored

### Target State
| Phase | Passing | Failing | New Tests |
|-------|---------|---------|-----------|
| After Phase 1 | 102 | 0 | +10 validation |
| After Phase 2 | 112 | 0 | +10 performance |
| After Phase 3 | 122 | 0 | +10 API |
| After Phase 4 | 142 | 0 | +20 features |
| After Phase 5 | 162 | 0 | +20 substrate |
| After Phase 6 | 182 | 0 | +20 cross-cutting |

---

## Risk Assessment

### High Risk

| Risk | Mitigation |
|------|------------|
| Per-stream metadata migration | Add version field, migrate lazily |
| Hash algorithm change | Support both algorithms during transition |
| Breaking sequence numbering | Document, don't change |

### Medium Risk

| Risk | Mitigation |
|------|------------|
| Performance regression | Benchmark before/after each phase |
| API compatibility | Keep existing methods, add new ones |

### Low Risk

| Risk | Mitigation |
|------|------------|
| Test coverage gaps | Write tests before implementation |

---

## Appendix: Stream Semantics (LOCKED)

### Decision

**Streams are logical filters, not isolated partitions. Sequences are global within a run.**

This is an architectural commitment, not a temporary limitation.

### Rationale

Run-level determinism is the core invariant. A single total order across all nondeterministic inputs is the simplest and strongest guarantee for:
- Replay correctness
- Integrity verification (single hash chain)
- Debugging (total ordering of events)

### Semantic Footgun Warning

Users expecting Redis Streams or Kafka semantics will be surprised. Documentation must clearly state:

> Streams in EventLog are logical filters, not isolated logs. Sequence numbers are global within a run and must not be interpreted as per-stream offsets.

### Implementation

- Per-stream METADATA (count, first/last sequence) for O(1) operations
- Global sequence space maintained
- Single hash chain across all events
- Filter by stream (event_type) at read time

---

## GitHub Issues to Create

### Bugs (Create if not exists)
- [x] #705: Payload type validation
- [x] #706: Empty stream name
- [x] #707: Sequence start
- [x] #708: Float NaN/Infinity

### Missing APIs
- [ ] EventLog: Per-stream metadata tracking (P0)
- [ ] EventLog: Batch append (P0)
- [ ] EventLog: Timestamp index and range queries (P0)
- [ ] EventLog: Consumer position tracking (P0)
- [ ] EventLog: Stream info API (P0)
- [ ] EventLog: Deterministic hash algorithm (P1)

### Testing
- [ ] EventLog: Transaction integration verification (P0)
- [ ] EventLog: Comprehensive test suite (P0)

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-23 | Initial comprehensive plan |
