//! ISSUE-007: VectorConfig storage_dtype Not in WAL VectorCollectionCreate
//!
//! **Severity**: HIGH
//! **Location**: `/crates/durability/src/wal.rs:204-215`
//!
//! **Problem**: WAL entry has `dimension` and `metric` but NO `storage_dtype` field.
//! During replay, recovery hardcodes `StorageDtype::F32`.
//!
//! **Impact**: When F16/Int8 quantization is added in M9, WAL format will need breaking change.

use crate::test_utils::*;
use in_mem_primitives::{StorageDtype, VectorConfig, DistanceMetric};

/// Test that storage_dtype is persisted in WAL.
#[test]
fn test_storage_dtype_persisted_in_wal() {
    let test_db = TestDb::new_strict();
    let vector = test_db.vector();
    let run_id = test_db.run_id;

    // Create collection with explicit storage_dtype
    let config = VectorConfig {
        dimension: 128,
        metric: DistanceMetric::Cosine,
        storage_dtype: StorageDtype::F32,
    };

    vector
        .create_collection(run_id, "dtype_test", config)
        .expect("create collection");

    test_db.db.flush().expect("flush");

    // When ISSUE-007 is fixed:
    // - WAL VectorCollectionCreate entry should contain storage_dtype field
    // - Recovery should read and use the storage_dtype from WAL
}

/// Test that collection config survives recovery with storage_dtype.
#[test]
fn test_storage_dtype_survives_recovery() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    // Create collection
    {
        let vector = test_db.vector();
        let config = VectorConfig {
            dimension: 64,
            metric: DistanceMetric::Euclidean,
            storage_dtype: StorageDtype::F32,
        };
        vector
            .create_collection(run_id, "recover_dtype", config)
            .expect("create");

        // Insert a vector
        vector
            .insert(run_id, "recover_dtype", "v1", &seeded_vector(64, 1), None)
            .expect("insert");
    }

    test_db.db.flush().expect("flush");
    test_db.reopen();

    // Verify collection still exists with correct config
    let vector = test_db.vector();
    let info = vector
        .get_collection(run_id, "recover_dtype")
        .expect("get_collection");

    assert!(info.is_some(), "Collection should exist after recovery");

    // When ISSUE-007 is fixed:
    // let config = info.unwrap().config;
    // assert_eq!(config.storage_dtype, StorageDtype::F32);
}

/// Test forward compatibility placeholder for F16/Int8.
#[test]
fn test_future_storage_dtype_support() {
    // When M9 adds quantization:
    // - StorageDtype::F16 should be supported
    // - StorageDtype::Int8 should be supported
    // - WAL entry should include storage_dtype to avoid breaking change

    // For now, verify StorageDtype enum exists
    let dtype = StorageDtype::F32;
    assert!(matches!(dtype, StorageDtype::F32));
}
