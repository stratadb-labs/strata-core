//! S6: Run Isolation Tests
//!
//! Invariant S6: Collections scoped to RunId.

use crate::test_utils::*;
use strata_core::types::RunId;

/// Test that different runs are isolated
#[test]
fn test_s6_different_runs_isolated() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    let run1 = RunId::new();
    let run2 = RunId::new();

    // Create same-named collection in both runs
    vector
        .create_collection(run1, "embeddings", config_minilm())
        .unwrap();
    vector
        .create_collection(run2, "embeddings", config_minilm())
        .unwrap();

    // Insert in run1
    let embedding1 = random_vector(384);
    vector
        .insert(run1, "embeddings", "key1", &embedding1, None)
        .unwrap();

    // run2 should not see run1's vectors
    let search_result = vector
        .search(run2, "embeddings", &random_vector(384), 10, None)
        .unwrap();
    assert!(search_result.is_empty(), "S6 VIOLATED: run2 sees run1's data");

    // run2's get should not find run1's key
    let get_result = vector.get(run2, "embeddings", "key1").unwrap();
    assert!(get_result.is_none(), "S6 VIOLATED: run2 can get run1's key");

    // Insert in run2
    vector
        .insert(run2, "embeddings", "key2", &random_vector(384), None)
        .unwrap();

    // run1 should not see run2's vectors
    let count1 = vector.count(run1, "embeddings").unwrap();
    assert_eq!(count1, 1, "S6 VIOLATED: run1 count affected by run2");

    let count2 = vector.count(run2, "embeddings").unwrap();
    assert_eq!(count2, 1, "S6 VIOLATED: run2 count affected by run1");
}

/// Test that delete in one run doesn't affect another
#[test]
fn test_s6_delete_in_one_run_doesnt_affect_other() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    let run1 = RunId::new();
    let run2 = RunId::new();

    vector
        .create_collection(run1, "embeddings", config_minilm())
        .unwrap();
    vector
        .create_collection(run2, "embeddings", config_minilm())
        .unwrap();

    // Insert same key in both runs
    vector
        .insert(run1, "embeddings", "key1", &random_vector(384), None)
        .unwrap();
    vector
        .insert(run2, "embeddings", "key1", &random_vector(384), None)
        .unwrap();

    // Delete from run1
    vector.delete(run1, "embeddings", "key1").unwrap();

    // run1's vector should be gone
    assert!(vector.get(run1, "embeddings", "key1").unwrap().is_none());

    // run2's vector should still exist
    let count2 = vector.count(run2, "embeddings").unwrap();
    assert_eq!(count2, 1, "S6 VIOLATED: delete in run1 affected run2");
    assert!(vector.get(run2, "embeddings", "key1").unwrap().is_some());
}

/// Test collection deletion is run-scoped
#[test]
fn test_s6_collection_delete_run_scoped() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    let run1 = RunId::new();
    let run2 = RunId::new();

    vector
        .create_collection(run1, "embeddings", config_minilm())
        .unwrap();
    vector
        .create_collection(run2, "embeddings", config_minilm())
        .unwrap();

    vector
        .insert(run1, "embeddings", "key1", &random_vector(384), None)
        .unwrap();
    vector
        .insert(run2, "embeddings", "key1", &random_vector(384), None)
        .unwrap();

    // Delete collection from run1
    vector.delete_collection(run1, "embeddings").unwrap();

    // run1's collection should be gone
    assert!(vector.get_collection(run1, "embeddings").unwrap().is_none());

    // run2's collection should still exist
    assert!(vector.get_collection(run2, "embeddings").unwrap().is_some());
    assert_eq!(vector.count(run2, "embeddings").unwrap(), 1);
}

/// Test that run isolation survives restart
#[test]
fn test_s6_run_isolation_survives_restart() {
    let mut test_db = TestDb::new();

    let run1 = RunId::new();
    let run2 = RunId::new();

    {
        let vector = test_db.vector();
        vector
            .create_collection(run1, "embeddings", config_minilm())
            .unwrap();
        vector
            .create_collection(run2, "embeddings", config_minilm())
            .unwrap();

        vector
            .insert(run1, "embeddings", "key1", &random_vector(384), None)
            .unwrap();
        vector
            .insert(run2, "embeddings", "key2", &random_vector(384), None)
            .unwrap();
    }

    // Restart
    test_db.reopen();

    let vector = test_db.vector();

    // Run isolation should be preserved
    assert!(vector.get(run1, "embeddings", "key1").unwrap().is_some());
    assert!(vector.get(run1, "embeddings", "key2").unwrap().is_none());

    assert!(vector.get(run2, "embeddings", "key2").unwrap().is_some());
    assert!(vector.get(run2, "embeddings", "key1").unwrap().is_none());
}

/// Test many runs are isolated
#[test]
fn test_s6_many_runs_isolated() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    let runs: Vec<RunId> = (0..10).map(|_| RunId::new()).collect();

    // Create collection and insert unique key in each run
    for (i, run_id) in runs.iter().enumerate() {
        vector
            .create_collection(*run_id, "embeddings", config_minilm())
            .unwrap();
        vector
            .insert(
                *run_id,
                "embeddings",
                &format!("run_{}_key", i),
                &random_vector(384),
                None,
            )
            .unwrap();
    }

    // Verify each run only sees its own key
    for (i, run_id) in runs.iter().enumerate() {
        let count = vector.count(*run_id, "embeddings").unwrap();
        assert_eq!(count, 1, "S6 VIOLATED: Run {} has wrong count", i);

        // Should find its own key
        let own_key = format!("run_{}_key", i);
        assert!(
            vector.get(*run_id, "embeddings", &own_key).unwrap().is_some(),
            "S6 VIOLATED: Run {} can't find own key",
            i
        );

        // Should not find other runs' keys
        for (j, _) in runs.iter().enumerate() {
            if i != j {
                let other_key = format!("run_{}_key", j);
                assert!(
                    vector.get(*run_id, "embeddings", &other_key).unwrap().is_none(),
                    "S6 VIOLATED: Run {} can see run {}'s key",
                    i,
                    j
                );
            }
        }
    }
}
