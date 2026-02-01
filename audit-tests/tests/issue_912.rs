//! Audit test for issue #912: WAL segment header database UUID never validated
//! Verdict: CONFIRMED BUG
//!
//! The database_uuid field in WAL segment headers is written and stored but
//! never validated against an expected UUID during segment open or replay.
//! This means segments from different databases could be silently mixed,
//! corrupting the recovered state.

use strata_durability::format::{SegmentHeader, WalRecord, WalSegment};
use tempfile::tempdir;

/// Demonstrates that WalSegment::open_read() does not validate the database UUID.
/// A segment created with one UUID can be opened with no UUID check.
#[test]
fn issue_912_open_read_ignores_database_uuid() {
    let dir = tempdir().unwrap();
    let uuid_a = [0xAA; 16];
    let uuid_b = [0xBB; 16];

    // Create a segment with UUID A
    {
        let mut segment = WalSegment::create(dir.path(), 1, uuid_a).unwrap();
        segment.write(b"test data for db A").unwrap();
        segment.close().unwrap();
    }

    // Open it for reading -- no UUID parameter is accepted, so there is
    // no way to validate the UUID even if we wanted to
    let segment = WalSegment::open_read(dir.path(), 1).unwrap();

    // The segment reports UUID A
    assert_eq!(segment.database_uuid(), uuid_a);

    // But there was no validation that the UUID matches any expected value.
    // If this segment belonged to database B (due to a copy/backup error),
    // it would be silently accepted.
    //
    // BUG: open_read() does not accept an expected_uuid parameter and
    // has no way to validate the UUID against a known database identity.

    // Demonstrate: we can read the UUID but the API provides no validation
    // The caller would have to manually check after opening, which is error-prone.
    assert_ne!(
        segment.database_uuid(),
        uuid_b,
        "UUID A != UUID B, but the API had no way to enforce this check"
    );
}

/// Demonstrates that WalSegment::open_append() does not validate the database UUID.
/// A writer with one UUID can reopen a segment written by a different database.
#[test]
fn issue_912_open_append_ignores_database_uuid() {
    let dir = tempdir().unwrap();
    let uuid_a = [0xAA; 16];

    // Create a segment with UUID A
    {
        let segment = WalSegment::create(dir.path(), 1, uuid_a).unwrap();
        drop(segment); // Don't close -- simulate an unclean shutdown
    }

    // Reopen for appending -- no expected UUID is checked
    let segment = WalSegment::open_append(dir.path(), 1).unwrap();
    assert_eq!(segment.database_uuid(), uuid_a);

    // BUG: open_append() takes no expected_uuid parameter.
    // A WalWriter with uuid_b could happily append to a segment
    // created by uuid_a without detecting the mismatch.
}

/// Demonstrates that segments with different UUIDs in the same directory
/// can be read sequentially during replay without any UUID consistency check.
#[test]
fn issue_912_mixed_uuid_segments_not_detected() {
    let dir = tempdir().unwrap();
    let uuid_a = [0xAA; 16];
    let uuid_b = [0xBB; 16];

    // Create segment 1 with UUID A
    {
        let mut seg = WalSegment::create(dir.path(), 1, uuid_a).unwrap();
        let record = WalRecord::new(1, [1u8; 16], 1000, vec![1, 2, 3]);
        seg.write(&record.to_bytes()).unwrap();
        seg.close().unwrap();
    }

    // Create segment 2 with UUID B (different database!)
    {
        let mut seg = WalSegment::create(dir.path(), 2, uuid_b).unwrap();
        let record = WalRecord::new(2, [2u8; 16], 2000, vec![4, 5, 6]);
        seg.write(&record.to_bytes()).unwrap();
        seg.close().unwrap();
    }

    // Open both segments -- no cross-segment UUID validation occurs
    let seg1 = WalSegment::open_read(dir.path(), 1).unwrap();
    let seg2 = WalSegment::open_read(dir.path(), 2).unwrap();

    assert_eq!(seg1.database_uuid(), uuid_a);
    assert_eq!(seg2.database_uuid(), uuid_b);

    // BUG: These segments belong to DIFFERENT databases but coexist
    // in the same directory. During WAL replay, both would be processed
    // sequentially, mixing records from two different databases.
    assert_ne!(
        seg1.database_uuid(),
        seg2.database_uuid(),
        "Segments from different databases coexist without any validation. \
         WAL replay would silently mix records from both databases."
    );
}

/// Demonstrates that SegmentHeader::is_valid() only checks magic bytes,
/// not the database UUID.
#[test]
fn issue_912_header_validation_ignores_uuid() {
    // Create a header with a known UUID
    let header = SegmentHeader::new(1, [0xAA; 16]);
    assert!(
        header.is_valid(),
        "Header with valid magic should pass validation"
    );

    // Modify UUID to all zeros (clearly wrong)
    let mut header_wrong_uuid = header;
    header_wrong_uuid.database_uuid = [0x00; 16];

    // is_valid() still returns true -- it only checks magic bytes
    assert!(
        header_wrong_uuid.is_valid(),
        "BUG: is_valid() returns true even with wrong UUID. \
         It only validates magic bytes, not the database identity."
    );

    // Serialize and deserialize -- UUID is preserved but never validated
    let bytes = header_wrong_uuid.to_bytes();
    let parsed = SegmentHeader::from_bytes(&bytes).unwrap();
    assert_eq!(parsed.database_uuid, [0x00; 16]);
    assert!(
        parsed.is_valid(),
        "Parsed header with zero UUID still passes validation"
    );
}
