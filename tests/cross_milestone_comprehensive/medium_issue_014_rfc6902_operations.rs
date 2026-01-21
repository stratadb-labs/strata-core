//! ISSUE-014: RFC 6902 Partial Implementation
//!
//! **Severity**: MEDIUM
//! **Location**: `/crates/core/src/json.rs:800-905`
//!
//! **Problem**: JsonPatch only supports `Set` and `Delete` operations.
//! Missing RFC 6902 operations: `add`, `test`, `move`, `copy`.
//!
//! **Impact**: Limited patch capabilities.

use crate::test_utils::*;
use strata_core::json::{JsonPath, JsonValue};
use strata_core::types::JsonDocId;

/// Test supported patch operations.
#[test]
fn test_supported_patch_operations() {
    let test_db = TestDb::new();
    let json = test_db.json();
    let run_id = test_db.run_id;

    // Create document
    let doc_id = JsonDocId::new();
    json.create(&run_id, &doc_id, JsonValue::from(serde_json::json!({"value": 1})))
        .expect("create");

    // Set operation - should work
    let value_path: JsonPath = "value".parse().expect("valid path");
    json.set(&run_id, &doc_id, &value_path, JsonValue::from(serde_json::json!(2)))
        .expect("set");

    // Delete operation - should work
    json.delete_at_path(&run_id, &doc_id, &value_path)
        .expect("delete_at_path");

    // Verify document state
    let doc = json.get(&run_id, &doc_id, &JsonPath::root()).expect("get").unwrap();
    // After delete, "value" should be gone
    assert!(doc.value.get("value").is_none());
}

/// Test missing RFC 6902 operations.
#[test]
fn test_missing_rfc6902_operations() {
    // RFC 6902 defines these operations:
    // - add: Add a value
    // - remove: Remove a value (we have this as delete)
    // - replace: Replace a value (we have this as set)
    // - move: Move a value
    // - copy: Copy a value
    // - test: Test a value

    // Current implementation only supports Set (replace) and Delete (remove)
    // When ISSUE-014 is addressed:
    // - Document which operations are intentionally unsupported
    // - Or implement the missing operations
}
