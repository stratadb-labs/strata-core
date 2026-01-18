//! Mega-Scale JSON Tests
//!
//! Tests with large JSON documents and many documents.

use crate::test_utils::*;
use in_mem_core::json::{JsonPath, JsonValue};
use in_mem_core::types::JsonDocId;

/// Test 10K JSON documents.
#[test]
fn test_10k_json_documents() {
    let test_db = TestDb::new_in_memory();
    let json = test_db.json();
    let run_id = test_db.run_id;

    // Track a specific doc_id for verification
    let mut doc_5000_id = JsonDocId::new();

    for i in 0..10_000 {
        let doc_id = JsonDocId::new();
        if i == 5000 {
            doc_5000_id = doc_id;
        }
        json.create(&run_id, &doc_id, test_json_value(i)).expect("create");

        if i % 1000 == 0 {
            eprintln!("Created {} documents", i);
        }
    }

    // Verify document exists
    let doc = json.get(&run_id, &doc_5000_id, &JsonPath::root()).expect("get");
    assert!(doc.is_some());
}

/// Test large JSON document.
#[test]
fn test_large_json_document() {
    let test_db = TestDb::new_in_memory();
    let json = test_db.json();
    let run_id = test_db.run_id;

    // Create 1MB document
    let large_doc = large_json_doc(1024 * 1024);
    let doc_id = JsonDocId::new();
    json.create(&run_id, &doc_id, large_doc).expect("create");

    // Should be retrievable
    let doc = json.get(&run_id, &doc_id, &JsonPath::root()).expect("get");
    assert!(doc.is_some());
}

/// Test deeply nested JSON.
#[test]
fn test_deep_nesting() {
    let test_db = TestDb::new_in_memory();
    let json = test_db.json();
    let run_id = test_db.run_id;

    // Create document with 50 levels of nesting (within limits)
    let deep_doc = nested_json_doc(50);
    let doc_id = JsonDocId::new();
    json.create(&run_id, &doc_id, deep_doc).expect("create");

    let doc = json.get(&run_id, &doc_id, &JsonPath::root()).expect("get");
    assert!(doc.is_some());
}
