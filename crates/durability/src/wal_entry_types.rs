//! WAL Entry Type Registry
//!
//! This module defines the WAL entry type registry for durability.
//! Entry types are organized by ranges to enable extensibility:
//!
//! ## Entry Type Ranges
//!
//! | Range | Primitive | Description |
//! |-------|-----------|-------------|
//! | 0x00-0x0F | Core | Transaction control (commit, abort, snapshot) |
//! | 0x10-0x1F | KV | Key-value operations |
//! | 0x20-0x2F | JSON | JSON document operations |
//! | 0x30-0x3F | Event | Event log operations |
//! | 0x40-0x4F | State | State cell operations |
//! | 0x50-0x5F | Reserved | Reserved for future primitives |
//! | 0x60-0x6F | Run | Run lifecycle operations |
//! | 0x70-0x7F | Vector | Reserved for M8 Vector primitive |
//! | 0x80-0xFF | Future | Reserved for future primitives |
//!
//! ## Design Principles
//!
//! 1. **Extensibility**: New primitives can be added by allocating a new range
//! 2. **Forward Compatibility**: Unknown entry types can be skipped with a warning
//! 3. **Backward Compatibility**: Existing entry types maintain their values
//! 4. **Self-Describing**: Each entry type includes version for format evolution

use thiserror::Error;

/// Primitive kind for categorization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrimitiveKind {
    /// Key-value store
    Kv,
    /// JSON document store
    Json,
    /// Event log
    Event,
    /// State cell
    State,
    /// Run index
    Run,
    /// Vector store
    Vector,
}

impl PrimitiveKind {
    /// Get the entry type range for this primitive
    pub fn entry_type_range(&self) -> (u8, u8) {
        match self {
            PrimitiveKind::Kv => (0x10, 0x1F),
            PrimitiveKind::Json => (0x20, 0x2F),
            PrimitiveKind::Event => (0x30, 0x3F),
            PrimitiveKind::State => (0x40, 0x4F),
            PrimitiveKind::Run => (0x60, 0x6F),
            PrimitiveKind::Vector => (0x70, 0x7F),
        }
    }

    /// Get the primitive ID for snapshot sections
    pub fn primitive_id(&self) -> u8 {
        match self {
            PrimitiveKind::Kv => 1,
            PrimitiveKind::Json => 2,
            PrimitiveKind::Event => 3,
            PrimitiveKind::State => 4,
            PrimitiveKind::Run => 6,
            PrimitiveKind::Vector => 7,
        }
    }
}

/// WAL entry types with explicit byte values
///
/// Entry types are organized by ranges for extensibility:
/// - 0x00-0x0F: Core (transaction control)
/// - 0x10-0x1F: KV primitive
/// - 0x20-0x2F: JSON primitive
/// - 0x30-0x3F: Event primitive
/// - 0x40-0x4F: State primitive
/// - 0x50-0x5F: Reserved for future primitives
/// - 0x60-0x6F: Run primitive
/// - 0x70-0x7F: Reserved for Vector
/// - 0x80-0xFF: Reserved for future primitives
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WalEntryType {
    // ========================================================================
    // Core (0x00-0x0F) - Transaction control
    // ========================================================================
    /// Transaction commit marker
    ///
    /// Written at the end of a transaction to mark it as committed.
    /// Entries without a commit marker are discarded during recovery.
    TransactionCommit = 0x00,

    /// Transaction abort marker
    ///
    /// Written when a transaction is explicitly aborted.
    /// All entries with this tx_id are discarded during recovery.
    TransactionAbort = 0x01,

    /// Snapshot marker
    ///
    /// Written when a snapshot is taken, marking a point for WAL truncation.
    SnapshotMarker = 0x02,

    // ========================================================================
    // KV Primitive (0x10-0x1F)
    // ========================================================================
    /// KV put operation
    KvPut = 0x10,

    /// KV delete operation
    KvDelete = 0x11,

    // ========================================================================
    // JSON Primitive (0x20-0x2F)
    // ========================================================================
    /// JSON document creation
    JsonCreate = 0x20,

    /// JSON set value at path
    JsonSet = 0x21,

    /// JSON delete value at path
    JsonDelete = 0x22,

    /// JSON destroy (delete entire document)
    ///
    /// Removes a complete JSON document from storage.
    /// Unlike JsonDelete (which deletes at a path), this removes
    /// the entire document.
    JsonDestroy = 0x23,

    /// JSON patch (RFC 6902) - RESERVED
    ///
    /// Entry type for applying RFC 6902 JSON patches.
    /// Note: Currently only Set and Delete operations are supported.
    /// Full RFC 6902 compliance (add, test, move, copy) is deferred to M6+.
    JsonPatch = 0x24,

    // ========================================================================
    // Event Primitive (0x30-0x3F)
    // ========================================================================
    /// Event append
    EventAppend = 0x30,

    // ========================================================================
    // State Primitive (0x40-0x4F)
    // ========================================================================
    /// State initialization
    StateInit = 0x40,

    /// State set
    StateSet = 0x41,

    /// State transition
    StateTransition = 0x42,

    // ========================================================================
    // Run Primitive (0x60-0x6F)
    // ========================================================================
    /// Run creation
    RunCreate = 0x60,

    /// Run update
    RunUpdate = 0x61,

    /// Run end
    RunEnd = 0x62,

    /// Run begin
    RunBegin = 0x63,

    // ========================================================================
    // Vector Primitive (0x70-0x7F) - M8
    // ========================================================================
    /// Vector collection creation
    ///
    /// Creates a new vector collection with config (dimension, metric, dtype).
    VectorCollectionCreate = 0x70,

    /// Vector collection deletion
    ///
    /// Deletes a collection and all its vectors.
    VectorCollectionDelete = 0x71,

    /// Vector upsert (insert or update)
    ///
    /// TEMPORARY M8 FORMAT: Full embedding in WAL payload.
    /// This bloats WAL size (~3KB per 768-dim vector) but is correct.
    /// M9 may optimize with external embedding storage or delta encoding.
    VectorUpsert = 0x72,

    /// Vector deletion
    VectorDelete = 0x73,
}

impl WalEntryType {
    /// Check if this is a control entry (transaction or snapshot marker)
    ///
    /// Control entries are not part of the data operations but
    /// manage transaction boundaries and snapshot points.
    pub fn is_control(&self) -> bool {
        matches!(
            self,
            WalEntryType::TransactionCommit
                | WalEntryType::TransactionAbort
                | WalEntryType::SnapshotMarker
        )
    }

    /// Check if this is a transaction boundary (commit or abort)
    pub fn is_transaction_boundary(&self) -> bool {
        matches!(
            self,
            WalEntryType::TransactionCommit | WalEntryType::TransactionAbort
        )
    }

    /// Get the primitive this entry type belongs to
    ///
    /// Returns None for control entries (0x00-0x0F range).
    pub fn primitive_kind(&self) -> Option<PrimitiveKind> {
        let value = *self as u8;
        match value {
            0x00..=0x0F => None, // Core/control entries
            0x10..=0x1F => Some(PrimitiveKind::Kv),
            0x20..=0x2F => Some(PrimitiveKind::Json),
            0x30..=0x3F => Some(PrimitiveKind::Event),
            0x40..=0x4F => Some(PrimitiveKind::State),
            0x50..=0x5F => None, // Reserved for future primitives
            0x60..=0x6F => Some(PrimitiveKind::Run),
            0x70..=0x7F => Some(PrimitiveKind::Vector),
            _ => None, // Reserved for future
        }
    }

    /// Get the entry type range for a given byte value
    ///
    /// Returns the category name for the range this value belongs to.
    pub fn range_name(value: u8) -> &'static str {
        match value {
            0x00..=0x0F => "Core",
            0x10..=0x1F => "KV",
            0x20..=0x2F => "JSON",
            0x30..=0x3F => "Event",
            0x40..=0x4F => "State",
            0x50..=0x5F => "Reserved",
            0x60..=0x6F => "Run",
            0x70..=0x7F => "Vector",
            _ => "Future (reserved)",
        }
    }

    /// Check if a byte value is in a reserved range
    ///
    /// Returns true for 0x74-0xFF which are reserved for future use.
    /// (0x70-0x73 are now used by Vector primitive in M8.)
    pub fn is_reserved(value: u8) -> bool {
        value >= 0x74
    }

    /// Get human-readable description of this entry type
    pub fn description(&self) -> &'static str {
        match self {
            WalEntryType::TransactionCommit => "Transaction commit marker",
            WalEntryType::TransactionAbort => "Transaction abort marker",
            WalEntryType::SnapshotMarker => "Snapshot boundary marker",
            WalEntryType::KvPut => "KV put operation",
            WalEntryType::KvDelete => "KV delete operation",
            WalEntryType::JsonCreate => "JSON document creation",
            WalEntryType::JsonSet => "JSON set at path",
            WalEntryType::JsonDelete => "JSON delete at path",
            WalEntryType::JsonDestroy => "JSON destroy (delete entire document)",
            WalEntryType::JsonPatch => "JSON patch (RFC 6902)",
            WalEntryType::EventAppend => "Event append",
            WalEntryType::StateInit => "State initialization",
            WalEntryType::StateSet => "State set",
            WalEntryType::StateTransition => "State transition",
            WalEntryType::RunCreate => "Run creation",
            WalEntryType::RunUpdate => "Run update",
            WalEntryType::RunEnd => "Run end",
            WalEntryType::RunBegin => "Run begin",
            WalEntryType::VectorCollectionCreate => "Vector collection creation",
            WalEntryType::VectorCollectionDelete => "Vector collection deletion",
            WalEntryType::VectorUpsert => "Vector upsert",
            WalEntryType::VectorDelete => "Vector deletion",
        }
    }
}

/// Error when parsing WAL entry type
#[derive(Debug, Error)]
pub enum WalEntryTypeError {
    /// Unknown entry type value
    #[error("Unknown WAL entry type: 0x{value:02X} (range: {range})")]
    UnknownEntryType {
        /// The unknown byte value
        value: u8,
        /// The range this value belongs to
        range: &'static str,
    },

    /// Entry type in reserved range
    #[error("WAL entry type 0x{value:02X} is in reserved range: {range}")]
    ReservedEntryType {
        /// The reserved byte value
        value: u8,
        /// The reserved range name
        range: &'static str,
    },
}

impl TryFrom<u8> for WalEntryType {
    type Error = WalEntryTypeError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            // Core (0x00-0x0F)
            0x00 => Ok(WalEntryType::TransactionCommit),
            0x01 => Ok(WalEntryType::TransactionAbort),
            0x02 => Ok(WalEntryType::SnapshotMarker),

            // KV (0x10-0x1F)
            0x10 => Ok(WalEntryType::KvPut),
            0x11 => Ok(WalEntryType::KvDelete),

            // JSON (0x20-0x2F)
            0x20 => Ok(WalEntryType::JsonCreate),
            0x21 => Ok(WalEntryType::JsonSet),
            0x22 => Ok(WalEntryType::JsonDelete),
            0x23 => Ok(WalEntryType::JsonDestroy),
            0x24 => Ok(WalEntryType::JsonPatch),

            // Event (0x30-0x3F)
            0x30 => Ok(WalEntryType::EventAppend),

            // State (0x40-0x4F)
            0x40 => Ok(WalEntryType::StateInit),
            0x41 => Ok(WalEntryType::StateSet),
            0x42 => Ok(WalEntryType::StateTransition),

            // Reserved (0x50-0x5F)
            0x50..=0x5F => Err(WalEntryTypeError::ReservedEntryType {
                value,
                range: "Reserved (0x50-0x5F)",
            }),

            // Run (0x60-0x6F)
            0x60 => Ok(WalEntryType::RunCreate),
            0x61 => Ok(WalEntryType::RunUpdate),
            0x62 => Ok(WalEntryType::RunEnd),
            0x63 => Ok(WalEntryType::RunBegin),

            // Vector (0x70-0x7F) - M8
            0x70 => Ok(WalEntryType::VectorCollectionCreate),
            0x71 => Ok(WalEntryType::VectorCollectionDelete),
            0x72 => Ok(WalEntryType::VectorUpsert),
            0x73 => Ok(WalEntryType::VectorDelete),

            // Reserved in Vector range (unused M8 slots)
            0x74..=0x7F => Err(WalEntryTypeError::ReservedEntryType {
                value,
                range: "Vector (M8, unused)",
            }),
            0x80..=0xFF => Err(WalEntryTypeError::ReservedEntryType {
                value,
                range: "Future primitives",
            }),

            // Unknown in known ranges
            _ => Err(WalEntryTypeError::UnknownEntryType {
                value,
                range: WalEntryType::range_name(value),
            }),
        }
    }
}

impl From<WalEntryType> for u8 {
    fn from(entry_type: WalEntryType) -> Self {
        entry_type as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entry_type_values() {
        // Core
        assert_eq!(WalEntryType::TransactionCommit as u8, 0x00);
        assert_eq!(WalEntryType::TransactionAbort as u8, 0x01);
        assert_eq!(WalEntryType::SnapshotMarker as u8, 0x02);

        // KV
        assert_eq!(WalEntryType::KvPut as u8, 0x10);
        assert_eq!(WalEntryType::KvDelete as u8, 0x11);

        // JSON
        assert_eq!(WalEntryType::JsonCreate as u8, 0x20);
        assert_eq!(WalEntryType::JsonSet as u8, 0x21);
        assert_eq!(WalEntryType::JsonDelete as u8, 0x22);
        assert_eq!(WalEntryType::JsonDestroy as u8, 0x23);
        assert_eq!(WalEntryType::JsonPatch as u8, 0x24);

        // Event
        assert_eq!(WalEntryType::EventAppend as u8, 0x30);

        // State
        assert_eq!(WalEntryType::StateInit as u8, 0x40);
        assert_eq!(WalEntryType::StateSet as u8, 0x41);
        assert_eq!(WalEntryType::StateTransition as u8, 0x42);

        // Run
        assert_eq!(WalEntryType::RunCreate as u8, 0x60);
        assert_eq!(WalEntryType::RunUpdate as u8, 0x61);
        assert_eq!(WalEntryType::RunEnd as u8, 0x62);
        assert_eq!(WalEntryType::RunBegin as u8, 0x63);

        // Vector
        assert_eq!(WalEntryType::VectorCollectionCreate as u8, 0x70);
        assert_eq!(WalEntryType::VectorCollectionDelete as u8, 0x71);
        assert_eq!(WalEntryType::VectorUpsert as u8, 0x72);
        assert_eq!(WalEntryType::VectorDelete as u8, 0x73);
    }

    #[test]
    fn test_try_from_valid() {
        assert_eq!(
            WalEntryType::try_from(0x00).unwrap(),
            WalEntryType::TransactionCommit
        );
        assert_eq!(WalEntryType::try_from(0x10).unwrap(), WalEntryType::KvPut);
        assert_eq!(
            WalEntryType::try_from(0x20).unwrap(),
            WalEntryType::JsonCreate
        );
        assert_eq!(
            WalEntryType::try_from(0x30).unwrap(),
            WalEntryType::EventAppend
        );
        assert_eq!(
            WalEntryType::try_from(0x40).unwrap(),
            WalEntryType::StateInit
        );
        // 0x50 is reserved
        assert!(WalEntryType::try_from(0x50).is_err());
        assert_eq!(
            WalEntryType::try_from(0x60).unwrap(),
            WalEntryType::RunCreate
        );
        assert_eq!(
            WalEntryType::try_from(0x70).unwrap(),
            WalEntryType::VectorCollectionCreate
        );
        assert_eq!(
            WalEntryType::try_from(0x72).unwrap(),
            WalEntryType::VectorUpsert
        );
    }

    #[test]
    fn test_try_from_unknown() {
        // Unknown in Core range
        let result = WalEntryType::try_from(0x0F);
        assert!(matches!(
            result,
            Err(WalEntryTypeError::UnknownEntryType { value: 0x0F, .. })
        ));

        // Unknown in KV range
        let result = WalEntryType::try_from(0x1F);
        assert!(matches!(
            result,
            Err(WalEntryTypeError::UnknownEntryType { value: 0x1F, .. })
        ));
    }

    #[test]
    fn test_try_from_reserved() {
        // Unused Vector range slots (0x74-0x7F)
        let result = WalEntryType::try_from(0x74);
        assert!(matches!(
            result,
            Err(WalEntryTypeError::ReservedEntryType { value: 0x74, .. })
        ));

        let result = WalEntryType::try_from(0x7F);
        assert!(matches!(
            result,
            Err(WalEntryTypeError::ReservedEntryType { value: 0x7F, .. })
        ));

        // Future range
        let result = WalEntryType::try_from(0x80);
        assert!(matches!(
            result,
            Err(WalEntryTypeError::ReservedEntryType { value: 0x80, .. })
        ));

        let result = WalEntryType::try_from(0xFF);
        assert!(matches!(
            result,
            Err(WalEntryTypeError::ReservedEntryType { value: 0xFF, .. })
        ));
    }

    #[test]
    fn test_is_control() {
        assert!(WalEntryType::TransactionCommit.is_control());
        assert!(WalEntryType::TransactionAbort.is_control());
        assert!(WalEntryType::SnapshotMarker.is_control());

        assert!(!WalEntryType::KvPut.is_control());
        assert!(!WalEntryType::JsonCreate.is_control());
        assert!(!WalEntryType::EventAppend.is_control());
    }

    #[test]
    fn test_is_transaction_boundary() {
        assert!(WalEntryType::TransactionCommit.is_transaction_boundary());
        assert!(WalEntryType::TransactionAbort.is_transaction_boundary());

        assert!(!WalEntryType::SnapshotMarker.is_transaction_boundary());
        assert!(!WalEntryType::KvPut.is_transaction_boundary());
    }

    #[test]
    fn test_primitive_kind() {
        // Core entries have no primitive
        assert_eq!(WalEntryType::TransactionCommit.primitive_kind(), None);
        assert_eq!(WalEntryType::TransactionAbort.primitive_kind(), None);
        assert_eq!(WalEntryType::SnapshotMarker.primitive_kind(), None);

        // Primitive entries
        assert_eq!(
            WalEntryType::KvPut.primitive_kind(),
            Some(PrimitiveKind::Kv)
        );
        assert_eq!(
            WalEntryType::KvDelete.primitive_kind(),
            Some(PrimitiveKind::Kv)
        );
        assert_eq!(
            WalEntryType::JsonCreate.primitive_kind(),
            Some(PrimitiveKind::Json)
        );
        assert_eq!(
            WalEntryType::JsonSet.primitive_kind(),
            Some(PrimitiveKind::Json)
        );
        assert_eq!(
            WalEntryType::EventAppend.primitive_kind(),
            Some(PrimitiveKind::Event)
        );
        assert_eq!(
            WalEntryType::StateInit.primitive_kind(),
            Some(PrimitiveKind::State)
        );
        assert_eq!(
            WalEntryType::RunCreate.primitive_kind(),
            Some(PrimitiveKind::Run)
        );
        assert_eq!(
            WalEntryType::VectorCollectionCreate.primitive_kind(),
            Some(PrimitiveKind::Vector)
        );
        assert_eq!(
            WalEntryType::VectorUpsert.primitive_kind(),
            Some(PrimitiveKind::Vector)
        );
    }

    #[test]
    fn test_range_name() {
        assert_eq!(WalEntryType::range_name(0x00), "Core");
        assert_eq!(WalEntryType::range_name(0x0F), "Core");
        assert_eq!(WalEntryType::range_name(0x10), "KV");
        assert_eq!(WalEntryType::range_name(0x20), "JSON");
        assert_eq!(WalEntryType::range_name(0x30), "Event");
        assert_eq!(WalEntryType::range_name(0x40), "State");
        assert_eq!(WalEntryType::range_name(0x50), "Reserved");
        assert_eq!(WalEntryType::range_name(0x60), "Run");
        assert_eq!(WalEntryType::range_name(0x70), "Vector");
        assert_eq!(WalEntryType::range_name(0x80), "Future (reserved)");
        assert_eq!(WalEntryType::range_name(0xFF), "Future (reserved)");
    }

    #[test]
    fn test_is_reserved() {
        // Not reserved
        assert!(!WalEntryType::is_reserved(0x00));
        assert!(!WalEntryType::is_reserved(0x6F));
        // Vector entries 0x70-0x73 are now used
        assert!(!WalEntryType::is_reserved(0x70));
        assert!(!WalEntryType::is_reserved(0x73));

        // Reserved (unused Vector slots and future range)
        assert!(WalEntryType::is_reserved(0x74));
        assert!(WalEntryType::is_reserved(0x7F));
        assert!(WalEntryType::is_reserved(0x80));
        assert!(WalEntryType::is_reserved(0xFF));
    }

    #[test]
    fn test_roundtrip() {
        // All defined entry types should roundtrip
        let entry_types = [
            WalEntryType::TransactionCommit,
            WalEntryType::TransactionAbort,
            WalEntryType::SnapshotMarker,
            WalEntryType::KvPut,
            WalEntryType::KvDelete,
            WalEntryType::JsonCreate,
            WalEntryType::JsonSet,
            WalEntryType::JsonDelete,
            WalEntryType::JsonDestroy,
            WalEntryType::JsonPatch,
            WalEntryType::EventAppend,
            WalEntryType::StateInit,
            WalEntryType::StateSet,
            WalEntryType::StateTransition,
            WalEntryType::RunCreate,
            WalEntryType::RunUpdate,
            WalEntryType::RunEnd,
            WalEntryType::RunBegin,
            WalEntryType::VectorCollectionCreate,
            WalEntryType::VectorCollectionDelete,
            WalEntryType::VectorUpsert,
            WalEntryType::VectorDelete,
        ];

        for entry_type in entry_types {
            let byte_value: u8 = entry_type.into();
            let parsed = WalEntryType::try_from(byte_value).unwrap();
            assert_eq!(entry_type, parsed);
        }
    }

    #[test]
    fn test_primitive_kind_range() {
        // Verify primitive ranges are correct
        let kv_range = PrimitiveKind::Kv.entry_type_range();
        assert_eq!(kv_range, (0x10, 0x1F));

        let json_range = PrimitiveKind::Json.entry_type_range();
        assert_eq!(json_range, (0x20, 0x2F));

        let event_range = PrimitiveKind::Event.entry_type_range();
        assert_eq!(event_range, (0x30, 0x3F));

        let state_range = PrimitiveKind::State.entry_type_range();
        assert_eq!(state_range, (0x40, 0x4F));

        let run_range = PrimitiveKind::Run.entry_type_range();
        assert_eq!(run_range, (0x60, 0x6F));

        let vector_range = PrimitiveKind::Vector.entry_type_range();
        assert_eq!(vector_range, (0x70, 0x7F));
    }

    #[test]
    fn test_primitive_id() {
        assert_eq!(PrimitiveKind::Kv.primitive_id(), 1);
        assert_eq!(PrimitiveKind::Json.primitive_id(), 2);
        assert_eq!(PrimitiveKind::Event.primitive_id(), 3);
        assert_eq!(PrimitiveKind::State.primitive_id(), 4);
        assert_eq!(PrimitiveKind::Run.primitive_id(), 6);
        assert_eq!(PrimitiveKind::Vector.primitive_id(), 7);
    }

    #[test]
    fn test_description_non_empty() {
        let entry_types = [
            WalEntryType::TransactionCommit,
            WalEntryType::TransactionAbort,
            WalEntryType::SnapshotMarker,
            WalEntryType::KvPut,
            WalEntryType::KvDelete,
            WalEntryType::JsonCreate,
            WalEntryType::JsonSet,
            WalEntryType::JsonDelete,
            WalEntryType::JsonDestroy,
            WalEntryType::JsonPatch,
            WalEntryType::EventAppend,
            WalEntryType::StateInit,
            WalEntryType::StateSet,
            WalEntryType::StateTransition,
            WalEntryType::RunCreate,
            WalEntryType::RunUpdate,
            WalEntryType::RunEnd,
            WalEntryType::RunBegin,
            WalEntryType::VectorCollectionCreate,
            WalEntryType::VectorCollectionDelete,
            WalEntryType::VectorUpsert,
            WalEntryType::VectorDelete,
        ];

        for entry_type in entry_types {
            assert!(!entry_type.description().is_empty());
        }
    }

    #[test]
    fn test_0x50_range_reserved() {
        // The 0x50-0x5F range is reserved
        for val in 0x50..=0x5F {
            let result = WalEntryType::try_from(val);
            assert!(matches!(
                result,
                Err(WalEntryTypeError::ReservedEntryType { .. })
            ));
        }
    }
}
