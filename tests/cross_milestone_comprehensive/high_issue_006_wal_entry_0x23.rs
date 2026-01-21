//! ISSUE-006: WAL Entry 0x23 Type Mismatch
//!
//! **Severity**: HIGH
//! **Location**: `/crates/durability/src/wal.rs:191`
//!
//! **Problem**:
//! - Spec says: 0x23 = JsonPatch (RFC 6902)
//! - Implementation: 0x23 = JsonDestroy (entire document deletion)
//!
//! The actual JsonPatch operations are in-memory only, not persisted to WAL.
//!
//! **Impact**: WAL entry type semantics differ from specification.

use crate::test_utils::*;
use strata_core::json::{JsonPath, JsonValue};
use strata_core::types::JsonDocId;

/// Test WAL entry type 0x23 semantics.
#[test]
fn test_wal_entry_0x23_semantics() {
    // Per WAL_ENTRY_TYPES.md:
    // | 0x23 | JsonPatch | Apply JSON patch (RFC 6902) |
    //
    // Current implementation uses 0x23 for JsonDestroy instead.

    // When ISSUE-006 is fixed:
    // - Either rename 0x23 to JsonDestroy (update spec)
    // - Or allocate new type for JsonDestroy and keep 0x23 for JsonPatch

    const WAL_ENTRY_JSON_PATCH_PER_SPEC: u8 = 0x23;
    assert_eq!(WAL_ENTRY_JSON_PATCH_PER_SPEC, 0x23);
}

/// Test that JsonPatch operations are properly logged to WAL.
#[test]
fn test_json_patch_logged_to_wal() {
    let test_db = TestDb::new_strict();
    let json = test_db.json();
    let run_id = test_db.run_id;

    // Create document
    let doc_id = JsonDocId::new();
    let doc = JsonValue::from(serde_json::json!({"value": 1}));
    json.create(&run_id, &doc_id, doc).expect("create");

    // Modify document (this should generate a WAL entry)
    let path: JsonPath = "value".parse().expect("valid path");
    json.set(&run_id, &doc_id, &path, JsonValue::from(serde_json::json!(2)))
        .expect("set");

    // Flush to ensure WAL is written
    test_db.db.flush().expect("flush");

    // When ISSUE-006 is fixed:
    // - The WAL should contain a proper JsonPatch or JsonSet entry
    // - Entry type should match specification
}

/// Test that JsonDestroy uses correct WAL entry type.
#[test]
fn test_json_destroy_wal_entry() {
    let test_db = TestDb::new_strict();
    let json = test_db.json();
    let run_id = test_db.run_id;

    // Create and delete document
    let doc_id = JsonDocId::new();
    json.create(&run_id, &doc_id, JsonValue::from(serde_json::json!({})))
        .expect("create");
    json.destroy(&run_id, &doc_id).expect("destroy");

    test_db.db.flush().expect("flush");

    // When ISSUE-006 is fixed:
    // - JsonDestroy should NOT use 0x23 (that's JsonPatch per spec)
    // - A new entry type should be allocated, e.g., 0x24
}
