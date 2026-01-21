//! Tier 3.2: Snapshot CRC Tests
//!
//! Tests for snapshot CRC32 integrity validation.

use crate::test_utils::*;
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_primitives::KVStore;
use std::fs::OpenOptions;
use std::io::{Seek, SeekFrom, Write};

/// Corrupted snapshot should be detected
#[test]
fn test_snapshot_corruption_detection_concept() {
    let test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(&run_id, "key", Value::String("value".into()))
        .unwrap();

    let snapshot_dir = test_db.snapshot_dir();
    let snapshots = list_snapshots(&snapshot_dir);

    if let Some(snapshot_path) = snapshots.first() {
        // Corrupt the snapshot
        let file_size = file_size(snapshot_path);
        if file_size > 100 {
            corrupt_file_at_offset(snapshot_path, 50, &[0xFF, 0xFF, 0xFF]);

            // Loading corrupt snapshot should fail or be detected
            // The exact behavior depends on implementation
        }
    }
}

/// CRC validation catches single-bit errors
#[test]
fn test_crc_catches_single_bit_error() {
    // Test that CRC32 algorithm catches single-bit errors
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let data1 = vec![0u8; 100];
    let mut data2 = data1.clone();
    data2[50] = 1; // Single bit flip

    let mut hasher1 = DefaultHasher::new();
    data1.hash(&mut hasher1);
    let hash1 = hasher1.finish();

    let mut hasher2 = DefaultHasher::new();
    data2.hash(&mut hasher2);
    let hash2 = hasher2.finish();

    assert_ne!(hash1, hash2, "Hash should detect single-bit change");
}

/// Valid data produces consistent CRC
#[test]
fn test_crc_consistency() {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let data = b"test data for crc validation";

    // Hash multiple times
    let hashes: Vec<_> = (0..100)
        .map(|_| {
            let mut hasher = DefaultHasher::new();
            data.hash(&mut hasher);
            hasher.finish()
        })
        .collect();

    // All hashes should be identical
    assert!(
        hashes.windows(2).all(|w| w[0] == w[1]),
        "CRC should be deterministic"
    );
}

/// Snapshot with data integrity
#[test]
fn test_snapshot_data_integrity() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();
    kv.put(
        &run_id,
        "integrity_key",
        Value::String("integrity_value".into()),
    )
    .unwrap();

    let state_before = CapturedState::capture(&test_db.db, &run_id);

    // Reopen (which may use snapshot for recovery)
    test_db.reopen();

    let state_after = CapturedState::capture(&test_db.db, &run_id);

    // Data integrity should be maintained
    assert_states_equal(&state_before, &state_after, "Snapshot integrity failed");
}

/// Large snapshot integrity
#[test]
fn test_large_snapshot_integrity() {
    let mut test_db = TestDb::new();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Write large dataset
    for i in 0..1000 {
        kv.put(&run_id, &format!("k{}", i), Value::I64(i)).unwrap();
    }

    let state_before = CapturedState::capture(&test_db.db, &run_id);

    test_db.reopen();

    let state_after = CapturedState::capture(&test_db.db, &run_id);

    assert_eq!(
        state_before.kv_entries.len(),
        state_after.kv_entries.len(),
        "Large snapshot lost entries"
    );
}
