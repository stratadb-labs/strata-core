//! Extension trait for primitives to integrate with storage
//!
//! This trait must be implemented by any new primitive to participate in:
//! - WAL entry processing during recovery
//! - Snapshot serialization/deserialization
//! - Dynamic primitive registration
//!
//! ## Core Guarantee
//!
//! Adding a new primitive requires ONLY:
//! 1. Implementing this trait
//! 2. Registering in PrimitiveRegistry
//! 3. Using allocated WAL entry types
//!
//! NO changes to WAL format, Snapshot format, or Recovery engine required.
//!
//! ## Example: Vector Primitive
//!
//! ```rust,ignore
//! impl PrimitiveStorageExt for VectorStore {
//!     fn primitive_type_id(&self) -> u8 { 7 }
//!
//!     fn wal_entry_types(&self) -> &'static [u8] {
//!         &[0x70, 0x71, 0x72]  // VectorInsert, VectorDelete, VectorUpdate
//!     }
//!
//!     fn snapshot_serialize(&self) -> Result<Vec<u8>, PrimitiveExtError> {
//!         bincode::serialize(&self.vectors)
//!             .map_err(|e| PrimitiveExtError::Serialization(e.to_string()))
//!     }
//!
//!     fn snapshot_deserialize(&mut self, data: &[u8]) -> Result<(), PrimitiveExtError> {
//!         self.vectors = bincode::deserialize(data)
//!             .map_err(|e| PrimitiveExtError::Deserialization(e.to_string()))?;
//!         Ok(())
//!     }
//!
//!     fn apply_wal_entry(&mut self, entry_type: u8, payload: &[u8]) -> Result<(), PrimitiveExtError> {
//!         match entry_type {
//!             0x70 => { /* VectorInsert */ }
//!             0x71 => { /* VectorDelete */ }
//!             0x72 => { /* VectorUpdate */ }
//!             _ => return Err(PrimitiveExtError::UnknownEntryType(entry_type)),
//!         }
//!         Ok(())
//!     }
//!
//!     fn primitive_name(&self) -> &'static str { "vector" }
//!
//!     fn rebuild_indexes(&mut self) -> Result<(), PrimitiveExtError> {
//!         // Rebuild HNSW index from vectors
//!         self.rebuild_hnsw_index()?;
//!         Ok(())
//!     }
//! }
//! ```

use thiserror::Error;

/// Errors from primitive storage operations
#[derive(Debug, Error)]
pub enum PrimitiveExtError {
    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Deserialization error
    #[error("Deserialization error: {0}")]
    Deserialization(String),

    /// Unknown WAL entry type
    #[error("Unknown WAL entry type: 0x{0:02X}")]
    UnknownEntryType(u8),

    /// Invalid operation
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),

    /// Index rebuild error
    #[error("Index rebuild error: {0}")]
    IndexRebuild(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Extension trait for primitives to integrate with storage
///
/// This trait defines the contract that new primitives must implement
/// to participate in the durability system (WAL, Snapshots, Recovery).
///
/// ## WAL Entry Type Ranges
///
/// Each primitive is allocated a 16-byte range:
///
/// | Primitive | Range | Status |
/// |-----------|-------|--------|
/// | Core | 0x00-0x0F | FROZEN |
/// | KV | 0x10-0x1F | FROZEN |
/// | JSON | 0x20-0x2F | FROZEN |
/// | Event | 0x30-0x3F | FROZEN |
/// | State | 0x40-0x4F | FROZEN |
/// | Run | 0x60-0x6F | FROZEN |
/// | Vector | 0x70-0x7F | RESERVED |
/// | Future | 0x80-0xFF | AVAILABLE |
///
/// ## Primitive Type IDs (for Snapshots)
///
/// | Primitive | Type ID |
/// |-----------|---------|
/// | KV | 1 |
/// | JSON | 2 |
/// | Event | 3 |
/// | State | 4 |
/// | Run | 6 |
/// | Vector | 7 |
pub trait PrimitiveStorageExt: Send + Sync {
    /// Unique identifier for this primitive type
    ///
    /// Used in snapshot sections. Must be unique and stable.
    /// Core primitives use 1-6. Vector will use 7.
    fn primitive_type_id(&self) -> u8;

    /// WAL entry types this primitive uses (from its allocated range)
    ///
    /// Used during recovery to route entries to the right primitive.
    /// Must be from the primitive's allocated range.
    fn wal_entry_types(&self) -> &'static [u8];

    /// Serialize primitive state for snapshot
    ///
    /// Should serialize all data needed to reconstruct the primitive.
    /// Do NOT include derived data (indexes) - those are rebuilt.
    fn snapshot_serialize(&self) -> Result<Vec<u8>, PrimitiveExtError>;

    /// Deserialize primitive state from snapshot
    ///
    /// Reconstruct primitive state from serialized bytes.
    /// Indexes will be rebuilt separately via rebuild_indexes().
    fn snapshot_deserialize(&mut self, data: &[u8]) -> Result<(), PrimitiveExtError>;

    /// Apply a WAL entry during recovery
    ///
    /// Called for each WAL entry with a type in wal_entry_types().
    /// Should apply the entry's effect to in-memory state.
    ///
    /// # Arguments
    ///
    /// * `entry_type` - The WAL entry type byte
    /// * `payload` - The entry payload (primitive-specific format)
    fn apply_wal_entry(&mut self, entry_type: u8, payload: &[u8]) -> Result<(), PrimitiveExtError>;

    /// Primitive name (for logging/debugging)
    fn primitive_name(&self) -> &'static str;

    /// Rebuild indexes after recovery
    ///
    /// Called after all WAL entries are applied.
    /// Override if primitive has indexes that need rebuilding.
    fn rebuild_indexes(&mut self) -> Result<(), PrimitiveExtError> {
        Ok(()) // Default: no indexes
    }

    /// Check if this primitive handles a given entry type
    fn handles_entry_type(&self, entry_type: u8) -> bool {
        self.wal_entry_types().contains(&entry_type)
    }
}

/// Primitive type IDs for snapshot sections
///
/// Each primitive has a unique type ID for snapshot serialization.
pub mod primitive_type_ids {
    /// KV Store
    pub const KV: u8 = 1;
    /// JSON Store
    pub const JSON: u8 = 2;
    /// Event Log
    pub const EVENT: u8 = 3;
    /// State Cell
    pub const STATE: u8 = 4;
    /// Run Index
    pub const RUN: u8 = 6;
    /// Vector Store (reserved)
    pub const VECTOR: u8 = 7;
}

/// WAL entry type ranges for each primitive
///
/// Each primitive is allocated a 16-byte range for its entry types.
/// This allows up to 16 different operations per primitive.
pub mod wal_ranges {
    /// Core transaction control (0x00-0x0F)
    pub const CORE_START: u8 = 0x00;
    /// Core transaction control end
    pub const CORE_END: u8 = 0x0F;

    /// KV primitive (0x10-0x1F)
    pub const KV_START: u8 = 0x10;
    /// KV primitive end
    pub const KV_END: u8 = 0x1F;

    /// JSON primitive (0x20-0x2F)
    pub const JSON_START: u8 = 0x20;
    /// JSON primitive end
    pub const JSON_END: u8 = 0x2F;

    /// Event primitive (0x30-0x3F)
    pub const EVENT_START: u8 = 0x30;
    /// Event primitive end
    pub const EVENT_END: u8 = 0x3F;

    /// State primitive (0x40-0x4F)
    pub const STATE_START: u8 = 0x40;
    /// State primitive end
    pub const STATE_END: u8 = 0x4F;

    /// Run primitive (0x60-0x6F)
    pub const RUN_START: u8 = 0x60;
    /// Run primitive end
    pub const RUN_END: u8 = 0x6F;

    /// Vector primitive - RESERVED (0x70-0x7F)
    pub const VECTOR_START: u8 = 0x70;
    /// Vector primitive end
    pub const VECTOR_END: u8 = 0x7F;

    /// Future primitives (0x80-0xFF)
    pub const FUTURE_START: u8 = 0x80;
    /// Future primitives end
    pub const FUTURE_END: u8 = 0xFF;
}

/// Check which primitive a WAL entry type belongs to
///
/// Returns the primitive name, or None for unknown types.
pub fn primitive_for_wal_type(wal_type: u8) -> Option<&'static str> {
    use wal_ranges::*;
    match wal_type {
        CORE_START..=CORE_END => Some("core"),
        KV_START..=KV_END => Some("kv"),
        JSON_START..=JSON_END => Some("json"),
        EVENT_START..=EVENT_END => Some("event"),
        STATE_START..=STATE_END => Some("state"),
        RUN_START..=RUN_END => Some("run"),
        VECTOR_START..=VECTOR_END => Some("vector"),
        _ => None, // Unknown or future - not assigned
    }
}

/// Check if a WAL entry type is in a reserved future range
pub fn is_future_wal_type(wal_type: u8) -> bool {
    wal_type >= wal_ranges::FUTURE_START
}

/// Check if a WAL entry type is in the Vector range
pub fn is_vector_wal_type(wal_type: u8) -> bool {
    (wal_ranges::VECTOR_START..=wal_ranges::VECTOR_END).contains(&wal_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_primitive_for_wal_type() {
        // Core
        assert_eq!(primitive_for_wal_type(0x00), Some("core"));
        assert_eq!(primitive_for_wal_type(0x0F), Some("core"));

        // KV
        assert_eq!(primitive_for_wal_type(0x10), Some("kv"));
        assert_eq!(primitive_for_wal_type(0x11), Some("kv"));

        // JSON
        assert_eq!(primitive_for_wal_type(0x20), Some("json"));
        assert_eq!(primitive_for_wal_type(0x23), Some("json"));

        // Event
        assert_eq!(primitive_for_wal_type(0x30), Some("event"));

        // State
        assert_eq!(primitive_for_wal_type(0x40), Some("state"));
        assert_eq!(primitive_for_wal_type(0x42), Some("state"));

        // Run
        assert_eq!(primitive_for_wal_type(0x60), Some("run"));
        assert_eq!(primitive_for_wal_type(0x63), Some("run"));

        // Vector
        assert_eq!(primitive_for_wal_type(0x70), Some("vector"));
        assert_eq!(primitive_for_wal_type(0x7F), Some("vector"));

        // Future
        assert_eq!(primitive_for_wal_type(0x80), None);
        assert_eq!(primitive_for_wal_type(0xFF), None);
    }

    #[test]
    fn test_is_future_wal_type() {
        assert!(!is_future_wal_type(0x00));
        assert!(!is_future_wal_type(0x7F));
        assert!(is_future_wal_type(0x80));
        assert!(is_future_wal_type(0xFF));
    }

    #[test]
    fn test_is_vector_wal_type() {
        assert!(!is_vector_wal_type(0x6F));
        assert!(is_vector_wal_type(0x70));
        assert!(is_vector_wal_type(0x7F));
        assert!(!is_vector_wal_type(0x80));
    }

    #[test]
    fn test_primitive_type_ids() {
        assert_eq!(primitive_type_ids::KV, 1);
        assert_eq!(primitive_type_ids::JSON, 2);
        assert_eq!(primitive_type_ids::EVENT, 3);
        assert_eq!(primitive_type_ids::STATE, 4);
        assert_eq!(primitive_type_ids::RUN, 6);
        assert_eq!(primitive_type_ids::VECTOR, 7);
    }

    /// Mock primitive for testing
    struct MockPrimitive {
        data: Vec<u8>,
    }

    impl PrimitiveStorageExt for MockPrimitive {
        fn primitive_type_id(&self) -> u8 {
            99
        }

        fn wal_entry_types(&self) -> &'static [u8] {
            &[0x99, 0x9A]
        }

        fn snapshot_serialize(&self) -> Result<Vec<u8>, PrimitiveExtError> {
            Ok(self.data.clone())
        }

        fn snapshot_deserialize(&mut self, data: &[u8]) -> Result<(), PrimitiveExtError> {
            self.data = data.to_vec();
            Ok(())
        }

        fn apply_wal_entry(
            &mut self,
            entry_type: u8,
            payload: &[u8],
        ) -> Result<(), PrimitiveExtError> {
            match entry_type {
                0x99 => {
                    self.data.extend_from_slice(payload);
                    Ok(())
                }
                0x9A => {
                    self.data.clear();
                    Ok(())
                }
                _ => Err(PrimitiveExtError::UnknownEntryType(entry_type)),
            }
        }

        fn primitive_name(&self) -> &'static str {
            "mock"
        }
    }

    #[test]
    fn test_mock_primitive_handles_entry_type() {
        let prim = MockPrimitive { data: vec![] };

        assert!(prim.handles_entry_type(0x99));
        assert!(prim.handles_entry_type(0x9A));
        assert!(!prim.handles_entry_type(0x9B));
        assert!(!prim.handles_entry_type(0x10));
    }

    #[test]
    fn test_mock_primitive_snapshot_roundtrip() {
        let mut prim = MockPrimitive {
            data: vec![1, 2, 3, 4, 5],
        };

        // Serialize
        let serialized = prim.snapshot_serialize().unwrap();
        assert_eq!(serialized, vec![1, 2, 3, 4, 5]);

        // Deserialize
        prim.data.clear();
        prim.snapshot_deserialize(&serialized).unwrap();
        assert_eq!(prim.data, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_mock_primitive_apply_wal_entry() {
        let mut prim = MockPrimitive { data: vec![] };

        // Apply append entry
        prim.apply_wal_entry(0x99, &[1, 2, 3]).unwrap();
        assert_eq!(prim.data, vec![1, 2, 3]);

        prim.apply_wal_entry(0x99, &[4, 5]).unwrap();
        assert_eq!(prim.data, vec![1, 2, 3, 4, 5]);

        // Apply clear entry
        prim.apply_wal_entry(0x9A, &[]).unwrap();
        assert!(prim.data.is_empty());

        // Unknown entry type
        let result = prim.apply_wal_entry(0x9B, &[]);
        assert!(matches!(
            result,
            Err(PrimitiveExtError::UnknownEntryType(0x9B))
        ));
    }
}
