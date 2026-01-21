//! Durability Mode Equivalence Tests
//!
//! Tests that behavior is consistent across durability modes.

use crate::test_utils::*;
use strata_core::value::Value;

/// Test in-memory and strict produce same results.
#[test]
fn test_inmemory_strict_equivalence() {
    // In-memory mode
    let inmem_db = TestDb::new_in_memory();
    let kv_inmem = inmem_db.kv();
    let run_id = inmem_db.run_id;

    kv_inmem.put(&run_id, "test", Value::I64(42)).expect("put");
    let inmem_value = kv_inmem.get(&run_id, "test").expect("get").map(|v| v.value);

    // Strict mode
    let strict_db = TestDb::new_strict();
    let kv_strict = strict_db.kv();
    let run_id_strict = strict_db.run_id;

    kv_strict.put(&run_id_strict, "test", Value::I64(42)).expect("put");
    let strict_value = kv_strict.get(&run_id_strict, "test").expect("get").map(|v| v.value);

    // Should produce same results
    assert_eq!(inmem_value, strict_value);
}

/// Test buffered and strict produce same results.
#[test]
fn test_buffered_strict_equivalence() {
    // Buffered mode
    let buffered_db = TestDb::new();
    let kv_buffered = buffered_db.kv();
    let run_id_buf = buffered_db.run_id;

    kv_buffered.put(&run_id_buf, "test", Value::I64(42)).expect("put");
    buffered_db.db.flush().expect("flush");
    let buffered_value = kv_buffered.get(&run_id_buf, "test").expect("get").map(|v| v.value);

    // Strict mode
    let strict_db = TestDb::new_strict();
    let kv_strict = strict_db.kv();
    let run_id_strict = strict_db.run_id;

    kv_strict.put(&run_id_strict, "test", Value::I64(42)).expect("put");
    let strict_value = kv_strict.get(&run_id_strict, "test").expect("get").map(|v| v.value);

    // Should produce same results
    assert_eq!(buffered_value, strict_value);
}

/// Test all primitives work in all modes.
#[test]
fn test_all_primitives_all_modes() {
    // Test each mode
    for test_db in [TestDb::new_in_memory(), TestDb::new(), TestDb::new_strict()] {
        assert_all_primitives_healthy(&test_db);
    }
}
