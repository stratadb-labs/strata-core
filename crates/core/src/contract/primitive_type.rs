//! Primitive type enumeration
//!
//! This type supports Invariant 6: Everything is Introspectable.
//! Every entity can report what kind of primitive it is.
//!
//! ## The Six Primitives
//!
//! The database has exactly six primitives:
//!
//! | Primitive | Purpose | Versioning |
//! |-----------|---------|------------|
//! | Kv | Key-value store | TxnId |
//! | Event | Append-only event log | Sequence |
//! | State | Named state cells with CAS | Counter |
//! | Run | Run lifecycle management | TxnId |
//! | Json | JSON document store | TxnId |
//! | Vector | Vector similarity search | TxnId |

use serde::{Deserialize, Serialize};

/// The six primitive types in the database
///
/// This enum identifies which primitive a value or operation belongs to.
/// Used for type discrimination, routing, and introspection.
///
/// ## Invariant
///
/// This enum MUST have exactly 6 variants - one for each primitive.
/// Adding a new primitive requires adding a variant here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PrimitiveType {
    /// Key-Value store
    ///
    /// Simple key-value storage with CRUD operations.
    /// Versioning: TxnId
    Kv,

    /// Event log
    ///
    /// Append-only event stream with sequence numbers.
    /// Versioning: Sequence
    Event,

    /// State cell
    ///
    /// Named state cells with compare-and-swap.
    /// Versioning: Counter
    State,

    /// Run index
    ///
    /// Run lifecycle management (create, status, metadata).
    /// Versioning: TxnId
    Run,

    /// JSON document store
    ///
    /// JSON documents with path-based operations.
    /// Versioning: TxnId
    Json,

    /// Vector store
    ///
    /// Vector similarity search with HNSW index.
    /// Versioning: TxnId
    Vector,
}

impl PrimitiveType {
    /// All primitive types (for iteration)
    pub const ALL: [PrimitiveType; 6] = [
        PrimitiveType::Kv,
        PrimitiveType::Event,
        PrimitiveType::State,
        PrimitiveType::Run,
        PrimitiveType::Json,
        PrimitiveType::Vector,
    ];

    /// Get all primitive types as a slice
    pub fn all() -> &'static [PrimitiveType] {
        &Self::ALL
    }

    /// Human-readable display name
    pub const fn name(&self) -> &'static str {
        match self {
            PrimitiveType::Kv => "KVStore",
            PrimitiveType::Event => "EventLog",
            PrimitiveType::State => "StateCell",
            PrimitiveType::Run => "RunIndex",
            PrimitiveType::Json => "JsonStore",
            PrimitiveType::Vector => "VectorStore",
        }
    }

    /// Short identifier (for serialization, URIs, etc.)
    pub const fn id(&self) -> &'static str {
        match self {
            PrimitiveType::Kv => "kv",
            PrimitiveType::Event => "event",
            PrimitiveType::State => "state",
            PrimitiveType::Run => "run",
            PrimitiveType::Json => "json",
            PrimitiveType::Vector => "vector",
        }
    }

    /// Parse from short identifier
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "kv" => Some(PrimitiveType::Kv),
            "event" => Some(PrimitiveType::Event),
            "state" => Some(PrimitiveType::State),
            "run" => Some(PrimitiveType::Run),
            "json" => Some(PrimitiveType::Json),
            "vector" => Some(PrimitiveType::Vector),
            _ => None,
        }
    }

    /// Check if this primitive supports CRUD lifecycle
    ///
    /// Kv, State, Run, Json, Vector support full CRUD.
    /// Event is append-only (CR only).
    pub const fn supports_crud(&self) -> bool {
        match self {
            PrimitiveType::Kv => true,
            PrimitiveType::Event => false, // Append-only
            PrimitiveType::State => true,
            PrimitiveType::Run => true,
            PrimitiveType::Json => true,
            PrimitiveType::Vector => true,
        }
    }

    /// Check if this primitive is append-only
    pub const fn is_append_only(&self) -> bool {
        !self.supports_crud()
    }
}

impl std::fmt::Display for PrimitiveType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_primitive_type_all() {
        let all = PrimitiveType::all();
        assert_eq!(all.len(), 6);

        // Verify all variants are present
        assert!(all.contains(&PrimitiveType::Kv));
        assert!(all.contains(&PrimitiveType::Event));
        assert!(all.contains(&PrimitiveType::State));
        assert!(all.contains(&PrimitiveType::Run));
        assert!(all.contains(&PrimitiveType::Json));
        assert!(all.contains(&PrimitiveType::Vector));
    }

    #[test]
    fn test_primitive_type_const_all() {
        assert_eq!(PrimitiveType::ALL.len(), 6);
    }

    #[test]
    fn test_primitive_type_names() {
        assert_eq!(PrimitiveType::Kv.name(), "KVStore");
        assert_eq!(PrimitiveType::Event.name(), "EventLog");
        assert_eq!(PrimitiveType::State.name(), "StateCell");
        assert_eq!(PrimitiveType::Run.name(), "RunIndex");
        assert_eq!(PrimitiveType::Json.name(), "JsonStore");
        assert_eq!(PrimitiveType::Vector.name(), "VectorStore");
    }

    #[test]
    fn test_primitive_type_ids() {
        assert_eq!(PrimitiveType::Kv.id(), "kv");
        assert_eq!(PrimitiveType::Event.id(), "event");
        assert_eq!(PrimitiveType::State.id(), "state");
        assert_eq!(PrimitiveType::Run.id(), "run");
        assert_eq!(PrimitiveType::Json.id(), "json");
        assert_eq!(PrimitiveType::Vector.id(), "vector");
    }

    #[test]
    fn test_primitive_type_from_id() {
        assert_eq!(PrimitiveType::from_id("kv"), Some(PrimitiveType::Kv));
        assert_eq!(PrimitiveType::from_id("event"), Some(PrimitiveType::Event));
        assert_eq!(PrimitiveType::from_id("state"), Some(PrimitiveType::State));
        assert_eq!(PrimitiveType::from_id("run"), Some(PrimitiveType::Run));
        assert_eq!(PrimitiveType::from_id("json"), Some(PrimitiveType::Json));
        assert_eq!(
            PrimitiveType::from_id("vector"),
            Some(PrimitiveType::Vector)
        );
        assert_eq!(PrimitiveType::from_id("invalid"), None);
    }

    #[test]
    fn test_primitive_type_roundtrip() {
        for pt in PrimitiveType::all() {
            let id = pt.id();
            let restored = PrimitiveType::from_id(id).unwrap();
            assert_eq!(*pt, restored);
        }
    }

    #[test]
    fn test_primitive_type_display() {
        assert_eq!(format!("{}", PrimitiveType::Kv), "KVStore");
        assert_eq!(format!("{}", PrimitiveType::Json), "JsonStore");
        assert_eq!(format!("{}", PrimitiveType::Vector), "VectorStore");
    }

    #[test]
    fn test_primitive_type_supports_crud() {
        // Full CRUD
        assert!(PrimitiveType::Kv.supports_crud());
        assert!(PrimitiveType::State.supports_crud());
        assert!(PrimitiveType::Run.supports_crud());
        assert!(PrimitiveType::Json.supports_crud());
        assert!(PrimitiveType::Vector.supports_crud());

        // Append-only (no delete/update)
        assert!(!PrimitiveType::Event.supports_crud());
    }

    #[test]
    fn test_primitive_type_is_append_only() {
        assert!(PrimitiveType::Event.is_append_only());

        assert!(!PrimitiveType::Kv.is_append_only());
        assert!(!PrimitiveType::State.is_append_only());
        assert!(!PrimitiveType::Run.is_append_only());
        assert!(!PrimitiveType::Json.is_append_only());
        assert!(!PrimitiveType::Vector.is_append_only());
    }

    #[test]
    fn test_primitive_type_copy() {
        let pt = PrimitiveType::Kv;
        let pt2 = pt; // Copy
        assert_eq!(pt, pt2);
    }

    #[test]
    fn test_primitive_type_hash() {
        use std::collections::HashSet;

        let mut set = HashSet::new();
        for pt in PrimitiveType::all() {
            set.insert(*pt);
        }
        assert_eq!(set.len(), 6, "All PrimitiveTypes should be unique");
    }

    #[test]
    fn test_primitive_type_serialization() {
        for pt in PrimitiveType::all() {
            let json = serde_json::to_string(pt).unwrap();
            let restored: PrimitiveType = serde_json::from_str(&json).unwrap();
            assert_eq!(*pt, restored);
        }
    }

    #[test]
    fn test_primitive_type_equality() {
        assert_eq!(PrimitiveType::Kv, PrimitiveType::Kv);
        assert_ne!(PrimitiveType::Kv, PrimitiveType::Event);
        assert_ne!(PrimitiveType::Event, PrimitiveType::State);
    }
}
