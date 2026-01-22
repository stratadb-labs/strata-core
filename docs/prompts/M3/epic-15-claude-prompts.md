# Epic 15: EventLog Primitive - Implementation Prompts

**Epic Goal**: Immutable append-only event stream with causal hash chaining.

**GitHub Issue**: [#161](https://github.com/anibjoshi/in-mem/issues/161)
**Status**: Ready to begin (after Epic 13)
**Dependencies**: Epic 13 (Primitives Foundation) complete

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M3_ARCHITECTURE.md` is the GOSPEL for ALL M3 implementation.**

Before starting ANY story in this epic, read:
- Section 5: EventLog Primitive
- Section 5.4: Hash Chaining (Causal, Not Cryptographic)
- Section 12.3: EventLog Invariant Enforcement

See `docs/prompts/M3_PROMPT_HEADER.md` for complete guidelines.

---

## Epic 15 Overview

### Critical Design Decisions

1. **Single-Writer-Ordered**: All appends serialize through CAS on metadata key. Parallel append is NOT supported.

2. **Hash Chaining is Causal, Not Cryptographic**: Uses `DefaultHasher` padded to 32 bytes. Provides tamper-evidence within process, NOT tamper-resistance.

3. **Append-Only Invariant**: EventLog has NO update/delete methods. Events are immutable once appended.

### Scope
- EventLog struct as stateless facade
- Event structure with sequence, type, payload, timestamp, hashes
- Append operation with automatic sequence assignment and hash chaining
- Read operations: single event, range, head, length
- Chain verification
- Query by event type
- EventLogExt transaction extension

### Success Criteria
- [ ] EventLog struct implemented with `Arc<Database>` reference
- [ ] Event struct with all fields (sequence, event_type, payload, timestamp, prev_hash, hash)
- [ ] `append()` atomically increments sequence and chains hash
- [ ] `read()` and `read_range()` return events by sequence
- [ ] `head()` returns latest event
- [ ] `len()` returns event count
- [ ] `verify_chain()` validates hash chain integrity
- [ ] `read_by_type()` filters events by type
- [ ] Append-only invariant enforced (no update/delete methods)
- [ ] All unit tests pass (>95% coverage)

### Component Breakdown
- **Story #174**: EventLog Core & Event Structure - BLOCKS others in this epic
- **Story #175**: EventLog Append with Hash Chaining
- **Story #176**: EventLog Read Operations
- **Story #177**: EventLog Chain Verification
- **Story #178**: EventLog Query by Type
- **Story #179**: EventLogExt Transaction Extension

---

## Dependency Graph

```
Phase 1 (Sequential):
  Story #174 (EventLog Core)
    └─> BLOCKS #175

Phase 2 (Sequential):
  Story #175 (Append with Hash Chaining)
    └─> Depends on #174
    └─> BLOCKS #176, #177, #178

Phase 3 (Parallel - 3 Claudes after #175):
  Story #176 (Read Operations)
  Story #177 (Chain Verification)
  Story #178 (Query by Type)
    └─> All depend on #174, #175
    └─> Independent of each other

Phase 4 (Sequential):
  Story #179 (EventLogExt Transaction Extension)
    └─> Depends on all previous stories
```

---

## Story #174: EventLog Core & Event Structure

**GitHub Issue**: [#174](https://github.com/anibjoshi/in-mem/issues/174)
**Estimated Time**: 4 hours
**Dependencies**: Epic 13 complete
**Blocks**: Story #175

### Start Story

```bash
/opt/homebrew/bin/gh issue view 174
./scripts/start-story.sh 15 174 eventlog-core
```

### Implementation

Create `crates/primitives/src/event_log.rs`:

```rust
//! EventLog: Immutable append-only event stream primitive
//!
//! ## Design Principles
//!
//! 1. **Single-Writer-Ordered**: All appends serialize through CAS on metadata key.
//! 2. **Causal Hash Chaining**: Each event includes hash of previous event.
//! 3. **Append-Only**: No update or delete operations - events are immutable.
//!
//! ## Hash Chain
//!
//! The hash chain provides tamper-evidence within process boundary, NOT
//! cryptographic security. Uses `DefaultHasher` padded to 32 bytes for
//! future SHA-256 upgrade path.

use std::sync::Arc;
use serde::{Serialize, Deserialize};
use in_mem_engine::Database;
use in_mem_core::{Key, Namespace, RunId, Value, Result};

/// An event in the log
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Event {
    /// Sequence number (auto-assigned, monotonic per run)
    pub sequence: u64,
    /// Event type (user-defined category)
    pub event_type: String,
    /// Event payload (arbitrary data)
    pub payload: Value,
    /// Timestamp when event was appended (milliseconds since epoch)
    pub timestamp: i64,
    /// Hash of previous event (for chaining)
    pub prev_hash: [u8; 32],
    /// Hash of this event
    pub hash: [u8; 32],
}

/// EventLog metadata stored per run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct EventLogMeta {
    pub next_sequence: u64,
    pub head_hash: [u8; 32],
}

impl Default for EventLogMeta {
    fn default() -> Self {
        Self {
            next_sequence: 0,
            head_hash: [0u8; 32],  // Genesis hash
        }
    }
}

/// Immutable append-only event stream
///
/// DESIGN: Single-writer-ordered per run.
/// All appends serialize through CAS on metadata key.
#[derive(Clone)]
pub struct EventLog {
    db: Arc<Database>,
}

impl EventLog {
    /// Create new EventLog instance
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Get the underlying database reference
    pub fn database(&self) -> &Arc<Database> {
        &self.db
    }

    /// Build namespace for run-scoped operations
    fn namespace_for_run(&self, run_id: &RunId) -> Namespace {
        Namespace::for_run(run_id)
    }
}
```

### Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_serialization() {
        let event = Event {
            sequence: 42,
            event_type: "test".to_string(),
            payload: Value::String("data".into()),
            timestamp: 1234567890,
            prev_hash: [0u8; 32],
            hash: [1u8; 32],
        };

        let json = serde_json::to_string(&event).unwrap();
        let restored: Event = serde_json::from_str(&json).unwrap();
        assert_eq!(event, restored);
    }

    #[test]
    fn test_eventlog_meta_default() {
        let meta = EventLogMeta::default();
        assert_eq!(meta.next_sequence, 0);
        assert_eq!(meta.head_hash, [0u8; 32]);
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 174
```

---

## Story #175: EventLog Append with Hash Chaining

**GitHub Issue**: [#175](https://github.com/anibjoshi/in-mem/issues/175)
**Estimated Time**: 5 hours
**Dependencies**: Story #174
**Blocks**: Stories #176, #177, #178

### Start Story

```bash
/opt/homebrew/bin/gh issue view 175
./scripts/start-story.sh 15 175 eventlog-append
```

### Implementation

Add to `crates/primitives/src/event_log.rs`:

```rust
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

/// Compute event hash (causal, not cryptographic)
///
/// Uses DefaultHasher padded to 32 bytes for future SHA-256 upgrade path.
fn compute_event_hash(
    sequence: u64,
    event_type: &str,
    payload: &Value,
    timestamp: i64,
    prev_hash: &[u8; 32],
) -> [u8; 32] {
    let mut hasher = DefaultHasher::new();
    sequence.hash(&mut hasher);
    event_type.hash(&mut hasher);
    // Hash payload as JSON string for determinism
    serde_json::to_string(payload).unwrap_or_default().hash(&mut hasher);
    timestamp.hash(&mut hasher);
    prev_hash.hash(&mut hasher);

    // Convert u64 to [u8; 32] (padded for future SHA-256)
    let h = hasher.finish();
    let mut result = [0u8; 32];
    result[0..8].copy_from_slice(&h.to_le_bytes());
    result
}

impl EventLog {
    /// Append a new event to the log
    ///
    /// Returns the assigned sequence number and event hash.
    /// Serializes through CAS on metadata key - parallel appends will retry.
    pub fn append(
        &self,
        run_id: &RunId,
        event_type: &str,
        payload: Value,
    ) -> Result<(u64, [u8; 32])> {
        self.db.transaction(run_id, |txn| {
            let ns = self.namespace_for_run(run_id);

            // Read current metadata (or default)
            let meta_key = Key::new_event_meta(ns.clone());
            let meta: EventLogMeta = match txn.get(&meta_key)? {
                Some(v) => serde_json::from_value(v.into_json()?)?,
                None => EventLogMeta::default(),
            };

            // Compute event hash
            let sequence = meta.next_sequence;
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64;

            let hash = compute_event_hash(
                sequence,
                event_type,
                &payload,
                timestamp,
                &meta.head_hash,
            );

            // Build event
            let event = Event {
                sequence,
                event_type: event_type.to_string(),
                payload: payload.clone(),
                timestamp,
                prev_hash: meta.head_hash,
                hash,
            };

            // Write event
            let event_key = Key::new_event(ns.clone(), sequence);
            txn.put(event_key, Value::from_json(serde_json::to_value(&event)?)?)?;

            // Update metadata (CAS semantics through transaction)
            let new_meta = EventLogMeta {
                next_sequence: sequence + 1,
                head_hash: hash,
            };
            txn.put(meta_key, Value::from_json(serde_json::to_value(&new_meta)?)?)?;

            Ok((sequence, hash))
        })
    }
}
```

### Tests

```rust
#[test]
fn test_append_first_event() {
    let (_temp, db, event_log) = setup();
    let run_id = RunId::new();
    db.begin_run(&run_id).unwrap();

    let (seq, hash) = event_log.append(&run_id, "test", Value::Null).unwrap();
    assert_eq!(seq, 0);
    assert_ne!(hash, [0u8; 32]);  // Hash is computed
}

#[test]
fn test_append_increments_sequence() {
    let (_temp, db, event_log) = setup();
    let run_id = RunId::new();
    db.begin_run(&run_id).unwrap();

    let (seq1, _) = event_log.append(&run_id, "test", Value::Null).unwrap();
    let (seq2, _) = event_log.append(&run_id, "test", Value::Null).unwrap();
    let (seq3, _) = event_log.append(&run_id, "test", Value::Null).unwrap();

    assert_eq!(seq1, 0);
    assert_eq!(seq2, 1);
    assert_eq!(seq3, 2);
}

#[test]
fn test_hash_chain_links() {
    let (_temp, db, event_log) = setup();
    let run_id = RunId::new();
    db.begin_run(&run_id).unwrap();

    let (_, hash1) = event_log.append(&run_id, "test", Value::Null).unwrap();
    let (_, _hash2) = event_log.append(&run_id, "test", Value::Null).unwrap();

    // Second event's prev_hash should be first event's hash
    // (verified through read in Story #176)
    let _ = hash1;  // Will verify chain integrity in #177
}
```

### Complete Story

```bash
./scripts/complete-story.sh 175
```

---

## Story #176: EventLog Read Operations

**GitHub Issue**: [#176](https://github.com/anibjoshi/in-mem/issues/176)
**Estimated Time**: 4 hours
**Dependencies**: Stories #174, #175

### Implementation

```rust
impl EventLog {
    /// Read a single event by sequence number
    pub fn read(&self, run_id: &RunId, sequence: u64) -> Result<Option<Event>> {
        self.db.transaction(run_id, |txn| {
            let ns = self.namespace_for_run(run_id);
            let event_key = Key::new_event(ns, sequence);

            match txn.get(&event_key)? {
                Some(v) => Ok(Some(serde_json::from_value(v.into_json()?)?)),
                None => Ok(None),
            }
        })
    }

    /// Read a range of events [start, end)
    pub fn read_range(&self, run_id: &RunId, start: u64, end: u64) -> Result<Vec<Event>> {
        self.db.transaction(run_id, |txn| {
            let mut events = Vec::new();
            let ns = self.namespace_for_run(run_id);

            for seq in start..end {
                let event_key = Key::new_event(ns.clone(), seq);
                if let Some(v) = txn.get(&event_key)? {
                    let event: Event = serde_json::from_value(v.into_json()?)?;
                    events.push(event);
                }
            }

            Ok(events)
        })
    }

    /// Get the latest event (head of the log)
    pub fn head(&self, run_id: &RunId) -> Result<Option<Event>> {
        self.db.transaction(run_id, |txn| {
            let ns = self.namespace_for_run(run_id);
            let meta_key = Key::new_event_meta(ns.clone());

            let meta: EventLogMeta = match txn.get(&meta_key)? {
                Some(v) => serde_json::from_value(v.into_json()?)?,
                None => return Ok(None),
            };

            if meta.next_sequence == 0 {
                return Ok(None);
            }

            let event_key = Key::new_event(ns, meta.next_sequence - 1);
            match txn.get(&event_key)? {
                Some(v) => Ok(Some(serde_json::from_value(v.into_json()?)?)),
                None => Ok(None),
            }
        })
    }

    /// Get the current length of the log
    pub fn len(&self, run_id: &RunId) -> Result<u64> {
        self.db.transaction(run_id, |txn| {
            let ns = self.namespace_for_run(run_id);
            let meta_key = Key::new_event_meta(ns);

            let meta: EventLogMeta = match txn.get(&meta_key)? {
                Some(v) => serde_json::from_value(v.into_json()?)?,
                None => EventLogMeta::default(),
            };

            Ok(meta.next_sequence)
        })
    }

    /// Check if log is empty
    pub fn is_empty(&self, run_id: &RunId) -> Result<bool> {
        Ok(self.len(run_id)? == 0)
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 176
```

---

## Story #177: EventLog Chain Verification

**GitHub Issue**: [#177](https://github.com/anibjoshi/in-mem/issues/177)
**Estimated Time**: 4 hours
**Dependencies**: Stories #174, #175

### Implementation

```rust
/// Chain verification result
#[derive(Debug, Clone)]
pub struct ChainVerification {
    pub is_valid: bool,
    pub length: u64,
    pub first_invalid: Option<u64>,
    pub error: Option<String>,
}

impl EventLog {
    /// Verify chain integrity from start to end
    pub fn verify_chain(&self, run_id: &RunId) -> Result<ChainVerification> {
        self.db.transaction(run_id, |txn| {
            let ns = self.namespace_for_run(run_id);
            let meta_key = Key::new_event_meta(ns.clone());

            let meta: EventLogMeta = match txn.get(&meta_key)? {
                Some(v) => serde_json::from_value(v.into_json()?)?,
                None => return Ok(ChainVerification {
                    is_valid: true,
                    length: 0,
                    first_invalid: None,
                    error: None,
                }),
            };

            let mut prev_hash = [0u8; 32];  // Genesis

            for seq in 0..meta.next_sequence {
                let event_key = Key::new_event(ns.clone(), seq);
                let event: Event = match txn.get(&event_key)? {
                    Some(v) => serde_json::from_value(v.into_json()?)?,
                    None => return Ok(ChainVerification {
                        is_valid: false,
                        length: meta.next_sequence,
                        first_invalid: Some(seq),
                        error: Some(format!("Missing event at sequence {}", seq)),
                    }),
                };

                // Verify prev_hash links
                if event.prev_hash != prev_hash {
                    return Ok(ChainVerification {
                        is_valid: false,
                        length: meta.next_sequence,
                        first_invalid: Some(seq),
                        error: Some(format!("prev_hash mismatch at sequence {}", seq)),
                    });
                }

                // Verify computed hash
                let computed = compute_event_hash(
                    event.sequence,
                    &event.event_type,
                    &event.payload,
                    event.timestamp,
                    &event.prev_hash,
                );

                if computed != event.hash {
                    return Ok(ChainVerification {
                        is_valid: false,
                        length: meta.next_sequence,
                        first_invalid: Some(seq),
                        error: Some(format!("Hash mismatch at sequence {}", seq)),
                    });
                }

                prev_hash = event.hash;
            }

            Ok(ChainVerification {
                is_valid: true,
                length: meta.next_sequence,
                first_invalid: None,
                error: None,
            })
        })
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 177
```

---

## Story #178: EventLog Query by Type

**GitHub Issue**: [#178](https://github.com/anibjoshi/in-mem/issues/178)
**Estimated Time**: 3 hours
**Dependencies**: Stories #174, #175

### Implementation

```rust
impl EventLog {
    /// Read events filtered by type
    pub fn read_by_type(&self, run_id: &RunId, event_type: &str) -> Result<Vec<Event>> {
        self.db.transaction(run_id, |txn| {
            let ns = self.namespace_for_run(run_id);
            let meta_key = Key::new_event_meta(ns.clone());

            let meta: EventLogMeta = match txn.get(&meta_key)? {
                Some(v) => serde_json::from_value(v.into_json()?)?,
                None => return Ok(Vec::new()),
            };

            let mut filtered = Vec::new();
            for seq in 0..meta.next_sequence {
                let event_key = Key::new_event(ns.clone(), seq);
                if let Some(v) = txn.get(&event_key)? {
                    let event: Event = serde_json::from_value(v.into_json()?)?;
                    if event.event_type == event_type {
                        filtered.push(event);
                    }
                }
            }

            Ok(filtered)
        })
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 178
```

---

## Story #179: EventLogExt Transaction Extension

**GitHub Issue**: [#179](https://github.com/anibjoshi/in-mem/issues/179)
**Estimated Time**: 3 hours
**Dependencies**: Stories #174-#178

### Implementation

```rust
use crate::extensions::EventLogExt;

impl EventLogExt for TransactionContext {
    fn event_append(&mut self, event_type: &str, payload: Value) -> Result<u64> {
        // Implementation similar to EventLog::append but operates on txn directly
        // Note: Must maintain hash chain within the transaction
        todo!("Implement in Story #179")
    }

    fn event_read(&mut self, sequence: u64) -> Result<Option<Value>> {
        let ns = self.namespace().clone();
        let event_key = Key::new_event(ns, sequence);
        self.get(&event_key)
    }
}
```

Update `crates/primitives/src/lib.rs`:

```rust
pub mod event_log;
pub use event_log::{EventLog, Event, ChainVerification};
```

### Complete Story

```bash
./scripts/complete-story.sh 179
```

---

## Epic 15 Completion Checklist

### Verify Deliverables

- [ ] EventLog struct is stateless
- [ ] Event struct has all required fields
- [ ] Hash chain links events correctly
- [ ] Append-only invariant enforced (no update/delete methods)
- [ ] verify_chain() validates integrity
- [ ] Run isolation maintained
- [ ] All tests pass

### Merge and Close

```bash
git checkout develop
git merge --no-ff epic-15-eventlog-primitive -m "Epic 15: EventLog Primitive

Complete:
- EventLog stateless facade
- Event structure with causal hash chaining
- Append with automatic sequence and hash
- Read operations (single, range, head, len)
- Chain verification
- Query by type
- EventLogExt transaction extension

Stories: #174, #175, #176, #177, #178, #179
"

/opt/homebrew/bin/gh issue close 161 --comment "Epic 15: EventLog Primitive - COMPLETE"
```

---

## Summary

Epic 15 implements the EventLog primitive - an append-only event stream with causal hash chaining. Key design decisions:
- Single-writer-ordered (all appends serialize)
- Hash chaining is causal, not cryptographic
- Append-only invariant strictly enforced
