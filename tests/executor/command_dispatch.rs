//! Command Dispatch Tests
//!
//! Tests that the Executor correctly dispatches all Command variants
//! and returns the appropriate Output types.

use crate::common::*;
use strata_core::Value;
use strata_executor::{Command, Output, DistanceMetric, RunId, RunStatus};

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
        Output::MaybeVersioned(Some(vv)) => {
            assert_eq!(vv.value, Value::Int(42));
        }
        _ => panic!("Expected MaybeVersioned(Some) output"),
    }
}

#[test]
fn kv_get_missing_returns_none() {
    let executor = create_executor();

    let output = executor.execute(Command::KvGet {
        run: None,
        key: "nonexistent".into(),
    }).unwrap();

    assert!(matches!(output, Output::MaybeVersioned(None)));
}

#[test]
fn kv_exists_returns_bool() {
    let executor = create_executor();

    executor.execute(Command::KvPut {
        run: None,
        key: "k".into(),
        value: Value::Int(1),
    }).unwrap();

    let output = executor.execute(Command::KvExists {
        run: None,
        key: "k".into(),
    }).unwrap();

    assert!(matches!(output, Output::Bool(true)));

    let output = executor.execute(Command::KvExists {
        run: None,
        key: "missing".into(),
    }).unwrap();

    assert!(matches!(output, Output::Bool(false)));
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

#[test]
fn kv_incr_returns_new_value() {
    let executor = create_executor();

    executor.execute(Command::KvPut {
        run: None,
        key: "counter".into(),
        value: Value::Int(10),
    }).unwrap();

    let output = executor.execute(Command::KvIncr {
        run: None,
        key: "counter".into(),
        delta: 5,
    }).unwrap();

    match output {
        Output::Int(v) => assert_eq!(v, 15),
        _ => panic!("Expected Int output"),
    }
}

#[test]
fn kv_keys_returns_key_list() {
    let executor = create_executor();

    executor.execute(Command::KvPut {
        run: None,
        key: "prefix:a".into(),
        value: Value::Int(1),
    }).unwrap();

    executor.execute(Command::KvPut {
        run: None,
        key: "prefix:b".into(),
        value: Value::Int(2),
    }).unwrap();

    executor.execute(Command::KvPut {
        run: None,
        key: "other".into(),
        value: Value::Int(3),
    }).unwrap();

    let output = executor.execute(Command::KvKeys {
        run: None,
        prefix: "prefix:".into(),
        limit: None,
    }).unwrap();

    match output {
        Output::Keys(keys) => {
            assert_eq!(keys.len(), 2);
            assert!(keys.contains(&"prefix:a".to_string()));
            assert!(keys.contains(&"prefix:b".to_string()));
        }
        _ => panic!("Expected Keys output"),
    }
}

#[test]
#[ignore] // kv_mput temporarily disabled during engine re-architecture
fn kv_mput_mget_batch_operations() {
    let executor = create_executor();

    // Batch put
    let output = executor.execute(Command::KvMput {
        run: None,
        entries: vec![
            ("k1".into(), Value::Int(1)),
            ("k2".into(), Value::Int(2)),
            ("k3".into(), Value::Int(3)),
        ],
    }).unwrap();

    assert!(matches!(output, Output::Version(_)));

    // Batch get
    let output = executor.execute(Command::KvMget {
        run: None,
        keys: vec!["k1".into(), "k2".into(), "missing".into()],
    }).unwrap();

    match output {
        Output::Values(values) => {
            assert_eq!(values.len(), 3);
            assert!(values[0].is_some());
            assert!(values[1].is_some());
            assert!(values[2].is_none());
        }
        _ => panic!("Expected Values output"),
    }
}

// ============================================================================
// Event Commands
// ============================================================================

#[test]
fn event_append_returns_version() {
    let executor = create_executor();

    let output = executor.execute(Command::EventAppend {
        run: None,
        stream: "test_stream".into(),
        payload: event_payload("data", Value::String("event1".into())),
    }).unwrap();

    assert!(matches!(output, Output::Version(_)));
}

#[test]
fn event_range_returns_events() {
    let executor = create_executor();

    // Append events
    for i in 0..3 {
        executor.execute(Command::EventAppend {
            run: None,
            stream: "stream".into(),
            payload: event_payload("n", Value::Int(i)),
        }).unwrap();
    }

    let output = executor.execute(Command::EventRange {
        run: None,
        stream: "stream".into(),
        start: None,
        end: None,
        limit: None,
    }).unwrap();

    match output {
        Output::VersionedValues(events) => {
            assert_eq!(events.len(), 3);
        }
        _ => panic!("Expected VersionedValues output"),
    }
}

#[test]
fn event_len_returns_count() {
    let executor = create_executor();

    for i in 0..5 {
        executor.execute(Command::EventAppend {
            run: None,
            stream: "counting".into(),
            payload: event_payload("i", Value::Int(i)),
        }).unwrap();
    }

    let output = executor.execute(Command::EventLen {
        run: None,
        stream: "counting".into(),
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
        Output::MaybeVersioned(Some(vv)) => {
            assert_eq!(vv.value, Value::String("active".into()));
        }
        _ => panic!("Expected MaybeVersioned(Some) output"),
    }
}

#[test]
fn state_exists_returns_bool() {
    let executor = create_executor();

    executor.execute(Command::StateSet {
        run: None,
        cell: "cell1".into(),
        value: Value::Int(1),
    }).unwrap();

    let output = executor.execute(Command::StateExists {
        run: None,
        cell: "cell1".into(),
    }).unwrap();

    assert!(matches!(output, Output::Bool(true)));

    let output = executor.execute(Command::StateExists {
        run: None,
        cell: "missing".into(),
    }).unwrap();

    assert!(matches!(output, Output::Bool(false)));
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
    let output = executor.execute(Command::RunCreate {
        run_id: Some("main".into()),
        metadata: None,
    }).unwrap();

    let run_id = match output {
        Output::RunWithVersion { info, .. } => {
            assert_eq!(info.id.as_str(), "main");
            info.id
        }
        _ => panic!("Expected RunCreated output"),
    };

    let output = executor.execute(Command::RunGet {
        run: run_id,
    }).unwrap();

    match output {
        Output::RunInfoVersioned(versioned) => {
            assert_eq!(versioned.info.id.as_str(), "main");
        }
        _ => panic!("Expected RunInfoVersioned output"),
    }
}

#[test]
fn run_names_can_be_human_readable() {
    let executor = create_executor();

    // Test various human-readable run names (like git branches)
    let names = ["experiment-1", "feature/new-model", "v2.0", "test_run"];

    for name in names {
        let output = executor.execute(Command::RunCreate {
            run_id: Some(name.into()),
            metadata: None,
        }).unwrap();

        match output {
            Output::RunWithVersion { info, .. } => {
                assert_eq!(info.id.as_str(), name, "Run name should be preserved");
            }
            _ => panic!("Expected RunWithVersion output"),
        }
    }
}

#[test]
fn run_list_returns_runs() {
    let executor = create_executor();

    executor.execute(Command::RunCreate {
        run_id: Some("production".into()),
        metadata: None,
    }).unwrap();

    executor.execute(Command::RunCreate {
        run_id: Some("staging".into()),
        metadata: None,
    }).unwrap();

    let output = executor.execute(Command::RunList {
        state: None,
        limit: Some(100),
        offset: None,
    }).unwrap();

    match output {
        Output::RunInfoList(runs) => {
            // At least the default run plus our two created runs
            assert!(runs.len() >= 2);
        }
        _ => panic!("Expected RunInfos output"),
    }
}

#[test]
fn run_complete_changes_status() {
    let executor = create_executor();

    let run_id = match executor.execute(Command::RunCreate {
        run_id: Some("batch-job-1".into()),
        metadata: None,
    }).unwrap() {
        Output::RunWithVersion { info, .. } => info.id,
        _ => panic!("Expected RunCreated"),
    };

    executor.execute(Command::RunComplete {
        run: run_id.clone(),
    }).unwrap();

    let output = executor.execute(Command::RunGet {
        run: run_id,
    }).unwrap();

    match output {
        Output::RunInfoVersioned(versioned) => {
            assert_eq!(versioned.info.status, RunStatus::Completed);
        }
        _ => panic!("Expected RunInfoVersioned output"),
    }
}

#[test]
fn run_pause_and_resume() {
    let executor = create_executor();

    let run_id = match executor.execute(Command::RunCreate {
        run_id: Some("pausable-run".into()),
        metadata: None,
    }).unwrap() {
        Output::RunWithVersion { info, .. } => info.id,
        _ => panic!("Expected RunWithVersion"),
    };

    // Pause the run
    executor.execute(Command::RunPause {
        run: run_id.clone(),
    }).unwrap();

    let output = executor.execute(Command::RunGet {
        run: run_id.clone(),
    }).unwrap();
    match output {
        Output::RunInfoVersioned(v) => assert_eq!(v.info.status, RunStatus::Paused),
        _ => panic!("Expected RunInfoVersioned"),
    }

    // Resume the run
    executor.execute(Command::RunResume {
        run: run_id.clone(),
    }).unwrap();

    let output = executor.execute(Command::RunGet {
        run: run_id,
    }).unwrap();
    match output {
        Output::RunInfoVersioned(v) => assert_eq!(v.info.status, RunStatus::Active),
        _ => panic!("Expected RunInfoVersioned"),
    }
}

#[test]
fn run_cancel_changes_status() {
    let executor = create_executor();

    let run_id = match executor.execute(Command::RunCreate {
        run_id: Some("cancellable-run".into()),
        metadata: None,
    }).unwrap() {
        Output::RunWithVersion { info, .. } => info.id,
        _ => panic!("Expected RunWithVersion"),
    };

    executor.execute(Command::RunCancel {
        run: run_id.clone(),
    }).unwrap();

    let output = executor.execute(Command::RunGet {
        run: run_id,
    }).unwrap();
    match output {
        Output::RunInfoVersioned(v) => assert_eq!(v.info.status, RunStatus::Cancelled),
        _ => panic!("Expected RunInfoVersioned"),
    }
}

#[test]
fn run_archive_changes_status() {
    let executor = create_executor();

    let run_id = match executor.execute(Command::RunCreate {
        run_id: Some("archivable-run".into()),
        metadata: None,
    }).unwrap() {
        Output::RunWithVersion { info, .. } => info.id,
        _ => panic!("Expected RunWithVersion"),
    };

    executor.execute(Command::RunArchive {
        run: run_id.clone(),
    }).unwrap();

    let output = executor.execute(Command::RunGet {
        run: run_id,
    }).unwrap();
    match output {
        Output::RunInfoVersioned(v) => assert_eq!(v.info.status, RunStatus::Archived),
        _ => panic!("Expected RunInfoVersioned"),
    }
}

#[test]
fn run_delete_removes_run() {
    let executor = create_executor();

    let run_id = match executor.execute(Command::RunCreate {
        run_id: Some("deletable-run".into()),
        metadata: None,
    }).unwrap() {
        Output::RunWithVersion { info, .. } => info.id,
        _ => panic!("Expected RunWithVersion"),
    };

    // Verify it exists
    let output = executor.execute(Command::RunExists {
        run: run_id.clone(),
    }).unwrap();
    assert!(matches!(output, Output::Bool(true)));

    // Delete it
    executor.execute(Command::RunDelete {
        run: run_id.clone(),
    }).unwrap();

    // Verify it's gone
    let output = executor.execute(Command::RunExists {
        run: run_id,
    }).unwrap();
    assert!(matches!(output, Output::Bool(false)));
}

#[test]
fn run_exists_returns_bool() {
    let executor = create_executor();

    // Non-existent run
    let output = executor.execute(Command::RunExists {
        run: RunId::from("non-existent-run"),
    }).unwrap();
    assert!(matches!(output, Output::Bool(false)));

    // Create a run
    executor.execute(Command::RunCreate {
        run_id: Some("exists-test".into()),
        metadata: None,
    }).unwrap();

    // Now it exists
    let output = executor.execute(Command::RunExists {
        run: RunId::from("exists-test"),
    }).unwrap();
    assert!(matches!(output, Output::Bool(true)));
}

#[test]
fn run_update_metadata() {
    let executor = create_executor();

    let run_id = match executor.execute(Command::RunCreate {
        run_id: Some("metadata-run".into()),
        metadata: Some(Value::Object([
            ("version".to_string(), Value::Int(1)),
        ].into_iter().collect())),
    }).unwrap() {
        Output::RunWithVersion { info, .. } => info.id,
        _ => panic!("Expected RunWithVersion"),
    };

    // Update metadata
    executor.execute(Command::RunUpdateMetadata {
        run: run_id.clone(),
        metadata: Value::Object([
            ("version".to_string(), Value::Int(2)),
            ("updated".to_string(), Value::Bool(true)),
        ].into_iter().collect()),
    }).unwrap();

    let output = executor.execute(Command::RunGet {
        run: run_id,
    }).unwrap();
    match output {
        Output::RunInfoVersioned(v) => {
            let meta = v.info.metadata.expect("Should have metadata");
            match meta {
                Value::Object(map) => {
                    assert_eq!(map.get("version"), Some(&Value::Int(2)));
                    assert_eq!(map.get("updated"), Some(&Value::Bool(true)));
                }
                _ => panic!("Expected Object metadata"),
            }
        }
        _ => panic!("Expected RunInfoVersioned"),
    }
}

#[test]
fn run_query_by_status() {
    let executor = create_executor();

    // Create runs with different statuses
    executor.execute(Command::RunCreate {
        run_id: Some("query-active-1".into()),
        metadata: None,
    }).unwrap();

    let completed_run = match executor.execute(Command::RunCreate {
        run_id: Some("query-completed-1".into()),
        metadata: None,
    }).unwrap() {
        Output::RunWithVersion { info, .. } => info.id,
        _ => panic!("Expected RunWithVersion"),
    };
    executor.execute(Command::RunComplete { run: completed_run }).unwrap();

    // Query by Active status
    let output = executor.execute(Command::RunQueryByStatus {
        state: RunStatus::Active,
    }).unwrap();
    match output {
        Output::RunInfoList(runs) => {
            assert!(runs.iter().any(|r| r.info.id.as_str() == "query-active-1"));
            assert!(runs.iter().all(|r| r.info.status == RunStatus::Active));
        }
        _ => panic!("Expected RunInfoList"),
    }

    // Query by Completed status
    let output = executor.execute(Command::RunQueryByStatus {
        state: RunStatus::Completed,
    }).unwrap();
    match output {
        Output::RunInfoList(runs) => {
            assert!(runs.iter().any(|r| r.info.id.as_str() == "query-completed-1"));
            assert!(runs.iter().all(|r| r.info.status == RunStatus::Completed));
        }
        _ => panic!("Expected RunInfoList"),
    }
}

#[test]
fn run_query_by_tag() {
    let executor = create_executor();

    let run_id = match executor.execute(Command::RunCreate {
        run_id: Some("tagged-query-run".into()),
        metadata: None,
    }).unwrap() {
        Output::RunWithVersion { info, .. } => info.id,
        _ => panic!("Expected RunWithVersion"),
    };

    executor.execute(Command::RunAddTags {
        run: run_id,
        tags: vec!["important".into(), "test".into()],
    }).unwrap();

    // Query by tag
    let output = executor.execute(Command::RunQueryByTag {
        tag: "important".into(),
    }).unwrap();
    match output {
        Output::RunInfoList(runs) => {
            assert!(runs.iter().any(|r| r.info.id.as_str() == "tagged-query-run"));
        }
        _ => panic!("Expected RunInfoList"),
    }
}

#[test]
fn run_count() {
    let executor = create_executor();

    // Get initial count
    let initial_count = match executor.execute(Command::RunCount {
        status: None,
    }).unwrap() {
        Output::Uint(n) => n,
        _ => panic!("Expected Uint"),
    };

    // Create some runs
    executor.execute(Command::RunCreate {
        run_id: Some("count-run-1".into()),
        metadata: None,
    }).unwrap();
    executor.execute(Command::RunCreate {
        run_id: Some("count-run-2".into()),
        metadata: None,
    }).unwrap();

    // Count should increase
    let output = executor.execute(Command::RunCount {
        status: None,
    }).unwrap();
    match output {
        Output::Uint(n) => assert_eq!(n, initial_count + 2),
        _ => panic!("Expected Uint"),
    }

    // Count by status
    let output = executor.execute(Command::RunCount {
        status: Some(RunStatus::Active),
    }).unwrap();
    match output {
        Output::Uint(n) => assert!(n >= 2),
        _ => panic!("Expected Uint"),
    }
}

#[test]
fn run_search() {
    let executor = create_executor();

    // Create runs with searchable metadata
    executor.execute(Command::RunCreate {
        run_id: Some("search-ml-training".into()),
        metadata: Some(Value::Object([
            ("type".to_string(), Value::String("training".into())),
            ("model".to_string(), Value::String("gpt".into())),
        ].into_iter().collect())),
    }).unwrap();

    executor.execute(Command::RunCreate {
        run_id: Some("search-ml-inference".into()),
        metadata: Some(Value::Object([
            ("type".to_string(), Value::String("inference".into())),
            ("model".to_string(), Value::String("gpt".into())),
        ].into_iter().collect())),
    }).unwrap();

    // Search runs - the search implementation may vary
    let output = executor.execute(Command::RunSearch {
        query: "ml".into(),
        limit: Some(10),
    }).unwrap();

    // Just verify we get a list back (search behavior depends on implementation)
    match output {
        Output::RunInfoList(_runs) => {
            // Search found some runs (exact behavior depends on search implementation)
        }
        _ => panic!("Expected RunInfoList"),
    }
}

#[test]
fn run_create_child_and_hierarchy() {
    let executor = create_executor();

    // Create parent run
    let parent_id = match executor.execute(Command::RunCreate {
        run_id: Some("parent-run".into()),
        metadata: None,
    }).unwrap() {
        Output::RunWithVersion { info, .. } => info.id,
        _ => panic!("Expected RunWithVersion"),
    };

    // Create child runs
    let child1_id = match executor.execute(Command::RunCreateChild {
        parent: parent_id.clone(),
        metadata: Some(Value::String("child 1".into())),
    }).unwrap() {
        Output::RunWithVersion { info, .. } => info.id,
        _ => panic!("Expected RunWithVersion"),
    };

    let child2_id = match executor.execute(Command::RunCreateChild {
        parent: parent_id.clone(),
        metadata: Some(Value::String("child 2".into())),
    }).unwrap() {
        Output::RunWithVersion { info, .. } => info.id,
        _ => panic!("Expected RunWithVersion"),
    };

    // Get children
    let output = executor.execute(Command::RunGetChildren {
        parent: parent_id.clone(),
    }).unwrap();
    match output {
        Output::RunInfoList(children) => {
            assert_eq!(children.len(), 2);
            let child_ids: Vec<_> = children.iter().map(|c| c.info.id.as_str()).collect();
            assert!(child_ids.contains(&child1_id.as_str()));
            assert!(child_ids.contains(&child2_id.as_str()));
        }
        _ => panic!("Expected RunInfoList"),
    }

    // Get parent from child
    let output = executor.execute(Command::RunGetParent {
        run: child1_id,
    }).unwrap();
    match output {
        Output::MaybeRunId(Some(pid)) => {
            assert_eq!(pid.as_str(), parent_id.as_str());
        }
        _ => panic!("Expected MaybeRunId(Some)"),
    }

    // Parent has no parent
    let output = executor.execute(Command::RunGetParent {
        run: parent_id,
    }).unwrap();
    assert!(matches!(output, Output::MaybeRunId(None)));
}

#[test]
fn run_retention_policy() {
    let executor = create_executor();

    let run_id = match executor.execute(Command::RunCreate {
        run_id: Some("retention-run".into()),
        metadata: None,
    }).unwrap() {
        Output::RunWithVersion { info, .. } => info.id,
        _ => panic!("Expected RunWithVersion"),
    };

    // Set retention policy - KeepLast variant
    let policy = strata_executor::RetentionPolicyInfo::KeepLast { count: 100 };
    executor.execute(Command::RunSetRetention {
        run: run_id.clone(),
        policy: policy.clone(),
    }).unwrap();

    // Get retention policy
    let output = executor.execute(Command::RunGetRetention {
        run: run_id.clone(),
    }).unwrap();
    match output {
        Output::RetentionPolicy(retrieved) => {
            assert_eq!(retrieved, strata_executor::RetentionPolicyInfo::KeepLast { count: 100 });
        }
        _ => panic!("Expected RetentionPolicy"),
    }

    // Set retention policy - KeepFor variant
    let policy = strata_executor::RetentionPolicyInfo::KeepFor { duration_secs: 86400 };
    executor.execute(Command::RunSetRetention {
        run: run_id.clone(),
        policy,
    }).unwrap();

    let output = executor.execute(Command::RunGetRetention {
        run: run_id,
    }).unwrap();
    match output {
        Output::RetentionPolicy(retrieved) => {
            assert_eq!(retrieved, strata_executor::RetentionPolicyInfo::KeepFor { duration_secs: 86400 });
        }
        _ => panic!("Expected RetentionPolicy"),
    }
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
        run: Some(RunId::default()),
        key: "default_test".into(),
    }).unwrap();

    // Should find the value
    match output {
        Output::MaybeVersioned(Some(vv)) => {
            assert_eq!(vv.value, Value::String("value".into()));
        }
        _ => panic!("Expected to find value in default run"),
    }
}

#[test]
fn different_runs_are_isolated() {
    let executor = create_executor();

    // Create two runs with human-readable names
    let run_a = match executor.execute(Command::RunCreate {
        run_id: Some("agent-alpha".into()),
        metadata: None,
    }).unwrap() {
        Output::RunWithVersion { info, .. } => info.id,
        _ => panic!("Expected RunCreated"),
    };

    let run_b = match executor.execute(Command::RunCreate {
        run_id: Some("agent-beta".into()),
        metadata: None,
    }).unwrap() {
        Output::RunWithVersion { info, .. } => info.id,
        _ => panic!("Expected RunCreated"),
    };

    // Put in run A
    executor.execute(Command::KvPut {
        run: Some(run_a.clone()),
        key: "shared_key".into(),
        value: Value::String("run_a_value".into()),
    }).unwrap();

    // Put in run B
    executor.execute(Command::KvPut {
        run: Some(run_b.clone()),
        key: "shared_key".into(),
        value: Value::String("run_b_value".into()),
    }).unwrap();

    // Get from run A
    let output = executor.execute(Command::KvGet {
        run: Some(run_a),
        key: "shared_key".into(),
    }).unwrap();

    match output {
        Output::MaybeVersioned(Some(vv)) => {
            assert_eq!(vv.value, Value::String("run_a_value".into()));
        }
        _ => panic!("Expected run A value"),
    }

    // Get from run B
    let output = executor.execute(Command::KvGet {
        run: Some(run_b),
        key: "shared_key".into(),
    }).unwrap();

    match output {
        Output::MaybeVersioned(Some(vv)) => {
            assert_eq!(vv.value, Value::String("run_b_value".into()));
        }
        _ => panic!("Expected run B value"),
    }
}
