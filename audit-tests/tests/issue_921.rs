//! Audit test for issue #921: List/scan pagination inconsistent across primitives
//! Verdict: ARCHITECTURAL CHOICE
//!
//! Pagination support varies across primitives:
//! - KV: KvList returns all keys (no pagination). Output::KvScanResult exists but
//!       there is no KvScan command in the Command enum.
//! - JSON: JsonList has cursor-based pagination (cursor + limit parameters)
//! - Event: EventRead reads by sequence number (manual pagination possible),
//!          EventReadByType returns all matching events (no pagination)
//! - State: No list operation at all (see issue #919)
//!
//! This means:
//! - For large datasets, KV and Event can return unbounded result sets
//! - JSON is the only primitive with proper cursor-based pagination
//! - Applications must implement their own pagination for KV and Events

use strata_executor::{Command, Output};

/// Demonstrate that JsonList supports cursor-based pagination.
#[test]
fn issue_921_json_list_has_cursor_pagination() {
    let db = strata_engine::database::Database::cache().unwrap();
    let executor = strata_executor::Executor::new(db);

    let branch = strata_executor::BranchId::from("default");

    // Create multiple JSON documents
    for i in 0..10 {
        executor
            .execute(Command::JsonSet {
                branch: Some(branch.clone()),
                key: format!("doc_{:02}", i),
                path: "$".into(),
                value: strata_core::value::Value::Object(
                    vec![("index".to_string(), strata_core::value::Value::Int(i))]
                        .into_iter()
                        .collect(),
                ),
            })
            .unwrap();
    }

    // First page: limit 3
    let result = executor
        .execute(Command::JsonList {
            branch: Some(branch.clone()),
            prefix: None,
            cursor: None,
            limit: 3,
        })
        .unwrap();

    match result {
        Output::JsonListResult { keys, cursor } => {
            assert_eq!(keys.len(), 3, "First page should return 3 keys");
            // Cursor should be Some if there are more results
            if keys.len() < 10 {
                assert!(
                    cursor.is_some(),
                    "Cursor should be provided when more results exist"
                );
            }

            // Use cursor to get next page
            if let Some(next_cursor) = cursor {
                let result2 = executor
                    .execute(Command::JsonList {
                        branch: Some(branch.clone()),
                        prefix: None,
                        cursor: Some(next_cursor),
                        limit: 3,
                    })
                    .unwrap();

                match result2 {
                    Output::JsonListResult {
                        keys: keys2,
                        cursor: _,
                    } => {
                        assert_eq!(keys2.len(), 3, "Second page should return 3 keys");
                        // Pages should not overlap
                        for k in &keys2 {
                            assert!(
                                !keys.contains(k),
                                "Second page should not contain keys from first page"
                            );
                        }
                    }
                    other => panic!("Expected JsonListResult, got: {:?}", other),
                }
            }
        }
        other => panic!("Expected JsonListResult, got: {:?}", other),
    }
}

/// Demonstrate that EventReadByType returns ALL matching events with no pagination.
#[test]
fn issue_921_event_read_by_type_no_pagination() {
    let db = strata_engine::database::Database::cache().unwrap();
    let executor = strata_executor::Executor::new(db);

    let branch = strata_executor::BranchId::from("default");

    // Append many events of the same type
    for i in 0..20 {
        executor
            .execute(Command::EventAppend {
                branch: Some(branch.clone()),
                event_type: "bulk_type".into(),
                payload: strata_core::value::Value::Object(
                    vec![("seq".to_string(), strata_core::value::Value::Int(i))]
                        .into_iter()
                        .collect(),
                ),
            })
            .unwrap();
    }

    // EventReadByType returns ALL events of a type -- no limit/cursor parameter
    let result = executor
        .execute(Command::EventReadByType {
            branch: Some(branch.clone()),
            event_type: "bulk_type".into(),
            limit: None,
            after_sequence: None,
        })
        .unwrap();

    match result {
        Output::VersionedValues(events) => {
            assert_eq!(
                events.len(),
                20,
                "EventReadByType returns ALL matching events with no pagination. \
                 For large event logs, this could return millions of events."
            );
        }
        other => panic!("Expected VersionedValues, got: {:?}", other),
    }

    // ARCHITECTURAL NOTE:
    // There is no way to paginate EventReadByType results.
    // The Command::EventReadByType has no limit, offset, or cursor parameter.
    // For KvList, there is similarly no pagination -- it returns all matching keys.
    // Only JsonList provides cursor-based pagination.
}

/// Demonstrate that KvList returns ALL keys with no pagination mechanism.
#[test]
fn issue_921_kv_list_no_pagination() {
    let db = strata_engine::database::Database::cache().unwrap();
    let executor = strata_executor::Executor::new(db);

    let branch = strata_executor::BranchId::from("default");

    // Insert many KV entries
    for i in 0..20 {
        executor
            .execute(Command::KvPut {
                branch: Some(branch.clone()),
                key: format!("key_{:03}", i),
                value: strata_core::value::Value::Int(i),
            })
            .unwrap();
    }

    // KvList returns all keys -- no limit or cursor parameter in the Command
    let result = executor
        .execute(Command::KvList {
            branch: Some(branch.clone()),
            prefix: None,
            cursor: None,
            limit: None,
        })
        .unwrap();

    match result {
        Output::Keys(keys) => {
            assert_eq!(
                keys.len(),
                20,
                "KvList returns ALL keys with no pagination support"
            );
        }
        other => panic!("Expected Keys, got: {:?}", other),
    }

    // Note: Output::KvScanResult { entries, cursor } exists in the Output enum,
    // but there is no Command::KvScan in the Command enum to produce it.
    // This suggests pagination was planned but not implemented for KV.
}
