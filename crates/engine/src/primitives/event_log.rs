//! EventLog: Immutable append-only event stream primitive
//!
//! ## Role: Determinism Boundary Recorder
//!
//! EventLog records nondeterministic external inputs that cross the determinism boundary.
//! Its purpose is to enable deterministic replay of agent runs by capturing exactly
//! the information needed to reproduce nondeterministic behavior.
//!
//! Key invariant: If an operation's result is NOT recorded in EventLog, that operation
//! MUST be deterministic given the current state.
//!
//! ## Design Principles
//!
//! 1. **Single-Writer-Ordered**: All appends serialize through CAS on metadata key.
//!    Parallel append is NOT supported - event ordering must be total within a run.
//!
//! 2. **Causal Hash Chaining**: Each event includes SHA-256 hash of previous event.
//!    Provides tamper-evidence and deterministic verification.
//!
//! 3. **Append-Only**: No update or delete operations - events are immutable.
//!
//! 4. **Object-Only Payloads**: All payloads must be JSON objects (not primitives/arrays).
//!
//! 5. **Global Sequences**: Streams are filters over a single global sequence per run.
//!
//! ## Hash Chain
//!
//! Uses SHA-256 for deterministic cross-platform hashing. Hash version 1 computes:
//! SHA256(sequence || event_type_len || event_type || timestamp || payload_len || payload || prev_hash)
//!
//! ## Key Design
//!
//! - TypeTag: Event (0x02)
//! - Event key: `<namespace>:<TypeTag::Event>:<sequence_be_bytes>`
//! - Metadata key: `<namespace>:<TypeTag::Event>:__meta__`

use crate::primitives::extensions::EventLogExt;
use sha2::{Digest, Sha256};
use strata_concurrency::TransactionContext;
use strata_core::contract::{Timestamp, Version, Versioned};
use strata_core::error::Result;
use strata_core::StrataError;
use strata_core::types::{Key, Namespace, RunId};
use strata_core::value::Value;
use crate::database::{Database, RetryConfig};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

// Re-export Event and ChainVerification from core
pub use strata_core::primitives::{ChainVerification, Event};

/// Hash version constants
const HASH_VERSION_LEGACY: u8 = 0; // DefaultHasher (deprecated, for migration)
const HASH_VERSION_SHA256: u8 = 1; // SHA-256 (current)

/// Per-stream metadata for O(1) access to stream statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamMeta {
    /// Number of events in this stream
    pub count: u64,
    /// First sequence number in this stream (global sequence)
    pub first_sequence: u64,
    /// Last sequence number in this stream (global sequence)
    pub last_sequence: u64,
    /// Timestamp of first event in stream (microseconds since epoch)
    pub first_timestamp: u64,
    /// Timestamp of last event in stream (microseconds since epoch)
    pub last_timestamp: u64,
}

impl StreamMeta {
    fn new(sequence: u64, timestamp: u64) -> Self {
        Self {
            count: 1,
            first_sequence: sequence,
            last_sequence: sequence,
            first_timestamp: timestamp,
            last_timestamp: timestamp,
        }
    }

    fn update(&mut self, sequence: u64, timestamp: u64) {
        self.count += 1;
        self.last_sequence = sequence;
        self.last_timestamp = timestamp;
    }
}

/// EventLog metadata stored per run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct EventLogMeta {
    /// Next sequence number to assign
    pub next_sequence: u64,
    /// Hash of the last event (head of chain)
    pub head_hash: [u8; 32],
    /// Hash algorithm version (0 = legacy DefaultHasher, 1 = SHA-256)
    #[serde(default)]
    pub hash_version: u8,
    /// Per-stream metadata for O(1) stream queries
    #[serde(default)]
    pub streams: HashMap<String, StreamMeta>,
}

impl Default for EventLogMeta {
    fn default() -> Self {
        Self {
            next_sequence: 0,
            head_hash: [0u8; 32],
            hash_version: HASH_VERSION_SHA256, // New logs use SHA-256
            streams: HashMap::new(),
        }
    }
}

/// Compute event hash using SHA-256 (version 1)
///
/// Deterministic across platforms and Rust versions.
/// Format: SHA256(sequence || event_type_len || event_type || timestamp || payload_len || payload || prev_hash)
fn compute_event_hash_v1(
    sequence: u64,
    event_type: &str,
    payload: &Value,
    timestamp: u64,
    prev_hash: &[u8; 32],
) -> [u8; 32] {
    let mut hasher = Sha256::new();

    // Sequence (8 bytes, little-endian)
    hasher.update(&sequence.to_le_bytes());

    // Event type with length prefix (4 bytes length + content)
    hasher.update(&(event_type.len() as u32).to_le_bytes());
    hasher.update(event_type.as_bytes());

    // Timestamp (8 bytes, little-endian)
    hasher.update(&timestamp.to_le_bytes());

    // Payload as canonical JSON with length prefix
    let payload_bytes = serde_json::to_vec(payload).unwrap_or_default();
    hasher.update(&(payload_bytes.len() as u32).to_le_bytes());
    hasher.update(&payload_bytes);

    // Previous hash (32 bytes)
    hasher.update(prev_hash);

    hasher.finalize().into()
}

/// Compute event hash using legacy DefaultHasher (version 0)
///
/// DEPRECATED: Only used for verifying events created before SHA-256 migration.
/// New events should always use compute_event_hash_v1.
#[allow(dead_code)]
fn compute_event_hash_v0(
    sequence: u64,
    event_type: &str,
    payload: &Value,
    timestamp: u64,
    prev_hash: &[u8; 32],
) -> [u8; 32] {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    sequence.hash(&mut hasher);
    event_type.hash(&mut hasher);
    serde_json::to_string(payload)
        .unwrap_or_default()
        .hash(&mut hasher);
    timestamp.hash(&mut hasher);
    prev_hash.hash(&mut hasher);

    let h = hasher.finish();
    let mut result = [0u8; 32];
    result[0..8].copy_from_slice(&h.to_le_bytes());
    result
}

/// Compute event hash using the specified version
fn compute_event_hash(
    hash_version: u8,
    sequence: u64,
    event_type: &str,
    payload: &Value,
    timestamp: u64,
    prev_hash: &[u8; 32],
) -> [u8; 32] {
    match hash_version {
        HASH_VERSION_LEGACY => compute_event_hash_v0(sequence, event_type, payload, timestamp, prev_hash),
        _ => compute_event_hash_v1(sequence, event_type, payload, timestamp, prev_hash),
    }
}

/// Validation error for EventLog operations
#[derive(Debug, Clone, PartialEq)]
pub enum EventLogValidationError {
    /// Payload must be an object, not a primitive or array
    PayloadNotObject,
    /// Payload contains NaN or Infinity which are not valid JSON
    PayloadContainsNonFiniteFloat,
    /// Event type cannot be empty
    EmptyEventType,
    /// Event type cannot exceed maximum length
    EventTypeTooLong(usize),
}

impl std::fmt::Display for EventLogValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PayloadNotObject => write!(f, "payload must be a JSON object"),
            Self::PayloadContainsNonFiniteFloat => write!(f, "payload contains NaN or Infinity"),
            Self::EmptyEventType => write!(f, "event_type cannot be empty"),
            Self::EventTypeTooLong(len) => write!(f, "event_type exceeds maximum length ({})", len),
        }
    }
}

/// Maximum allowed event type length
const MAX_EVENT_TYPE_LENGTH: usize = 256;

/// Validate event type
fn validate_event_type(event_type: &str) -> std::result::Result<(), EventLogValidationError> {
    if event_type.is_empty() {
        return Err(EventLogValidationError::EmptyEventType);
    }
    if event_type.len() > MAX_EVENT_TYPE_LENGTH {
        return Err(EventLogValidationError::EventTypeTooLong(event_type.len()));
    }
    Ok(())
}

/// Validate payload is an object and contains no non-finite floats
fn validate_payload(payload: &Value) -> std::result::Result<(), EventLogValidationError> {
    // Payload must be an object
    if !matches!(payload, Value::Object(_)) {
        return Err(EventLogValidationError::PayloadNotObject);
    }

    // Check for non-finite floats recursively
    if contains_non_finite_float(payload) {
        return Err(EventLogValidationError::PayloadContainsNonFiniteFloat);
    }

    Ok(())
}

/// Check if a Value contains NaN or Infinity
fn contains_non_finite_float(value: &Value) -> bool {
    match value {
        Value::Float(f) => !f.is_finite(),
        Value::Object(map) => map.values().any(contains_non_finite_float),
        Value::Array(arr) => arr.iter().any(contains_non_finite_float),
        _ => false,
    }
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

    // ========== Append Operation ==========

    /// Append a new event to the log
    ///
    /// Returns the assigned sequence version.
    /// Serializes through CAS on metadata key - parallel appends will retry
    /// automatically with exponential backoff.
    ///
    /// # Arguments
    /// * `run_id` - The run to append to
    /// * `event_type` - User-defined event category (non-empty, max 256 chars)
    /// * `payload` - Event data (must be a JSON object, no NaN/Infinity)
    ///
    /// # Returns
    /// Version::Sequence containing the assigned sequence number
    ///
    /// # Errors
    /// Returns error if:
    /// - `event_type` is empty or exceeds 256 characters
    /// - `payload` is not a JSON object
    /// - `payload` contains NaN or Infinity float values
    pub fn append(
        &self,
        run_id: &RunId,
        event_type: &str,
        payload: Value,
    ) -> Result<Version> {
        // Validate inputs before entering transaction
        validate_event_type(event_type)
            .map_err(|e| StrataError::invalid_input(e.to_string()))?;
        validate_payload(&payload)
            .map_err(|e| StrataError::invalid_input(e.to_string()))?;

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
                let mut meta: EventLogMeta = match txn.get(&meta_key)? {
                    Some(v) => from_stored_value(&v).unwrap_or_else(|_| EventLogMeta::default()),
                    None => EventLogMeta::default(),
                };

                // Compute event hash using current hash version
                let sequence = meta.next_sequence;
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_micros() as u64;

                let hash = compute_event_hash(
                    meta.hash_version,
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

                // Update stream metadata
                match meta.streams.get_mut(&event_type_owned) {
                    Some(stream_meta) => stream_meta.update(sequence, timestamp),
                    None => {
                        meta.streams.insert(
                            event_type_owned.clone(),
                            StreamMeta::new(sequence, timestamp),
                        );
                    }
                }

                // Update metadata (CAS semantics through transaction)
                meta.next_sequence = sequence + 1;
                meta.head_hash = hash;
                txn.put(meta_key, to_stored_value(&meta))?;

                Ok(Version::Sequence(sequence))
            })
    }

    /// Append multiple events atomically in a single transaction
    ///
    /// All events are appended with consecutive sequence numbers.
    /// Either all succeed or none are written (atomic batch).
    ///
    /// # Arguments
    /// * `run_id` - The run to append to
    /// * `events` - List of (event_type, payload) pairs to append
    ///
    /// # Returns
    /// Vector of Version::Sequence for each appended event
    ///
    /// # Errors
    /// Returns error if any event fails validation. In this case, no events are written.
    pub fn append_batch(
        &self,
        run_id: &RunId,
        events: &[(&str, Value)],
    ) -> Result<Vec<Version>> {
        // Validate all inputs before entering transaction
        for (event_type, payload) in events {
            validate_event_type(event_type)
                .map_err(|e| StrataError::invalid_input(e.to_string()))?;
            validate_payload(payload)
                .map_err(|e| StrataError::invalid_input(e.to_string()))?;
        }

        if events.is_empty() {
            return Ok(vec![]);
        }

        let retry_config = RetryConfig::default()
            .with_max_retries(200)
            .with_base_delay_ms(1)
            .with_max_delay_ms(50);

        let ns = self.namespace_for_run(run_id);
        let events_owned: Vec<_> = events
            .iter()
            .map(|(et, p)| (et.to_string(), p.clone()))
            .collect();

        self.db
            .transaction_with_retry(*run_id, retry_config, |txn| {
                // Read current metadata
                let meta_key = Key::new_event_meta(ns.clone());
                let mut meta: EventLogMeta = match txn.get(&meta_key)? {
                    Some(v) => from_stored_value(&v).unwrap_or_else(|_| EventLogMeta::default()),
                    None => EventLogMeta::default(),
                };

                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_micros() as u64;

                let mut versions = Vec::with_capacity(events_owned.len());
                let mut prev_hash = meta.head_hash;

                for (event_type, payload) in &events_owned {
                    let sequence = meta.next_sequence;

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
                        event_type: event_type.clone(),
                        payload: payload.clone(),
                        timestamp,
                        prev_hash,
                        hash,
                    };

                    // Write event
                    let event_key = Key::new_event(ns.clone(), sequence);
                    txn.put(event_key, to_stored_value(&event))?;

                    // Update stream metadata
                    match meta.streams.get_mut(event_type) {
                        Some(stream_meta) => stream_meta.update(sequence, timestamp),
                        None => {
                            meta.streams.insert(
                                event_type.clone(),
                                StreamMeta::new(sequence, timestamp),
                            );
                        }
                    }

                    meta.next_sequence = sequence + 1;
                    versions.push(Version::Sequence(sequence));
                    prev_hash = hash;
                }

                // Update metadata with final state
                meta.head_hash = prev_hash;
                txn.put(meta_key, to_stored_value(&meta))?;

                Ok(versions)
            })
    }

    // ========== Read Operations ==========

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
                    .map_err(|e| strata_core::StrataError::serialization(e.to_string()))?;
                Ok(Some(Versioned::with_timestamp(
                    event.clone(),
                    Version::Sequence(sequence),
                    Timestamp::from_micros(event.timestamp),
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
                        strata_core::StrataError::serialization(e.to_string())
                    })?;
                    Ok(Some(Versioned::with_timestamp(
                        event.clone(),
                        Version::Sequence(sequence),
                        Timestamp::from_micros(event.timestamp),
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
                        strata_core::StrataError::serialization(e.to_string())
                    })?;
                    events.push(Versioned::with_timestamp(
                        event.clone(),
                        Version::Sequence(seq),
                        Timestamp::from_micros(event.timestamp),
                    ));
                }
            }

            Ok(events)
        })
    }

    /// Read a range of events in reverse order (newest first)
    ///
    /// Returns events from `start` (inclusive) down to `end` (inclusive),
    /// in descending sequence order.
    ///
    /// # Arguments
    /// * `run_id` - The run to read from
    /// * `start` - Higher sequence (inclusive), the starting point
    /// * `end` - Lower sequence (inclusive), the ending point
    ///
    /// # Returns
    /// Vec<Versioned<Event>> in descending sequence order (newest first)
    pub fn read_range_reverse(
        &self,
        run_id: &RunId,
        start: u64,
        end: u64,
    ) -> Result<Vec<Versioned<Event>>> {
        if start < end {
            return Ok(vec![]); // Invalid range
        }

        self.db.transaction(*run_id, |txn| {
            let mut events = Vec::new();
            let ns = self.namespace_for_run(run_id);

            // Iterate in reverse order (from start down to end)
            for seq in (end..=start).rev() {
                let event_key = Key::new_event(ns.clone(), seq);
                if let Some(v) = txn.get(&event_key)? {
                    let event: Event = from_stored_value(&v).map_err(|e| {
                        strata_core::StrataError::serialization(e.to_string())
                    })?;
                    events.push(Versioned::with_timestamp(
                        event.clone(),
                        Version::Sequence(seq),
                        Timestamp::from_micros(event.timestamp),
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
                        strata_core::StrataError::serialization(e.to_string())
                    })?;
                    Ok(Some(Versioned::with_timestamp(
                        event.clone(),
                        Version::Sequence(sequence),
                        Timestamp::from_micros(event.timestamp),
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

    // ========== Chain Verification ==========

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
                        strata_core::StrataError::serialization(e.to_string())
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

                // Verify computed hash using the log's hash version
                let computed = compute_event_hash(
                    meta.hash_version,
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

    // ========== O(1) Stream Query Methods ==========

    /// Get count of events for a specific event type (O(1))
    ///
    /// Returns the number of events with the given event type.
    /// This is O(1) because counts are tracked in metadata.
    pub fn len_by_type(&self, run_id: &RunId, event_type: &str) -> Result<u64> {
        use strata_core::traits::SnapshotView;

        let snapshot = self.db.storage().create_snapshot();
        let ns = self.namespace_for_run(run_id);
        let meta_key = Key::new_event_meta(ns);

        let meta: EventLogMeta = match snapshot.get(&meta_key)? {
            Some(vv) => from_stored_value(&vv.value).unwrap_or_else(|_| EventLogMeta::default()),
            None => EventLogMeta::default(),
        };

        Ok(meta.streams.get(event_type).map(|s| s.count).unwrap_or(0))
    }

    /// Get the latest sequence number for a specific event type (O(1))
    ///
    /// Returns the global sequence number of the most recent event of this type.
    /// Returns None if no events of this type exist.
    pub fn latest_sequence_by_type(&self, run_id: &RunId, event_type: &str) -> Result<Option<u64>> {
        use strata_core::traits::SnapshotView;

        let snapshot = self.db.storage().create_snapshot();
        let ns = self.namespace_for_run(run_id);
        let meta_key = Key::new_event_meta(ns);

        let meta: EventLogMeta = match snapshot.get(&meta_key)? {
            Some(vv) => from_stored_value(&vv.value).unwrap_or_else(|_| EventLogMeta::default()),
            None => EventLogMeta::default(),
        };

        Ok(meta.streams.get(event_type).map(|s| s.last_sequence))
    }

    /// Get stream metadata for a specific event type (O(1))
    ///
    /// Returns full stream statistics including count, first/last sequence,
    /// and first/last timestamps. Returns None if no events of this type exist.
    pub fn stream_info(&self, run_id: &RunId, event_type: &str) -> Result<Option<StreamMeta>> {
        use strata_core::traits::SnapshotView;

        let snapshot = self.db.storage().create_snapshot();
        let ns = self.namespace_for_run(run_id);
        let meta_key = Key::new_event_meta(ns);

        let meta: EventLogMeta = match snapshot.get(&meta_key)? {
            Some(vv) => from_stored_value(&vv.value).unwrap_or_else(|_| EventLogMeta::default()),
            None => EventLogMeta::default(),
        };

        Ok(meta.streams.get(event_type).cloned())
    }

    /// Get the head (latest) event for a specific event type
    ///
    /// Returns the most recent event of the given type. This is O(1) to find
    /// the sequence number (via metadata), then O(1) to read the event.
    pub fn head_by_type(&self, run_id: &RunId, event_type: &str) -> Result<Option<Versioned<Event>>> {
        let seq = match self.latest_sequence_by_type(run_id, event_type)? {
            Some(s) => s,
            None => return Ok(None),
        };

        self.read(run_id, seq)
    }

    /// Get all stream names that have events
    ///
    /// Returns the list of all event types that have at least one event.
    /// This is O(1) because stream names are tracked in metadata.
    pub fn stream_names(&self, run_id: &RunId) -> Result<Vec<String>> {
        use strata_core::traits::SnapshotView;

        let snapshot = self.db.storage().create_snapshot();
        let ns = self.namespace_for_run(run_id);
        let meta_key = Key::new_event_meta(ns);

        let meta: EventLogMeta = match snapshot.get(&meta_key)? {
            Some(vv) => from_stored_value(&vv.value).unwrap_or_else(|_| EventLogMeta::default()),
            None => EventLogMeta::default(),
        };

        Ok(meta.streams.keys().cloned().collect())
    }

    // ========== Query by Type ==========

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
                        strata_core::StrataError::serialization(e.to_string())
                    })?;
                    if event.event_type == event_type {
                        filtered.push(Versioned::with_timestamp(
                            event.clone(),
                            Version::Sequence(seq),
                            Timestamp::from_micros(event.timestamp),
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
                        strata_core::StrataError::serialization(e.to_string())
                    })?;
                    types.insert(event.event_type);
                }
            }

            Ok(types.into_iter().collect())
        })
    }

    // ========== Search API ==========

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
        use crate::primitives::searchable::{build_search_response, SearchCandidate};
        use strata_core::search_types::EntityRef;
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
                    EntityRef::Event {
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

// ========== Searchable Trait Implementation ==========

impl crate::primitives::searchable::Searchable for EventLog {
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

// ========== EventLogExt Implementation ==========

impl EventLogExt for TransactionContext {
    fn event_append(&mut self, event_type: &str, payload: Value) -> Result<u64> {
        // Validate inputs
        validate_event_type(event_type)
            .map_err(|e| StrataError::invalid_input(e.to_string()))?;
        validate_payload(&payload)
            .map_err(|e| StrataError::invalid_input(e.to_string()))?;

        let ns = Namespace::for_run(self.run_id);

        // Read current metadata (or default)
        let meta_key = Key::new_event_meta(ns.clone());
        let mut meta: EventLogMeta = match self.get(&meta_key)? {
            Some(v) => from_stored_value(&v).unwrap_or_else(|_| EventLogMeta::default()),
            None => EventLogMeta::default(),
        };

        // Compute event hash using current hash version
        let sequence = meta.next_sequence;
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        let hash = compute_event_hash(
            meta.hash_version,
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
        self.put(event_key, to_stored_value(&event))?;

        // Update stream metadata
        let event_type_owned = event_type.to_string();
        match meta.streams.get_mut(&event_type_owned) {
            Some(stream_meta) => stream_meta.update(sequence, timestamp),
            None => {
                meta.streams.insert(
                    event_type_owned,
                    StreamMeta::new(sequence, timestamp),
                );
            }
        }

        // Update metadata
        meta.next_sequence = sequence + 1;
        meta.head_hash = hash;
        self.put(meta_key, to_stored_value(&meta))?;

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

    /// Helper to create an empty object payload
    fn empty_payload() -> Value {
        Value::Object(HashMap::new())
    }

    /// Helper to create an object payload with a single value
    fn payload_with(key: &str, value: Value) -> Value {
        Value::Object(HashMap::from([(key.to_string(), value)]))
    }

    /// Helper to create an object payload with an integer
    fn int_payload(v: i64) -> Value {
        payload_with("value", Value::Int(v))
    }

    // ========== Core Structure Tests ==========

    #[test]
    fn test_event_serialization() {
        let event = Event {
            sequence: 42,
            event_type: "test".to_string(),
            payload: payload_with("data", Value::String("test".into())),
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
        assert_eq!(meta.hash_version, HASH_VERSION_SHA256);
        assert!(meta.streams.is_empty());
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

    // ========== Validation Tests ==========

    #[test]
    fn test_validation_rejects_null_payload() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        let result = log.append(&run_id, "test", Value::Null);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("object"));
    }

    #[test]
    fn test_validation_rejects_primitive_payload() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        // Test various primitive types
        assert!(log.append(&run_id, "test", Value::Int(42)).is_err());
        assert!(log.append(&run_id, "test", Value::String("hello".into())).is_err());
        assert!(log.append(&run_id, "test", Value::Bool(true)).is_err());
        assert!(log.append(&run_id, "test", Value::Float(3.14)).is_err());
    }

    #[test]
    fn test_validation_rejects_array_payload() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        let result = log.append(&run_id, "test", Value::Array(vec![Value::Int(1)]));
        assert!(result.is_err());
    }

    #[test]
    fn test_validation_rejects_nan_in_payload() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        let payload = payload_with("value", Value::Float(f64::NAN));
        let result = log.append(&run_id, "test", payload);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NaN"));
    }

    #[test]
    fn test_validation_rejects_infinity_in_payload() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        let payload = payload_with("value", Value::Float(f64::INFINITY));
        let result = log.append(&run_id, "test", payload);
        assert!(result.is_err());
    }

    #[test]
    fn test_validation_rejects_empty_event_type() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        let result = log.append(&run_id, "", empty_payload());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn test_validation_rejects_too_long_event_type() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        let long_type = "x".repeat(MAX_EVENT_TYPE_LENGTH + 1);
        let result = log.append(&run_id, &long_type, empty_payload());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("length"));
    }

    #[test]
    fn test_validation_accepts_valid_object_payload() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        let payload = Value::Object(HashMap::from([
            ("tool".to_string(), Value::String("search".into())),
            ("count".to_string(), Value::Int(42)),
        ]));

        let result = log.append(&run_id, "test", payload);
        assert!(result.is_ok());
    }

    // ========== Append Tests ==========

    #[test]
    fn test_append_first_event() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        let version = log.append(&run_id, "test", empty_payload()).unwrap();
        assert!(matches!(version, Version::Sequence(0)));
    }

    #[test]
    fn test_append_increments_sequence() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        let v1 = log.append(&run_id, "test", empty_payload()).unwrap();
        let v2 = log.append(&run_id, "test", empty_payload()).unwrap();
        let v3 = log.append(&run_id, "test", empty_payload()).unwrap();

        assert!(matches!(v1, Version::Sequence(0)));
        assert!(matches!(v2, Version::Sequence(1)));
        assert!(matches!(v3, Version::Sequence(2)));
    }

    #[test]
    fn test_hash_chain_links() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        log.append(&run_id, "test", empty_payload()).unwrap();
        let event1 = log.read(&run_id, 0).unwrap().unwrap();
        log.append(&run_id, "test", empty_payload()).unwrap();

        // Verify chain through read
        let event2 = log.read(&run_id, 1).unwrap().unwrap();
        assert_eq!(event2.value.prev_hash, event1.value.hash);
    }

    #[test]
    fn test_append_with_payload() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        let payload = Value::Object(HashMap::from([
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

        log.append(&run1, "run1_event", int_payload(1)).unwrap();
        log.append(&run1, "run1_event", int_payload(2)).unwrap();
        log.append(&run2, "run2_event", int_payload(100)).unwrap();

        assert_eq!(log.len(&run1).unwrap(), 2);
        assert_eq!(log.len(&run2).unwrap(), 1);

        let run1_events = log.read_range(&run1, 0, 10).unwrap();
        let run2_events = log.read_range(&run2, 0, 10).unwrap();

        assert_eq!(run1_events.len(), 2);
        assert_eq!(run2_events.len(), 1);
        assert_eq!(run2_events[0].value.event_type, "run2_event");
    }

    // ========== Read Tests ==========

    #[test]
    fn test_read_single_event() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        let payload = payload_with("data", Value::String("test".into()));
        log.append(&run_id, "test", payload.clone()).unwrap();

        let versioned = log.read(&run_id, 0).unwrap().unwrap();
        assert_eq!(versioned.value.sequence, 0);
        assert_eq!(versioned.value.event_type, "test");
        assert_eq!(versioned.value.payload, payload);
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
            log.append(&run_id, "test", int_payload(i)).unwrap();
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
        log.append(&run_id, "first", int_payload(1)).unwrap();
        log.append(&run_id, "second", int_payload(2)).unwrap();
        log.append(&run_id, "third", int_payload(3)).unwrap();

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

        log.append(&run_id, "test", empty_payload()).unwrap();
        assert_eq!(log.len(&run_id).unwrap(), 1);
        assert!(!log.is_empty(&run_id).unwrap());

        log.append(&run_id, "test", empty_payload()).unwrap();
        log.append(&run_id, "test", empty_payload()).unwrap();
        assert_eq!(log.len(&run_id).unwrap(), 3);
    }

    // ========== Chain Verification Tests ==========

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
            log.append(&run_id, "test", int_payload(i)).unwrap();
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

        log.append(&run_id, "type_a", payload_with("data", Value::String("test".into())))
            .unwrap();
        log.append(&run_id, "type_b", int_payload(42)).unwrap();
        log.append(&run_id, "type_a", payload_with("flag", Value::Bool(true))).unwrap();

        let verification = log.verify_chain(&run_id).unwrap();
        assert!(verification.is_valid);
        assert_eq!(verification.length, 3);
    }

    // ========== O(1) Stream Query Tests ==========

    #[test]
    fn test_len_by_type() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        log.append(&run_id, "tool_call", int_payload(1)).unwrap();
        log.append(&run_id, "tool_result", int_payload(2)).unwrap();
        log.append(&run_id, "tool_call", int_payload(3)).unwrap();

        assert_eq!(log.len_by_type(&run_id, "tool_call").unwrap(), 2);
        assert_eq!(log.len_by_type(&run_id, "tool_result").unwrap(), 1);
        assert_eq!(log.len_by_type(&run_id, "nonexistent").unwrap(), 0);
    }

    #[test]
    fn test_latest_sequence_by_type() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        log.append(&run_id, "tool_call", int_payload(1)).unwrap();   // seq 0
        log.append(&run_id, "tool_result", int_payload(2)).unwrap(); // seq 1
        log.append(&run_id, "tool_call", int_payload(3)).unwrap();   // seq 2

        assert_eq!(log.latest_sequence_by_type(&run_id, "tool_call").unwrap(), Some(2));
        assert_eq!(log.latest_sequence_by_type(&run_id, "tool_result").unwrap(), Some(1));
        assert_eq!(log.latest_sequence_by_type(&run_id, "nonexistent").unwrap(), None);
    }

    #[test]
    fn test_stream_info() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        log.append(&run_id, "tool_call", int_payload(1)).unwrap();   // seq 0
        log.append(&run_id, "tool_result", int_payload(2)).unwrap(); // seq 1
        log.append(&run_id, "tool_call", int_payload(3)).unwrap();   // seq 2

        let info = log.stream_info(&run_id, "tool_call").unwrap().unwrap();
        assert_eq!(info.count, 2);
        assert_eq!(info.first_sequence, 0);
        assert_eq!(info.last_sequence, 2);

        let info = log.stream_info(&run_id, "tool_result").unwrap().unwrap();
        assert_eq!(info.count, 1);
        assert_eq!(info.first_sequence, 1);
        assert_eq!(info.last_sequence, 1);

        assert!(log.stream_info(&run_id, "nonexistent").unwrap().is_none());
    }

    #[test]
    fn test_head_by_type() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        log.append(&run_id, "tool_call", int_payload(1)).unwrap();   // seq 0
        log.append(&run_id, "tool_result", int_payload(2)).unwrap(); // seq 1
        log.append(&run_id, "tool_call", int_payload(3)).unwrap();   // seq 2

        let head = log.head_by_type(&run_id, "tool_call").unwrap().unwrap();
        assert_eq!(head.value.sequence, 2);
        assert_eq!(head.value.payload, int_payload(3));

        assert!(log.head_by_type(&run_id, "nonexistent").unwrap().is_none());
    }

    #[test]
    fn test_stream_names() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        log.append(&run_id, "type_a", empty_payload()).unwrap();
        log.append(&run_id, "type_b", empty_payload()).unwrap();
        log.append(&run_id, "type_a", empty_payload()).unwrap();
        log.append(&run_id, "type_c", empty_payload()).unwrap();

        let names = log.stream_names(&run_id).unwrap();
        assert_eq!(names.len(), 3);
        assert!(names.contains(&"type_a".to_string()));
        assert!(names.contains(&"type_b".to_string()));
        assert!(names.contains(&"type_c".to_string()));
    }

    // ========== SHA-256 Hash Tests ==========

    #[test]
    fn test_sha256_hash_determinism() {
        // Same inputs should produce same hash
        let hash1 = compute_event_hash_v1(
            42,
            "test_event",
            &int_payload(100),
            1234567890,
            &[0u8; 32],
        );
        let hash2 = compute_event_hash_v1(
            42,
            "test_event",
            &int_payload(100),
            1234567890,
            &[0u8; 32],
        );
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_sha256_hash_differs_for_different_inputs() {
        let base = compute_event_hash_v1(42, "test", &empty_payload(), 1234567890, &[0u8; 32]);

        // Different sequence
        let diff_seq = compute_event_hash_v1(43, "test", &empty_payload(), 1234567890, &[0u8; 32]);
        assert_ne!(base, diff_seq);

        // Different event type
        let diff_type = compute_event_hash_v1(42, "other", &empty_payload(), 1234567890, &[0u8; 32]);
        assert_ne!(base, diff_type);

        // Different timestamp
        let diff_ts = compute_event_hash_v1(42, "test", &empty_payload(), 1234567891, &[0u8; 32]);
        assert_ne!(base, diff_ts);

        // Different prev_hash
        let diff_prev = compute_event_hash_v1(42, "test", &empty_payload(), 1234567890, &[1u8; 32]);
        assert_ne!(base, diff_prev);
    }

    #[test]
    fn test_sha256_uses_full_32_bytes() {
        let hash = compute_event_hash_v1(42, "test", &empty_payload(), 1234567890, &[0u8; 32]);

        // SHA-256 should use all 32 bytes, not just the first 8 like DefaultHasher
        // Check that bytes beyond the first 8 are non-zero (statistically likely)
        let non_zero_after_8: usize = hash[8..].iter().filter(|&&b| b != 0).count();
        assert!(non_zero_after_8 > 0, "SHA-256 should use all 32 bytes");
    }

    // ========== Query by Type Tests ==========

    #[test]
    fn test_read_by_type() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        log.append(&run_id, "tool_call", int_payload(1)).unwrap();
        log.append(&run_id, "tool_result", int_payload(2)).unwrap();
        log.append(&run_id, "tool_call", int_payload(3)).unwrap();
        log.append(&run_id, "thought", int_payload(4)).unwrap();
        log.append(&run_id, "tool_call", int_payload(5)).unwrap();

        let tool_calls = log.read_by_type(&run_id, "tool_call").unwrap();
        assert_eq!(tool_calls.len(), 3);
        assert_eq!(tool_calls[0].value.payload, int_payload(1));
        assert_eq!(tool_calls[1].value.payload, int_payload(3));
        assert_eq!(tool_calls[2].value.payload, int_payload(5));

        let thoughts = log.read_by_type(&run_id, "thought").unwrap();
        assert_eq!(thoughts.len(), 1);

        let nonexistent = log.read_by_type(&run_id, "nonexistent").unwrap();
        assert!(nonexistent.is_empty());
    }

    #[test]
    fn test_event_types() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        log.append(&run_id, "type_a", empty_payload()).unwrap();
        log.append(&run_id, "type_b", empty_payload()).unwrap();
        log.append(&run_id, "type_a", empty_payload()).unwrap();
        log.append(&run_id, "type_c", empty_payload()).unwrap();

        let types = log.event_types(&run_id).unwrap();
        assert_eq!(types.len(), 3);
        assert!(types.contains(&"type_a".to_string()));
        assert!(types.contains(&"type_b".to_string()));
        assert!(types.contains(&"type_c".to_string()));
    }

    // ========== EventLogExt Tests ==========

    #[test]
    fn test_eventlog_ext_append() {
        use crate::primitives::extensions::EventLogExt;

        let (_temp, db, log) = setup();
        let run_id = RunId::new();

        // Append via extension trait
        db.transaction(run_id, |txn| {
            let seq = txn.event_append("ext_event", payload_with("data", Value::String("test".into())))?;
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
        use crate::primitives::extensions::EventLogExt;

        let (_temp, db, log) = setup();
        let run_id = RunId::new();

        // Append via EventLog
        log.append(&run_id, "test", int_payload(42)).unwrap();

        // Read via extension trait
        db.transaction(run_id, |txn| {
            let value = txn.event_read(0)?;
            assert!(value.is_some());
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn test_eventlog_ext_validation() {
        use crate::primitives::extensions::EventLogExt;

        let (_temp, db, _log) = setup();
        let run_id = RunId::new();

        // EventLogExt should also validate payloads
        let result = db.transaction(run_id, |txn| {
            txn.event_append("test", Value::Int(42)) // primitive not allowed
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_cross_primitive_transaction() {
        use crate::primitives::extensions::{EventLogExt, KVStoreExt};

        let (_temp, db, _log) = setup();
        let run_id = RunId::new();

        // Atomic: KV put + event append
        db.transaction(run_id, |txn| {
            txn.kv_put("key", Value::String("value".into()))?;
            txn.event_append("kv_updated", payload_with("key", Value::String("key".into())))?;
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

    // ========== Batch Append Tests ==========

    #[test]
    fn test_append_batch_empty() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        let versions = log.append_batch(&run_id, &[]).unwrap();
        assert!(versions.is_empty());
        assert_eq!(log.len(&run_id).unwrap(), 0);
    }

    #[test]
    fn test_append_batch_single() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        let versions = log.append_batch(&run_id, &[("test", int_payload(1))]).unwrap();
        assert_eq!(versions.len(), 1);
        assert!(matches!(versions[0], Version::Sequence(0)));
        assert_eq!(log.len(&run_id).unwrap(), 1);
    }

    #[test]
    fn test_append_batch_multiple() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        let events = vec![
            ("orders", int_payload(1)),
            ("payments", int_payload(2)),
            ("orders", int_payload(3)),
        ];

        let versions = log.append_batch(&run_id, &events).unwrap();
        assert_eq!(versions.len(), 3);
        assert!(matches!(versions[0], Version::Sequence(0)));
        assert!(matches!(versions[1], Version::Sequence(1)));
        assert!(matches!(versions[2], Version::Sequence(2)));

        assert_eq!(log.len(&run_id).unwrap(), 3);
        assert_eq!(log.len_by_type(&run_id, "orders").unwrap(), 2);
        assert_eq!(log.len_by_type(&run_id, "payments").unwrap(), 1);
    }

    #[test]
    fn test_append_batch_preserves_chain() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        let events = vec![
            ("a", int_payload(1)),
            ("b", int_payload(2)),
            ("c", int_payload(3)),
        ];

        log.append_batch(&run_id, &events).unwrap();

        let verification = log.verify_chain(&run_id).unwrap();
        assert!(verification.is_valid);
        assert_eq!(verification.length, 3);
    }

    #[test]
    fn test_append_batch_validation_failure_rolls_back() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        // First batch succeeds
        log.append_batch(&run_id, &[("test", int_payload(1))]).unwrap();
        assert_eq!(log.len(&run_id).unwrap(), 1);

        // Second batch with invalid payload fails
        let events = vec![
            ("test", int_payload(2)),
            ("test", Value::Int(42)), // Invalid: not an object
            ("test", int_payload(3)),
        ];

        let result = log.append_batch(&run_id, &events);
        assert!(result.is_err());

        // Original event still exists, but no new events
        assert_eq!(log.len(&run_id).unwrap(), 1);
    }

    #[test]
    fn test_append_batch_updates_stream_metadata() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        let events = vec![
            ("orders", int_payload(1)),
            ("payments", int_payload(2)),
            ("orders", int_payload(3)),
        ];

        log.append_batch(&run_id, &events).unwrap();

        let orders_info = log.stream_info(&run_id, "orders").unwrap().unwrap();
        assert_eq!(orders_info.count, 2);
        assert_eq!(orders_info.first_sequence, 0);
        assert_eq!(orders_info.last_sequence, 2);

        let payments_info = log.stream_info(&run_id, "payments").unwrap().unwrap();
        assert_eq!(payments_info.count, 1);
        assert_eq!(payments_info.first_sequence, 1);
        assert_eq!(payments_info.last_sequence, 1);
    }

    // ========== Reverse Range Tests ==========

    #[test]
    fn test_read_range_reverse_empty() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        let events = log.read_range_reverse(&run_id, 5, 0).unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn test_read_range_reverse_invalid_range() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        // Append some events
        for i in 0..5 {
            log.append(&run_id, "test", int_payload(i)).unwrap();
        }

        // start < end is invalid
        let events = log.read_range_reverse(&run_id, 2, 4).unwrap();
        assert!(events.is_empty());
    }

    #[test]
    fn test_read_range_reverse_single() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        log.append(&run_id, "test", int_payload(42)).unwrap();

        let events = log.read_range_reverse(&run_id, 0, 0).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].value.payload, int_payload(42));
    }

    #[test]
    fn test_read_range_reverse_order() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        for i in 0..5 {
            log.append(&run_id, "test", int_payload(i)).unwrap();
        }

        let events = log.read_range_reverse(&run_id, 4, 0).unwrap();
        assert_eq!(events.len(), 5);

        // Should be in reverse order: 4, 3, 2, 1, 0
        for (i, event) in events.iter().enumerate() {
            assert_eq!(event.value.sequence, (4 - i) as u64);
            assert_eq!(event.value.payload, int_payload(4 - i as i64));
        }
    }

    #[test]
    fn test_read_range_reverse_subset() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        for i in 0..10 {
            log.append(&run_id, "test", int_payload(i)).unwrap();
        }

        // Read middle range in reverse
        let events = log.read_range_reverse(&run_id, 7, 3).unwrap();
        assert_eq!(events.len(), 5); // 7, 6, 5, 4, 3

        assert_eq!(events[0].value.sequence, 7);
        assert_eq!(events[1].value.sequence, 6);
        assert_eq!(events[2].value.sequence, 5);
        assert_eq!(events[3].value.sequence, 4);
        assert_eq!(events[4].value.sequence, 3);
    }

    #[test]
    fn test_read_range_reverse_vs_forward() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        for i in 0..5 {
            log.append(&run_id, "test", int_payload(i)).unwrap();
        }

        let forward = log.read_range(&run_id, 1, 4).unwrap();
        let reverse = log.read_range_reverse(&run_id, 3, 1).unwrap();

        // Same events, opposite order
        assert_eq!(forward.len(), 3);
        assert_eq!(reverse.len(), 3);

        for i in 0..3 {
            assert_eq!(forward[i].value.sequence, reverse[2 - i].value.sequence);
        }
    }

    // ========== Fast Path Tests ==========

    #[test]
    fn test_fast_read_returns_correct_value() {
        let (_temp, _db, log) = setup();
        let run_id = RunId::new();

        let payload = payload_with("data", Value::String("test".into()));
        log.append(&run_id, "test", payload.clone()).unwrap();

        let versioned = log.read(&run_id, 0).unwrap().unwrap();
        assert_eq!(versioned.value.event_type, "test");
        assert_eq!(versioned.value.payload, payload);
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

        log.append(&run_id, "test", int_payload(42)).unwrap();

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

        log.append(&run_id, "test", empty_payload()).unwrap();
        assert_eq!(log.len(&run_id).unwrap(), 1);

        log.append(&run_id, "test", empty_payload()).unwrap();
        log.append(&run_id, "test", empty_payload()).unwrap();
        assert_eq!(log.len(&run_id).unwrap(), 3);
    }

    #[test]
    fn test_fast_read_run_isolation() {
        let (_temp, _db, log) = setup();
        let run1 = RunId::new();
        let run2 = RunId::new();

        log.append(&run1, "run1", int_payload(1)).unwrap();
        log.append(&run2, "run2", int_payload(2)).unwrap();

        // Each run sees only its own events
        let event1 = log.read(&run1, 0).unwrap().unwrap();
        let event2 = log.read(&run2, 0).unwrap().unwrap();

        assert_eq!(event1.value.event_type, "run1");
        assert_eq!(event2.value.event_type, "run2");

        // Cross-run reads return None
        assert!(log.read(&run1, 1).unwrap().is_none());
    }
}
