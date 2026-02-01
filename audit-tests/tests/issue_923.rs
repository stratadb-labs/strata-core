//! Audit test for issue #923: vector_upsert auto-creates collection with hardcoded Cosine metric
//! Verdict: FIXED
//!
//! Previously, vector_upsert auto-created a collection if it did not exist, always using
//! DistanceMetric::Cosine regardless of user intent. This was removed so that
//! VectorUpsert now requires an explicit VectorCreateCollection call first.
//!
//! If you call VectorUpsert on a non-existent collection, it returns
//! CollectionNotFound instead of silently creating a Cosine collection.

use strata_executor::{Command, DistanceMetric, Output};

/// Verify that VectorUpsert on a non-existent collection now returns an error
/// instead of silently auto-creating with Cosine metric.
#[test]
fn issue_923_upsert_without_create_fails() {
    let db = strata_engine::database::Database::cache().unwrap();
    let executor = strata_executor::Executor::new(db);

    let branch = strata_executor::BranchId::from("default");

    // Upsert into a collection that does not exist.
    // Previously this auto-created with Cosine metric; now it should fail.
    let result = executor.execute(Command::VectorUpsert {
        branch: Some(branch.clone()),
        collection: "auto_created".into(),
        key: "vec1".into(),
        vector: vec![1.0, 0.0, 0.0],
        metadata: None,
    });

    assert!(
        result.is_err(),
        "VectorUpsert on a non-existent collection should fail with CollectionNotFound. \
         The auto-create behavior (issue #923) has been removed."
    );
}

/// Verify that explicit VectorCreateCollection with a specific metric works correctly,
/// and that VectorUpsert respects the explicitly created collection.
#[test]
fn issue_923_explicit_create_preserves_metric() {
    let db = strata_engine::database::Database::cache().unwrap();
    let executor = strata_executor::Executor::new(db);

    let branch = strata_executor::BranchId::from("default");

    // Explicitly create the collection with Euclidean metric
    executor
        .execute(Command::VectorCreateCollection {
            branch: Some(branch.clone()),
            collection: "euclidean_col".into(),
            dimension: 3,
            metric: DistanceMetric::Euclidean,
        })
        .unwrap();

    // Now upsert -- should succeed because collection exists
    executor
        .execute(Command::VectorUpsert {
            branch: Some(branch.clone()),
            collection: "euclidean_col".into(),
            key: "vec1".into(),
            vector: vec![1.0, 2.0, 3.0],
            metadata: None,
        })
        .unwrap();

    // Verify the metric is Euclidean (not silently overridden to Cosine)
    let result = executor
        .execute(Command::VectorListCollections {
            branch: Some(branch.clone()),
        })
        .unwrap();

    match result {
        Output::VectorCollectionList(collections) => {
            let col = collections
                .iter()
                .find(|c| c.name == "euclidean_col")
                .expect("Collection should exist");

            assert_eq!(
                col.metric,
                DistanceMetric::Euclidean,
                "Explicitly created collection should keep its Euclidean metric."
            );
        }
        other => panic!("Expected VectorCollectionList, got: {:?}", other),
    }
}

/// Verify that VectorUpsert works correctly with an explicitly created DotProduct collection.
#[test]
fn issue_923_explicit_dotproduct_collection() {
    let db = strata_engine::database::Database::cache().unwrap();
    let executor = strata_executor::Executor::new(db);

    let branch = strata_executor::BranchId::from("default");

    // Create with DotProduct
    executor
        .execute(Command::VectorCreateCollection {
            branch: Some(branch.clone()),
            collection: "dotprod_col".into(),
            dimension: 4,
            metric: DistanceMetric::DotProduct,
        })
        .unwrap();

    // Upsert should succeed on the explicitly created collection
    let result = executor.execute(Command::VectorUpsert {
        branch: Some(branch.clone()),
        collection: "dotprod_col".into(),
        key: "vec1".into(),
        vector: vec![1.0, 0.0, 0.0, 0.0],
        metadata: None,
    });

    assert!(
        result.is_ok(),
        "VectorUpsert should succeed on an explicitly created collection"
    );

    // Verify the original metric is preserved
    let list = executor
        .execute(Command::VectorListCollections {
            branch: Some(branch.clone()),
        })
        .unwrap();

    match list {
        Output::VectorCollectionList(cols) => {
            let col = cols.iter().find(|c| c.name == "dotprod_col").unwrap();
            assert_eq!(
                col.metric,
                DistanceMetric::DotProduct,
                "DotProduct metric should be preserved"
            );
        }
        _ => panic!("Expected VectorCollectionList"),
    }
}
