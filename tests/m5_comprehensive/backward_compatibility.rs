//! Backward Compatibility Tests
//!
//! Tests ensuring API stability and compatibility:
//! - API contract tests
//! - Error message stability
//! - Type compatibility

use crate::test_utils::*;

// =============================================================================
// API Contract Tests
// =============================================================================

/// JsonStore::new accepts Arc<Database>.
#[test]
fn test_jsonstore_new_api() {
    let db = create_test_db();
    let _store = JsonStore::new(db);
    // Should compile and not panic
}

/// All CRUD operations exist with expected signatures.
#[test]
fn test_crud_api_exists() {
    let db = create_test_db();
    let store = JsonStore::new(db);
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // Create - returns Result<u64, _> (version)
    let create_result = store.create(&run_id, &doc_id, JsonValue::null());
    assert!(create_result.is_ok());

    // Exists
    let exists_result: Result<bool, _> = store.exists(&run_id, &doc_id);
    assert!(exists_result.is_ok());

    // Get - now returns Versioned<JsonValue>
    let get_result = store.get(&run_id, &doc_id, &root());
    assert!(get_result.is_ok());

    // Get version
    let version_result: Result<Option<u64>, _> = store.get_version(&run_id, &doc_id);
    assert!(version_result.is_ok());

    // Set - returns Result<u64, _> (version)
    // First set root to an object so we can set paths on it
    let set_result = store.set(&run_id, &doc_id, &root(), JsonValue::object());
    assert!(set_result.is_ok());

    // Delete at path - may fail for nonexistent paths, test with valid path
    store
        .set(&run_id, &doc_id, &path("to_delete"), JsonValue::from(1i64))
        .unwrap();
    let delete_result = store.delete_at_path(&run_id, &doc_id, &path("to_delete"));
    assert!(delete_result.is_ok());

    // Destroy - returns Result<bool, _>
    let destroy_result = store.destroy(&run_id, &doc_id);
    assert!(destroy_result.is_ok());
}

/// JsonValue type constructors.
#[test]
fn test_jsonvalue_constructors() {
    // Null
    let _null = JsonValue::null();

    // Object
    let _obj = JsonValue::object();

    // Array
    let _arr = JsonValue::array();

    // From primitives
    let _from_bool = JsonValue::from(true);
    let _from_i64 = JsonValue::from(42i64);
    let _from_f64 = JsonValue::from(3.14f64);
    let _from_str = JsonValue::from("hello");
    let _from_string = JsonValue::from(String::from("world"));
}

/// JsonValue type checks.
#[test]
fn test_jsonvalue_type_checks() {
    let null_val = JsonValue::null();
    assert!(null_val.is_null());

    let bool_val = JsonValue::from(true);
    assert!(bool_val.is_boolean());

    let int_val = JsonValue::from(42i64);
    assert!(int_val.is_i64() || int_val.is_number());

    let float_val = JsonValue::from(3.14f64);
    assert!(float_val.is_f64() || float_val.is_number());

    let str_val = JsonValue::from("hello");
    assert!(str_val.is_string());

    let obj_val = JsonValue::object();
    assert!(obj_val.is_object());

    let arr_val = JsonValue::array();
    assert!(arr_val.is_array());
}

/// JsonValue accessors.
#[test]
fn test_jsonvalue_accessors() {
    let bool_val = JsonValue::from(true);
    assert_eq!(bool_val.as_bool(), Some(true));

    let int_val = JsonValue::from(42i64);
    assert_eq!(int_val.as_i64(), Some(42));

    let str_val = JsonValue::from("hello");
    assert_eq!(str_val.as_str(), Some("hello"));
}

/// JsonPath construction.
#[test]
fn test_jsonpath_construction() {
    // Root
    let _root = JsonPath::root();

    // Parse from string
    let _parsed: JsonPath = "a.b.c".parse().unwrap();

    // Invalid path handling
    let invalid = "a..b".parse::<JsonPath>();
    // Should either succeed (lenient) or fail (strict) - just shouldn't panic
    let _ = invalid;
}

/// JsonPath operations.
#[test]
fn test_jsonpath_operations() {
    let root = JsonPath::root();

    // Key navigation
    let with_key = root.clone().key("foo");
    assert!(!with_key.is_root());

    // Index navigation
    let with_index = root.clone().index(0);
    assert!(!with_index.is_root());

    // Chaining
    let chained = root.key("a").key("b").index(0);
    assert!(!chained.is_root());
}

// =============================================================================
// Type Compatibility Tests
// =============================================================================

/// RunId and JsonDocId are distinct types.
#[test]
fn test_id_types_distinct() {
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // They should not be accidentally interchangeable
    // (This is a compile-time check - if it compiles, types are correctly distinct)
    let _run: RunId = run_id;
    let _doc: JsonDocId = doc_id;
}

/// IDs can be cloned.
#[test]
fn test_ids_cloneable() {
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    let _run_clone = run_id.clone();
    let _doc_clone = doc_id.clone();
}

/// JsonValue can be converted to/from serde_json::Value.
#[test]
fn test_jsonvalue_serde_interop() {
    // From serde_json
    let serde_val = serde_json::json!({
        "key": "value",
        "number": 42
    });
    let json_val: JsonValue = serde_val.into();

    // Access inner
    let inner = json_val.as_inner();
    assert!(inner.is_object());

    // Into inner
    let _back: serde_json::Value = json_val.into_inner();
}

// =============================================================================
// Error Handling Contract Tests
// =============================================================================

/// Operations on non-existent document return appropriate errors.
#[test]
fn test_nonexistent_doc_errors() {
    let db = create_test_db();
    let store = JsonStore::new(db);
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // Get on non-existent - should return Ok(None) or Err, not panic
    let get_result = store.get(&run_id, &doc_id, &root());
    match get_result {
        Ok(None) => {} // Expected
        Ok(Some(_)) => panic!("Should not return Some for non-existent doc"),
        Err(_) => {} // Also acceptable
    }

    // Set on non-existent - should return Err
    let set_result = store.set(&run_id, &doc_id, &root(), JsonValue::from(1i64));
    assert!(set_result.is_err());

    // Exists on non-existent - should return Ok(false)
    let exists_result = store.exists(&run_id, &doc_id).unwrap();
    assert!(!exists_result);
}

/// Duplicate create returns error.
#[test]
fn test_duplicate_create_error() {
    let db = create_test_db();
    let store = JsonStore::new(db);
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // First create succeeds
    store.create(&run_id, &doc_id, JsonValue::null()).unwrap();

    // Second create fails
    let result = store.create(&run_id, &doc_id, JsonValue::null());
    assert!(result.is_err());
}

// =============================================================================
// Stateless Facade Contract Tests
// =============================================================================

/// JsonStore is stateless - multiple instances work correctly.
#[test]
fn test_jsonstore_stateless() {
    let db = create_test_db();
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // Create with one store instance
    {
        let store1 = JsonStore::new(db.clone());
        store1
            .create(&run_id, &doc_id, JsonValue::from(1i64))
            .unwrap();
    }

    // Modify with another store instance
    {
        let store2 = JsonStore::new(db.clone());
        store2
            .set(&run_id, &doc_id, &root(), JsonValue::from(2i64))
            .unwrap();
    }

    // Read with yet another store instance
    {
        let store3 = JsonStore::new(db.clone());
        let value = store3.get(&run_id, &doc_id, &root()).unwrap().unwrap();
        assert_eq!(value.value.as_i64(), Some(2));
    }
}

/// Multiple JsonStore instances can coexist.
#[test]
fn test_multiple_jsonstore_instances() {
    let db = create_test_db();

    let store1 = JsonStore::new(db.clone());
    let store2 = JsonStore::new(db.clone());
    let store3 = JsonStore::new(db.clone());

    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // All stores work with same data
    store1
        .create(&run_id, &doc_id, JsonValue::from(1i64))
        .unwrap();

    assert_eq!(
        store2
            .get(&run_id, &doc_id, &root())
            .unwrap()
            .unwrap().value.as_i64(),
        Some(1)
    );

    store3
        .set(&run_id, &doc_id, &root(), JsonValue::from(2i64))
        .unwrap();

    assert_eq!(
        store1
            .get(&run_id, &doc_id, &root())
            .unwrap()
            .unwrap().value.as_i64(),
        Some(2)
    );
}

// =============================================================================
// Default Value Contract Tests
// =============================================================================

/// New document version starts at 1.
#[test]
fn test_new_document_version_starts_at_1() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::null());

    let version = store.get_version(&run_id, &doc_id).unwrap().unwrap();
    assert_eq!(version, 1);
}

/// Empty object is valid initial value.
#[test]
fn test_empty_object_valid() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::object());

    assert!(store.exists(&run_id, &doc_id).unwrap());
    let value = store.get(&run_id, &doc_id, &root()).unwrap().unwrap();
    assert!(value.value.is_object());
}

/// Empty array is valid initial value.
#[test]
fn test_empty_array_valid() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::array());

    assert!(store.exists(&run_id, &doc_id).unwrap());
    let value = store.get(&run_id, &doc_id, &root()).unwrap().unwrap();
    assert!(value.value.is_array());
}

/// Null is valid initial value.
#[test]
fn test_null_valid() {
    let (_, store, run_id, doc_id) = setup_doc(JsonValue::null());

    assert!(store.exists(&run_id, &doc_id).unwrap());
    let value = store.get(&run_id, &doc_id, &root()).unwrap().unwrap();
    assert!(value.value.is_null());
}
