//! WAL entry encoding and decoding
//!
//! This module provides encoding/decoding for WAL entries with CRC32 checksums
//! for corruption detection.
//!
//! ## Entry Format
//!
//! ```text
//! [length: u32][type: u8][payload: bytes][crc32: u32]
//! ```
//!
//! - **length**: Total size of type + payload + crc (NOT including length itself)
//! - **type**: Entry type tag (1=BeginTxn, 2=Write, 3=Delete, 4=CommitTxn, 5=AbortTxn, 6=Checkpoint)
//! - **payload**: bincode-serialized WALEntry
//! - **crc32**: CRC32 checksum over \[type\]\[payload\]
//!
//! ## Why This Format
//!
//! - Length enables reading variable-sized entries
//! - Type tag enables forward compatibility (skip unknown types)
//! - CRC32 detects corruption (bit flips, partial writes)
//! - bincode serialization: fast, deterministic, compact

use crate::wal::WALEntry;
use crc32fast::Hasher;
use strata_core::error::Result;
use strata_core::StrataError;
use std::io::{Cursor, Read, Write};

/// Entry type tags for forward compatibility
const TYPE_BEGIN_TXN: u8 = 1;
const TYPE_WRITE: u8 = 2;
const TYPE_DELETE: u8 = 3;
const TYPE_COMMIT_TXN: u8 = 4;
const TYPE_ABORT_TXN: u8 = 5;
const TYPE_CHECKPOINT: u8 = 6;

// JSON entry type tags (0x20 range) - M5
/// JSON document creation entry
pub const TYPE_JSON_CREATE: u8 = 0x20;
/// JSON set value at path entry
pub const TYPE_JSON_SET: u8 = 0x21;
/// JSON delete value at path entry
pub const TYPE_JSON_DELETE: u8 = 0x22;
/// JSON destroy (delete entire document) entry
pub const TYPE_JSON_DESTROY: u8 = 0x23;

// Vector entry type tags (0x70 range) - M8
/// Vector collection creation entry
pub const TYPE_VECTOR_COLLECTION_CREATE: u8 = 0x70;
/// Vector collection deletion entry
pub const TYPE_VECTOR_COLLECTION_DELETE: u8 = 0x71;
/// Vector upsert (insert or update) entry
pub const TYPE_VECTOR_UPSERT: u8 = 0x72;
/// Vector delete entry
pub const TYPE_VECTOR_DELETE: u8 = 0x73;

/// Encode WAL entry to bytes
///
/// Format: `[length: u32][type: u8][payload: bytes][crc32: u32]`
///
/// Returns byte buffer ready for file I/O.
///
/// # Arguments
///
/// * `entry` - The WAL entry to encode
///
/// # Returns
///
/// * `Ok(Vec<u8>)` - Encoded bytes ready for writing
/// * `Err` - If serialization fails
///
/// # Example
///
/// ```ignore
/// use strata_durability::encoding::encode_entry;
/// use strata_durability::wal::WALEntry;
///
/// let entry = WALEntry::CommitTxn { txn_id: 1, run_id };
/// let bytes = encode_entry(&entry)?;
/// // Write bytes to file
/// ```
pub fn encode_entry(entry: &WALEntry) -> Result<Vec<u8>> {
    // Determine type tag
    let type_tag = match entry {
        WALEntry::BeginTxn { .. } => TYPE_BEGIN_TXN,
        WALEntry::Write { .. } => TYPE_WRITE,
        WALEntry::Delete { .. } => TYPE_DELETE,
        WALEntry::CommitTxn { .. } => TYPE_COMMIT_TXN,
        WALEntry::AbortTxn { .. } => TYPE_ABORT_TXN,
        WALEntry::Checkpoint { .. } => TYPE_CHECKPOINT,
        // JSON operations
        WALEntry::JsonCreate { .. } => TYPE_JSON_CREATE,
        WALEntry::JsonSet { .. } => TYPE_JSON_SET,
        WALEntry::JsonDelete { .. } => TYPE_JSON_DELETE,
        WALEntry::JsonDestroy { .. } => TYPE_JSON_DESTROY,
        // Vector operations
        WALEntry::VectorCollectionCreate { .. } => TYPE_VECTOR_COLLECTION_CREATE,
        WALEntry::VectorCollectionDelete { .. } => TYPE_VECTOR_COLLECTION_DELETE,
        WALEntry::VectorUpsert { .. } => TYPE_VECTOR_UPSERT,
        WALEntry::VectorDelete { .. } => TYPE_VECTOR_DELETE,
    };

    // Serialize payload with bincode
    let payload = bincode::serialize(entry)?;

    // Calculate total length: type(1) + payload + crc(4)
    let total_len = 1 + payload.len() + 4;

    // Build buffer: [length][type][payload][crc]
    let mut buf = Vec::with_capacity(4 + total_len);

    // Write length
    buf.write_all(&(total_len as u32).to_le_bytes())
        .map_err(|e| StrataError::storage(format!("Failed to write length: {}", e)))?;

    // Write type tag
    buf.write_all(&[type_tag])
        .map_err(|e| StrataError::storage(format!("Failed to write type: {}", e)))?;

    // Write payload
    buf.write_all(&payload)
        .map_err(|e| StrataError::storage(format!("Failed to write payload: {}", e)))?;

    // Calculate CRC over [type][payload]
    let mut hasher = Hasher::new();
    hasher.update(&[type_tag]);
    hasher.update(&payload);
    let crc = hasher.finalize();

    // Write CRC
    buf.write_all(&crc.to_le_bytes())
        .map_err(|e| StrataError::storage(format!("Failed to write CRC: {}", e)))?;

    Ok(buf)
}

/// Decode WAL entry from bytes with CRC validation
///
/// Format: `[length: u32][type: u8][payload: bytes][crc32: u32]`
///
/// Returns the decoded entry and the number of bytes consumed.
///
/// # Arguments
///
/// * `buf` - Buffer containing encoded entry
/// * `offset` - File offset for error reporting (helps with debugging)
///
/// # Returns
///
/// * `Ok((WALEntry, usize))` - Decoded entry and bytes consumed
/// * `Err(StrataError::corruption)` - If CRC mismatch or truncated data
///
/// # Errors
///
/// Returns `StrataError::corruption` with offset information when:
/// - Buffer is too short to read length
/// - Buffer is too short for declared entry size
/// - CRC32 checksum doesn't match (data corruption)
/// - Type tag doesn't match deserialized entry type
/// - Deserialization fails
///
/// # Example
///
/// ```ignore
/// use strata_durability::encoding::decode_entry;
///
/// let bytes = read_from_file();
/// let (entry, consumed) = decode_entry(&bytes, file_offset)?;
/// // Process entry...
/// ```
pub fn decode_entry(buf: &[u8], offset: u64) -> Result<(WALEntry, usize)> {
    let mut cursor = Cursor::new(buf);

    // Read length
    let mut len_buf = [0u8; 4];
    cursor.read_exact(&mut len_buf).map_err(|_| {
        // Buffer too short to read length - incomplete entry, not corruption
        StrataError::storage(format!(
            "Incomplete entry at offset {}: need {} bytes, have {}",
            offset, 4, buf.len()
        ))
    })?;
    let total_len = u32::from_le_bytes(len_buf) as usize;

    // Validate minimum length before arithmetic (prevent underflow)
    // Minimum valid entry: type(1) + crc(4) = 5 bytes
    if total_len < 5 {
        return Err(StrataError::corruption(format!(
            "offset {}: Invalid entry length {} (minimum is 5 bytes: type(1) + crc(4))",
            offset, total_len
        )));
    }

    // Check buffer has enough bytes - this is incomplete data, not corruption
    if buf.len() < 4 + total_len {
        return Err(StrataError::storage(format!(
            "Incomplete entry at offset {}: need {} bytes, have {}",
            offset, 4 + total_len, buf.len()
        )));
    }

    // Read type tag
    let mut type_buf = [0u8; 1];
    cursor
        .read_exact(&mut type_buf)
        .map_err(|_| StrataError::corruption(format!("offset {}: Failed to read type tag", offset)))?;
    let type_tag = type_buf[0];

    // Read payload (total_len - type(1) - crc(4))
    let payload_len = total_len - 1 - 4;
    let mut payload = vec![0u8; payload_len];
    cursor
        .read_exact(&mut payload)
        .map_err(|_| StrataError::corruption(format!("offset {}: Failed to read payload", offset)))?;

    // Read CRC
    let mut crc_buf = [0u8; 4];
    cursor
        .read_exact(&mut crc_buf)
        .map_err(|_| StrataError::corruption(format!("offset {}: Failed to read CRC", offset)))?;
    let expected_crc = u32::from_le_bytes(crc_buf);

    // Verify CRC
    let mut hasher = Hasher::new();
    hasher.update(&[type_tag]);
    hasher.update(&payload);
    let actual_crc = hasher.finalize();

    if actual_crc != expected_crc {
        return Err(StrataError::corruption(format!(
            "offset {}: CRC mismatch: expected {:08x}, got {:08x}",
            offset, expected_crc, actual_crc
        )));
    }

    // Deserialize payload
    let entry: WALEntry = bincode::deserialize(&payload).map_err(|e| {
        StrataError::corruption(format!("offset {}: Deserialization failed: {}", offset, e))
    })?;

    // Verify type tag matches entry type
    let expected_type = match &entry {
        WALEntry::BeginTxn { .. } => TYPE_BEGIN_TXN,
        WALEntry::Write { .. } => TYPE_WRITE,
        WALEntry::Delete { .. } => TYPE_DELETE,
        WALEntry::CommitTxn { .. } => TYPE_COMMIT_TXN,
        WALEntry::AbortTxn { .. } => TYPE_ABORT_TXN,
        WALEntry::Checkpoint { .. } => TYPE_CHECKPOINT,
        // JSON operations
        WALEntry::JsonCreate { .. } => TYPE_JSON_CREATE,
        WALEntry::JsonSet { .. } => TYPE_JSON_SET,
        WALEntry::JsonDelete { .. } => TYPE_JSON_DELETE,
        WALEntry::JsonDestroy { .. } => TYPE_JSON_DESTROY,
        // Vector operations
        WALEntry::VectorCollectionCreate { .. } => TYPE_VECTOR_COLLECTION_CREATE,
        WALEntry::VectorCollectionDelete { .. } => TYPE_VECTOR_COLLECTION_DELETE,
        WALEntry::VectorUpsert { .. } => TYPE_VECTOR_UPSERT,
        WALEntry::VectorDelete { .. } => TYPE_VECTOR_DELETE,
    };

    if type_tag != expected_type {
        return Err(StrataError::corruption(format!(
            "offset {}: Type tag mismatch: expected {}, got {}",
            offset, expected_type, type_tag
        )));
    }

    Ok((entry, 4 + total_len))
}

#[cfg(test)]
mod tests {
    use super::*;
    use strata_core::types::{Key, Namespace, RunId};
    use strata_core::value::Value;
    use strata_core::Timestamp;
    use uuid::Uuid;

    /// Helper to get current timestamp
    fn now() -> Timestamp {
        Timestamp::now()
    }

    #[test]
    fn test_encode_decode_roundtrip() {
        let run_id = RunId::new();
        let entry = WALEntry::BeginTxn {
            txn_id: 42,
            run_id,
            timestamp: now(),
        };

        let encoded = encode_entry(&entry).unwrap();
        let (decoded, bytes_consumed) = decode_entry(&encoded, 0).unwrap();

        assert_eq!(entry, decoded);
        assert_eq!(bytes_consumed, encoded.len());
    }

    #[test]
    fn test_encode_all_entry_types() {
        let run_id = RunId::new();
        let ns = Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        );

        let entries = vec![
            WALEntry::BeginTxn {
                txn_id: 1,
                run_id,
                timestamp: now(),
            },
            WALEntry::Write {
                run_id,
                key: Key::new_kv(ns.clone(), "key"),
                value: Value::Bytes(b"value".to_vec()),
                version: 10,
            },
            WALEntry::Delete {
                run_id,
                key: Key::new_kv(ns.clone(), "key"),
                version: 11,
            },
            WALEntry::CommitTxn { txn_id: 1, run_id },
            WALEntry::AbortTxn { txn_id: 2, run_id },
            WALEntry::Checkpoint {
                snapshot_id: Uuid::new_v4(),
                version: 100,
                active_runs: vec![run_id],
            },
        ];

        for entry in entries {
            let encoded = encode_entry(&entry).unwrap();
            let (decoded, _) = decode_entry(&encoded, 0).unwrap();
            assert_eq!(entry, decoded);
        }
    }

    #[test]
    fn test_crc_detects_corruption() {
        let run_id = RunId::new();
        let entry = WALEntry::BeginTxn {
            txn_id: 42,
            run_id,
            timestamp: now(),
        };

        let mut encoded = encode_entry(&entry).unwrap();

        // Corrupt payload (flip a bit in the middle of the buffer)
        let corrupt_idx = encoded.len() / 2;
        encoded[corrupt_idx] ^= 0xFF;

        // Decode should fail with CRC error
        let result = decode_entry(&encoded, 100);
        assert!(result.is_err());

        match result {
            Err(StrataError::Corruption { message }) => {
                assert!(message.contains("CRC mismatch"), "Expected CRC mismatch error");
                assert!(message.contains("100"), "Error should include offset");
            }
            _ => panic!("Expected Corruption error with CRC mismatch"),
        }
    }

    #[test]
    fn test_truncated_entry() {
        let run_id = RunId::new();
        let entry = WALEntry::BeginTxn {
            txn_id: 42,
            run_id,
            timestamp: now(),
        };

        let encoded = encode_entry(&entry).unwrap();

        // Truncate buffer (remove last 10 bytes)
        let truncated = &encoded[..encoded.len() - 10];

        // Decode should fail with incomplete entry error
        let result = decode_entry(truncated, 200);
        assert!(result.is_err());

        match result {
            Err(StrataError::Storage { message, .. }) => {
                assert!(message.contains("Incomplete entry"), "Should indicate incomplete entry");
                assert!(message.contains("offset 200"), "Should include offset 200");
            }
            _ => panic!("Expected Storage error with incomplete entry message, got {:?}", result),
        }
    }

    #[test]
    fn test_entry_format() {
        let run_id = RunId::new();
        let entry = WALEntry::CommitTxn { txn_id: 42, run_id };

        let encoded = encode_entry(&entry).unwrap();

        // Verify format: [length: 4][type: 1][payload: N][crc: 4]
        assert!(encoded.len() >= 4 + 1 + 4); // Minimum size

        // Read length
        let len_bytes = &encoded[0..4];
        let total_len =
            u32::from_le_bytes([len_bytes[0], len_bytes[1], len_bytes[2], len_bytes[3]]);

        // Verify encoded size matches
        assert_eq!(encoded.len(), 4 + total_len as usize);

        // Verify type tag
        let type_tag = encoded[4];
        assert_eq!(type_tag, TYPE_COMMIT_TXN);
    }

    #[test]
    fn test_offset_included_in_errors() {
        // Test that offset is properly included in error messages for debugging
        let short_buf = [0u8; 2]; // Too short to read length

        let result = decode_entry(&short_buf, 12345);
        assert!(result.is_err());

        match result {
            Err(StrataError::Storage { message, .. }) => {
                assert!(message.contains("Incomplete entry"), "Should indicate incomplete entry");
                assert!(message.contains("offset 12345"), "Should include offset 12345");
                assert!(message.contains("need 4"), "Should indicate needed bytes");
                assert!(message.contains("have 2"), "Should indicate buffer size");
            }
            _ => panic!("Expected Storage error with incomplete entry message, got {:?}", result),
        }
    }

    #[test]
    fn test_multiple_entries_in_buffer() {
        // Encode multiple entries into a single buffer
        let run_id = RunId::new();
        let entry1 = WALEntry::BeginTxn {
            txn_id: 1,
            run_id,
            timestamp: now(),
        };
        let entry2 = WALEntry::CommitTxn { txn_id: 1, run_id };

        let encoded1 = encode_entry(&entry1).unwrap();
        let encoded2 = encode_entry(&entry2).unwrap();

        // Concatenate entries
        let mut combined = encoded1.clone();
        combined.extend_from_slice(&encoded2);

        // Decode first entry
        let (decoded1, consumed1) = decode_entry(&combined, 0).unwrap();
        assert_eq!(entry1, decoded1);
        assert_eq!(consumed1, encoded1.len());

        // Decode second entry from remaining buffer
        let (decoded2, consumed2) = decode_entry(&combined[consumed1..], consumed1 as u64).unwrap();
        assert_eq!(entry2, decoded2);
        assert_eq!(consumed2, encoded2.len());
    }

    #[test]
    fn test_zero_length_entry_causes_corruption_error() {
        // Regression test for issue #51: decoder panic on zero-length entry
        //
        // This can happen when:
        // - Filesystem bugs cause trailing zeros to be appended
        // - Pre-allocation fills unused space with zeros
        // - Disk corruption zeros out data
        //
        // The decoder should return StrataError::corruption instead of panicking
        // with integer underflow when total_len < 5.

        // Create buffer with zero length field
        let mut buf = vec![0u8; 8];
        buf[0..4].copy_from_slice(&0u32.to_le_bytes()); // length = 0

        // This should return CorruptionError, NOT panic
        let result = decode_entry(&buf, 0);

        assert!(
            result.is_err(),
            "Zero-length entry should be rejected as corruption"
        );

        match result {
            Err(StrataError::Corruption { message }) => {
                assert!(
                    message.contains("Invalid entry length 0"),
                    "Error message should mention invalid length: {}",
                    message
                );
                assert!(
                    message.contains("minimum is 5"),
                    "Error message should mention minimum size: {}",
                    message
                );
            }
            _ => panic!("Expected Corruption error, got: {:?}", result),
        }
    }

    #[test]
    fn test_length_less_than_minimum_causes_corruption_error() {
        // Test all invalid lengths from 1-4 (minimum valid is 5)
        for invalid_len in 1..5 {
            let mut buf = vec![0u8; 8];
            buf[0..4].copy_from_slice(&(invalid_len as u32).to_le_bytes());

            let result = decode_entry(&buf, 0);

            assert!(
                result.is_err(),
                "Length {} should be rejected (minimum is 5)",
                invalid_len
            );

            match result {
                Err(StrataError::Corruption { message }) => {
                    assert!(
                        message.contains(&format!("Invalid entry length {}", invalid_len)),
                        "Error should mention length {}: {}",
                        invalid_len,
                        message
                    );
                }
                _ => panic!(
                    "Expected Corruption error for length {}, got: {:?}",
                    invalid_len, result
                ),
            }
        }
    }

    // ========================================================================
    // JSON Entry Encoding Tests
    // ========================================================================

    use strata_core::primitives::json::JsonPath;

    #[test]
    fn test_json_create_encode_decode() {
        let run_id = RunId::new();
        let doc_id = "test-doc";

        let entry = WALEntry::JsonCreate {
            run_id,
            doc_id: doc_id.to_string(),
            value_bytes: vec![0x80], // msgpack empty map
            version: 1,
            timestamp: now(),
        };

        let encoded = encode_entry(&entry).unwrap();
        let (decoded, consumed) = decode_entry(&encoded, 0).unwrap();

        assert_eq!(entry, decoded);
        assert_eq!(consumed, encoded.len());

        // Verify type tag is 0x20
        assert_eq!(encoded[4], TYPE_JSON_CREATE);
    }

    #[test]
    fn test_json_set_encode_decode() {
        let run_id = RunId::new();
        let doc_id = "test-doc";

        let entry = WALEntry::JsonSet {
            run_id,
            doc_id: doc_id.to_string(),
            path: "user.name".parse::<JsonPath>().unwrap(),
            value_bytes: b"\xa5Alice".to_vec(),
            version: 2,
        };

        let encoded = encode_entry(&entry).unwrap();
        let (decoded, consumed) = decode_entry(&encoded, 0).unwrap();

        assert_eq!(entry, decoded);
        assert_eq!(consumed, encoded.len());

        // Verify type tag is 0x21
        assert_eq!(encoded[4], TYPE_JSON_SET);
    }

    #[test]
    fn test_json_delete_encode_decode() {
        let run_id = RunId::new();
        let doc_id = "test-doc";

        let entry = WALEntry::JsonDelete {
            run_id,
            doc_id: doc_id.to_string(),
            path: "temp.field".parse::<JsonPath>().unwrap(),
            version: 3,
        };

        let encoded = encode_entry(&entry).unwrap();
        let (decoded, consumed) = decode_entry(&encoded, 0).unwrap();

        assert_eq!(entry, decoded);
        assert_eq!(consumed, encoded.len());

        // Verify type tag is 0x22
        assert_eq!(encoded[4], TYPE_JSON_DELETE);
    }

    #[test]
    fn test_json_destroy_encode_decode() {
        let run_id = RunId::new();
        let doc_id = "test-doc";

        let entry = WALEntry::JsonDestroy { run_id, doc_id: doc_id.to_string() };

        let encoded = encode_entry(&entry).unwrap();
        let (decoded, consumed) = decode_entry(&encoded, 0).unwrap();

        assert_eq!(entry, decoded);
        assert_eq!(consumed, encoded.len());

        // Verify type tag is 0x23
        assert_eq!(encoded[4], TYPE_JSON_DESTROY);
    }

    #[test]
    fn test_json_entries_in_sequence() {
        // Test that multiple JSON entries can be decoded in sequence
        let run_id = RunId::new();
        let doc_id = "test-doc";

        let entries = vec![
            WALEntry::JsonCreate {
                run_id,
                doc_id: doc_id.to_string(),
                value_bytes: vec![0x80],
                version: 1,
                timestamp: now(),
            },
            WALEntry::JsonSet {
                run_id,
                doc_id: doc_id.to_string(),
                path: "name".parse().unwrap(),
                value_bytes: b"\xa4test".to_vec(),
                version: 2,
            },
            WALEntry::JsonDestroy { run_id, doc_id: doc_id.to_string() },
        ];

        // Encode all entries into a single buffer
        let mut combined = Vec::new();
        for entry in &entries {
            combined.extend_from_slice(&encode_entry(entry).unwrap());
        }

        // Decode entries in sequence
        let mut offset = 0;
        for (idx, expected) in entries.iter().enumerate() {
            let (decoded, consumed) = decode_entry(&combined[offset..], offset as u64).unwrap();
            assert_eq!(&decoded, expected, "Entry {} mismatch", idx);
            offset += consumed;
        }

        assert_eq!(offset, combined.len(), "Should consume entire buffer");
    }

    // ========================================================================
    // Vector Entry Encoding Tests
    // ========================================================================

    #[test]
    fn test_vector_collection_create_encode_decode() {
        let run_id = RunId::new();
        let entry = WALEntry::VectorCollectionCreate {
            run_id,
            collection: "embeddings".to_string(),
            dimension: 384,
            metric: 0,
            version: 1,
        };

        let encoded = encode_entry(&entry).unwrap();
        assert_eq!(encoded[4], TYPE_VECTOR_COLLECTION_CREATE);

        let (decoded, consumed) = decode_entry(&encoded, 0).unwrap();
        assert_eq!(entry, decoded);
        assert_eq!(consumed, encoded.len());
    }

    #[test]
    fn test_vector_upsert_encode_decode() {
        let run_id = RunId::new();
        let entry = WALEntry::VectorUpsert {
            run_id,
            collection: "col".to_string(),
            key: "k1".to_string(),
            vector_id: 42,
            embedding: vec![0.1, 0.2, 0.3],
            metadata: Some(vec![0x80]),
            version: 5,
            source_ref: None,
        };

        let encoded = encode_entry(&entry).unwrap();
        assert_eq!(encoded[4], TYPE_VECTOR_UPSERT);

        let (decoded, consumed) = decode_entry(&encoded, 0).unwrap();
        assert_eq!(entry, decoded);
        assert_eq!(consumed, encoded.len());
    }

    #[test]
    fn test_vector_delete_encode_decode() {
        let run_id = RunId::new();
        let entry = WALEntry::VectorDelete {
            run_id,
            collection: "col".to_string(),
            key: "k1".to_string(),
            vector_id: 42,
            version: 6,
        };

        let encoded = encode_entry(&entry).unwrap();
        assert_eq!(encoded[4], TYPE_VECTOR_DELETE);

        let (decoded, consumed) = decode_entry(&encoded, 0).unwrap();
        assert_eq!(entry, decoded);
        assert_eq!(consumed, encoded.len());
    }

    #[test]
    fn test_vector_collection_delete_encode_decode() {
        let run_id = RunId::new();
        let entry = WALEntry::VectorCollectionDelete {
            run_id,
            collection: "old_col".to_string(),
            version: 99,
        };

        let encoded = encode_entry(&entry).unwrap();
        assert_eq!(encoded[4], TYPE_VECTOR_COLLECTION_DELETE);

        let (decoded, consumed) = decode_entry(&encoded, 0).unwrap();
        assert_eq!(entry, decoded);
        assert_eq!(consumed, encoded.len());
    }

    // ========================================================================
    // Adversarial Encoding Tests
    // ========================================================================

    #[test]
    fn test_crc_corruption_single_bit_in_type_tag() {
        let run_id = RunId::new();
        let entry = WALEntry::CommitTxn { txn_id: 1, run_id };

        let mut encoded = encode_entry(&entry).unwrap();
        // Flip a bit in the type tag byte (byte 4)
        encoded[4] ^= 0x01;

        let result = decode_entry(&encoded, 0);
        assert!(result.is_err(), "Single bit flip in type tag should cause CRC mismatch");
    }

    #[test]
    fn test_crc_corruption_last_payload_byte() {
        let run_id = RunId::new();
        let entry = WALEntry::BeginTxn {
            txn_id: 1,
            run_id,
            timestamp: now(),
        };

        let mut encoded = encode_entry(&entry).unwrap();
        // Corrupt the last byte before the CRC
        let crc_start = encoded.len() - 4;
        encoded[crc_start - 1] ^= 0xFF;

        let result = decode_entry(&encoded, 0);
        assert!(result.is_err());
        match result {
            Err(StrataError::Corruption { message }) => {
                assert!(message.contains("CRC mismatch"));
            }
            _ => panic!("Expected Corruption error"),
        }
    }

    #[test]
    fn test_forged_type_tag_with_valid_crc_still_detected() {
        // Forge a buffer where type tag doesn't match the deserialized entry type
        // but CRC is recomputed to be valid. The type tag cross-check should catch this.
        let run_id = RunId::new();
        let entry = WALEntry::CommitTxn { txn_id: 1, run_id };

        let encoded = encode_entry(&entry).unwrap();

        // Extract the payload
        let len_bytes = &encoded[0..4];
        let total_len = u32::from_le_bytes([len_bytes[0], len_bytes[1], len_bytes[2], len_bytes[3]]) as usize;
        let payload = &encoded[5..5 + total_len - 1 - 4];

        // Re-encode with wrong type tag but recomputed CRC
        let wrong_type: u8 = TYPE_BEGIN_TXN; // CommitTxn has type 4, use type 1 instead
        let mut forged = Vec::new();
        forged.extend_from_slice(&(total_len as u32).to_le_bytes());
        forged.push(wrong_type);
        forged.extend_from_slice(payload);

        // Recompute CRC with the wrong type tag
        let mut hasher = Hasher::new();
        hasher.update(&[wrong_type]);
        hasher.update(payload);
        let crc = hasher.finalize();
        forged.extend_from_slice(&crc.to_le_bytes());

        // Decode should fail because deserialized entry type won't match the forged type tag
        let result = decode_entry(&forged, 0);
        assert!(result.is_err(), "Forged type tag should be detected even with valid CRC");
    }

    #[test]
    fn test_decode_empty_buffer() {
        let result = decode_entry(&[], 0);
        assert!(result.is_err());
        // Should be Storage error (incomplete), not Corruption
        match result {
            Err(StrataError::Storage { message, .. }) => {
                assert!(message.contains("Incomplete entry"));
            }
            _ => panic!("Expected Storage/Incomplete error for empty buffer, got: {:?}", result),
        }
    }

    #[test]
    fn test_decode_buffer_too_short_for_declared_length() {
        // Declare a large length but provide a short buffer
        let mut buf = vec![0u8; 20];
        buf[0..4].copy_from_slice(&1000u32.to_le_bytes()); // length = 1000

        let result = decode_entry(&buf, 0);
        assert!(result.is_err());
        match result {
            Err(StrataError::Storage { message, .. }) => {
                assert!(message.contains("Incomplete entry"));
            }
            _ => panic!("Expected Storage/Incomplete error for short buffer"),
        }
    }

    #[test]
    fn test_all_type_tags_are_distinct() {
        // Verify no type tag collision between core, JSON, and vector entry types
        let all_tags = [
            TYPE_BEGIN_TXN,
            TYPE_WRITE,
            TYPE_DELETE,
            TYPE_COMMIT_TXN,
            TYPE_ABORT_TXN,
            TYPE_CHECKPOINT,
            TYPE_JSON_CREATE,
            TYPE_JSON_SET,
            TYPE_JSON_DELETE,
            TYPE_JSON_DESTROY,
            TYPE_VECTOR_COLLECTION_CREATE,
            TYPE_VECTOR_COLLECTION_DELETE,
            TYPE_VECTOR_UPSERT,
            TYPE_VECTOR_DELETE,
        ];

        let mut seen = std::collections::HashSet::new();
        for tag in all_tags {
            assert!(
                seen.insert(tag),
                "Duplicate type tag: 0x{:02x}",
                tag
            );
        }
    }

    #[test]
    fn test_type_tag_ranges() {
        // Core types: 1-6
        assert!(TYPE_BEGIN_TXN >= 1 && TYPE_BEGIN_TXN <= 6);
        assert!(TYPE_CHECKPOINT >= 1 && TYPE_CHECKPOINT <= 6);

        // JSON types: 0x20-0x23
        assert_eq!(TYPE_JSON_CREATE, 0x20);
        assert_eq!(TYPE_JSON_SET, 0x21);
        assert_eq!(TYPE_JSON_DELETE, 0x22);
        assert_eq!(TYPE_JSON_DESTROY, 0x23);

        // Vector types: 0x70-0x73
        assert_eq!(TYPE_VECTOR_COLLECTION_CREATE, 0x70);
        assert_eq!(TYPE_VECTOR_COLLECTION_DELETE, 0x71);
        assert_eq!(TYPE_VECTOR_UPSERT, 0x72);
        assert_eq!(TYPE_VECTOR_DELETE, 0x73);
    }

    #[test]
    fn test_large_entry_encode_decode() {
        let run_id = RunId::new();
        // Large embedding vector (1000 dimensions)
        let entry = WALEntry::VectorUpsert {
            run_id,
            collection: "large_col".to_string(),
            key: "large_vec".to_string(),
            vector_id: 1,
            embedding: vec![0.12345; 1000],
            metadata: Some(vec![0xAB; 10_000]), // 10KB metadata
            version: 1,
            source_ref: None,
        };

        let encoded = encode_entry(&entry).unwrap();
        assert!(encoded.len() > 10_000, "Large entry should be >10KB");

        let (decoded, consumed) = decode_entry(&encoded, 0).unwrap();
        assert_eq!(entry, decoded);
        assert_eq!(consumed, encoded.len());
    }
}
