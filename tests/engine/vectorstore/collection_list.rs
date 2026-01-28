//! Tier 5: Collection List Tests

use crate::common::*;

#[test]
fn test_list_collections() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "embeddings1", config_minilm()).unwrap();
    vector.create_collection(test_db.run_id, "embeddings2", config_openai_ada()).unwrap();
    vector.create_collection(test_db.run_id, "embeddings3", config_small()).unwrap();

    let collections = vector.list_collections(test_db.run_id).unwrap();
    let names: Vec<&str> = collections.iter().map(|c| c.name.as_str()).collect();

    assert!(names.contains(&"embeddings1"));
    assert!(names.contains(&"embeddings2"));
    assert!(names.contains(&"embeddings3"));
}

#[test]
fn test_list_collections_empty() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    let collections = vector.list_collections(test_db.run_id).unwrap();
    assert!(collections.is_empty());
}

#[test]
fn test_list_collections_after_delete() {
    let test_db = TestDb::new();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "col1", config_minilm()).unwrap();
    vector.create_collection(test_db.run_id, "col2", config_minilm()).unwrap();

    assert_eq!(vector.list_collections(test_db.run_id).unwrap().len(), 2);

    vector.delete_collection(test_db.run_id, "col1").unwrap();

    let collections = vector.list_collections(test_db.run_id).unwrap();
    assert_eq!(collections.len(), 1);
    assert_eq!(collections[0].name, "col2");
}

#[test]
fn test_list_collections_survives_restart() {
    let mut test_db = TestDb::new_strict();
    let run_id = test_db.run_id;

    {
        let vector = test_db.vector();
        vector.create_collection(run_id, "col1", config_minilm()).unwrap();
        vector.create_collection(run_id, "col2", config_minilm()).unwrap();
    }

    test_db.reopen();

    let collections = test_db.vector().list_collections(run_id).unwrap();
    assert_eq!(collections.len(), 2);
}
