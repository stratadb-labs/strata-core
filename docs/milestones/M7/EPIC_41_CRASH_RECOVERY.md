# Epic 41: Crash Recovery

**Goal**: Implement crash recovery from snapshot + WAL

**Dependencies**: Epic 40 (Snapshot Format)

---

## Scope

- SnapshotReader with checksum validation
- Snapshot discovery (find latest valid, fallback to older)
- Recovery sequence: load snapshot + replay WAL
- Corrupt entry handling with configurable limits
- RecoveryResult and RecoveryOptions types

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #298 | SnapshotReader with Validation | CRITICAL |
| #299 | Snapshot Discovery (Find Latest Valid) | CRITICAL |
| #300 | Recovery Sequence Implementation | CRITICAL |
| #301 | WAL Replay from Offset | CRITICAL |
| #302 | Corrupt Entry Handling | HIGH |
| #303 | Fallback to Older Snapshot | HIGH |
| #304 | RecoveryResult and RecoveryOptions | HIGH |

---

## Story #298: SnapshotReader with Validation

**File**: `crates/durability/src/snapshot.rs`

**Deliverable**: SnapshotReader that validates and loads snapshots

### Implementation

```rust
pub struct SnapshotReader;

impl SnapshotReader {
    /// Read and validate snapshot
    pub fn read(path: &Path) -> Result<SnapshotData, SnapshotError> {
        // Read entire file
        let data = std::fs::read(path)?;

        // Validate checksum first
        Self::validate_checksum_from_bytes(&data)?;

        // Parse header
        let header = SnapshotHeader::from_bytes(&data)?;

        // Parse primitive sections
        let sections = Self::parse_sections(&data[38..])?;

        Ok(SnapshotData {
            header,
            sections,
        })
    }

    /// Validate snapshot without fully loading
    pub fn validate(path: &Path) -> Result<SnapshotInfo, SnapshotError> {
        let data = std::fs::read(path)?;

        // Validate checksum
        Self::validate_checksum_from_bytes(&data)?;

        // Parse header only
        let header = SnapshotHeader::from_bytes(&data)?;

        Ok(SnapshotInfo {
            path: path.to_path_buf(),
            timestamp_micros: header.timestamp_micros,
            wal_offset: header.wal_offset,
        })
    }

    fn validate_checksum_from_bytes(data: &[u8]) -> Result<(), SnapshotError> {
        if data.len() < 4 {
            return Err(SnapshotError::TooShort);
        }

        let (content, checksum_bytes) = data.split_at(data.len() - 4);
        let stored = u32::from_le_bytes(checksum_bytes.try_into().unwrap());

        let mut hasher = crc32fast::Hasher::new();
        hasher.update(content);
        let computed = hasher.finalize();

        if stored != computed {
            return Err(SnapshotError::ChecksumMismatch {
                expected: stored,
                actual: computed,
            });
        }

        Ok(())
    }

    fn parse_sections(data: &[u8]) -> Result<Vec<PrimitiveSection>, SnapshotError> {
        let mut sections = Vec::new();
        let mut offset = 0;

        // Read primitive count
        if offset >= data.len() {
            return Err(SnapshotError::TooShort);
        }
        let count = data[offset] as usize;
        offset += 1;

        for _ in 0..count {
            // Read type
            if offset >= data.len() {
                return Err(SnapshotError::TooShort);
            }
            let primitive_type = data[offset];
            offset += 1;

            // Read length
            if offset + 8 > data.len() {
                return Err(SnapshotError::TooShort);
            }
            let len = u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap()) as usize;
            offset += 8;

            // Read data
            if offset + len > data.len() {
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

---

## Story #299: Snapshot Discovery (Find Latest Valid)

**File**: `crates/durability/src/recovery.rs` (NEW)

**Deliverable**: Find latest valid snapshot with fallback

### Implementation

```rust
use std::path::{Path, PathBuf};

pub struct SnapshotDiscovery;

impl SnapshotDiscovery {
    /// Find the latest valid snapshot in directory
    ///
    /// Tries snapshots from newest to oldest until a valid one is found.
    /// Returns None if no valid snapshots exist.
    pub fn find_latest_valid(snapshot_dir: &Path) -> Result<Option<SnapshotInfo>, SnapshotError> {
        if !snapshot_dir.exists() {
            return Ok(None);
        }

        // List all snapshot files
        let mut snapshots = Self::list_snapshots(snapshot_dir)?;

        // Sort by timestamp descending (newest first)
        snapshots.sort_by(|a, b| b.cmp(a));

        // Try each snapshot
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
    fn list_snapshots(dir: &Path) -> Result<Vec<PathBuf>, SnapshotError> {
        let mut snapshots = Vec::new();

        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            // Check for snapshot file extension
            if path.extension().map_or(false, |ext| ext == "dat") {
                // Verify it starts with "snapshot_"
                if path.file_name()
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
- [ ] Sorts by timestamp (newest first)
- [ ] Validates each snapshot before use
- [ ] Falls back to older on corruption
- [ ] Returns None if no valid snapshots

---

## Story #300: Recovery Sequence Implementation

**File**: `crates/durability/src/recovery.rs`

**Deliverable**: Full recovery sequence

### Implementation

```rust
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

        for section in data.sections {
            match section.primitive_type {
                primitive_ids::KV => {
                    let entries: Vec<(Key, Value)> = bincode::deserialize(&section.data)?;
                    for (key, value) in entries {
                        db.kv.put_raw(key, value)?;
                    }
                }
                primitive_ids::JSON => {
                    let docs: Vec<(Key, JsonDoc)> = bincode::deserialize(&section.data)?;
                    for (key, doc) in docs {
                        db.json.set_raw(key, doc)?;
                    }
                }
                // Similar for other primitives...
                _ => {
                    tracing::warn!("Unknown primitive type: {}", section.primitive_type);
                }
            }
        }

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

---

## Story #301: WAL Replay from Offset

**File**: `crates/durability/src/recovery.rs`

**Deliverable**: WAL replay with transaction boundaries

### Implementation

```rust
struct WalReplayResult {
    entries_replayed: u64,
    transactions_recovered: u64,
    orphaned_transactions: u64,
    corrupt_entries: u64,
}

impl RecoveryEngine {
    /// Replay WAL entries from given offset
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
        let mut current_tx: Option<TxId> = None;

        while let Some(entry_result) = reader.next_entry() {
            let entry = match entry_result {
                Ok(e) => e,
                Err(WalError::ChecksumMismatch) => {
                    result.corrupt_entries += 1;
                    if result.corrupt_entries > options.max_corrupt_entries as u64 {
                        return Err(RecoveryError::TooManyCorruptEntries(
                            result.corrupt_entries,
                        ));
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
                        tx_entries
                            .entry(tx_id)
                            .or_insert_with(Vec::new)
                            .push(entry);
                    }
                }
            }
        }

        // Orphaned transactions (in WAL but no commit marker)
        for (tx_id, _entries) in tx_entries {
            tracing::warn!("Orphaned transaction: {:?}", tx_id);
            result.orphaned_transactions += 1;
        }

        Ok(result)
    }
}
```

### Acceptance Criteria

- [ ] Replays from specified offset
- [ ] Groups entries by transaction ID
- [ ] Only applies entries with commit markers
- [ ] Tracks orphaned transactions
- [ ] Handles corrupt entries gracefully

---

## Story #302: Corrupt Entry Handling

**File**: `crates/durability/src/recovery.rs`

**Deliverable**: Graceful handling of corrupt WAL entries

### Implementation

```rust
impl RecoveryEngine {
    /// Handle corrupt entry during replay
    fn handle_corrupt_entry(
        result: &mut WalReplayResult,
        options: &RecoveryOptions,
    ) -> Result<(), RecoveryError> {
        result.corrupt_entries += 1;

        tracing::warn!(
            "Corrupt WAL entry detected (total: {})",
            result.corrupt_entries
        );

        if result.corrupt_entries > options.max_corrupt_entries as u64 {
            return Err(RecoveryError::TooManyCorruptEntries(
                result.corrupt_entries,
            ));
        }

        Ok(())
    }
}

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

    #[error("Deserialization error: {0}")]
    Deserialize(#[from] bincode::Error),
}
```

### Acceptance Criteria

- [ ] Skips corrupt entries (logs warning)
- [ ] Counts corrupt entries
- [ ] Fails if too many corrupt (configurable limit)
- [ ] Clear error messages

---

## Story #303: Fallback to Older Snapshot

**File**: `crates/durability/src/recovery.rs`

**Deliverable**: Automatic fallback when newest snapshot is corrupt

### Implementation

```rust
impl SnapshotDiscovery {
    /// Find latest valid snapshot with fallback
    ///
    /// If newest snapshot is corrupt, tries older ones.
    /// Logs which snapshot is being used.
    pub fn find_latest_valid_with_fallback(
        snapshot_dir: &Path,
    ) -> Result<Option<(SnapshotInfo, usize)>, SnapshotError> {
        let mut snapshots = Self::list_snapshots(snapshot_dir)?;
        snapshots.sort_by(|a, b| b.cmp(a));  // Newest first

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

- [ ] Tries snapshots from newest to oldest
- [ ] Logs which snapshot is being used
- [ ] Logs skipped corrupt snapshots
- [ ] Falls back to full WAL replay if none valid

---

## Story #304: RecoveryResult and RecoveryOptions

**File**: `crates/durability/src/recovery.rs`

**Deliverable**: Configuration and result types for recovery

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
            "Recovery complete: {} transactions, {} WAL entries, {} orphaned, {} corrupt, {:.2}ms",
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
- [ ] RecoveryResult captures all recovery metrics
- [ ] summary() provides human-readable output

---

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_recovery_from_snapshot() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path();

        // Create DB and populate
        let db = create_test_db(data_dir);
        populate_test_data(&db);

        // Snapshot
        db.snapshot().unwrap();

        // Recover
        let (recovered, result) = RecoveryEngine::recover(
            data_dir,
            RecoveryOptions::default(),
        ).unwrap();

        assert!(result.snapshot_used.is_some());
        assert_data_matches(&db, &recovered);
    }

    #[test]
    fn test_recovery_with_wal_replay() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path();

        // Create DB and populate
        let db = create_test_db(data_dir);
        populate_test_data(&db);

        // Snapshot
        db.snapshot().unwrap();

        // Add more data (not in snapshot)
        db.kv.put(run_id, "new_key", "new_value").unwrap();

        // Recover
        let (recovered, result) = RecoveryEngine::recover(
            data_dir,
            RecoveryOptions::default(),
        ).unwrap();

        // Should have all data including post-snapshot
        assert_eq!(
            recovered.kv.get(run_id, "new_key").unwrap(),
            Some("new_value".to_string())
        );
        assert_eq!(result.wal_entries_replayed, 1);
    }

    #[test]
    fn test_corrupt_snapshot_fallback() {
        let temp_dir = TempDir::new().unwrap();
        let data_dir = temp_dir.path();
        let snapshot_dir = data_dir.join("snapshots");
        std::fs::create_dir_all(&snapshot_dir).unwrap();

        // Create two snapshots
        let db = create_test_db(data_dir);
        populate_test_data(&db);
        db.snapshot().unwrap();  // snapshot_1
        std::thread::sleep(std::time::Duration::from_millis(10));
        db.snapshot().unwrap();  // snapshot_2

        // Corrupt newest snapshot
        let snapshots = SnapshotDiscovery::list_snapshots(&snapshot_dir).unwrap();
        let newest = snapshots.iter().max().unwrap();
        let mut data = std::fs::read(newest).unwrap();
        data[50] ^= 0xFF;
        std::fs::write(newest, &data).unwrap();

        // Recovery should use older snapshot
        let (recovered, result) = RecoveryEngine::recover(
            data_dir,
            RecoveryOptions::default(),
        ).unwrap();

        // Should still have data (from older snapshot + WAL)
        assert!(result.snapshot_used.is_some());
    }
}
```

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/durability/src/recovery.rs` | CREATE - Recovery engine |
| `crates/durability/src/snapshot.rs` | MODIFY - Add reader, discovery |
| `crates/durability/src/lib.rs` | MODIFY - Export recovery module |
| `crates/engine/src/database.rs` | MODIFY - Add open() with recovery |
