//! WAL Entry Types and Envelope Format
//!
//! This module implements the WAL entry format with self-validating entries:
//!
//! ## Entry Format
//!
//! ```text
//! +----------------+
//! | Length (u32)   |  Total bytes after this field (type+version+txid+payload+crc)
//! +----------------+
//! | Type (u8)      |  Entry type from registry
//! +----------------+
//! | Version (u8)   |  Format version for this entry type
//! +----------------+
//! | TxId (16)      |  Transaction ID (UUID, nil for non-transactional)
//! +----------------+
//! | Payload        |  Type-specific data
//! +----------------+
//! | CRC32 (u32)    |  Checksum of Type + Version + TxId + Payload
//! +----------------+
//! ```
//!
//! ## Key Features
//!
//! 1. **Version field**: Enables format evolution for each entry type
//! 2. **TxId in envelope**: Groups entries by transaction for atomic recovery
//! 3. **Explicit transaction framing**: Commit marker required for visibility
//!
//! ## Why This Format
//!
//! - **Transaction grouping**: Recovery can group entries by tx_id without deserializing payload
//! - **Format evolution**: Version field enables backward-compatible changes
//! - **Self-validating**: CRC32 detects corruption
//! - **Prefix-consistent recovery**: Only committed transactions are visible

use crate::wal_entry_types::{WalEntryType, WalEntryTypeError};
use crc32fast::Hasher;
use serde::{Deserialize, Serialize};
use std::io::Write;
use thiserror::Error;
use uuid::Uuid;

// ============================================================================
// Transaction ID
// ============================================================================

/// Transaction ID for grouping WAL entries
///
/// Every data mutation entry includes a TxId to enable:
/// - Atomic recovery (either all entries with tx_id are visible, or none)
/// - Transaction framing (entries without commit marker are discarded)
/// - Efficient grouping during replay
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TxId(Uuid);

impl TxId {
    /// Create a new unique transaction ID
    pub fn new() -> Self {
        TxId(Uuid::new_v4())
    }

    /// Create a nil transaction ID (for non-transactional entries)
    pub fn nil() -> Self {
        TxId(Uuid::nil())
    }

    /// Check if this is a nil transaction ID
    pub fn is_nil(&self) -> bool {
        self.0.is_nil()
    }

    /// Convert to bytes (16 bytes)
    pub fn to_bytes(&self) -> [u8; 16] {
        *self.0.as_bytes()
    }

    /// Create from bytes
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        TxId(Uuid::from_bytes(bytes))
    }

    /// Get the inner UUID
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for TxId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TxId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<Uuid> for TxId {
    fn from(uuid: Uuid) -> Self {
        TxId(uuid)
    }
}

impl From<TxId> for Uuid {
    fn from(tx_id: TxId) -> Self {
        tx_id.0
    }
}

// ============================================================================
// WAL Entry Errors
// ============================================================================

/// Errors that can occur during WAL entry operations
#[derive(Debug, Error)]
pub enum WalEntryError {
    /// Entry too short to be valid
    #[error("WAL entry too short: expected at least {expected} bytes, got {actual}")]
    TooShort {
        /// Minimum expected bytes
        expected: usize,
        /// Actual bytes received
        actual: usize,
    },

    /// Entry length exceeds maximum allowed size
    #[error("WAL entry too large: {size} bytes (max: {max})")]
    TooLarge {
        /// Actual size
        size: usize,
        /// Maximum allowed size
        max: usize,
    },

    /// CRC32 checksum mismatch (corruption detected)
    #[error(
        "CRC32 checksum mismatch at offset {offset}: expected 0x{expected:08X}, got 0x{actual:08X}"
    )]
    ChecksumMismatch {
        /// File offset where corruption was detected
        offset: u64,
        /// Expected checksum
        expected: u32,
        /// Actual checksum
        actual: u32,
    },

    /// Invalid entry type
    #[error("Invalid entry type: {0}")]
    InvalidEntryType(#[from] WalEntryTypeError),

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),

    /// Deserialization error
    #[error("Deserialization error at offset {offset}: {message}")]
    Deserialization {
        /// File offset
        offset: u64,
        /// Error message
        message: String,
    },
}

// ============================================================================
// WAL Entry
// ============================================================================

/// Maximum WAL entry size (16 MB)
pub const MAX_WAL_ENTRY_SIZE: usize = 16 * 1024 * 1024;

/// Minimum valid entry size: type(1) + version(1) + txid(16) + crc(4) = 22 bytes
pub const MIN_WAL_ENTRY_SIZE: usize = 1 + 1 + 16 + 4;

/// Current format version for WAL entries
pub const WAL_FORMAT_VERSION: u8 = 1;

/// WAL entry with envelope format
///
/// This is the self-validating entry format:
/// - Entry type identifies the operation
/// - Version enables format evolution
/// - TxId groups entries for atomic recovery
/// - Payload contains operation-specific data
/// - CRC32 ensures integrity
#[derive(Debug, Clone, PartialEq)]
pub struct WalEntry {
    /// Entry type from the registry
    pub entry_type: WalEntryType,

    /// Format version for this entry type
    pub version: u8,

    /// Transaction ID (nil for non-transactional entries like snapshot markers)
    pub tx_id: TxId,

    /// Payload (type-specific serialized data)
    pub payload: Vec<u8>,
}

impl WalEntry {
    /// Create a new WAL entry
    pub fn new(entry_type: WalEntryType, tx_id: TxId, payload: Vec<u8>) -> Self {
        WalEntry {
            entry_type,
            version: WAL_FORMAT_VERSION,
            tx_id,
            payload,
        }
    }

    /// Create a transaction commit marker
    pub fn commit_marker(tx_id: TxId) -> Self {
        WalEntry {
            entry_type: WalEntryType::TransactionCommit,
            version: WAL_FORMAT_VERSION,
            tx_id,
            payload: vec![],
        }
    }

    /// Create a transaction abort marker
    pub fn abort_marker(tx_id: TxId) -> Self {
        WalEntry {
            entry_type: WalEntryType::TransactionAbort,
            version: WAL_FORMAT_VERSION,
            tx_id,
            payload: vec![],
        }
    }

    /// Create a snapshot marker
    pub fn snapshot_marker(snapshot_id: Uuid, wal_offset: u64) -> Self {
        let mut payload = Vec::with_capacity(24);
        payload.extend_from_slice(snapshot_id.as_bytes());
        payload.extend_from_slice(&wal_offset.to_le_bytes());

        WalEntry {
            entry_type: WalEntryType::SnapshotMarker,
            version: WAL_FORMAT_VERSION,
            tx_id: TxId::nil(),
            payload,
        }
    }

    /// Check if this is a control entry (commit, abort, snapshot)
    pub fn is_control(&self) -> bool {
        self.entry_type.is_control()
    }

    /// Check if this is a transaction boundary (commit or abort)
    pub fn is_transaction_boundary(&self) -> bool {
        self.entry_type.is_transaction_boundary()
    }

    /// Serialize entry to bytes with envelope and CRC32
    ///
    /// Format:
    /// ```text
    /// [length: u32][type: u8][version: u8][tx_id: 16][payload: N][crc32: u32]
    /// ```
    ///
    /// CRC32 is computed over [type][version][tx_id][payload].
    pub fn serialize(&self) -> Result<Vec<u8>, WalEntryError> {
        // Build content (everything that CRC covers)
        let mut content = Vec::with_capacity(1 + 1 + 16 + self.payload.len());
        content.push(self.entry_type as u8);
        content.push(self.version);
        content.extend_from_slice(&self.tx_id.to_bytes());
        content.extend_from_slice(&self.payload);

        // Compute CRC32
        let mut hasher = Hasher::new();
        hasher.update(&content);
        let crc = hasher.finalize();

        // Total length: content + crc(4)
        let total_len = content.len() + 4;

        // Check size limit
        if total_len > MAX_WAL_ENTRY_SIZE {
            return Err(WalEntryError::TooLarge {
                size: total_len,
                max: MAX_WAL_ENTRY_SIZE,
            });
        }

        // Build final buffer: [length][content][crc]
        let mut buf = Vec::with_capacity(4 + total_len);
        buf.write_all(&(total_len as u32).to_le_bytes())
            .map_err(|e| WalEntryError::Serialization(e.to_string()))?;
        buf.write_all(&content)
            .map_err(|e| WalEntryError::Serialization(e.to_string()))?;
        buf.write_all(&crc.to_le_bytes())
            .map_err(|e| WalEntryError::Serialization(e.to_string()))?;

        Ok(buf)
    }

    /// Deserialize entry from bytes with CRC32 validation
    ///
    /// # Arguments
    ///
    /// * `data` - Buffer containing the serialized entry
    /// * `offset` - File offset for error reporting
    ///
    /// # Returns
    ///
    /// * `Ok((WalEntry, usize))` - Decoded entry and bytes consumed
    /// * `Err(WalEntryError)` - If validation or parsing fails
    pub fn deserialize(data: &[u8], offset: u64) -> Result<(Self, usize), WalEntryError> {
        // Need at least 4 bytes for length
        if data.len() < 4 {
            return Err(WalEntryError::TooShort {
                expected: 4,
                actual: data.len(),
            });
        }

        // Read length
        let mut len_bytes = [0u8; 4];
        len_bytes.copy_from_slice(&data[0..4]);
        let total_len = u32::from_le_bytes(len_bytes) as usize;

        // Validate minimum length
        if total_len < MIN_WAL_ENTRY_SIZE {
            return Err(WalEntryError::TooShort {
                expected: MIN_WAL_ENTRY_SIZE,
                actual: total_len,
            });
        }

        // Validate maximum length
        if total_len > MAX_WAL_ENTRY_SIZE {
            return Err(WalEntryError::TooLarge {
                size: total_len,
                max: MAX_WAL_ENTRY_SIZE,
            });
        }

        // Check buffer has enough data
        let total_bytes = 4 + total_len;
        if data.len() < total_bytes {
            return Err(WalEntryError::TooShort {
                expected: total_bytes,
                actual: data.len(),
            });
        }

        // Extract content and CRC
        let content_end = 4 + total_len - 4; // Everything before CRC
        let content = &data[4..content_end];

        let mut crc_bytes = [0u8; 4];
        crc_bytes.copy_from_slice(&data[content_end..total_bytes]);
        let expected_crc = u32::from_le_bytes(crc_bytes);

        // Validate CRC
        let mut hasher = Hasher::new();
        hasher.update(content);
        let actual_crc = hasher.finalize();

        if expected_crc != actual_crc {
            return Err(WalEntryError::ChecksumMismatch {
                offset,
                expected: expected_crc,
                actual: actual_crc,
            });
        }

        // Parse content
        if content.len() < 18 {
            // type(1) + version(1) + tx_id(16)
            return Err(WalEntryError::Deserialization {
                offset,
                message: format!("Content too short: {} bytes", content.len()),
            });
        }

        let entry_type = WalEntryType::try_from(content[0])?;
        let version = content[1];

        let mut tx_id_bytes = [0u8; 16];
        tx_id_bytes.copy_from_slice(&content[2..18]);
        let tx_id = TxId::from_bytes(tx_id_bytes);

        let payload = content[18..].to_vec();

        Ok((
            WalEntry {
                entry_type,
                version,
                tx_id,
                payload,
            },
            total_bytes,
        ))
    }

    /// Get the serialized size of this entry
    pub fn serialized_size(&self) -> usize {
        // length(4) + type(1) + version(1) + tx_id(16) + payload + crc(4)
        4 + 1 + 1 + 16 + self.payload.len() + 4
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tx_id_new() {
        let tx1 = TxId::new();
        let tx2 = TxId::new();

        // New TxIds should be unique
        assert_ne!(tx1, tx2);

        // New TxIds should not be nil
        assert!(!tx1.is_nil());
        assert!(!tx2.is_nil());
    }

    #[test]
    fn test_tx_id_nil() {
        let tx = TxId::nil();
        assert!(tx.is_nil());
    }

    #[test]
    fn test_tx_id_roundtrip() {
        let tx = TxId::new();
        let bytes = tx.to_bytes();
        let recovered = TxId::from_bytes(bytes);
        assert_eq!(tx, recovered);
    }

    #[test]
    fn test_wal_entry_serialize_roundtrip() {
        let tx_id = TxId::new();
        let entry = WalEntry::new(WalEntryType::KvPut, tx_id, b"test payload".to_vec());

        let serialized = entry.serialize().unwrap();
        let (deserialized, consumed) = WalEntry::deserialize(&serialized, 0).unwrap();

        assert_eq!(entry, deserialized);
        assert_eq!(consumed, serialized.len());
    }

    #[test]
    fn test_wal_entry_crc_validation() {
        let tx_id = TxId::new();
        let entry = WalEntry::new(WalEntryType::KvPut, tx_id, b"test payload".to_vec());

        let mut serialized = entry.serialize().unwrap();

        // Corrupt a byte in the payload
        let corrupt_idx = serialized.len() / 2;
        serialized[corrupt_idx] ^= 0xFF;

        // Deserialization should fail with CRC mismatch
        let result = WalEntry::deserialize(&serialized, 100);
        assert!(matches!(
            result,
            Err(WalEntryError::ChecksumMismatch { offset: 100, .. })
        ));
    }

    #[test]
    fn test_wal_entry_commit_marker() {
        let tx_id = TxId::new();
        let entry = WalEntry::commit_marker(tx_id);

        assert_eq!(entry.entry_type, WalEntryType::TransactionCommit);
        assert_eq!(entry.tx_id, tx_id);
        assert!(entry.payload.is_empty());
        assert!(entry.is_transaction_boundary());
        assert!(entry.is_control());

        // Should serialize and deserialize correctly
        let serialized = entry.serialize().unwrap();
        let (deserialized, _) = WalEntry::deserialize(&serialized, 0).unwrap();
        assert_eq!(entry, deserialized);
    }

    #[test]
    fn test_wal_entry_abort_marker() {
        let tx_id = TxId::new();
        let entry = WalEntry::abort_marker(tx_id);

        assert_eq!(entry.entry_type, WalEntryType::TransactionAbort);
        assert_eq!(entry.tx_id, tx_id);
        assert!(entry.payload.is_empty());
        assert!(entry.is_transaction_boundary());
        assert!(entry.is_control());
    }

    #[test]
    fn test_wal_entry_snapshot_marker() {
        let snapshot_id = Uuid::new_v4();
        let wal_offset = 12345u64;
        let entry = WalEntry::snapshot_marker(snapshot_id, wal_offset);

        assert_eq!(entry.entry_type, WalEntryType::SnapshotMarker);
        assert!(entry.tx_id.is_nil());
        assert_eq!(entry.payload.len(), 24); // 16 bytes UUID + 8 bytes offset
        assert!(!entry.is_transaction_boundary());
        assert!(entry.is_control());

        // Verify payload contents
        let mut uuid_bytes = [0u8; 16];
        uuid_bytes.copy_from_slice(&entry.payload[0..16]);
        assert_eq!(Uuid::from_bytes(uuid_bytes), snapshot_id);

        let mut offset_bytes = [0u8; 8];
        offset_bytes.copy_from_slice(&entry.payload[16..24]);
        assert_eq!(u64::from_le_bytes(offset_bytes), wal_offset);
    }

    #[test]
    fn test_wal_entry_too_short() {
        let short_data = [0u8; 3]; // Less than 4 bytes for length
        let result = WalEntry::deserialize(&short_data, 0);
        assert!(matches!(result, Err(WalEntryError::TooShort { .. })));
    }

    #[test]
    fn test_wal_entry_invalid_length() {
        // Length field says 5 (minimum) but we only provide partial data
        let mut data = vec![0u8; 10];
        data[0..4].copy_from_slice(&5u32.to_le_bytes()); // Length = 5

        let result = WalEntry::deserialize(&data, 0);
        // This should fail because 4 + 5 = 9 bytes needed, but content validation will fail
        assert!(result.is_err());
    }

    #[test]
    fn test_wal_entry_all_entry_types() {
        let tx_id = TxId::new();

        // Test each entry type
        let entry_types = [
            WalEntryType::TransactionCommit,
            WalEntryType::TransactionAbort,
            WalEntryType::SnapshotMarker,
            WalEntryType::KvPut,
            WalEntryType::KvDelete,
            WalEntryType::JsonCreate,
            WalEntryType::JsonSet,
            WalEntryType::JsonDelete,
            WalEntryType::JsonPatch,
            WalEntryType::EventAppend,
            WalEntryType::StateInit,
            WalEntryType::StateSet,
            WalEntryType::StateTransition,
            WalEntryType::TraceRecord,
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
            let entry = WalEntry::new(entry_type, tx_id, vec![1, 2, 3, 4]);
            let serialized = entry.serialize().unwrap();
            let (deserialized, consumed) = WalEntry::deserialize(&serialized, 0).unwrap();

            assert_eq!(entry, deserialized, "Failed for {:?}", entry_type);
            assert_eq!(consumed, serialized.len());
        }
    }

    #[test]
    fn test_wal_entry_empty_payload() {
        let tx_id = TxId::new();
        let entry = WalEntry::new(WalEntryType::KvDelete, tx_id, vec![]);

        let serialized = entry.serialize().unwrap();
        let (deserialized, _) = WalEntry::deserialize(&serialized, 0).unwrap();

        assert_eq!(entry, deserialized);
        assert!(deserialized.payload.is_empty());
    }

    #[test]
    fn test_wal_entry_large_payload() {
        let tx_id = TxId::new();
        let large_payload = vec![0xAB; 1024 * 1024]; // 1 MB payload
        let entry = WalEntry::new(WalEntryType::KvPut, tx_id, large_payload.clone());

        let serialized = entry.serialize().unwrap();
        let (deserialized, _) = WalEntry::deserialize(&serialized, 0).unwrap();

        assert_eq!(deserialized.payload, large_payload);
    }

    #[test]
    fn test_wal_entry_serialized_size() {
        let tx_id = TxId::new();
        let payload = vec![1, 2, 3, 4, 5];
        let entry = WalEntry::new(WalEntryType::KvPut, tx_id, payload);

        let expected_size = 4 + 1 + 1 + 16 + 5 + 4; // length + type + version + txid + payload + crc
        assert_eq!(entry.serialized_size(), expected_size);

        let serialized = entry.serialize().unwrap();
        assert_eq!(serialized.len(), expected_size);
    }

    #[test]
    fn test_wal_entry_multiple_in_buffer() {
        let tx_id = TxId::new();
        let entry1 = WalEntry::new(WalEntryType::KvPut, tx_id, b"key1=value1".to_vec());
        let entry2 = WalEntry::new(WalEntryType::KvPut, tx_id, b"key2=value2".to_vec());
        let entry3 = WalEntry::commit_marker(tx_id);

        // Concatenate entries
        let mut combined = Vec::new();
        combined.extend_from_slice(&entry1.serialize().unwrap());
        combined.extend_from_slice(&entry2.serialize().unwrap());
        combined.extend_from_slice(&entry3.serialize().unwrap());

        // Deserialize entries in sequence
        let mut offset = 0;
        let mut file_offset = 0u64;

        let (d1, consumed1) = WalEntry::deserialize(&combined[offset..], file_offset).unwrap();
        assert_eq!(d1, entry1);
        offset += consumed1;
        file_offset += consumed1 as u64;

        let (d2, consumed2) = WalEntry::deserialize(&combined[offset..], file_offset).unwrap();
        assert_eq!(d2, entry2);
        offset += consumed2;
        file_offset += consumed2 as u64;

        let (d3, consumed3) = WalEntry::deserialize(&combined[offset..], file_offset).unwrap();
        assert_eq!(d3, entry3);
        offset += consumed3;

        assert_eq!(offset, combined.len());
    }

    #[test]
    fn test_wal_entry_version_field() {
        let tx_id = TxId::new();
        let entry = WalEntry::new(WalEntryType::KvPut, tx_id, vec![]);

        assert_eq!(entry.version, WAL_FORMAT_VERSION);

        let serialized = entry.serialize().unwrap();
        let (deserialized, _) = WalEntry::deserialize(&serialized, 0).unwrap();

        assert_eq!(deserialized.version, WAL_FORMAT_VERSION);
    }

    #[test]
    fn test_tx_id_display() {
        let tx_id = TxId::new();
        let display = format!("{}", tx_id);
        assert!(!display.is_empty());
        // Should be a valid UUID string format
        assert_eq!(display.len(), 36); // UUID string format: 8-4-4-4-12
    }

    #[test]
    fn test_tx_id_from_uuid() {
        let uuid = Uuid::new_v4();
        let tx_id: TxId = uuid.into();
        let back: Uuid = tx_id.into();
        assert_eq!(uuid, back);
    }
}
