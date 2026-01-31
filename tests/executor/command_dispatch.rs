//! Command Dispatch Tests
//!
//! Tests that the Executor correctly dispatches all Command variants
//! and returns the appropriate Output types.

use crate::common::*;
use strata_core::Value;
use strata_executor::{Command, Output, DistanceMetric, BranchId};

// ============================================================================
// Database Commands
// ============================================================================

#[test]
fn ping_returns_version_string() {
    let executor = create_executor();

    let output = executor.execute(Command::Ping).unwrap();

    match output {
        Output::Pong { version } => {
            assert!(!version.is_empty());
        }
        _ => panic!("Expected Pong output"),
    }
}

#[test]
fn info_returns_database_info() {
    let executor = create_executor();

    let output = executor.execute(Command::Info).unwrap();

    match output {
        Output::DatabaseInfo(info) => {
            assert!(!info.version.is_empty());
        }
        _ => panic!("Expected DatabaseInfo output"),
    }
}

#[test]
fn flush_returns_unit() {
    let executor = create_executor();

    let output = executor.execute(Command::Flush).unwrap();
    assert!(matches!(output, Output::Unit));
}

#[test]
fn compact_returns_unit() {
    let executor = create_executor();

    let output = executor.execute(Command::Compact).unwrap();
    assert!(matches!(output, Output::Unit));
}

// ============================================================================
// KV Commands
// ============================================================================

#[test]
fn kv_put_returns_version() {
    let executor = create_executor();

    let output = executor.execute(Command::KvPut {
        run: None,
        key: "test_key".into(),
        value: Value::String("test_value".into()),
    }).unwrap();

    match output {
        Output::Version(v) => assert!(v > 0),
        _ => panic!("Expected Version output"),
    }
}

#[test]
fn kv_get_returns_maybe_versioned() {
    let executor = create_executor();

    // Put first
    executor.execute(Command::KvPut {
        run: None,
        key: "k".into(),
        value: Value::Int(42),
    }).unwrap();

    // Get
    let output = executor.execute(Command::KvGet {
        run: None,
        key: "k".into(),
    }).unwrap();

    match output {
        Output::Maybe(Some(val)) => {
            assert_eq!(val, Value::Int(42));
        }
        _ => panic!("Expected Maybe(Some) output"),
    }
}

#[test]
fn kv_get_missing_returns_none() {
    let executor = create_executor();

    let output = executor.execute(Command::KvGet {
        run: None,
        key: "nonexistent".into(),
    }).unwrap();

    assert!(matches!(output, Output::Maybe(None)));
}

#[test]
fn kv_delete_returns_bool() {
    let executor = create_executor();

    executor.execute(Command::KvPut {
        run: None,
        key: "k".into(),
        value: Value::Int(1),
    }).unwrap();

    let output = executor.execute(Command::KvDelete {
        run: None,
        key: "k".into(),
    }).unwrap();

    assert!(matches!(output, Output::Bool(true)));

    // Delete again - should return false
    let output = executor.execute(Command::KvDelete {
        run: None,
        key: "k".into(),
    }).unwrap();

    assert!(matches!(output, Output::Bool(false)));
}

// ============================================================================
// Event Commands
// ============================================================================

#[test]
fn event_append_returns_version() {
    let executor = create_executor();

    let output = executor.execute(Command::EventAppend {
        run: None,
        event_type: "test_stream".into(),
        payload: event_payload("data", Value::String("event1".into())),
    }).unwrap();

    assert!(matches!(output, Output::Version(_)));
}

#[test]
fn event_len_returns_count() {
    let executor = create_executor();

    for i in 0..5 {
        executor.execute(Command::EventAppend {
            run: None,
            event_type: "counting".into(),
            payload: event_payload("i", Value::Int(i)),
        }).unwrap();
    }

    let output = executor.execute(Command::EventLen {
        run: None,
    }).unwrap();

    match output {
        Output::Uint(count) => assert_eq!(count, 5),
        _ => panic!("Expected Uint output"),
    }
}

// ============================================================================
// State Commands
// ============================================================================

#[test]
fn state_set_read_cycle() {
    let executor = create_executor();

    let output = executor.execute(Command::StateSet {
        run: None,
        cell: "status".into(),
        value: Value::String("active".into()),
    }).unwrap();

    assert!(matches!(output, Output::Version(_)));

    let output = executor.execute(Command::StateRead {
        run: None,
        cell: "status".into(),
    }).unwrap();

    match output {
        Output::Maybe(Some(v)) => {
            assert_eq!(v, Value::String("active".into()));
        }
        _ => panic!("Expected Maybe(Some) output"),
    }
}

// ============================================================================
// Vector Commands
// ============================================================================

#[test]
fn vector_create_collection_and_upsert() {
    let executor = create_executor();

    // Create collection
    let output = executor.execute(Command::VectorCreateCollection {
        run: None,
        collection: "embeddings".into(),
        dimension: 4,
        metric: DistanceMetric::Cosine,
    }).unwrap();

    assert!(matches!(output, Output::Version(_)));

    // Upsert vector
    let output = executor.execute(Command::VectorUpsert {
        run: None,
        collection: "embeddings".into(),
        key: "v1".into(),
        vector: vec![1.0, 0.0, 0.0, 0.0],
        metadata: None,
    }).unwrap();

    assert!(matches!(output, Output::Version(_)));
}

#[test]
fn vector_search_returns_matches() {
    let executor = create_executor();

    executor.execute(Command::VectorCreateCollection {
        run: None,
        collection: "search_test".into(),
        dimension: 4,
        metric: DistanceMetric::Cosine,
    }).unwrap();

    executor.execute(Command::VectorUpsert {
        run: None,
        collection: "search_test".into(),
        key: "v1".into(),
        vector: vec![1.0, 0.0, 0.0, 0.0],
        metadata: None,
    }).unwrap();

    executor.execute(Command::VectorUpsert {
        run: None,
        collection: "search_test".into(),
        key: "v2".into(),
        vector: vec![0.0, 1.0, 0.0, 0.0],
        metadata: None,
    }).unwrap();

    let output = executor.execute(Command::VectorSearch {
        run: None,
        collection: "search_test".into(),
        query: vec![1.0, 0.0, 0.0, 0.0],
        k: 10,
        filter: None,
        metric: None,
    }).unwrap();

    match output {
        Output::VectorMatches(matches) => {
            assert_eq!(matches.len(), 2);
            assert_eq!(matches[0].key, "v1"); // Exact match should be first
        }
        _ => panic!("Expected VectorMatches output"),
    }
}

#[test]
fn vector_list_collections() {
    let executor = create_executor();

    executor.execute(Command::VectorCreateCollection {
        run: None,
        collection: "coll_a".into(),
        dimension: 4,
        metric: DistanceMetric::Cosine,
    }).unwrap();

    executor.execute(Command::VectorCreateCollection {
        run: None,
        collection: "coll_b".into(),
        dimension: 8,
        metric: DistanceMetric::Euclidean,
    }).unwrap();

    let output = executor.execute(Command::VectorListCollections {
        run: None,
    }).unwrap();

    match output {
        Output::VectorCollectionList(infos) => {
            assert_eq!(infos.len(), 2);
        }
        _ => panic!("Expected VectorCollectionList output"),
    }
}

// ============================================================================
// Run Commands
// ============================================================================

#[test]
fn run_create_and_get() {
    let executor = create_executor();

    // Users can name runs like git branches - no UUID required
    let output = executor.execute(Command::BranchCreate {
        branch_id: Some("main".into()),
        metadata: None,
    }).unwrap();

    let branch_id = match output {
        Output::BranchWithVersion { info, .. } => {
            assert_eq!(info.id.as_str(), "main");
            info.id
        }
        _ => panic!("Expected BranchCreated output"),
    };

    let output = executor.execute(Command::BranchGet {
        run: branch_id,
    }).unwrap();

    match output {
        Output::BranchInfoVersioned(versioned) => {
            assert_eq!(versioned.info.id.as_str(), "main");
        }
        _ => panic!("Expected BranchInfoVersioned output"),
    }
}

#[test]
fn run_names_can_be_human_readable() {
    let executor = create_executor();

    // Test various human-readable run names (like git branches)
    let names = ["experiment-1", "feature/new-model", "v2.0", "test_run"];

    for name in names {
        let output = executor.execute(Command::BranchCreate {
            branch_id: Some(name.into()),
            metadata: None,
        }).unwrap();

        match output {
            Output::BranchWithVersion { info, .. } => {
                assert_eq!(info.id.as_str(), name, "Run name should be preserved");
            }
            _ => panic!("Expected BranchWithVersion output"),
        }
    }
}

#[test]
fn run_list_returns_runs() {
    let executor = create_executor();

    executor.execute(Command::BranchCreate {
        branch_id: Some("production".into()),
        metadata: None,
    }).unwrap();

    executor.execute(Command::BranchCreate {
        branch_id: Some("staging".into()),
        metadata: None,
    }).unwrap();

    let output = executor.execute(Command::BranchList {
        state: None,
        limit: Some(100),
        offset: None,
    }).unwrap();

    match output {
        Output::BranchInfoList(runs) => {
            // At least the default run plus our two created runs
            assert!(runs.len() >= 2, "Expected >= 2 runs (production + staging), got {}", runs.len());
        }
        _ => panic!("Expected BranchInfoList output"),
    }
}

#[test]
fn run_delete_removes_run() {
    let executor = create_executor();

    let branch_id = match executor.execute(Command::BranchCreate {
        branch_id: Some("deletable-run".into()),
        metadata: None,
    }).unwrap() {
        Output::BranchWithVersion { info, .. } => info.id,
        _ => panic!("Expected BranchWithVersion"),
    };

    // Verify it exists
    let output = executor.execute(Command::BranchExists {
        run: branch_id.clone(),
    }).unwrap();
    assert!(matches!(output, Output::Bool(true)));

    // Delete it
    executor.execute(Command::BranchDelete {
        run: branch_id.clone(),
    }).unwrap();

    // Verify it's gone
    let output = executor.execute(Command::BranchExists {
        run: branch_id,
    }).unwrap();
    assert!(matches!(output, Output::Bool(false)));
}

#[test]
fn run_exists_returns_bool() {
    let executor = create_executor();

    // Non-existent run
    let output = executor.execute(Command::BranchExists {
        run: BranchId::from("non-existent-run"),
    }).unwrap();
    assert!(matches!(output, Output::Bool(false)));

    // Create a run
    executor.execute(Command::BranchCreate {
        branch_id: Some("exists-test".into()),
        metadata: None,
    }).unwrap();

    // Now it exists
    let output = executor.execute(Command::BranchExists {
        run: BranchId::from("exists-test"),
    }).unwrap();
    assert!(matches!(output, Output::Bool(true)));
}

// ============================================================================
// Default Run Resolution
// ============================================================================

#[test]
fn commands_with_none_run_use_default() {
    let executor = create_executor();

    // Put with run: None
    executor.execute(Command::KvPut {
        run: None,
        key: "default_test".into(),
        value: Value::String("value".into()),
    }).unwrap();

    // Get with explicit default run
    let output = executor.execute(Command::KvGet {
        run: Some(BranchId::default()),
        key: "default_test".into(),
    }).unwrap();

    // Should find the value
    match output {
        Output::Maybe(Some(val)) => {
            assert_eq!(val, Value::String("value".into()));
        }
        _ => panic!("Expected to find value in default run"),
    }
}

#[test]
fn different_runs_are_isolated() {
    let executor = create_executor();

    // Create two runs with human-readable names
    let branch_a = match executor.execute(Command::BranchCreate {
        branch_id: Some("agent-alpha".into()),
        metadata: None,
    }).unwrap() {
        Output::BranchWithVersion { info, .. } => info.id,
        _ => panic!("Expected BranchCreated"),
    };

    let branch_b = match executor.execute(Command::BranchCreate {
        branch_id: Some("agent-beta".into()),
        metadata: None,
    }).unwrap() {
        Output::BranchWithVersion { info, .. } => info.id,
        _ => panic!("Expected BranchCreated"),
    };

    // Put in run A
    executor.execute(Command::KvPut {
        run: Some(branch_a.clone()),
        key: "shared_key".into(),
        value: Value::String("run_a_value".into()),
    }).unwrap();

    // Put in run B
    executor.execute(Command::KvPut {
        run: Some(branch_b.clone()),
        key: "shared_key".into(),
        value: Value::String("run_b_value".into()),
    }).unwrap();

    // Get from run A
    let output = executor.execute(Command::KvGet {
        run: Some(branch_a),
        key: "shared_key".into(),
    }).unwrap();

    match output {
        Output::Maybe(Some(val)) => {
            assert_eq!(val, Value::String("run_a_value".into()));
        }
        _ => panic!("Expected run A value"),
    }

    // Get from run B
    let output = executor.execute(Command::KvGet {
        run: Some(branch_b),
        key: "shared_key".into(),
    }).unwrap();

    match output {
        Output::Maybe(Some(val)) => {
            assert_eq!(val, Value::String("run_b_value".into()));
        }
        _ => panic!("Expected run B value"),
    }
}
