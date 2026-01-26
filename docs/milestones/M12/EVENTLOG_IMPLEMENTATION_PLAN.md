# EventLog Implementation Plan

> **Status**: Ready for Implementation
> **Date**: 2026-01-23
> **Scope**: Primitive + Substrate + Test Suite
> **Decisions**: All locked (see EVENTLOG_DECISIONS.md)

---

## Architectural Commitments (Non-Negotiable)

| Commitment | Implication |
|------------|-------------|
| **Global sequences** | Streams are filters, not partitions |
| **SHA-256 hashing** | Deterministic, versioned migration |
| **Object-only payloads** | Validate at primitive layer |
| **Record/replay model** | EventLog = nondeterminism boundary |
| **Transactional coupling** | Append must be in same txn as guarded mutations |

---

## Part 1: Primitive Layer Fixes

### Current Primitive API

```rust
// crates/primitives/src/event_log.rs
impl EventLog {
    pub fn append(&self, run_id: &RunId, event_type: &str, payload: Value) -> Result<Version>;
    pub fn read(&self, run_id: &RunId, sequence: u64) -> Result<Option<Versioned<Event>>>;
    pub fn read_range(&self, run_id: &RunId, start: u64, end: u64) -> Result<Vec<Versioned<Event>>>;
    pub fn head(&self, run_id: &RunId) -> Result<Option<Versioned<Event>>>;
    pub fn len(&self, run_id: &RunId) -> Result<u64>;
    pub fn is_empty(&self, run_id: &RunId) -> Result<bool>;
    pub fn read_by_type(&self, run_id: &RunId, event_type: &str) -> Result<Vec<Versioned<Event>>>;
    pub fn verify_chain(&self, run_id: &RunId) -> Result<ChainVerification>;
    pub fn event_types(&self, run_id: &RunId) -> Result<Vec<String>>;
}
```

### Target Primitive API

```rust
impl EventLog {
    // === Core Operations ===
    pub fn append(&self, run_id: &RunId, event_type: &str, payload: Value) -> Result<Version>;
    pub fn append_batch(&self, run_id: &RunId, events: &[(&str, Value)]) -> Result<Vec<Version>>;

    // === Point Reads ===
    pub fn read(&self, run_id: &RunId, sequence: u64) -> Result<Option<Versioned<Event>>>;
    pub fn head(&self, run_id: &RunId) -> Result<Option<Versioned<Event>>>;
    pub fn head_by_type(&self, run_id: &RunId, event_type: &str) -> Result<Option<Versioned<Event>>>;

    // === Range Reads ===
    pub fn read_range(&self, run_id: &RunId, start: u64, end: u64) -> Result<Vec<Versioned<Event>>>;
    pub fn read_range_reverse(&self, run_id: &RunId, start: u64, end: u64) -> Result<Vec<Versioned<Event>>>;
    pub fn read_by_type(&self, run_id: &RunId, event_type: &str) -> Result<Vec<Versioned<Event>>>;
    pub fn read_range_by_time(&self, run_id: &RunId, start: Option<i64>, end: Option<i64>) -> Result<Vec<Versioned<Event>>>;

    // === Metadata (O(1)) ===
    pub fn len(&self, run_id: &RunId) -> Result<u64>;
    pub fn len_by_type(&self, run_id: &RunId, event_type: &str) -> Result<u64>;
    pub fn latest_sequence(&self, run_id: &RunId) -> Result<Option<u64>>;
    pub fn latest_sequence_by_type(&self, run_id: &RunId, event_type: &str) -> Result<Option<u64>>;
    pub fn stream_info(&self, run_id: &RunId, event_type: &str) -> Result<Option<StreamMeta>>;
    pub fn is_empty(&self, run_id: &RunId) -> Result<bool>;

    // === Discovery ===
    pub fn event_types(&self, run_id: &RunId) -> Result<Vec<String>>;

    // === Integrity ===
    pub fn verify_chain(&self, run_id: &RunId) -> Result<ChainVerification>;

    // === Consumer Positions ===
    pub fn consumer_get_position(&self, run_id: &RunId, consumer_id: &str) -> Result<Option<u64>>;
    pub fn consumer_set_position(&self, run_id: &RunId, consumer_id: &str, position: u64) -> Result<()>;
    pub fn consumer_list(&self, run_id: &RunId) -> Result<Vec<String>>;
}
```

---

### Primitive Fix 1: Per-Stream Metadata

**Goal**: O(1) access to stream-level statistics.

**Current Metadata**:
```rust
struct EventLogMeta {
    next_sequence: u64,
    head_hash: [u8; 32],
}
```

**New Metadata**:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct EventLogMeta {
    next_sequence: u64,
    head_hash: [u8; 32],
    hash_version: u8,  // 0 = legacy DefaultHasher, 1 = SHA-256
    streams: HashMap<String, StreamMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamMeta {
    pub count: u64,
    pub first_sequence: u64,
    pub last_sequence: u64,
    pub first_timestamp: i64,
    pub last_timestamp: i64,
}
```

**Storage Key**:
```
<namespace>:<TypeTag::Event>:__meta__
```

**Update Logic** (in `append`):
```rust
fn append(&self, run_id: &RunId, event_type: &str, payload: Value) -> Result<Version> {
    // ... existing validation ...

    self.db.transaction(run_id, |txn| {
        let mut meta = self.get_or_create_meta(txn, run_id)?;

        // Update global metadata
        let sequence = meta.next_sequence;
        meta.next_sequence += 1;

        // Update per-stream metadata
        let stream_meta = meta.streams.entry(event_type.to_string())
            .or_insert(StreamMeta {
                count: 0,
                first_sequence: sequence,
                last_sequence: sequence,
                first_timestamp: timestamp,
                last_timestamp: timestamp,
            });
        stream_meta.count += 1;
        stream_meta.last_sequence = sequence;
        stream_meta.last_timestamp = timestamp;

        // ... compute hash, store event, update meta ...
    })
}
```

**New Methods**:
```rust
pub fn len_by_type(&self, run_id: &RunId, event_type: &str) -> Result<u64> {
    let meta = self.get_meta(run_id)?;
    Ok(meta.streams.get(event_type).map(|s| s.count).unwrap_or(0))
}

pub fn latest_sequence_by_type(&self, run_id: &RunId, event_type: &str) -> Result<Option<u64>> {
    let meta = self.get_meta(run_id)?;
    Ok(meta.streams.get(event_type).map(|s| s.last_sequence))
}

pub fn stream_info(&self, run_id: &RunId, event_type: &str) -> Result<Option<StreamMeta>> {
    let meta = self.get_meta(run_id)?;
    Ok(meta.streams.get(event_type).cloned())
}

pub fn head_by_type(&self, run_id: &RunId, event_type: &str) -> Result<Option<Versioned<Event>>> {
    match self.stream_info(run_id, event_type)? {
        Some(meta) => self.read(run_id, meta.last_sequence),
        None => Ok(None),
    }
}
```

---

### Primitive Fix 2: SHA-256 Hash Algorithm

**Goal**: Deterministic hash chain across all platforms and Rust versions.

**Dependencies**: Add to `crates/primitives/Cargo.toml`:
```toml
[dependencies]
sha2 = "0.10"
```

**New Hash Function**:
```rust
use sha2::{Sha256, Digest};

fn compute_event_hash_v1(
    sequence: u64,
    event_type: &str,
    payload: &Value,
    timestamp: i64,
    prev_hash: &[u8; 32],
) -> [u8; 32] {
    let mut hasher = Sha256::new();

    // Fixed field order - canonicalized
    hasher.update(&sequence.to_le_bytes());
    hasher.update(&(event_type.len() as u32).to_le_bytes());
    hasher.update(event_type.as_bytes());
    hasher.update(&timestamp.to_le_bytes());

    // Canonicalized JSON payload
    let payload_bytes = serde_json::to_vec(payload).unwrap_or_default();
    hasher.update(&(payload_bytes.len() as u32).to_le_bytes());
    hasher.update(&payload_bytes);

    hasher.update(prev_hash);

    hasher.finalize().into()
}

fn compute_event_hash(
    hash_version: u8,
    sequence: u64,
    event_type: &str,
    payload: &Value,
    timestamp: i64,
    prev_hash: &[u8; 32],
) -> [u8; 32] {
    match hash_version {
        0 => compute_event_hash_v0(sequence, event_type, payload, timestamp, prev_hash),
        1 => compute_event_hash_v1(sequence, event_type, payload, timestamp, prev_hash),
        _ => compute_event_hash_v1(sequence, event_type, payload, timestamp, prev_hash),
    }
}
```

**Migration Strategy**:
- New EventLogs start with `hash_version: 1`
- Existing EventLogs keep `hash_version: 0` (or missing = 0)
- `verify_chain` uses appropriate algorithm based on metadata
- No automatic upgrade (would require re-hashing entire chain)

**Test Vectors** (add to tests):
```rust
#[test]
fn test_sha256_hash_determinism() {
    let hash = compute_event_hash_v1(
        0,
        "test_stream",
        &json!({"key": "value"}),
        1706000000000000_i64,
        &[0u8; 32],
    );
    // This exact value must never change
    assert_eq!(
        hex::encode(hash),
        "expected_hex_value_here"
    );
}
```

---

### Primitive Fix 3: Payload Validation

**Goal**: Enforce Object-only payloads at primitive layer.

**Location**: In `append()` method, before any storage operations.

```rust
pub fn append(&self, run_id: &RunId, event_type: &str, payload: Value) -> Result<Version> {
    // Validate payload is Object
    if !matches!(payload, Value::Object(_)) {
        return Err(StrataError::ConstraintViolation(
            "Event payload must be Object".into()
        ));
    }

    // Validate no NaN/Infinity in payload
    validate_no_special_floats(&payload)?;

    // Validate event_type
    if event_type.is_empty() {
        return Err(StrataError::InvalidKey("Event type cannot be empty".into()));
    }
    if event_type.contains('\0') {
        return Err(StrataError::InvalidKey("Event type cannot contain NUL".into()));
    }
    if event_type.len() > 1024 {
        return Err(StrataError::InvalidKey("Event type exceeds 1024 bytes".into()));
    }

    // ... rest of append logic ...
}

fn validate_no_special_floats(value: &Value) -> Result<()> {
    match value {
        Value::Float(f) if f.is_nan() || f.is_infinite() => {
            Err(StrataError::ConstraintViolation(
                "Payload cannot contain NaN or Infinity".into()
            ))
        }
        Value::Array(arr) => {
            for v in arr {
                validate_no_special_floats(v)?;
            }
            Ok(())
        }
        Value::Object(map) => {
            for v in map.values() {
                validate_no_special_floats(v)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
```

---

### Primitive Fix 4: Batch Append

**Goal**: Atomic multi-event append.

```rust
pub fn append_batch(
    &self,
    run_id: &RunId,
    events: &[(&str, Value)],
) -> Result<Vec<Version>> {
    // Validate all payloads first
    for (event_type, payload) in events {
        if !matches!(payload, Value::Object(_)) {
            return Err(StrataError::ConstraintViolation(
                "Event payload must be Object".into()
            ));
        }
        validate_no_special_floats(payload)?;
        // ... validate event_type ...
    }

    if events.is_empty() {
        return Ok(vec![]);
    }

    self.db.transaction(run_id, |txn| {
        let mut meta = self.get_or_create_meta(txn, run_id)?;
        let mut versions = Vec::with_capacity(events.len());
        let mut prev_hash = meta.head_hash;
        let timestamp = current_timestamp();

        for (event_type, payload) in events {
            let sequence = meta.next_sequence;
            meta.next_sequence += 1;

            let hash = compute_event_hash(
                meta.hash_version,
                sequence,
                event_type,
                payload,
                timestamp,
                &prev_hash,
            );

            let event = Event {
                sequence,
                event_type: event_type.to_string(),
                payload: payload.clone(),
                timestamp,
                prev_hash,
                hash,
            };

            // Store event
            let key = self.event_key(run_id, sequence);
            txn.put(&key, to_stored_value(&event))?;

            // Update stream metadata
            let stream_meta = meta.streams.entry(event_type.to_string())
                .or_insert(StreamMeta {
                    count: 0,
                    first_sequence: sequence,
                    last_sequence: sequence,
                    first_timestamp: timestamp,
                    last_timestamp: timestamp,
                });
            stream_meta.count += 1;
            stream_meta.last_sequence = sequence;
            stream_meta.last_timestamp = timestamp;

            versions.push(Version::Sequence(sequence));
            prev_hash = hash;
        }

        meta.head_hash = prev_hash;
        self.put_meta(txn, run_id, &meta)?;

        Ok(versions)
    })
}
```

---

### Primitive Fix 5: Reverse Range

**Goal**: Efficient newest-first iteration.

```rust
pub fn read_range_reverse(
    &self,
    run_id: &RunId,
    start: u64,  // Higher sequence (inclusive)
    end: u64,    // Lower sequence (inclusive)
) -> Result<Vec<Versioned<Event>>> {
    if start < end {
        return Ok(vec![]);  // Invalid range
    }

    let namespace = self.namespace_for_run(run_id);
    let snapshot = self.db.snapshot();

    let mut results = Vec::new();
    for seq in (end..=start).rev() {
        if let Some(event) = self.read_at_snapshot(&snapshot, run_id, seq)? {
            results.push(event);
        }
    }

    Ok(results)
}
```

---

### Primitive Fix 6: Time-Based Range

**Goal**: Query events by timestamp.

**Option A: Secondary Index** (Better for large logs)

Add storage keys:
```
<namespace>:<TypeTag::Event>:time:<timestamp_be_bytes>:<sequence_be_bytes>
```

**Option B: Scan with Filter** (Simpler, OK for moderate sizes)

```rust
pub fn read_range_by_time(
    &self,
    run_id: &RunId,
    start_time: Option<i64>,
    end_time: Option<i64>,
    limit: Option<u64>,
) -> Result<Vec<Versioned<Event>>> {
    let meta = self.get_meta(run_id)?;
    let snapshot = self.db.snapshot();

    let mut results = Vec::new();
    let max = limit.unwrap_or(u64::MAX) as usize;

    for seq in 0..meta.next_sequence {
        if results.len() >= max {
            break;
        }

        if let Some(event) = self.read_at_snapshot(&snapshot, run_id, seq)? {
            let ts = event.value.timestamp;
            let in_range = start_time.map_or(true, |s| ts >= s)
                        && end_time.map_or(true, |e| ts <= e);
            if in_range {
                results.push(event);
            }
        }
    }

    Ok(results)
}
```

**Recommendation**: Start with Option B, add secondary index if performance requires.

---

### Primitive Fix 7: Consumer Positions

**Goal**: Track consumer read positions.

**Storage Keys**:
```
<namespace>:<TypeTag::Event>:consumer:<consumer_id>
```

**Value**:
```rust
#[derive(Serialize, Deserialize)]
struct ConsumerPosition {
    position: u64,
    updated_at: i64,
}
```

**Implementation**:
```rust
pub fn consumer_get_position(&self, run_id: &RunId, consumer_id: &str) -> Result<Option<u64>> {
    let key = self.consumer_key(run_id, consumer_id);
    match self.db.get(&key)? {
        Some(value) => {
            let pos: ConsumerPosition = from_stored_value(&value)?;
            Ok(Some(pos.position))
        }
        None => Ok(None),
    }
}

pub fn consumer_set_position(
    &self,
    run_id: &RunId,
    consumer_id: &str,
    position: u64,
) -> Result<()> {
    let key = self.consumer_key(run_id, consumer_id);
    let pos = ConsumerPosition {
        position,
        updated_at: current_timestamp(),
    };
    self.db.put(&key, to_stored_value(&pos))?;
    Ok(())
}

pub fn consumer_list(&self, run_id: &RunId) -> Result<Vec<String>> {
    let prefix = self.consumer_prefix(run_id);
    let consumers = self.db.list_keys_with_prefix(&prefix)?
        .into_iter()
        .filter_map(|k| self.extract_consumer_id(&k))
        .collect();
    Ok(consumers)
}

fn consumer_key(&self, run_id: &RunId, consumer_id: &str) -> Key {
    Key::new_with_suffix(
        self.namespace_for_run(run_id),
        TypeTag::Event,
        format!("consumer:{}", consumer_id),
    )
}
```

---

### Primitive Summary

| Fix | Priority | Effort | Status |
|-----|----------|--------|--------|
| Per-stream metadata | P0 | Medium | TODO |
| SHA-256 hashing | P0 | Low | TODO |
| Payload validation | P0 | Low | TODO |
| Batch append | P0 | Medium | TODO |
| Reverse range | P1 | Low | TODO |
| Time-based range | P1 | Medium | TODO |
| Consumer positions | P1 | Medium | TODO |

---

## Part 2: Substrate Layer Fixes

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

### Target Substrate API

```rust
trait EventLog {
    // === Core Operations ===
    fn event_append(&self, run: &ApiRunId, stream: &str, payload: Value) -> StrataResult<Version>;
    fn event_append_batch(&self, run: &ApiRunId, events: &[(&str, Value)]) -> StrataResult<Vec<Version>>;

    // === Point Reads ===
    fn event_get(&self, run: &ApiRunId, stream: &str, sequence: u64) -> StrataResult<Option<Versioned<Value>>>;
    fn event_head(&self, run: &ApiRunId, stream: &str) -> StrataResult<Option<Versioned<Value>>>;

    // === Range Reads ===
    fn event_range(&self, run: &ApiRunId, stream: &str, start: Option<u64>, end: Option<u64>, limit: Option<u64>) -> StrataResult<Vec<Versioned<Value>>>;
    fn event_rev_range(&self, run: &ApiRunId, stream: &str, start: Option<u64>, end: Option<u64>, limit: Option<u64>) -> StrataResult<Vec<Versioned<Value>>>;
    fn event_range_by_time(&self, run: &ApiRunId, stream: &str, start_time: Option<i64>, end_time: Option<i64>, limit: Option<u64>) -> StrataResult<Vec<Versioned<Value>>>;

    // === Metadata (O(1)) ===
    fn event_len(&self, run: &ApiRunId, stream: &str) -> StrataResult<u64>;
    fn event_latest_sequence(&self, run: &ApiRunId, stream: &str) -> StrataResult<Option<u64>>;
    fn event_stream_info(&self, run: &ApiRunId, stream: &str) -> StrataResult<StreamInfo>;

    // === Discovery ===
    fn event_streams(&self, run: &ApiRunId) -> StrataResult<Vec<String>>;

    // === Integrity ===
    fn event_verify_chain(&self, run: &ApiRunId) -> StrataResult<ChainVerification>;

    // === Consumer Positions ===
    fn event_consumer_position(&self, run: &ApiRunId, stream: &str, consumer_id: &str) -> StrataResult<Option<u64>>;
    fn event_consumer_checkpoint(&self, run: &ApiRunId, stream: &str, consumer_id: &str, position: u64) -> StrataResult<()>;
    fn event_consumers(&self, run: &ApiRunId, stream: &str) -> StrataResult<Vec<String>>;
}
```

---

### Substrate Implementation Notes

**Key Principle**: Substrate is a thin wrapper. It validates inputs, converts types, and delegates to primitives.

**Stream → event_type Mapping**:
```rust
// Substrate uses "stream", Primitive uses "event_type"
// They are the same thing - document this clearly

fn event_append(&self, run: &ApiRunId, stream: &str, payload: Value) -> StrataResult<Version> {
    validate_stream_name(stream)?;
    // Primitive validates payload
    self.event().append(&run.to_run_id(), stream, payload)
        .map_err(convert_error)
}
```

**O(1) Methods** (after primitive fixes):
```rust
fn event_len(&self, run: &ApiRunId, stream: &str) -> StrataResult<u64> {
    validate_stream_name(stream)?;
    self.event().len_by_type(&run.to_run_id(), stream)
        .map_err(convert_error)
}

fn event_latest_sequence(&self, run: &ApiRunId, stream: &str) -> StrataResult<Option<u64>> {
    validate_stream_name(stream)?;
    self.event().latest_sequence_by_type(&run.to_run_id(), stream)
        .map_err(convert_error)
}

fn event_head(&self, run: &ApiRunId, stream: &str) -> StrataResult<Option<Versioned<Value>>> {
    validate_stream_name(stream)?;
    self.event().head_by_type(&run.to_run_id(), stream)
        .map_err(convert_error)
        .map(|opt| opt.map(versioned_event_to_versioned_value))
}
```

**Validation Functions**:
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

// Note: Payload validation happens at primitive layer
// Substrate just passes through
```

---

## Part 3: Test Suite Design

### Test Organization

```
tests/substrate_api_comprehensive/eventlog/
├── mod.rs
├── basic_ops.rs          # Append, get, range
├── batch_ops.rs          # Batch append
├── validation.rs         # Input validation
├── streams.rs            # Stream filtering, metadata
├── time_queries.rs       # Time-based range
├── consumer.rs           # Consumer positions
├── integrity.rs          # Hash chain verification
├── concurrency.rs        # Concurrent access
├── edge_cases.rs         # Boundary conditions
├── invariants.rs         # Architectural invariants
└── equivalence.rs        # Substrate/Primitive equivalence
```

---

### Test Categories

#### Category 1: Basic Operations

```rust
// basic_ops.rs

#[test]
fn test_append_returns_sequence_version() {
    let (substrate, run) = setup();
    let v1 = substrate.event_append(&run, "stream", json!({"a": 1})).unwrap();
    let v2 = substrate.event_append(&run, "stream", json!({"b": 2})).unwrap();

    assert!(matches!(v1, Version::Sequence(0)));
    assert!(matches!(v2, Version::Sequence(1)));
}

#[test]
fn test_get_returns_appended_event() {
    let (substrate, run) = setup();
    let payload = json!({"key": "value"});
    substrate.event_append(&run, "stream", payload.clone()).unwrap();

    let event = substrate.event_get(&run, "stream", 0).unwrap().unwrap();
    assert_eq!(event.value, payload);
}

#[test]
fn test_get_nonexistent_returns_none() {
    let (substrate, run) = setup();
    let result = substrate.event_get(&run, "stream", 999).unwrap();
    assert!(result.is_none());
}

#[test]
fn test_range_returns_events_in_order() {
    let (substrate, run) = setup();
    for i in 0..10 {
        substrate.event_append(&run, "stream", json!({"i": i})).unwrap();
    }

    let events = substrate.event_range(&run, "stream", Some(3), Some(6), None).unwrap();
    assert_eq!(events.len(), 4);  // 3, 4, 5, 6

    for (idx, event) in events.iter().enumerate() {
        let expected_i = 3 + idx;
        assert_eq!(event.value["i"], expected_i);
    }
}

#[test]
fn test_range_with_limit() {
    let (substrate, run) = setup();
    for i in 0..100 {
        substrate.event_append(&run, "stream", json!({"i": i})).unwrap();
    }

    let events = substrate.event_range(&run, "stream", None, None, Some(10)).unwrap();
    assert_eq!(events.len(), 10);
}

#[test]
fn test_rev_range_returns_newest_first() {
    let (substrate, run) = setup();
    for i in 0..10 {
        substrate.event_append(&run, "stream", json!({"i": i})).unwrap();
    }

    let events = substrate.event_rev_range(&run, "stream", None, None, Some(3)).unwrap();
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].value["i"], 9);  // Newest
    assert_eq!(events[1].value["i"], 8);
    assert_eq!(events[2].value["i"], 7);
}

#[test]
fn test_head_returns_latest_event() {
    let (substrate, run) = setup();
    substrate.event_append(&run, "stream", json!({"first": true})).unwrap();
    substrate.event_append(&run, "stream", json!({"last": true})).unwrap();

    let head = substrate.event_head(&run, "stream").unwrap().unwrap();
    assert_eq!(head.value["last"], true);
}

#[test]
fn test_head_empty_stream_returns_none() {
    let (substrate, run) = setup();
    let head = substrate.event_head(&run, "stream").unwrap();
    assert!(head.is_none());
}
```

---

#### Category 2: Validation

```rust
// validation.rs

// === Payload Validation ===

#[test]
fn test_payload_must_be_object() {
    let (substrate, run) = setup();

    let result = substrate.event_append(&run, "stream", Value::String("hello".into()));
    assert!(matches!(result, Err(StrataError::ConstraintViolation(_))));
}

#[test]
fn test_payload_null_rejected() {
    let (substrate, run) = setup();
    let result = substrate.event_append(&run, "stream", Value::Null);
    assert!(matches!(result, Err(StrataError::ConstraintViolation(_))));
}

#[test]
fn test_payload_int_rejected() {
    let (substrate, run) = setup();
    let result = substrate.event_append(&run, "stream", Value::Int(42));
    assert!(matches!(result, Err(StrataError::ConstraintViolation(_))));
}

#[test]
fn test_payload_array_rejected() {
    let (substrate, run) = setup();
    let result = substrate.event_append(&run, "stream", json!([1, 2, 3]));
    assert!(matches!(result, Err(StrataError::ConstraintViolation(_))));
}

#[test]
fn test_payload_empty_object_allowed() {
    let (substrate, run) = setup();
    let result = substrate.event_append(&run, "stream", json!({}));
    assert!(result.is_ok());
}

#[test]
fn test_payload_nan_rejected() {
    let (substrate, run) = setup();
    let result = substrate.event_append(&run, "stream", json!({"value": f64::NAN}));
    assert!(matches!(result, Err(StrataError::ConstraintViolation(_))));
}

#[test]
fn test_payload_infinity_rejected() {
    let (substrate, run) = setup();
    let result = substrate.event_append(&run, "stream", json!({"value": f64::INFINITY}));
    assert!(matches!(result, Err(StrataError::ConstraintViolation(_))));
}

#[test]
fn test_payload_nested_nan_rejected() {
    let (substrate, run) = setup();
    let result = substrate.event_append(&run, "stream", json!({"nested": {"value": f64::NAN}}));
    assert!(matches!(result, Err(StrataError::ConstraintViolation(_))));
}

// === Stream Name Validation ===

#[test]
fn test_stream_name_empty_rejected() {
    let (substrate, run) = setup();
    let result = substrate.event_append(&run, "", json!({"a": 1}));
    assert!(matches!(result, Err(StrataError::InvalidKey(_))));
}

#[test]
fn test_stream_name_nul_rejected() {
    let (substrate, run) = setup();
    let result = substrate.event_append(&run, "test\0name", json!({"a": 1}));
    assert!(matches!(result, Err(StrataError::InvalidKey(_))));
}

#[test]
fn test_stream_name_max_length() {
    let (substrate, run) = setup();
    let long_name = "x".repeat(1024);
    let result = substrate.event_append(&run, &long_name, json!({"a": 1}));
    assert!(result.is_ok());

    let too_long = "x".repeat(1025);
    let result = substrate.event_append(&run, &too_long, json!({"a": 1}));
    assert!(matches!(result, Err(StrataError::InvalidKey(_))));
}
```

---

#### Category 3: Stream Semantics (Global Sequences)

```rust
// streams.rs

#[test]
fn test_sequences_are_global_not_per_stream() {
    let (substrate, run) = setup();

    let v1 = substrate.event_append(&run, "orders", json!({"order": 1})).unwrap();
    let v2 = substrate.event_append(&run, "payments", json!({"payment": 1})).unwrap();
    let v3 = substrate.event_append(&run, "orders", json!({"order": 2})).unwrap();

    // Sequences are global
    assert!(matches!(v1, Version::Sequence(0)));
    assert!(matches!(v2, Version::Sequence(1)));
    assert!(matches!(v3, Version::Sequence(2)));
}

#[test]
fn test_stream_filters_events() {
    let (substrate, run) = setup();

    substrate.event_append(&run, "orders", json!({"order": 1})).unwrap();
    substrate.event_append(&run, "payments", json!({"payment": 1})).unwrap();
    substrate.event_append(&run, "orders", json!({"order": 2})).unwrap();

    let orders = substrate.event_range(&run, "orders", None, None, None).unwrap();
    let payments = substrate.event_range(&run, "payments", None, None, None).unwrap();

    assert_eq!(orders.len(), 2);
    assert_eq!(payments.len(), 1);
}

#[test]
fn test_stream_len_counts_filtered_events() {
    let (substrate, run) = setup();

    substrate.event_append(&run, "orders", json!({"a": 1})).unwrap();
    substrate.event_append(&run, "payments", json!({"b": 1})).unwrap();
    substrate.event_append(&run, "orders", json!({"c": 1})).unwrap();

    assert_eq!(substrate.event_len(&run, "orders").unwrap(), 2);
    assert_eq!(substrate.event_len(&run, "payments").unwrap(), 1);
    assert_eq!(substrate.event_len(&run, "unknown").unwrap(), 0);
}

#[test]
fn test_stream_latest_sequence_returns_last_in_stream() {
    let (substrate, run) = setup();

    substrate.event_append(&run, "orders", json!({"a": 1})).unwrap();    // seq 0
    substrate.event_append(&run, "payments", json!({"b": 1})).unwrap();  // seq 1
    substrate.event_append(&run, "orders", json!({"c": 1})).unwrap();    // seq 2

    assert_eq!(substrate.event_latest_sequence(&run, "orders").unwrap(), Some(2));
    assert_eq!(substrate.event_latest_sequence(&run, "payments").unwrap(), Some(1));
    assert_eq!(substrate.event_latest_sequence(&run, "unknown").unwrap(), None);
}

#[test]
fn test_event_streams_lists_all_streams() {
    let (substrate, run) = setup();

    substrate.event_append(&run, "orders", json!({"a": 1})).unwrap();
    substrate.event_append(&run, "payments", json!({"b": 1})).unwrap();
    substrate.event_append(&run, "inventory", json!({"c": 1})).unwrap();

    let mut streams = substrate.event_streams(&run).unwrap();
    streams.sort();

    assert_eq!(streams, vec!["inventory", "orders", "payments"]);
}

#[test]
fn test_stream_info_returns_metadata() {
    let (substrate, run) = setup();

    substrate.event_append(&run, "orders", json!({"a": 1})).unwrap();  // seq 0
    substrate.event_append(&run, "other", json!({"b": 1})).unwrap();   // seq 1
    substrate.event_append(&run, "orders", json!({"c": 1})).unwrap();  // seq 2

    let info = substrate.event_stream_info(&run, "orders").unwrap();

    assert_eq!(info.count, 2);
    assert_eq!(info.first_sequence, Some(0));
    assert_eq!(info.last_sequence, Some(2));
}

#[test]
fn test_get_wrong_stream_returns_none() {
    let (substrate, run) = setup();

    substrate.event_append(&run, "orders", json!({"a": 1})).unwrap();  // seq 0

    // Event exists at seq 0, but not in "payments" stream
    let result = substrate.event_get(&run, "payments", 0).unwrap();
    assert!(result.is_none());
}
```

---

#### Category 4: Batch Operations

```rust
// batch_ops.rs

#[test]
fn test_batch_append_atomic() {
    let (substrate, run) = setup();

    let events = vec![
        ("orders", json!({"order": 1})),
        ("orders", json!({"order": 2})),
        ("payments", json!({"payment": 1})),
    ];

    let versions = substrate.event_append_batch(&run, &events).unwrap();

    assert_eq!(versions.len(), 3);
    assert!(matches!(versions[0], Version::Sequence(0)));
    assert!(matches!(versions[1], Version::Sequence(1)));
    assert!(matches!(versions[2], Version::Sequence(2)));
}

#[test]
fn test_batch_append_all_or_nothing() {
    let (substrate, run) = setup();

    // First append some valid events
    substrate.event_append(&run, "stream", json!({"first": true})).unwrap();

    // Batch with one invalid payload
    let events = vec![
        ("stream", json!({"valid": true})),
        ("stream", Value::String("invalid".into())),  // Not an object!
        ("stream", json!({"also_valid": true})),
    ];

    let result = substrate.event_append_batch(&run, &events);
    assert!(result.is_err());

    // Only the first event should exist
    assert_eq!(substrate.event_len(&run, "stream").unwrap(), 1);
}

#[test]
fn test_batch_append_empty() {
    let (substrate, run) = setup();

    let versions = substrate.event_append_batch(&run, &[]).unwrap();
    assert!(versions.is_empty());
}

#[test]
fn test_batch_preserves_hash_chain() {
    let (substrate, run) = setup();

    let events = vec![
        ("stream", json!({"a": 1})),
        ("stream", json!({"b": 2})),
        ("stream", json!({"c": 3})),
    ];

    substrate.event_append_batch(&run, &events).unwrap();

    let verification = substrate.event_verify_chain(&run).unwrap();
    assert!(verification.is_valid);
    assert_eq!(verification.length, 3);
}
```

---

#### Category 5: Integrity (Hash Chain)

```rust
// integrity.rs

#[test]
fn test_verify_chain_valid() {
    let (substrate, run) = setup();

    for i in 0..10 {
        substrate.event_append(&run, "stream", json!({"i": i})).unwrap();
    }

    let verification = substrate.event_verify_chain(&run).unwrap();
    assert!(verification.is_valid);
    assert_eq!(verification.length, 10);
    assert!(verification.first_invalid.is_none());
    assert!(verification.error.is_none());
}

#[test]
fn test_verify_chain_empty() {
    let (substrate, run) = setup();

    let verification = substrate.event_verify_chain(&run).unwrap();
    assert!(verification.is_valid);
    assert_eq!(verification.length, 0);
}

#[test]
fn test_hash_is_deterministic() {
    // Two runs with identical events should produce identical hashes
    let (substrate1, run1) = setup();
    let (substrate2, run2) = setup();

    let payload = json!({"key": "value"});

    substrate1.event_append(&run1, "stream", payload.clone()).unwrap();
    substrate2.event_append(&run2, "stream", payload.clone()).unwrap();

    // Get events and compare hashes
    // Note: This requires exposing hash in the API or using primitive directly
}
```

---

#### Category 6: Architectural Invariants

```rust
// invariants.rs

#[test]
fn test_eventlog_is_append_only() {
    let (substrate, run) = setup();

    substrate.event_append(&run, "stream", json!({"a": 1})).unwrap();

    // There should be no update or delete methods
    // This test documents the invariant
}

#[test]
fn test_sequences_are_monotonic() {
    let (substrate, run) = setup();

    let mut prev_seq = None;
    for i in 0..100 {
        let version = substrate.event_append(&run, "stream", json!({"i": i})).unwrap();
        if let Version::Sequence(seq) = version {
            if let Some(p) = prev_seq {
                assert!(seq > p, "Sequences must be monotonically increasing");
            }
            prev_seq = Some(seq);
        }
    }
}

#[test]
fn test_events_are_immutable() {
    let (substrate, run) = setup();

    let payload = json!({"original": true});
    substrate.event_append(&run, "stream", payload.clone()).unwrap();

    // Read event
    let event1 = substrate.event_get(&run, "stream", 0).unwrap().unwrap();

    // Append more events
    substrate.event_append(&run, "stream", json!({"other": true})).unwrap();

    // Original event unchanged
    let event2 = substrate.event_get(&run, "stream", 0).unwrap().unwrap();
    assert_eq!(event1.value, event2.value);
}

#[test]
fn test_run_isolation() {
    let substrate = setup_substrate();
    let run1 = create_run(&substrate);
    let run2 = create_run(&substrate);

    substrate.event_append(&run1, "stream", json!({"run": 1})).unwrap();
    substrate.event_append(&run2, "stream", json!({"run": 2})).unwrap();

    // Events are isolated by run
    let events1 = substrate.event_range(&run1, "stream", None, None, None).unwrap();
    let events2 = substrate.event_range(&run2, "stream", None, None, None).unwrap();

    assert_eq!(events1.len(), 1);
    assert_eq!(events2.len(), 1);
    assert_eq!(events1[0].value["run"], 1);
    assert_eq!(events2[0].value["run"], 2);
}

#[test]
fn test_transactional_atomicity() {
    // EventLog append must be in same transaction as guarded state mutations
    // This requires explicit transaction API testing
    let (substrate, run) = setup();

    // This test verifies the critical I3 invariant
    substrate.transaction(&run, |txn| {
        txn.event_append("api_response", json!({"data": "response"}))?;
        txn.kv_put("processed", Value::Bool(true))?;
        Ok(())
    }).unwrap();

    // Both should be visible
    let event = substrate.event_get(&run, "api_response", 0).unwrap();
    let kv = substrate.kv_get(&run, "processed").unwrap();

    assert!(event.is_some());
    assert!(kv.is_some());
}

#[test]
fn test_transactional_rollback() {
    let (substrate, run) = setup();

    // Append outside transaction first
    substrate.event_append(&run, "stream", json!({"committed": true})).unwrap();

    // Transaction that rolls back
    let result = substrate.transaction(&run, |txn| {
        txn.event_append("stream", json!({"uncommitted": true}))?;
        Err(StrataError::Internal("forced rollback".into()))
    });

    assert!(result.is_err());

    // Only the first event should exist
    assert_eq!(substrate.event_len(&run, "stream").unwrap(), 1);
}
```

---

#### Category 7: Consumer Positions

```rust
// consumer.rs

#[test]
fn test_consumer_position_initial_none() {
    let (substrate, run) = setup();

    let pos = substrate.event_consumer_position(&run, "stream", "consumer1").unwrap();
    assert!(pos.is_none());
}

#[test]
fn test_consumer_checkpoint_and_read() {
    let (substrate, run) = setup();

    substrate.event_consumer_checkpoint(&run, "stream", "consumer1", 42).unwrap();

    let pos = substrate.event_consumer_position(&run, "stream", "consumer1").unwrap();
    assert_eq!(pos, Some(42));
}

#[test]
fn test_consumer_checkpoint_updates() {
    let (substrate, run) = setup();

    substrate.event_consumer_checkpoint(&run, "stream", "consumer1", 10).unwrap();
    substrate.event_consumer_checkpoint(&run, "stream", "consumer1", 20).unwrap();

    let pos = substrate.event_consumer_position(&run, "stream", "consumer1").unwrap();
    assert_eq!(pos, Some(20));
}

#[test]
fn test_multiple_consumers() {
    let (substrate, run) = setup();

    substrate.event_consumer_checkpoint(&run, "stream", "consumer1", 10).unwrap();
    substrate.event_consumer_checkpoint(&run, "stream", "consumer2", 20).unwrap();

    assert_eq!(substrate.event_consumer_position(&run, "stream", "consumer1").unwrap(), Some(10));
    assert_eq!(substrate.event_consumer_position(&run, "stream", "consumer2").unwrap(), Some(20));
}

#[test]
fn test_list_consumers() {
    let (substrate, run) = setup();

    substrate.event_consumer_checkpoint(&run, "stream", "consumer1", 10).unwrap();
    substrate.event_consumer_checkpoint(&run, "stream", "consumer2", 20).unwrap();

    let mut consumers = substrate.event_consumers(&run, "stream").unwrap();
    consumers.sort();

    assert_eq!(consumers, vec!["consumer1", "consumer2"]);
}
```

---

### Test Count Summary

| Category | Test Count | Priority |
|----------|------------|----------|
| Basic Operations | ~15 | P0 |
| Validation | ~15 | P0 |
| Stream Semantics | ~10 | P0 |
| Batch Operations | ~5 | P0 |
| Integrity | ~5 | P0 |
| Invariants | ~10 | P0 |
| Consumer Positions | ~5 | P1 |
| Time Queries | ~5 | P1 |
| Concurrency | ~5 | P1 |
| Edge Cases | ~10 | P2 |
| **Total** | **~85** | |

---

## Part 4: Implementation Phases

### Phase 1: Foundation (Week 1)

**Primitive**:
1. Add `sha2` dependency
2. Implement SHA-256 hash function with versioning
3. Update metadata structure with hash_version and streams
4. Implement per-stream metadata tracking in append
5. Add `len_by_type()`, `latest_sequence_by_type()`, `stream_info()`, `head_by_type()`
6. Add payload validation (Object-only, no NaN/Infinity)
7. Add event_type validation

**Substrate**:
1. Wire to new primitive O(1) methods
2. Add stream name validation
3. Add test suite: basic_ops, validation, streams

**Tests**: ~40 tests passing

### Phase 2: Batch & Range (Week 2)

**Primitive**:
1. Implement `append_batch()`
2. Implement `read_range_reverse()`

**Substrate**:
1. Add `event_append_batch()`
2. Fix `event_rev_range()` to use primitive method

**Tests**: +15 tests (batch_ops, integrity)

### Phase 3: Consumer & Time (Week 3)

**Primitive**:
1. Implement consumer position storage
2. Implement `read_range_by_time()`

**Substrate**:
1. Add `event_consumer_position()`, `event_consumer_checkpoint()`, `event_consumers()`
2. Add `event_range_by_time()`

**Tests**: +15 tests (consumer, time_queries)

### Phase 4: Polish & Invariants (Week 4)

**All Layers**:
1. Transaction integration verification
2. Cross-primitive transaction tests
3. Concurrency tests
4. Edge case coverage
5. Documentation updates

**Tests**: +15 tests (invariants, concurrency, edge_cases)

---

## Success Criteria

| Metric | Target |
|--------|--------|
| Test count | 85+ |
| Test pass rate | 100% |
| O(1) metadata ops | Verified |
| Hash determinism | Test vectors pass |
| Payload validation | All rejections verified |
| Transaction atomicity | Verified |
| Stream semantics | Documented and tested |

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-23 | Initial implementation plan |
