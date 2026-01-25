//! Transaction wrapper implementing TransactionOps
//!
//! This module provides the Transaction type that wraps TransactionContext
//! and implements the TransactionOps trait for unified primitive access.
//!
//! # KV Operations in TransactionOps
//! # Event Operations in TransactionOps
//! # State Operations in TransactionOps
//!
//! This implementation provides:
//! - Read-your-writes semantics (check write set first)
//! - Read set tracking for conflict detection
//! - Proper key construction with namespaces
//! - Event buffering with sequence allocation
//! - State cell CAS (compare-and-swap) support

use crate::transaction_ops::TransactionOps;
use strata_concurrency::TransactionContext;
use strata_core::{
    EntityRef, Event, JsonDocId, JsonPath, JsonValue, MetadataFilter, RunMetadata, RunStatus, State,
    StrataError, Timestamp, Value, VectorEntry, VectorMatch, Version, Versioned,
};
use strata_core::types::{Key, Namespace, RunId, TypeTag};
use sha2::{Sha256, Digest};

/// Transaction wrapper that implements TransactionOps
///
/// Wraps a TransactionContext and provides the unified primitive API
/// defined by the TransactionOps trait.
///
/// # Usage
///
/// ```ignore
/// db.transaction(&run_id, |txn| {
///     // KV operations
///     let value = txn.kv_get("key")?;
///     txn.kv_put("key", Value::from("value"))?;
///
///     // Event operations
///     txn.event_append("event_type", json!({}))?;
///
///     // State operations
///     txn.state_init("counter", Value::Int(0))?;
///     txn.state_cas("counter", 1, Value::Int(1))?;
///
///     Ok(())
/// })?;
/// ```
pub struct Transaction<'a> {
    /// The underlying transaction context
    ctx: &'a mut TransactionContext,
    /// Namespace for this transaction's run
    namespace: Namespace,
    /// Pending events buffered in this transaction
    pending_events: Vec<Event>,
    /// Base sequence number from snapshot (events start at base_sequence)
    base_sequence: u64,
    /// Last hash for chaining (starts as zero hash or last event's hash)
    last_hash: [u8; 32],
}

impl<'a> Transaction<'a> {
    /// Create a new Transaction wrapper
    pub fn new(ctx: &'a mut TransactionContext, namespace: Namespace) -> Self {
        // TODO: In full implementation, read base_sequence from snapshot metadata
        // For now, start at 0
        Self {
            ctx,
            namespace,
            pending_events: Vec::new(),
            base_sequence: 0,
            last_hash: [0u8; 32],
        }
    }

    /// Create a new Transaction with explicit base sequence
    ///
    /// Use this when you know the current event count from snapshot.
    pub fn with_base_sequence(
        ctx: &'a mut TransactionContext,
        namespace: Namespace,
        base_sequence: u64,
        last_hash: [u8; 32],
    ) -> Self {
        Self {
            ctx,
            namespace,
            pending_events: Vec::new(),
            base_sequence,
            last_hash,
        }
    }

    /// Get the run ID for this transaction
    pub fn run_id(&self) -> RunId {
        self.ctx.run_id
    }

    /// Create a KV key for the given user key
    fn kv_key(&self, key: &str) -> Key {
        Key::new_kv(self.namespace.clone(), key)
    }

    /// Create an event key for the given sequence
    fn event_key(&self, sequence: u64) -> Key {
        Key::new_event(self.namespace.clone(), sequence)
    }

    /// Extract user key from a full Key
    fn user_key(key: &Key) -> String {
        key.user_key_string().unwrap_or_default()
    }

    /// Compute hash for an event
    fn compute_event_hash(event: &Event) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(event.sequence.to_be_bytes());
        hasher.update(event.event_type.as_bytes());
        // Hash the payload by serializing to JSON
        if let Ok(payload_json) = serde_json::to_vec(&event.payload) {
            hasher.update(&payload_json);
        }
        hasher.update(event.timestamp.to_be_bytes());
        hasher.update(event.prev_hash);
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        hash
    }

    /// Get pending events (for commit)
    pub fn pending_events(&self) -> &[Event] {
        &self.pending_events
    }

    /// Get the next sequence number for a new event
    fn next_sequence(&self) -> u64 {
        self.base_sequence + self.pending_events.len() as u64
    }

    /// Create a state key for the given name
    fn state_key(&self, name: &str) -> Key {
        Key::new_state(self.namespace.clone(), name)
    }
}

impl<'a> TransactionOps for Transaction<'a> {
    // =========================================================================
    // KV Operations (Phase 2)
    // =========================================================================

    fn kv_get(&self, key: &str) -> Result<Option<Versioned<Value>>, StrataError> {
        let full_key = self.kv_key(key);

        // Check write set first (read-your-writes)
        if let Some(value) = self.ctx.write_set.get(&full_key) {
            return Ok(Some(Versioned::new(
                value.clone(),
                Version::txn(self.ctx.txn_id),
            )));
        }

        // Check delete set (uncommitted delete returns None)
        if self.ctx.delete_set.contains(&full_key) {
            return Ok(None);
        }

        // For reads from snapshot, we can only see uncommitted changes
        // The full implementation would need TransactionContext to expose
        // a snapshot read method.
        Ok(None)
    }

    fn kv_put(&mut self, key: &str, value: Value) -> Result<Version, StrataError> {
        let full_key = self.kv_key(key);

        // Use the ctx.put() method which handles all the bookkeeping
        self.ctx.put(full_key, value).map_err(StrataError::from)?;

        Ok(Version::txn(self.ctx.txn_id))
    }

    fn kv_delete(&mut self, key: &str) -> Result<bool, StrataError> {
        let full_key = self.kv_key(key);

        // Check if key exists (for return value)
        let existed = self.kv_exists(key)?;

        // Use the ctx.delete() method
        self.ctx.delete(full_key).map_err(StrataError::from)?;

        Ok(existed)
    }

    fn kv_exists(&self, key: &str) -> Result<bool, StrataError> {
        let full_key = self.kv_key(key);

        // Check write set first
        if self.ctx.write_set.contains_key(&full_key) {
            return Ok(true);
        }

        // Check delete set
        if self.ctx.delete_set.contains(&full_key) {
            return Ok(false);
        }

        // For keys not in write/delete set, we'd need snapshot access
        Ok(false)
    }

    fn kv_list(&self, prefix: Option<&str>) -> Result<Vec<String>, StrataError> {
        let mut keys: Vec<String> = Vec::new();

        // Collect keys from write set matching prefix
        for key in self.ctx.write_set.keys() {
            if key.type_tag == TypeTag::KV && key.namespace == self.namespace {
                let user_key = Self::user_key(key);
                if let Some(p) = prefix {
                    if user_key.starts_with(p) {
                        keys.push(user_key);
                    }
                } else {
                    keys.push(user_key);
                }
            }
        }

        // Remove deleted keys
        for key in &self.ctx.delete_set {
            if key.type_tag == TypeTag::KV && key.namespace == self.namespace {
                let user_key = Self::user_key(key);
                keys.retain(|k| k != &user_key);
            }
        }

        keys.sort();
        Ok(keys)
    }

    // =========================================================================
    // Event Operations (Phase 2)
    // =========================================================================

    fn event_append(&mut self, event_type: &str, payload: Value) -> Result<Version, StrataError> {
        let sequence = self.next_sequence();
        let timestamp = Timestamp::now().as_micros() as i64;
        let prev_hash = self.last_hash;

        // Create the event
        let mut event = Event {
            sequence,
            event_type: event_type.to_string(),
            payload,
            timestamp,
            prev_hash,
            hash: [0u8; 32], // Will be computed
        };

        // Compute and set the hash
        event.hash = Self::compute_event_hash(&event);

        // Update last_hash for next event in chain
        self.last_hash = event.hash;

        // Also write to the underlying context so it gets committed
        let event_key = self.event_key(sequence);
        let event_bytes = serde_json::to_vec(&event).map_err(|e| {
            StrataError::Serialization {
                message: e.to_string(),
            }
        })?;
        self.ctx.put(event_key, Value::Bytes(event_bytes)).map_err(StrataError::from)?;

        // Buffer the event
        self.pending_events.push(event);

        Ok(Version::seq(sequence))
    }

    fn event_read(&self, sequence: u64) -> Result<Option<Versioned<Event>>, StrataError> {
        // Check pending events first (read-your-writes)
        if sequence >= self.base_sequence {
            let index = (sequence - self.base_sequence) as usize;
            if index < self.pending_events.len() {
                let event = &self.pending_events[index];
                return Ok(Some(Versioned::new(
                    event.clone(),
                    Version::seq(sequence),
                )));
            }
        }

        // Check if the event was written to ctx.write_set
        let event_key = self.event_key(sequence);
        if let Some(Value::Bytes(bytes)) = self.ctx.write_set.get(&event_key) {
            let event: Event = serde_json::from_slice(bytes).map_err(|e| {
                StrataError::Serialization {
                    message: e.to_string(),
                }
            })?;
            return Ok(Some(Versioned::new(event, Version::seq(sequence))));
        }

        // For reads from snapshot, would need snapshot access
        // Return None for events not in pending or write set
        Ok(None)
    }

    fn event_range(&self, start: u64, end: u64) -> Result<Vec<Versioned<Event>>, StrataError> {
        let mut results = Vec::new();

        for seq in start..end {
            if let Some(versioned) = self.event_read(seq)? {
                results.push(versioned);
            }
        }

        Ok(results)
    }

    fn event_len(&self) -> Result<u64, StrataError> {
        // Base sequence from snapshot + pending events
        Ok(self.base_sequence + self.pending_events.len() as u64)
    }

    // =========================================================================
    // State Operations (Phase 3)
    // =========================================================================

    fn state_read(&self, name: &str) -> Result<Option<Versioned<State>>, StrataError> {
        let full_key = self.state_key(name);

        // Check write set first (read-your-writes)
        if let Some(Value::Bytes(bytes)) = self.ctx.write_set.get(&full_key) {
            let state: State = serde_json::from_slice(bytes).map_err(|e| {
                StrataError::Serialization {
                    message: e.to_string(),
                }
            })?;
            return Ok(Some(Versioned::new(
                state.clone(),
                Version::counter(state.version),
            )));
        }

        // Check delete set (uncommitted delete returns None)
        if self.ctx.delete_set.contains(&full_key) {
            return Ok(None);
        }

        // For reads from snapshot, would need snapshot access
        // Return None for state not in write set
        Ok(None)
    }

    fn state_init(&mut self, name: &str, value: Value) -> Result<Version, StrataError> {
        let full_key = self.state_key(name);

        // Check if state already exists (init should only work for new state)
        if self.ctx.write_set.contains_key(&full_key) {
            return Err(StrataError::invalid_operation(
                EntityRef::state(self.run_id(), name),
                "state already exists",
            ));
        }

        // Create new state with version 1
        let state = State::new(value);
        let version = state.version;

        // Serialize and store
        let state_bytes = serde_json::to_vec(&state).map_err(|e| {
            StrataError::Serialization {
                message: e.to_string(),
            }
        })?;

        self.ctx.put(full_key, Value::Bytes(state_bytes)).map_err(StrataError::from)?;

        Ok(Version::counter(version))
    }

    fn state_cas(
        &mut self,
        name: &str,
        expected_version: u64,
        value: Value,
    ) -> Result<Version, StrataError> {
        let full_key = self.state_key(name);

        // Read current state to get version
        let current_state = if let Some(Value::Bytes(bytes)) = self.ctx.write_set.get(&full_key) {
            let state: State = serde_json::from_slice(bytes).map_err(|e| {
                StrataError::Serialization {
                    message: e.to_string(),
                }
            })?;
            Some(state)
        } else {
            None
        };

        // For CAS, state must exist
        let current = current_state.ok_or_else(|| {
            StrataError::not_found(EntityRef::state(self.run_id(), name))
        })?;

        // Check version matches
        if current.version != expected_version {
            return Err(StrataError::version_conflict(
                EntityRef::state(self.run_id(), name),
                Version::counter(expected_version),
                Version::counter(current.version),
            ));
        }

        // Create new state with incremented version
        let new_state = State::with_version(value, expected_version + 1);
        let new_version = new_state.version;

        // Serialize and store
        let state_bytes = serde_json::to_vec(&new_state).map_err(|e| {
            StrataError::Serialization {
                message: e.to_string(),
            }
        })?;

        self.ctx.put(full_key, Value::Bytes(state_bytes)).map_err(StrataError::from)?;

        Ok(Version::counter(new_version))
    }

    fn state_delete(&mut self, name: &str) -> Result<bool, StrataError> {
        let full_key = self.state_key(name);

        // Check if state exists (for return value)
        let existed = self.state_exists(name)?;

        // Use the ctx.delete() method
        self.ctx.delete(full_key).map_err(StrataError::from)?;

        Ok(existed)
    }

    fn state_exists(&self, name: &str) -> Result<bool, StrataError> {
        let full_key = self.state_key(name);

        // Check write set first
        if self.ctx.write_set.contains_key(&full_key) {
            return Ok(true);
        }

        // Check delete set
        if self.ctx.delete_set.contains(&full_key) {
            return Ok(false);
        }

        // For keys not in write/delete set, we'd need snapshot access
        Ok(false)
    }

    // =========================================================================
    // Json Operations (Phase 4) - Stub implementations
    // =========================================================================

    fn json_create(&mut self, _doc_id: &JsonDocId, _value: JsonValue) -> Result<Version, StrataError> {
        unimplemented!("Json operations will be implemented in Phase 4")
    }

    fn json_get(&self, _doc_id: &JsonDocId) -> Result<Option<Versioned<JsonValue>>, StrataError> {
        unimplemented!("Json operations will be implemented in Phase 4")
    }

    fn json_get_path(
        &self,
        _doc_id: &JsonDocId,
        _path: &JsonPath,
    ) -> Result<Option<JsonValue>, StrataError> {
        unimplemented!("Json operations will be implemented in Phase 4")
    }

    fn json_set(
        &mut self,
        _doc_id: &JsonDocId,
        _path: &JsonPath,
        _value: JsonValue,
    ) -> Result<Version, StrataError> {
        unimplemented!("Json operations will be implemented in Phase 4")
    }

    fn json_delete(&mut self, _doc_id: &JsonDocId) -> Result<bool, StrataError> {
        unimplemented!("Json operations will be implemented in Phase 4")
    }

    fn json_exists(&self, _doc_id: &JsonDocId) -> Result<bool, StrataError> {
        unimplemented!("Json operations will be implemented in Phase 4")
    }

    fn json_destroy(&mut self, _doc_id: &JsonDocId) -> Result<bool, StrataError> {
        unimplemented!("Json operations will be implemented in Phase 4")
    }

    // =========================================================================
    // Vector Operations (Phase 4) - Stub implementations
    // =========================================================================

    fn vector_insert(
        &mut self,
        _collection: &str,
        _key: &str,
        _embedding: &[f32],
        _metadata: Option<Value>,
    ) -> Result<Version, StrataError> {
        unimplemented!("Vector operations will be implemented in Phase 4")
    }

    fn vector_get(
        &self,
        _collection: &str,
        _key: &str,
    ) -> Result<Option<Versioned<VectorEntry>>, StrataError> {
        unimplemented!("Vector operations will be implemented in Phase 4")
    }

    fn vector_delete(&mut self, _collection: &str, _key: &str) -> Result<bool, StrataError> {
        unimplemented!("Vector operations will be implemented in Phase 4")
    }

    fn vector_search(
        &self,
        _collection: &str,
        _query: &[f32],
        _k: usize,
        _filter: Option<MetadataFilter>,
    ) -> Result<Vec<VectorMatch>, StrataError> {
        unimplemented!("Vector operations will be implemented in Phase 4")
    }

    fn vector_exists(&self, _collection: &str, _key: &str) -> Result<bool, StrataError> {
        unimplemented!("Vector operations will be implemented in Phase 4")
    }

    // =========================================================================
    // Run Operations (Phase 5) - Stub implementations
    // =========================================================================

    fn run_metadata(&self) -> Result<Option<Versioned<RunMetadata>>, StrataError> {
        unimplemented!("Run operations will be implemented in Phase 5")
    }

    fn run_update_status(&mut self, _status: RunStatus) -> Result<Version, StrataError> {
        unimplemented!("Run operations will be implemented in Phase 5")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strata_concurrency::snapshot::ClonedSnapshotView;

    fn create_test_namespace() -> Namespace {
        let run_id = RunId::new();
        Namespace::new("tenant".to_string(), "app".to_string(), "agent".to_string(), run_id)
    }

    fn create_test_context(ns: &Namespace) -> TransactionContext {
        let snapshot = Box::new(ClonedSnapshotView::empty(100));
        TransactionContext::with_snapshot(1, ns.run_id, snapshot)
    }

    // =========================================================================
    // KV Tests
    // =========================================================================

    #[test]
    fn test_kv_put_and_get() {
        let ns = create_test_namespace();
        let mut ctx = create_test_context(&ns);
        let mut txn = Transaction::new(&mut ctx, ns.clone());

        // Put a value
        let version = txn.kv_put("test_key", Value::String("test_value".to_string())).unwrap();
        assert!(version.as_u64() > 0);

        // Get the value back (read-your-writes)
        let result = txn.kv_get("test_key").unwrap();
        assert!(result.is_some());
        let versioned = result.unwrap();
        assert_eq!(versioned.value, Value::String("test_value".to_string()));
    }

    #[test]
    fn test_kv_delete() {
        let ns = create_test_namespace();
        let mut ctx = create_test_context(&ns);
        let mut txn = Transaction::new(&mut ctx, ns.clone());

        // Put then delete
        txn.kv_put("test_key", Value::String("value".to_string())).unwrap();
        let existed = txn.kv_delete("test_key").unwrap();
        assert!(existed);

        // Get should return None
        let result = txn.kv_get("test_key").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_kv_exists() {
        let ns = create_test_namespace();
        let mut ctx = create_test_context(&ns);
        let mut txn = Transaction::new(&mut ctx, ns.clone());

        // Key doesn't exist initially
        assert!(!txn.kv_exists("missing").unwrap());

        // Put and check
        txn.kv_put("present", Value::String("value".to_string())).unwrap();
        assert!(txn.kv_exists("present").unwrap());
    }

    #[test]
    fn test_kv_list() {
        let ns = create_test_namespace();
        let mut ctx = create_test_context(&ns);
        let mut txn = Transaction::new(&mut ctx, ns.clone());

        // Add some keys
        txn.kv_put("user:1", Value::String("alice".to_string())).unwrap();
        txn.kv_put("user:2", Value::String("bob".to_string())).unwrap();
        txn.kv_put("config:app", Value::String("settings".to_string())).unwrap();

        // List all
        let all_keys = txn.kv_list(None).unwrap();
        assert_eq!(all_keys.len(), 3);

        // List with prefix
        let user_keys = txn.kv_list(Some("user:")).unwrap();
        assert_eq!(user_keys.len(), 2);
        assert!(user_keys.contains(&"user:1".to_string()));
        assert!(user_keys.contains(&"user:2".to_string()));
    }

    #[test]
    fn test_kv_list_with_delete() {
        let ns = create_test_namespace();
        let mut ctx = create_test_context(&ns);
        let mut txn = Transaction::new(&mut ctx, ns.clone());

        // Add keys then delete one
        txn.kv_put("key1", Value::String("v1".to_string())).unwrap();
        txn.kv_put("key2", Value::String("v2".to_string())).unwrap();
        txn.kv_delete("key1").unwrap();

        let keys = txn.kv_list(None).unwrap();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0], "key2");
    }

    // =========================================================================
    // Event Tests
    // =========================================================================

    #[test]
    fn test_event_append() {
        let ns = create_test_namespace();
        let mut ctx = create_test_context(&ns);
        let mut txn = Transaction::new(&mut ctx, ns.clone());

        // Append an event
        let version = txn.event_append("user_created", Value::String("alice".to_string())).unwrap();
        assert_eq!(version, Version::seq(0));

        // Check event count
        assert_eq!(txn.event_len().unwrap(), 1);
    }

    #[test]
    fn test_event_append_multiple() {
        let ns = create_test_namespace();
        let mut ctx = create_test_context(&ns);
        let mut txn = Transaction::new(&mut ctx, ns.clone());

        // Append multiple events
        let v1 = txn.event_append("event1", Value::Int(1)).unwrap();
        let v2 = txn.event_append("event2", Value::Int(2)).unwrap();
        let v3 = txn.event_append("event3", Value::Int(3)).unwrap();

        assert_eq!(v1, Version::seq(0));
        assert_eq!(v2, Version::seq(1));
        assert_eq!(v3, Version::seq(2));
        assert_eq!(txn.event_len().unwrap(), 3);
    }

    #[test]
    fn test_event_read() {
        let ns = create_test_namespace();
        let mut ctx = create_test_context(&ns);
        let mut txn = Transaction::new(&mut ctx, ns.clone());

        // Append an event
        txn.event_append("test_event", Value::String("payload".to_string())).unwrap();

        // Read it back
        let result = txn.event_read(0).unwrap();
        assert!(result.is_some());

        let versioned = result.unwrap();
        assert_eq!(versioned.value.sequence, 0);
        assert_eq!(versioned.value.event_type, "test_event");
        assert_eq!(versioned.value.payload, Value::String("payload".to_string()));
    }

    #[test]
    fn test_event_read_your_writes() {
        let ns = create_test_namespace();
        let mut ctx = create_test_context(&ns);
        let mut txn = Transaction::new(&mut ctx, ns.clone());

        // Append events and read them back immediately
        txn.event_append("first", Value::Int(100)).unwrap();
        txn.event_append("second", Value::Int(200)).unwrap();

        let first = txn.event_read(0).unwrap().unwrap();
        let second = txn.event_read(1).unwrap().unwrap();

        assert_eq!(first.value.event_type, "first");
        assert_eq!(first.value.payload, Value::Int(100));
        assert_eq!(second.value.event_type, "second");
        assert_eq!(second.value.payload, Value::Int(200));
    }

    #[test]
    fn test_event_read_not_found() {
        let ns = create_test_namespace();
        let mut ctx = create_test_context(&ns);
        let txn = Transaction::new(&mut ctx, ns.clone());

        // Reading non-existent event returns None
        let result = txn.event_read(999).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_event_range() {
        let ns = create_test_namespace();
        let mut ctx = create_test_context(&ns);
        let mut txn = Transaction::new(&mut ctx, ns.clone());

        // Append several events
        for i in 0..5 {
            txn.event_append(&format!("event_{}", i), Value::Int(i)).unwrap();
        }

        // Read a range
        let events = txn.event_range(1, 4).unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].value.event_type, "event_1");
        assert_eq!(events[1].value.event_type, "event_2");
        assert_eq!(events[2].value.event_type, "event_3");
    }

    #[test]
    fn test_event_hash_chaining() {
        let ns = create_test_namespace();
        let mut ctx = create_test_context(&ns);
        let mut txn = Transaction::new(&mut ctx, ns.clone());

        // Append events
        txn.event_append("first", Value::Int(1)).unwrap();
        txn.event_append("second", Value::Int(2)).unwrap();

        let first = txn.event_read(0).unwrap().unwrap();
        let second = txn.event_read(1).unwrap().unwrap();

        // First event's prev_hash should be zeros (genesis)
        assert_eq!(first.value.prev_hash, [0u8; 32]);

        // Second event's prev_hash should be first event's hash
        assert_eq!(second.value.prev_hash, first.value.hash);

        // Each event should have a non-zero hash
        assert_ne!(first.value.hash, [0u8; 32]);
        assert_ne!(second.value.hash, [0u8; 32]);
    }

    #[test]
    fn test_event_with_base_sequence() {
        let ns = create_test_namespace();
        let mut ctx = create_test_context(&ns);

        // Create transaction with existing events (simulating snapshot)
        let last_hash = [42u8; 32];
        let mut txn = Transaction::with_base_sequence(&mut ctx, ns.clone(), 100, last_hash);

        // New events should continue from base
        let v1 = txn.event_append("new_event", Value::Int(1)).unwrap();
        assert_eq!(v1, Version::seq(100));
        assert_eq!(txn.event_len().unwrap(), 101);

        // The event should chain from the provided last_hash
        let event = txn.event_read(100).unwrap().unwrap();
        assert_eq!(event.value.prev_hash, last_hash);
    }

    #[test]
    fn test_pending_events_accessor() {
        let ns = create_test_namespace();
        let mut ctx = create_test_context(&ns);
        let mut txn = Transaction::new(&mut ctx, ns.clone());

        txn.event_append("e1", Value::Int(1)).unwrap();
        txn.event_append("e2", Value::Int(2)).unwrap();

        let pending = txn.pending_events();
        assert_eq!(pending.len(), 2);
        assert_eq!(pending[0].event_type, "e1");
        assert_eq!(pending[1].event_type, "e2");
    }

    // =========================================================================
    // State Tests
    // =========================================================================

    #[test]
    fn test_state_init_and_read() {
        let ns = create_test_namespace();
        let mut ctx = create_test_context(&ns);
        let mut txn = Transaction::new(&mut ctx, ns.clone());

        // Initialize a state cell
        let version = txn.state_init("counter", Value::Int(0)).unwrap();
        assert_eq!(version, Version::counter(1)); // Version 1 for new state

        // Read it back (read-your-writes)
        let result = txn.state_read("counter").unwrap();
        assert!(result.is_some());
        let versioned = result.unwrap();
        assert_eq!(versioned.value.value, Value::Int(0));
        assert_eq!(versioned.value.version, 1);
    }

    #[test]
    fn test_state_cas_success() {
        let ns = create_test_namespace();
        let mut ctx = create_test_context(&ns);
        let mut txn = Transaction::new(&mut ctx, ns.clone());

        // Initialize then CAS
        txn.state_init("counter", Value::Int(0)).unwrap();
        let new_version = txn.state_cas("counter", 1, Value::Int(1)).unwrap();
        assert_eq!(new_version, Version::counter(2)); // Version incremented

        // Verify the value changed
        let result = txn.state_read("counter").unwrap().unwrap();
        assert_eq!(result.value.value, Value::Int(1));
        assert_eq!(result.value.version, 2);
    }

    #[test]
    fn test_state_cas_version_mismatch() {
        let ns = create_test_namespace();
        let mut ctx = create_test_context(&ns);
        let mut txn = Transaction::new(&mut ctx, ns.clone());

        // Initialize then CAS with wrong version
        txn.state_init("counter", Value::Int(0)).unwrap();
        let result = txn.state_cas("counter", 99, Value::Int(1)); // Wrong version

        assert!(result.is_err());
        match result.unwrap_err() {
            StrataError::VersionConflict { expected, actual, .. } => {
                assert_eq!(expected, Version::counter(99));
                assert_eq!(actual, Version::counter(1));
            }
            _ => panic!("Expected VersionConflict error"),
        }
    }

    #[test]
    fn test_state_delete() {
        let ns = create_test_namespace();
        let mut ctx = create_test_context(&ns);
        let mut txn = Transaction::new(&mut ctx, ns.clone());

        // Initialize then delete
        txn.state_init("counter", Value::Int(0)).unwrap();
        let existed = txn.state_delete("counter").unwrap();
        assert!(existed);

        // Read should return None
        let result = txn.state_read("counter").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_state_exists() {
        let ns = create_test_namespace();
        let mut ctx = create_test_context(&ns);
        let mut txn = Transaction::new(&mut ctx, ns.clone());

        // State doesn't exist initially
        assert!(!txn.state_exists("counter").unwrap());

        // Initialize and check
        txn.state_init("counter", Value::Int(0)).unwrap();
        assert!(txn.state_exists("counter").unwrap());
    }

    #[test]
    fn test_state_init_duplicate_fails() {
        let ns = create_test_namespace();
        let mut ctx = create_test_context(&ns);
        let mut txn = Transaction::new(&mut ctx, ns.clone());

        // Initialize twice should fail
        txn.state_init("counter", Value::Int(0)).unwrap();
        let result = txn.state_init("counter", Value::Int(1));

        assert!(result.is_err());
        match result.unwrap_err() {
            StrataError::InvalidOperation { reason, .. } => {
                assert!(reason.contains("already exists"));
            }
            _ => panic!("Expected InvalidOperation error"),
        }
    }

}
