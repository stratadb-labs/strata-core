//! Universal entity reference type
//!
//! This type expresses Invariant 1: Everything is Addressable.
//! Every entity in the database can be referenced by an EntityRef.
//!
//! ## The Problem EntityRef Solves
//!
//! Different primitives have different key structures:
//! - KV: namespace + user_key
//! - EventLog: namespace + sequence
//! - StateCell: namespace + cell_name
//! - etc.
//!
//! EntityRef provides a **uniform way to reference any entity**.
//!
//! ## Structure
//!
//! Every EntityRef has:
//! - `run_id`: The run this entity belongs to (Invariant 5: Run-scoped)
//! - Primitive-specific fields
//!
//! ## Usage
//!
//! ```
//! use strata_core::{EntityRef, RunId, PrimitiveType};
//!
//! let run_id = RunId::new();
//!
//! // Reference a KV entry
//! let kv_ref = EntityRef::kv(run_id, "my-key");
//!
//! // Reference an event
//! let event_ref = EntityRef::event(run_id, 42);
//!
//! // Get the primitive type
//! assert_eq!(kv_ref.primitive_type(), PrimitiveType::Kv);
//! ```

use super::PrimitiveType;
use crate::types::{JsonDocId, RunId};
use serde::{Deserialize, Serialize};

/// Universal reference to any entity in the database
///
/// EntityRef is the canonical way to identify any piece of data.
/// It combines run_id (scope) with primitive-specific addressing.
///
/// ## Invariants
///
/// - Every EntityRef has exactly one variant (primitive type)
/// - Every EntityRef has a run_id
/// - EntityRef variants match PrimitiveType variants 1:1
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntityRef {
    /// Reference to a KV entry
    Kv {
        /// Run scope
        run_id: RunId,
        /// Key (user-defined)
        key: String,
    },

    /// Reference to an event in the log
    Event {
        /// Run scope
        run_id: RunId,
        /// Sequence number in the event log
        sequence: u64,
    },

    /// Reference to a state cell
    State {
        /// Run scope
        run_id: RunId,
        /// Cell name (user-defined)
        name: String,
    },

    /// Reference to a trace entry
    Trace {
        /// Run scope
        run_id: RunId,
        /// Trace ID (UUID string)
        trace_id: String,
    },

    /// Reference to a run's metadata
    Run {
        /// The run being referenced (also the scope)
        run_id: RunId,
    },

    /// Reference to a JSON document
    Json {
        /// Run scope
        run_id: RunId,
        /// Document ID
        doc_id: JsonDocId,
    },

    /// Reference to a vector entry
    Vector {
        /// Run scope
        run_id: RunId,
        /// Collection name
        collection: String,
        /// Key within collection
        key: String,
    },
}

impl EntityRef {
    // =========================================================================
    // Constructors
    // =========================================================================

    /// Create a KV entity reference
    pub fn kv(run_id: RunId, key: impl Into<String>) -> Self {
        EntityRef::Kv {
            run_id,
            key: key.into(),
        }
    }

    /// Create an event entity reference
    pub fn event(run_id: RunId, sequence: u64) -> Self {
        EntityRef::Event { run_id, sequence }
    }

    /// Create a state cell entity reference
    pub fn state(run_id: RunId, name: impl Into<String>) -> Self {
        EntityRef::State {
            run_id,
            name: name.into(),
        }
    }

    /// Create a trace entity reference
    pub fn trace(run_id: RunId, trace_id: impl Into<String>) -> Self {
        EntityRef::Trace {
            run_id,
            trace_id: trace_id.into(),
        }
    }

    /// Create a run entity reference
    pub fn run(run_id: RunId) -> Self {
        EntityRef::Run { run_id }
    }

    /// Create a JSON document entity reference
    pub fn json(run_id: RunId, doc_id: JsonDocId) -> Self {
        EntityRef::Json { run_id, doc_id }
    }

    /// Create a vector entity reference
    pub fn vector(run_id: RunId, collection: impl Into<String>, key: impl Into<String>) -> Self {
        EntityRef::Vector {
            run_id,
            collection: collection.into(),
            key: key.into(),
        }
    }

    // =========================================================================
    // Accessors
    // =========================================================================

    /// Get the run_id this entity belongs to
    ///
    /// All entities are run-scoped (Invariant 5).
    pub fn run_id(&self) -> RunId {
        match self {
            EntityRef::Kv { run_id, .. } => *run_id,
            EntityRef::Event { run_id, .. } => *run_id,
            EntityRef::State { run_id, .. } => *run_id,
            EntityRef::Trace { run_id, .. } => *run_id,
            EntityRef::Run { run_id } => *run_id,
            EntityRef::Json { run_id, .. } => *run_id,
            EntityRef::Vector { run_id, .. } => *run_id,
        }
    }

    /// Get the primitive type of this entity
    pub fn primitive_type(&self) -> PrimitiveType {
        match self {
            EntityRef::Kv { .. } => PrimitiveType::Kv,
            EntityRef::Event { .. } => PrimitiveType::Event,
            EntityRef::State { .. } => PrimitiveType::State,
            EntityRef::Trace { .. } => PrimitiveType::Trace,
            EntityRef::Run { .. } => PrimitiveType::Run,
            EntityRef::Json { .. } => PrimitiveType::Json,
            EntityRef::Vector { .. } => PrimitiveType::Vector,
        }
    }

    /// Deprecated: Use primitive_type() instead
    #[deprecated(since = "0.9.0", note = "Use primitive_type() instead")]
    pub fn primitive_kind(&self) -> PrimitiveType {
        self.primitive_type()
    }

    // =========================================================================
    // Type Checks
    // =========================================================================

    /// Check if this is a KV reference
    pub fn is_kv(&self) -> bool {
        matches!(self, EntityRef::Kv { .. })
    }

    /// Check if this is an event reference
    pub fn is_event(&self) -> bool {
        matches!(self, EntityRef::Event { .. })
    }

    /// Check if this is a state reference
    pub fn is_state(&self) -> bool {
        matches!(self, EntityRef::State { .. })
    }

    /// Check if this is a trace reference
    pub fn is_trace(&self) -> bool {
        matches!(self, EntityRef::Trace { .. })
    }

    /// Check if this is a run reference
    pub fn is_run(&self) -> bool {
        matches!(self, EntityRef::Run { .. })
    }

    /// Check if this is a JSON reference
    pub fn is_json(&self) -> bool {
        matches!(self, EntityRef::Json { .. })
    }

    /// Check if this is a vector reference
    pub fn is_vector(&self) -> bool {
        matches!(self, EntityRef::Vector { .. })
    }

    // =========================================================================
    // Extraction
    // =========================================================================

    /// Get the KV key if this is a KV reference
    pub fn kv_key(&self) -> Option<&str> {
        match self {
            EntityRef::Kv { key, .. } => Some(key),
            _ => None,
        }
    }

    /// Get the event sequence if this is an event reference
    pub fn event_sequence(&self) -> Option<u64> {
        match self {
            EntityRef::Event { sequence, .. } => Some(*sequence),
            _ => None,
        }
    }

    /// Get the state cell name if this is a state reference
    pub fn state_name(&self) -> Option<&str> {
        match self {
            EntityRef::State { name, .. } => Some(name),
            _ => None,
        }
    }

    /// Get the trace ID if this is a trace reference
    pub fn trace_id(&self) -> Option<&str> {
        match self {
            EntityRef::Trace { trace_id, .. } => Some(trace_id),
            _ => None,
        }
    }

    /// Get the JSON doc ID if this is a JSON reference
    pub fn json_doc_id(&self) -> Option<JsonDocId> {
        match self {
            EntityRef::Json { doc_id, .. } => Some(*doc_id),
            _ => None,
        }
    }

    /// Get the vector collection and key if this is a vector reference
    pub fn vector_location(&self) -> Option<(&str, &str)> {
        match self {
            EntityRef::Vector {
                collection, key, ..
            } => Some((collection, key)),
            _ => None,
        }
    }
}

impl std::fmt::Display for EntityRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntityRef::Kv { run_id, key } => {
                write!(f, "kv://{}/{}", run_id, key)
            }
            EntityRef::Event { run_id, sequence } => {
                write!(f, "event://{}/{}", run_id, sequence)
            }
            EntityRef::State { run_id, name } => {
                write!(f, "state://{}/{}", run_id, name)
            }
            EntityRef::Trace { run_id, trace_id } => {
                write!(f, "trace://{}/{}", run_id, trace_id)
            }
            EntityRef::Run { run_id } => {
                write!(f, "run://{}", run_id)
            }
            EntityRef::Json { run_id, doc_id } => {
                write!(f, "json://{}/{}", run_id, doc_id)
            }
            EntityRef::Vector {
                run_id,
                collection,
                key,
            } => {
                write!(f, "vector://{}/{}/{}", run_id, collection, key)
            }
        }
    }
}

// ============================================================================
// DocRef Alias (backwards compatibility)
// ============================================================================

/// Alias for EntityRef
///
/// DocRef was the original name used in M6 search types.
/// New code should use EntityRef directly.
pub type DocRef = EntityRef;

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_ref_kv() {
        let run_id = RunId::new();
        let ref_ = EntityRef::kv(run_id, "my-key");

        assert!(ref_.is_kv());
        assert!(!ref_.is_event());
        assert_eq!(ref_.run_id(), run_id);
        assert_eq!(ref_.primitive_type(), PrimitiveType::Kv);
        assert_eq!(ref_.kv_key(), Some("my-key"));
    }

    #[test]
    fn test_entity_ref_event() {
        let run_id = RunId::new();
        let ref_ = EntityRef::event(run_id, 42);

        assert!(ref_.is_event());
        assert_eq!(ref_.run_id(), run_id);
        assert_eq!(ref_.primitive_type(), PrimitiveType::Event);
        assert_eq!(ref_.event_sequence(), Some(42));
    }

    #[test]
    fn test_entity_ref_state() {
        let run_id = RunId::new();
        let ref_ = EntityRef::state(run_id, "cell-name");

        assert!(ref_.is_state());
        assert_eq!(ref_.run_id(), run_id);
        assert_eq!(ref_.primitive_type(), PrimitiveType::State);
        assert_eq!(ref_.state_name(), Some("cell-name"));
    }

    #[test]
    fn test_entity_ref_trace() {
        let run_id = RunId::new();
        let ref_ = EntityRef::trace(run_id, "trace-uuid-123");

        assert!(ref_.is_trace());
        assert_eq!(ref_.run_id(), run_id);
        assert_eq!(ref_.primitive_type(), PrimitiveType::Trace);
        assert_eq!(ref_.trace_id(), Some("trace-uuid-123"));
    }

    #[test]
    fn test_entity_ref_run() {
        let run_id = RunId::new();
        let ref_ = EntityRef::run(run_id);

        assert!(ref_.is_run());
        assert_eq!(ref_.run_id(), run_id);
        assert_eq!(ref_.primitive_type(), PrimitiveType::Run);
    }

    #[test]
    fn test_entity_ref_json() {
        let run_id = RunId::new();
        let doc_id = JsonDocId::new();
        let ref_ = EntityRef::json(run_id, doc_id);

        assert!(ref_.is_json());
        assert_eq!(ref_.run_id(), run_id);
        assert_eq!(ref_.primitive_type(), PrimitiveType::Json);
        assert_eq!(ref_.json_doc_id(), Some(doc_id));
    }

    #[test]
    fn test_entity_ref_vector() {
        let run_id = RunId::new();
        let ref_ = EntityRef::vector(run_id, "embeddings", "doc-1");

        assert!(ref_.is_vector());
        assert_eq!(ref_.run_id(), run_id);
        assert_eq!(ref_.primitive_type(), PrimitiveType::Vector);
        assert_eq!(ref_.vector_location(), Some(("embeddings", "doc-1")));
    }

    #[test]
    fn test_entity_ref_display() {
        let run_id = RunId::new();

        let kv = EntityRef::kv(run_id, "key");
        assert!(format!("{}", kv).starts_with("kv://"));

        let event = EntityRef::event(run_id, 42);
        assert!(format!("{}", event).starts_with("event://"));

        let state = EntityRef::state(run_id, "cell");
        assert!(format!("{}", state).starts_with("state://"));

        let trace = EntityRef::trace(run_id, "trace-id");
        assert!(format!("{}", trace).starts_with("trace://"));

        let run_ref = EntityRef::run(run_id);
        assert!(format!("{}", run_ref).starts_with("run://"));

        let doc_id = JsonDocId::new();
        let json = EntityRef::json(run_id, doc_id);
        assert!(format!("{}", json).starts_with("json://"));

        let vector = EntityRef::vector(run_id, "col", "key");
        assert!(format!("{}", vector).starts_with("vector://"));
    }

    #[test]
    fn test_entity_ref_equality() {
        let run_id = RunId::new();

        let ref1 = EntityRef::kv(run_id, "key");
        let ref2 = EntityRef::kv(run_id, "key");
        let ref3 = EntityRef::kv(run_id, "other");

        assert_eq!(ref1, ref2);
        assert_ne!(ref1, ref3);
    }

    #[test]
    fn test_entity_ref_hash() {
        use std::collections::HashSet;

        let run_id = RunId::new();

        let mut set = HashSet::new();
        set.insert(EntityRef::kv(run_id, "key1"));
        set.insert(EntityRef::kv(run_id, "key2"));
        set.insert(EntityRef::kv(run_id, "key1")); // Duplicate

        assert_eq!(set.len(), 2);
    }

    #[test]
    fn test_entity_ref_serialization() {
        let run_id = RunId::new();
        let refs = vec![
            EntityRef::kv(run_id, "key"),
            EntityRef::event(run_id, 42),
            EntityRef::state(run_id, "cell"),
            EntityRef::trace(run_id, "trace-id"),
            EntityRef::run(run_id),
            EntityRef::json(run_id, JsonDocId::new()),
            EntityRef::vector(run_id, "col", "key"),
        ];

        for ref_ in refs {
            let json = serde_json::to_string(&ref_).unwrap();
            let restored: EntityRef = serde_json::from_str(&json).unwrap();
            assert_eq!(ref_, restored);
        }
    }

    #[test]
    fn test_doc_ref_alias() {
        // DocRef should be an alias for EntityRef
        let run_id = RunId::new();
        let doc_ref: DocRef = EntityRef::kv(run_id, "key");
        let entity_ref: EntityRef = EntityRef::kv(run_id, "key");
        assert_eq!(doc_ref, entity_ref);
    }

    #[test]
    fn test_wrong_extraction_returns_none() {
        let run_id = RunId::new();
        let kv_ref = EntityRef::kv(run_id, "key");

        // Wrong extractors should return None
        assert!(kv_ref.event_sequence().is_none());
        assert!(kv_ref.state_name().is_none());
        assert!(kv_ref.trace_id().is_none());
        assert!(kv_ref.json_doc_id().is_none());
        assert!(kv_ref.vector_location().is_none());
    }

    #[test]
    fn test_all_primitive_types_covered() {
        let run_id = RunId::new();
        let doc_id = JsonDocId::new();

        // Create one of each type
        let refs = vec![
            EntityRef::kv(run_id, "k"),
            EntityRef::event(run_id, 0),
            EntityRef::state(run_id, "s"),
            EntityRef::trace(run_id, "t"),
            EntityRef::run(run_id),
            EntityRef::json(run_id, doc_id),
            EntityRef::vector(run_id, "c", "k"),
        ];

        // Verify they map to all 7 primitive types
        let types: std::collections::HashSet<_> = refs.iter().map(|r| r.primitive_type()).collect();
        assert_eq!(types.len(), 7);
    }
}
