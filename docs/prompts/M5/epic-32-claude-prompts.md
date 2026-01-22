# Epic 32: Validation & Non-Regression - Implementation Prompts

**Epic Goal**: Ensure correctness and maintain M4 performance

**GitHub Issue**: [#262](https://github.com/anibjoshi/in-mem/issues/262)
**Status**: Ready after all other M5 epics
**Dependencies**: Epics 26-31 complete

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M5_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M5_ARCHITECTURE.md`
2. **Epic Spec**: `docs/milestones/M5/EPIC_32_VALIDATION.md`
3. **Prompt Header**: `docs/prompts/M5/M5_PROMPT_HEADER.md` for the 6 architectural rules

**The architecture spec is LAW.** Epic docs provide implementation details but MUST NOT contradict the architecture spec.

---

## CRITICAL: NON-REGRESSION

**M5 must NOT degrade M4 primitive performance.**

| Operation | M4 Target | M5 Requirement |
|-----------|-----------|----------------|
| KV put (InMemory) | < 3µs | No regression |
| KV put (Buffered) | < 30µs | No regression |
| KV get (fast path) | < 5µs | No regression |
| Event append | < 10µs | No regression |
| State read | < 5µs | No regression |
| Trace append | < 15µs | No regression |

**If ANY M4 target regresses > 10%, STOP and FIX before proceeding.**

---

## Epic 32 Overview

### Scope
- JSON unit tests
- JSON integration tests
- Non-regression benchmark suite
- Performance baseline documentation

### Success Criteria
- [ ] Unit tests cover all path operations and edge cases
- [ ] Integration tests verify WAL replay and transactions
- [ ] KV, Event, State, Trace maintain M4 latency targets
- [ ] JSON operations meet performance baselines
- [ ] No memory leaks detected under load
- [ ] Cross-primitive transaction tests pass
- [ ] Documentation complete and accurate

### Component Breakdown
- **Story #253 (GitHub #291)**: JSON Unit Tests
- **Story #254 (GitHub #292)**: JSON Integration Tests
- **Story #255 (GitHub #293)**: Non-Regression Benchmark Suite
- **Story #256 (GitHub #294)**: Performance Baseline Documentation

---

## Story #291: JSON Unit Tests

**GitHub Issue**: [#291](https://github.com/anibjoshi/in-mem/issues/291)
**Estimated Time**: 4 hours
**Dependencies**: All implementation epics complete

### Start Story

```bash
gh issue view 291
./scripts/start-story.sh 32 291 json-unit-tests
```

### Implementation

Create `crates/core/src/json_types/tests.rs`:

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
            assert_eq!(v.as_i64(), None);
        }

        #[test]
        fn test_string() {
            let v = JsonValue::from("hello");
            assert!(v.is_string());
            assert_eq!(v.as_str(), Some("hello"));
        }

        #[test]
        fn test_object_preserves_insertion_order() {
            let mut obj = IndexMap::new();
            obj.insert("z".to_string(), JsonValue::from(1));
            obj.insert("a".to_string(), JsonValue::from(2));
            obj.insert("m".to_string(), JsonValue::from(3));

            let v = JsonValue::Object(obj);
            let keys: Vec<_> = v.as_object().unwrap().keys().collect();
            assert_eq!(keys, vec!["z", "a", "m"]);
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
        fn test_parse_with_dollar_prefix() {
            let path = JsonPath::parse("$.foo.bar").unwrap();
            assert_eq!(path.len(), 2);
            assert_eq!(path.to_string(), "$.foo.bar");
        }

        #[test]
        fn test_overlaps() {
            let root = JsonPath::root();
            let foo = JsonPath::parse("foo").unwrap();
            let foo_bar = JsonPath::parse("foo.bar").unwrap();
            let baz = JsonPath::parse("baz").unwrap();
            let items_0 = JsonPath::parse("items[0]").unwrap();
            let items_1 = JsonPath::parse("items[1]").unwrap();

            assert!(root.overlaps(&foo));
            assert!(foo.overlaps(&foo_bar));
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
        fn test_set_creates_intermediate() {
            let mut doc = JsonValue::Object(IndexMap::new());
            let path = JsonPath::parse("a.b.c").unwrap();
            set_at_path(&mut doc, &path, JsonValue::from(42)).unwrap();

            let result = get_at_path(&doc, &path);
            assert_eq!(result.and_then(|v| v.as_i64()), Some(42));
        }

        #[test]
        fn test_delete_array_element() {
            let mut doc = sample_doc();
            let path = JsonPath::parse("items[1]").unwrap();

            let deleted = delete_at_path(&mut doc, &path).unwrap();
            assert_eq!(deleted.and_then(|v| v.as_i64()), Some(2));

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
            assert!(validate_json_value(&value).is_err());
        }
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 291
```

---

## Story #292: JSON Integration Tests

**GitHub Issue**: [#292](https://github.com/anibjoshi/in-mem/issues/292)
**Estimated Time**: 4 hours
**Dependencies**: Story #291

### Start Story

```bash
gh issue view 292
./scripts/start-story.sh 32 292 json-integration-tests
```

### Implementation

Create `crates/primitives/tests/json_integration.rs`:

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

    db.transaction(run_id, |txn| {
        txn.json_set(&key, &JsonPath::parse("a").unwrap(), JsonValue::from(1))?;
        txn.json_set(&key, &JsonPath::parse("b").unwrap(), JsonValue::from(2))?;
        txn.json_set(&key, &JsonPath::parse("c").unwrap(), JsonValue::from(3))?;
        Ok(())
    }).unwrap();

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

    let result = db.transaction(run_id, |txn| {
        txn.json_set(&key, &JsonPath::root(), JsonValue::from(42))?;
        Err::<(), _>(TransactionError::Custom("forced failure".into()))
    });

    assert!(result.is_err());

    let value = json.get(&run_id, &doc_id, &JsonPath::root()).unwrap();
    assert_eq!(value.and_then(|v| v.as_i64()), Some(0));
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

    db.transaction(run_id, |txn| {
        let json_key = Key::new_json(Namespace::for_run(run_id), &doc_id);
        let kv_key = Key::new_kv(Namespace::for_run(run_id), b"counter");

        txn.json_set(&json_key, &JsonPath::parse("updated").unwrap(), JsonValue::from(true))?;
        txn.put(kv_key, Value::from(42))?;
        txn.append_event(run_id, Event::new("updated", json!({})))?;

        Ok(())
    }).unwrap();

    let json_val = json.get(&run_id, &doc_id, &JsonPath::parse("updated").unwrap()).unwrap();
    assert_eq!(json_val.and_then(|v| v.as_bool()), Some(true));

    let kv_val = kv.get(&run_id, b"counter").unwrap();
    assert_eq!(kv_val.and_then(|v| v.as_i64()), Some(42));

    assert_eq!(events.count(&run_id).unwrap(), 1);
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

### Complete Story

```bash
./scripts/complete-story.sh 292
```

---

## Story #293: Non-Regression Benchmark Suite

**GitHub Issue**: [#293](https://github.com/anibjoshi/in-mem/issues/293)
**Estimated Time**: 4 hours
**Dependencies**: Story #292

### Start Story

```bash
gh issue view 293
./scripts/start-story.sh 32 293 m5-benchmarks
```

### Implementation

Create `benches/m5_performance.rs`:

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

            let mut value = JsonValue::from(42);
            for _ in 0..depth {
                let mut obj = IndexMap::new();
                obj.insert("nested".to_string(), value);
                value = JsonValue::Object(obj);
            }
            json.create(&run_id, &doc_id, value).unwrap();

            let path_str = (0..depth).map(|_| "nested").collect::<Vec<_>>().join(".");
            let path = JsonPath::parse(&path_str).unwrap();

            b.iter(|| {
                json.get(&run_id, &doc_id, black_box(&path)).unwrap()
            });
        });
    }

    group.finish();
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

criterion_group!(
    json_benches,
    bench_json_create,
    bench_json_get_at_path,
);

criterion_group!(
    regression_benches,
    bench_kv_put_inmemory,
    bench_kv_put_buffered,
    bench_kv_get_fast_path,
);

criterion_main!(json_benches, regression_benches);
```

Update `Cargo.toml`:

```toml
[[bench]]
name = "m5_performance"
harness = false
```

### Validation

```bash
~/.cargo/bin/cargo bench --bench m5_performance
```

### Complete Story

```bash
./scripts/complete-story.sh 293
```

---

## Story #294: Performance Baseline Documentation

**GitHub Issue**: [#294](https://github.com/anibjoshi/in-mem/issues/294)
**Estimated Time**: 2 hours
**Dependencies**: Story #293

### Start Story

```bash
gh issue view 294
./scripts/start-story.sh 32 294 m5-baselines
```

### Implementation

Create `docs/performance/M5_BASELINES.md`:

```markdown
# M5 Performance Baselines

## Test Environment

| Attribute | Value |
|-----------|-------|
| Hardware | [To be filled] |
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

## Non-Regression Verification

### M4 Baseline Comparison

| Operation | M4 Target | M4 Actual | M5 Measured | Delta | Status |
|-----------|-----------|-----------|-------------|-------|--------|
| KV put (InMemory) | < 3µs | | | | |
| KV put (Buffered) | < 30µs | | | | |
| KV get (fast path) | < 5µs | | | | |

### Regression Threshold

- **Acceptable**: < 5% regression
- **Warning**: 5-10% regression
- **Failure**: > 10% regression

## Running Benchmarks

```bash
# Run all M5 benchmarks
cargo bench --bench m5_performance

# Compare with baseline
cargo bench --bench m5_performance -- --save-baseline m5
cargo bench --bench m5_performance -- --baseline m4
```
```

### Complete Story

```bash
./scripts/complete-story.sh 294
```

---

## Epic 32 Completion Checklist

### Final Validation

```bash
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo bench --bench m5_performance
~/.cargo/bin/cargo bench --bench m4_performance  # Non-regression
~/.cargo/bin/cargo clippy --workspace -- -D warnings
```

### Verify Non-Regression

All M4 targets must be met:
- [ ] KV put (InMemory) < 3µs
- [ ] KV put (Buffered) < 30µs
- [ ] KV get (fast path) < 5µs

### Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-32-validation -m "Epic 32: Validation & Non-Regression complete

Delivered:
- JSON unit tests (comprehensive coverage)
- JSON integration tests (WAL, transactions, cross-primitive)
- Non-regression benchmark suite
- Performance baseline documentation

Stories: #291, #292, #293, #294

NON-REGRESSION VERIFIED: All M4 targets maintained.
"
git push origin develop
gh issue close 262 --comment "Epic 32: Validation & Non-Regression - COMPLETE"
```

---

## M5 Milestone Complete

After Epic 32, merge develop to main:

```bash
git checkout main
git merge --no-ff develop -m "M5: JSON Primitive - Complete

Milestone 5 delivers the JSON document storage primitive:
- JsonStore stateless facade (Rule 2)
- Documents in ShardedStore via Key::new_json() (Rule 1)
- JsonStoreExt trait on TransactionContext (Rule 3)
- Path-level operations with validation (Rule 4)
- Unified WAL with entry types 0x20-0x23 (Rule 5)
- API consistent with other primitives (Rule 6)

Performance:
- JSON create: < 1ms (1KB)
- JSON get at path: < 100µs
- JSON set at path: < 1ms
- Non-regression: All M4 targets maintained

Epics: #256-#262
Stories: #263-#294
"
git push origin main
```
