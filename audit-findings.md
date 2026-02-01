# Strata-Core Architectural Audit Findings

**Date**: 2026-01-31
**Branch**: `cleanup/run-to-branch-stragglers`
**Scope**: Full codebase audit — all crates (executor, engine, storage, core, concurrency, durability)

---

## CRITICAL

### AUD-01: `to_stored_value` silently returns `Value::Null` on serialization failure

**Crate**: `engine`
**Files**: `crates/engine/src/primitives/kv.rs`, `crates/engine/src/primitives/state.rs`, `crates/engine/src/primitives/json.rs`
**Impact**: Silent data corruption

When `serde_json::to_vec()` fails during a write operation, `to_stored_value()` returns a `StoredValue` containing `Value::Null` instead of propagating the error. This means a value that cannot be serialized (e.g., too large, contains unsupported types) is silently replaced with `Null` — the caller receives a success response while their data is lost.

**Fix**: Propagate the serialization error to the caller as `Result::Err`.

---

### AUD-02: 5 additional commands bypass Session transaction scope

**Crate**: `executor`
**File**: `crates/executor/src/session.rs`
**Impact**: Broken snapshot isolation
**Related**: Issue #837 (`StateSet` bypass)

The following read commands are not routed through the transaction layer:
- `StateReadv`
- `KvGetv`
- `JsonGetv`
- `JsonList`
- `Search`

These commands read directly from the store, bypassing any in-flight transaction write-set. A user inside an explicit transaction who calls `kv_getv()` will see committed state, not their own uncommitted writes.

**Fix**: Route these commands through the transaction dispatch path, or document them as non-transactional and return an error when called inside an active transaction.

---

## HIGH

### AUD-03: 28 `expect()` panics on `None` branch in `to_core_branch_id()`

**Crate**: `executor`
**File**: `crates/executor/src/bridge.rs`
**Impact**: Server panic on invalid input

`to_core_branch_id()` uses `.expect()` which panics the process when given an invalid branch identifier. Any malformed branch name from a client will crash the server rather than returning an error.

**Fix**: Replace `.expect()` with `.ok_or_else(|| Error::InvalidInput(...))` and propagate the error.

---

### AUD-04: Manual `unsafe impl Send/Sync` on `Primitives`

**Crate**: `executor`
**File**: `crates/executor/src/bridge.rs`
**Impact**: Potential undefined behavior

`unsafe impl Send for Primitives` and `unsafe impl Sync for Primitives` are declared without safety comments or auditing that all inner types satisfy the contracts. If any inner type is not truly `Send`/`Sync`, this is undefined behavior.

**Fix**: Audit all inner types. If they are all `Send`/`Sync`, add `// SAFETY:` comments. If not, remove the unsafe impls and fix the underlying type constraints.

---

### AUD-05: Transaction context leak on panic

**Crate**: `executor`
**File**: `crates/executor/src/session.rs`
**Impact**: Session stuck in broken state after panic

If a command handler panics mid-transaction (via `expect()`, index out-of-bounds, etc.), the `Session` retains its transaction context. Subsequent commands on the same session will either fail or operate on a corrupted transaction state. The `Drop` impl may also fail to clean up properly.

**Fix**: Wrap handler execution in `catch_unwind` and auto-rollback the transaction on panic, or use a poison flag.

---

### AUD-06: Event hash algorithm diverges between `EventLog` and `Transaction`

**Crate**: `engine`
**Files**: `crates/engine/src/primitives/event.rs`, `crates/engine/src/transaction.rs`
**Impact**: Event chain verification failure

`EventLog` and `Transaction` compute event content hashes with different algorithms/inputs. When events written through a transaction are later verified through the `EventLog` chain integrity check, the hashes will not match.

**Fix**: Extract hash computation into a single shared function used by both code paths.

---

### AUD-07: `Transaction` wrapper cannot read pre-existing data from the store

**Crate**: `engine`
**File**: `crates/engine/src/transaction.rs`
**Impact**: Incomplete read-your-writes; reads of pre-existing data may return `None`

The `Transaction<'a>` struct only reads from its local write-set. If a key existed in the store before the transaction began, a `get()` call within the transaction returns `None` rather than the pre-existing value. Only keys written within the transaction are visible.

**Fix**: Fall through to the underlying store when the write-set does not contain the key.

---

### AUD-08: `from_stored_value` silently resets metadata on deserialization failure

**Crate**: `engine`
**Files**: `crates/engine/src/primitives/kv.rs`, `crates/engine/src/primitives/state.rs`
**Impact**: Silent metadata loss

When metadata deserialization fails, the function substitutes `Default::default()` metadata instead of returning an error. This silently drops timestamps, version info, or other metadata attached to the value.

**Fix**: Propagate the deserialization error.

---

### AUD-09: Bundle export does not capture version history

**Crate**: `engine`
**File**: `crates/engine/src/bundle.rs`
**Impact**: Data loss on export/import

Bundle export only captures the current (latest) value for each key/cell. The full version chain is not included. An import from an exported bundle loses all historical versions.

**Fix**: Include version history in the bundle format, or document the limitation clearly.

---

### AUD-10: `ShardedStore::list()` and `count()` do not filter tombstones

**Crate**: `storage`
**File**: `crates/storage/src/sharded.rs`
**Impact**: Incorrect results — deleted entries counted/returned as live

Four methods in `ShardedStore` — `list()`, `list_range()`, `count()`, and `count_range()` — iterate over all entries including tombstoned (deleted) ones. A `kv_list()` or `kv_count()` call will include keys that have been deleted.

**Fix**: Add `!sv.is_tombstone()` filter to all enumeration methods.

---

### AUD-11: No automatic GC trigger for version chains

**Crate**: `storage`
**File**: `crates/storage/src/sharded.rs`
**Impact**: Unbounded memory growth

Version chains grow without limit as values are updated. There is no compaction, pruning, or size-triggered GC. A frequently-updated key will accumulate an ever-growing chain of old versions.

**Fix**: Implement a configurable retention policy (e.g., max versions per key, max age) and trigger GC during compaction or on a threshold.

---

## MEDIUM

### AUD-12: Event handler does not validate `event_type` string

**Crate**: `executor`
**File**: `crates/executor/src/handlers/event.rs`
**Impact**: Empty or whitespace-only event types accepted

The event append handler does not validate the `event_type` parameter. An empty string or whitespace-only string is accepted and stored.

**Fix**: Validate `event_type` is non-empty and trimmed, or reuse `validate_key()`.

---

### AUD-13: Vector handler does not validate key format

**Crate**: `executor`
**File**: `crates/executor/src/handlers/vector.rs`
**Impact**: Invalid keys accepted for vector operations

Vector operations (create collection, upsert, get, delete, search) do not validate collection names or vector IDs against the key format rules enforced by other primitives.

**Fix**: Apply `validate_key()` to collection names and vector IDs.

---

### AUD-14: `state_cas` handler swallows all errors as `None`

**Crate**: `executor`
**File**: `crates/executor/src/handlers/state.rs`
**Impact**: CAS failures indistinguishable from version mismatch

The `state_cas` handler catches all `Err(_)` results from `p.state.cas()` and returns `Output::MaybeVersion(None)`. A legitimate internal error (e.g., I/O failure, serialization bug) is reported to the user as "version mismatch" rather than an error.

**Fix**: Match on specific error kinds — return `None` only for version conflicts, propagate other errors.

---

### AUD-15: `FilterOp` variants beyond `Eq` silently ignored

**Crate**: `executor`
**File**: `crates/executor/src/handlers/kv.rs`
**Impact**: Incorrect query results

The KV list filter only implements the `Eq` operator. Other `FilterOp` variants (`Lt`, `Gt`, `Gte`, `Lte`, `Ne`, `Contains`) silently match nothing, returning an empty result instead of an error.

**Fix**: Return `Error::NotImplemented` for unsupported filter operators, or implement them.

---

### AUD-16: No branch existence check on `TxnBegin`

**Crate**: `executor`
**File**: `crates/executor/src/session.rs`
**Impact**: Transaction opened on non-existent branch succeeds, fails later

`TxnBegin { branch: Some("nonexistent") }` succeeds and creates a transaction context. The error only surfaces on the first read/write operation. This is confusing for users.

**Fix**: Validate the branch exists (if specified) before creating the transaction context.

---

### AUD-17: Branch index update outside transaction (not atomic)

**Crate**: `engine`
**File**: `crates/engine/src/primitives/branch/index.rs`
**Impact**: Index inconsistency on crash

Branch metadata updates (create, delete) modify the in-memory index and persist to storage in separate steps. A crash between the two leaves the index and storage out of sync.

**Fix**: Wrap index + storage update in a single atomic operation or ensure recovery rebuilds the index from storage.

---

### AUD-18: Global recovery registry uses poisonable `Mutex`

**Crate**: `engine`
**File**: `crates/engine/src/recovery/registry.rs`
**Impact**: All future recovery attempts fail after a single panic

The global `REGISTRY` uses `std::sync::Mutex`. If any thread panics while holding the lock, the mutex is poisoned and all subsequent `register_*()` or `recover_all_participants()` calls will fail.

**Fix**: Use `parking_lot::Mutex` (non-poisonable) or handle the poisoned state.

---

### AUD-19: `begin_transaction` not gated by shutdown flag

**Crate**: `engine`
**File**: `crates/engine/src/transaction.rs`
**Impact**: New transactions can start during shutdown

After `Database::shutdown()` is called, new transactions can still be created. Writes during shutdown may be lost or cause errors.

**Fix**: Check the shutdown flag in `begin_transaction()` and return an error.

---

### AUD-20: `EventLogMeta` written with empty streams on init

**Crate**: `engine`
**File**: `crates/engine/src/primitives/event.rs`
**Impact**: Unnecessary empty metadata entries in storage

When the event log is initialized, an `EventLogMeta` with an empty streams map is written to storage. This is harmless but wasteful and can confuse debugging.

---

### AUD-21: Version chain ordering not enforced in storage

**Crate**: `storage`
**File**: `crates/storage/src/sharded.rs`
**Impact**: Version history may return entries in wrong order

The version chain is a linked list prepended on each write. If a bug or concurrent write causes misordering, there is no validation or repair. Queries that assume newest-first ordering may return incorrect results.

**Fix**: Add a debug assertion that the version being prepended is strictly greater than the chain head.

---

### AUD-22: `contains()` ignores tombstones

**Crate**: `storage`
**File**: `crates/storage/src/sharded.rs`
**Impact**: `contains("deleted-key")` returns `true`

The `contains()` method checks for key existence without filtering tombstones. A deleted key reports as existing.

**Fix**: Check `!sv.is_tombstone()` in the `contains()` implementation.

---

### AUD-23: TOCTOU race in `delete_with_version`

**Crate**: `storage`
**File**: `crates/storage/src/sharded.rs`
**Impact**: Double-delete or version mismatch under concurrency

`delete_with_version()` reads the current version, checks it matches the expected version, then writes the tombstone — in separate steps without holding a lock across the entire operation. A concurrent write between the read and the write can cause incorrect behavior.

**Fix**: Hold the shard lock across the entire read-check-write sequence.

---

### AUD-24: TTLIndex is disconnected (never checked or enforced)

**Crate**: `storage`
**File**: `crates/storage/src/ttl.rs`
**Impact**: TTL entries are indexed but never expire

The `TTLIndex` tracks expiration times but no background task or read-path check enforces expiry. Keys with a TTL never actually expire.

**Fix**: Either implement TTL enforcement (background reaper or read-time check) or remove the TTL infrastructure to avoid confusion.

---

### AUD-25: `Value::Null` ambiguous with tombstones after serde round-trip

**Crate**: `core`
**File**: `crates/core/src/types.rs`
**Impact**: Potential confusion after serialization round-trip

Although `StoredValue::is_tombstone` was added (#825), after a serde round-trip through external formats (JSON export, bundle), the `is_tombstone` flag may not survive, collapsing `Value::Null` and tombstones back to being indistinguishable.

**Fix**: Ensure the `is_tombstone` flag is included in all serialization formats, or use a sentinel wrapper type.

---

### AUD-26: `State::with_version` accepts non-Counter versions silently

**Crate**: `core`
**File**: `crates/core/src/primitives/state.rs`
**Impact**: Incorrect version type stored

`State::with_version()` accepts any `Version` variant (Counter, Hash, Timestamp) without validating that it's a `Counter`. State cells are documented to use counter-based versioning, but a `Version::Hash` can be passed in and stored.

**Fix**: Validate the version variant or use a typed `CounterVersion` newtype.

---

## LOW

### AUD-27: Hardcoded database UUID

**Crate**: `engine`
**File**: `crates/engine/src/database.rs`
**Impact**: All databases have the same UUID — not useful as an identifier

### AUD-28: 15 unused `Output` variants

**Crate**: `executor`
**File**: `crates/executor/src/types.rs`
**Impact**: Dead code — confusing for contributors

### AUD-29: Doc comment mismatches

**Crate**: Various
**Impact**: Comments describe behavior that doesn't match implementation

### AUD-30: Non-deterministic timestamps in tests

**Crate**: `engine`
**Impact**: Flaky test potential — timestamps depend on wall clock

### AUD-31: Incorrect date formatting in metadata

**Crate**: `engine`
**Impact**: Cosmetic — dates formatted incorrectly in some metadata fields

### AUD-32: Precision/rounding issues in vector distance calculations

**Crate**: `core`
**File**: `crates/core/src/primitives/vector.rs`
**Impact**: Minor inaccuracy in similarity scores for edge cases

### AUD-33: Missing validation guards on various public constructors

**Crate**: `core`, `storage`
**Impact**: Invalid states constructible from public API

---
---

# Phase 2: Concurrency, Storage (Deep), and Durability Audit

---

## CRITICAL

### AUD-34: Per-branch commit lock serializes all non-conflicting transactions

**Crate**: `concurrency`
**File**: `crates/concurrency/src/manager.rs` (lines 72–83)
**Impact**: Throughput bottleneck

A single `Mutex<()>` per branch serializes ALL commits on that branch, even transactions touching entirely different keys. OCC should allow non-conflicting transactions to commit in parallel, but the coarse lock prevents this.

**Fix**: Consider key-range or key-hash locking for finer granularity, or release the lock earlier in the commit path.

---

### AUD-35: Commit lock held during WAL I/O — validation-to-apply window too wide

**Crate**: `concurrency`
**File**: `crates/concurrency/src/manager.rs` (lines 184–256)
**Impact**: Branch-level stall during disk I/O

The per-branch commit lock is held across validation, WAL write (disk I/O), and storage apply — ~70 lines of code including potential disk fsync. Other transactions on the same branch are completely blocked during WAL flush.

**Fix**: Release the lock after WAL durability point and before storage apply, or move WAL write outside the critical section.

---

### AUD-36: "Snapshot isolation" terminology misleading — actually OCC with write-skew allowed

**Crate**: `concurrency`
**File**: `crates/concurrency/src/snapshot.rs` (lines 1–25)
**Impact**: Applications may assume write-skew prevention

The code uses "snapshot isolation" terminology but implements OCC with first-committer-wins. True snapshot isolation prevents write skew; this implementation does not. Two transactions reading each other's keys and writing the other can both commit.

**Fix**: Clearly label the isolation level as "OCC (write skew allowed)" in documentation and API docs.

---

### AUD-37: Unbounded clones during list operations hold DashMap shard locks

**Crate**: `storage`
**File**: `crates/storage/src/sharded.rs` (lines 477–530)
**Impact**: Memory spike + shard lock contention

`list_branch()`, `list_by_prefix()`, and `list_by_type()` clone every key and value while holding the DashMap shard lock. For N keys, this is N Key clones + N VersionedValue clones before the lock is released. Large values cause GB-scale allocations under lock.

**Fix**: Use a streaming/iterator API, or clone keys only (defer value reads), or use `Arc<Value>` to make clones cheap.

---

### AUD-38: WAL write ordering — segment rotation uses post-encoding size

**Crate**: `durability`
**File**: `crates/durability/src/format/wal_record.rs` (lines 145–159)
**Impact**: Records may land in wrong segment

The WAL writer checks segment size after encoding (which may differ from original size if codec compresses/expands). If the encoded size differs from the pre-check estimate, the rotation decision may be incorrect.

**Fix**: Determine encoded size before the rotation decision, or encode first, then check, then rotate if needed before writing.

---

### AUD-39: Snapshot parent directory fsync not guaranteed after rename

**Crate**: `durability`
**File**: `crates/durability/src/disk_snapshot/writer.rs` (lines 126–131)
**Impact**: Snapshot not durable after crash

The snapshot uses write-fsync-rename but the final `dir.sync_all()` on the parent directory happens after the rename with no recovery if it fails. A crash between rename and parent fsync means the snapshot file may not be visible after recovery.

**Fix**: Handle fsync failure by retrying or marking snapshot as unverified.

---

### AUD-40: MANIFEST CRC not validated before trusting codec ID

**Crate**: `durability`
**File**: `crates/durability/src/recovery/coordinator.rs` (lines 87–115)
**Impact**: Silent data corruption during recovery

The recovery coordinator validates codec ID after loading the MANIFEST, but doesn't independently verify MANIFEST integrity. If MANIFEST is corrupted (CRC passes by coincidence or CRC itself is corrupted), wrong codec could be used for snapshot loading.

**Fix**: Validate MANIFEST CRC independently before trusting any field.

---

## HIGH

### AUD-41: Storage errors silently treated as version 0 in validation

**Crate**: `concurrency`
**File**: `crates/concurrency/src/validation.rs` (lines 148–173)
**Impact**: Transaction commits despite I/O failure

During read-set validation, if `store.get(key)` returns `Err(_)`, the error is silently treated as version 0 (key not found). An I/O failure or storage corruption could cause a transaction to commit when it should have aborted.

**Fix**: Propagate storage errors to the caller. Log the error. Fail the transaction on I/O error.

---

### AUD-42: Read-only transactions unnecessarily acquire per-branch commit lock

**Crate**: `concurrency`
**File**: `crates/concurrency/src/manager.rs` (lines 188–189)
**Impact**: Read-only transactions block writers and vice versa

Read-only transactions always succeed validation (validation.rs line 336) but still acquire the per-branch commit lock. This serializes reads behind writes unnecessarily.

**Fix**: Add fast-path: check `txn.is_read_only()` before acquiring the lock and skip it entirely.

---

### AUD-43: No transaction priority or timeout mechanism

**Crate**: `concurrency`
**File**: `crates/concurrency/src/manager.rs` (lines 188–189)
**Impact**: Starvation and SLA violations

`parking_lot::Mutex::lock()` blocks indefinitely with no timeout. One slow transaction (e.g., large WAL write) blocks all other transactions on the same branch with no way to abort, cancel, or prioritize.

**Fix**: Use `try_lock_for(Duration)` or implement transaction-level timeouts.

---

### AUD-44: `SnapshotView` trait doesn't require `Sync`

**Crate**: `concurrency`
**File**: `crates/concurrency/src/transaction.rs` (lines 335–405)
**Impact**: Potential data race with interior mutability

`TransactionContext` takes a `Box<dyn SnapshotView>` but the trait doesn't require `Sync`. If an implementation uses `RefCell` or non-atomic interior mutability, concurrent access is undefined behavior.

**Fix**: Add `Sync` bound to the `SnapshotView` trait, or document the thread-safety requirement.

---

### AUD-45: DashMap iterator contention — shard lock held during full clone

**Crate**: `storage`
**File**: `crates/storage/src/sharded.rs` (lines 477–530)
**Impact**: Writers blocked during list operations

DashMap's `get()` holds a read lock on the shard bucket. During list operations, this lock is held for the entire duration of iterating and cloning all entries in that shard. Writers targeting any key in the same shard are blocked.

**Fix**: Minimize lock hold time — snapshot the keys, release the lock, then read values individually.

---

### AUD-46: `apply_batch()` not atomic — concurrent readers see partial state

**Crate**: `storage`
**File**: `crates/storage/src/sharded.rs` (lines 419–451)
**Impact**: Dirty reads during batch apply

`apply_batch()` applies writes one-by-one, each acquiring and releasing the shard lock independently. A concurrent reader between two writes in the same batch sees partial (inconsistent) state.

**Fix**: Group batch operations by shard and apply each shard's writes under a single lock acquisition.

---

### AUD-47: `StoredValue` has no serialization versioning

**Crate**: `storage`
**File**: `crates/storage/src/stored_value.rs` (lines 1–141)
**Impact**: No upgrade path for schema changes

`StoredValue` has no format version marker. If the struct evolves (new fields, changed serialization), persisted data from older versions cannot be read. No backwards-compatibility mechanism exists.

**Fix**: Add a version byte/header to the serialized format.

---

### AUD-48: Secondary indexes (`BranchIndex`, `TypeIndex`) not thread-safe

**Crate**: `storage`
**File**: `crates/storage/src/index.rs` (lines 17–134)
**Impact**: Data race if indexes are ever used concurrently

All methods on `BranchIndex` and `TypeIndex` require `&mut self`. They cannot be called concurrently. Currently dead code, but if ever activated, they would cause data races with `ShardedStore`'s `&self` API.

**Fix**: Wrap in `RwLock` or `DashMap` if they will be used, or remove them.

---

### AUD-49: Segment rotation doesn't verify old segment fsync succeeded

**Crate**: `durability`
**File**: `crates/durability/src/wal/writer.rs` (lines 212–229)
**Impact**: Data loss if old segment fsync fails

During segment rotation, the old segment is closed with `sync()`, but if the internal fsync fails, the new segment is created anyway. Data in the old segment is not guaranteed durable.

**Fix**: Check return value of old segment close/sync. Abort rotation if fsync fails.

---

### AUD-50: Batched mode fsync may exceed configured `interval_ms`

**Crate**: `durability`
**File**: `crates/durability/src/wal/writer.rs` (lines 179–192)
**Impact**: Data loss window exceeds configured durability guarantee

In Batched mode, fsync timing depends on `Instant::elapsed()` which is only checked on write. If no writes occur, no fsync is triggered. A write followed by a long pause and then a crash could lose data written `interval_ms` ago.

**Fix**: Add a background timer thread or check-on-read to ensure fsync happens within the configured interval.

---

### AUD-51: Partial WAL record truncation not atomic with recovery

**Crate**: `durability`
**File**: `crates/durability/src/wal/reader.rs` (lines 99–129)
**Impact**: Duplicate record processing on crash during recovery

Recovery identifies partial records but truncation happens separately. If a crash occurs between recovery and truncation, the partial record is re-processed on next recovery, potentially creating duplicate state.

**Fix**: Make truncation part of the atomic recovery operation, or use idempotent replay.

---

### AUD-52: MANIFEST parent directory fsync failure may be ignored

**Crate**: `durability`
**File**: `crates/durability/src/format/manifest.rs` (lines 225–234)
**Impact**: MANIFEST not durable after atomic rename

After atomic rename of MANIFEST, parent directory fsync can fail. If the calling code doesn't check the `Result`, the fsync failure is silently ignored and MANIFEST may not be visible after crash.

**Fix**: Ensure all callers propagate the fsync error.

---

### AUD-53: Tombstone cleanup in compaction not persisted atomically

**Crate**: `durability`
**File**: `crates/durability/src/compaction/tombstone.rs` (lines 350–364)
**Impact**: Deleted entries reappear after crash

`cleanup_before()` modifies tombstones in-memory only. If the process crashes before the tombstone index is persisted to a snapshot, cleaned-up tombstones are lost and deleted entries reappear.

**Fix**: Persist tombstone cleanup atomically, or re-derive tombstone state from WAL during recovery.

---

### AUD-54: Snapshot watermark not cross-checked against WAL records

**Crate**: `durability`
**File**: `crates/durability/src/disk_snapshot/reader.rs`
**Impact**: Duplicate transaction application

The snapshot reader loads sections but doesn't validate that the snapshot watermark matches actual WAL transaction IDs. A corrupted watermark could cause already-included transactions to be re-applied.

**Fix**: Cross-check watermark against WAL records during recovery.

---

### AUD-55: Bundle manifest uses non-cryptographic checksum (XXH3)

**Crate**: `durability`
**File**: `crates/durability/src/branch_bundle/writer.rs` (lines 95–99)
**Impact**: Bundle tampering may go undetected

The bundle manifest checksums use XXH3 (non-cryptographic hash). An attacker could forge a manifest with matching checksums.

**Fix**: Use SHA-256 or BLAKE3 for bundle integrity, or document that bundles are not tamper-resistant.

---

### AUD-56: WAL checksum valid but payload parse fails — misleading error

**Crate**: `durability`
**File**: `crates/durability/src/format/wal_record.rs` (lines 394–454)
**Impact**: Incorrect corruption diagnosis

If CRC matches but payload structure is invalid, the error reports `InvalidFormat` without indicating the checksum was valid. Recovery logs may incorrectly report file corruption when it's just a truncation or version mismatch.

**Fix**: Include checksum validation status in format errors to aid diagnosis.

---

### AUD-57: Recovery doesn't distinguish file-not-found from I/O error

**Crate**: `durability`
**File**: `crates/durability/src/recovery/coordinator.rs` (lines 139–150)
**Impact**: Disk I/O failures silently ignored

The recovery coordinator treats "file not found" (expected — no snapshot yet) and "I/O error" (unexpected — disk failure) the same way. A real disk error could be silently ignored.

**Fix**: Match on `ErrorKind::NotFound` separately from other I/O errors.

---

### AUD-58: Codec decode errors not diagnosable

**Crate**: `durability`
**File**: `crates/durability/src/format/primitives.rs` (lines 140–142)
**Impact**: Cannot distinguish bad codec from corrupted data

When `codec.decode()` fails, there's no way to distinguish between wrong codec, corrupted data, or version mismatch. All failures produce the same opaque error.

**Fix**: Include codec ID and data length in error context.

---

## MEDIUM

### AUD-59: DashMap entry() lock ordering not documented

**Crate**: `concurrency`
**File**: `crates/concurrency/src/manager.rs` (lines 83, 188–189)
**Impact**: Future deadlock risk

DashMap's `entry()` API holds an internal shard lock during `or_insert_with()`. This creates an implicit lock ordering (DashMap shard lock -> branch Mutex) that is not documented. Future refactoring could reverse the order and deadlock.

**Fix**: Add a comment documenting the lock ordering hierarchy.

---

### AUD-60: Version allocation gap on failed commit

**Crate**: `concurrency`
**File**: `crates/concurrency/src/manager.rs` (lines 136–138, 200, 213)
**Impact**: Non-contiguous version numbers

Version is allocated via `fetch_add` before WAL write. If WAL write fails, the version number is consumed but never used, creating gaps in the version sequence.

**Fix**: Document that version gaps are expected by design, or defer version allocation until after WAL success.

---

### AUD-61: SeqCst ordering on all atomics may be overly conservative

**Crate**: `concurrency`
**File**: `crates/concurrency/src/manager.rs` (lines 114, 119, 137)
**Impact**: Unnecessary performance overhead on high-core systems

All atomic operations use `Ordering::SeqCst` without justification. `Acquire`/`Release` semantics would suffice for most operations and reduce cross-core synchronization overhead.

**Fix**: Analyze ordering requirements and add comments. Relax to `Acquire`/`Release` where safe.

---

### AUD-62: Panic during commit leaves transaction in indeterminate state

**Crate**: `concurrency`
**File**: `crates/concurrency/src/manager.rs` (lines 189–256)
**Impact**: Subsequent transactions on same branch see inconsistent state

`parking_lot::Mutex` doesn't poison on panic (the lock is released), but the transaction may be left in a `Validating` state — partially committed. The next transaction on the same branch observes inconsistent state.

**Fix**: Add `catch_unwind` around the critical section and ensure rollback on panic.

---

### AUD-63: Validation re-reads storage for every key in read-set

**Crate**: `concurrency`
**File**: `crates/concurrency/src/validation.rs` (lines 148–173)
**Impact**: Storage contention during commit

Every commit validation calls `store.get()` for every key in the read-set. If 100 concurrent transactions each read 100 keys, that's 10,000 storage reads during validation alone. No caching of validation results.

**Fix**: Batch read-set validation into a single storage scan, or cache recently-validated versions.

---

### AUD-64: CAS operations don't add to read-set

**Crate**: `concurrency`
**File**: `crates/concurrency/src/transaction.rs` (lines 148–165, 715–748)
**Impact**: Undocumented isolation semantics

CAS does not add to the read-set by design, but this means a CAS + write to the same key within a transaction has no conflict detection. The interaction is undocumented and could surprise developers.

**Fix**: Document the CAS/read-set semantics explicitly in API docs.

---

### AUD-65: Every `get()` clones entire `VersionedValue`

**Crate**: `storage`
**File**: `crates/storage/src/sharded.rs` (lines 901–934)
**Impact**: High allocation overhead on read-heavy workloads

Every `Storage::get()` call clones the entire `VersionedValue` including the value payload. For large strings/vecs, this is expensive. No copy-on-write or ref-counting optimization.

**Fix**: Use `Arc<VersionedValue>` to make clones cheap (pointer copy), or return a reference guard.

---

### AUD-66: Snapshot version not protected from GC reclaim

**Crate**: `storage`
**File**: `crates/storage/src/sharded.rs` (lines 708–713)
**Impact**: Long-lived snapshots become silently invalid

`ShardedSnapshot` holds a version number but doesn't pin that version against GC. If a GC runs and reclaims versions older than the snapshot's version, the snapshot can no longer read any data.

**Fix**: Implement version pinning — track active snapshot versions and prevent GC from reclaiming pinned versions.

---

### AUD-67: Watermark filtering assumes monotonic txn_ids

**Crate**: `durability`
**File**: `crates/durability/src/recovery/replayer.rs` (lines 82–118)
**Impact**: Committed transactions skipped during recovery

Replay filters by `record.txn_id <= watermark` assuming IDs are monotonically increasing. If txn IDs have gaps or non-monotonic segments (e.g., from branch merges), committed transactions could be incorrectly skipped.

**Fix**: Validate monotonicity assumption or use a set of applied txn_ids instead of a single watermark.

---

### AUD-68: Segment sync not called in all write failure paths

**Crate**: `durability`
**File**: `crates/durability/src/format/wal_record.rs` (lines 293–299)
**Impact**: Partial writes may not be fsynced before close

If `write()` fails after partial flush, `write_position` is updated regardless of fsync status. The `close()` method calls `sync_all()` but the position tracking may be incorrect.

**Fix**: Only update `write_position` after successful fsync, or verify position on close.

---

### AUD-69: Snapshot creation not point-in-time consistent

**Crate**: `durability`
**File**: `crates/durability/src/snapshot.rs`
**Impact**: Snapshot captures partial state of concurrent transactions

Snapshot creation reads all primitive state but has no mechanism to ensure reads are consistent with a single point in time. State changes during snapshot creation produce an internally inconsistent snapshot.

**Fix**: Acquire a read lock or version pin before reading state for snapshot.

---

### AUD-70: No graceful downgrade for future WAL format versions

**Crate**: `durability`
**File**: `crates/durability/src/format/wal_record.rs` (lines 436–439)
**Impact**: Single unknown record stops entire WAL replay

If format version is unknown, parsing fails completely. There is no partial recovery mode — a single record with a newer format version stops the entire replay.

**Fix**: Skip unknown format versions with a warning, or implement a forward-compatible record envelope.

---

### AUD-71: No magic number validation for codec header

**Crate**: `durability`
**File**: `crates/durability/src/disk_snapshot/reader.rs`
**Impact**: Wrong codec used for decompression

Codec ID is read but never validated against a known registry of magic numbers. A corrupted codec ID could silently select the wrong codec, producing corrupted data.

**Fix**: Validate codec ID against a known-good set before using it.

---

### AUD-72: No integrity check on imported WAL.branchlog payload

**Crate**: `durability`
**File**: `crates/durability/src/branch_bundle/wal_log.rs`
**Impact**: Importing corrupted bundle causes data corruption

The BranchBundle import doesn't validate that WAL payloads match the expected database schema. Importing a bundle from a different database version could apply mutations to wrong entities.

**Fix**: Validate bundle schema version matches target database.

---

### AUD-73: Retention policy timestamp comparison has overflow risk

**Crate**: `durability`
**File**: `crates/durability/src/retention/policy.rs` (lines 131–134)
**Impact**: Old entries retained forever

`saturating_sub()` can mask timing bugs — if `current_time < duration.as_micros()`, cutoff saturates to 0 and all entries are retained regardless of age.

**Fix**: Log a warning when saturation occurs. Validate that `current_time` is reasonable.

---

### AUD-74: `WalWriter` not explicitly marked `Send + Sync`

**Crate**: `durability`
**File**: `crates/durability/src/wal/writer.rs` (lines 28–58)
**Impact**: Potential data race if shared across threads

`WalWriter` contains file handles and `Instant` but is not explicitly marked or tested for `Send + Sync`. If the engine shares it across threads without synchronization, data corruption could occur.

**Fix**: Add static assertions for `Send` (if intended) or document single-threaded usage requirement.

---

## LOW

### AUD-75: WAL segment header database UUID never validated

**Crate**: `durability`
**File**: `crates/durability/src/format/wal_record.rs` (lines 164–204)
**Impact**: Segments from different databases could be mixed

When opening segments for reading, the database UUID in the segment header is read but never validated against the expected database UUID.

---

### AUD-76: Retention policy cannot be changed after branch creation

**Crate**: `durability`
**File**: `crates/durability/src/retention/policy.rs`
**Impact**: Inflexible — requires data re-write to change policy

---

### AUD-77: No locking coordination between snapshot creation and WAL writes

**Crate**: `durability`
**File**: Architecture-level
**Impact**: Snapshot may capture inconsistent state if engine doesn't coordinate (outside durability crate scope)
