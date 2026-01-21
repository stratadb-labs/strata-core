//! KVStore Semantic Invariant Tests
//!
//! KVStore is lower risk but still needs verification for:
//! - Read-your-writes consistency
//! - Delete visibility
//! - TTL expiration behavior
//! - Run isolation

use super::*;
use strata_core::types::RunId;
use strata_core::value::Value;
use strata_primitives::KVStore;

/// Read your own writes within same session
#[test]
fn kv_read_your_writes() {
    test_across_modes("kv_read_your_writes", |db| {
        let kv = KVStore::new(db);
        let run_id = RunId::new();

        // Write
        kv.put(&run_id, "key", Value::String("value".to_string()))
            .unwrap();

        // Immediate read should see the write
        let v = kv.get(&run_id, "key").unwrap();
        v.map(|x| x.value) == Some(Value::String("value".to_string()))
    });
}

/// Put overwrites existing value
#[test]
fn kv_put_overwrites() {
    test_across_modes("kv_put_overwrites", |db| {
        let kv = KVStore::new(db);
        let run_id = RunId::new();

        kv.put(&run_id, "key", Value::I64(1)).unwrap();
        kv.put(&run_id, "key", Value::I64(2)).unwrap();
        kv.put(&run_id, "key", Value::I64(3)).unwrap();

        let v = kv.get(&run_id, "key").unwrap();
        v.map(|x| x.value) == Some(Value::I64(3))
    });
}

/// Delete removes key
#[test]
fn kv_delete_removes() {
    test_across_modes("kv_delete_removes", |db| {
        let kv = KVStore::new(db);
        let run_id = RunId::new();

        kv.put(&run_id, "key", Value::I64(42)).unwrap();
        assert!(kv.exists(&run_id, "key").unwrap());

        let deleted = kv.delete(&run_id, "key").unwrap();
        assert!(deleted);

        assert!(!kv.exists(&run_id, "key").unwrap());
        assert!(kv.get(&run_id, "key").unwrap().is_none());

        true
    });
}

/// Delete returns false for non-existent key
#[test]
fn kv_delete_nonexistent() {
    test_across_modes("kv_delete_nonexistent", |db| {
        let kv = KVStore::new(db);
        let run_id = RunId::new();

        let deleted = kv.delete(&run_id, "never_existed").unwrap();
        !deleted
    });
}

/// Exists reflects current state
#[test]
fn kv_exists_accuracy() {
    test_across_modes("kv_exists_accuracy", |db| {
        let kv = KVStore::new(db);
        let run_id = RunId::new();

        assert!(!kv.exists(&run_id, "key").unwrap());

        kv.put(&run_id, "key", Value::I64(1)).unwrap();
        assert!(kv.exists(&run_id, "key").unwrap());

        kv.delete(&run_id, "key").unwrap();
        assert!(!kv.exists(&run_id, "key").unwrap());

        true
    });
}

/// Get returns None for non-existent key
#[test]
fn kv_get_nonexistent() {
    test_across_modes("kv_get_nonexistent", |db| {
        let kv = KVStore::new(db);
        let run_id = RunId::new();

        let v = kv.get(&run_id, "does_not_exist").unwrap();
        v.is_none()
    });
}

/// Keys are isolated per run
#[test]
fn kv_run_isolation() {
    test_across_modes("kv_run_isolation", |db| {
        let kv = KVStore::new(db);
        let run_a = RunId::new();
        let run_b = RunId::new();

        // Same key name in different runs
        kv.put(&run_a, "shared_key", Value::I64(100)).unwrap();
        kv.put(&run_b, "shared_key", Value::I64(200)).unwrap();

        let val_a = kv.get(&run_a, "shared_key").unwrap().map(|x| x.value);
        let val_b = kv.get(&run_b, "shared_key").unwrap().map(|x| x.value);

        assert_eq!(val_a, Some(Value::I64(100)));
        assert_eq!(val_b, Some(Value::I64(200)));

        // Delete in one run doesn't affect other
        kv.delete(&run_a, "shared_key").unwrap();

        assert!(kv.get(&run_a, "shared_key").unwrap().is_none());
        assert_eq!(kv.get(&run_b, "shared_key").unwrap().map(|x| x.value), Some(Value::I64(200)));

        true
    });
}

/// List returns all keys
#[test]
fn kv_list_keys() {
    test_across_modes("kv_list_keys", |db| {
        let kv = KVStore::new(db);
        let run_id = RunId::new();

        kv.put(&run_id, "apple", Value::I64(1)).unwrap();
        kv.put(&run_id, "banana", Value::I64(2)).unwrap();
        kv.put(&run_id, "cherry", Value::I64(3)).unwrap();

        let mut keys = kv.list(&run_id, None).unwrap();
        keys.sort();

        assert_eq!(keys.len(), 3);
        assert!(keys.contains(&"apple".to_string()));
        assert!(keys.contains(&"banana".to_string()));
        assert!(keys.contains(&"cherry".to_string()));

        true
    });
}

/// List with prefix filters correctly
#[test]
fn kv_list_with_prefix() {
    test_across_modes("kv_list_with_prefix", |db| {
        let kv = KVStore::new(db);
        let run_id = RunId::new();

        kv.put(&run_id, "user:1", Value::I64(1)).unwrap();
        kv.put(&run_id, "user:2", Value::I64(2)).unwrap();
        kv.put(&run_id, "item:1", Value::I64(3)).unwrap();
        kv.put(&run_id, "item:2", Value::I64(4)).unwrap();

        let users = kv.list(&run_id, Some("user:")).unwrap();
        let items = kv.list(&run_id, Some("item:")).unwrap();

        assert_eq!(users.len(), 2);
        assert_eq!(items.len(), 2);

        for key in &users {
            assert!(key.starts_with("user:"));
        }

        for key in &items {
            assert!(key.starts_with("item:"));
        }

        true
    });
}

/// Get many returns correct values
#[test]
fn kv_get_many() {
    test_across_modes("kv_get_many", |db| {
        let kv = KVStore::new(db);
        let run_id = RunId::new();

        kv.put(&run_id, "a", Value::I64(1)).unwrap();
        kv.put(&run_id, "b", Value::I64(2)).unwrap();
        kv.put(&run_id, "c", Value::I64(3)).unwrap();

        let values = kv.get_many(&run_id, &["a", "b", "missing", "c"]).unwrap();

        assert_eq!(values.len(), 4);
        assert_eq!(values[0].as_ref().map(|x| x.value.clone()), Some(Value::I64(1)));
        assert_eq!(values[1].as_ref().map(|x| x.value.clone()), Some(Value::I64(2)));
        assert_eq!(values[2], None); // missing
        assert_eq!(values[3].as_ref().map(|x| x.value.clone()), Some(Value::I64(3)));

        true
    });
}

/// Transaction provides isolation
#[test]
fn kv_transaction_isolation() {
    test_across_modes("kv_transaction_isolation", |db| {
        let kv = KVStore::new(db);
        let run_id = RunId::new();

        // Setup initial state
        kv.put(&run_id, "x", Value::I64(10)).unwrap();

        // Transaction that reads and writes
        let result = kv.transaction(&run_id, |txn| {
            let v = txn.get("x")?.unwrap();
            match v {
                Value::I64(n) => {
                    txn.put("x", Value::I64(n + 1))?;
                    txn.put("y", Value::I64(n * 2))?;
                    Ok(n)
                }
                _ => Ok(0),
            }
        });

        assert_eq!(result.unwrap(), 10);
        assert_eq!(kv.get(&run_id, "x").unwrap().map(|v| v.value), Some(Value::I64(11)));
        assert_eq!(kv.get(&run_id, "y").unwrap().map(|v| v.value), Some(Value::I64(20)));

        true
    });
}

/// Different value types are preserved
#[test]
fn kv_value_types_preserved() {
    test_across_modes("kv_value_types_preserved", |db| {
        let kv = KVStore::new(db);
        let run_id = RunId::new();

        kv.put(&run_id, "int", Value::I64(42)).unwrap();
        kv.put(&run_id, "float", Value::F64(3.14)).unwrap();
        kv.put(&run_id, "string", Value::String("hello".to_string()))
            .unwrap();
        kv.put(&run_id, "bool", Value::Bool(true)).unwrap();
        kv.put(&run_id, "bytes", Value::Bytes(vec![1, 2, 3]))
            .unwrap();

        assert_eq!(kv.get(&run_id, "int").unwrap().map(|v| v.value), Some(Value::I64(42)));
        assert_eq!(kv.get(&run_id, "float").unwrap().map(|v| v.value), Some(Value::F64(3.14)));
        assert_eq!(
            kv.get(&run_id, "string").unwrap().map(|v| v.value),
            Some(Value::String("hello".to_string()))
        );
        assert_eq!(kv.get(&run_id, "bool").unwrap().map(|v| v.value), Some(Value::Bool(true)));
        assert_eq!(
            kv.get(&run_id, "bytes").unwrap().map(|v| v.value),
            Some(Value::Bytes(vec![1, 2, 3]))
        );

        true
    });
}

/// Empty string key works
#[test]
fn kv_empty_key() {
    test_across_modes("kv_empty_key", |db| {
        let kv = KVStore::new(db);
        let run_id = RunId::new();

        kv.put(&run_id, "", Value::I64(99)).unwrap();
        let v = kv.get(&run_id, "").unwrap();

        v.map(|x| x.value) == Some(Value::I64(99))
    });
}

/// Large values work
#[test]
fn kv_large_value() {
    test_across_modes("kv_large_value", |db| {
        let kv = KVStore::new(db);
        let run_id = RunId::new();

        let large_string: String = "x".repeat(100_000);
        kv.put(&run_id, "large", Value::String(large_string.clone()))
            .unwrap();

        let v = kv.get(&run_id, "large").unwrap();
        v.map(|x| x.value) == Some(Value::String(large_string))
    });
}

/// Many keys scale
#[test]
fn kv_many_keys() {
    test_across_modes_with_validation(
        "kv_many_keys",
        |db| {
            let kv = KVStore::new(db);
            let run_id = RunId::new();

            for i in 0..1000 {
                kv.put(&run_id, &format!("key_{}", i), Value::I64(i as i64))
                    .unwrap();
            }

            // Verify some random keys
            let mut found = 0;
            for i in [0, 100, 500, 999] {
                if let Some(versioned) = kv.get(&run_id, &format!("key_{}", i)).unwrap() {
                    if let Value::I64(n) = versioned.value {
                        if n == i as i64 {
                            found += 1;
                        }
                    }
                }
            }

            found
        },
        |found| *found == 4,
    );
}

#[cfg(test)]
mod kv_unit_tests {
    use super::*;

    #[test]
    fn test_basic_put_get() {
        let db = create_inmemory_db();
        let kv = KVStore::new(db);
        let run_id = RunId::new();

        kv.put(&run_id, "test", Value::I64(123)).unwrap();
        let v = kv.get(&run_id, "test").unwrap();

        assert_eq!(v.map(|x| x.value), Some(Value::I64(123)));
    }

    #[test]
    fn test_list_with_values() {
        let db = create_inmemory_db();
        let kv = KVStore::new(db);
        let run_id = RunId::new();

        kv.put(&run_id, "a", Value::I64(1)).unwrap();
        kv.put(&run_id, "b", Value::I64(2)).unwrap();

        let kvs = kv.list_with_values(&run_id, None).unwrap();
        assert_eq!(kvs.len(), 2);
    }
}
