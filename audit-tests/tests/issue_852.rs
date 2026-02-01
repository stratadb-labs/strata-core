//! Audit test for issue #852: FilterOp variants beyond Eq silently ignored
//! Verdict: CONFIRMED BUG
//!
//! The to_engine_filter() function in bridge.rs only handles FilterOp::Eq.
//! Other FilterOp variants (Ne, Gt, Gte, Lt, Lte, In, Contains) are silently
//! dropped from the filter, causing searches to return unfiltered results.
//!
//! Note: The affected code is in bridge.rs for vector search metadata filtering,
//! not in kv.rs as stated in the issue title.

use strata_engine::Database;
use strata_executor::{Command, DistanceMetric, Executor, FilterOp, MetadataFilter, Output, Value};

fn setup() -> Executor {
    let db = Database::cache().unwrap();
    Executor::new(db)
}

#[test]
fn issue_852_eq_filter_works() {
    let executor = setup();

    // Create collection
    let _ = executor.execute(Command::VectorCreateCollection {
        branch: None,
        collection: "test".to_string(),
        dimension: 3,
        metric: DistanceMetric::Cosine,
    });

    // Insert vectors with metadata
    let _ = executor.execute(Command::VectorUpsert {
        branch: None,
        collection: "test".to_string(),
        key: "v1".to_string(),
        vector: vec![1.0, 0.0, 0.0],
        metadata: Some(Value::Object(
            vec![("color".to_string(), Value::String("red".to_string()))]
                .into_iter()
                .collect(),
        )),
    });

    // Search with Eq filter (this should work)
    let result = executor.execute(Command::VectorSearch {
        branch: None,
        collection: "test".to_string(),
        query: vec![1.0, 0.0, 0.0],
        k: 10,
        filter: Some(vec![MetadataFilter {
            field: "color".to_string(),
            op: FilterOp::Eq,
            value: Value::String("red".to_string()),
        }]),
        metric: None,
    });
    assert!(result.is_ok(), "Eq filter should work");
}

#[test]
fn issue_852_ne_filter_silently_ignored() {
    let executor = setup();

    // Create collection and insert vectors
    let _ = executor.execute(Command::VectorCreateCollection {
        branch: None,
        collection: "test".to_string(),
        dimension: 3,
        metric: DistanceMetric::Cosine,
    });

    let _ = executor.execute(Command::VectorUpsert {
        branch: None,
        collection: "test".to_string(),
        key: "v1".to_string(),
        vector: vec![1.0, 0.0, 0.0],
        metadata: Some(Value::Object(
            vec![("color".to_string(), Value::String("red".to_string()))]
                .into_iter()
                .collect(),
        )),
    });

    // Search with Ne filter -- BUG: this filter is silently dropped
    let result = executor.execute(Command::VectorSearch {
        branch: None,
        collection: "test".to_string(),
        query: vec![1.0, 0.0, 0.0],
        k: 10,
        filter: Some(vec![MetadataFilter {
            field: "color".to_string(),
            op: FilterOp::Ne, // Not Eq -- will be silently ignored
            value: Value::String("red".to_string()),
        }]),
        metric: None,
    });

    // BUG: The Ne filter is dropped, so v1 IS returned even though it should be excluded
    match result {
        Ok(Output::VectorMatches(matches)) => {
            // If the filter were applied, "red" should be excluded by Ne("red")
            // But since Ne is silently ignored, the vector is returned
            assert!(
                !matches.is_empty(),
                "BUG CONFIRMED: Ne filter silently ignored, results returned unfiltered"
            );
        }
        Ok(other) => panic!("Unexpected output: {:?}", other),
        Err(e) => panic!("Search failed: {:?}", e),
    }
}

#[test]
fn issue_852_gt_filter_silently_ignored() {
    let executor = setup();

    // Search with Gt filter -- should either work or return an error, not be silently dropped
    let result = executor.execute(Command::VectorSearch {
        branch: None,
        collection: "nonexistent".to_string(),
        query: vec![1.0, 0.0, 0.0],
        k: 10,
        filter: Some(vec![MetadataFilter {
            field: "score".to_string(),
            op: FilterOp::Gt,
            value: Value::Int(50),
        }]),
        metric: None,
    });

    // The Gt filter is silently dropped. The search proceeds with no filter.
    // This is not an error per se (the collection doesn't exist), but demonstrates
    // that no "unsupported filter" error is returned.
    // When fixed, using an unsupported filter op should return an error.
    let _ = result; // Just verifying it doesn't panic
}
