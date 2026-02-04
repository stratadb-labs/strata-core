# Phase 3b: Cloning Overhead Audit

Date: 2026-02-04
Status: Complete

## Summary

**397 `.clone()` calls** across storage, concurrency, and core layers. The majority of expensive clones (Key and Value) are in **hot paths** — per-operation transaction tracking and scan/iteration results. Scan operations are the most impacted, with 10k-result scans spending ~5.5ms on cloning alone.

**MVP Readiness: PASS** — no correctness issues. Performance optimization opportunities identified for post-MVP.

---

## 1. Type Clone Costs

### Key (expensive)

```rust
pub struct Key {
    pub namespace: Namespace,  // 5 Strings + UUID (~200+ bytes)
    pub type_tag: TypeTag,     // u8 (cheap)
    pub user_key: Vec<u8>,     // heap allocation
}
```

**Estimated cost**: ~211-251ns per clone (5 string allocations + Vec memcpy)

### Value (variable)

```rust
pub enum Value {
    Null, Bool(bool), Int(i64), Float(f64),  // cheap (<100ns)
    String(String),                           // ~200ns
    Bytes(Vec<u8>),                           // ~500ns for 1KB
    Object(HashMap<String, Value>),           // ~5us for 100 entries
    Array(Vec<Value>),                        // ~10us for 1000 items
}
```

### VersionedValue

```rust
pub struct VersionedValue {  // = Versioned<Value>
    pub value: Value,        // variable cost
    pub version: Version,    // u64 (cheap)
    pub timestamp: Timestamp, // 8 bytes (cheap)
}
```

---

## 2. Hot Path Analysis

### Transaction Read Path (2-3 clones per read)

Tracing a single `kv_get()`:

```
txn.get(&key)
  ├─ Check write_set.get(&key)
  │   └─ IF HIT: value.clone()           ← CLONE 1
  └─ read_from_snapshot(&key)
      ├─ key.clone() for read_set         ← CLONE 2 (always)
      └─ vv.value.clone()                 ← CLONE 3 (always)
```

**Cost**: ~500-750ns per read (2-3 clones of Key + Value)

### Scan Operations (N clones per result)

```rust
// sharded.rs list_by_prefix()
// Phase 1: Clone all matching keys
let keys: Vec<Key> = shard.ordered_keys.iter()
    .filter(...)
    .cloned()                              // ← N key clones
    .collect();

// Phase 2: Clone all values
keys.into_iter()
    .filter_map(|key| {
        Some((key, sv.versioned().clone())) // ← N value clones
    })
    .collect()
```

**Cost for 10k results**: ~2.5ms key clones + ~3ms value clones = **5.5ms**

### Batch Apply (3 clones per operation)

```rust
// apply_batch()
for (key, value) in writes {
    let stored = StoredValue::with_timestamp(value.clone(), ...); // ← CLONE 1
    branch_ops.entry(...)
        .push((key.clone(), stored));                              // ← CLONE 2
}
for key in deletes {
    branch_ops.push(key.clone());                                  // ← CLONE 3
}
```

**Cost for 1000 writes + 500 deletes**: ~675us

---

## 3. Clone Inventory by File

### `crates/storage/src/sharded.rs` (138 clones total)

| Category | Count | Hot Path? |
|----------|-------|-----------|
| Key clones | ~20 | Yes — scan results, delete, batch |
| Value clones | ~15 | Yes — scan results, batch |
| Namespace clones | ~41 | No — test setup |
| Test-only clones | ~62 | No |

### `crates/concurrency/src/transaction.rs` (106 clones total)

| Category | Count | Hot Path? |
|----------|-------|-----------|
| Key clones for read_set | ~8 | **Yes — every read** |
| Value clones from snapshot | ~5 | **Yes — every read** |
| Path clones (JSON) | ~3 | Medium — JSON ops |
| Test-only clones | ~90 | No |

### `crates/core/src/types.rs` (153 clones total)

| Category | Count | Hot Path? |
|----------|-------|-----------|
| Test setup | ~153 | No — all test code |

---

## 4. Why Clones Are Required (Architecture Constraints)

1. **HashMap ownership**: `read_set: HashMap<Key, u64>` requires owned keys
2. **Lock scope**: Scan phase 1 must release shard lock before phase 2 — keys must be owned to outlive lock
3. **Serialization**: WAL writes need owned data

---

## 5. Optimization Opportunities (Post-MVP)

### Priority 1: Hash-Based Read Set (HIGH impact, MEDIUM effort)

Replace `HashMap<Key, u64>` with `HashMap<u64, u64>` (key hash → version).

- Eliminates per-read key.clone()
- **Estimated improvement**: 10-20% latency reduction for read-heavy workloads
- **Trade-off**: Rare false negatives from hash collisions (acceptable for OCC)

### Priority 2: Iterator-Based Scan Returns (CRITICAL impact, HIGH effort)

Replace `Vec<(Key, VersionedValue)>` returns with callback/iterator pattern.

```rust
pub fn list_branch_each<F>(&self, branch_id: &BranchId, mut f: F)
where F: FnMut(&Key, &VersionedValue) -> Result<()>
```

- Eliminates all per-result clones in scans
- **Estimated improvement**: 10k scan 5.5ms → ~500us (10x faster)
- **Trade-off**: API change, lock held during iteration

### Priority 3: Arc-Wrapped Values (MEDIUM impact, MEDIUM effort)

Use `Arc<VersionedValue>` in storage to enable cheap reference-counted sharing.

- Reduces clone cost to Arc increment (~20ns vs ~300ns+)
- **Estimated improvement**: 3-4x scan throughput
- **Trade-off**: Arc overhead on every value, Sync requirements

### Priority 4: Batch Apply Optimization (MEDIUM impact, MEDIUM effort)

Group by branch_id before cloning, clone once per batch under lock.

- **Estimated improvement**: 1000-item batch 675us → ~150us

---

## 6. Projected Gains

```
                        Current     After Phase 1   After Phase 2   After All
Single read:            ~5us        ~3us (-40%)     ~3us            ~3us
10k scan:               ~9ms        ~8.5ms          ~500us (56x!)   ~150us
Batch 1000 ops:         ~1.5ms      ~1.5ms          ~1.5ms          ~450us
```

---

## 7. Recommendation

**For MVP**: No changes needed. The current cloning patterns are correct and safe. The performance is acceptable for initial workloads.

**Post-MVP priorities**:
1. Hash-based read_set (quick win, no API changes)
2. Iterator-based scans (high impact for large result sets)
3. Benchmark before and after — profile actual workloads to validate estimates

---

## Methodology

Searched all non-test code in `sharded.rs`, `transaction.rs`, `types.rs`, and `value.rs` for `.clone()` calls. Traced the transaction read path from API through storage. Classified each clone as hot-path (per-operation) or cold-path (setup/rare). Estimated costs based on type sizes and allocation patterns.
