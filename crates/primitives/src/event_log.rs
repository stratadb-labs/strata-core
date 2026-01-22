//! EventLog: Immutable append-only event stream primitive
//!
//! ## Design Principles
//!
//! 1. **Single-Writer-Ordered**: All appends serialize through CAS on metadata key.
//!    Parallel append is NOT supported - event ordering must be total within a run.
//!
//! 2. **Causal Hash Chaining**: Each event includes hash of previous event.
//!    Provides tamper-evidence within process, NOT cryptographic security.
//!
//! 3. **Append-Only**: No update or delete operations - events are immutable.
//!
//! ## Hash Chain
//!
//! The hash chain provides tamper-evidence within process boundary, NOT
//! cryptographic security. Uses `DefaultHasher` padded to 32 bytes for
//! future SHA-256 upgrade path.
//!
//! ## Key Design
//!
//! - TypeTag: Event (0x02)
//! - Event key: `<namespace>:<TypeTag::Event>:<sequence_be_bytes>`
//! - Metadata key: `<namespace>:<TypeTag::Event>:__meta__`

use crate::extensions::EventLogExt;
use strata_concurrency::TransactionContext;
use strata_core::contract::{Timestamp, Version, Versioned};
use strata_core::error::Result;
use strata_core::types::{Key, Namespace, RunId};
use strata_core::value::Value;
use strata_engine::{Database, RetryConfig};
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

// Re-export Event and ChainVerification from core
pub use strata_core::primitives::{ChainVerification, Event};

/// EventLog metadata stored per run
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub(crate) struct EventLogMeta {
    pub next_sequence: u64,
    pub head_hash: [u8; 32],
}

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
    serde_json::to_string(payload)
        .unwrap_or_default()
        .hash(&mut hasher);
    timestamp.hash(&mut hasher);
    prev_hash.hash(&mut hasher);

    // Convert u64 to [u8; 32] (padded for future SHA-256)
    let h = hasher.finish();
    let mut result = [0u8; 32];
    result[0..8].copy_from_slice(&h.to_le_bytes());
    result
}

/// Serialize a struct to Value::String for storage
fn to_stored_value<T: Serialize>(v: &T) -> Value {
    match serde_json::to_string(v) {
        Ok(s) => Value::String(s),
        Err(_) => Value::Null,
    }
}

/// Deserialize from Value::String storage
fn from_stored_value<T: for<'de> Deserialize<'de>>(
    v: &Value,
) -> std::result::Result<T, serde_json::Error> {
    match v {
        Value::String(s) => serde_json::from_str(s),
        _ => serde_json::from_str("null"), // Will fail with appropriate error
    }
}

/// Immutable append-only event stream
///
/// DESIGN: Single-writer-ordered per run.
/// All appends serialize through CAS on metadata key.
///
/// # Example
///
/// ```ignore
/// use strata_primitives::EventLog;
/// use strata_engine::Database;
/// use strata_core::types::RunId;
/// use strata_core::value::Value;
///
/// let db = Arc::new(Database::open("/path/to/data")?);
/// let log = EventLog::new(db);
/// let run_id = RunId::new();
///
/// // Append events
/// let (seq, hash) = log.append(&run_id, "tool_call", Value::String("search".into()))?;
///
/// // Read events
/// let event = log.read(&run_id, seq)?;
///
/// // Verify chain
/// let verification = log.verify_chain(&run_id)?;
/// assert!(verification.is_valid);
/// ```
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
        Namespace::for_run(*run_id)
    }

    // ========== Append Operation (Story #175) ==========

    /// Append a new event to the log
    ///
    /// Returns the assigned sequence version.
    /// Serializes through CAS on metadata key - parallel appends will retry
    /// automatically with exponential backoff.
    ///
    /// # Arguments
    /// * `run_id` - The run to append to
    /// * `event_type` - User-defined event category
    /// * `payload` - Event data
    ///
    /// # Returns
    /// Version::Sequence containing the assigned sequence number
    pub fn append(
        &self,
        run_id: &RunId,
        event_type: &str,
        payload: Value,
    ) -> Result<Version> {
        // Use high retry count for contention scenarios
        // EventLog appends serialize through metadata CAS, so conflicts are expected
        // With N concurrent threads, worst case needs N retries per append
        // 200 retries with fast backoff handles 100+ concurrent threads reliably
        let retry_config = RetryConfig::default()
            .with_max_retries(200)
            .with_base_delay_ms(1)
            .with_max_delay_ms(50);

        let ns = self.namespace_for_run(run_id);
        let event_type_owned = event_type.to_string();

        self.db
            .transaction_with_retry(*run_id, retry_config, |txn| {
                // Read current metadata (or default)
                let meta_key = Key::new_event_meta(ns.clone());
                let meta: EventLogMeta = match txn.get(&meta_key)? {
                    Some(v) => from_stored_value(&v).unwrap_or_else(|_| EventLogMeta::default()),
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
                    &event_type_owned,
                    &payload,
                    timestamp,
                    &meta.head_hash,
                );

                // Build event
                let event = Event {
                    sequence,
                    event_type: event_type_owned.clone(),
                    payload: payload.clone(),
                    timestamp,
                    prev_hash: meta.head_hash,
                    hash,
                };

                // Write event
                let event_key = Key::new_event(ns.clone(), sequence);
                txn.put(event_key, to_stored_value(&event))?;

                // Update metadata (CAS semantics through transaction)
                let new_meta = EventLogMeta {
                    next_sequence: sequence + 1,
                    head_hash: hash,
                };
                txn.put(meta_key, to_stored_value(&new_meta))?;

                Ok(Version::Sequence(sequence))
            })
    }

    // ========== Read Operations (Story #176) ==========

    /// Read a single event by sequence number (FAST PATH)
    ///
    /// Bypasses full transaction overhead for read-only access.
    /// Uses direct snapshot read which maintains snapshot isolation.
    /// Returns Versioned<Event> if found.
    pub fn read(&self, run_id: &RunId, sequence: u64) -> Result<Option<Versioned<Event>>> {
        use strata_core::traits::SnapshotView;

        let snapshot = self.db.storage().create_snapshot();
        let ns = self.namespace_for_run(run_id);
        let event_key = Key::new_event(ns, sequence);

        match snapshot.get(&event_key)? {
            Some(vv) => {
                let event: Event = from_stored_value(&vv.value)
                    .map_err(|e| strata_core::error::Error::SerializationError(e.to_string()))?;
                Ok(Some(Versioned::with_timestamp(
                    event.clone(),
                    Version::Sequence(sequence),
                    Timestamp::from_millis(event.timestamp as u64),
                )))
            }
            None => Ok(None),
        }
    }

    /// Read a single event by sequence number (with full transaction)
    ///
    /// Use this when you need transaction semantics (e.g., consistent multi-read).
    /// Returns Versioned<Event> if found.
    pub fn read_in_transaction(&self, run_id: &RunId, sequence: u64) -> Result<Option<Versioned<Event>>> {
        self.db.transaction(*run_id, |txn| {
            let ns = self.namespace_for_run(run_id);
            let event_key = Key::new_event(ns, sequence);

            match txn.get(&event_key)? {
                Some(v) => {
                    let event: Event = from_stored_value(&v).map_err(|e| {
                        strata_core::error::Error::SerializationError(e.to_string())
                    })?;
                    Ok(Some(Versioned::with_timestamp(
                        event.clone(),
                        Version::Sequence(sequence),
                        Timestamp::from_millis(event.timestamp as u64),
                    )))
                }
                None => Ok(None),
            }
        })
    }

    /// Read a range of events [start, end)
    ///
    /// Returns Vec<Versioned<Event>> for the range.
    pub fn read_range(&self, run_id: &RunId, start: u64, end: u64) -> Result<Vec<Versioned<Event>>> {
        self.db.transaction(*run_id, |txn| {
            let mut events = Vec::new();
            let ns = self.namespace_for_run(run_id);

            for seq in start..end {
                let event_key = Key::new_event(ns.clone(), seq);
                if let Some(v) = txn.get(&event_key)? {
                    let event: Event = from_stored_value(&v).map_err(|e| {
                        strata_core::error::Error::SerializationError(e.to_string())
                    })?;
                    events.push(Versioned::with_timestamp(
                        event.clone(),
                        Version::Sequence(seq),
                        Timestamp::from_millis(event.timestamp as u64),
                    ));
                }
            }

            Ok(events)
        })
    }

    /// Get the latest event (head of the log)
    ///
    /// Returns Versioned<Event> if the log is not empty.
    pub fn head(&self, run_id: &RunId) -> Result<Option<Versioned<Event>>> {
        self.db.transaction(*run_id, |txn| {
            let ns = self.namespace_for_run(run_id);
            let meta_key = Key::new_event_meta(ns.clone());

            let meta: EventLogMeta = match txn.get(&meta_key)? {
                Some(v) => from_stored_value(&v).unwrap_or_else(|_| EventLogMeta::default()),
                None => return Ok(None),
            };

            if meta.next_sequence == 0 {
                return Ok(None);
            }

            let sequence = meta.next_sequence - 1;
            let event_key = Key::new_event(ns, sequence);
            match txn.get(&event_key)? {
                Some(v) => {
                    let event: Event = from_stored_value(&v).map_err(|e| {
                        strata_core::error::Error::SerializationError(e.to_string())
                    })?;
                    Ok(Some(Versioned::with_timestamp(
                        event.clone(),
                        Version::Sequence(sequence),
                        Timestamp::from_millis(event.timestamp as u64),
                    )))
                }
                None => Ok(None),
            }
        })
    }

    /// Get the current length of the log (FAST PATH)
    ///
    /// Bypasses full transaction overhead for read-only access.
    pub fn len(&self, run_id: &RunId) -> Result<u64> {
        use strata_core::traits::SnapshotView;

        let snapshot = self.db.storage().create_snapshot();
        let ns = self.namespace_for_run(run_id);
        let meta_key = Key::new_event_meta(ns);

        let meta: EventLogMeta = match snapshot.get(&meta_key)? {
            Some(vv) => from_stored_value(&vv.value).unwrap_or_else(|_| EventLogMeta::default()),
            None => EventLogMeta::default(),
        };

        Ok(meta.next_sequence)
    }

    /// Check if log is empty (FAST PATH)
    pub fn is_empty(&self, run_id: &RunId) -> Result<bool> {
        Ok(self.len(run_id)? == 0)
    }

    // ========== Chain Verification (Story #177) ==========

    /// Verify chain integrity from start to end
    ///
    /// Validates:
    /// 1. All events exist (no gaps)
    /// 2. Each event's prev_hash matches previous event's hash
    /// 3. Each event's hash is correctly computed
    pub fn verify_chain(&self, run_id: &RunId) -> Result<ChainVerification> {
        self.db.transaction(*run_id, |txn| {
            let ns = self.namespace_for_run(run_id);
            let meta_key = Key::new_event_meta(ns.clone());

            let meta: EventLogMeta = match txn.get(&meta_key)? {
                Some(v) => from_stored_value(&v).unwrap_or_else(|_| EventLogMeta::default()),
                None => {
                    return Ok(ChainVerification {
                        is_valid: true,
                        length: 0,
                        first_invalid: None,
                        error: None,
                    })
                }
            };

            let mut prev_hash = [0u8; 32]; // Genesis

            for seq in 0..meta.next_sequence {
                let event_key = Key::new_event(ns.clone(), seq);
                let event: Event = match txn.get(&event_key)? {
                    Some(v) => from_stored_value(&v).map_err(|e| {
                        strata_core::error::Error::SerializationError(e.to_string())
                    })?,
                    None => {
                        return Ok(ChainVerification {
                            is_valid: false,
                            length: meta.next_sequence,
                            first_invalid: Some(seq),
                            error: Some(format!("Missing event at sequence {}", seq)),
                        })
                    }
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

    // ========== Query by Type (Story #178) ==========

    /// Read events filtered by type
    ///
    /// Returns Vec<Versioned<Event>> for events matching the type.
    pub fn read_by_type(&self, run_id: &RunId, event_type: &str) -> Result<Vec<Versioned<Event>>> {
        self.db.transaction(*run_id, |txn| {
            let ns = self.namespace_for_run(run_id);
            let meta_key = Key::new_event_meta(ns.clone());

            let meta: EventLogMeta = match txn.get(&meta_key)? {
                Some(v) => from_stored_value(&v).unwrap_or_else(|_| EventLogMeta::default()),
                None => return Ok(Vec::new()),
            };

            let mut filtered = Vec::new();
            for seq in 0..meta.next_sequence {
                let event_key = Key::new_event(ns.clone(), seq);
                if let Some(v) = txn.get(&event_key)? {
                    let event: Event = from_stored_value(&v).map_err(|e| {
                        strata_core::error::Error::SerializationError(e.to_string())
                    })?;
                    if event.event_type == event_type {
                        filtered.push(Versioned::with_timestamp(
                            event.clone(),
                            Version::Sequence(seq),
                            Timestamp::from_millis(event.timestamp as u64),
                        ));
                    }
                }
            }

            Ok(filtered)
        })
    }

    /// Get all distinct event types in the log
    pub fn event_types(&self, run_id: &RunId) -> Result<Vec<String>> {
        self.db.transaction(*run_id, |txn| {
            let ns = self.namespace_for_run(run_id);
            let meta_key = Key::new_event_meta(ns.clone());

            let meta: EventLogMeta = match txn.get(&meta_key)? {
                Some(v) => from_stored_value(&v).unwrap_or_else(|_| EventLogMeta::default()),
                None => return Ok(Vec::new()),
            };

            let mut types = std::collections::HashSet::new();
            for seq in 0..meta.next_sequence {
                let event_key = Key::new_event(ns.clone(), seq);
                if let Some(v) = txn.get(&event_key)? {
                    let event: Event = from_stored_value(&v).map_err(|e| {
                        strata_core::error::Error::SerializationError(e.to_string())
                    })?;
                    types.insert(event.event_type);
                }
            }

            Ok(types.into_iter().collect())
        })
    }

    // ========== Search API (M6) ==========

    /// Search events
    ///
    /// Searches event type and payload. Respects budget constraints.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use strata_core::SearchRequest;
    ///
    /// let response = log.search(&SearchRequest::new(run_id, "error"))?;
    /// for hit in response.hits {
    ///     println!("Found event {:?} with score {}", hit.doc_ref, hit.score);
    /// }
    /// ```
    pub fn search(
        &self,
        req: &strata_core::SearchRequest,
    ) -> strata_core::error::Result<strata_core::SearchResponse> {
        use crate::searchable::{build_search_response, SearchCandidate};
        use strata_core::search_types::DocRef;
        use strata_core::traits::SnapshotView;
        use std::time::Instant;

        let start = Instant::now();
        let snapshot = self.db.storage().create_snapshot();
        let ns = self.namespace_for_run(&req.run_id);

        // Get metadata to know how many events exist
        let meta_key = Key::new_event_meta(ns.clone());
        let meta: EventLogMeta = match snapshot.get(&meta_key)? {
            Some(vv) => from_stored_value(&vv.value).unwrap_or_default(),
            None => return Ok(strata_core::SearchResponse::empty()),
        };

        let mut candidates = Vec::new();
        let mut truncated = false;

        // Scan all events
        for seq in 0..meta.next_sequence {
            // Check budget constraints
            if start.elapsed().as_micros() as u64 >= req.budget.max_wall_time_micros {
                truncated = true;
                break;
            }
            if candidates.len() >= req.budget.max_candidates_per_primitive {
                truncated = true;
                break;
            }

            let event_key = Key::new_event(ns.clone(), seq);
            if let Some(vv) = snapshot.get(&event_key)? {
                let event: Event = match from_stored_value(&vv.value) {
                    Ok(e) => e,
                    Err(_) => continue,
                };

                // Time range filter
                if let Some((start_ts, end_ts)) = req.time_range {
                    let ts = event.timestamp as u64;
                    if ts < start_ts || ts > end_ts {
                        continue;
                    }
                }

                // Extract searchable text: event type + payload
                let text = self.extract_event_text(&event);

                candidates.push(SearchCandidate::new(
                    DocRef::Event {
                        run_id: req.run_id,
                        sequence: seq,
                    },
                    text,
                    Some(event.timestamp as u64),
                ));
            }
        }

        Ok(build_search_response(
            candidates,
            &req.query,
            req.k,
            truncated,
            start.elapsed().as_micros() as u64,
        ))
    }

    /// Extract searchable text from an event
    fn extract_event_text(&self, event: &Event) -> String {
        let mut parts = vec![event.event_type.clone()];
        if let Ok(s) = serde_json::to_string(&event.payload) {
            parts.push(s);
        }
        parts.join(" ")
    }
}

// ========== Searchable Trait Implementation (M6) ==========

impl crate::searchable::Searchable for EventLog {
    fn search(
        &self,
        req: &strata_core::SearchRequest,
    ) -> strata_core::error::Result<strata_core::SearchResponse> {
        self.search(req)
    }

    fn primitive_kind(&self) -> strata_core::PrimitiveType {
        strata_core::PrimitiveType::Event
    }
}

// ========== EventLogExt Implementation (Story #179) ==========

impl EventLogExt for TransactionContext {
    fn event_append(&mut self, event_type: &str, payload: Value) -> Result<u64> {
        let ns = Namespace::for_run(self.run_id);

        // Read current metadata (or default)
        let meta_key = Key::new_event_meta(ns.clone());
        let meta: EventLogMeta = match self.get(&meta_key)? {
            Some(v) => from_stored_value(&v).unwrap_or_else(|_| EventLogMeta::default()),
            None => EventLogMeta::default(),
        };

        // Compute event hash
        let sequence = meta.next_sequence;
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        let hash = compute_event_hash(sequence, event_type, &payload, timestamp, &meta.head_hash);

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
        self.put(event_key, to_stored_value(&event))?;

        // Update metadata
        let new_meta = EventLogMeta {
            next_sequence: sequence + 1,
            head_hash: hash,
        };
        self.put(meta_key, to_stored_value(&new_meta))?;

        Ok(sequence)
    }

    fn event_read(&mut self, sequence: u64) -> Result<Option<Value>> {
        let ns = Namespace::for_run(self.run_id);
        let event_key = Key::new_event(ns, sequence);
        self.get(&event_key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Arc<Database>, EventLog) {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path()).unwrap());
        let log = EventLog::new(db.clone());
        (temp_dir, db, log)
    }

    // ========== Core Structure Tests (Story #174) ==========

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

    #[test]
    fn test_eventlog_creation() {
        let (_temp, _db, log) = setup();
        assert!(Arc::strong_count(log.database()) >= 1);
    }

    #[test]
    fn test_eventlog_is_clone() {
        let (_temp, _db, log1) = setup();
        let log2 = log1.clone();
        assert!(Arc::ptr_eq(log1.database(), log2.database()));
    }

    #[test]
    fn test_eventlog_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<EventLog>();
    }

    // ========== Append Tests (Story #175) ==========

    #[test]
    fn test_append_first_event() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        let version = log.append(&run_id, "test", Value::Null).unwrap();
        assert!(matches!(version, Version::Sequence(0)));
    }

    #[test]
    fn test_append_increments_sequence() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        let v1 = log.append(&run_id, "test", Value::Null).unwrap();
        let v2 = log.append(&run_id, "test", Value::Null).unwrap();
        let v3 = log.append(&run_id, "test", Value::Null).unwrap();

        assert!(matches!(v1, Version::Sequence(0)));
        assert!(matches!(v2, Version::Sequence(1)));
        assert!(matches!(v3, Version::Sequence(2)));
    }

    #[test]
    fn test_hash_chain_links() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        log.append(&run_id, "test", Value::Null).unwrap();
        let event1 = log.read(&run_id, 0).unwrap().unwrap();
        log.append(&run_id, "test", Value::Null).unwrap();

        // Verify chain through read
        let event2 = log.read(&run_id, 1).unwrap().unwrap();
        assert_eq!(event2.value.prev_hash, event1.value.hash);
    }

    #[test]
    fn test_append_with_payload() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        let payload = Value::Map(std::collections::HashMap::from([
            ("tool".to_string(), Value::String("search".into())),
            ("query".to_string(), Value::String("rust async".into())),
        ]));

        let version = log.append(&run_id, "tool_call", payload.clone()).unwrap();
        let seq = match version { Version::Sequence(s) => s, _ => panic!("Expected sequence") };
        let event = log.read(&run_id, seq).unwrap().unwrap();

        assert_eq!(event.value.event_type, "tool_call");
        assert_eq!(event.value.payload, payload);
    }

    #[test]
    fn test_run_isolation() {
        let (_temp, _db, log) = setup();
        let run1 = RunId::new();
        let run2 = RunId::new();

        log.append(&run1, "run1_event", Value::I64(1)).unwrap();
        log.append(&run1, "run1_event", Value::I64(2)).unwrap();
        log.append(&run2, "run2_event", Value::I64(100)).unwrap();

        assert_eq!(log.len(&run1).unwrap(), 2);
        assert_eq!(log.len(&run2).unwrap(), 1);

        let run1_events = log.read_range(&run1, 0, 10).unwrap();
        let run2_events = log.read_range(&run2, 0, 10).unwrap();

        assert_eq!(run1_events.len(), 2);
        assert_eq!(run2_events.len(), 1);
        assert_eq!(run2_events[0].value.event_type, "run2_event");
    }

    // ========== Read Tests (Story #176) ==========

    #[test]
    fn test_read_single_event() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        log.append(&run_id, "test", Value::String("data".into()))
            .unwrap();

        let versioned = log.read(&run_id, 0).unwrap().unwrap();
        assert_eq!(versioned.value.sequence, 0);
        assert_eq!(versioned.value.event_type, "test");
        assert_eq!(versioned.value.payload, Value::String("data".into()));
        assert!(matches!(versioned.version, Version::Sequence(0)));
    }

    #[test]
    fn test_read_nonexistent() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        let event = log.read(&run_id, 999).unwrap();
        assert!(event.is_none());
    }

    #[test]
    fn test_read_range() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        for i in 0..5 {
            log.append(&run_id, "test", Value::I64(i)).unwrap();
        }

        let events = log.read_range(&run_id, 1, 4).unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].value.sequence, 1);
        assert_eq!(events[1].value.sequence, 2);
        assert_eq!(events[2].value.sequence, 3);
        // Verify version matches sequence
        assert!(matches!(events[0].version, Version::Sequence(1)));
        assert!(matches!(events[1].version, Version::Sequence(2)));
        assert!(matches!(events[2].version, Version::Sequence(3)));
    }

    #[test]
    fn test_head() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        // Empty log
        assert!(log.head(&run_id).unwrap().is_none());

        // After appends
        log.append(&run_id, "first", Value::I64(1)).unwrap();
        log.append(&run_id, "second", Value::I64(2)).unwrap();
        log.append(&run_id, "third", Value::I64(3)).unwrap();

        let head = log.head(&run_id).unwrap().unwrap();
        assert_eq!(head.value.sequence, 2);
        assert_eq!(head.value.event_type, "third");
        assert!(matches!(head.version, Version::Sequence(2)));
    }

    #[test]
    fn test_len() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        assert_eq!(log.len(&run_id).unwrap(), 0);
        assert!(log.is_empty(&run_id).unwrap());

        log.append(&run_id, "test", Value::Null).unwrap();
        assert_eq!(log.len(&run_id).unwrap(), 1);
        assert!(!log.is_empty(&run_id).unwrap());

        log.append(&run_id, "test", Value::Null).unwrap();
        log.append(&run_id, "test", Value::Null).unwrap();
        assert_eq!(log.len(&run_id).unwrap(), 3);
    }

    // ========== Chain Verification Tests (Story #177) ==========

    #[test]
    fn test_verify_empty_chain() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        let verification = log.verify_chain(&run_id).unwrap();
        assert!(verification.is_valid);
        assert_eq!(verification.length, 0);
    }

    #[test]
    fn test_verify_valid_chain() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        for i in 0..10 {
            log.append(&run_id, "test", Value::I64(i)).unwrap();
        }

        let verification = log.verify_chain(&run_id).unwrap();
        assert!(verification.is_valid);
        assert_eq!(verification.length, 10);
        assert!(verification.first_invalid.is_none());
        assert!(verification.error.is_none());
    }

    #[test]
    fn test_chain_integrity_with_different_types() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        log.append(&run_id, "type_a", Value::String("data".into()))
            .unwrap();
        log.append(&run_id, "type_b", Value::I64(42)).unwrap();
        log.append(&run_id, "type_a", Value::Bool(true)).unwrap();

        let verification = log.verify_chain(&run_id).unwrap();
        assert!(verification.is_valid);
        assert_eq!(verification.length, 3);
    }

    // ========== Query by Type Tests (Story #178) ==========

    #[test]
    fn test_read_by_type() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        log.append(&run_id, "tool_call", Value::I64(1)).unwrap();
        log.append(&run_id, "tool_result", Value::I64(2)).unwrap();
        log.append(&run_id, "tool_call", Value::I64(3)).unwrap();
        log.append(&run_id, "thought", Value::I64(4)).unwrap();
        log.append(&run_id, "tool_call", Value::I64(5)).unwrap();

        let tool_calls = log.read_by_type(&run_id, "tool_call").unwrap();
        assert_eq!(tool_calls.len(), 3);
        assert_eq!(tool_calls[0].value.payload, Value::I64(1));
        assert_eq!(tool_calls[1].value.payload, Value::I64(3));
        assert_eq!(tool_calls[2].value.payload, Value::I64(5));

        let thoughts = log.read_by_type(&run_id, "thought").unwrap();
        assert_eq!(thoughts.len(), 1);

        let nonexistent = log.read_by_type(&run_id, "nonexistent").unwrap();
        assert!(nonexistent.is_empty());
    }

    #[test]
    fn test_event_types() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        log.append(&run_id, "type_a", Value::Null).unwrap();
        log.append(&run_id, "type_b", Value::Null).unwrap();
        log.append(&run_id, "type_a", Value::Null).unwrap();
        log.append(&run_id, "type_c", Value::Null).unwrap();

        let types = log.event_types(&run_id).unwrap();
        assert_eq!(types.len(), 3);
        assert!(types.contains(&"type_a".to_string()));
        assert!(types.contains(&"type_b".to_string()));
        assert!(types.contains(&"type_c".to_string()));
    }

    // ========== EventLogExt Tests (Story #179) ==========

    #[test]
    fn test_eventlog_ext_append() {
        use crate::extensions::EventLogExt;

        let (_temp, db, log) = setup();
        let run_id = RunId::new();

        // Append via extension trait
        db.transaction(run_id, |txn| {
            let seq = txn.event_append("ext_event", Value::String("test".into()))?;
            assert_eq!(seq, 0);
            Ok(())
        })
        .unwrap();

        // Verify via EventLog
        let versioned = log.read(&run_id, 0).unwrap().unwrap();
        assert_eq!(versioned.value.event_type, "ext_event");
    }

    #[test]
    fn test_eventlog_ext_read() {
        use crate::extensions::EventLogExt;

        let (_temp, db, log) = setup();
        let run_id = RunId::new();

        // Append via EventLog
        log.append(&run_id, "test", Value::I64(42)).unwrap();

        // Read via extension trait
        db.transaction(run_id, |txn| {
            let value = txn.event_read(0)?;
            assert!(value.is_some());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_cross_primitive_transaction() {
        use crate::extensions::{EventLogExt, KVStoreExt};

        let (_temp, db, _log) = setup();
        let run_id = RunId::new();

        // Atomic: KV put + event append
        db.transaction(run_id, |txn| {
            txn.kv_put("key", Value::String("value".into()))?;
            txn.event_append("kv_updated", Value::String("key".into()))?;
            Ok(())
        })
        .unwrap();

        // Verify both operations committed
        db.transaction(run_id, |txn| {
            let kv_val = txn.kv_get("key")?;
            assert_eq!(kv_val, Some(Value::String("value".into())));

            let event_val = txn.event_read(0)?;
            assert!(event_val.is_some());
            Ok(())
        })
        .unwrap();
    }

    // ========== Fast Path Tests (Story #238) ==========

    #[test]
    fn test_fast_read_returns_correct_value() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        log.append(&run_id, "test", Value::String("data".into()))
            .unwrap();

        let versioned = log.read(&run_id, 0).unwrap().unwrap();
        assert_eq!(versioned.value.event_type, "test");
        assert_eq!(versioned.value.payload, Value::String("data".into()));
    }

    #[test]
    fn test_fast_read_returns_none_for_missing() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        let event = log.read(&run_id, 999).unwrap();
        assert!(event.is_none());
    }

    #[test]
    fn test_fast_read_equals_transaction_read() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        log.append(&run_id, "test", Value::I64(42)).unwrap();

        let fast = log.read(&run_id, 0).unwrap();
        let txn = log.read_in_transaction(&run_id, 0).unwrap();

        // Both should return Versioned<Event> with same value
        assert_eq!(fast.as_ref().map(|v| &v.value), txn.as_ref().map(|v| &v.value));
    }

    #[test]
    fn test_fast_len_returns_correct_count() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        assert_eq!(log.len(&run_id).unwrap(), 0);

        log.append(&run_id, "test", Value::Null).unwrap();
        assert_eq!(log.len(&run_id).unwrap(), 1);

        log.append(&run_id, "test", Value::Null).unwrap();
        log.append(&run_id, "test", Value::Null).unwrap();
        assert_eq!(log.len(&run_id).unwrap(), 3);
    }

    #[test]
    fn test_fast_read_run_isolation() {
        let (_temp, _db, log) = setup();
        let run1 = RunId::new();
        let run2 = RunId::new();

        log.append(&run1, "run1", Value::I64(1)).unwrap();
        log.append(&run2, "run2", Value::I64(2)).unwrap();

        // Each run sees only its own events
        let event1 = log.read(&run1, 0).unwrap().unwrap();
        let event2 = log.read(&run2, 0).unwrap().unwrap();

        assert_eq!(event1.value.event_type, "run1");
        assert_eq!(event2.value.event_type, "run2");

        // Cross-run reads return None
        assert!(log.read(&run1, 1).unwrap().is_none());
    }
}
