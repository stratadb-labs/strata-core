# Event Primitive - Architecture Reference

## Overview

The Event primitive provides an append-only log with hash-chained integrity. Events are ordered by sequence number within a branch, grouped by event type, and linked via SHA-256 hashes for tamper detection.

- **Version semantics**: `Version::Sequence(u64)` - monotonic sequence numbers (0, 1, 2, ...) per branch
- **Key construction**: `Key { namespace: Namespace::for_branch(branch_id), type_tag: TypeTag::Event (0x02), user_key: sequence.to_be_bytes() }`
- **Metadata key**: `Key { ..., user_key: b"__meta__" }` stores `EventLogMeta` (next sequence, head hash, per-type stream metadata)
- **Storage format**: Events serialized as JSON strings via `to_stored_value()` into `Value::String`
- **Transactional**: Yes - each append runs in a transaction with retry (up to 200 retries for contention)

## Layer Architecture

```
+------------------------------------------------------------------+
|  CLIENT                                                          |
|  Command::EventAppend { branch, event_type, payload }            |
+------------------------------------------------------------------+
        |
        v
+------------------------------------------------------------------+
|  SESSION (session.rs)                                            |
|  Routes to executor OR active transaction context                |
|  - If txn active: Transaction::new(ctx, ns).event_append()      |
|  - If no txn: executor.execute(cmd)                              |
+------------------------------------------------------------------+
        |
        v
+------------------------------------------------------------------+
|  EXECUTOR (executor.rs)                                          |
|  Dispatches to: crate::handlers::event::event_append(...)        |
+------------------------------------------------------------------+
        |
        v
+------------------------------------------------------------------+
|  HANDLER (handlers/event.rs + bridge.rs)                         |
|  1. to_core_branch_id(&branch) -> core::BranchId                |
|  2. Call primitives.event.append(&branch_id, &event_type, value) |
|  3. extract_version(&version) -> u64                             |
|  (No key validation - event_type validated in engine)            |
+------------------------------------------------------------------+
        |
        v
+------------------------------------------------------------------+
|  ENGINE PRIMITIVE (primitives/event.rs - EventLog)               |
|  1. Validate event_type (non-empty, <= 256 chars)                |
|  2. Validate payload (must be Object, no NaN/Infinity)           |
|  3. transaction_with_retry(branch_id, retry_config, |txn| {     |
|       - Read EventLogMeta from __meta__ key                      |
|       - Assign sequence = meta.next_sequence                     |
|       - Compute SHA-256 hash (chained with prev_hash)            |
|       - Write Event to Key::new_event(ns, sequence)              |
|       - Update meta: next_sequence++, head_hash = new hash       |
|       - Write updated meta to __meta__ key                       |
|     })                                                           |
|  4. Update inverted index (if enabled)                           |
|  5. Return Version::Sequence(sequence)                           |
+------------------------------------------------------------------+
        |
        v
+------------------------------------------------------------------+
|  TRANSACTION (concurrency crate)                                 |
|  Two writes per append:                                          |
|  1. Key::new_event(ns, seq) -> Event JSON                        |
|  2. Key::new_event_meta(ns) -> EventLogMeta JSON                 |
|  Retries up to 200 times on OCC conflict (1ms-50ms backoff)      |
+------------------------------------------------------------------+
        |
        v
+------------------------------------------------------------------+
|  STORAGE (storage/sharded.rs)                                    |
|  Both event and meta keys stored in VersionChain                 |
|  Events are immutable once written (append-only)                 |
+------------------------------------------------------------------+
```

## Operation Flows

### EventAppend

```
Client               Handler             Engine (EventLog)    Transaction          Storage
  |                    |                   |                    |                   |
  |-- EventAppend ---->|                   |                    |                   |
  | {branch,type,val}  |                   |                    |                   |
  |                    |                   |                    |                   |
  |                    |-- branch->UUID -->|                    |                   |
  |                    |                   |                    |                   |
  |                    |                   |-- validate ------->|                   |
  |                    |                   |   event_type       |                   |
  |                    |                   |   payload          |                   |
  |                    |                   |                    |                   |
  |                    |                   |== RETRY LOOP (max 200) =============>|
  |                    |                   |                    |                   |
  |                    |                   |-- begin txn ------>|                   |
  |                    |                   |                    |                   |
  |                    |                   |-- txn.get -------->|-- read meta ----->|
  |                    |                   |   __meta__ key     |                   |
  |                    |                   |                    |                   |
  |                    |                   |<- EventLogMeta ----|                   |
  |                    |                   |  {next_seq, hash}  |                   |
  |                    |                   |                    |                   |
  |                    |                   |-- compute hash --->|                   |
  |                    |                   |  SHA256(seq ||     |                   |
  |                    |                   |   type_len || type |                   |
  |                    |                   |   || ts || pay_len |                   |
  |                    |                   |   || payload ||    |                   |
  |                    |                   |   prev_hash)       |                   |
  |                    |                   |                    |                   |
  |                    |                   |-- txn.put -------->|-- write event --->|
  |                    |                   |  Key::new_event    |   write_set       |
  |                    |                   |  (ns, seq)         |                   |
  |                    |                   |                    |                   |
  |                    |                   |-- txn.put -------->|-- write meta ---->|
  |                    |                   |  __meta__ key      |   write_set       |
  |                    |                   |  {next_seq+1,      |                   |
  |                    |                   |   new_hash}        |                   |
  |                    |                   |                    |                   |
  |                    |                   |-- commit --------->|-- OCC validate -->|
  |                    |                   |                    |   persist both    |
  |                    |                   |                    |                   |
  |                    |                   |== END RETRY LOOP (retry on conflict) =|
  |                    |                   |                    |                   |
  |                    |                   |-- index update --->|                   |
  |                    |                   |   (if enabled)     |                   |
  |                    |                   |                    |                   |
  |<- Output::Version -|<- extract u64 ----|                    |                   |
  |   (sequence_num)   |                   |                    |                   |
```

**Steps:**

1. **Handler**: Converts branch ID. No key validation (event_type is validated in engine).
2. **Engine (EventLog)**: Validates `event_type` (non-empty, <= 256 chars) and `payload` (must be `Value::Object`, no NaN/Infinity floats). Opens a retry-capable transaction (200 retries, 1-50ms exponential backoff).
3. **Inside transaction**:
   - Reads `EventLogMeta` from the `__meta__` key (or creates default if first event)
   - Assigns `sequence = meta.next_sequence`
   - Captures current timestamp in microseconds
   - Computes SHA-256 hash: `H(sequence_le8 || type_len_le4 || type_bytes || timestamp_le8 || payload_len_le4 || payload_json || prev_hash_32)`
   - Builds `Event { sequence, event_type, payload, timestamp, prev_hash, hash }`
   - Writes event to `Key::new_event(ns, sequence)` as JSON string
   - Updates per-type stream metadata in `EventLogMeta.streams`
   - Increments `meta.next_sequence`, sets `meta.head_hash = new_hash`
   - Writes updated meta to `__meta__` key
4. **Transaction**: Two puts per append. On OCC conflict (concurrent appends to same branch), retries with backoff.
5. **Post-transaction**: Updates inverted index for full-text search if enabled.

**Hash chain format** (canonical, from `compute_event_hash` in `primitives/event.rs`):
```
SHA256(
  sequence:     u64 little-endian (8 bytes)
  type_len:     u32 little-endian (4 bytes)
  event_type:   variable bytes
  timestamp:    u64 little-endian (8 bytes)
  payload_len:  u32 little-endian (4 bytes)
  payload:      canonical JSON bytes
  prev_hash:    32 bytes (all zeros for first event)
)
```

---

### EventRead

```
Client               Handler             Engine (EventLog)    Transaction          Storage
  |                    |                   |                    |                   |
  |-- EventRead ------>|                   |                    |                   |
  | {branch, sequence} |                   |                    |                   |
  |                    |                   |                    |                   |
  |                    |-- branch->UUID -->|                    |                   |
  |                    |                   |                    |                   |
  |                    |                   |-- begin txn ------>|                   |
  |                    |                   |                    |                   |
  |                    |                   |-- txn.get -------->|                   |
  |                    |                   |  Key::new_event    |-- read chain ---->|
  |                    |                   |  (ns, sequence)    |  version <=       |
  |                    |                   |                    |  snapshot          |
  |                    |                   |                    |                   |
  |                    |                   |<- Option<Value> ---|<- StoredValue ----|
  |                    |                   |                    |                   |
  |                    |                   |-- deserialize ---->|                   |
  |                    |                   |   from_stored_val  |                   |
  |                    |                   |   -> Event struct  |                   |
  |                    |                   |                    |                   |
  |                    |                   |-- wrap in -------->|                   |
  |                    |                   |   Versioned<Event> |                   |
  |                    |                   |   Version::Seq(n)  |                   |
  |                    |                   |                    |                   |
  |                    |<- Option<Event> ---|                    |                   |
  |                    |                   |                    |                   |
  |                    |-- map to -------->|                   |                    |
  |                    |   VersionedValue  |                   |                    |
  |                    |   {payload,       |                   |                    |
  |                    |    version, ts}   |                   |                    |
  |                    |                   |                    |                   |
  |<- MaybeVersioned --|                   |                    |                   |
```

**Steps:**

1. **Handler**: Converts branch. Calls `primitives.event.read(&branch_id, sequence)`. Maps the returned `Versioned<Event>` to `VersionedValue { value: event.payload, version: sequence, timestamp }`.
2. **Engine (EventLog)**: Constructs `Key::new_event(ns, sequence)` (sequence as big-endian 8 bytes). Opens transaction. Calls `txn.get()`. Deserializes the JSON string back to `Event` struct. Wraps in `Versioned::with_timestamp(event, Version::Sequence(seq), Timestamp)`.
3. **Transaction/Storage**: Standard read path through write set -> delete set -> snapshot.

**Returns**: `Output::MaybeVersioned(Option<VersionedValue>)` where `VersionedValue.value` is the event payload only (not the full Event struct).

---

### EventReadByType

```
Client               Handler             Engine (EventLog)    Transaction          Storage
  |                    |                   |                    |                   |
  |-- ReadByType ----->|                   |                    |                   |
  | {branch, type}     |                   |                    |                   |
  |                    |-- branch->UUID -->|                    |                   |
  |                    |                   |                    |                   |
  |                    |                   |-- begin txn ------>|                   |
  |                    |                   |                    |                   |
  |                    |                   |-- txn.get -------->|-- read meta ----->|
  |                    |                   |   __meta__ key     |                   |
  |                    |                   |                    |                   |
  |                    |                   |<- EventLogMeta ----|                   |
  |                    |                   |  next_sequence = N |                   |
  |                    |                   |                    |                   |
  |                    |                   |== SCAN seq 0..N =======================|
  |                    |                   |                    |                   |
  |                    |                   |-- txn.get(seq=i) ->|-- read event ---->|
  |                    |                   |                    |                   |
  |                    |                   |-- deserialize ---->|                   |
  |                    |                   |   if type matches: |                   |
  |                    |                   |   add to results   |                   |
  |                    |                   |                    |                   |
  |                    |                   |== END SCAN ================================|
  |                    |                   |                    |                   |
  |                    |<- Vec<Versioned> -|                    |                   |
  |                    |                   |                    |                   |
  |                    |-- map each to --->|                    |                   |
  |                    |   VersionedValue  |                    |                   |
  |                    |                   |                    |                   |
  |<- VersionedValues -|                   |                    |                   |
```

**Steps:**

1. **Handler**: Converts branch. Calls `primitives.event.read_by_type()`. Maps each `Versioned<Event>` to `VersionedValue { value: event.payload, version: sequence, timestamp }`.
2. **Engine (EventLog)**: Opens transaction. Reads `EventLogMeta` to get `next_sequence` (total event count). Iterates through ALL events from sequence 0 to N-1. For each event, deserializes and checks if `event.event_type == target_type`. Collects matching events.
3. **Performance note**: This is an O(N) scan over all events in the branch. The `EventLogMeta.streams` map tracks per-type metadata but is not currently used to optimize the scan.

**Returns**: `Output::VersionedValues(Vec<VersionedValue>)` - all events of the specified type, ordered by sequence.

---

### EventLen

```
Client               Handler             Engine (EventLog)    Transaction          Storage
  |                    |                   |                    |                   |
  |-- EventLen ------->|                   |                    |                   |
  | {branch}           |                   |                    |                   |
  |                    |-- branch->UUID -->|                    |                   |
  |                    |                   |                    |                   |
  |                    |                   |-- begin txn ------>|                   |
  |                    |                   |                    |                   |
  |                    |                   |-- txn.get -------->|-- read meta ----->|
  |                    |                   |   __meta__ key     |                   |
  |                    |                   |                    |                   |
  |                    |                   |<- EventLogMeta ----|                   |
  |                    |                   |  next_sequence = N |                   |
  |                    |                   |                    |                   |
  |<-- Output::Uint ---|<-- u64 -----------|<-- N --------------|                   |
```

**Steps:**

1. **Handler**: Converts branch. Calls `primitives.event.len()`.
2. **Engine (EventLog)**: Opens transaction. Reads `EventLogMeta` from `__meta__` key. Returns `meta.next_sequence` (which equals the total event count since sequences start at 0).
3. **Single read operation** - very efficient.

**Returns**: `Output::Uint(u64)` - total number of events in the branch.

## Storage Format

```
Event entries:
  TypeTag:         0x02 (Event)
  Key format:      Namespace + TypeTag::Event + sequence.to_be_bytes()
  Value format:    Value::String(JSON) containing Event { sequence, event_type, payload, timestamp, prev_hash, hash }
  Version:         Version::Txn(commit_version) in storage chain; Version::Sequence(seq) returned to caller

Event metadata:
  TypeTag:         0x02 (Event)
  Key format:      Namespace + TypeTag::Event + b"__meta__"
  Value format:    Value::String(JSON) containing EventLogMeta { next_sequence, head_hash, streams }
```

### Event Struct

```
Event {
    sequence:    u64          // Position in log (0-indexed)
    event_type:  String       // User-defined type tag
    payload:     Value        // Must be Value::Object
    timestamp:   u64          // Microseconds since epoch
    prev_hash:   [u8; 32]    // Previous event's hash (zeros for first)
    hash:        [u8; 32]    // SHA-256 hash of this event
}
```

### EventLogMeta Struct

```
EventLogMeta {
    next_sequence:  u64                         // Next sequence to assign
    head_hash:      [u8; 32]                    // Latest event's hash
    streams:        HashMap<String, StreamMeta> // Per-type metadata
}

StreamMeta {
    first_sequence:  u64    // First event of this type
    last_sequence:   u64    // Latest event of this type
    count:           u64    // Total events of this type
    first_timestamp: u64    // Timestamp of first event
    last_timestamp:  u64    // Timestamp of latest event
}
```

## Transaction Behavior

| Aspect | Behavior |
|--------|----------|
| Isolation | Snapshot isolation |
| Concurrency control | OCC with aggressive retry (200 attempts) |
| Retry backoff | Exponential: 1ms base, 50ms max |
| Writes per append | 2 (event entry + metadata) |
| Read-your-writes | Yes within transaction |
| Multi-command txn | Yes via Session (event_append through Transaction wrapper) |

## Consistency Notes

- Event uses `Version::Sequence(u64)` unlike KV's `Version::Txn(u64)` - the sequence number is application-meaningful (position in log), not a global transaction ID
- Events are the only primitive with **hash chaining** - provides tamper-evidence for the append-only log
- Events are the only primitive with **aggressive retry** (200 attempts) - necessary because all appends contend on the shared `__meta__` key
- `EventReadByType` does a full O(N) scan despite `StreamMeta` tracking per-type ranges - optimization opportunity
- The `payload` must be `Value::Object` (validated in engine), unlike KV which accepts any `Value` type
- Events are immutable once written - there is no update or delete for individual events
- The Session transaction path for `EventAppend` uses a `Transaction` wrapper which has its own hash computation, using the canonical `compute_event_hash()` function from `primitives/event.rs`
