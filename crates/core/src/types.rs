//! Core types for Strata database
//!
//! This module defines the foundational types:
//! - RunId: Unique identifier for agent runs
//! - Namespace: Hierarchical namespace (tenant/app/agent/run)
//! - TypeTag: Type discriminator for unified storage
//! - Key: Composite key (namespace + type_tag + user_key)

use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// Unique identifier for an agent run
///
/// A RunId is a wrapper around a UUID v4, providing unique identification
/// for each agent execution run. RunIds are used throughout the system
/// to scope data and enable run-specific queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunId(Uuid);

impl RunId {
    /// Create a new random RunId using UUID v4
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Create a RunId from raw bytes
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(Uuid::from_bytes(bytes))
    }

    /// Parse a RunId from a string representation
    ///
    /// Accepts standard UUID format (with or without hyphens).
    ///
    /// # Errors
    /// Returns None if the string is not a valid UUID.
    pub fn from_string(s: &str) -> Option<Self> {
        Uuid::parse_str(s).ok().map(Self)
    }

    /// Get the raw bytes of this RunId
    pub fn as_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }
}

impl Default for RunId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for RunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Hierarchical namespace: tenant → app → agent → run
///
/// Namespaces provide multi-tenant isolation and hierarchical organization
/// of data. The hierarchy enables efficient querying and access control.
///
/// Format: "tenant/app/agent/run_id"
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Namespace {
    /// Tenant identifier (top-level isolation)
    pub tenant: String,
    /// Application identifier
    pub app: String,
    /// Agent identifier
    pub agent: String,
    /// Run identifier
    pub run_id: RunId,
}

impl Namespace {
    /// Create a new namespace
    pub fn new(tenant: String, app: String, agent: String, run_id: RunId) -> Self {
        Self {
            tenant,
            app,
            agent,
            run_id,
        }
    }

    /// Create a namespace for a run with default tenant/app/agent
    ///
    /// This is a convenience method for M3 primitives that only need
    /// run-level isolation. Uses "default" for tenant, app, and agent.
    pub fn for_run(run_id: RunId) -> Self {
        Self {
            tenant: "default".to_string(),
            app: "default".to_string(),
            agent: "default".to_string(),
            run_id,
        }
    }
}

impl fmt::Display for Namespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}/{}/{}/{}",
            self.tenant, self.app, self.agent, self.run_id
        )
    }
}

// Ord implementation for BTreeMap key ordering
// Orders by: tenant → app → agent → run_id
impl Ord for Namespace {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.tenant
            .cmp(&other.tenant)
            .then(self.app.cmp(&other.app))
            .then(self.agent.cmp(&other.agent))
            .then(self.run_id.0.cmp(&other.run_id.0))
    }
}

impl PartialOrd for Namespace {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Type tag for discriminating primitive types in unified storage
///
/// The unified storage design uses a single BTreeMap with type-tagged keys
/// instead of separate stores per primitive. This TypeTag enum enables
/// type discrimination and defines the sort order in BTreeMap.
///
/// ## TypeTag Values
///
/// These values are part of the on-disk format and MUST NOT change:
/// - KV = 0x01
/// - Event = 0x02
/// - State = 0x03
/// - Run = 0x05
/// - Vector = 0x10 (M8 vector metadata)
/// - Json = 0x11 (M5 JSON primitive)
/// - VectorConfig = 0x12 (M8 vector collection config)
///
/// Note: 0x04 was formerly Trace (TraceStore was removed in 0.12.0)
///
/// Ordering: KV < Event < State < Run < Vector < Json < VectorConfig
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[repr(u8)]
pub enum TypeTag {
    /// Key-Value primitive data
    KV = 0x01,
    /// Event log entries
    Event = 0x02,
    /// State cell records (renamed from StateMachine in M3)
    State = 0x03,
    /// Reserved for backwards compatibility (TraceStore was removed)
    #[deprecated(since = "0.12.0", note = "TraceStore primitive was removed")]
    Trace = 0x04,
    /// Run index entries
    Run = 0x05,
    /// Vector store entries (M8)
    Vector = 0x10,
    /// JSON document store entries (M5)
    Json = 0x11,
    /// Vector collection configuration (M8)
    VectorConfig = 0x12,
}

impl TypeTag {
    /// Convert to byte representation
    pub fn as_byte(&self) -> u8 {
        *self as u8
    }

    /// Try to create from byte
    #[allow(deprecated)]
    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0x01 => Some(TypeTag::KV),
            0x02 => Some(TypeTag::Event),
            0x03 => Some(TypeTag::State),
            0x04 => Some(TypeTag::Trace), // Deprecated but needed for backwards compatibility
            0x05 => Some(TypeTag::Run),
            0x10 => Some(TypeTag::Vector),
            0x11 => Some(TypeTag::Json),
            0x12 => Some(TypeTag::VectorConfig),
            _ => None,
        }
    }
}

/// Unique identifier for a JSON document within a run
///
/// Each document has a unique ID that persists for its lifetime.
/// IDs are UUIDs to ensure global uniqueness. JsonDocId is designed
/// to be small (Copy) and efficient for use as keys.
///
/// # Examples
///
/// ```
/// use strata_core::JsonDocId;
///
/// let id1 = JsonDocId::new();
/// let id2 = JsonDocId::new();
/// assert_ne!(id1, id2); // UUIDs are unique
///
/// // Round-trip through bytes
/// let bytes = id1.as_bytes();
/// let recovered = JsonDocId::try_from_bytes(bytes).unwrap();
/// assert_eq!(id1, recovered);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JsonDocId(Uuid);

impl JsonDocId {
    /// Create a new unique document ID using UUID v4
    pub fn new() -> Self {
        JsonDocId(Uuid::new_v4())
    }

    /// Create from existing UUID (for deserialization/recovery)
    pub fn from_uuid(uuid: Uuid) -> Self {
        JsonDocId(uuid)
    }

    /// Get the underlying UUID
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }

    /// Get bytes for key encoding (16 bytes)
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    /// Try to parse from bytes (for key decoding)
    ///
    /// Returns None if bytes length is not exactly 16.
    pub fn try_from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() == 16 {
            Uuid::from_slice(bytes).ok().map(JsonDocId)
        } else {
            None
        }
    }
}

impl Default for JsonDocId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for JsonDocId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unified key for all storage types
///
/// A Key combines namespace, type tag, and user-defined key bytes to create
/// a composite key that enables efficient prefix scans and type discrimination
/// in the unified BTreeMap storage.
///
/// # Ordering
///
/// Keys are ordered by: namespace → type_tag → user_key
///
/// This ordering is critical for BTreeMap efficiency:
/// - All keys for a namespace are grouped together
/// - Within a namespace, keys are grouped by type
/// - Within a type, keys are ordered by user_key (enabling prefix scans)
///
/// # Examples
///
/// ```
/// use strata_core::{Key, Namespace, TypeTag, RunId};
///
/// let run_id = RunId::new();
/// let ns = Namespace::new("tenant".to_string(), "app".to_string(),
///                         "agent".to_string(), run_id);
///
/// // Create a KV key
/// let key = Key::new_kv(ns.clone(), "session_state");
///
/// // Create an event key with sequence number
/// let event_key = Key::new_event(ns.clone(), 42);
///
/// // Create a prefix for scanning
/// let prefix = Key::new_kv(ns.clone(), "user:");
/// let user_key = Key::new_kv(ns.clone(), "user:alice");
/// assert!(user_key.starts_with(&prefix));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Key {
    /// Namespace (tenant/app/agent/run hierarchy)
    pub namespace: Namespace,
    /// Type discriminator (KV, Event, State, Trace, Run, etc.)
    pub type_tag: TypeTag,
    /// User-defined key bytes (supports arbitrary binary keys)
    pub user_key: Vec<u8>,
}

impl Key {
    /// Create a new key with the given namespace, type tag, and user key
    pub fn new(namespace: Namespace, type_tag: TypeTag, user_key: Vec<u8>) -> Self {
        Self {
            namespace,
            type_tag,
            user_key,
        }
    }

    /// Create a KV key
    ///
    /// Helper that automatically sets type_tag to TypeTag::KV
    pub fn new_kv(namespace: Namespace, key: impl AsRef<[u8]>) -> Self {
        Self::new(namespace, TypeTag::KV, key.as_ref().to_vec())
    }

    /// Create an event key with sequence number
    ///
    /// Helper that automatically sets type_tag to TypeTag::Event and
    /// encodes the sequence number as big-endian bytes
    pub fn new_event(namespace: Namespace, seq: u64) -> Self {
        Self::new(namespace, TypeTag::Event, seq.to_be_bytes().to_vec())
    }

    /// Create an event log metadata key
    ///
    /// The metadata key stores: { next_sequence: u64, head_hash: [u8; 32] }
    pub fn new_event_meta(namespace: Namespace) -> Self {
        Self::new(namespace, TypeTag::Event, b"__meta__".to_vec())
    }

    /// Create a state cell key
    ///
    /// Helper that automatically sets type_tag to TypeTag::State
    pub fn new_state(namespace: Namespace, key: impl AsRef<[u8]>) -> Self {
        Self::new(namespace, TypeTag::State, key.as_ref().to_vec())
    }

    /// Create a run index key
    ///
    /// Helper that automatically sets type_tag to TypeTag::Run and
    /// uses the run_id as the key
    pub fn new_run(namespace: Namespace, run_id: RunId) -> Self {
        Self::new(namespace, TypeTag::Run, run_id.as_bytes().to_vec())
    }

    /// Create a run index key from string run_id
    ///
    /// Alternative helper that accepts string run_id for index keys
    pub fn new_run_with_id(namespace: Namespace, run_id: &str) -> Self {
        Self::new(namespace, TypeTag::Run, run_id.as_bytes().to_vec())
    }

    /// Create a run index secondary index key
    ///
    /// Index keys enable efficient queries by status, tag, or parent.
    /// Format: `__idx_{index_type}__{index_value}__{run_id}`
    ///
    /// Example index types:
    /// - by-status: `__idx_status__Active__run123`
    /// - by-tag: `__idx_tag__experiment__run123`
    /// - by-parent: `__idx_parent__parent123__run123`
    pub fn new_run_index(
        namespace: Namespace,
        index_type: &str,
        index_value: &str,
        run_id: &str,
    ) -> Self {
        let key_data = format!("__idx_{}__{}__{}", index_type, index_value, run_id);
        Self::new(namespace, TypeTag::Run, key_data.into_bytes())
    }

    /// Create key for JSON document storage
    ///
    /// Helper that automatically sets type_tag to TypeTag::Json and
    /// uses the JsonDocId bytes as the key.
    ///
    /// # Example
    ///
    /// ```
    /// use strata_core::{Key, Namespace, TypeTag, RunId, JsonDocId};
    ///
    /// let run_id = RunId::new();
    /// let doc_id = JsonDocId::new();
    /// let namespace = Namespace::for_run(run_id);
    /// let key = Key::new_json(namespace, &doc_id);
    /// assert_eq!(key.type_tag, TypeTag::Json);
    /// ```
    pub fn new_json(namespace: Namespace, doc_id: &JsonDocId) -> Self {
        Self::new(namespace, TypeTag::Json, doc_id.as_bytes().to_vec())
    }

    /// Create prefix for scanning all JSON docs in namespace
    ///
    /// This key can be used with starts_with() to match all JSON
    /// documents in a namespace.
    pub fn new_json_prefix(namespace: Namespace) -> Self {
        Self::new(namespace, TypeTag::Json, vec![])
    }

    /// Create key for vector metadata
    ///
    /// Format: namespace + TypeTag::Vector + collection_name + "/" + vector_key
    pub fn new_vector(namespace: Namespace, collection: &str, key: &str) -> Self {
        let user_key = format!("{}/{}", collection, key);
        Self::new(namespace, TypeTag::Vector, user_key.into_bytes())
    }

    /// Create key for collection configuration
    ///
    /// Format: namespace + TypeTag::VectorConfig + collection_name
    pub fn new_vector_config(namespace: Namespace, collection: &str) -> Self {
        Self::new(
            namespace,
            TypeTag::VectorConfig,
            collection.as_bytes().to_vec(),
        )
    }

    /// Create prefix for scanning all vectors in a collection
    pub fn vector_collection_prefix(namespace: Namespace, collection: &str) -> Self {
        let user_key = format!("{}/", collection);
        Self::new(namespace, TypeTag::Vector, user_key.into_bytes())
    }

    /// Create prefix for scanning all vector collections
    pub fn new_vector_config_prefix(namespace: Namespace) -> Self {
        Self::new(namespace, TypeTag::VectorConfig, vec![])
    }

    /// Extract user key as string (if valid UTF-8)
    ///
    /// Returns None if the user_key is not valid UTF-8
    pub fn user_key_string(&self) -> Option<String> {
        String::from_utf8(self.user_key.clone()).ok()
    }

    /// Check if this key starts with the given prefix
    ///
    /// For a key to match a prefix:
    /// - namespace must be equal
    /// - type_tag must be equal
    /// - user_key must start with prefix.user_key
    ///
    /// This enables efficient prefix scans in BTreeMap:
    /// ```
    /// # use strata_core::{Key, Namespace, RunId};
    /// # let run_id = RunId::new();
    /// # let ns = Namespace::new("t".to_string(), "a".to_string(), "ag".to_string(), run_id);
    /// let prefix = Key::new_kv(ns.clone(), "user:");
    /// let key = Key::new_kv(ns.clone(), "user:alice");
    /// assert!(key.starts_with(&prefix));
    /// ```
    pub fn starts_with(&self, prefix: &Key) -> bool {
        self.namespace == prefix.namespace
            && self.type_tag == prefix.type_tag
            && self.user_key.starts_with(&prefix.user_key)
    }
}

/// Ordering implementation for BTreeMap
///
/// Keys are ordered by: namespace → type_tag → user_key
/// This ordering is critical for efficient prefix scans
impl Ord for Key {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.namespace
            .cmp(&other.namespace)
            .then(self.type_tag.cmp(&other.type_tag))
            .then(self.user_key.cmp(&other.user_key))
    }
}

impl PartialOrd for Key {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================
    // RunId Tests
    // ========================================

    #[test]
    fn test_run_id_creation_uniqueness() {
        let id1 = RunId::new();
        let id2 = RunId::new();
        assert_ne!(id1, id2, "RunIds should be unique");
    }

    #[test]
    fn test_run_id_serialization_roundtrip() {
        let id = RunId::new();
        let bytes = id.as_bytes();
        let restored = RunId::from_bytes(*bytes);
        assert_eq!(id, restored, "RunId should roundtrip through bytes");
    }

    #[test]
    fn test_run_id_display() {
        let id = RunId::new();
        let s = format!("{}", id);
        assert!(!s.is_empty(), "Display should produce non-empty string");
        assert_eq!(
            s.len(),
            36,
            "UUID v4 should format as 36 characters with hyphens"
        );
    }

    #[test]
    fn test_run_id_hash_consistency() {
        use std::collections::HashSet;

        let id1 = RunId::new();
        let id2 = id1; // Copy

        let mut set = HashSet::new();
        set.insert(id1);

        assert!(
            set.contains(&id2),
            "Hash should be consistent for copied RunId"
        );

        let id3 = RunId::new();
        set.insert(id3);

        assert_eq!(
            set.len(),
            2,
            "Different RunIds should have different hashes"
        );
    }

    #[test]
    fn test_run_id_default() {
        let id1 = RunId::default();
        let id2 = RunId::default();
        assert_ne!(id1, id2, "Default RunIds should be unique");
    }

    #[test]
    fn test_run_id_clone() {
        let id1 = RunId::new();
        let id2 = id1.clone();
        assert_eq!(id1, id2, "Cloned RunId should equal original");
    }

    #[test]
    fn test_run_id_debug() {
        let id = RunId::new();
        let debug_str = format!("{:?}", id);
        assert!(
            debug_str.contains("RunId"),
            "Debug should include type name"
        );
    }

    // ========================================
    // Namespace Tests
    // ========================================

    #[test]
    fn test_namespace_construction() {
        let run_id = RunId::new();
        let ns = Namespace::new(
            "acme".to_string(),
            "chatbot".to_string(),
            "agent-42".to_string(),
            run_id,
        );

        assert_eq!(ns.tenant, "acme");
        assert_eq!(ns.app, "chatbot");
        assert_eq!(ns.agent, "agent-42");
        assert_eq!(ns.run_id, run_id);
    }

    #[test]
    fn test_namespace_display_format() {
        let run_id = RunId::new();
        let ns = Namespace::new(
            "acme".to_string(),
            "chatbot".to_string(),
            "agent-42".to_string(),
            run_id,
        );

        let display_str = format!("{}", ns);
        let expected = format!("acme/chatbot/agent-42/{}", run_id);
        assert_eq!(
            display_str, expected,
            "Namespace should format as tenant/app/agent/run_id"
        );
    }

    #[test]
    fn test_namespace_equality() {
        let run_id1 = RunId::new();
        let run_id2 = RunId::new();

        let ns1 = Namespace::new(
            "acme".to_string(),
            "chatbot".to_string(),
            "agent-42".to_string(),
            run_id1,
        );

        let ns2 = Namespace::new(
            "acme".to_string(),
            "chatbot".to_string(),
            "agent-42".to_string(),
            run_id1,
        );

        let ns3 = Namespace::new(
            "acme".to_string(),
            "chatbot".to_string(),
            "agent-42".to_string(),
            run_id2,
        );

        assert_eq!(ns1, ns2, "Namespaces with same values should be equal");
        assert_ne!(
            ns1, ns3,
            "Namespaces with different run_ids should not be equal"
        );
    }

    #[test]
    fn test_namespace_clone() {
        let run_id = RunId::new();
        let ns1 = Namespace::new(
            "acme".to_string(),
            "chatbot".to_string(),
            "agent-42".to_string(),
            run_id,
        );

        let ns2 = ns1.clone();
        assert_eq!(ns1, ns2, "Cloned namespace should equal original");
    }

    #[test]
    fn test_namespace_debug() {
        let run_id = RunId::new();
        let ns = Namespace::new(
            "acme".to_string(),
            "chatbot".to_string(),
            "agent-42".to_string(),
            run_id,
        );

        let debug_str = format!("{:?}", ns);
        assert!(
            debug_str.contains("Namespace"),
            "Debug should include type name"
        );
        assert!(debug_str.contains("acme"), "Debug should include tenant");
    }

    #[test]
    fn test_namespace_with_special_characters() {
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant-1".to_string(),
            "my_app".to_string(),
            "agent.42".to_string(),
            run_id,
        );

        let display = format!("{}", ns);
        assert!(display.contains("tenant-1"));
        assert!(display.contains("my_app"));
        assert!(display.contains("agent.42"));
    }

    #[test]
    fn test_namespace_with_empty_strings() {
        let run_id = RunId::new();
        let ns = Namespace::new("".to_string(), "".to_string(), "".to_string(), run_id);

        // Should still construct, even if semantically invalid
        assert_eq!(ns.tenant, "");
        assert_eq!(ns.app, "");
        assert_eq!(ns.agent, "");
    }

    #[test]
    fn test_namespace_ordering() {
        let run1 = RunId::new();
        let run2 = RunId::new();

        let ns1 = Namespace::new(
            "tenant1".to_string(),
            "app1".to_string(),
            "agent1".to_string(),
            run1,
        );
        let ns2 = Namespace::new(
            "tenant1".to_string(),
            "app1".to_string(),
            "agent1".to_string(),
            run2,
        );
        let ns3 = Namespace::new(
            "tenant2".to_string(),
            "app1".to_string(),
            "agent1".to_string(),
            run1,
        );
        let ns4 = Namespace::new(
            "tenant1".to_string(),
            "app2".to_string(),
            "agent1".to_string(),
            run1,
        );
        let ns5 = Namespace::new(
            "tenant1".to_string(),
            "app1".to_string(),
            "agent2".to_string(),
            run1,
        );

        // Same tenant/app/agent, different run_id - order depends on UUID
        assert_ne!(ns1, ns2);

        // Different tenant should sort differently
        assert!(ns1 < ns3, "tenant1 should be less than tenant2");

        // Different app within same tenant
        assert!(ns1 < ns4, "app1 should be less than app2");

        // Different agent within same tenant/app
        assert!(ns5 > ns1, "agent2 should be greater than agent1");
    }

    #[test]
    fn test_namespace_serialization() {
        let run_id = RunId::new();
        let ns = Namespace::new(
            "acme".to_string(),
            "myapp".to_string(),
            "agent-42".to_string(),
            run_id,
        );

        let json = serde_json::to_string(&ns).unwrap();
        let ns2: Namespace = serde_json::from_str(&json).unwrap();

        assert_eq!(ns, ns2, "Namespace should roundtrip through JSON");
    }

    #[test]
    fn test_namespace_btreemap_ordering() {
        use std::collections::BTreeMap;

        let run1 = RunId::new();
        let run2 = RunId::new();

        let ns1 = Namespace::new(
            "acme".to_string(),
            "app1".to_string(),
            "agent1".to_string(),
            run1,
        );
        let ns2 = Namespace::new(
            "acme".to_string(),
            "app1".to_string(),
            "agent2".to_string(),
            run2,
        );
        let ns3 = Namespace::new(
            "acme".to_string(),
            "app2".to_string(),
            "agent1".to_string(),
            run1,
        );

        let mut map = BTreeMap::new();
        map.insert(ns3.clone(), "value3");
        map.insert(ns1.clone(), "value1");
        map.insert(ns2.clone(), "value2");

        // Collect keys in order
        let keys: Vec<_> = map.keys().cloned().collect();

        // Should be ordered: ns1 (app1/agent1) < ns2 (app1/agent2) < ns3 (app2/agent1)
        assert_eq!(keys[0], ns1);
        assert_eq!(keys[1], ns2);
        assert_eq!(keys[2], ns3);
    }

    // ========================================
    // TypeTag Tests
    // ========================================

    #[test]
    fn test_typetag_variants() {
        // Test that all TypeTag variants can be constructed
        let _kv = TypeTag::KV;
        let _event = TypeTag::Event;
        let _state = TypeTag::State;
        let _run = TypeTag::Run;
        let _vector = TypeTag::Vector;
        let _json = TypeTag::Json;
    }

    #[test]
    #[allow(deprecated)]
    fn test_typetag_ordering() {
        // TypeTag ordering must be stable for BTreeMap
        assert!(TypeTag::KV < TypeTag::Event);
        assert!(TypeTag::Event < TypeTag::State);
        assert!(TypeTag::State < TypeTag::Run);
        assert!(TypeTag::Run < TypeTag::Vector);
        assert!(TypeTag::Vector < TypeTag::Json);

        // Verify numeric values match spec
        assert_eq!(TypeTag::KV as u8, 0x01);
        assert_eq!(TypeTag::Event as u8, 0x02);
        assert_eq!(TypeTag::State as u8, 0x03);
        // TypeTag::Trace (0x04) is deprecated but still exists for backwards compatibility
        assert_eq!(TypeTag::Trace as u8, 0x04);
        assert_eq!(TypeTag::Run as u8, 0x05);
        assert_eq!(TypeTag::Vector as u8, 0x10);
        assert_eq!(TypeTag::Json as u8, 0x11);
    }

    #[test]
    #[allow(deprecated)]
    fn test_typetag_as_byte() {
        assert_eq!(TypeTag::KV.as_byte(), 0x01);
        assert_eq!(TypeTag::Event.as_byte(), 0x02);
        assert_eq!(TypeTag::State.as_byte(), 0x03);
        // TypeTag::Trace (0x04) is deprecated but still exists for backwards compatibility
        assert_eq!(TypeTag::Trace.as_byte(), 0x04);
        assert_eq!(TypeTag::Run.as_byte(), 0x05);
        assert_eq!(TypeTag::Vector.as_byte(), 0x10);
        assert_eq!(TypeTag::Json.as_byte(), 0x11);
    }

    #[test]
    #[allow(deprecated)]
    fn test_typetag_from_byte() {
        assert_eq!(TypeTag::from_byte(0x01), Some(TypeTag::KV));
        assert_eq!(TypeTag::from_byte(0x02), Some(TypeTag::Event));
        assert_eq!(TypeTag::from_byte(0x03), Some(TypeTag::State));
        // 0x04 still parses to Trace for backwards compatibility
        assert_eq!(TypeTag::from_byte(0x04), Some(TypeTag::Trace));
        assert_eq!(TypeTag::from_byte(0x05), Some(TypeTag::Run));
        assert_eq!(TypeTag::from_byte(0x10), Some(TypeTag::Vector));
        assert_eq!(TypeTag::from_byte(0x11), Some(TypeTag::Json));
        assert_eq!(TypeTag::from_byte(0x00), None);
        assert_eq!(TypeTag::from_byte(0xFF), None);
    }

    #[test]
    fn test_typetag_no_collisions() {
        // Ensure all TypeTag values are unique
        let tags = [
            TypeTag::KV,
            TypeTag::Event,
            TypeTag::State,
            TypeTag::Run,
            TypeTag::Vector,
            TypeTag::Json,
        ];
        let bytes: Vec<u8> = tags.iter().map(|t| t.as_byte()).collect();
        let unique: std::collections::HashSet<u8> = bytes.iter().cloned().collect();
        assert_eq!(bytes.len(), unique.len(), "TypeTag values must be unique");
    }

    #[test]
    fn test_typetag_serialization() {
        // Test JSON serialization roundtrip for all variants
        let tags = vec![
            TypeTag::KV,
            TypeTag::Event,
            TypeTag::State,
            TypeTag::Run,
            TypeTag::Vector,
            TypeTag::Json,
        ];

        for tag in tags {
            let json = serde_json::to_string(&tag).unwrap();
            let restored: TypeTag = serde_json::from_str(&json).unwrap();
            assert_eq!(
                tag, restored,
                "TypeTag {:?} should roundtrip through JSON",
                tag
            );
        }
    }

    #[test]
    fn test_typetag_json_value() {
        // M5: TypeTag::Json must be 0x11 per architecture spec
        assert_eq!(TypeTag::Json as u8, 0x11);
        assert_eq!(TypeTag::from_byte(0x11), Some(TypeTag::Json));
    }

    #[test]
    fn test_typetag_equality() {
        assert_eq!(TypeTag::KV, TypeTag::KV);
        assert_ne!(TypeTag::KV, TypeTag::Event);
        assert_ne!(TypeTag::Event, TypeTag::State);
    }

    #[test]
    fn test_typetag_clone_copy() {
        let tag1 = TypeTag::KV;
        let tag2 = tag1; // Should be Copy
        assert_eq!(tag1, tag2);

        let tag3 = tag1.clone();
        assert_eq!(tag1, tag3);
    }

    #[test]
    fn test_typetag_hash() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        set.insert(TypeTag::KV);
        set.insert(TypeTag::Event);
        set.insert(TypeTag::KV); // Duplicate

        assert_eq!(set.len(), 2, "Set should contain 2 unique TypeTags");
        assert!(set.contains(&TypeTag::KV));
        assert!(set.contains(&TypeTag::Event));
    }

    // ========================================
    // Key Tests
    // ========================================

    #[test]
    fn test_key_construction() {
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Test generic constructor
        let key = Key::new(ns.clone(), TypeTag::KV, b"mykey".to_vec());
        assert_eq!(key.namespace, ns);
        assert_eq!(key.type_tag, TypeTag::KV);
        assert_eq!(key.user_key, b"mykey");
    }

    #[test]
    fn test_key_helpers() {
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Test KV helper
        let kv_key = Key::new_kv(ns.clone(), "mykey");
        assert_eq!(kv_key.type_tag, TypeTag::KV);
        assert_eq!(kv_key.user_key, b"mykey");

        // Test event helper
        let event_key = Key::new_event(ns.clone(), 42);
        assert_eq!(event_key.type_tag, TypeTag::Event);
        assert_eq!(
            u64::from_be_bytes(event_key.user_key.as_slice().try_into().unwrap()),
            42
        );

        // Test state cell helper
        let state_key = Key::new_state(ns.clone(), "state1");
        assert_eq!(state_key.type_tag, TypeTag::State);
        assert_eq!(state_key.user_key, b"state1");

        // Test run index helper
        let run_key = Key::new_run(ns.clone(), run_id);
        assert_eq!(run_key.type_tag, TypeTag::Run);
        assert_eq!(run_key.user_key, run_id.as_bytes().to_vec());
    }

    #[test]
    fn test_new_event_meta() {
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        let key = Key::new_event_meta(ns);
        assert_eq!(key.type_tag, TypeTag::Event);
        assert_eq!(key.user_key, b"__meta__");
    }

    #[test]
    fn test_new_run_index() {
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Test by-status index
        let key = Key::new_run_index(ns.clone(), "status", "Active", "run-123");
        assert_eq!(key.type_tag, TypeTag::Run);
        assert!(key
            .user_key_string()
            .unwrap()
            .contains("__idx_status__Active__run-123"));

        // Test by-tag index
        let tag_key = Key::new_run_index(ns.clone(), "tag", "experiment", "run-456");
        assert!(tag_key
            .user_key_string()
            .unwrap()
            .contains("__idx_tag__experiment__run-456"));
    }

    #[test]
    fn test_user_key_string() {
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Valid UTF-8
        let key = Key::new_kv(ns.clone(), "hello-world");
        assert_eq!(key.user_key_string(), Some("hello-world".to_string()));

        // Invalid UTF-8 (binary data)
        let binary_key = Key::new(ns.clone(), TypeTag::KV, vec![0xFF, 0xFE, 0x00, 0x01]);
        assert_eq!(binary_key.user_key_string(), None);
    }

    #[test]
    fn test_event_keys_sort_by_sequence() {
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        let key1 = Key::new_event(ns.clone(), 1);
        let key2 = Key::new_event(ns.clone(), 10);
        let key3 = Key::new_event(ns.clone(), 100);

        // Big-endian encoding ensures lexicographic sort = numeric sort
        assert!(key1 < key2);
        assert!(key2 < key3);
    }

    #[test]
    fn test_keys_with_same_inputs_are_equal() {
        let run_id = RunId::new();
        let ns1 = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );
        let ns2 = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        let key1 = Key::new_kv(ns1, "same-key");
        let key2 = Key::new_kv(ns2, "same-key");
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_key_btree_ordering() {
        use std::collections::BTreeMap;

        let run1 = RunId::new();

        let ns1 = Namespace::new(
            "tenant1".to_string(),
            "app1".to_string(),
            "agent1".to_string(),
            run1,
        );
        let ns2 = Namespace::new(
            "tenant2".to_string(),
            "app1".to_string(),
            "agent1".to_string(),
            run1,
        );

        // Test ordering: namespace → type_tag → user_key
        let key1 = Key::new_kv(ns1.clone(), b"aaa");
        let key2 = Key::new_kv(ns1.clone(), b"zzz");
        let key3 = Key::new_event(ns1.clone(), 1);
        let key4 = Key::new_kv(ns2.clone(), b"aaa");

        // Same namespace, same type, different user_key
        assert!(key1 < key2, "user_key 'aaa' should be < 'zzz'");

        // Same namespace, different type (KV < Event)
        assert!(key1 < key3, "TypeTag::KV should be < TypeTag::Event");

        // Different namespace (tenant1 < tenant2)
        assert!(key1 < key4, "ns1 should be < ns2");

        // Test BTreeMap ordering
        let mut map = BTreeMap::new();
        map.insert(key4.clone(), "value4");
        map.insert(key2.clone(), "value2");
        map.insert(key1.clone(), "value1");
        map.insert(key3.clone(), "value3");

        let keys: Vec<_> = map.keys().cloned().collect();

        // Expected order: key1 (ns1/KV/aaa) < key2 (ns1/KV/zzz) < key3 (ns1/Event/1) < key4 (ns2/KV/aaa)
        assert_eq!(keys[0], key1);
        assert_eq!(keys[1], key2);
        assert_eq!(keys[2], key3);
        assert_eq!(keys[3], key4);
    }

    #[test]
    fn test_key_ordering_components() {
        let run_id = RunId::new();
        let ns1 = Namespace::new(
            "a".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );
        let ns2 = Namespace::new(
            "b".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        let key1 = Key::new(ns1.clone(), TypeTag::KV, b"key1".to_vec());
        let key2 = Key::new(ns1.clone(), TypeTag::Event, b"key1".to_vec());
        let key3 = Key::new(ns1.clone(), TypeTag::KV, b"key2".to_vec());
        let key4 = Key::new(ns2.clone(), TypeTag::KV, b"key1".to_vec());

        // Test namespace ordering (first component)
        assert!(
            key1 < key4,
            "Different namespace: ordering by namespace first"
        );

        // Test type_tag ordering (second component, same namespace)
        assert!(
            key1 < key2,
            "Same namespace, different type: ordering by type_tag"
        );

        // Test user_key ordering (third component, same namespace and type)
        assert!(key1 < key3, "Same namespace and type: ordering by user_key");
    }

    #[test]
    fn test_key_prefix_matching() {
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        let prefix = Key::new_kv(ns.clone(), b"user:");
        let key1 = Key::new_kv(ns.clone(), b"user:alice");
        let key2 = Key::new_kv(ns.clone(), b"user:bob");
        let key3 = Key::new_kv(ns.clone(), b"config:foo");
        let key4 = Key::new_event(ns.clone(), 1);

        // Should match keys with same namespace, type, and user_key prefix
        assert!(
            key1.starts_with(&prefix),
            "user:alice should match prefix user:"
        );
        assert!(
            key2.starts_with(&prefix),
            "user:bob should match prefix user:"
        );

        // Should not match different user_key prefix
        assert!(
            !key3.starts_with(&prefix),
            "config:foo should not match prefix user:"
        );

        // Should not match different type_tag
        assert!(
            !key4.starts_with(&prefix),
            "Event type should not match KV prefix"
        );
    }

    #[test]
    fn test_key_prefix_matching_empty() {
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Empty prefix should match all keys of same namespace and type
        let prefix = Key::new_kv(ns.clone(), b"");
        let key1 = Key::new_kv(ns.clone(), b"anything");
        let key2 = Key::new_kv(ns.clone(), b"");

        assert!(
            key1.starts_with(&prefix),
            "Any key should match empty prefix"
        );
        assert!(
            key2.starts_with(&prefix),
            "Empty key should match empty prefix"
        );
    }

    #[test]
    fn test_key_serialization() {
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );
        let key = Key::new_kv(ns, "testkey");

        // Test JSON roundtrip
        let json = serde_json::to_string(&key).unwrap();
        let key2: Key = serde_json::from_str(&json).unwrap();
        assert_eq!(key, key2, "Key should roundtrip through JSON");
    }

    #[test]
    fn test_key_equality() {
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        let key1 = Key::new_kv(ns.clone(), "mykey");
        let key2 = Key::new_kv(ns.clone(), "mykey");
        let key3 = Key::new_kv(ns.clone(), "other");

        assert_eq!(key1, key2, "Identical keys should be equal");
        assert_ne!(key1, key3, "Different user_keys should not be equal");
    }

    #[test]
    fn test_key_clone() {
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        let key1 = Key::new_kv(ns, "mykey");
        let key2 = key1.clone();

        assert_eq!(key1, key2, "Cloned key should equal original");
    }

    #[test]
    fn test_key_hash() {
        use std::collections::HashSet;

        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        let key1 = Key::new_kv(ns.clone(), "key1");
        let key2 = Key::new_kv(ns.clone(), "key2");
        let key3 = Key::new_kv(ns.clone(), "key1"); // Duplicate

        let mut set = HashSet::new();
        set.insert(key1);
        set.insert(key2);
        set.insert(key3);

        assert_eq!(set.len(), 2, "Set should contain 2 unique keys");
    }

    #[test]
    fn test_key_binary_user_key() {
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        // Test with binary data (not UTF-8)
        let binary_data = vec![0u8, 1, 2, 255, 254, 253];
        let key = Key::new(ns.clone(), TypeTag::KV, binary_data.clone());

        assert_eq!(
            key.user_key, binary_data,
            "Binary user_key should be preserved"
        );
    }

    // ========================================
    // JsonDocId Tests (M5)
    // ========================================

    #[test]
    fn test_json_doc_id_unique() {
        let id1 = JsonDocId::new();
        let id2 = JsonDocId::new();
        assert_ne!(id1, id2, "JsonDocIds should be unique");
    }

    #[test]
    fn test_json_doc_id_bytes_roundtrip() {
        let id = JsonDocId::new();
        let bytes = id.as_bytes();
        let recovered = JsonDocId::try_from_bytes(bytes).unwrap();
        assert_eq!(id, recovered, "JsonDocId should roundtrip through bytes");
    }

    #[test]
    fn test_json_doc_id_is_copy() {
        let id = JsonDocId::new();
        let id_copy = id; // Copy
        assert_eq!(id, id_copy, "JsonDocId should be Copy");
    }

    #[test]
    fn test_json_doc_id_display() {
        let id = JsonDocId::new();
        let s = format!("{}", id);
        assert!(!s.is_empty(), "Display should produce non-empty string");
        assert_eq!(
            s.len(),
            36,
            "UUID v4 should format as 36 characters with hyphens"
        );
    }

    #[test]
    fn test_json_doc_id_default() {
        let id1 = JsonDocId::default();
        let id2 = JsonDocId::default();
        assert_ne!(id1, id2, "Default JsonDocIds should be unique");
    }

    #[test]
    fn test_json_doc_id_hash() {
        use std::collections::HashSet;

        let id1 = JsonDocId::new();
        let id2 = id1; // Copy

        let mut set = HashSet::new();
        set.insert(id1);

        assert!(
            set.contains(&id2),
            "Hash should be consistent for copied JsonDocId"
        );

        let id3 = JsonDocId::new();
        set.insert(id3);

        assert_eq!(
            set.len(),
            2,
            "Different JsonDocIds should have different hashes"
        );
    }

    #[test]
    fn test_json_doc_id_bytes_length() {
        let id = JsonDocId::new();
        let bytes = id.as_bytes();
        assert_eq!(bytes.len(), 16, "JsonDocId bytes should be 16 bytes (UUID)");
    }

    #[test]
    fn test_json_doc_id_try_from_bytes_invalid() {
        // Too short
        let short = vec![0u8; 10];
        assert!(
            JsonDocId::try_from_bytes(&short).is_none(),
            "Should reject short bytes"
        );

        // Too long
        let long = vec![0u8; 20];
        assert!(
            JsonDocId::try_from_bytes(&long).is_none(),
            "Should reject long bytes"
        );

        // Empty
        assert!(
            JsonDocId::try_from_bytes(&[]).is_none(),
            "Should reject empty bytes"
        );
    }

    #[test]
    fn test_json_doc_id_from_uuid() {
        use uuid::Uuid;
        let uuid = Uuid::new_v4();
        let id = JsonDocId::from_uuid(uuid);
        assert_eq!(id.as_uuid(), &uuid, "Should preserve underlying UUID");
    }

    #[test]
    fn test_json_doc_id_serialization() {
        let id = JsonDocId::new();
        let json = serde_json::to_string(&id).unwrap();
        let id2: JsonDocId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, id2, "JsonDocId should roundtrip through JSON");
    }

    // ========================================
    // Key::new_json Tests (M5)
    // ========================================

    #[test]
    fn test_key_new_json() {
        let run_id = RunId::new();
        let doc_id = JsonDocId::new();
        let namespace = Namespace::for_run(run_id);
        let key = Key::new_json(namespace.clone(), &doc_id);

        assert_eq!(key.type_tag, TypeTag::Json);
        assert_eq!(key.namespace, namespace);
        assert_eq!(key.user_key, doc_id.as_bytes().to_vec());
    }

    #[test]
    fn test_key_new_json_prefix() {
        let run_id = RunId::new();
        let namespace = Namespace::for_run(run_id);
        let prefix = Key::new_json_prefix(namespace.clone());

        assert_eq!(prefix.type_tag, TypeTag::Json);
        assert_eq!(prefix.namespace, namespace);
        assert!(prefix.user_key.is_empty());

        // Test prefix matching
        let doc_id = JsonDocId::new();
        let key = Key::new_json(namespace.clone(), &doc_id);
        assert!(
            key.starts_with(&prefix),
            "JSON key should match JSON prefix"
        );
    }

    #[test]
    fn test_key_json_different_docs_different_keys() {
        let run_id = RunId::new();
        let namespace = Namespace::for_run(run_id);
        let doc_id1 = JsonDocId::new();
        let doc_id2 = JsonDocId::new();

        let key1 = Key::new_json(namespace.clone(), &doc_id1);
        let key2 = Key::new_json(namespace.clone(), &doc_id2);

        assert_ne!(key1, key2, "Different docs should have different keys");
    }

    #[test]
    fn test_key_json_same_doc_same_key() {
        let run_id = RunId::new();
        let namespace = Namespace::for_run(run_id);
        let doc_id = JsonDocId::new();

        let key1 = Key::new_json(namespace.clone(), &doc_id);
        let key2 = Key::new_json(namespace.clone(), &doc_id);

        assert_eq!(key1, key2, "Same doc should have same key");
    }

    #[test]
    fn test_key_json_ordering_with_other_types() {
        let run_id = RunId::new();
        let namespace = Namespace::for_run(run_id);
        let doc_id = JsonDocId::new();

        let kv_key = Key::new_kv(namespace.clone(), "test");
        let event_key = Key::new_event(namespace.clone(), 1);
        let json_key = Key::new_json(namespace.clone(), &doc_id);

        // JSON keys should sort after all other types (0x11 > 0x10 > 0x05 > ...)
        assert!(kv_key < json_key, "KV should be < JSON");
        assert!(event_key < json_key, "Event should be < JSON");
    }

    #[test]
    fn test_key_json_does_not_match_other_type_prefix() {
        let run_id = RunId::new();
        let namespace = Namespace::for_run(run_id);
        let doc_id = JsonDocId::new();

        let json_key = Key::new_json(namespace.clone(), &doc_id);
        let kv_prefix = Key::new_kv(namespace.clone(), "");

        assert!(
            !json_key.starts_with(&kv_prefix),
            "JSON key should not match KV prefix"
        );
    }
}
