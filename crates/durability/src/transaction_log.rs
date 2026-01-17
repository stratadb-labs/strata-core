//! Cross-Primitive Transaction Support
//!
//! This module provides transaction grouping for atomic cross-primitive operations.
//! All entries in a transaction share the same tx_id, ensuring either all effects
//! are visible after crash recovery or none.
//!
//! ## Core Guarantees
//!
//! 1. **Atomicity**: All entries in a transaction share the same tx_id
//! 2. **Commit Markers**: Transaction only visible after commit marker is written
//! 3. **Recovery Safety**: Recovery only applies entries with commit markers
//! 4. **No Partial State**: Orphaned transactions (no commit) are discarded
//!
//! ## Example
//!
//! ```ignore
//! let mut tx = Transaction::new();
//! tx.kv_put("key1", b"value1")
//!   .json_set("doc1", json!({"field": "value"}))
//!   .state_set("state1", b"active");
//!
//! // All entries share the same tx_id
//! let (tx_id, entries) = tx.into_wal_entries();
//! ```

use crate::wal_types::{TxId, WalEntry};
use crate::wal_entry_types::WalEntryType;

/// A transaction that can span multiple primitives
///
/// All operations added to the transaction share the same tx_id,
/// ensuring atomic commit (all or nothing) semantics.
#[derive(Debug)]
pub struct Transaction {
    /// Transaction ID shared by all entries
    id: TxId,
    /// Entries accumulated during transaction
    entries: Vec<TxEntry>,
}

/// Entry types for cross-primitive transactions
///
/// Each variant corresponds to a primitive operation that can be
/// part of an atomic transaction.
#[derive(Debug, Clone, PartialEq)]
pub enum TxEntry {
    // ========================================================================
    // KV Primitive Operations
    // ========================================================================
    /// KV put operation
    KvPut {
        /// Key to write
        key: Vec<u8>,
        /// Value to store
        value: Vec<u8>,
    },
    /// KV delete operation
    KvDelete {
        /// Key to delete
        key: Vec<u8>,
    },

    // ========================================================================
    // JSON Primitive Operations
    // ========================================================================
    /// JSON document creation
    JsonCreate {
        /// Document key
        key: Vec<u8>,
        /// JSON document content
        doc: Vec<u8>,
    },
    /// JSON set value
    JsonSet {
        /// Document key
        key: Vec<u8>,
        /// JSON document content
        doc: Vec<u8>,
    },
    /// JSON delete
    JsonDelete {
        /// Document key
        key: Vec<u8>,
    },
    /// JSON patch (RFC 6902)
    JsonPatch {
        /// Document key
        key: Vec<u8>,
        /// Patch operations
        patch: Vec<u8>,
    },

    // ========================================================================
    // Event Primitive Operations
    // ========================================================================
    /// Event append
    EventAppend {
        /// Event payload
        payload: Vec<u8>,
    },

    // ========================================================================
    // State Primitive Operations
    // ========================================================================
    /// State initialization
    StateInit {
        /// State key
        key: Vec<u8>,
        /// Initial value
        value: Vec<u8>,
    },
    /// State set
    StateSet {
        /// State key
        key: Vec<u8>,
        /// New value
        value: Vec<u8>,
    },
    /// State transition
    StateTransition {
        /// State key
        key: Vec<u8>,
        /// Expected from value
        from: Vec<u8>,
        /// New to value
        to: Vec<u8>,
    },

    // ========================================================================
    // Trace Primitive Operations
    // ========================================================================
    /// Trace span record
    TraceRecord {
        /// Span data
        span: Vec<u8>,
    },

    // ========================================================================
    // Run Primitive Operations
    // ========================================================================
    /// Run creation
    RunCreate {
        /// Run metadata
        metadata: Vec<u8>,
    },
    /// Run update
    RunUpdate {
        /// Run metadata
        metadata: Vec<u8>,
    },
    /// Run begin
    RunBegin {
        /// Run metadata
        metadata: Vec<u8>,
    },
    /// Run end
    RunEnd {
        /// Run metadata
        metadata: Vec<u8>,
    },
}

impl Default for Transaction {
    fn default() -> Self {
        Self::new()
    }
}

impl Transaction {
    /// Create a new transaction with a fresh tx_id
    pub fn new() -> Self {
        Transaction {
            id: TxId::new(),
            entries: Vec::new(),
        }
    }

    /// Create a transaction with a specific tx_id (for testing/recovery)
    pub fn with_id(id: TxId) -> Self {
        Transaction {
            id,
            entries: Vec::new(),
        }
    }

    /// Get the transaction ID
    pub fn id(&self) -> TxId {
        self.id
    }

    /// Get the entries in this transaction
    pub fn entries(&self) -> &[TxEntry] {
        &self.entries
    }

    /// Get the number of entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the transaction is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    // ========================================================================
    // KV Operations
    // ========================================================================

    /// Add a KV put operation
    pub fn kv_put(&mut self, key: impl Into<Vec<u8>>, value: impl Into<Vec<u8>>) -> &mut Self {
        self.entries.push(TxEntry::KvPut {
            key: key.into(),
            value: value.into(),
        });
        self
    }

    /// Add a KV delete operation
    pub fn kv_delete(&mut self, key: impl Into<Vec<u8>>) -> &mut Self {
        self.entries.push(TxEntry::KvDelete { key: key.into() });
        self
    }

    // ========================================================================
    // JSON Operations
    // ========================================================================

    /// Add a JSON create operation
    pub fn json_create(&mut self, key: impl Into<Vec<u8>>, doc: impl Into<Vec<u8>>) -> &mut Self {
        self.entries.push(TxEntry::JsonCreate {
            key: key.into(),
            doc: doc.into(),
        });
        self
    }

    /// Add a JSON set operation
    pub fn json_set(&mut self, key: impl Into<Vec<u8>>, doc: impl Into<Vec<u8>>) -> &mut Self {
        self.entries.push(TxEntry::JsonSet {
            key: key.into(),
            doc: doc.into(),
        });
        self
    }

    /// Add a JSON delete operation
    pub fn json_delete(&mut self, key: impl Into<Vec<u8>>) -> &mut Self {
        self.entries.push(TxEntry::JsonDelete { key: key.into() });
        self
    }

    /// Add a JSON patch operation
    pub fn json_patch(&mut self, key: impl Into<Vec<u8>>, patch: impl Into<Vec<u8>>) -> &mut Self {
        self.entries.push(TxEntry::JsonPatch {
            key: key.into(),
            patch: patch.into(),
        });
        self
    }

    // ========================================================================
    // Event Operations
    // ========================================================================

    /// Add an event append operation
    pub fn event_append(&mut self, payload: impl Into<Vec<u8>>) -> &mut Self {
        self.entries.push(TxEntry::EventAppend {
            payload: payload.into(),
        });
        self
    }

    // ========================================================================
    // State Operations
    // ========================================================================

    /// Add a state init operation
    pub fn state_init(&mut self, key: impl Into<Vec<u8>>, value: impl Into<Vec<u8>>) -> &mut Self {
        self.entries.push(TxEntry::StateInit {
            key: key.into(),
            value: value.into(),
        });
        self
    }

    /// Add a state set operation
    pub fn state_set(&mut self, key: impl Into<Vec<u8>>, value: impl Into<Vec<u8>>) -> &mut Self {
        self.entries.push(TxEntry::StateSet {
            key: key.into(),
            value: value.into(),
        });
        self
    }

    /// Add a state transition operation
    pub fn state_transition(
        &mut self,
        key: impl Into<Vec<u8>>,
        from: impl Into<Vec<u8>>,
        to: impl Into<Vec<u8>>,
    ) -> &mut Self {
        self.entries.push(TxEntry::StateTransition {
            key: key.into(),
            from: from.into(),
            to: to.into(),
        });
        self
    }

    // ========================================================================
    // Trace Operations
    // ========================================================================

    /// Add a trace record operation
    pub fn trace_record(&mut self, span: impl Into<Vec<u8>>) -> &mut Self {
        self.entries
            .push(TxEntry::TraceRecord { span: span.into() });
        self
    }

    // ========================================================================
    // Run Operations
    // ========================================================================

    /// Add a run create operation
    pub fn run_create(&mut self, metadata: impl Into<Vec<u8>>) -> &mut Self {
        self.entries.push(TxEntry::RunCreate {
            metadata: metadata.into(),
        });
        self
    }

    /// Add a run update operation
    pub fn run_update(&mut self, metadata: impl Into<Vec<u8>>) -> &mut Self {
        self.entries.push(TxEntry::RunUpdate {
            metadata: metadata.into(),
        });
        self
    }

    /// Add a run begin operation
    pub fn run_begin(&mut self, metadata: impl Into<Vec<u8>>) -> &mut Self {
        self.entries.push(TxEntry::RunBegin {
            metadata: metadata.into(),
        });
        self
    }

    /// Add a run end operation
    pub fn run_end(&mut self, metadata: impl Into<Vec<u8>>) -> &mut Self {
        self.entries.push(TxEntry::RunEnd {
            metadata: metadata.into(),
        });
        self
    }

    // ========================================================================
    // Conversion to WAL Entries
    // ========================================================================

    /// Convert transaction to WAL entries
    ///
    /// Returns the tx_id and a vector of WAL entries. All entries share the
    /// same tx_id for atomic grouping. Note: This does NOT include the commit
    /// marker - that should be written separately after all entries.
    pub fn into_wal_entries(self) -> (TxId, Vec<WalEntry>) {
        let tx_id = self.id;
        let entries = self
            .entries
            .into_iter()
            .map(|entry| {
                let (entry_type, payload) = entry.to_wal_payload();
                WalEntry::new(entry_type, tx_id, payload)
            })
            .collect();

        (tx_id, entries)
    }

    /// Convert to WAL entries without consuming the transaction
    pub fn to_wal_entries(&self) -> (TxId, Vec<WalEntry>) {
        let tx_id = self.id;
        let entries = self
            .entries
            .iter()
            .map(|entry| {
                let (entry_type, payload) = entry.to_wal_payload();
                WalEntry::new(entry_type, tx_id, payload)
            })
            .collect();

        (tx_id, entries)
    }
}

impl TxEntry {
    /// Get the WAL entry type for this transaction entry
    pub fn entry_type(&self) -> WalEntryType {
        match self {
            TxEntry::KvPut { .. } => WalEntryType::KvPut,
            TxEntry::KvDelete { .. } => WalEntryType::KvDelete,
            TxEntry::JsonCreate { .. } => WalEntryType::JsonCreate,
            TxEntry::JsonSet { .. } => WalEntryType::JsonSet,
            TxEntry::JsonDelete { .. } => WalEntryType::JsonDelete,
            TxEntry::JsonPatch { .. } => WalEntryType::JsonPatch,
            TxEntry::EventAppend { .. } => WalEntryType::EventAppend,
            TxEntry::StateInit { .. } => WalEntryType::StateInit,
            TxEntry::StateSet { .. } => WalEntryType::StateSet,
            TxEntry::StateTransition { .. } => WalEntryType::StateTransition,
            TxEntry::TraceRecord { .. } => WalEntryType::TraceRecord,
            TxEntry::RunCreate { .. } => WalEntryType::RunCreate,
            TxEntry::RunUpdate { .. } => WalEntryType::RunUpdate,
            TxEntry::RunBegin { .. } => WalEntryType::RunBegin,
            TxEntry::RunEnd { .. } => WalEntryType::RunEnd,
        }
    }

    /// Serialize to WAL payload
    ///
    /// Payload format for key-value entries:
    /// - key_len: u32 (little-endian)
    /// - key: bytes
    /// - value: remaining bytes
    ///
    /// For single-payload entries:
    /// - payload: bytes
    pub fn to_wal_payload(&self) -> (WalEntryType, Vec<u8>) {
        let entry_type = self.entry_type();
        let payload = match self {
            TxEntry::KvPut { key, value } => serialize_kv(key, value),
            TxEntry::KvDelete { key } => key.clone(),
            TxEntry::JsonCreate { key, doc } => serialize_kv(key, doc),
            TxEntry::JsonSet { key, doc } => serialize_kv(key, doc),
            TxEntry::JsonDelete { key } => key.clone(),
            TxEntry::JsonPatch { key, patch } => serialize_kv(key, patch),
            TxEntry::EventAppend { payload } => payload.clone(),
            TxEntry::StateInit { key, value } => serialize_kv(key, value),
            TxEntry::StateSet { key, value } => serialize_kv(key, value),
            TxEntry::StateTransition { key, from, to } => serialize_transition(key, from, to),
            TxEntry::TraceRecord { span } => span.clone(),
            TxEntry::RunCreate { metadata } => metadata.clone(),
            TxEntry::RunUpdate { metadata } => metadata.clone(),
            TxEntry::RunBegin { metadata } => metadata.clone(),
            TxEntry::RunEnd { metadata } => metadata.clone(),
        };
        (entry_type, payload)
    }

    /// Parse from WAL entry type and payload
    pub fn from_wal_payload(entry_type: WalEntryType, payload: &[u8]) -> Option<Self> {
        match entry_type {
            WalEntryType::KvPut => {
                let (key, value) = deserialize_kv(payload)?;
                Some(TxEntry::KvPut { key, value })
            }
            WalEntryType::KvDelete => Some(TxEntry::KvDelete {
                key: payload.to_vec(),
            }),
            WalEntryType::JsonCreate => {
                let (key, doc) = deserialize_kv(payload)?;
                Some(TxEntry::JsonCreate { key, doc })
            }
            WalEntryType::JsonSet => {
                let (key, doc) = deserialize_kv(payload)?;
                Some(TxEntry::JsonSet { key, doc })
            }
            WalEntryType::JsonDelete => Some(TxEntry::JsonDelete {
                key: payload.to_vec(),
            }),
            WalEntryType::JsonPatch => {
                let (key, patch) = deserialize_kv(payload)?;
                Some(TxEntry::JsonPatch { key, patch })
            }
            WalEntryType::EventAppend => Some(TxEntry::EventAppend {
                payload: payload.to_vec(),
            }),
            WalEntryType::StateInit => {
                let (key, value) = deserialize_kv(payload)?;
                Some(TxEntry::StateInit { key, value })
            }
            WalEntryType::StateSet => {
                let (key, value) = deserialize_kv(payload)?;
                Some(TxEntry::StateSet { key, value })
            }
            WalEntryType::StateTransition => {
                let (key, from, to) = deserialize_transition(payload)?;
                Some(TxEntry::StateTransition { key, from, to })
            }
            WalEntryType::TraceRecord => Some(TxEntry::TraceRecord {
                span: payload.to_vec(),
            }),
            WalEntryType::RunCreate => Some(TxEntry::RunCreate {
                metadata: payload.to_vec(),
            }),
            WalEntryType::RunUpdate => Some(TxEntry::RunUpdate {
                metadata: payload.to_vec(),
            }),
            WalEntryType::RunBegin => Some(TxEntry::RunBegin {
                metadata: payload.to_vec(),
            }),
            WalEntryType::RunEnd => Some(TxEntry::RunEnd {
                metadata: payload.to_vec(),
            }),
            // Control entries are not transaction entries
            WalEntryType::TransactionCommit
            | WalEntryType::TransactionAbort
            | WalEntryType::SnapshotMarker => None,
        }
    }
}

/// Serialize key-value pair
///
/// Format: key_len (u32 LE) + key + value
fn serialize_kv(key: &[u8], value: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(4 + key.len() + value.len());
    buf.extend_from_slice(&(key.len() as u32).to_le_bytes());
    buf.extend_from_slice(key);
    buf.extend_from_slice(value);
    buf
}

/// Deserialize key-value pair
fn deserialize_kv(data: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
    if data.len() < 4 {
        return None;
    }
    let key_len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    if data.len() < 4 + key_len {
        return None;
    }
    let key = data[4..4 + key_len].to_vec();
    let value = data[4 + key_len..].to_vec();
    Some((key, value))
}

/// Serialize state transition (key, from, to)
///
/// Format: key_len (u32 LE) + key + from_len (u32 LE) + from + to
fn serialize_transition(key: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(8 + key.len() + from.len() + to.len());
    buf.extend_from_slice(&(key.len() as u32).to_le_bytes());
    buf.extend_from_slice(key);
    buf.extend_from_slice(&(from.len() as u32).to_le_bytes());
    buf.extend_from_slice(from);
    buf.extend_from_slice(to);
    buf
}

/// Deserialize state transition
fn deserialize_transition(data: &[u8]) -> Option<(Vec<u8>, Vec<u8>, Vec<u8>)> {
    if data.len() < 4 {
        return None;
    }
    let key_len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    if data.len() < 4 + key_len + 4 {
        return None;
    }
    let key = data[4..4 + key_len].to_vec();
    let from_offset = 4 + key_len;
    let from_len = u32::from_le_bytes([
        data[from_offset],
        data[from_offset + 1],
        data[from_offset + 2],
        data[from_offset + 3],
    ]) as usize;
    if data.len() < from_offset + 4 + from_len {
        return None;
    }
    let from = data[from_offset + 4..from_offset + 4 + from_len].to_vec();
    let to = data[from_offset + 4 + from_len..].to_vec();
    Some((key, from, to))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_new() {
        let tx = Transaction::new();
        assert!(tx.is_empty());
        assert_eq!(tx.len(), 0);
        // tx_id should be valid UUID
        assert!(!tx.id().is_nil());
    }

    #[test]
    fn test_transaction_with_id() {
        let id = TxId::new();
        let tx = Transaction::with_id(id);
        assert_eq!(tx.id(), id);
    }

    #[test]
    fn test_transaction_builder_kv() {
        let mut tx = Transaction::new();
        tx.kv_put("key1", "value1").kv_delete("key2");

        assert_eq!(tx.len(), 2);

        let entries = tx.entries();
        assert!(matches!(entries[0], TxEntry::KvPut { .. }));
        assert!(matches!(entries[1], TxEntry::KvDelete { .. }));
    }

    #[test]
    fn test_transaction_builder_json() {
        let mut tx = Transaction::new();
        tx.json_create("doc1", b"{\"a\":1}".to_vec())
            .json_set("doc2", b"{\"b\":2}".to_vec())
            .json_delete("doc3")
            .json_patch("doc4", b"[]".to_vec());

        assert_eq!(tx.len(), 4);
    }

    #[test]
    fn test_transaction_builder_event() {
        let mut tx = Transaction::new();
        tx.event_append(b"event_data".to_vec());

        assert_eq!(tx.len(), 1);
        assert!(matches!(&tx.entries()[0], TxEntry::EventAppend { .. }));
    }

    #[test]
    fn test_transaction_builder_state() {
        let mut tx = Transaction::new();
        tx.state_init("state1", "initial")
            .state_set("state2", "value")
            .state_transition("state3", "from", "to");

        assert_eq!(tx.len(), 3);
    }

    #[test]
    fn test_transaction_builder_trace() {
        let mut tx = Transaction::new();
        tx.trace_record(b"span_data".to_vec());

        assert_eq!(tx.len(), 1);
        assert!(matches!(&tx.entries()[0], TxEntry::TraceRecord { .. }));
    }

    #[test]
    fn test_transaction_builder_run() {
        let mut tx = Transaction::new();
        tx.run_create(b"create_meta".to_vec())
            .run_begin(b"begin_meta".to_vec())
            .run_update(b"update_meta".to_vec())
            .run_end(b"end_meta".to_vec());

        assert_eq!(tx.len(), 4);
    }

    #[test]
    fn test_cross_primitive_transaction() {
        let mut tx = Transaction::new();
        tx.kv_put("kv_key", "kv_value")
            .json_set("json_key", b"{\"field\":\"value\"}".to_vec())
            .event_append(b"event_payload".to_vec())
            .state_set("state_key", "active")
            .trace_record(b"trace_span".to_vec());

        assert_eq!(tx.len(), 5);

        // All entries should share the same tx_id
        let (tx_id, entries) = tx.into_wal_entries();
        assert!(!tx_id.is_nil());
        assert_eq!(entries.len(), 5);

        for entry in &entries {
            assert_eq!(entry.tx_id, tx_id);
        }
    }

    #[test]
    fn test_tx_entry_to_wal_payload() {
        let entry = TxEntry::KvPut {
            key: b"test_key".to_vec(),
            value: b"test_value".to_vec(),
        };

        let (entry_type, payload) = entry.to_wal_payload();
        assert_eq!(entry_type, WalEntryType::KvPut);

        // Verify payload structure
        let key_len = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]) as usize;
        assert_eq!(key_len, 8); // "test_key"
        assert_eq!(&payload[4..12], b"test_key");
        assert_eq!(&payload[12..], b"test_value");
    }

    #[test]
    fn test_tx_entry_roundtrip() {
        let entries = vec![
            TxEntry::KvPut {
                key: b"key".to_vec(),
                value: b"value".to_vec(),
            },
            TxEntry::KvDelete {
                key: b"key".to_vec(),
            },
            TxEntry::JsonCreate {
                key: b"doc".to_vec(),
                doc: b"{}".to_vec(),
            },
            TxEntry::JsonSet {
                key: b"doc".to_vec(),
                doc: b"{\"a\":1}".to_vec(),
            },
            TxEntry::JsonDelete {
                key: b"doc".to_vec(),
            },
            TxEntry::JsonPatch {
                key: b"doc".to_vec(),
                patch: b"[]".to_vec(),
            },
            TxEntry::EventAppend {
                payload: b"event".to_vec(),
            },
            TxEntry::StateInit {
                key: b"state".to_vec(),
                value: b"init".to_vec(),
            },
            TxEntry::StateSet {
                key: b"state".to_vec(),
                value: b"set".to_vec(),
            },
            TxEntry::StateTransition {
                key: b"state".to_vec(),
                from: b"a".to_vec(),
                to: b"b".to_vec(),
            },
            TxEntry::TraceRecord {
                span: b"trace".to_vec(),
            },
            TxEntry::RunCreate {
                metadata: b"run".to_vec(),
            },
            TxEntry::RunUpdate {
                metadata: b"run".to_vec(),
            },
            TxEntry::RunBegin {
                metadata: b"run".to_vec(),
            },
            TxEntry::RunEnd {
                metadata: b"run".to_vec(),
            },
        ];

        for entry in entries {
            let (entry_type, payload) = entry.to_wal_payload();
            let parsed = TxEntry::from_wal_payload(entry_type, &payload).unwrap();
            assert_eq!(entry, parsed);
        }
    }

    #[test]
    fn test_to_wal_entries_non_consuming() {
        let mut tx = Transaction::new();
        tx.kv_put("key1", "value1").kv_put("key2", "value2");

        // Non-consuming version
        let (tx_id1, entries1) = tx.to_wal_entries();

        // Can still access transaction
        assert_eq!(tx.len(), 2);
        assert_eq!(tx.id(), tx_id1);

        // Entries match
        assert_eq!(entries1.len(), 2);
    }

    #[test]
    fn test_serialize_kv() {
        let key = b"hello";
        let value = b"world";
        let payload = serialize_kv(key, value);

        let (parsed_key, parsed_value) = deserialize_kv(&payload).unwrap();
        assert_eq!(parsed_key, key);
        assert_eq!(parsed_value, value);
    }

    #[test]
    fn test_serialize_transition() {
        let key = b"state";
        let from = b"old";
        let to = b"new";
        let payload = serialize_transition(key, from, to);

        let (parsed_key, parsed_from, parsed_to) = deserialize_transition(&payload).unwrap();
        assert_eq!(parsed_key, key);
        assert_eq!(parsed_from, from);
        assert_eq!(parsed_to, to);
    }

    #[test]
    fn test_deserialize_kv_invalid() {
        // Too short
        assert!(deserialize_kv(&[]).is_none());
        assert!(deserialize_kv(&[1, 2, 3]).is_none());

        // Key length exceeds data
        let mut bad = vec![0, 0, 0, 100]; // key_len = 100
        bad.push(0); // only 1 byte of data
        assert!(deserialize_kv(&bad).is_none());
    }

    #[test]
    fn test_deserialize_transition_invalid() {
        // Too short
        assert!(deserialize_transition(&[]).is_none());
        assert!(deserialize_transition(&[1, 2, 3]).is_none());

        // Key length exceeds data
        let bad = vec![0, 0, 0, 100, 0];
        assert!(deserialize_transition(&bad).is_none());
    }

    #[test]
    fn test_from_wal_payload_control_entries() {
        // Control entries should return None
        assert!(TxEntry::from_wal_payload(WalEntryType::TransactionCommit, &[]).is_none());
        assert!(TxEntry::from_wal_payload(WalEntryType::TransactionAbort, &[]).is_none());
        assert!(TxEntry::from_wal_payload(WalEntryType::SnapshotMarker, &[]).is_none());
    }

    #[test]
    fn test_entry_types_correct() {
        assert_eq!(
            TxEntry::KvPut {
                key: vec![],
                value: vec![]
            }
            .entry_type(),
            WalEntryType::KvPut
        );
        assert_eq!(
            TxEntry::KvDelete { key: vec![] }.entry_type(),
            WalEntryType::KvDelete
        );
        assert_eq!(
            TxEntry::JsonCreate {
                key: vec![],
                doc: vec![]
            }
            .entry_type(),
            WalEntryType::JsonCreate
        );
        assert_eq!(
            TxEntry::JsonSet {
                key: vec![],
                doc: vec![]
            }
            .entry_type(),
            WalEntryType::JsonSet
        );
        assert_eq!(
            TxEntry::JsonDelete { key: vec![] }.entry_type(),
            WalEntryType::JsonDelete
        );
        assert_eq!(
            TxEntry::JsonPatch {
                key: vec![],
                patch: vec![]
            }
            .entry_type(),
            WalEntryType::JsonPatch
        );
        assert_eq!(
            TxEntry::EventAppend { payload: vec![] }.entry_type(),
            WalEntryType::EventAppend
        );
        assert_eq!(
            TxEntry::StateInit {
                key: vec![],
                value: vec![]
            }
            .entry_type(),
            WalEntryType::StateInit
        );
        assert_eq!(
            TxEntry::StateSet {
                key: vec![],
                value: vec![]
            }
            .entry_type(),
            WalEntryType::StateSet
        );
        assert_eq!(
            TxEntry::StateTransition {
                key: vec![],
                from: vec![],
                to: vec![]
            }
            .entry_type(),
            WalEntryType::StateTransition
        );
        assert_eq!(
            TxEntry::TraceRecord { span: vec![] }.entry_type(),
            WalEntryType::TraceRecord
        );
        assert_eq!(
            TxEntry::RunCreate { metadata: vec![] }.entry_type(),
            WalEntryType::RunCreate
        );
        assert_eq!(
            TxEntry::RunUpdate { metadata: vec![] }.entry_type(),
            WalEntryType::RunUpdate
        );
        assert_eq!(
            TxEntry::RunBegin { metadata: vec![] }.entry_type(),
            WalEntryType::RunBegin
        );
        assert_eq!(
            TxEntry::RunEnd { metadata: vec![] }.entry_type(),
            WalEntryType::RunEnd
        );
    }

    #[test]
    fn test_transaction_default() {
        let tx = Transaction::default();
        assert!(tx.is_empty());
        assert!(!tx.id().is_nil());
    }
}
