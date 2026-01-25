//! Snapshot format types
//!
//! This module defines the snapshot envelope format used for point-in-time
//! database state persistence. Snapshots compress WAL effects and are used
//! for faster recovery.
//!
//! ## Snapshot File Layout
//!
//! ```text
//! +------------------+
//! | Magic (10 bytes) |  "INMEM_SNAP"
//! +------------------+
//! | Version (4)      |  Format version (1)
//! +------------------+
//! | Timestamp (8)    |  Microseconds since epoch
//! +------------------+
//! | WAL Offset (8)   |  WAL position covered
//! +------------------+
//! | Tx Count (8)     |  Transactions included
//! +------------------+
//! | Primitive Count  |  Number of primitive sections (1 byte)
//! +------------------+
//! | Primitive 1      |  Type (1) + Length (8) + Data
//! +------------------+
//! | ...              |
//! +------------------+
//! | CRC32 (4)        |  Checksum of everything above
//! +------------------+
//! ```
//!
//! ## Key Principle
//!
//! Snapshots are **physical** (materialized state), not **semantic** (history).
//! They compress WAL effects but are not the history itself.

use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

/// Snapshot file magic bytes
pub const SNAPSHOT_MAGIC: &[u8; 10] = b"INMEM_SNAP";

/// Snapshot format version 1
pub const SNAPSHOT_VERSION_1: u32 = 1;

/// Header size: Magic(10) + Version(4) + Timestamp(8) + Offset(8) + TxCount(8)
pub const SNAPSHOT_HEADER_SIZE: usize = 38;

/// Minimum snapshot size: Header + PrimitiveCount(1) + CRC32(4)
pub const MIN_SNAPSHOT_SIZE: usize = SNAPSHOT_HEADER_SIZE + 1 + 4;

// ============================================================================
// Primitive Type IDs
// ============================================================================

/// Primitive type IDs for snapshot sections
pub mod primitive_ids {
    /// KV Store primitive
    pub const KV: u8 = 1;
    /// JSON Store primitive
    pub const JSON: u8 = 2;
    /// Event Log primitive
    pub const EVENT: u8 = 3;
    /// State Cell primitive
    pub const STATE: u8 = 4;
    /// Run Index primitive (ID 5 was formerly TRACE, skipped for compatibility)
    pub const RUN: u8 = 6;
    /// Vector primitive
    pub const VECTOR: u8 = 7;

    /// Get name for primitive type ID
    pub fn name(id: u8) -> &'static str {
        match id {
            KV => "KV",
            JSON => "JSON",
            EVENT => "Event",
            STATE => "State",
            RUN => "Run",
            VECTOR => "Vector",
            _ => "Unknown",
        }
    }

    /// Check if primitive ID is valid
    pub fn is_valid(id: u8) -> bool {
        // IDs 1-4 (KV, JSON, Event, State), 6-7 (Run, Vector) are valid
        // ID 5 (formerly Trace) is no longer valid
        matches!(id, 1..=4 | 6..=7)
    }
}

// ============================================================================
// Snapshot Envelope
// ============================================================================

/// Snapshot envelope (parsed representation)
///
/// Contains all metadata and primitive sections from a snapshot file.
#[derive(Debug, Clone)]
pub struct SnapshotEnvelope {
    /// Format version
    pub version: u32,
    /// When snapshot was taken (microseconds since epoch)
    pub timestamp_micros: u64,
    /// WAL offset this snapshot covers up to
    pub wal_offset: u64,
    /// Number of transactions included
    pub transaction_count: u64,
    /// Primitive sections
    pub sections: Vec<PrimitiveSection>,
    /// CRC32 checksum (of everything before this)
    pub checksum: u32,
}

impl SnapshotEnvelope {
    /// Create a new empty envelope
    pub fn new(wal_offset: u64, transaction_count: u64) -> Self {
        SnapshotEnvelope {
            version: SNAPSHOT_VERSION_1,
            timestamp_micros: now_micros(),
            wal_offset,
            transaction_count,
            sections: Vec::new(),
            checksum: 0,
        }
    }

    /// Add a primitive section
    pub fn add_section(&mut self, primitive_type: u8, data: Vec<u8>) {
        self.sections.push(PrimitiveSection {
            primitive_type,
            data,
        });
    }

    /// Get section by primitive type
    pub fn get_section(&self, primitive_type: u8) -> Option<&PrimitiveSection> {
        self.sections
            .iter()
            .find(|s| s.primitive_type == primitive_type)
    }
}

// ============================================================================
// Primitive Section
// ============================================================================

/// A section of snapshot data for one primitive
#[derive(Debug, Clone)]
pub struct PrimitiveSection {
    /// Primitive type ID
    pub primitive_type: u8,
    /// Serialized data
    pub data: Vec<u8>,
}

impl PrimitiveSection {
    /// Create a new primitive section
    pub fn new(primitive_type: u8, data: Vec<u8>) -> Self {
        PrimitiveSection {
            primitive_type,
            data,
        }
    }

    /// Get the primitive name
    pub fn name(&self) -> &'static str {
        primitive_ids::name(self.primitive_type)
    }

    /// Calculate serialized size: type(1) + length(8) + data
    pub fn serialized_size(&self) -> usize {
        1 + 8 + self.data.len()
    }
}

// ============================================================================
// Snapshot Header
// ============================================================================

/// Snapshot header with metadata
///
/// Contains the fixed-size header portion of a snapshot file.
#[derive(Debug, Clone)]
pub struct SnapshotHeader {
    /// Format version
    pub version: u32,
    /// When snapshot was taken (microseconds since epoch)
    pub timestamp_micros: u64,
    /// WAL offset this snapshot covers up to
    pub wal_offset: u64,
    /// Number of transactions included
    pub transaction_count: u64,
    /// Database version that created this snapshot (not stored in binary)
    pub db_version: String,
}

impl SnapshotHeader {
    /// Create new header with current timestamp
    pub fn new(wal_offset: u64, transaction_count: u64) -> Self {
        SnapshotHeader {
            version: SNAPSHOT_VERSION_1,
            timestamp_micros: now_micros(),
            wal_offset,
            transaction_count,
            db_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    /// Create header with explicit timestamp
    pub fn with_timestamp(wal_offset: u64, transaction_count: u64, timestamp_micros: u64) -> Self {
        SnapshotHeader {
            version: SNAPSHOT_VERSION_1,
            timestamp_micros,
            wal_offset,
            transaction_count,
            db_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    /// Serialize header to bytes (including magic)
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(SNAPSHOT_HEADER_SIZE);
        buf.extend_from_slice(SNAPSHOT_MAGIC);
        buf.extend_from_slice(&self.version.to_le_bytes());
        buf.extend_from_slice(&self.timestamp_micros.to_le_bytes());
        buf.extend_from_slice(&self.wal_offset.to_le_bytes());
        buf.extend_from_slice(&self.transaction_count.to_le_bytes());
        buf
    }

    /// Parse header from bytes (including magic)
    pub fn from_bytes(data: &[u8]) -> Result<Self, SnapshotError> {
        if data.len() < SNAPSHOT_HEADER_SIZE {
            return Err(SnapshotError::TooShort {
                expected: SNAPSHOT_HEADER_SIZE,
                actual: data.len(),
            });
        }

        // Validate magic
        if &data[0..10] != SNAPSHOT_MAGIC {
            return Err(SnapshotError::InvalidMagic {
                found: data[0..10].to_vec(),
            });
        }

        let version = u32::from_le_bytes([data[10], data[11], data[12], data[13]]);
        if version != SNAPSHOT_VERSION_1 {
            return Err(SnapshotError::UnsupportedVersion(version));
        }

        let timestamp_micros = u64::from_le_bytes(data[14..22].try_into().unwrap());
        let wal_offset = u64::from_le_bytes(data[22..30].try_into().unwrap());
        let transaction_count = u64::from_le_bytes(data[30..38].try_into().unwrap());

        Ok(SnapshotHeader {
            version,
            timestamp_micros,
            wal_offset,
            transaction_count,
            db_version: String::new(), // Not stored in binary format
        })
    }
}

// ============================================================================
// Snapshot Info
// ============================================================================

/// Snapshot info returned after successful write
#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    /// Path to snapshot file
    pub path: std::path::PathBuf,
    /// Timestamp when snapshot was taken
    pub timestamp_micros: u64,
    /// WAL offset covered by this snapshot
    pub wal_offset: u64,
    /// Total size in bytes
    pub size_bytes: u64,
}

// ============================================================================
// Error Types
// ============================================================================

/// Snapshot errors
#[derive(Debug, Error)]
pub enum SnapshotError {
    /// Snapshot data too short
    #[error("Snapshot too short: expected at least {expected} bytes, got {actual}")]
    TooShort {
        /// Expected minimum size
        expected: usize,
        /// Actual size
        actual: usize,
    },

    /// Invalid magic bytes
    #[error("Invalid magic bytes: expected INMEM_SNAP, found {:?}", found)]
    InvalidMagic {
        /// Found bytes
        found: Vec<u8>,
    },

    /// Unsupported version
    #[error("Unsupported snapshot version: {0}")]
    UnsupportedVersion(u32),

    /// Checksum mismatch
    #[error("Checksum mismatch: expected {expected:08x}, got {actual:08x}")]
    ChecksumMismatch {
        /// Expected checksum
        expected: u32,
        /// Actual checksum
        actual: u32,
    },

    /// Unknown primitive type
    #[error("Unknown primitive type: {0}")]
    UnknownPrimitive(u8),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialize(String),

    /// Deserialization error
    #[error("Deserialization error: {0}")]
    Deserialize(String),

    /// Missing primitive section
    #[error("Missing primitive section: {0}")]
    MissingSection(&'static str),

    /// Primitive data corrupted
    #[error("Primitive data corrupted: {primitive} - {message}")]
    PrimitiveCorrupted {
        /// Primitive name
        primitive: &'static str,
        /// Error message
        message: String,
    },
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Get current time in microseconds since epoch
///
/// Returns 0 if system clock is before Unix epoch (clock went backwards).
pub fn now_micros() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_magic_bytes() {
        assert_eq!(SNAPSHOT_MAGIC.len(), 10);
        assert_eq!(SNAPSHOT_MAGIC, b"INMEM_SNAP");
    }

    #[test]
    fn test_snapshot_version() {
        assert_eq!(SNAPSHOT_VERSION_1, 1);
    }

    #[test]
    fn test_header_size() {
        // Magic(10) + Version(4) + Timestamp(8) + Offset(8) + TxCount(8) = 38
        assert_eq!(SNAPSHOT_HEADER_SIZE, 38);
    }

    #[test]
    fn test_primitive_ids_unique() {
        let ids = [
            primitive_ids::KV,
            primitive_ids::JSON,
            primitive_ids::EVENT,
            primitive_ids::STATE,
            primitive_ids::RUN,
            primitive_ids::VECTOR,
        ];
        let mut set = HashSet::new();
        for id in ids {
            assert!(set.insert(id), "Duplicate primitive ID: {}", id);
        }
    }

    #[test]
    fn test_primitive_ids_values() {
        assert_eq!(primitive_ids::KV, 1);
        assert_eq!(primitive_ids::JSON, 2);
        assert_eq!(primitive_ids::EVENT, 3);
        assert_eq!(primitive_ids::STATE, 4);
        // ID 5 was formerly TRACE, now skipped
        assert_eq!(primitive_ids::RUN, 6);
        assert_eq!(primitive_ids::VECTOR, 7);
    }

    #[test]
    fn test_primitive_ids_names() {
        assert_eq!(primitive_ids::name(primitive_ids::KV), "KV");
        assert_eq!(primitive_ids::name(primitive_ids::JSON), "JSON");
        assert_eq!(primitive_ids::name(primitive_ids::EVENT), "Event");
        assert_eq!(primitive_ids::name(primitive_ids::STATE), "State");
        // ID 5 (formerly TRACE) is now unknown
        assert_eq!(primitive_ids::name(5), "Unknown");
        assert_eq!(primitive_ids::name(primitive_ids::RUN), "Run");
        assert_eq!(primitive_ids::name(primitive_ids::VECTOR), "Vector");
        assert_eq!(primitive_ids::name(99), "Unknown");
    }

    #[test]
    fn test_primitive_ids_is_valid() {
        // Valid IDs: 1-4 (KV, JSON, Event, State), 6-7 (Run, Vector)
        for id in 1..=4 {
            assert!(primitive_ids::is_valid(id), "ID {} should be valid", id);
        }
        assert!(!primitive_ids::is_valid(5), "ID 5 (formerly Trace) should not be valid");
        for id in 6..=7 {
            assert!(primitive_ids::is_valid(id), "ID {} should be valid", id);
        }
        assert!(!primitive_ids::is_valid(0));
        assert!(!primitive_ids::is_valid(8));
        assert!(!primitive_ids::is_valid(255));
    }

    #[test]
    fn test_snapshot_envelope_new() {
        let envelope = SnapshotEnvelope::new(12345, 100);
        assert_eq!(envelope.version, SNAPSHOT_VERSION_1);
        assert_eq!(envelope.wal_offset, 12345);
        assert_eq!(envelope.transaction_count, 100);
        assert!(envelope.sections.is_empty());
        assert!(envelope.timestamp_micros > 0);
    }

    #[test]
    fn test_snapshot_envelope_add_section() {
        let mut envelope = SnapshotEnvelope::new(0, 0);
        envelope.add_section(primitive_ids::KV, vec![1, 2, 3]);
        envelope.add_section(primitive_ids::JSON, vec![4, 5, 6]);

        assert_eq!(envelope.sections.len(), 2);
        assert_eq!(envelope.sections[0].primitive_type, primitive_ids::KV);
        assert_eq!(envelope.sections[0].data, vec![1, 2, 3]);
        assert_eq!(envelope.sections[1].primitive_type, primitive_ids::JSON);
    }

    #[test]
    fn test_snapshot_envelope_get_section() {
        let mut envelope = SnapshotEnvelope::new(0, 0);
        envelope.add_section(primitive_ids::KV, vec![1, 2, 3]);

        let section = envelope.get_section(primitive_ids::KV);
        assert!(section.is_some());
        assert_eq!(section.unwrap().data, vec![1, 2, 3]);

        let missing = envelope.get_section(primitive_ids::JSON);
        assert!(missing.is_none());
    }

    #[test]
    fn test_primitive_section_new() {
        let section = PrimitiveSection::new(primitive_ids::KV, vec![1, 2, 3]);
        assert_eq!(section.primitive_type, primitive_ids::KV);
        assert_eq!(section.data, vec![1, 2, 3]);
    }

    #[test]
    fn test_primitive_section_name() {
        let section = PrimitiveSection::new(primitive_ids::JSON, vec![]);
        assert_eq!(section.name(), "JSON");
    }

    #[test]
    fn test_primitive_section_serialized_size() {
        let section = PrimitiveSection::new(primitive_ids::KV, vec![1, 2, 3]);
        // type(1) + length(8) + data(3) = 12
        assert_eq!(section.serialized_size(), 12);
    }

    #[test]
    fn test_snapshot_header_new() {
        let header = SnapshotHeader::new(12345, 100);
        assert_eq!(header.version, SNAPSHOT_VERSION_1);
        assert_eq!(header.wal_offset, 12345);
        assert_eq!(header.transaction_count, 100);
        assert!(header.timestamp_micros > 0);
        assert!(!header.db_version.is_empty());
    }

    #[test]
    fn test_snapshot_header_with_timestamp() {
        let header = SnapshotHeader::with_timestamp(12345, 100, 9999);
        assert_eq!(header.timestamp_micros, 9999);
    }

    #[test]
    fn test_snapshot_header_to_bytes() {
        let header = SnapshotHeader::with_timestamp(12345, 100, 9999);
        let bytes = header.to_bytes();

        assert_eq!(bytes.len(), SNAPSHOT_HEADER_SIZE);
        assert_eq!(&bytes[0..10], SNAPSHOT_MAGIC);
    }

    #[test]
    fn test_snapshot_header_roundtrip() {
        let header = SnapshotHeader::with_timestamp(12345, 100, 9999);
        let bytes = header.to_bytes();

        let parsed = SnapshotHeader::from_bytes(&bytes).unwrap();
        assert_eq!(parsed.version, header.version);
        assert_eq!(parsed.wal_offset, header.wal_offset);
        assert_eq!(parsed.transaction_count, header.transaction_count);
        assert_eq!(parsed.timestamp_micros, header.timestamp_micros);
    }

    #[test]
    fn test_snapshot_header_from_bytes_too_short() {
        let data = vec![0u8; 10];
        let result = SnapshotHeader::from_bytes(&data);
        assert!(matches!(result, Err(SnapshotError::TooShort { .. })));
    }

    #[test]
    fn test_snapshot_header_invalid_magic() {
        let mut data = vec![0u8; SNAPSHOT_HEADER_SIZE];
        data[0..10].copy_from_slice(b"WRONGMAGIC");

        let result = SnapshotHeader::from_bytes(&data);
        assert!(matches!(result, Err(SnapshotError::InvalidMagic { .. })));
    }

    #[test]
    fn test_snapshot_header_unsupported_version() {
        let header = SnapshotHeader::new(0, 0);
        let mut bytes = header.to_bytes();
        // Overwrite version with 99
        bytes[10..14].copy_from_slice(&99u32.to_le_bytes());

        let result = SnapshotHeader::from_bytes(&bytes);
        assert!(matches!(result, Err(SnapshotError::UnsupportedVersion(99))));
    }

    #[test]
    fn test_now_micros() {
        let t1 = now_micros();
        let t2 = now_micros();
        assert!(t2 >= t1);
        // Should be a reasonable timestamp (after 2024)
        assert!(t1 > 1_700_000_000_000_000);
    }

    #[test]
    fn test_snapshot_error_display() {
        let err = SnapshotError::TooShort {
            expected: 100,
            actual: 50,
        };
        assert!(err.to_string().contains("100"));
        assert!(err.to_string().contains("50"));

        let err = SnapshotError::ChecksumMismatch {
            expected: 0x12345678,
            actual: 0xDEADBEEF,
        };
        let msg = err.to_string();
        assert!(msg.contains("12345678"));
        assert!(msg.contains("deadbeef"));
    }
}
