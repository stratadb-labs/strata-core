# Resource Leak and Cleanup Audit

## 1. TransactionContext Pool

**Verdict: No leaks.**

```
  TransactionPool (thread_local RefCell<Vec<TransactionContext>>)
  ┌──────────────────────────────────────────────────────┐
  │  MAX_POOL_SIZE = 8 per thread                        │
  │                                                      │
  │  acquire() ──► pop from pool or allocate new         │
  │    └── calls reset() on reused contexts              │
  │                                                      │
  │  release() ──► push to pool (if room) or drop        │
  │    └── excess contexts freed, not hoarded            │
  └──────────────────────────────────────────────────────┘
```

**Location**: `crates/engine/src/transaction/pool.rs`

### Acquisition/Release Pairing

| Acquisition Point | Release Point | Guaranteed? |
|-------------------|---------------|-------------|
| `Database::begin_transaction()` (mod.rs:723) | `Database::end_transaction()` (mod.rs:749) | Yes |
| `Database::transaction()` closure (mod.rs:596) | Same closure, line 604 | Yes — finally-like pattern |
| `Database::transaction_with_retry()` (mod.rs:669) | Same method, line 686 | Yes — each retry ends txn |
| `Database::transaction_with_version()` (mod.rs:625) | Same method, line 639 | Yes |
| `Session::handle_begin()` (session.rs:141) | `Session::handle_commit/abort()` | Yes |
| (same) | `Session::drop()` (session.rs:370-376) | Fallback — catches orphaned ctx |

### reset() Completeness

All 11 mutable fields are cleared in `TransactionContext::reset()` (transaction.rs:1285-1319):

| Field | Clear Method | Stale Data? |
|-------|-------------|-------------|
| `txn_id` | Replaced | No |
| `branch_id` | Replaced | No |
| `start_version` | Replaced | No |
| `snapshot` | Replaced | No |
| `read_set` | `.clear()` | No (capacity preserved) |
| `write_set` | `.clear()` | No (capacity preserved) |
| `delete_set` | `.clear()` | No (capacity preserved) |
| `cas_set` | `.clear()` | No (capacity preserved) |
| `event_sequence_count` | `= None` | No (deallocated) |
| `event_last_hash` | `= None` | No (deallocated) |
| `json_reads` | `= None` | No (deallocated) |
| `json_writes` | `= None` | No (deallocated) |
| `json_snapshot_versions` | `= None` | No (deallocated) |
| `status` | `= Active` | No |
| `start_time` | `= Instant::now()` | No |

## 2. WAL Segment File Handles

**Verdict: Sound. All file handles properly managed.**

### Segment Rotation Sequence

```
  rotate_segment()  (wal/writer.rs:219-237)
  │
  ├─ 1. old_segment.close()
  │     └─ file.sync_all()          ← fsync before marking closed
  │     └─ closed = true
  │
  ├─ 2. WalSegment::create(new_number)
  │     └─ File::create_new()       ← new file opened
  │     └─ write header
  │
  └─ 3. self.segment = Some(new_segment)
        └─ old_segment dropped       ← File::drop closes handle
```

### File Handle Inventory

| Open Location | Type | Close Mechanism | Sync Before Close? |
|---------------|------|-----------------|-------------------|
| `WalSegment::create()` (wal_record.rs:145) | Create new | Drop or explicit close() | Yes (close syncs) |
| `WalSegment::open_read()` (wal_record.rs:167) | Read only | Drop | N/A (read-only) |
| `WalSegment::open_append()` (wal_record.rs:212) | Read+write | Drop or explicit close() | Yes (close syncs) |
| `snapshot.rs:142` File::create | Snapshot write | Drop after sync_all | Yes |
| `disk_snapshot/writer.rs:76` | Temp snapshot | Drop after sync_all + rename | Yes |
| `manifest.rs:215` | Temp manifest | Drop after sync_all + rename | Yes |

### Atomic Write Pattern (used in 3 places)

```
  1. Write to temp file (.tmp)
  2. file.sync_all()              ← data durable
  3. std::fs::rename(temp, final) ← atomic on POSIX
  4. parent_dir.sync_all()        ← directory entry durable
```

Used in: `snapshot.rs:write_atomic`, `disk_snapshot/writer.rs:create_snapshot`, `manifest.rs:persist`

### WalWriter::drop Safety

```rust
// wal/writer.rs:338-346
impl Drop for WalWriter {
    fn drop(&mut self) {
        if self.has_unsynced_data {
            if let Some(ref mut segment) = self.segment {
                let _ = segment.sync();  // best-effort sync
            }
        }
    }
}
```

### Minor Issue: Manifest Temp File on Rename Failure

`manifest.rs:226` — if `std::fs::rename()` fails, the temp file (`.tmp`) is not cleaned up. The snapshot writer (`snapshot.rs:245`) does clean up on rename failure, but the manifest writer does not. Next `persist()` call will overwrite it (via `truncate(true)`), so this is low severity.

## 3. Vector Memory Management

**Verdict: Sound. Slots properly reused, collections properly freed.**

### VectorHeap Memory Layout

```
  data: Vec<f32>  (contiguous buffer)
  ┌────────┬────────┬────────┬────────┬────────┐
  │ vec 0  │ vec 1  │ (free) │ vec 3  │ vec 4  │
  │ dim=3  │ dim=3  │ zeroed │ dim=3  │ dim=3  │
  └────────┴────────┴────────┴────────┴────────┘
       ↑         ↑        ↑
  id_to_offset  id_to_offset  free_slots=[6]
  {0: 0}       {1: 3}        (offset in floats)
                              {3: 9, 4: 12}
```

**Location**: `crates/engine/src/primitives/vector/heap.rs`

### Slot Lifecycle

| Operation | What Happens | Memory Effect |
|-----------|-------------|---------------|
| `upsert(new_id)` — no free slots | `data.extend_from_slice(embedding)` | Vec grows |
| `upsert(new_id)` — free slot available | `free_slots.pop()`, copy into existing slot | No growth |
| `upsert(existing_id)` | In-place copy at existing offset | No growth |
| `delete(id)` | `id_to_offset.remove()`, `free_slots.push(offset)`, zero data | No shrink, slot recycled |

### free_slots Reuse — Verified

```rust
// heap.rs:196 — on insert, check free_slots first
let offset = if let Some(slot) = self.free_slots.pop() {
    self.data[start..end].copy_from_slice(embedding);  // reuse
    slot
} else {
    let offset = self.data.len();
    self.data.extend_from_slice(embedding);  // grow
    offset
};
```

Test at heap.rs:392-416 (`test_slot_reuse`) confirms `data.len()` unchanged after delete+reinsert.

### Collection Delete — Complete Cleanup

```
  delete_collection()  (store.rs:216-251)
  │
  ├─ 1. delete_all_vectors()     ← KV tombstones for all vector records
  │     └─ scan prefix, delete each in transaction
  │
  ├─ 2. Delete config key        ← KV tombstone for collection config
  │
  └─ 3. backends.write().remove(&collection_id)
        └─ Box<dyn VectorIndexBackend> dropped
           └─ BruteForceBackend dropped
              └─ VectorHeap dropped
                 └─ data: Vec<f32> freed     ← ALL embedding memory released
                 └─ id_to_offset: BTreeMap freed
                 └─ free_slots: Vec<usize> freed
```

### Recovery Idempotency

`insert_with_id()` calls `upsert()`, which checks `id_to_offset` first. If the same VectorId is replayed twice during recovery, the second call updates in-place instead of double-allocating.

## 4. commit_locks DashMap — Unbounded Growth

**Verdict: LEAK. Entries never removed.**

```
  commit_locks: DashMap<BranchId, Mutex<()>>

  Branch created  → entry added on first commit (lazy)
  Branch deleted  → entry STAYS forever
  Branch created  → another entry
  Branch deleted  → another entry stays
  ...
  After 1M branches → 1M Mutex entries (~48MB+ wasted)
```

**Location**: `crates/concurrency/src/manager.rs:83`

There is no call to `commit_locks.remove()` anywhere in the codebase. Branch deletion (`branch/index.rs:312-373`) cleans up storage shards but does not notify the TransactionManager to release the commit lock entry.

Each entry is a `Mutex<()>` (parking_lot — 1 byte internal) plus DashMap overhead (~40-48 bytes per entry for key + hash + pointer). For workloads with ephemeral branches (e.g., per-request branches), this grows without bound.

## 5. VersionChain GC — Never Invoked

**Verdict: LEAK. Version chains grow unbounded.**

```
  VersionChain for key "kv:counter"
  ┌──────────────────────────────────────────────┐
  │ Txn(1000) → Txn(999) → ... → Txn(1)        │
  │                                              │
  │ 1000 versions retained, GC never called      │
  └──────────────────────────────────────────────┘
```

**GC method exists** at `sharded.rs:103-120`:

```rust
pub fn gc(&mut self, min_version: u64) {
    while self.versions.len() > 1 {
        if let Some(oldest) = self.versions.back() {
            if oldest.version().as_u64() < min_version {
                self.versions.pop_back();
            } else { break; }
        } else { break; }
    }
}
```

**Retention policy framework exists** at `durability/src/retention/policy.rs`:
- `KeepAll`, `KeepLast(n)`, `KeepFor(duration)`, `Composite` policies defined
- `should_retain()` method implemented
- **But never called from any production code path**

There are zero calls to `VersionChain::gc()` or `RetentionPolicy::should_retain()` in production code. Every update to a key adds a new version entry that persists forever. A key updated 1M times retains all 1M versions in its chain.

**Related existing issue**: #861 (TTLIndex disconnected — entries never expire)

## 6. Branch Delete — Incomplete Cascading Cleanup

**Verdict: Partial cleanup. Several resources not cleaned.**

Branch deletion (`branch/index.rs:312-373`) performs:

| Resource | Cleaned? | How |
|----------|----------|-----|
| Branch metadata key | Yes | KV delete in transaction |
| KV data (TypeTag::KV) | Yes | Scan + delete per key |
| Event data (TypeTag::Event) | Yes | Scan + delete per key |
| State data (TypeTag::State) | Yes | Scan + delete per key |
| JSON data (TypeTag::Json) | Yes | Scan + delete per key |
| Vector data (TypeTag::Vector) | Yes | Scan + delete per key |
| Storage shard (DashMap) | Yes | `clear_branch()` (sharded.rs:693) |
| **commit_locks entry** | **No** | Never removed |
| **Search index entries** | **No** | Postings for branch data remain |
| **Vector backend (in-memory)** | **No** | RwLock entry persists until explicit delete_collection |

The storage shard IS properly cleaned via `clear_branch()`. But three resources are missed:

1. **commit_locks** — TransactionManager not notified
2. **Search index** — InvertedIndex postings for deleted data remain (stale results possible)
3. **Vector backends** — In-memory VectorHeap not dropped (data is gone from KV, but heap retains embeddings until process restart)

## 7. Summary

| # | Finding | Severity | Type |
|---|---------|----------|------|
| 1 | TransactionContext pool — no leaks | — | Correct |
| 2 | WAL file handles — all properly closed | — | Correct |
| 3 | VectorHeap free_slots — properly reused | — | Correct |
| 4 | Collection delete — fully cleans memory | — | Correct |
| 5 | Recovery — idempotent, no double allocation | — | Correct |
| 6 | commit_locks never cleaned on branch delete | High | Resource leak |
| 7 | VersionChain GC never invoked — unbounded growth | High | Resource leak |
| 8 | Branch delete doesn't clean search index entries | Medium | Incomplete cleanup |
| 9 | Branch delete doesn't clean vector backends | Medium | Incomplete cleanup |
| 10 | Manifest temp file not cleaned on rename failure | Low | Minor leak |
