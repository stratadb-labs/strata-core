# Crash Recovery / Durability Audit

Simulate crash at every write point. Verify: Does recovery restore a consistent state? Are there write sequences where data is acknowledged but lost? Does WAL replay correctly handle partial writes?

## 1. On-Disk Format Inventory

### WAL Segment Format

**File naming**: `wal-NNNNNN.seg` (6-digit zero-padded segment number)

**Segment header** (32 bytes, immutable):

```
Offset  Size  Field              Type
0       4     Magic              [u8; 4]     "STRA" (0x53545241)
4       4     Format version     u32 LE      Currently: 1
8       8     Segment number     u64 LE      Monotonically increasing
16      16    Database UUID      [u8; 16]    Integrity checking across segments
```

**Location**: `crates/durability/src/format/wal_record.rs:49-104`

**Header validation**: Magic bytes and format version are checked as constants. Segment number is verified against expected value. **No CRC on the header itself** — corruption in segment_number or database_uuid fields goes undetected.

### WAL Record Format

Each record is self-delimiting with length prefix and CRC32 checksum:

```
┌─────────────────┬──────────────────┬─────────────────────────┬──────────┐
│ Length (4 bytes) │ Format Ver (1)   │ Payload (variable)      │ CRC32 (4)│
└─────────────────┴──────────────────┴─────────────────────────┴──────────┘

Payload:
┌──────────────┬──────────────────┬──────────────┬─────────────────────────┐
│ TxnId (8)    │ BranchId (16)    │ Timestamp (8)│ Writeset (variable)     │
└──────────────┴──────────────────┴──────────────┴─────────────────────────┘
```

**Location**: `crates/durability/src/format/wal_record.rs:1-30, 364-388`

- **Length field** (u32 LE): Size of `format_version + payload + CRC32`. Does NOT include the length field itself.
- **CRC32**: Computed over payload only (everything after the length field, before the CRC). Uses `crc32fast` crate (ISO-HDLC polynomial).
- **Minimum payload size**: 33 bytes (1 version + 8 txn_id + 16 branch_id + 8 timestamp + 0 writeset).

### MANIFEST Format

**File**: `MANIFEST` (single file, atomically replaced)

```
+------------------+
| Magic: "STRM"    | 4 bytes
| Format Version   | 4 bytes (u32 LE)
| Database UUID    | 16 bytes
| Codec ID Length  | 4 bytes (u32 LE)
| Codec ID         | variable (UTF-8)
| Active WAL Seg   | 8 bytes (u64 LE)
| Snapshot Watermark| 8 bytes (u64 LE, 0 = none)
| Snapshot ID      | 8 bytes (u64 LE, 0 = none)
| CRC32            | 4 bytes
+------------------+
```

**Location**: `crates/durability/src/format/manifest.rs:1-20`

CRC32 computed over all preceding bytes. Validated on load (manifest.rs:116-124).

### Snapshot Format

**File naming**: `snap-NNNNNN.chk` (temp: `.snap-NNNNNN.tmp`)

```
┌────────────────────────────────────┐
│ Header (64 bytes)                  │
│   Magic "SNAP" (4)                 │
│   Format version (4)               │
│   Snapshot ID (8)                  │
│   Watermark TXN (8)               │
│   Created At (8)                   │
│   Database UUID (16)               │
│   Codec ID Length (1)              │
│   Reserved (15)                    │
├────────────────────────────────────┤
│ Codec ID (variable)               │
├────────────────────────────────────┤
│ Section 1: [Type(1) Len(8) Data]  │
│ Section 2: [Type(1) Len(8) Data]  │
│ ...                                │
├────────────────────────────────────┤
│ CRC32 (4 bytes)                   │
└────────────────────────────────────┘
```

**Location**: `crates/durability/src/disk_snapshot/writer.rs:64-141`

CRC32 computed over header + codec ID + all sections. Written as footer. Validated on load.

## 2. Commit Sequence

**Location**: `crates/concurrency/src/manager.rs:178-260`

The commit protocol has 5 steps with a clearly marked durability point:

```
TransactionManager::commit(txn, store, wal)
│
├─ [1] Acquire per-branch commit lock                    (manager.rs:188-192)
│      DashMap<BranchId, Mutex<()>> — prevents TOCTOU race
│
├─ [2] txn.commit(store)?                                (manager.rs:197)
│      Active → Validating → Committed (in-memory only)
│      Read-set validation: first-committer-wins
│
├─ [3] commit_version = allocate_version()               (manager.rs:203)
│      AtomicU64::fetch_add(1, SeqCst) + 1
│
├─ [4] WAL write (if WAL provided)                       (manager.rs:207-232)
│  ├─ Build TransactionPayload from write/delete/cas sets (manager.rs:208)
│  ├─ Create WalRecord(txn_id, branch_id, timestamp, payload.to_bytes())
│  ├─ wal.append(&record)?                               (manager.rs:216)
│  │  └─ Serialize → codec.encode() → segment.write() → maybe_sync()
│  └─ wal.flush()?                                       (manager.rs:223)
│     └─ segment.sync() — forces fsync
│
│  ═══ DURABILITY POINT (manager.rs:230) ═══
│  "Even if we crash after this, recovery will replay from WAL"
│
├─ [5] txn.apply_writes(store, commit_version)           (manager.rs:235)
│      Apply puts/deletes to in-memory storage
│      If fails after WAL: log error, return success (WAL is authoritative)
│
└─ Return Ok(commit_version)                             (manager.rs:259)
```

## 3. Durability Modes

**Location**: `crates/durability/src/wal/writer.rs:176-206`

| Mode | fsync Behavior | Data Loss Window | Latency |
|------|---------------|------------------|---------|
| `None` | No WAL created | All data | <3µs |
| `Strict` | fsync after every record (writer.rs:178-183) | Zero | 10ms+ |
| `Batched` | fsync on time/count/bytes threshold (writer.rs:185-198) | Up to interval_ms or batch_size writes | <30µs |

**Batched mode sync thresholds** (writer.rs:189-191):

```rust
let should_sync = self.writes_since_sync >= batch_size
    || self.last_sync_time.elapsed().as_millis() as u64 >= interval_ms
    || self.bytes_since_sync >= self.config.buffered_sync_bytes;
```

**Critical**: The `interval_ms` check only runs when a new write arrives in `maybe_sync()`. The `sync_if_overdue()` method (writer.rs:256-272) exists to handle the case where no new writes arrive, but it is **never called from any production code path**. A maintenance timer is needed to call it. This means in Batched mode, if writes stop arriving, buffered data can remain unfsynced indefinitely past `interval_ms`. (Existing issue #887)

## 4. WAL Write Path

**Location**: `crates/durability/src/wal/writer.rs:139-173`

```
WalWriter::append(record)
│
├─ [1] Skip if DurabilityMode::Cache                     (writer.rs:141-143)
│
├─ [2] record.to_bytes()                                 (writer.rs:151)
│      [Length(4)] + [FormatVer(1) + TxnId(8) + BranchId(16)
│       + Timestamp(8) + Writeset(var)] + [CRC32(4)]
│
├─ [3] codec.encode(&record_bytes)                       (writer.rs:154)
│      IdentityCodec: pass-through (encryption/compression seam)
│
├─ [4] Check segment rotation                            (writer.rs:157-159)
│      If segment.size() + encoded.len() > config.segment_size:
│      └─ rotate_segment() → close current (sync), create new
│
├─ [5] segment.write(&encoded)                           (writer.rs:163)
│      file.write_all(data) — buffered in OS
│
├─ [6] Update sync counters                              (writer.rs:165-167)
│
└─ [7] maybe_sync()                                      (writer.rs:170)
       Strict: always fsync
       Batched: conditional fsync
       None: no-op
```

**Segment rotation** (writer.rs:219-237): Close syncs old segment before creating new one. Safe on crash — old segment is immutable after close, new segment starts with a fresh header.

**WalWriter::drop** (writer.rs:338-345): Best-effort sync of unsynced data. Uses `let _ = segment.sync()` — errors silently discarded. This is appropriate for Drop, which cannot return errors.

## 5. Recovery Algorithm

**Location**: `crates/durability/src/recovery/coordinator.rs:87-164`

```
RecoveryCoordinator::recover(on_snapshot, on_record)
│
├─ [1] plan_recovery()                                   (coordinator.rs:87-115)
│  ├─ Load MANIFEST                                      (coordinator.rs:89)
│  ├─ Validate codec matches                             (coordinator.rs:93-98)
│  └─ Determine path: snapshot + WAL or WAL-only         (coordinator.rs:101-107)
│
├─ [2] Load snapshot if referenced by MANIFEST           (coordinator.rs:138-147)
│  ├─ SnapshotReader::load(snapshot_path)?
│  │  └─ Validate magic, CRC, codec. Parse sections.
│  └─ on_snapshot(sections)?
│      └─ Caller applies snapshot sections to storage
│
├─ [3] Replay WAL after watermark                        (coordinator.rs:150-153)
│  └─ WalReplayer::replay_after(watermark, on_record)
│     ├─ Read all segments in order                      (replayer.rs:97-101)
│     ├─ For each record:
│     │  ├─ if record.txn_id <= watermark: skip          (replayer.rs:107-111)
│     │  └─ else: apply_fn(&record)                      (replayer.rs:114)
│     └─ Stop at first invalid/partial record
│
├─ [4] Truncate partial records at WAL tail              (coordinator.rs:156)
│  └─ file.set_len(valid_end)?; file.sync_all()?
│
└─ Return RecoveryResult { manifest, watermark, replay_stats, bytes_truncated }
```

### Recovery Properties

1. **Deterministic**: Same MANIFEST + snapshot + WAL → same state (replayer.rs:7)
2. **Idempotent**: Multiple recoveries → same result (test at coordinator.rs:540-571)
3. **Atomic**: Either fully recovers or fails cleanly (coordinator.rs:13)
4. **Version-preserving**: WAL records store commit versions; recovery replays them exactly

### Watermark Semantics

- **Source**: `manifest.snapshot_watermark` (u64 transaction ID)
- **Filtering**: Records with `txn_id <= watermark` are skipped (already in snapshot)
- **No snapshot**: `watermark = None` → all records replayed

## 6. Partial Write Detection

**Location**: `crates/durability/src/wal/reader.rs:43-101`

Three-level detection during segment reading:

| Detection | Mechanism | Stop Reason |
|-----------|-----------|-------------|
| Insufficient data | Length field says more bytes than available | `PartialRecord` |
| CRC mismatch | Stored CRC ≠ computed CRC | `ChecksumMismatch` |
| Format error | Unknown format version or invalid payload | `ParseError` |

```
ReadStopReason:
  EndOfData           ← Normal: all records valid
  PartialRecord       ← Expected after crash: truncated write
  ChecksumMismatch    ← Corruption: bit flip in data
  ParseError          ← Codec/version mismatch (not corruption)
```

**Behavior**: On any non-EndOfData stop reason, reading halts. `valid_end` tracks the last byte of the last complete record. During recovery, the segment is truncated at `valid_end` (coordinator.rs:190-195).

**Critical design choice**: Only the last (active) segment is truncated. Earlier segments are assumed immutable and complete.

## 7. Crash Scenario Analysis

### Scenario 1: Crash during WAL append (before fsync)

```
State: Record written to OS buffer, not fsynced
```

**Strict mode**: Cannot happen — fsync is called inline before `append()` returns.

**Batched mode**: Record is in kernel buffer. On crash:
- Record may or may not reach disk (OS-dependent)
- If partially written: detected by CRC mismatch or InsufficientData during recovery
- Truncated during recovery (coordinator.rs:172-197)
- **Data loss**: Expected — this transaction was never acknowledged as durable

**None mode**: No WAL file exists. All data lost.

### Scenario 2: Crash after WAL fsync, before storage apply

```
State: Record on disk (fsynced), in-memory storage not updated
```

**Recovery**: WAL replay applies all records > watermark. Storage is rebuilt from WAL. **Zero data loss**.

**Code path** (manager.rs:235-255): If `apply_writes()` fails after WAL commit, the error is logged but `Ok(commit_version)` is returned — WAL is authoritative. Recovery will replay the transaction.

### Scenario 3: Crash during segment rotation

```
State: Old segment closed (fsynced), new segment being created
```

**Safety sequence** (writer.rs:219-237):
1. `segment.close()` → `file.sync_all()` — old segment is durable
2. `WalSegment::create(new_number)` → writes 32-byte header
3. `self.segment = Some(new_segment)` — old segment dropped

If crash between step 1 and 2: Old segment is complete. New segment doesn't exist. On restart, `WalWriter::new()` finds the old segment and opens it or creates a new one (writer.rs:96-116).

If crash during step 2 (partial header write): New segment has incomplete header. `open_append()` will fail on header validation. `WalWriter::new()` creates segment `num + 1` (writer.rs:103-108). The partial segment is harmless — it contains no records, and recovery reads only the old segment.

### Scenario 4: Crash during snapshot write

```
State: Writing to .snap-NNNNNN.tmp, MANIFEST unchanged
```

**Recovery**: MANIFEST doesn't reference any snapshot (or references the previous snapshot). Temp file is incomplete. `cleanup_temp_files()` removes it during checkpoint (checkpoint.rs:153). Recovery uses WAL-only or previous snapshot + WAL. **Zero data loss**.

### Scenario 5: Crash after snapshot fsync, before rename

```
State: .snap-NNNNNN.tmp is complete and fsynced, rename not done
```

**Recovery**: Same as scenario 4 — MANIFEST doesn't reference this snapshot. Temp file is abandoned. WAL replay rebuilds the same state. Slight inefficiency (wasted snapshot work) but **zero data loss**.

### Scenario 6: Crash after snapshot rename, before MANIFEST update

```
State: snap-NNNNNN.chk exists on disk, MANIFEST still points to old snapshot (or none)
```

**Recovery**: MANIFEST says snapshot = old/none. New snapshot file exists on disk but is unreferenced. Recovery uses old path (old snapshot + WAL or WAL-only). Orphaned snapshot file is harmless — just occupies disk space until garbage collection. **Zero data loss**, but slower recovery (misses optimization).

### Scenario 7: Crash during MANIFEST write

```
State: MANIFEST.tmp partially written, old MANIFEST intact
```

**MANIFEST uses atomic write-fsync-rename** (manifest.rs:211-236):
1. Write to `MANIFEST.tmp`
2. `file.sync_all()`
3. `rename(MANIFEST.tmp, MANIFEST)`
4. `parent_dir.sync_all()`

If crash before step 3: Old MANIFEST is still valid. Temp file is either incomplete or complete-but-unrenamed. On restart, `ManifestManager::load()` reads the old MANIFEST. **Zero data loss**.

### Scenario 8: Corrupted snapshot file

```
State: snap-NNNNNN.chk has bit flip, MANIFEST references it
```

**Recovery**: `SnapshotReader::load()` validates CRC32. CRC mismatch → `SnapshotReadError::CrcMismatch`. This propagates as `RecoveryError::Snapshot(...)` (coordinator.rs:140). **Recovery fails entirely** — no automatic fallback to WAL-only replay.

**Workaround**: Manually delete the corrupted snapshot, clear snapshot reference from MANIFEST, restart. Recovery falls back to full WAL replay.

### Scenario 9: Corrupted WAL segment (mid-segment)

```
State: WAL segment has bit flip in a record somewhere in the middle
```

**Recovery**: Records are read sequentially. CRC32 of corrupted record will fail. Reading stops at `ChecksumMismatch`. All records before the corruption are recovered. All records after (including valid ones) are **lost** — the reader doesn't skip past corrupted records.

### Scenario 10: Acknowledged data in Batched mode

```
Client calls commit → gets Ok(version) → crash before next fsync
```

**Strict mode**: Cannot happen — fsync completes before Ok returns.

**Batched mode**: `commit()` returns Ok(version) after WAL append but potentially before fsync. If crash occurs before the next sync threshold, this transaction is lost despite the client seeing success. The `flush()` call at manager.rs:223 forces a sync after append, so in practice the commit path always syncs. **This is safe** — the manager calls `wal.flush()` after `wal.append()`, which calls `segment.sync()`.

However, the `flush()` in the commit path is redundant with `maybe_sync()` for Strict mode, and for Batched mode it forces an immediate sync even when thresholds haven't been met. This means **Batched mode effectively behaves like Strict mode during commits** because `wal.flush()` always fsyncs.

## 8. Atomic Write Patterns

Three locations use the write-fsync-rename pattern:

| Location | Temp File | Final File | Dir Sync | Clean on Failure |
|----------|-----------|------------|----------|-----------------|
| Snapshot (disk_snapshot/writer.rs:76-132) | `.snap-NNNNNN.tmp` | `snap-NNNNNN.chk` | Yes (line 131-132) | Yes (cleanup_temp_files) |
| MANIFEST (manifest.rs:211-236) | `MANIFEST.tmp` | `MANIFEST` | Yes (line 229-233) | **No** |
| WAL Segment (wal_record.rs:134-158) | N/A (create_new) | `wal-NNNNNN.seg` | No | N/A |

**MANIFEST gap**: If `rename()` fails at manifest.rs:226, the temp file `MANIFEST.tmp` is not cleaned up. The next `persist()` call will overwrite it (via `truncate(true)` at line 218), so this is low severity. Snapshot writer does clean up temp files (writer.rs:147-163) but the manifest writer does not.

## 9. Recovery Participants

Beyond WAL replay, several subsystems have recovery-specific behavior:

### Event Log Recovery

Events are stored as KV entries with sequence-numbered keys. During recovery:
- KV entries are restored from snapshot/WAL
- Event sequence numbers are preserved from storage (not regenerated)
- Hash chain integrity is maintained

**Known issue**: If event metadata deserialization fails during engine recovery, `unwrap_or_else(|_| EventLogMeta::default())` silently resets the sequence counter to 0, causing the next event to overwrite sequence 0. (Existing issue #845)

### Vector State Recovery

Vectors are stored as KV entries. During recovery:
- KV entries are restored from snapshot/WAL
- In-memory vector backends (VectorHeap) are rebuilt from recovered KV entries
- `insert_with_id()` calls `upsert()`, which is idempotent — replaying the same record twice updates in-place

### Search Index Recovery

The search index is **in-memory only** and is **not recovered from WAL**. After recovery:
- Index is empty
- Rebuilt lazily on first search query or explicitly during startup
- This is by design — search is an optimization, not a correctness requirement

### Branch Recovery

Branch metadata is stored as KV entries. During recovery:
- All branch metadata is restored from KV
- `commit_locks` DashMap starts empty (lazily populated on first commit per branch)

## 10. Problems Found

### Problem 1: No CRC on WAL segment header

**Severity**: Medium

**Location**: `crates/durability/src/format/wal_record.rs:100-103`

```rust
pub fn is_valid(&self) -> bool {
    self.magic == SEGMENT_MAGIC
}
```

The segment header (32 bytes) is validated only by magic bytes (`"STRA"`) and format version. There is no CRC protecting the segment_number and database_uuid fields. A bit flip in the segment_number field could cause the segment to be misidentified — though the reader also validates segment_number against the expected value from the filename (wal_record.rs:184-192), which provides partial protection.

A bit flip in the database_uuid field would go completely undetected. If segments from different databases somehow end up in the same directory (unlikely but possible with manual file operations), the UUID check is the only guard, and it's not integrity-protected.

### Problem 2: Corrupted snapshot causes hard failure, no WAL fallback

**Severity**: Medium

**Location**: `crates/durability/src/recovery/coordinator.rs:138-147`

```rust
if let Some(snapshot_path) = &plan.snapshot_path {
    let snapshot_reader = SnapshotReader::new(clone_codec(self.codec.as_ref())?);
    let loaded = snapshot_reader.load(snapshot_path)?;  // ← hard failure on CRC mismatch
    on_snapshot(RecoverySnapshot { ... })?;
}
```

When MANIFEST references a snapshot with CRC corruption, `load()` returns a CRC error and recovery fails entirely. There is no automatic fallback to WAL-only replay.

The design philosophy appears intentional — "if MANIFEST says snapshot exists, it MUST be valid" — but this means a single bit flip in a snapshot file prevents database startup. The WAL contains all the information needed for full recovery; the snapshot is an optimization. A graceful degradation path (log warning, ignore snapshot, replay full WAL) would be more resilient.

Verified by test: `test_recover_corrupted_snapshot_crc_mismatch` (coordinator.rs:575-621) confirms recovery returns `Err(RecoveryError::Snapshot(_))`.

### Problem 3: Batched mode sync_if_overdue never called

**Severity**: Low (existing #887)

**Location**: `crates/durability/src/wal/writer.rs:256-272`

```rust
pub fn sync_if_overdue(&mut self) -> std::io::Result<bool> {
    if !self.has_unsynced_data {
        return Ok(false);
    }
    if let DurabilityMode::Standard { interval_ms, .. } = self.durability {
        if self.last_sync_time.elapsed().as_millis() as u64 >= interval_ms {
            // ... sync ...
        }
    }
    Ok(false)
}
```

This method exists to ensure Batched mode honors `interval_ms` even when no new writes arrive, but it is never called from any production code path. A periodic maintenance timer is needed. Without it, if writes stop arriving, buffered data can remain unfsynced indefinitely.

**However**, this is mitigated by the fact that `manager.rs:223` calls `wal.flush()` after every commit, which forces an immediate sync regardless of mode. So the `interval_ms` gap only matters for non-commit WAL operations (if any exist) or if the flush call is removed in the future.

### Problem 4: MANIFEST temp file not cleaned on rename failure

**Severity**: Low

**Location**: `crates/durability/src/format/manifest.rs:225-226`

```rust
// Atomic rename
std::fs::rename(&temp_path, &self.path)?;
```

If `rename()` fails, the function returns an error but does not clean up `MANIFEST.tmp`. The snapshot writer (disk_snapshot/writer.rs) has `cleanup_temp_files()` for this purpose, but the manifest writer has no equivalent. Next `persist()` call will overwrite the temp file via `truncate(true)`, so this is a minor issue.

### Problem 5: WAL recovery stops at first corrupted record, losing subsequent valid records

**Severity**: Medium

**Location**: `crates/durability/src/wal/reader.rs:83-86`

```rust
Err(WalRecordError::ChecksumMismatch { .. }) => {
    stop_reason = ReadStopReason::ChecksumMismatch { offset };
    break;
}
```

If a single record in the middle of a segment has a CRC mismatch (e.g., from a transient storage error), all subsequent valid records in that segment are lost. The reader does not attempt to scan forward past the corrupted record.

This is a design choice with tradeoffs:
- **Pro**: Simple, deterministic recovery. No risk of applying records out of order.
- **Con**: A single bit flip can cause loss of many valid, committed records.

For comparison, database systems like PostgreSQL and MySQL can skip corrupted WAL records and continue with subsequent valid records, recovering more data at the cost of complexity.

### Problem 6: Commit path always calls flush(), negating Batched mode benefits

**Severity**: Low (design observation)

**Location**: `crates/concurrency/src/manager.rs:223-228`

```rust
if let Err(e) = wal.flush() {
    txn.status = TransactionStatus::Aborted { reason: ... };
    return Err(CommitError::WALError(e.to_string()));
}
```

After `wal.append()`, the commit path unconditionally calls `wal.flush()`, which calls `segment.sync()` (an fsync). This means every commit triggers an fsync regardless of the durability mode. Batched mode's `interval_ms` and `batch_size` thresholds are effectively bypassed for commit operations.

This is **correct for durability** — the commit docstring at manager.rs:230 says "DURABILITY POINT: Transaction is now durable." But it means Batched mode offers no latency benefit over Strict mode for the commit path. The Batched mode thresholds only apply within `maybe_sync()` (writer.rs:185-198), which runs before `flush()`.

If Batched mode is intended to trade some durability for lower latency, the `flush()` call should be removed and the transaction should be considered durable only after the next batch sync. If Batched mode is intended as Strict-with-background-sync, the current behavior is correct but the mode's documentation is misleading.

## 11. Durability Guarantees Summary

| Scenario | Strict | Batched | None |
|----------|--------|---------|------|
| Crash before WAL append | Lost | Lost | N/A (no WAL) |
| Crash after append, before fsync | N/A (inline fsync) | Lost* | N/A |
| Crash after fsync | Recovered | Recovered | N/A |
| Crash during segment rotation | Recovered | Recovered | N/A |
| Crash during snapshot write | Recovered (WAL) | Recovered (WAL) | N/A |
| Crash during MANIFEST write | Recovered (old MANIFEST) | Recovered | N/A |
| Corrupted snapshot | **Fails** | **Fails** | N/A |
| Corrupted WAL mid-segment | Prefix recovered | Prefix recovered | N/A |

*Batched mode in practice always calls `flush()` after commit (manager.rs:223), so this scenario doesn't occur for commits.

## 12. Recovery Correctness Assessment

### Can recovery produce inconsistent state?

**No**, given the current design:

1. **WAL is authoritative**: All durable state is in the WAL. Storage is rebuilt from WAL on recovery.
2. **Single record per transaction**: Each committed transaction is a single WalRecord. There's no multi-record framing (BeginTxn/CommitTxn) to get out of sync.
3. **Watermark filtering is correct**: Records with `txn_id <= watermark` are skipped. Snapshot captures state at watermark. No double-application.
4. **Version preservation**: WAL records carry commit versions. Recovery applies them exactly, preserving MVCC ordering.
5. **Idempotent replay**: `put_with_version()` is idempotent — same key+version → same result regardless of how many times applied.

### Can acknowledged data be lost?

**No**, for Strict mode. The commit path fsyncs before returning Ok.

**Effectively no**, for Batched mode. Despite the mode name suggesting deferred fsync, the commit path calls `wal.flush()` which forces immediate fsync. A transaction that returns Ok has been fsynced.

**Yes**, for None mode. No WAL is written. All data is lost on crash.

## 13. Summary

| # | Finding | Severity | Type |
|---|---------|----------|------|
| 1 | No CRC on WAL segment header — corruption in header fields undetected | Medium | Missing integrity check |
| 2 | Corrupted snapshot causes hard failure, no WAL-only fallback | Medium | Missing graceful degradation |
| 3 | Batched mode sync_if_overdue never called (existing #887) | Low | Incomplete feature |
| 4 | MANIFEST temp file not cleaned on rename failure | Low | Minor resource leak |
| 5 | WAL recovery stops at first corrupted record, losing subsequent valid data | Medium | Design limitation |
| 6 | Commit path always calls flush(), negating Batched mode latency benefits | Low | Design observation |

**Overall**: The crash recovery system is well-designed and correct. The WAL + snapshot + MANIFEST architecture provides strong durability guarantees with clear crash safety at every step. The write-fsync-rename pattern is used consistently. The main gaps are: (a) no graceful degradation when a snapshot is corrupted, (b) no forward-scanning past corrupted WAL records, and (c) segment headers lack CRC protection. All three are defense-in-depth improvements rather than fundamental correctness issues.
