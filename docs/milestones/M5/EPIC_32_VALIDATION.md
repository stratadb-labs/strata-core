# Epic 32: Validation & Non-Regression

**Goal**: Ensure correctness and maintain M4 performance

**Dependencies**: All other epics complete

**GitHub Issue**: #262

---

## Scope

- JSON unit tests
- JSON integration tests
- Non-regression benchmark suite
- Performance baseline documentation

---

## Critical Requirements

**Non-Regression**: M5 must NOT degrade M4 primitive performance:

| Operation | M4 Target | M5 Requirement |
|-----------|-----------|----------------|
| KV put (InMemory) | < 3µs | No regression |
| KV put (Buffered) | < 30µs | No regression |
| KV get (fast path) | < 5µs | No regression |
| Event append | < 10µs | No regression |
| State read | < 5µs | No regression |
| Trace append | < 15µs | No regression |

**JSON Performance Targets**:

| Operation | Document Size | Target |
|-----------|---------------|--------|
| JSON create | 1KB | < 1ms |
| JSON get at path | 1KB | < 100µs |
| JSON set at path | 1KB | < 1ms |
| JSON delete at path | 1KB | < 500µs |

---

## User Stories

| Story | Description | Priority | GitHub Issue |
|-------|-------------|----------|--------------|
| #253 | JSON Unit Tests | CRITICAL | #291 |
| #254 | JSON Integration Tests | CRITICAL | #292 |
| #255 | Non-Regression Benchmark Suite | CRITICAL | #293 |
| #256 | Performance Baseline Documentation | HIGH | #294 |

---

## Story #253: JSON Unit Tests

**File**: `crates/core/src/json_types/tests.rs` (NEW)

**Deliverable**: Comprehensive unit tests for JSON types and operations

### Test Coverage

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // ========== JsonValue Tests ==========

    mod json_value {
        use super::*;

        #[test]
        fn test_null() {
            let v = JsonValue::Null;
            assert!(v.is_null());
            assert!(!v.is_bool());
        }

        #[test]
        fn test_bool() {
            let v = JsonValue::Bool(true);
            assert!(v.is_bool());
            assert_eq!(v.as_bool(), Some(true));
        }

        #[test]
        fn test_number_int() {
            let v = JsonValue::from(42i64);
            assert!(v.is_number());
            assert_eq!(v.as_i64(), Some(42));
            assert_eq!(v.as_f64(), Some(42.0));
        }

        #[test]
        fn test_number_float() {
            let v = JsonValue::from(3.14);
            assert!(v.is_number());
            assert_eq!(v.as_f64(), Some(3.14));
            assert_eq!(v.as_i64(), None); // Float doesn't convert to int
        }

        #[test]
        fn test_string() {
            let v = JsonValue::from("hello");
            assert!(v.is_string());
            assert_eq!(v.as_str(), Some("hello"));
        }

        #[test]
        fn test_array() {
            let v = JsonValue::Array(vec![
                JsonValue::from(1),
                JsonValue::from(2),
                JsonValue::from(3),
            ]);
            assert!(v.is_array());
            assert_eq!(v.as_array().unwrap().len(), 3);
        }

        #[test]
        fn test_object() {
            let mut obj = IndexMap::new();
            obj.insert("key".to_string(), JsonValue::from("value"));
            let v = JsonValue::Object(obj);
            assert!(v.is_object());
            assert_eq!(v.as_object().unwrap().len(), 1);
        }

        #[test]
        fn test_object_preserves_insertion_order() {
            let mut obj = IndexMap::new();
            obj.insert("z".to_string(), JsonValue::from(1));
            obj.insert("a".to_string(), JsonValue::from(2));
            obj.insert("m".to_string(), JsonValue::from(3));

            let v = JsonValue::Object(obj);
            let keys: Vec<_> = v.as_object().unwrap().keys().collect();
            assert_eq!(keys, vec!["z", "a", "m"]); // Insertion order preserved
        }

        #[test]
        fn test_from_impls() {
            assert!(JsonValue::from(true).is_bool());
            assert!(JsonValue::from(42i32).is_number());
            assert!(JsonValue::from(42i64).is_number());
            assert!(JsonValue::from(3.14f64).is_number());
            assert!(JsonValue::from("str").is_string());
            assert!(JsonValue::from(String::from("owned")).is_string());
        }

        #[test]
        fn test_value_conversion_roundtrip() {
            let values = vec![
                JsonValue::Null,
                JsonValue::from(true),
                JsonValue::from(42),
                JsonValue::from("test"),
            ];

            for original in values {
                let value: Value = original.clone().into();
                let back: JsonValue = value.into();
                assert_eq!(original, back);
            }
        }
    }

    // ========== JsonPath Tests ==========

    mod json_path {
        use super::*;

        #[test]
        fn test_root() {
            let path = JsonPath::root();
            assert!(path.is_root());
            assert!(path.is_empty());
            assert_eq!(path.len(), 0);
        }

        #[test]
        fn test_parse_simple_key() {
            let path = JsonPath::parse("foo").unwrap();
            assert_eq!(path.len(), 1);
            assert_eq!(path.to_string(), "$.foo");
        }

        #[test]
        fn test_parse_nested_keys() {
            let path = JsonPath::parse("foo.bar.baz").unwrap();
            assert_eq!(path.len(), 3);
            assert_eq!(path.to_string(), "$.foo.bar.baz");
        }

        #[test]
        fn test_parse_array_index() {
            let path = JsonPath::parse("items[0]").unwrap();
            assert_eq!(path.len(), 2);
            assert_eq!(path.to_string(), "$.items[0]");
        }

        #[test]
        fn test_parse_mixed() {
            let path = JsonPath::parse("users[0].name").unwrap();
            assert_eq!(path.len(), 3);
            assert_eq!(path.to_string(), "$.users[0].name");
        }

        #[test]
        fn test_parse_with_dollar_prefix() {
            let path = JsonPath::parse("$.foo.bar").unwrap();
            assert_eq!(path.len(), 2);
            assert_eq!(path.to_string(), "$.foo.bar");
        }

        #[test]
        fn test_parse_errors() {
            assert!(JsonPath::parse("[abc]").is_err()); // Invalid index
            assert!(JsonPath::parse("foo[").is_err());   // Unclosed bracket
            assert!(JsonPath::parse("foo..bar").is_err()); // Empty key
        }

        #[test]
        fn test_builder_pattern() {
            let path = JsonPath::root().key("foo").index(0).key("bar");
            assert_eq!(path.to_string(), "$.foo[0].bar");
        }

        #[test]
        fn test_parent() {
            let path = JsonPath::parse("foo.bar.baz").unwrap();
            let parent = path.parent().unwrap();
            assert_eq!(parent.to_string(), "$.foo.bar");

            let grandparent = parent.parent().unwrap();
            assert_eq!(grandparent.to_string(), "$.foo");

            assert!(JsonPath::root().parent().is_none());
        }

        #[test]
        fn test_ancestor_descendant() {
            let foo = JsonPath::parse("foo").unwrap();
            let foo_bar = JsonPath::parse("foo.bar").unwrap();
            let baz = JsonPath::parse("baz").unwrap();

            assert!(foo.is_ancestor_of(&foo_bar));
            assert!(foo_bar.is_descendant_of(&foo));
            assert!(!foo.is_ancestor_of(&baz));
        }

        #[test]
        fn test_overlaps() {
            let root = JsonPath::root();
            let foo = JsonPath::parse("foo").unwrap();
            let foo_bar = JsonPath::parse("foo.bar").unwrap();
            let baz = JsonPath::parse("baz").unwrap();
            let items_0 = JsonPath::parse("items[0]").unwrap();
            let items_1 = JsonPath::parse("items[1]").unwrap();

            // Root overlaps with everything
            assert!(root.overlaps(&foo));
            assert!(root.overlaps(&foo_bar));

            // Equal paths overlap
            assert!(foo.overlaps(&foo));

            // Ancestor/descendant overlap
            assert!(foo.overlaps(&foo_bar));
            assert!(foo_bar.overlaps(&foo));

            // Disjoint paths don't overlap
            assert!(!foo.overlaps(&baz));
            assert!(!items_0.overlaps(&items_1));
        }
    }

    // ========== Path Operations Tests ==========

    mod path_operations {
        use super::*;

        fn sample_doc() -> JsonValue {
            let mut inner = IndexMap::new();
            inner.insert("bar".to_string(), JsonValue::from(42));

            let mut outer = IndexMap::new();
            outer.insert("foo".to_string(), JsonValue::Object(inner));
            outer.insert("items".to_string(), JsonValue::Array(vec![
                JsonValue::from(1),
                JsonValue::from(2),
                JsonValue::from(3),
            ]));

            JsonValue::Object(outer)
        }

        #[test]
        fn test_get_at_root() {
            let doc = sample_doc();
            let result = get_at_path(&doc, &JsonPath::root());
            assert_eq!(result, Some(&doc));
        }

        #[test]
        fn test_get_at_object_key() {
            let doc = sample_doc();
            let path = JsonPath::parse("foo.bar").unwrap();
            let result = get_at_path(&doc, &path);
            assert_eq!(result.and_then(|v| v.as_i64()), Some(42));
        }

        #[test]
        fn test_get_at_array_index() {
            let doc = sample_doc();
            let path = JsonPath::parse("items[1]").unwrap();
            let result = get_at_path(&doc, &path);
            assert_eq!(result.and_then(|v| v.as_i64()), Some(2));
        }

        #[test]
        fn test_get_missing_path() {
            let doc = sample_doc();
            let path = JsonPath::parse("nonexistent").unwrap();
            let result = get_at_path(&doc, &path);
            assert!(result.is_none());
        }

        #[test]
        fn test_set_at_root() {
            let mut doc = sample_doc();
            set_at_path(&mut doc, &JsonPath::root(), JsonValue::from("replaced")).unwrap();
            assert_eq!(doc.as_str(), Some("replaced"));
        }

        #[test]
        fn test_set_creates_intermediate() {
            let mut doc = JsonValue::Object(IndexMap::new());
            let path = JsonPath::parse("a.b.c").unwrap();
            set_at_path(&mut doc, &path, JsonValue::from(42)).unwrap();

            let result = get_at_path(&doc, &path);
            assert_eq!(result.and_then(|v| v.as_i64()), Some(42));
        }

        #[test]
        fn test_set_overwrites() {
            let mut doc = sample_doc();
            let path = JsonPath::parse("foo.bar").unwrap();
            set_at_path(&mut doc, &path, JsonValue::from(100)).unwrap();

            let result = get_at_path(&doc, &path);
            assert_eq!(result.and_then(|v| v.as_i64()), Some(100));
        }

        #[test]
        fn test_delete_at_path() {
            let mut doc = sample_doc();
            let path = JsonPath::parse("foo.bar").unwrap();

            let deleted = delete_at_path(&mut doc, &path).unwrap();
            assert_eq!(deleted.and_then(|v| v.as_i64()), Some(42));

            // Value should be gone
            assert!(get_at_path(&doc, &path).is_none());
        }

        #[test]
        fn test_delete_array_element() {
            let mut doc = sample_doc();
            let path = JsonPath::parse("items[1]").unwrap();

            let deleted = delete_at_path(&mut doc, &path).unwrap();
            assert_eq!(deleted.and_then(|v| v.as_i64()), Some(2));

            // Array should shift
            let arr = get_at_path(&doc, &JsonPath::parse("items").unwrap()).unwrap();
            assert_eq!(arr.as_array().unwrap().len(), 2);
            assert_eq!(arr.as_array().unwrap()[1].as_i64(), Some(3));
        }
    }

    // ========== Validation Tests ==========

    mod validation {
        use super::*;

        #[test]
        fn test_depth_validation_passes() {
            let mut value = JsonValue::from(42);
            for _ in 0..50 {
                value = JsonValue::Array(vec![value]);
            }
            assert!(validate_json_value(&value).is_ok());
        }

        #[test]
        fn test_depth_validation_fails() {
            let mut value = JsonValue::from(42);
            for _ in 0..101 {
                value = JsonValue::Array(vec![value]);
            }
            assert!(matches!(
                validate_json_value(&value),
                Err(JsonValidationError::NestingTooDeep { .. })
            ));
        }

        #[test]
        fn test_array_size_validation_passes() {
            let arr: Vec<JsonValue> = (0..1000).map(|i| JsonValue::from(i as i64)).collect();
            let value = JsonValue::Array(arr);
            assert!(validate_json_value(&value).is_ok());
        }

        #[test]
        fn test_array_size_validation_fails() {
            let arr: Vec<JsonValue> = (0..1_000_001).map(|i| JsonValue::from(i as i64)).collect();
            let value = JsonValue::Array(arr);
            assert!(matches!(
                validate_json_value(&value),
                Err(JsonValidationError::ArrayTooLarge { .. })
            ));
        }

        #[test]
        fn test_path_length_validation_passes() {
            let mut path = JsonPath::root();
            for i in 0..100 {
                path = path.key(format!("key{}", i));
            }
            assert!(validate_path(&path).is_ok());
        }

        #[test]
        fn test_path_length_validation_fails() {
            let mut path = JsonPath::root();
            for i in 0..257 {
                path = path.key(format!("key{}", i));
            }
            assert!(matches!(
                validate_path(&path),
                Err(JsonValidationError::PathTooLong { .. })
            ));
        }
    }

    // ========== Serialization Tests ==========

    mod serialization {
        use super::*;

        #[test]
        fn test_json_doc_roundtrip() {
            let doc = JsonDoc::new(
                JsonDocId::new(),
                JsonValue::from("test"),
            );

            let serialized = JsonStore::serialize_doc(&doc).unwrap();
            let deserialized = JsonStore::deserialize_doc(&serialized).unwrap();

            assert_eq!(doc.id, deserialized.id);
            assert_eq!(doc.value, deserialized.value);
            assert_eq!(doc.version, deserialized.version);
        }

        #[test]
        fn test_complex_value_roundtrip() {
            let mut obj = IndexMap::new();
            obj.insert("null".to_string(), JsonValue::Null);
            obj.insert("bool".to_string(), JsonValue::Bool(true));
            obj.insert("int".to_string(), JsonValue::from(42));
            obj.insert("float".to_string(), JsonValue::from(3.14));
            obj.insert("string".to_string(), JsonValue::from("hello"));
            obj.insert("array".to_string(), JsonValue::Array(vec![
                JsonValue::from(1),
                JsonValue::from(2),
            ]));

            let doc = JsonDoc::new(JsonDocId::new(), JsonValue::Object(obj));

            let serialized = JsonStore::serialize_doc(&doc).unwrap();
            let deserialized = JsonStore::deserialize_doc(&serialized).unwrap();

            assert_eq!(doc.value, deserialized.value);
        }

        #[test]
        fn test_json_path_serialization() {
            let path = JsonPath::parse("foo.bar[0].baz").unwrap();
            let bytes = rmp_serde::to_vec(&path).unwrap();
            let deserialized: JsonPath = rmp_serde::from_slice(&bytes).unwrap();
            assert_eq!(path, deserialized);
        }
    }
}
```

### Acceptance Criteria

- [ ] All JsonValue types tested
- [ ] All path parsing cases covered
- [ ] All path operations tested (get, set, delete)
- [ ] Edge cases covered (empty, max depth, max array)
- [ ] Error conditions tested
- [ ] Serialization roundtrips verified

---

## Story #254: JSON Integration Tests

**File**: `crates/primitives/tests/json_integration.rs` (NEW)

**Deliverable**: Integration tests for JSON with WAL and transactions

### Test Coverage

```rust
use in_mem_core::json_types::*;
use in_mem_engine::Database;
use in_mem_primitives::JsonStore;
use std::sync::Arc;

#[test]
fn test_json_create_and_read() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db);
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    let mut obj = IndexMap::new();
    obj.insert("name".to_string(), JsonValue::from("test"));
    obj.insert("count".to_string(), JsonValue::from(42));

    json.create(&run_id, &doc_id, JsonValue::Object(obj)).unwrap();

    let name = json.get(&run_id, &doc_id, &JsonPath::parse("name").unwrap()).unwrap();
    assert_eq!(name.and_then(|v| v.as_str().map(String::from)), Some("test".to_string()));

    let count = json.get(&run_id, &doc_id, &JsonPath::parse("count").unwrap()).unwrap();
    assert_eq!(count.and_then(|v| v.as_i64()), Some(42));
}

#[test]
fn test_json_update_at_path() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db);
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    json.create(&run_id, &doc_id, JsonValue::Object(IndexMap::new())).unwrap();

    // Set multiple paths
    json.set(&run_id, &doc_id, &JsonPath::parse("a").unwrap(), JsonValue::from(1)).unwrap();
    json.set(&run_id, &doc_id, &JsonPath::parse("b.c").unwrap(), JsonValue::from(2)).unwrap();
    json.set(&run_id, &doc_id, &JsonPath::parse("d.e.f").unwrap(), JsonValue::from(3)).unwrap();

    // Verify
    assert_eq!(json.get(&run_id, &doc_id, &JsonPath::parse("a").unwrap()).unwrap().and_then(|v| v.as_i64()), Some(1));
    assert_eq!(json.get(&run_id, &doc_id, &JsonPath::parse("b.c").unwrap()).unwrap().and_then(|v| v.as_i64()), Some(2));
    assert_eq!(json.get(&run_id, &doc_id, &JsonPath::parse("d.e.f").unwrap()).unwrap().and_then(|v| v.as_i64()), Some(3));
}

#[test]
fn test_json_wal_replay() {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir.path().to_path_buf();

    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    // Create and modify
    {
        let db = Arc::new(Database::open(&path).unwrap());
        let json = JsonStore::new(db);

        json.create(&run_id, &doc_id, JsonValue::Object(IndexMap::new())).unwrap();
        json.set(&run_id, &doc_id, &JsonPath::parse("version").unwrap(), JsonValue::from(1)).unwrap();
        json.set(&run_id, &doc_id, &JsonPath::parse("version").unwrap(), JsonValue::from(2)).unwrap();
        json.set(&run_id, &doc_id, &JsonPath::parse("version").unwrap(), JsonValue::from(3)).unwrap();
    }

    // Recover and verify
    {
        let db = Arc::new(Database::recover(&path).unwrap());
        let json = JsonStore::new(db);

        let version = json.get(&run_id, &doc_id, &JsonPath::parse("version").unwrap()).unwrap();
        assert_eq!(version.and_then(|v| v.as_i64()), Some(3));
    }
}

#[test]
fn test_json_transaction_commit() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();
    let key = Key::new_json(Namespace::for_run(run_id), &doc_id);

    json.create(&run_id, &doc_id, JsonValue::Object(IndexMap::new())).unwrap();

    // Transaction with multiple operations
    db.transaction(run_id, |txn| {
        txn.json_set(&key, &JsonPath::parse("a").unwrap(), JsonValue::from(1))?;
        txn.json_set(&key, &JsonPath::parse("b").unwrap(), JsonValue::from(2))?;
        txn.json_set(&key, &JsonPath::parse("c").unwrap(), JsonValue::from(3))?;
        Ok(())
    }).unwrap();

    // All should be committed
    assert_eq!(json.get(&run_id, &doc_id, &JsonPath::parse("a").unwrap()).unwrap().and_then(|v| v.as_i64()), Some(1));
    assert_eq!(json.get(&run_id, &doc_id, &JsonPath::parse("b").unwrap()).unwrap().and_then(|v| v.as_i64()), Some(2));
    assert_eq!(json.get(&run_id, &doc_id, &JsonPath::parse("c").unwrap()).unwrap().and_then(|v| v.as_i64()), Some(3));
}

#[test]
fn test_json_transaction_rollback() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();
    let key = Key::new_json(Namespace::for_run(run_id), &doc_id);

    json.create(&run_id, &doc_id, JsonValue::from(0)).unwrap();

    // Transaction that fails
    let result = db.transaction(run_id, |txn| {
        txn.json_set(&key, &JsonPath::root(), JsonValue::from(42))?;
        Err::<(), _>(TransactionError::Custom("forced failure".into()))
    });

    assert!(result.is_err());

    // Should be rolled back
    let value = json.get(&run_id, &doc_id, &JsonPath::root()).unwrap();
    assert_eq!(value.and_then(|v| v.as_i64()), Some(0));
}

#[test]
fn test_json_conflict_detection() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();
    let key = Key::new_json(Namespace::for_run(run_id), &doc_id);

    json.create(&run_id, &doc_id, JsonValue::Object(IndexMap::new())).unwrap();

    // Overlapping writes should conflict
    let result = db.transaction(run_id, |txn| {
        txn.json_set(&key, &JsonPath::parse("foo").unwrap(), JsonValue::from(1))?;
        txn.json_set(&key, &JsonPath::parse("foo.bar").unwrap(), JsonValue::from(2))?;
        Ok(())
    });

    assert!(result.is_err());
}

#[test]
fn test_json_cross_primitive_transaction() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db.clone());
    let kv = KVStore::new(db.clone());
    let events = EventLog::new(db.clone());

    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    json.create(&run_id, &doc_id, JsonValue::Object(IndexMap::new())).unwrap();

    // Transaction spanning all primitives
    db.transaction(run_id, |txn| {
        let json_key = Key::new_json(Namespace::for_run(run_id), &doc_id);
        let kv_key = Key::new_kv(Namespace::for_run(run_id), b"counter");

        txn.json_set(&json_key, &JsonPath::parse("updated").unwrap(), JsonValue::from(true))?;
        txn.put(kv_key, Value::from(42))?;
        txn.append_event(run_id, Event::new("updated", json!({})))?;

        Ok(())
    }).unwrap();

    // All should be committed atomically
    let json_val = json.get(&run_id, &doc_id, &JsonPath::parse("updated").unwrap()).unwrap();
    assert_eq!(json_val.and_then(|v| v.as_bool()), Some(true));

    let kv_val = kv.get(&run_id, b"counter").unwrap();
    assert_eq!(kv_val.and_then(|v| v.as_i64()), Some(42));

    assert_eq!(events.count(&run_id).unwrap(), 1);
}

#[test]
fn test_json_concurrent_access() {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db.clone());
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();

    json.create(&run_id, &doc_id, JsonValue::from(0)).unwrap();

    // Concurrent increments
    let handles: Vec<_> = (0..10).map(|_| {
        let json = json.clone();
        let run_id = run_id.clone();
        let doc_id = doc_id.clone();

        std::thread::spawn(move || {
            for _ in 0..100 {
                let _ = json.set(&run_id, &doc_id, &JsonPath::root(), JsonValue::from(1));
            }
        })
    }).collect();

    for handle in handles {
        handle.join().unwrap();
    }

    // Should not crash, final value may vary due to races
    let value = json.get(&run_id, &doc_id, &JsonPath::root()).unwrap();
    assert!(value.is_some());
}

#[test]
fn test_json_durability_modes() {
    for mode in [DurabilityMode::InMemory, DurabilityMode::Buffered, DurabilityMode::Sync] {
        let db = Arc::new(Database::open_temp_with_mode(mode).unwrap());
        let json = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = JsonDocId::new();

        json.create(&run_id, &doc_id, JsonValue::from(42)).unwrap();

        let value = json.get(&run_id, &doc_id, &JsonPath::root()).unwrap();
        assert_eq!(value.and_then(|v| v.as_i64()), Some(42));
    }
}
```

### Acceptance Criteria

- [ ] WAL replay reconstructs state correctly
- [ ] Transaction isolation works
- [ ] Conflicts detected correctly
- [ ] Cross-primitive atomicity works
- [ ] Concurrent access is safe
- [ ] All durability modes work

---

## Story #255: Non-Regression Benchmark Suite

**File**: `benches/m5_performance.rs` (NEW)

**Deliverable**: Benchmarks ensuring M4 targets maintained

### Benchmark Coverage

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use in_mem_core::json_types::*;
use in_mem_engine::Database;
use in_mem_primitives::*;
use std::sync::Arc;

// ========== JSON Operation Benchmarks ==========

fn bench_json_create(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_create");

    for size in [100, 1_000, 10_000] {
        group.bench_with_input(BenchmarkId::new("bytes", size), &size, |b, &size| {
            let db = Arc::new(Database::open_temp().unwrap());
            let json = JsonStore::new(db);
            let run_id = RunId::new();

            // Create document of specified size
            let value = JsonValue::from("x".repeat(size));

            b.iter(|| {
                let doc_id = JsonDocId::new();
                json.create(&run_id, &doc_id, black_box(value.clone())).unwrap()
            });
        });
    }

    group.finish();
}

fn bench_json_get_at_path(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_get_at_path");

    for depth in [1, 5, 10] {
        group.bench_with_input(BenchmarkId::new("depth", depth), &depth, |b, &depth| {
            let db = Arc::new(Database::open_temp().unwrap());
            let json = JsonStore::new(db);
            let run_id = RunId::new();
            let doc_id = JsonDocId::new();

            // Create nested document
            let mut value = JsonValue::from(42);
            for _ in 0..depth {
                let mut obj = IndexMap::new();
                obj.insert("nested".to_string(), value);
                value = JsonValue::Object(obj);
            }
            json.create(&run_id, &doc_id, value).unwrap();

            // Build path
            let path_str = (0..depth).map(|_| "nested").collect::<Vec<_>>().join(".");
            let path = JsonPath::parse(&path_str).unwrap();

            b.iter(|| {
                json.get(&run_id, &doc_id, black_box(&path)).unwrap()
            });
        });
    }

    group.finish();
}

fn bench_json_set_at_path(c: &mut Criterion) {
    let mut group = c.benchmark_group("json_set_at_path");

    for depth in [1, 5, 10] {
        group.bench_with_input(BenchmarkId::new("depth", depth), &depth, |b, &depth| {
            let db = Arc::new(Database::open_temp().unwrap());
            let json = JsonStore::new(db);
            let run_id = RunId::new();
            let doc_id = JsonDocId::new();

            // Create nested document
            let mut value = JsonValue::from(42);
            for _ in 0..depth {
                let mut obj = IndexMap::new();
                obj.insert("nested".to_string(), value);
                value = JsonValue::Object(obj);
            }
            json.create(&run_id, &doc_id, value).unwrap();

            let path_str = (0..depth).map(|_| "nested").collect::<Vec<_>>().join(".");
            let path = JsonPath::parse(&path_str).unwrap();
            let mut counter = 0i64;

            b.iter(|| {
                counter += 1;
                json.set(&run_id, &doc_id, black_box(&path), JsonValue::from(counter)).unwrap()
            });
        });
    }

    group.finish();
}

fn bench_json_delete_at_path(c: &mut Criterion) {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db);
    let run_id = RunId::new();

    c.bench_function("json_delete_at_path", |b| {
        b.iter_batched(
            || {
                let doc_id = JsonDocId::new();
                let mut obj = IndexMap::new();
                obj.insert("to_delete".to_string(), JsonValue::from(42));
                obj.insert("keep".to_string(), JsonValue::from(43));
                json.create(&run_id, &doc_id, JsonValue::Object(obj)).unwrap();
                doc_id
            },
            |doc_id| {
                json.delete_at_path(&run_id, &doc_id, &JsonPath::parse("to_delete").unwrap()).unwrap()
            },
            criterion::BatchSize::SmallInput,
        )
    });
}

// ========== Non-Regression Benchmarks (M4 Targets) ==========

fn bench_kv_put_inmemory(c: &mut Criterion) {
    let db = Arc::new(Database::open_temp_with_mode(DurabilityMode::InMemory).unwrap());
    let kv = KVStore::new(db);
    let run_id = RunId::new();
    let mut counter = 0u64;

    c.bench_function("kv_put_inmemory", |b| {
        b.iter(|| {
            counter += 1;
            let key = format!("key_{}", counter);
            kv.set(&run_id, key.as_bytes(), Value::from(counter as i64)).unwrap()
        });
    });
}

fn bench_kv_put_buffered(c: &mut Criterion) {
    let db = Arc::new(Database::open_temp_with_mode(DurabilityMode::Buffered).unwrap());
    let kv = KVStore::new(db);
    let run_id = RunId::new();
    let mut counter = 0u64;

    c.bench_function("kv_put_buffered", |b| {
        b.iter(|| {
            counter += 1;
            let key = format!("key_{}", counter);
            kv.set(&run_id, key.as_bytes(), Value::from(counter as i64)).unwrap()
        });
    });
}

fn bench_kv_get_fast_path(c: &mut Criterion) {
    let db = Arc::new(Database::open_temp().unwrap());
    let kv = KVStore::new(db);
    let run_id = RunId::new();

    // Pre-populate
    for i in 0..1000 {
        let key = format!("key_{}", i);
        kv.set(&run_id, key.as_bytes(), Value::from(i as i64)).unwrap();
    }

    let mut counter = 0;

    c.bench_function("kv_get_fast_path", |b| {
        b.iter(|| {
            counter = (counter + 1) % 1000;
            let key = format!("key_{}", counter);
            kv.get(&run_id, black_box(key.as_bytes())).unwrap()
        });
    });
}

fn bench_event_append(c: &mut Criterion) {
    let db = Arc::new(Database::open_temp().unwrap());
    let events = EventLog::new(db);
    let run_id = RunId::new();

    c.bench_function("event_append", |b| {
        b.iter(|| {
            events.append(&run_id, Event::new("test", json!({"data": "value"}))).unwrap()
        });
    });
}

fn bench_state_read(c: &mut Criterion) {
    let db = Arc::new(Database::open_temp().unwrap());
    let state = StateCell::new(db);
    let run_id = RunId::new();

    state.set(&run_id, b"key", Value::from(42)).unwrap();

    c.bench_function("state_read", |b| {
        b.iter(|| {
            state.get(&run_id, black_box(b"key")).unwrap()
        });
    });
}

fn bench_trace_append(c: &mut Criterion) {
    let db = Arc::new(Database::open_temp().unwrap());
    let trace = Trace::new(db);
    let run_id = RunId::new();

    c.bench_function("trace_append", |b| {
        b.iter(|| {
            trace.append(&run_id, TraceEntry::new("span", json!({"data": "value"}))).unwrap()
        });
    });
}

// ========== Mixed Workload ==========

fn bench_mixed_json_kv(c: &mut Criterion) {
    let db = Arc::new(Database::open_temp().unwrap());
    let json = JsonStore::new(db.clone());
    let kv = KVStore::new(db);
    let run_id = RunId::new();

    // Setup
    let doc_id = JsonDocId::new();
    json.create(&run_id, &doc_id, JsonValue::Object(IndexMap::new())).unwrap();

    let mut counter = 0u64;

    c.bench_function("mixed_json_kv", |b| {
        b.iter(|| {
            counter += 1;

            // JSON operation
            json.set(&run_id, &doc_id, &JsonPath::parse("counter").unwrap(), JsonValue::from(counter as i64)).unwrap();

            // KV operation
            let key = format!("key_{}", counter);
            kv.set(&run_id, key.as_bytes(), Value::from(counter as i64)).unwrap();
        });
    });
}

fn bench_cross_primitive_transaction(c: &mut Criterion) {
    let db = Arc::new(Database::open_temp().unwrap());
    let run_id = RunId::new();

    // Setup
    let doc_id = JsonDocId::new();
    let json_key = Key::new_json(Namespace::for_run(run_id), &doc_id);
    let kv_key = Key::new_kv(Namespace::for_run(run_id), b"counter");

    {
        let json = JsonStore::new(db.clone());
        json.create(&run_id, &doc_id, JsonValue::Object(IndexMap::new())).unwrap();
    }

    let mut counter = 0i64;

    c.bench_function("cross_primitive_transaction", |b| {
        let db = db.clone();
        b.iter(|| {
            counter += 1;
            db.transaction(run_id, |txn| {
                txn.json_set(&json_key, &JsonPath::parse("value").unwrap(), JsonValue::from(counter))?;
                txn.put(kv_key.clone(), Value::from(counter))?;
                Ok(())
            }).unwrap()
        });
    });
}

criterion_group!(
    json_benches,
    bench_json_create,
    bench_json_get_at_path,
    bench_json_set_at_path,
    bench_json_delete_at_path,
);

criterion_group!(
    regression_benches,
    bench_kv_put_inmemory,
    bench_kv_put_buffered,
    bench_kv_get_fast_path,
    bench_event_append,
    bench_state_read,
    bench_trace_append,
);

criterion_group!(
    mixed_benches,
    bench_mixed_json_kv,
    bench_cross_primitive_transaction,
);

criterion_main!(json_benches, regression_benches, mixed_benches);
```

### Acceptance Criteria

- [ ] JSON create < 1ms for 1KB document
- [ ] JSON get at path < 100µs for 1KB document
- [ ] JSON set at path < 1ms for 1KB document
- [ ] KV put InMemory < 3µs (M4 target)
- [ ] KV put Buffered < 30µs (M4 target)
- [ ] KV get fast path < 5µs (M4 target)
- [ ] No regression > 10% from M4 baselines

---

## Story #256: Performance Baseline Documentation

**File**: `docs/performance/M5_BASELINES.md` (NEW)

**Deliverable**: Document performance baselines and testing methodology

### Content

```markdown
# M5 Performance Baselines

## Test Environment

| Attribute | Value |
|-----------|-------|
| Hardware | [To be filled at measurement time] |
| CPU | |
| Memory | |
| Storage | |
| OS | |
| Rust Version | |
| Commit | |
| Date | |

## JSON Operation Baselines

### Create Performance

| Document Size | Target | Measured | Status |
|---------------|--------|----------|--------|
| 100 bytes | < 500µs | | |
| 1KB | < 1ms | | |
| 10KB | < 5ms | | |

### Get at Path Performance

| Path Depth | Target | Measured | Status |
|------------|--------|----------|--------|
| Depth 1 | < 50µs | | |
| Depth 5 | < 75µs | | |
| Depth 10 | < 100µs | | |

### Set at Path Performance

| Path Depth | Target | Measured | Status |
|------------|--------|----------|--------|
| Depth 1 | < 500µs | | |
| Depth 5 | < 750µs | | |
| Depth 10 | < 1ms | | |

### Delete at Path Performance

| Operation | Target | Measured | Status |
|-----------|--------|----------|--------|
| Object key | < 500µs | | |
| Array element | < 750µs | | |

## Non-Regression Verification

### M4 Baseline Comparison

| Operation | M4 Target | M4 Actual | M5 Measured | Delta | Status |
|-----------|-----------|-----------|-------------|-------|--------|
| KV put (InMemory) | < 3µs | | | | |
| KV put (Buffered) | < 30µs | | | | |
| KV get (fast path) | < 5µs | | | | |
| Event append | < 10µs | | | | |
| State read | < 5µs | | | | |
| Trace append | < 15µs | | | | |

### Regression Threshold

- **Acceptable**: < 5% regression
- **Warning**: 5-10% regression
- **Failure**: > 10% regression

## Mixed Workload Performance

| Workload | Target | Measured | Status |
|----------|--------|----------|--------|
| JSON + KV mixed | < 2ms per pair | | |
| Cross-primitive transaction | < 3ms | | |

## Methodology

### Benchmark Configuration

- **Warmup**: 100 iterations discarded
- **Measurement**: 1000 iterations minimum
- **Statistics**: p50, p95, p99 reported

### Running Benchmarks

```bash
# Run all M5 benchmarks
cargo bench --bench m5_performance

# Run specific benchmark group
cargo bench --bench m5_performance -- json_benches
cargo bench --bench m5_performance -- regression_benches

# Compare with baseline
cargo bench --bench m5_performance -- --save-baseline m5
cargo bench --bench m5_performance -- --baseline m4
```

### Memory Profiling

```bash
# Run with memory profiling
RUSTFLAGS="-C target-cpu=native" cargo bench --bench m5_performance -- --profile-time 30
```

## Known Limitations

1. **Path depth impact**: Deep paths (>10 levels) may have higher latency
2. **Large documents**: Documents >1MB may exceed targets
3. **Concurrent transactions**: High contention may increase conflict rate

## Recommendations

1. Keep documents under 1MB for best performance
2. Limit path depth to <10 for critical paths
3. Use batch operations (apply_patches) for multiple updates
4. Consider sharding for high-contention documents
```

### Acceptance Criteria

- [ ] All baselines documented
- [ ] Methodology described
- [ ] Test environment template provided
- [ ] Comparison with M4 included
- [ ] Running instructions provided
- [ ] Known limitations documented

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/core/src/json_types/tests.rs` | CREATE - Unit tests |
| `crates/primitives/tests/json_integration.rs` | CREATE - Integration tests |
| `benches/m5_performance.rs` | CREATE - Performance benchmarks |
| `docs/performance/M5_BASELINES.md` | CREATE - Baseline documentation |

---

## Success Criteria

- [ ] Unit tests cover all path operations and edge cases
- [ ] Integration tests verify WAL replay and transactions
- [ ] KV, Event, State, Trace maintain M4 latency targets
- [ ] JSON operations meet performance baselines
- [ ] No memory leaks detected under load
- [ ] Cross-primitive transaction tests pass
- [ ] Documentation complete and accurate
