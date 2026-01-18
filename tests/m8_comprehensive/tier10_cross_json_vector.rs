//! Tier 10: Cross-Primitive JSON + Vector Tests
//!
//! Note: These tests are currently simplified. The JsonStore API uses different types
//! (JsonDocId, JsonPath, JsonValue) that require more complex setup.
//! Cross-primitive durability is tested in tier10_cross_kv_vector.rs.

use crate::test_utils::*;

#[test]
fn test_cross_primitive_placeholder() {
    // Placeholder test - cross-primitive durability is tested in other tier10 files
    let test_db = TestDb::new_strict();
    let vector = test_db.vector();

    vector.create_collection(test_db.run_id, "embeddings", config_minilm()).unwrap();
    vector.insert(test_db.run_id, "embeddings", "key1", &random_vector(384), None).unwrap();

    assert_eq!(vector.count(test_db.run_id, "embeddings").unwrap(), 1);
}
