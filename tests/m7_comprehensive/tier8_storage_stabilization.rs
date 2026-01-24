//! Tier 8: Storage Stabilization Tests
//!
//! Tests for PrimitiveStorageExt and registry operations.

use crate::test_utils::*;
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_durability::{PrimitiveKind, WalEntryType};

/// WalEntryType registry is complete
#[test]
fn test_wal_entry_type_registry() {
    // Core entry types should be defined
    let types = [
        WalEntryType::KvPut,
        WalEntryType::KvDelete,
        WalEntryType::TransactionCommit,
        WalEntryType::TransactionAbort,
        WalEntryType::RunBegin,
        WalEntryType::RunEnd,
    ];

    // Each should have a unique byte representation
    let bytes: Vec<u8> = types.iter().map(|t| (*t).into()).collect();
    for i in 0..bytes.len() {
        for j in (i + 1)..bytes.len() {
            assert_ne!(
                bytes[i], bytes[j],
                "Entry types {:?} and {:?} have same byte",
                types[i], types[j]
            );
        }
    }
}

/// PrimitiveKind enumeration
#[test]
fn test_primitive_kind_enumeration() {
    // Core primitive kinds should be defined
    let kinds = [
        PrimitiveKind::Kv,
        PrimitiveKind::Json,
        PrimitiveKind::Event,
        PrimitiveKind::State,
    ];

    // Each kind should have a unique entry type range
    let ranges: Vec<_> = kinds.iter().map(|k| k.entry_type_range()).collect();
    for i in 0..ranges.len() {
        for j in (i + 1)..ranges.len() {
            // Ranges should not overlap
            let (start_i, end_i) = ranges[i];
            let (start_j, end_j) = ranges[j];
            assert!(
                end_i < start_j || end_j < start_i,
                "Primitive kinds {:?} and {:?} have overlapping ranges",
                kinds[i],
                kinds[j]
            );
        }
    }
}

/// KV primitive storage operations
#[test]
fn test_kv_primitive_operations() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Put
    kv.put(&run_id, "key", Value::String("value".into()))
        .unwrap();

    // Get
    let value = kv.get(&run_id, "key").unwrap().map(|v| v.value);
    assert_eq!(value, Some(Value::String("value".into())));

    // Delete
    kv.delete(&run_id, "key").unwrap();

    let value = kv.get(&run_id, "key").unwrap();
    assert!(value.is_none());
}

/// KV primitive list operation
#[test]
fn test_kv_primitive_list() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Write multiple keys
    for i in 0..10 {
        kv.put(&run_id, &format!("k{}", i), Value::Int(i)).unwrap();
    }

    // List
    let keys = kv.list(&run_id, None).unwrap();
    assert_eq!(keys.len(), 10);
}

/// Storage handles various value types
#[test]
fn test_storage_value_types() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // String
    kv.put(&run_id, "str", Value::String("hello".into()))
        .unwrap();
    assert_eq!(
        kv.get(&run_id, "str").unwrap().map(|v| v.value),
        Some(Value::String("hello".into()))
    );

    // Int
    kv.put(&run_id, "int", Value::Int(-42)).unwrap();
    assert_eq!(kv.get(&run_id, "int").unwrap().map(|v| v.value), Some(Value::Int(-42)));

    // Float
    kv.put(&run_id, "float", Value::Float(3.14159)).unwrap();
    if let Some(versioned) = kv.get(&run_id, "float").unwrap() {
        if let Value::Float(f) = versioned.value {
            assert!((f - 3.14159).abs() < 0.0001);
        }
    }

    // Bool
    kv.put(&run_id, "bool", Value::Bool(true)).unwrap();
    assert_eq!(kv.get(&run_id, "bool").unwrap().map(|v| v.value), Some(Value::Bool(true)));

    // Null
    kv.put(&run_id, "null", Value::Null).unwrap();
    assert_eq!(kv.get(&run_id, "null").unwrap().map(|v| v.value), Some(Value::Null));
}

/// Storage handles large values
#[test]
fn test_storage_large_values() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Large string
    let large = "x".repeat(100_000);
    kv.put(&run_id, "large", Value::String(large.clone()))
        .unwrap();

    if let Some(versioned) = kv.get(&run_id, "large").unwrap() {
        if let Value::String(s) = versioned.value {
            assert_eq!(s.len(), large.len());
        }
    }
}

/// Storage handles many keys
#[test]
fn test_storage_many_keys() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Write many keys
    for i in 0..1000 {
        kv.put(&run_id, &format!("key_{:04}", i), Value::Int(i))
            .unwrap();
    }

    // All should be readable
    for i in 0..1000 {
        let value = kv.get(&run_id, &format!("key_{:04}", i)).unwrap().map(|v| v.value);
        assert_eq!(value, Some(Value::Int(i)));
    }
}

/// Storage run isolation
#[test]
fn test_storage_run_isolation() {
    let test_db = TestDb::new_in_memory();
    let run_id1 = RunId::new();
    let run_id2 = RunId::new();

    let kv = test_db.kv();

    // Write to different runs
    kv.put(&run_id1, "shared_key", Value::String("run1".into()))
        .unwrap();
    kv.put(&run_id2, "shared_key", Value::String("run2".into()))
        .unwrap();

    // Reads are isolated
    assert_eq!(
        kv.get(&run_id1, "shared_key").unwrap().map(|v| v.value),
        Some(Value::String("run1".into()))
    );
    assert_eq!(
        kv.get(&run_id2, "shared_key").unwrap().map(|v| v.value),
        Some(Value::String("run2".into()))
    );
}

/// Storage overwrite semantics
#[test]
fn test_storage_overwrite() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    kv.put(&run_id, "key", Value::Int(1)).unwrap();
    kv.put(&run_id, "key", Value::Int(2)).unwrap();
    kv.put(&run_id, "key", Value::Int(3)).unwrap();

    assert_eq!(kv.get(&run_id, "key").unwrap().map(|v| v.value), Some(Value::Int(3)));
}

/// Storage delete semantics
#[test]
fn test_storage_delete_semantics() {
    let test_db = TestDb::new_in_memory();
    let run_id = test_db.run_id;

    let kv = test_db.kv();

    // Delete non-existent key should succeed
    kv.delete(&run_id, "nonexistent").unwrap();

    // Create, delete, recreate
    kv.put(&run_id, "key", Value::Int(1)).unwrap();
    kv.delete(&run_id, "key").unwrap();
    assert!(kv.get(&run_id, "key").unwrap().is_none());

    kv.put(&run_id, "key", Value::Int(2)).unwrap();
    assert_eq!(kv.get(&run_id, "key").unwrap().map(|v| v.value), Some(Value::Int(2)));
}
