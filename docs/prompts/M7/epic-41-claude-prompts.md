# Epic 41: Crash Recovery - Implementation Prompts

**Epic Goal**: Implement crash recovery from snapshot + WAL

**GitHub Issue**: [#339](https://github.com/anibjoshi/in-mem/issues/339)
**Status**: Ready to begin (after Epic 40 complete)
**Dependencies**: Epic 40 (Snapshot Format), Epic 42 (WAL Enhancement)

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M7_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M7_ARCHITECTURE.md`
2. **Epic Spec**: `docs/milestones/M7/EPIC_41_CRASH_RECOVERY.md`
3. **Prompt Header**: `docs/prompts/M7/M7_PROMPT_HEADER.md` for the 5 architectural rules

---

## Epic 41 Overview

### Scope
- SnapshotReader with checksum validation
- Snapshot discovery (find latest valid, fallback to older)
- Recovery sequence: load snapshot + replay WAL
- Corrupt entry handling with configurable limits
- RecoveryResult and RecoveryOptions types

### Key Rules: Recovery Invariants (R1-R6)

| # | Invariant | Meaning |
|---|-----------|---------|
| R1 | Deterministic | Same WAL + Snapshot = Same state |
| R2 | Idempotent | Replaying recovery produces identical state |
| R3 | Prefix-consistent | No partial transactions visible after recovery |
| R4 | Never invents data | Only committed data appears |
| R5 | Never drops committed | All durable commits survive |
| R6 | May drop uncommitted | Depending on durability mode |

### Success Criteria
- [ ] SnapshotReader validates checksum before loading
- [ ] Discovery finds latest valid snapshot, falls back if corrupt
- [ ] Recovery: load snapshot + replay WAL from offset
- [ ] Corrupt WAL entries skipped with warning (up to limit)
- [ ] RecoveryResult reports transactions recovered, corrupt entries skipped
- [ ] All recovery invariants (R1-R6) satisfied

### Component Breakdown
- **Story #298 (GitHub #353)**: SnapshotReader with Validation - CRITICAL
- **Story #299 (GitHub #354)**: Snapshot Discovery (Find Latest Valid) - CRITICAL
- **Story #300 (GitHub #355)**: Recovery Sequence Implementation - CRITICAL
- **Story #301 (GitHub #356)**: WAL Replay from Offset - CRITICAL
- **Story #302 (GitHub #357)**: Corrupt Entry Handling - HIGH
- **Story #303 (GitHub #358)**: Fallback to Older Snapshot - HIGH
- **Story #304 (GitHub #359)**: RecoveryResult and RecoveryOptions - HIGH

---

## Dependency Graph

```
Story #353 (Reader) ──> Story #354 (Discovery) ──> Story #358 (Fallback)
                                    │
                                    v
                        Story #355 (Recovery Sequence)
                                    │
                                    v
Story #356 (WAL Replay) <───────────┤
         │                          │
         v                          v
Story #357 (Corrupt) ────> Story #359 (Result/Options)
```

---

## Story #353: SnapshotReader with Validation

**GitHub Issue**: [#353](https://github.com/anibjoshi/in-mem/issues/353)
**Estimated Time**: 2 hours
**Dependencies**: Epic 40 complete
**Blocks**: Story #354

### Start Story

```bash
gh issue view 353
./scripts/start-story.sh 41 353 snapshot-reader
```

### Implementation

Add to `crates/durability/src/snapshot.rs`:

```rust
/// Snapshot reader with validation
pub struct SnapshotReader;

impl SnapshotReader {
    /// Read and validate snapshot
    pub fn read(path: &Path) -> Result<SnapshotData, SnapshotError> {
        // Read entire file
        let data = std::fs::read(path)?;

        // Validate checksum first
        validate_checksum_from_bytes(&data)?;

        // Parse header
        let header = SnapshotHeader::from_bytes(&data)?;

        // Parse primitive sections (after header)
        let sections = Self::parse_sections(&data[38..])?;

        Ok(SnapshotData { header, sections })
    }

    /// Validate snapshot without fully loading
    pub fn validate(path: &Path) -> Result<SnapshotInfo, SnapshotError> {
        let data = std::fs::read(path)?;

        // Validate checksum
        validate_checksum_from_bytes(&data)?;

        // Parse header only
        let header = SnapshotHeader::from_bytes(&data)?;

        Ok(SnapshotInfo {
            path: path.to_path_buf(),
            timestamp_micros: header.timestamp_micros,
            wal_offset: header.wal_offset,
        })
    }

    fn parse_sections(data: &[u8]) -> Result<Vec<PrimitiveSection>, SnapshotError> {
        let mut sections = Vec::new();
        let mut offset = 0;

        // data starts after header, before CRC
        // Last 4 bytes are CRC, so stop before that
        let end = data.len().saturating_sub(4);

        if offset >= end {
            return Ok(sections);
        }

        // Read primitive count
        let count = data[offset] as usize;
        offset += 1;

        for _ in 0..count {
            if offset >= end {
                return Err(SnapshotError::TooShort);
            }

            // Read type
            let primitive_type = data[offset];
            offset += 1;

            // Read length
            if offset + 8 > end {
                return Err(SnapshotError::TooShort);
            }
            let len = u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap()) as usize;
            offset += 8;

            // Read data
            if offset + len > end {
                return Err(SnapshotError::TooShort);
            }
            let section_data = data[offset..offset + len].to_vec();
            offset += len;

            sections.push(PrimitiveSection {
                primitive_type,
                data: section_data,
            });
        }

        Ok(sections)
    }
}

/// Loaded snapshot data
#[derive(Debug)]
pub struct SnapshotData {
    pub header: SnapshotHeader,
    pub sections: Vec<PrimitiveSection>,
}
```

### Acceptance Criteria

- [ ] Validates checksum before parsing
- [ ] Returns error on checksum mismatch
- [ ] Parses header and sections correctly
- [ ] validate() allows quick validation without full load

### Complete Story

```bash
./scripts/complete-story.sh 353
```

---

## Story #354: Snapshot Discovery (Find Latest Valid)

**GitHub Issue**: [#354](https://github.com/anibjoshi/in-mem/issues/354)
**Estimated Time**: 2 hours
**Dependencies**: Story #353

### Start Story

```bash
gh issue view 354
./scripts/start-story.sh 41 354 snapshot-discovery
```

### Implementation

Create `crates/durability/src/recovery.rs`:

```rust
//! Crash recovery implementation

use crate::snapshot::*;
use crate::snapshot_types::*;
use std::path::{Path, PathBuf};

/// Snapshot discovery
pub struct SnapshotDiscovery;

impl SnapshotDiscovery {
    /// Find the latest valid snapshot in directory
    pub fn find_latest_valid(snapshot_dir: &Path) -> Result<Option<SnapshotInfo>, SnapshotError> {
        if !snapshot_dir.exists() {
            return Ok(None);
        }

        let mut snapshots = Self::list_snapshots(snapshot_dir)?;

        // Sort by name (contains timestamp) descending
        snapshots.sort_by(|a, b| b.cmp(a));

        // Try each snapshot from newest to oldest
        for path in snapshots {
            match SnapshotReader::validate(&path) {
                Ok(info) => {
                    tracing::info!("Using snapshot: {:?}", path);
                    return Ok(Some(info));
                }
                Err(e) => {
                    tracing::warn!("Snapshot {:?} invalid: {}, trying older...", path, e);
                    continue;
                }
            }
        }

        tracing::warn!("No valid snapshots found");
        Ok(None)
    }

    /// List all snapshot files in directory
    pub fn list_snapshots(dir: &Path) -> Result<Vec<PathBuf>, SnapshotError> {
        let mut snapshots = Vec::new();

        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().map_or(false, |ext| ext == "dat") {
                if path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map_or(false, |n| n.starts_with("snapshot_"))
                {
                    snapshots.push(path);
                }
            }
        }

        Ok(snapshots)
    }
}
```

### Acceptance Criteria

- [ ] Lists all snapshot files in directory
- [ ] Sorts by name (newest first)
- [ ] Validates each before use
- [ ] Returns None if no valid snapshots

### Complete Story

```bash
./scripts/complete-story.sh 354
```

---

## Story #355: Recovery Sequence Implementation

**GitHub Issue**: [#355](https://github.com/anibjoshi/in-mem/issues/355)
**Estimated Time**: 4 hours
**Dependencies**: Stories #354, #356

### Start Story

```bash
gh issue view 355
./scripts/start-story.sh 41 355 recovery-sequence
```

### Implementation

Add to `crates/durability/src/recovery.rs`:

```rust
/// Recovery engine
pub struct RecoveryEngine;

impl RecoveryEngine {
    /// Recover database from disk
    ///
    /// 1. Find latest valid snapshot
    /// 2. Load snapshot into memory
    /// 3. Replay WAL from snapshot offset
    /// 4. Rebuild indexes
    pub fn recover(
        data_dir: &Path,
        options: RecoveryOptions,
    ) -> Result<(Database, RecoveryResult), RecoveryError> {
        let start = std::time::Instant::now();
        let mut result = RecoveryResult::default();

        // 1. Find snapshot
        let snapshot_dir = data_dir.join("snapshots");
        let snapshot_info = SnapshotDiscovery::find_latest_valid(&snapshot_dir)?;

        // 2. Load snapshot or create empty DB
        let mut db = if let Some(ref info) = snapshot_info {
            result.snapshot_used = Some(info.clone());
            let data = SnapshotReader::read(&info.path)?;
            Self::load_from_snapshot(data)?
        } else {
            Database::empty()
        };

        // 3. Determine WAL replay start
        let replay_from = snapshot_info
            .as_ref()
            .map(|s| s.wal_offset)
            .unwrap_or(0);

        // 4. Replay WAL
        let wal_path = data_dir.join("wal.dat");
        if wal_path.exists() {
            let replay_result = Self::replay_wal(&mut db, &wal_path, replay_from, &options)?;
            result.wal_entries_replayed = replay_result.entries_replayed;
            result.transactions_recovered = replay_result.transactions_recovered;
            result.orphaned_transactions = replay_result.orphaned_transactions;
            result.corrupt_entries_skipped = replay_result.corrupt_entries;
        }

        // 5. Rebuild indexes
        if options.rebuild_indexes {
            db.rebuild_all_indexes()?;
        }

        result.recovery_time_micros = start.elapsed().as_micros() as u64;

        Ok((db, result))
    }

    fn load_from_snapshot(data: SnapshotData) -> Result<Database, RecoveryError> {
        let mut db = Database::empty();
        deserialize_primitives(
            &data.sections,
            &mut db.kv,
            &mut db.json,
            &mut db.event,
            &mut db.state,
            &mut db.trace,
            &mut db.run,
        )?;
        Ok(db)
    }
}
```

### Acceptance Criteria

- [ ] Finds and loads latest valid snapshot
- [ ] Creates empty DB if no snapshot
- [ ] Replays WAL from correct offset
- [ ] Rebuilds indexes if configured
- [ ] Returns complete RecoveryResult

### Complete Story

```bash
./scripts/complete-story.sh 355
```

---

## Story #356: WAL Replay from Offset

**GitHub Issue**: [#356](https://github.com/anibjoshi/in-mem/issues/356)
**Estimated Time**: 4 hours
**Dependencies**: Epic 42 complete

### Start Story

```bash
gh issue view 356
./scripts/start-story.sh 41 356 wal-replay
```

### Implementation

```rust
/// WAL replay result
#[derive(Default)]
struct WalReplayResult {
    entries_replayed: u64,
    transactions_recovered: u64,
    orphaned_transactions: u64,
    corrupt_entries: u64,
}

impl RecoveryEngine {
    /// Replay WAL entries from given offset
    ///
    /// CRITICAL: Only applies entries with commit markers.
    /// Entries without commit markers (orphaned transactions) are discarded.
    fn replay_wal(
        db: &mut Database,
        wal_path: &Path,
        from_offset: u64,
        options: &RecoveryOptions,
    ) -> Result<WalReplayResult, RecoveryError> {
        let mut result = WalReplayResult::default();
        let mut reader = WalReader::open(wal_path)?;
        reader.seek_to(from_offset)?;

        // Buffer entries by transaction
        let mut tx_entries: HashMap<TxId, Vec<WalEntry>> = HashMap::new();

        while let Some(entry_result) = reader.next_entry() {
            let entry = match entry_result {
                Ok(e) => e,
                Err(WalError::ChecksumMismatch) => {
                    result.corrupt_entries += 1;
                    if result.corrupt_entries > options.max_corrupt_entries as u64 {
                        return Err(RecoveryError::TooManyCorruptEntries(result.corrupt_entries));
                    }
                    tracing::warn!("Skipping corrupt WAL entry");
                    continue;
                }
                Err(e) => return Err(e.into()),
            };

            result.entries_replayed += 1;

            match entry.entry_type {
                WalEntryType::TransactionCommit => {
                    // Apply all buffered entries for this transaction
                    if let Some(tx_id) = entry.tx_id {
                        if let Some(entries) = tx_entries.remove(&tx_id) {
                            for e in entries {
                                db.apply_wal_entry(&e)?;
                            }
                            result.transactions_recovered += 1;
                        }
                    }
                }
                WalEntryType::TransactionAbort => {
                    // Discard buffered entries
                    if let Some(tx_id) = entry.tx_id {
                        tx_entries.remove(&tx_id);
                        result.orphaned_transactions += 1;
                    }
                }
                _ => {
                    // Buffer entry for transaction
                    if let Some(tx_id) = entry.tx_id {
                        tx_entries.entry(tx_id).or_default().push(entry);
                    }
                }
            }
        }

        // Orphaned transactions (in WAL but no commit marker)
        for (tx_id, _) in tx_entries {
            tracing::warn!("Orphaned transaction (no commit marker): {:?}", tx_id);
            result.orphaned_transactions += 1;
        }

        Ok(result)
    }
}
```

### Acceptance Criteria

- [ ] Replays from specified offset
- [ ] Groups entries by transaction ID
- [ ] Only applies entries with commit markers (R3: prefix-consistent)
- [ ] Tracks orphaned transactions
- [ ] Recovery is deterministic (R1)

### Complete Story

```bash
./scripts/complete-story.sh 356
```

---

## Story #357: Corrupt Entry Handling

**GitHub Issue**: [#357](https://github.com/anibjoshi/in-mem/issues/357)
**Estimated Time**: 2 hours
**Dependencies**: Story #356

### Start Story

```bash
gh issue view 357
./scripts/start-story.sh 41 357 corrupt-handling
```

### Implementation

Add error types:

```rust
/// Recovery errors
#[derive(Debug, thiserror::Error)]
pub enum RecoveryError {
    #[error("Too many corrupt entries: {0}")]
    TooManyCorruptEntries(u64),

    #[error("Snapshot error: {0}")]
    Snapshot(#[from] SnapshotError),

    #[error("WAL error: {0}")]
    Wal(#[from] WalError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Database error: {0}")]
    Database(String),
}
```

### Acceptance Criteria

- [ ] Skips corrupt entries (logs warning)
- [ ] Counts corrupt entries
- [ ] Fails if too many corrupt (configurable)
- [ ] Clear error messages

### Complete Story

```bash
./scripts/complete-story.sh 357
```

---

## Story #358: Fallback to Older Snapshot

**GitHub Issue**: [#358](https://github.com/anibjoshi/in-mem/issues/358)
**Estimated Time**: 2 hours
**Dependencies**: Story #354

### Start Story

```bash
gh issue view 358
./scripts/start-story.sh 41 358 snapshot-fallback
```

### Implementation

Enhance SnapshotDiscovery:

```rust
impl SnapshotDiscovery {
    /// Find latest valid snapshot with detailed logging
    pub fn find_latest_valid_with_fallback(
        snapshot_dir: &Path,
    ) -> Result<Option<(SnapshotInfo, usize)>, SnapshotError> {
        let mut snapshots = Self::list_snapshots(snapshot_dir)?;
        snapshots.sort_by(|a, b| b.cmp(a)); // Newest first

        let total = snapshots.len();

        for (idx, path) in snapshots.iter().enumerate() {
            match SnapshotReader::validate(path) {
                Ok(info) => {
                    if idx > 0 {
                        tracing::warn!(
                            "Using snapshot {} (skipped {} newer corrupt snapshots)",
                            path.display(),
                            idx
                        );
                    } else {
                        tracing::info!("Using latest snapshot: {}", path.display());
                    }
                    return Ok(Some((info, total - idx)));
                }
                Err(e) => {
                    tracing::warn!(
                        "Snapshot {} is corrupt: {}. Trying older...",
                        path.display(),
                        e
                    );
                }
            }
        }

        tracing::warn!("No valid snapshots found in {}", snapshot_dir.display());
        Ok(None)
    }
}
```

### Acceptance Criteria

- [ ] Tries snapshots newest to oldest
- [ ] Logs which is being used
- [ ] Logs skipped corrupt snapshots
- [ ] Falls back to full WAL replay if none valid

### Complete Story

```bash
./scripts/complete-story.sh 358
```

---

## Story #359: RecoveryResult and RecoveryOptions

**GitHub Issue**: [#359](https://github.com/anibjoshi/in-mem/issues/359)
**Estimated Time**: 2 hours
**Dependencies**: None

### Start Story

```bash
gh issue view 359
./scripts/start-story.sh 41 359 recovery-types
```

### Implementation

```rust
/// Recovery options
#[derive(Debug, Clone)]
pub struct RecoveryOptions {
    /// Maximum corrupt entries to tolerate before failing
    pub max_corrupt_entries: usize,
    /// Whether to verify all checksums (slower but safer)
    pub verify_all_checksums: bool,
    /// Whether to rebuild indexes after recovery
    pub rebuild_indexes: bool,
    /// Whether to log recovery progress
    pub verbose: bool,
}

impl Default for RecoveryOptions {
    fn default() -> Self {
        RecoveryOptions {
            max_corrupt_entries: 10,
            verify_all_checksums: true,
            rebuild_indexes: true,
            verbose: false,
        }
    }
}

impl RecoveryOptions {
    pub fn strict() -> Self {
        RecoveryOptions {
            max_corrupt_entries: 0,
            verify_all_checksums: true,
            rebuild_indexes: true,
            verbose: true,
        }
    }

    pub fn permissive() -> Self {
        RecoveryOptions {
            max_corrupt_entries: 100,
            verify_all_checksums: false,
            rebuild_indexes: true,
            verbose: false,
        }
    }
}

/// Recovery result
#[derive(Debug, Default)]
pub struct RecoveryResult {
    /// Snapshot used (if any)
    pub snapshot_used: Option<SnapshotInfo>,
    /// WAL entries replayed
    pub wal_entries_replayed: u64,
    /// Transactions successfully recovered
    pub transactions_recovered: u64,
    /// Orphaned transactions (no commit marker)
    pub orphaned_transactions: u64,
    /// Corrupt entries skipped
    pub corrupt_entries_skipped: u64,
    /// Total recovery time (microseconds)
    pub recovery_time_micros: u64,
}

impl RecoveryResult {
    pub fn summary(&self) -> String {
        format!(
            "Recovery: {} tx, {} WAL entries, {} orphaned, {} corrupt, {:.2}ms",
            self.transactions_recovered,
            self.wal_entries_replayed,
            self.orphaned_transactions,
            self.corrupt_entries_skipped,
            self.recovery_time_micros as f64 / 1000.0
        )
    }
}
```

### Acceptance Criteria

- [ ] RecoveryOptions has all configuration fields
- [ ] Default options are sensible
- [ ] strict() and permissive() presets available
- [ ] RecoveryResult captures all metrics
- [ ] summary() provides human-readable output

### Complete Story

```bash
./scripts/complete-story.sh 359
```

---

## Epic 41 Completion Checklist

### 1. Final Validation

```bash
~/.cargo/bin/cargo test -p in-mem-durability -- recovery
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### 2. Verify Recovery Invariants

```rust
#[test]
fn test_recovery_deterministic() {
    // R1: Same inputs = same outputs
    let (db1, _) = recover(&data_dir)?;
    let (db2, _) = recover(&data_dir)?;
    assert_eq!(db1.state(), db2.state());
}

#[test]
fn test_recovery_prefix_consistent() {
    // R3: No partial transactions visible
    // Transaction without commit marker should not appear
}

#[test]
fn test_recovery_never_drops_committed() {
    // R5: All durable commits survive
}
```

### 3. Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-41-crash-recovery -m "Epic 41: Crash Recovery complete

Delivered:
- SnapshotReader with validation
- Snapshot discovery with fallback
- Recovery sequence implementation
- WAL replay from offset
- Corrupt entry handling
- RecoveryResult and RecoveryOptions

All recovery invariants (R1-R6) verified.

Stories: #353, #354, #355, #356, #357, #358, #359
"
git push origin develop
gh issue close 339 --comment "Epic 41: Crash Recovery - COMPLETE"
```

---

## Summary

Epic 41 implements crash recovery that is deterministic, idempotent, and prefix-consistent. After recovery, the database corresponds to a prefix of the committed transaction history - no partial transactions visible.
