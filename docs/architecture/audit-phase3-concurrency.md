# Phase 3a: Concurrency Safety Audit

Date: 2026-02-04
Status: Complete

## Summary

The codebase demonstrates **strong concurrency design** with proper use of lock-free structures (DashMap), parking_lot over std::sync, correct atomic memory ordering, and no Arc reference cycles. A few minor documentation and code improvements are recommended.

**MVP Readiness: PASS** — no blocking issues. 3 minor fixes recommended.

---

## 1. Lock Inventory

### parking_lot::Mutex (preferred — no poisoning)

| Crate | File | Purpose |
|-------|------|---------|
| concurrency | `manager.rs:83` | Per-branch commit locks (`DashMap<BranchId, Mutex<()>>`) |
| engine | `database/mod.rs:133` | WAL writer access |
| engine | `database/mod.rs:168` | Flush thread handle |
| durability | `database/handle.rs:11` | WAL writer mutex |
| durability | `compaction/wal_only.rs:26` | Compaction locking |

### parking_lot::RwLock

| Crate | File | Purpose |
|-------|------|---------|
| engine | `primitives/vector/store.rs:38` | Vector store backend indexing |
| engine | `recovery/participant.rs:32` | Recovery coordination state |

### DashMap (lock-free concurrent HashMap)

| Crate | File | Purpose | Sharding |
|-------|------|---------|----------|
| storage | `sharded.rs:252` | Per-branch data shards | 16-way by BranchId |
| engine | `database/mod.rs:159` | Extension storage | By TypeId |
| engine | `search/index.rs` | Search index backends | Multiple collections |
| concurrency | `manager.rs:83` | Commit lock registry | By BranchId |

### std::sync::Mutex (1 instance — poisoning risk)

| Crate | File | Purpose | Risk |
|-------|------|---------|------|
| engine | `database/mod.rs:263` | Global `OPEN_DATABASES` registry | Poisoning on panic |

### Atomics

| Type | Location | Purpose | Ordering |
|------|----------|---------|----------|
| AtomicU64 | `concurrency/manager.rs:65` | Global version counter | SeqCst |
| AtomicU64 | `concurrency/manager.rs:70` | Transaction ID allocator | SeqCst |
| AtomicU64 | `storage/sharded.rs:254` | Global storage version | Acquire/Release |
| AtomicBool | `engine/database/mod.rs:151` | Transaction acceptance flag | Relaxed |
| AtomicBool | `engine/database/mod.rs:162` | WAL flush shutdown signal | Relaxed |

All atomic orderings are correct: SeqCst for correctness-critical allocation, Relaxed for metrics/shutdown flags, Acquire/Release for version barriers.

---

## 2. Lock Ordering & Deadlock Analysis

### Inferred Lock Hierarchy

1. Global Registry Lock (`OPEN_DATABASES`) — outermost
2. Per-Branch Commit Lock (`commit_locks`) — per-branch
3. WAL Mutex (`wal_writer`) — per-database
4. Storage Shard Locks (implicit in DashMap) — per-shard

### Deadlock Risk Assessment

**No deadlock potential detected.** Key observations:

- Per-branch commit locks prevent cross-branch contention — transactions on different branches never contend
- WAL mutex is acquired only within commit path, after branch lock
- No code path acquires locks in reverse order
- DashMap entry operations use trivial closures (no blocking inside)

### Missing Documentation

**Lock ordering is not explicitly documented** in the codebase. The convention works correctly but is implicit, which is a maintenance risk.

---

## 3. DashMap Usage Patterns

### Pattern 1: `entry().or_insert_with()` — Acceptable

```rust
// concurrency/manager.rs:213-216
let branch_lock = self.commit_locks
    .entry(txn.branch_id)
    .or_insert_with(|| Mutex::new(()));
```

**Assessment**: Currently safe — closure is trivial (`Mutex::new()`). Would deadlock if closure accessed the same DashMap. Should be documented.

### Pattern 2: Iteration during concurrent mutation — Verified Safe

Tests in `storage/sharded.rs` verify concurrent reads/writes don't deadlock or corrupt data. DashMap's per-shard locking handles this correctly.

### Pattern 3: get() + read guard — Safe

All DashMap read guards are short-lived with no cross-await operations.

---

## 4. Arc Cycle Analysis

**No cycles detected.**

- `Database` → `Arc<ShardedStore>` — one-way, no back-reference
- `Executor` → `Arc<Primitives>` → `Arc<Database>` — one-way chain
- `OPEN_DATABASES` registry uses `Weak<Database>` — prevents cycles
- `ClonedSnapshotView` wraps `Arc<BTreeMap>` — immutable, no cycle

---

## 5. Thread Spawning

### Production Thread: WAL Flush (`database/mod.rs:323-335`)

- Named thread (`strata-wal-flush`)
- Graceful shutdown via `AtomicBool` flag
- Handle stored and joined on `Database::drop()`
- Uses parking_lot mutex (no poisoning on thread panic)
- `.expect()` on spawn failure — justified (OS resource exhaustion is unrecoverable)

**Assessment**: Well-designed background thread.

### No Other Production Threads

All other `thread::spawn` calls are in test code, properly joined with `.unwrap()`.

---

## 6. Send/Sync Boundaries

- `Executor`: `unsafe impl Send + Sync` — verified sound (see Phase 1b audit)
- All primitive types (`KVStore`, `JsonStore`, etc.) contain only `Arc<Database>` — automatically Send+Sync
- `SimpleFuser`, `RRFFuser`: Send+Sync assertions verified at compile time
- No `!Send` or `!Sync` types found in shared contexts

---

## 7. Known Issues (from `critical_audit_tests.rs`)

The codebase documents 9 known concurrency risks in test annotations:

| Issue | Description | Status |
|-------|-------------|--------|
| #594 | TOCTOU race in validation/apply | Mitigated by per-branch locks |
| #596 | RwLock poisoning cascade | Mitigated by parking_lot |
| #597 | SystemTime panic on clock backwards | Risk present (`.unwrap()`) |
| #598 | WAL Mutex poisoning | Mitigated by parking_lot |
| #599 | Standard durability silent failure | Mitigated by WAL-then-storage order |
| #600 | Wire encoding precision loss (u64 > 2^53) | Risk present for JSON wire format |

---

## 8. Risk Matrix

| Area | Risk | Notes |
|------|------|-------|
| DashMap sharding | LOW | Excellent per-branch isolation |
| parking_lot locks | LOW | No poisoning cascades |
| Atomic ordering | LOW | Correct SeqCst/Relaxed usage |
| Arc patterns | LOW | No cycles, Weak refs used |
| Thread safety | LOW | All shared types are Send+Sync |
| Lock ordering | LOW | Works correctly, but undocumented |
| `OPEN_DATABASES` registry | LOW | std::sync::Mutex, short critical section |
| `entry().or_insert_with()` | LOW | Trivial closure, but footgun potential |
| SystemTime unwrap | LOW | Edge case (clock backwards) |

---

## 9. Recommendations

### Pre-MVP (minor)

1. **Document lock ordering convention** — Add a module-level comment in `concurrency/manager.rs` describing the lock hierarchy.

2. **Add SAFETY comment to `entry().or_insert_with()`** — Document why the closure must remain trivial.

3. **Change `OPEN_DATABASES.lock().unwrap()` to `.expect()`** — Better panic message if registry is poisoned. Or replace with `parking_lot::Mutex` for consistency.

### Post-MVP

4. Replace the sole `std::sync::Mutex` with `parking_lot::Mutex` for consistency.
5. Add graceful handling for `SystemTime` backwards clock edge case.
6. Add lock contention metrics for production monitoring.

---

## Methodology

Searched all crates for `Mutex`, `RwLock`, `DashMap`, `Arc`, `Atomic`, `thread::spawn`, `unsafe impl Send`, `unsafe impl Sync`. Read surrounding context for each synchronization primitive. Traced lock acquisition paths for deadlock potential. Verified DashMap usage patterns against known footguns.
