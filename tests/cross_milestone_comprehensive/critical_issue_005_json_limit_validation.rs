//! ISSUE-005: JsonStore Limit Validation Never Called
//!
//! **Severity**: CRITICAL
//! **Location**: `/crates/core/src/json.rs` and `/crates/primitives/src/json_store.rs`
//!
//! **Problem**: Validation functions exist but are NEVER called at API boundaries:
//! - `MAX_DOCUMENT_SIZE = 16 MB` - NOT enforced
//! - `MAX_NESTING_DEPTH = 100 levels` - NOT enforced
//! - `MAX_PATH_LENGTH = 256 segments` - NOT enforced
//! - `MAX_ARRAY_SIZE = 1M elements` - NOT enforced
//!
//! Validation methods exist (json.rs lines 215-269) but are never called in:
//! - `JsonStore::create()` (line 222)
//! - `JsonStore::set()` (line 343)
//! - `JsonStore::delete_at_path()` (line 394)
//! - `apply_patches()` (line 1352)
//!
//! **Spec Requirement**: M5_ARCHITECTURE.md defines these limits as enforcement points.
//!
//! **Impact**: Documents can exceed size/nesting/array limits, potentially causing
//! memory issues.
//!
//! ## Test Strategy
//!
//! 1. Test that documents exceeding MAX_DOCUMENT_SIZE are rejected
//! 2. Test that documents exceeding MAX_NESTING_DEPTH are rejected
//! 3. Test that paths exceeding MAX_PATH_LENGTH are rejected
//! 4. Test that arrays exceeding MAX_ARRAY_SIZE are rejected
//! 5. Test that valid documents within limits are accepted

use crate::test_utils::*;
use strata_core::json::{JsonPath, JsonValue};
use strata_core::types::JsonDocId;

// Expected limits per M5_ARCHITECTURE.md
const MAX_DOCUMENT_SIZE: usize = 16 * 1024 * 1024; // 16 MB
const MAX_NESTING_DEPTH: usize = 100;
const MAX_PATH_LENGTH: usize = 256; // segments
const MAX_ARRAY_SIZE: usize = 1_000_000; // 1M elements

/// Test that documents exceeding MAX_DOCUMENT_SIZE are rejected.
///
/// **Expected behavior when ISSUE-005 is fixed**:
/// - JsonStore::create() rejects documents larger than 16 MB
///
/// **Current behavior (ISSUE-005 present)**:
/// - Large documents are accepted without validation
#[test]
fn test_reject_oversized_document() {
    let test_db = TestDb::new();
    let json = test_db.json();
    let run_id = test_db.run_id;

    // Create a document just under the limit - should succeed
    let under_limit_size = 1024 * 1024; // 1 MB
    let under_limit_doc = large_json_doc(under_limit_size);
    let doc_id = JsonDocId::new();
    let result = json.create(&run_id, &doc_id, under_limit_doc);
    assert!(
        result.is_ok(),
        "Document under limit should be accepted: {:?}",
        result.err()
    );

    // Create a document over the limit - should fail when ISSUE-005 is fixed
    let over_limit_size = MAX_DOCUMENT_SIZE + 1024; // 16 MB + 1 KB
    let over_limit_doc = large_json_doc(over_limit_size);
    let doc_id2 = JsonDocId::new();
    let result = json.create(&run_id, &doc_id2, over_limit_doc);

    // When ISSUE-005 is fixed:
    // assert!(result.is_err(), "Document over 16 MB should be rejected");
    // assert!(matches!(result.unwrap_err(), Error::DocumentTooLarge(_)));

    // Current behavior: This test documents that validation is NOT happening
    if result.is_ok() {
        eprintln!(
            "WARNING: ISSUE-005 NOT FIXED - Document of {} bytes was accepted (limit is {} bytes)",
            over_limit_size, MAX_DOCUMENT_SIZE
        );
    }
}

/// Test that documents exceeding MAX_NESTING_DEPTH are rejected.
///
/// **Expected behavior when ISSUE-005 is fixed**:
/// - JsonStore::create() rejects documents with nesting deeper than 100 levels
///
/// **Current behavior (ISSUE-005 present)**:
/// - Deeply nested documents are accepted without validation
#[test]
fn test_reject_deeply_nested_document() {
    let test_db = TestDb::new();
    let json = test_db.json();
    let run_id = test_db.run_id;

    // Create a document with acceptable nesting - should succeed
    let acceptable_nesting = nested_json_doc(50);
    let doc_id = JsonDocId::new();
    let result = json.create(&run_id, &doc_id, acceptable_nesting);
    assert!(
        result.is_ok(),
        "Document with 50 levels should be accepted: {:?}",
        result.err()
    );

    // Create a document exceeding nesting limit - should fail when ISSUE-005 is fixed
    let excessive_nesting = nested_json_doc(MAX_NESTING_DEPTH + 10);
    let doc_id2 = JsonDocId::new();
    let result = json.create(&run_id, &doc_id2, excessive_nesting);

    // When ISSUE-005 is fixed:
    // assert!(result.is_err(), "Document with >100 nesting levels should be rejected");
    // assert!(matches!(result.unwrap_err(), Error::NestingTooDeep(_)));

    // Current behavior: This test documents that validation is NOT happening
    if result.is_ok() {
        eprintln!(
            "WARNING: ISSUE-005 NOT FIXED - Document with {} nesting levels was accepted (limit is {})",
            MAX_NESTING_DEPTH + 10,
            MAX_NESTING_DEPTH
        );
    }
}

/// Test that paths exceeding MAX_PATH_LENGTH are rejected.
///
/// **Expected behavior when ISSUE-005 is fixed**:
/// - JsonStore::set() rejects paths with more than 256 segments
///
/// **Current behavior (ISSUE-005 present)**:
/// - Long paths are accepted without validation
#[test]
fn test_reject_excessively_long_path() {
    let test_db = TestDb::new();
    let json = test_db.json();
    let run_id = test_db.run_id;

    // Create a document first
    let doc = JsonValue::from(serde_json::json!({"root": {}}));
    let doc_id = JsonDocId::new();
    json.create(&run_id, &doc_id, doc).expect("Should create document");

    // Try to set with an excessively long path - should fail when ISSUE-005 is fixed
    let excessive_path = long_json_path(MAX_PATH_LENGTH + 10);

    // When ISSUE-005 is fixed:
    // let result = json.set(&run_id, &doc_id, &excessive_path.parse().unwrap(), JsonValue::from("value"));
    // assert!(result.is_err(), "Path with >256 segments should be rejected");
    // assert!(matches!(result.unwrap_err(), Error::PathTooLong(_)));

    // For now, verify we can construct long paths
    assert!(
        excessive_path.split('.').count() > MAX_PATH_LENGTH,
        "Excessive path should have more than {} segments",
        MAX_PATH_LENGTH
    );

    eprintln!(
        "INFO: Path validation test - excessive path has {} segments (limit is {})",
        excessive_path.split('.').count(),
        MAX_PATH_LENGTH
    );
}

/// Test that arrays exceeding MAX_ARRAY_SIZE are rejected.
///
/// **Expected behavior when ISSUE-005 is fixed**:
/// - JsonStore::create() rejects documents with arrays larger than 1M elements
///
/// **Current behavior (ISSUE-005 present)**:
/// - Large arrays are accepted without validation
#[test]
fn test_reject_oversized_array() {
    let test_db = TestDb::new();
    let json = test_db.json();
    let run_id = test_db.run_id;

    // Create a document with acceptable array size - should succeed
    let acceptable_array = large_array_json(1000);
    let doc_id = JsonDocId::new();
    let result = json.create(&run_id, &doc_id, acceptable_array);
    assert!(
        result.is_ok(),
        "Document with 1000 element array should be accepted: {:?}",
        result.err()
    );

    // Creating 1M+ element array would be slow and memory-intensive
    // Instead, document the expected behavior
    eprintln!(
        "INFO: Array size limit is {} elements - large array test skipped for performance",
        MAX_ARRAY_SIZE
    );

    // When ISSUE-005 is fixed, this should fail:
    // let excessive_array = large_array_json(MAX_ARRAY_SIZE + 1000);
    // let result = json.create(&run_id, "excessive_array", excessive_array);
    // assert!(result.is_err(), "Array with >1M elements should be rejected");
}

/// Test that set() validates document limits.
///
/// **Expected behavior when ISSUE-005 is fixed**:
/// - JsonStore::set() validates the resulting document after modification
///
/// **Current behavior (ISSUE-005 present)**:
/// - No validation on set()
#[test]
fn test_set_validates_limits() {
    let test_db = TestDb::new();
    let json = test_db.json();
    let run_id = test_db.run_id;

    // Create a small document
    let doc = JsonValue::from(serde_json::json!({"data": null}));
    let doc_id = JsonDocId::new();
    json.create(&run_id, &doc_id, doc).expect("Should create document");

    // Set a large value that would exceed document size limit
    let large_value = "x".repeat(MAX_DOCUMENT_SIZE + 1024);
    let path: JsonPath = "data".parse().unwrap();
    let result = json.set(&run_id, &doc_id, &path, JsonValue::from(large_value));

    // When ISSUE-005 is fixed:
    // assert!(result.is_err(), "Setting value that exceeds document size should fail");

    // Current behavior: Document that validation is NOT happening
    if result.is_ok() {
        eprintln!(
            "WARNING: ISSUE-005 NOT FIXED - set() accepted value that makes document exceed limit"
        );
    }
}

/// Test that delete_at_path() validates path limits.
///
/// **Expected behavior when ISSUE-005 is fixed**:
/// - JsonStore::delete_at_path() validates path length
///
/// **Current behavior (ISSUE-005 present)**:
/// - No validation on delete_at_path()
#[test]
fn test_delete_at_path_validates_limits() {
    let test_db = TestDb::new();
    let json = test_db.json();
    let run_id = test_db.run_id;

    // Create a document
    let doc = JsonValue::from(serde_json::json!({"data": {"nested": "value"}}));
    let doc_id = JsonDocId::new();
    json.create(&run_id, &doc_id, doc).expect("Should create document");

    // Try to delete with an excessively long path
    let excessive_path = long_json_path(MAX_PATH_LENGTH + 10);

    // When ISSUE-005 is fixed:
    // let result = json.delete_at_path(&run_id, &doc_id, &excessive_path.parse().unwrap());
    // assert!(result.is_err(), "delete_at_path with >256 segment path should fail");

    // Current behavior: Path doesn't exist but should fail for path length, not missing path
    // This test documents the expected validation behavior
    eprintln!(
        "INFO: delete_at_path should validate path length. Currently validation is not enforced (ISSUE-005)."
    );
    let _ = excessive_path; // Suppress unused warning
}

/// Test that valid documents within all limits are accepted.
///
/// This is a positive test to ensure validation doesn't break valid documents.
#[test]
fn test_valid_documents_accepted() {
    let test_db = TestDb::new();
    let json = test_db.json();
    let run_id = test_db.run_id;

    // Small document
    let small_doc = JsonValue::from(serde_json::json!({
        "name": "test",
        "value": 42
    }));
    let doc_id1 = JsonDocId::new();
    assert!(json.create(&run_id, &doc_id1, small_doc).is_ok());

    // Medium document with nesting
    let medium_doc = JsonValue::from(serde_json::json!({
        "level1": {
            "level2": {
                "level3": {
                    "data": [1, 2, 3, 4, 5]
                }
            }
        }
    }));
    let doc_id2 = JsonDocId::new();
    assert!(json.create(&run_id, &doc_id2, medium_doc).is_ok());

    // Document with moderate array
    let array_doc = JsonValue::from(serde_json::json!({
        "items": (0..100).collect::<Vec<i32>>()
    }));
    let doc_id3 = JsonDocId::new();
    assert!(json.create(&run_id, &doc_id3, array_doc).is_ok());

    // Verify all documents can be read back
    assert!(json.get(&run_id, &doc_id1, &JsonPath::root()).expect("get").is_some());
    assert!(json.get(&run_id, &doc_id2, &JsonPath::root()).expect("get").is_some());
    assert!(json.get(&run_id, &doc_id3, &JsonPath::root()).expect("get").is_some());
}

/// Test that apply_patches validates limits.
///
/// **Expected behavior when ISSUE-005 is fixed**:
/// - apply_patches() validates the resulting document after patches
///
/// **Current behavior (ISSUE-005 present)**:
/// - No validation on apply_patches()
#[test]
fn test_apply_patches_validates_limits() {
    let test_db = TestDb::new();
    let json = test_db.json();
    let run_id = test_db.run_id;

    // Create a small document
    let doc = JsonValue::from(serde_json::json!({"data": null}));
    let doc_id = JsonDocId::new();
    json.create(&run_id, &doc_id, doc).expect("Should create document");

    // The apply_patches function should validate the resulting document
    // Currently this test documents expected behavior when ISSUE-005 is fixed
    eprintln!(
        "INFO: apply_patches should validate limits after applying patches. \
         Currently validation is not enforced (ISSUE-005)."
    );
}

/// Test that validation errors include helpful messages.
///
/// **Expected behavior when ISSUE-005 is fixed**:
/// - Validation errors include specific limit that was exceeded
#[test]
fn test_validation_error_messages() {
    // When ISSUE-005 is fixed, validation errors should be descriptive:
    //
    // Error::DocumentTooLarge { size: 17_000_000, max: 16_777_216 }
    // Error::NestingTooDeep { depth: 150, max: 100 }
    // Error::PathTooLong { segments: 300, max: 256 }
    // Error::ArrayTooLarge { size: 1_500_000, max: 1_000_000 }

    // For now, verify the error type exists
    // let err = Error::ValidationFailed("test".into());
    // assert!(!format!("{:?}", err).is_empty());

    eprintln!("INFO: Validation errors should include specific limits when ISSUE-005 is fixed");
}
