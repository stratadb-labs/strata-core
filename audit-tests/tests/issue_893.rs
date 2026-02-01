//! Audit test for issue #893: WAL checksum valid but payload parse fails — misleading error message
//! Verdict: CONFIRMED BUG
//!
//! When a WAL record has a valid CRC but the payload fails to parse (e.g., payload
//! too short or unsupported format version), the error is a bare `InvalidFormat` with
//! no indication that the CRC was actually valid. This makes it indistinguishable from
//! random corruption in error handling code.

use strata_durability::format::wal_record::{WalRecord, WalRecordError, WAL_RECORD_FORMAT_VERSION};

/// Build a raw WAL record with a valid CRC but a payload that is too short to parse.
/// This simulates a scenario where a record was written by a different version of the
/// software that uses a shorter payload format, or where a codec produced valid but
/// shorter output.
fn build_crc_valid_but_unparseable_record() -> Vec<u8> {
    // Build a payload that:
    // - Has a valid format version byte
    // - But is too short (less than 33 bytes: 1 version + 8 txn_id + 16 branch_id + 8 timestamp)
    // We'll make a 10-byte payload: version(1) + 9 bytes of junk
    let mut payload = Vec::new();
    payload.push(WAL_RECORD_FORMAT_VERSION); // valid format version
    payload.extend_from_slice(&[0u8; 9]); // not enough for txn_id + branch_id + timestamp

    // Compute valid CRC of this payload
    let crc = crc32fast::hash(&payload);

    // Build the record: length(4) + payload + crc(4)
    let total_len = payload.len() + 4; // payload + crc
    let mut record = Vec::new();
    record.extend_from_slice(&(total_len as u32).to_le_bytes());
    record.extend_from_slice(&payload);
    record.extend_from_slice(&crc.to_le_bytes());

    record
}

/// Build a raw WAL record with a valid CRC but an unsupported format version.
fn build_crc_valid_but_wrong_version_record() -> Vec<u8> {
    // Build a full-size payload with wrong format version
    let mut payload = Vec::new();
    payload.push(99); // unsupported format version
    payload.extend_from_slice(&1u64.to_le_bytes()); // txn_id
    payload.extend_from_slice(&[0u8; 16]); // branch_id
    payload.extend_from_slice(&1000u64.to_le_bytes()); // timestamp
    payload.extend_from_slice(&[1, 2, 3]); // writeset

    // Compute valid CRC
    let crc = crc32fast::hash(&payload);

    let total_len = payload.len() + 4;
    let mut record = Vec::new();
    record.extend_from_slice(&(total_len as u32).to_le_bytes());
    record.extend_from_slice(&payload);
    record.extend_from_slice(&crc.to_le_bytes());

    record
}

#[test]
fn issue_893_crc_valid_but_payload_too_short_gives_opaque_error() {
    // A record with valid CRC but payload too short to parse should
    // ideally indicate that the CRC was valid (ruling out corruption).
    // Currently it returns a bare InvalidFormat with no context.
    let bytes = build_crc_valid_but_unparseable_record();
    let result = WalRecord::from_bytes(&bytes);

    // The CRC check passes, then payload parsing fails with InvalidFormat
    let err = result.unwrap_err();

    // BUG: The error is just `InvalidFormat` with no indication that CRC was valid.
    // A developer seeing this error would reasonably assume disk corruption,
    // when in fact the data is intact but the format is incompatible.
    assert!(
        matches!(err, WalRecordError::InvalidFormat),
        "Expected InvalidFormat for short payload, got: {:?}",
        err
    );

    // The error message contains no diagnostic information
    let msg = err.to_string();
    assert_eq!(
        msg, "Invalid record format",
        "Error message should be the bare 'Invalid record format' with no context"
    );

    // Verify this is NOT a ChecksumMismatch (CRC actually passed)
    assert!(
        !matches!(err, WalRecordError::ChecksumMismatch { .. }),
        "Should NOT be ChecksumMismatch since the CRC was valid"
    );
}

#[test]
fn issue_893_crc_valid_wrong_version_gives_useful_error() {
    // Contrast: wrong version DOES give useful context via UnsupportedVersion variant
    let bytes = build_crc_valid_but_wrong_version_record();
    let result = WalRecord::from_bytes(&bytes);

    let err = result.unwrap_err();

    // UnsupportedVersion includes the actual version number - this IS useful
    assert!(
        matches!(err, WalRecordError::UnsupportedVersion(99)),
        "Expected UnsupportedVersion(99), got: {:?}",
        err
    );
}

#[test]
fn issue_893_actual_corruption_gives_checksum_mismatch() {
    // For comparison: actual corruption gives a ChecksumMismatch error
    let record = WalRecord::new(1, [0u8; 16], 1000, vec![1, 2, 3]);
    let mut bytes = record.to_bytes();

    // Corrupt a byte in the payload area (after the length prefix)
    bytes[10] ^= 0xFF;

    let result = WalRecord::from_bytes(&bytes);
    let err = result.unwrap_err();

    // This correctly identifies as corruption
    assert!(
        matches!(err, WalRecordError::ChecksumMismatch { .. }),
        "Expected ChecksumMismatch for corrupted data, got: {:?}",
        err
    );
}

#[test]
fn issue_893_invalid_format_error_lacks_context_fields() {
    // Demonstrate that the InvalidFormat variant carries no diagnostic data.
    // Compare with ChecksumMismatch which carries both expected and computed values.
    let err_no_context = WalRecordError::InvalidFormat;
    let err_with_context = WalRecordError::ChecksumMismatch {
        expected: 0xDEADBEEF,
        computed: 0xCAFEBABE,
    };

    // InvalidFormat: bare string, no fields
    assert_eq!(err_no_context.to_string(), "Invalid record format");

    // ChecksumMismatch: includes both values for diagnosis
    let msg = err_with_context.to_string();
    assert!(msg.contains("deadbeef"), "Should contain expected CRC");
    assert!(msg.contains("cafebabe"), "Should contain computed CRC");
}
