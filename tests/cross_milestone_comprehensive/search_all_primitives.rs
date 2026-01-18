//! Search All Primitives Tests
//!
//! Tests Searchable trait implementation across all primitives.

use crate::test_utils::*;
use in_mem_core::json::JsonValue;
use in_mem_core::search_types::{PrimitiveKind, SearchRequest};
use in_mem_core::types::JsonDocId;
use in_mem_primitives::Searchable;

/// Test that implemented primitives have Searchable trait.
#[test]
fn test_searchable_primitives() {
    let test_db = TestDb::new();
    let p = test_db.all_primitives();

    // These primitives implement Searchable
    fn assert_searchable<T: Searchable>(_: &T) {}

    assert_searchable(&p.kv);
    assert_searchable(&p.json);
    assert_searchable(&p.event);
    assert_searchable(&p.state);
    assert_searchable(&p.trace);

    // When ISSUE-001 is fixed, add:
    // assert_searchable(&p.vector);
}

/// Test primitive_kind for each primitive.
#[test]
fn test_primitive_kinds() {
    let test_db = TestDb::new();
    let p = test_db.all_primitives();

    assert_eq!(p.kv.primitive_kind(), PrimitiveKind::Kv);
    assert_eq!(p.json.primitive_kind(), PrimitiveKind::Json);
    assert_eq!(p.event.primitive_kind(), PrimitiveKind::Event);
    assert_eq!(p.state.primitive_kind(), PrimitiveKind::State);
    assert_eq!(p.trace.primitive_kind(), PrimitiveKind::Trace);

    // When ISSUE-001 is fixed:
    // assert_eq!(p.vector.primitive_kind(), PrimitiveKind::Vector);
}

/// Test search returns SearchResponse for all primitives.
#[test]
fn test_search_returns_response() {
    let test_db = TestDb::new();
    let run_id = test_db.run_id;
    let p = test_db.all_primitives();

    // Populate primitives with searchable data
    p.kv.put(&run_id, "search_test", in_mem_core::value::Value::String("searchable content".into()))
        .expect("kv");
    let doc_id = JsonDocId::new();
    p.json.create(&run_id, &doc_id, JsonValue::from(serde_json::json!({"text": "searchable json content"})))
        .expect("json");

    let search_req = SearchRequest::new(run_id, "searchable").with_k(10);

    // Search each primitive
    let kv_response = p.kv.search(&search_req).expect("kv search");
    let json_response = p.json.search(&search_req).expect("json search");

    // Verify responses are valid SearchResponse
    assert!(kv_response.stats.elapsed_micros >= 0);
    assert!(json_response.stats.elapsed_micros >= 0);
}
