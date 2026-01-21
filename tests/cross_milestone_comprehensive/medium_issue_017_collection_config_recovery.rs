//! ISSUE-017: Collection Config Validation Missing on Recovery
//!
//! **Severity**: MEDIUM
//! **Location**: `/crates/primitives/src/vector/store.rs:282-283`
//!
//! **Problem**: During WAL replay, if a collection already exists, errors are
//! silently ignored. No validation that config matches.
//!
//! **Impact**: Potential silent data corruption if configs differ.

use crate::test_utils::*;
use strata_primitives::{DistanceMetric, StorageDtype, VectorConfig};

/// Test config validation on recovery.
#[test]
fn test_config_validation_on_recovery() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    // Create collection with specific config
    {
        let vector = test_db.vector();
        let config = VectorConfig {
            dimension: 128,
            metric: DistanceMetric::Cosine,
            storage_dtype: StorageDtype::F32,
        };
        vector.create_collection(run_id, "config_test", config).expect("create");
        vector.insert(run_id, "config_test", "v1", &seeded_vector(128, 1), None).expect("insert");
    }

    test_db.db.flush().expect("flush");
    test_db.reopen();

    // Verify collection exists with correct config
    let vector = test_db.vector();
    let info = vector.get_collection(run_id, "config_test").expect("get_collection");
    assert!(info.is_some(), "Collection should exist after recovery");

    // When ISSUE-017 is fixed:
    // - Recovery should validate config matches
    // - Mismatched config should log warning or error
}

/// Test recovery with conflicting WAL entries.
#[test]
fn test_recovery_conflicting_wal() {
    // When ISSUE-017 is fixed:
    // - WAL replay should detect config mismatches
    // - Recovery should either fail or log warning
}
