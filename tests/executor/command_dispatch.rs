//! Command Dispatch Tests
//!
//! Tests that the Executor correctly dispatches all Command variants
//! and returns the appropriate Output types.

use crate::common::*;
use strata_core::Value;
use strata_executor::{BranchId, Command, DistanceMetric, Output};

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
fn compact_succeeds_on_ephemeral() {
    let executor = create_executor();

    // Compact on an ephemeral database is a no-op
    let result = executor.execute(Command::Compact);
    assert!(result.is_ok());
}

// ============================================================================
// KV Commands
// ============================================================================

#[test]
fn kv_put_returns_version() {
    let executor = create_executor();

    let output = executor
        .execute(Command::KvPut {
            branch: None,
            space: None,
            key: "test_key".into(),
            value: Value::String("test_value".into()),
        })
        .unwrap();

    match output {
        Output::Version(v) => assert!(v > 0),
        _ => panic!("Expected Version output"),
    }
}

#[test]
fn kv_get_returns_maybe_versioned() {
    let executor = create_executor();

    // Put first
    executor
        .execute(Command::KvPut {
            branch: None,
            space: None,
            key: "k".into(),
            value: Value::Int(42),
        })
        .unwrap();

    // Get
    let output = executor
        .execute(Command::KvGet {
            branch: None,
            space: None,
            key: "k".into(),
        })
        .unwrap();

    match output {
        Output::MaybeVersioned(Some(vv)) => {
            let val = vv.value;
            assert_eq!(val, Value::Int(42));
        }
        _ => panic!("Expected Maybe(Some) output"),
    }
}

#[test]
fn kv_get_missing_returns_none() {
    let executor = create_executor();

    let output = executor
        .execute(Command::KvGet {
            branch: None,
            space: None,
            key: "nonexistent".into(),
        })
        .unwrap();

    assert!(matches!(
        output,
        Output::MaybeVersioned(None) | Output::Maybe(None)
    ));
}

#[test]
fn kv_delete_returns_bool() {
    let executor = create_executor();

    executor
        .execute(Command::KvPut {
            branch: None,
            space: None,
            key: "k".into(),
            value: Value::Int(1),
        })
        .unwrap();

    let output = executor
        .execute(Command::KvDelete {
            branch: None,
            space: None,
            key: "k".into(),
        })
        .unwrap();

    assert!(matches!(output, Output::Bool(true)));

    // Delete again - should return false
    let output = executor
        .execute(Command::KvDelete {
            branch: None,
            space: None,
            key: "k".into(),
        })
        .unwrap();

    assert!(matches!(output, Output::Bool(false)));
}

// ============================================================================
// Event Commands
// ============================================================================

#[test]
fn event_append_returns_version() {
    let executor = create_executor();

    let output = executor
        .execute(Command::EventAppend {
            branch: None,
            space: None,
            event_type: "test_stream".into(),
            payload: event_payload("data", Value::String("event1".into())),
        })
        .unwrap();

    assert!(matches!(output, Output::Version(_)));
}

#[test]
fn event_len_returns_count() {
    let executor = create_executor();

    for i in 0..5 {
        executor
            .execute(Command::EventAppend {
                branch: None,
                space: None,
                event_type: "counting".into(),
                payload: event_payload("i", Value::Int(i)),
            })
            .unwrap();
    }

    let output = executor
        .execute(Command::EventLen {
            branch: None,
            space: None,
        })
        .unwrap();

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

    let output = executor
        .execute(Command::StateSet {
            branch: None,
            space: None,
            cell: "status".into(),
            value: Value::String("active".into()),
        })
        .unwrap();

    assert!(matches!(output, Output::Version(_)));

    let output = executor
        .execute(Command::StateGet {
            branch: None,
            space: None,
            cell: "status".into(),
        })
        .unwrap();

    match output {
        Output::MaybeVersioned(Some(vv)) => {
            let v = vv.value;
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
    let output = executor
        .execute(Command::VectorCreateCollection {
            branch: None,
            space: None,
            collection: "embeddings".into(),
            dimension: 4,
            metric: DistanceMetric::Cosine,
        })
        .unwrap();

    assert!(matches!(output, Output::Version(_)));

    // Upsert vector
    let output = executor
        .execute(Command::VectorUpsert {
            branch: None,
            space: None,
            collection: "embeddings".into(),
            key: "v1".into(),
            vector: vec![1.0, 0.0, 0.0, 0.0],
            metadata: None,
        })
        .unwrap();

    assert!(matches!(output, Output::Version(_)));
}

#[test]
fn vector_search_returns_matches() {
    let executor = create_executor();

    executor
        .execute(Command::VectorCreateCollection {
            branch: None,
            space: None,
            collection: "search_test".into(),
            dimension: 4,
            metric: DistanceMetric::Cosine,
        })
        .unwrap();

    executor
        .execute(Command::VectorUpsert {
            branch: None,
            space: None,
            collection: "search_test".into(),
            key: "v1".into(),
            vector: vec![1.0, 0.0, 0.0, 0.0],
            metadata: None,
        })
        .unwrap();

    executor
        .execute(Command::VectorUpsert {
            branch: None,
            space: None,
            collection: "search_test".into(),
            key: "v2".into(),
            vector: vec![0.0, 1.0, 0.0, 0.0],
            metadata: None,
        })
        .unwrap();

    let output = executor
        .execute(Command::VectorSearch {
            branch: None,
            space: None,
            collection: "search_test".into(),
            query: vec![1.0, 0.0, 0.0, 0.0],
            k: 10,
            filter: None,
            metric: None,
        })
        .unwrap();

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

    executor
        .execute(Command::VectorCreateCollection {
            branch: None,
            space: None,
            collection: "coll_a".into(),
            dimension: 4,
            metric: DistanceMetric::Cosine,
        })
        .unwrap();

    executor
        .execute(Command::VectorCreateCollection {
            branch: None,
            space: None,
            collection: "coll_b".into(),
            dimension: 8,
            metric: DistanceMetric::Euclidean,
        })
        .unwrap();

    let output = executor
        .execute(Command::VectorListCollections {
            branch: None,
            space: None,
        })
        .unwrap();

    match output {
        Output::VectorCollectionList(infos) => {
            assert_eq!(infos.len(), 2);
        }
        _ => panic!("Expected VectorCollectionList output"),
    }
}

// ============================================================================
// Branch Commands
// ============================================================================

#[test]
fn branch_create_and_get() {
    let executor = create_executor();

    // Users can name branches like git branches - no UUID required
    let output = executor
        .execute(Command::BranchCreate {
            branch_id: Some("main".into()),
            metadata: None,
        })
        .unwrap();

    let branch_id = match output {
        Output::BranchWithVersion { info, .. } => {
            assert_eq!(info.id.as_str(), "main");
            info.id
        }
        _ => panic!("Expected BranchCreated output"),
    };

    let output = executor
        .execute(Command::BranchGet { branch: branch_id })
        .unwrap();

    match output {
        Output::MaybeBranchInfo(Some(versioned)) => {
            assert_eq!(versioned.info.id.as_str(), "main");
        }
        _ => panic!("Expected MaybeBranchInfo(Some(...)) output"),
    }
}

#[test]
fn branch_names_can_be_human_readable() {
    let executor = create_executor();

    // Test various human-readable branch names (like git branches)
    let names = ["experiment-1", "feature/new-model", "v2.0", "test_branch"];

    for name in names {
        let output = executor
            .execute(Command::BranchCreate {
                branch_id: Some(name.into()),
                metadata: None,
            })
            .unwrap();

        match output {
            Output::BranchWithVersion { info, .. } => {
                assert_eq!(info.id.as_str(), name, "Branch name should be preserved");
            }
            _ => panic!("Expected BranchWithVersion output"),
        }
    }
}

#[test]
fn branch_list_returns_branches() {
    let executor = create_executor();

    executor
        .execute(Command::BranchCreate {
            branch_id: Some("production".into()),
            metadata: None,
        })
        .unwrap();

    executor
        .execute(Command::BranchCreate {
            branch_id: Some("staging".into()),
            metadata: None,
        })
        .unwrap();

    let output = executor
        .execute(Command::BranchList {
            state: None,
            limit: Some(100),
            offset: None,
        })
        .unwrap();

    match output {
        Output::BranchInfoList(branches) => {
            // At least the default branch plus our two created branches
            assert!(
                branches.len() >= 2,
                "Expected >= 2 branches (production + staging), got {}",
                branches.len()
            );
        }
        _ => panic!("Expected BranchInfoList output"),
    }
}

#[test]
fn branch_delete_removes_branch() {
    let executor = create_executor();

    let branch_id = match executor
        .execute(Command::BranchCreate {
            branch_id: Some("deletable-branch".into()),
            metadata: None,
        })
        .unwrap()
    {
        Output::BranchWithVersion { info, .. } => info.id,
        _ => panic!("Expected BranchWithVersion"),
    };

    // Verify it exists
    let output = executor
        .execute(Command::BranchExists {
            branch: branch_id.clone(),
        })
        .unwrap();
    assert!(matches!(output, Output::Bool(true)));

    // Delete it
    executor
        .execute(Command::BranchDelete {
            branch: branch_id.clone(),
        })
        .unwrap();

    // Verify it's gone
    let output = executor
        .execute(Command::BranchExists { branch: branch_id })
        .unwrap();
    assert!(matches!(output, Output::Bool(false)));
}

#[test]
fn branch_exists_returns_bool() {
    let executor = create_executor();

    // Non-existent branch
    let output = executor
        .execute(Command::BranchExists {
            branch: BranchId::from("non-existent-branch"),
        })
        .unwrap();
    assert!(matches!(output, Output::Bool(false)));

    // Create a branch
    executor
        .execute(Command::BranchCreate {
            branch_id: Some("exists-test".into()),
            metadata: None,
        })
        .unwrap();

    // Now it exists
    let output = executor
        .execute(Command::BranchExists {
            branch: BranchId::from("exists-test"),
        })
        .unwrap();
    assert!(matches!(output, Output::Bool(true)));
}

// ============================================================================
// Default Branch Resolution
// ============================================================================

#[test]
fn commands_with_none_branch_use_default() {
    let executor = create_executor();

    // Put with branch: None
    executor
        .execute(Command::KvPut {
            branch: None,
            space: None,
            key: "default_test".into(),
            value: Value::String("value".into()),
        })
        .unwrap();

    // Get with explicit default branch
    let output = executor
        .execute(Command::KvGet {
            branch: Some(BranchId::default()),
            space: None,
            key: "default_test".into(),
        })
        .unwrap();

    // Should find the value
    match output {
        Output::MaybeVersioned(Some(vv)) => {
            let val = vv.value;
            assert_eq!(val, Value::String("value".into()));
        }
        _ => panic!("Expected to find value in default branch"),
    }
}

#[test]
fn different_branches_are_isolated() {
    let executor = create_executor();

    // Create two branches with human-readable names
    let branch_a = match executor
        .execute(Command::BranchCreate {
            branch_id: Some("agent-alpha".into()),
            metadata: None,
        })
        .unwrap()
    {
        Output::BranchWithVersion { info, .. } => info.id,
        _ => panic!("Expected BranchCreated"),
    };

    let branch_b = match executor
        .execute(Command::BranchCreate {
            branch_id: Some("agent-beta".into()),
            metadata: None,
        })
        .unwrap()
    {
        Output::BranchWithVersion { info, .. } => info.id,
        _ => panic!("Expected BranchCreated"),
    };

    // Put in branch A
    executor
        .execute(Command::KvPut {
            branch: Some(branch_a.clone()),
            space: None,
            key: "shared_key".into(),
            value: Value::String("branch_a_value".into()),
        })
        .unwrap();

    // Put in branch B
    executor
        .execute(Command::KvPut {
            branch: Some(branch_b.clone()),
            space: None,
            key: "shared_key".into(),
            value: Value::String("branch_b_value".into()),
        })
        .unwrap();

    // Get from branch A
    let output = executor
        .execute(Command::KvGet {
            branch: Some(branch_a),
            space: None,
            key: "shared_key".into(),
        })
        .unwrap();

    match output {
        Output::MaybeVersioned(Some(vv)) => {
            let val = vv.value;
            assert_eq!(val, Value::String("branch_a_value".into()));
        }
        _ => panic!("Expected branch A value"),
    }

    // Get from branch B
    let output = executor
        .execute(Command::KvGet {
            branch: Some(branch_b),
            space: None,
            key: "shared_key".into(),
        })
        .unwrap();

    match output {
        Output::MaybeVersioned(Some(vv)) => {
            let val = vv.value;
            assert_eq!(val, Value::String("branch_b_value".into()));
        }
        _ => panic!("Expected branch B value"),
    }
}
