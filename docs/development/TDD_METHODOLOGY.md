# Test-Driven Development Methodology for in-mem

## Overview

This project uses a **hybrid TDD approach** tailored to each layer of the architecture. The methodology balances rigorous testing with pragmatic development speed.

## Core Principle: Different Layers, Different Testing Strategies

Not all code benefits equally from strict TDD. We adapt our approach based on the component:

| Layer | Approach | Rationale |
|-------|----------|-----------|
| Core Types | Definition → Tests → Implementation | Types are simple but foundational |
| Storage | Pure TDD (Test-First) | Complex with many edge cases |
| WAL/Durability | Corruption Tests Early | Data loss bugs are unacceptable |
| Recovery | Property-Based Testing | Must work for ALL sequences |
| Engine | Integration Tests | Orchestrates all layers |
| Primitives | Facade Tests | Stateless wrappers over engine |

## Phase-by-Phase Strategy

### Phase 1: Core Types (Epic 1, Stories #6-11)

**Approach: Definition → Tests → Implementation**

#### Why This Order?
- Traits are contracts - must be correct from start
- Core types are foundation - everything depends on them
- Tests catch design issues early (serialization, ordering, validation)
- These are simple enough that TDD doesn't slow you down

#### Workflow

**Step 1: Define the interface/type**
```rust
// core/src/types.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RunId(Uuid);

impl RunId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(Uuid::from_bytes(bytes))
    }

    pub fn as_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }
}
```

**Step 2: Write tests**
```rust
// core/src/types.rs (in #[cfg(test)] mod tests)
#[test]
fn test_run_id_creation() {
    let id1 = RunId::new();
    let id2 = RunId::new();
    assert_ne!(id1, id2); // UUIDs should be unique
}

#[test]
fn test_run_id_serialization() {
    let id = RunId::new();
    let bytes = id.as_bytes();
    let restored = RunId::from_bytes(*bytes);
    assert_eq!(id, restored);
}

#[test]
fn test_run_id_display() {
    let id = RunId::new();
    let s = format!("{}", id);
    assert!(s.len() > 0);
}
```

**Step 3: Run tests (they should pass if implementation is complete)**
```bash
cargo test -p in-mem-core test_run_id
```

#### What to Test in Core Types

**RunId (Story #7):**
- ✅ Unique generation
- ✅ Serialization roundtrip
- ✅ Display formatting
- ✅ Hash consistency

**Namespace (Story #7):**
- ✅ Construction from parts
- ✅ String formatting (tenant/app/agent/run)
- ✅ Equality comparison

**Key (Story #8):**
- ✅ BTreeMap ordering (prefix scans depend on this!)
- ✅ Prefix matching
- ✅ Serialization roundtrip
- ✅ Type tag discrimination

**Value (Story #9):**
- ✅ All enum variants construct correctly
- ✅ Serialization for each variant
- ✅ Size estimation (for memory accounting)

**Error (Story #10):**
- ✅ Error conversion (from std::io::Error, etc.)
- ✅ Error message formatting
- ✅ Error kind discrimination

### Phase 2: Storage Layer (Epic 2, Stories #12-16)

**Approach: Pure TDD (Test-First)**

#### Why Pure TDD?
- Storage is complex with subtle bugs
- Version management has race conditions
- Indices must stay consistent with main store
- Tests prevent regressions as you add features

#### TDD Cycle (Red-Green-Refactor)

**Red: Write failing test**
```rust
// storage/src/unified.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_put_increments_version() {
        let store = UnifiedStore::new();
        let key = Key::new(/* ... */);
        let value = Value::String("test".to_string());

        let v1 = store.put(key.clone(), value.clone(), None).unwrap();
        let v2 = store.put(key.clone(), value.clone(), None).unwrap();

        assert_eq!(v2, v1 + 1); // Version should increment
    }
}
```

**Green: Minimal implementation to pass**
```rust
pub struct UnifiedStore {
    data: Arc<RwLock<BTreeMap<Key, VersionedValue>>>,
    global_version: AtomicU64,
}

impl UnifiedStore {
    pub fn put(&self, key: Key, value: Value, ttl: Option<Duration>) -> Result<u64> {
        let version = self.global_version.fetch_add(1, Ordering::SeqCst);
        let versioned = VersionedValue {
            value,
            version,
            timestamp: Timestamp::now(),
            ttl,
        };

        let mut data = self.data.write();
        data.insert(key, versioned);

        Ok(version)
    }
}
```

**Refactor: Clean up after tests pass**
```rust
// Extract version generation
fn next_version(&self) -> u64 {
    self.global_version.fetch_add(1, Ordering::SeqCst)
}

pub fn put(&self, key: Key, value: Value, ttl: Option<Duration>) -> Result<u64> {
    let version = self.next_version();
    // ... rest of implementation
}
```

#### Critical Storage Tests

**Story #12 (UnifiedStore):**

```rust
#[test]
fn test_get_nonexistent_returns_none() {
    let store = UnifiedStore::new();
    assert!(store.get(&Key::new(/* ... */)).unwrap().is_none());
}

#[test]
fn test_put_get_roundtrip() {
    let store = UnifiedStore::new();
    let key = Key::new(/* ... */);
    let value = Value::String("hello".to_string());

    store.put(key.clone(), value.clone(), None).unwrap();
    let retrieved = store.get(&key).unwrap().unwrap();

    assert_eq!(retrieved.value, value);
}

#[test]
fn test_delete_removes_key() {
    let store = UnifiedStore::new();
    let key = Key::new(/* ... */);

    store.put(key.clone(), Value::String("test".to_string()), None).unwrap();
    assert!(store.get(&key).unwrap().is_some());

    store.delete(&key).unwrap();
    assert!(store.get(&key).unwrap().is_none());
}

#[test]
fn test_concurrent_reads() {
    let store = Arc::new(UnifiedStore::new());
    let key = Key::new(/* ... */);

    store.put(key.clone(), Value::I64(42), None).unwrap();

    // Spawn 10 concurrent readers
    let handles: Vec<_> = (0..10).map(|_| {
        let store = Arc::clone(&store);
        let key = key.clone();
        thread::spawn(move || {
            store.get(&key).unwrap().unwrap().value
        })
    }).collect();

    for handle in handles {
        assert_eq!(handle.join().unwrap(), Value::I64(42));
    }
}

#[test]
fn test_version_monotonic() {
    let store = UnifiedStore::new();
    let key = Key::new(/* ... */);

    let mut versions = vec![];
    for i in 0..100 {
        let v = store.put(key.clone(), Value::I64(i), None).unwrap();
        versions.push(v);
    }

    // Versions should be strictly increasing
    for window in versions.windows(2) {
        assert!(window[1] > window[0]);
    }
}
```

**Story #13 (Secondary Indices):**

```rust
#[test]
fn test_run_index_maintained() {
    let store = UnifiedStore::new();
    let run_id = RunId::new();
    let key1 = Key::new(run_id, TypeTag::KV, b"key1");
    let key2 = Key::new(run_id, TypeTag::KV, b"key2");

    store.put(key1.clone(), Value::String("v1".to_string()), None).unwrap();
    store.put(key2.clone(), Value::String("v2".to_string()), None).unwrap();

    let keys = store.scan_by_run(run_id, u64::MAX).unwrap();
    assert_eq!(keys.len(), 2);
}

#[test]
fn test_type_index_maintained() {
    let store = UnifiedStore::new();
    let run_id = RunId::new();
    let key1 = Key::new(run_id, TypeTag::KV, b"key1");
    let key2 = Key::new(run_id, TypeTag::Event, b"event1");

    store.put(key1, Value::String("kv".to_string()), None).unwrap();
    store.put(key2, Value::String("event".to_string()), None).unwrap();

    let kv_keys = store.scan_by_type(TypeTag::KV, u64::MAX).unwrap();
    assert_eq!(kv_keys.len(), 1);
}
```

**Story #14 (TTL Index):**

```rust
#[test]
fn test_ttl_expiry() {
    let store = UnifiedStore::new();
    let key = Key::new(/* ... */);

    // Insert with 100ms TTL
    store.put(key.clone(), Value::I64(42), Some(Duration::from_millis(100))).unwrap();

    // Should exist immediately
    assert!(store.get(&key).unwrap().is_some());

    // Wait for expiry
    thread::sleep(Duration::from_millis(150));

    // Should be gone
    assert!(store.get(&key).unwrap().is_none());
}

#[test]
fn test_ttl_cleanup_transactional() {
    // This test ensures TTL cleanup doesn't race with normal operations
    let store = Arc::new(UnifiedStore::new());
    let key = Key::new(/* ... */);

    // Start TTL cleaner
    let cleaner = TTLCleaner::start(Arc::clone(&store));

    // Insert with short TTL
    store.put(key.clone(), Value::I64(1), Some(Duration::from_millis(50))).unwrap();

    // Repeatedly read while cleanup might be happening
    for _ in 0..100 {
        let _ = store.get(&key);
        thread::sleep(Duration::from_millis(1));
    }

    // No panics = success (cleanup is transactional)
    cleaner.stop();
}
```

### Phase 3: WAL + Durability (Epic 3, Stories #17-22)

**Approach: Corruption Tests Early**

#### Why Corruption Tests First?
- WAL bugs cause data loss - UNACCEPTABLE
- Corruption scenarios hard to think of after implementation
- Forces defensive recovery design from the start

#### Workflow: Corruption Tests BEFORE Recovery

**Step 1: Write corruption simulation tests (Story #22, but do it early!)**

```rust
// durability/tests/corruption_simulation.rs

#[test]
fn test_corrupted_entry_detected() {
    let wal = WAL::create("test.wal").unwrap();

    // Write valid entry
    wal.append(WALEntry::Write { /* ... */ }).unwrap();
    wal.sync().unwrap();

    // Corrupt the file (flip random bits)
    corrupt_file("test.wal", 0.01); // 1% of bits flipped

    // Attempt to read - should detect corruption
    let result = WAL::open("test.wal");
    assert!(matches!(result, Err(Error::Corruption(_))));
}

#[test]
fn test_partial_write_detected() {
    let wal = WAL::create("test.wal").unwrap();

    // Simulate crash mid-write (truncate file)
    wal.append(WALEntry::Write { /* ... */ }).unwrap();
    truncate_file("test.wal", -10); // Remove last 10 bytes

    // Should detect incomplete entry
    let entries = WAL::open("test.wal").unwrap().read_all().unwrap();
    assert_eq!(entries.len(), 0); // Incomplete entry discarded
}

#[test]
fn test_zeroed_blocks_handled() {
    let wal = WAL::create("test.wal").unwrap();

    wal.append(WALEntry::Write { /* ... */ }).unwrap();
    wal.sync().unwrap();

    // Zero out middle of file (simulates disk failure)
    zero_bytes("test.wal", 100, 200);

    // Should stop at corruption, not crash
    let result = WAL::open("test.wal").unwrap().read_all();
    assert!(result.is_err());
}

fn corrupt_file(path: &str, corruption_rate: f64) {
    let mut data = fs::read(path).unwrap();
    let mut rng = rand::thread_rng();

    for byte in &mut data {
        if rng.gen::<f64>() < corruption_rate {
            *byte ^= 1 << rng.gen_range(0..8); // Flip random bit
        }
    }

    fs::write(path, data).unwrap();
}
```

**Step 2: Write encoding tests with CRC (Story #18)**

```rust
// durability/src/encoding.rs

#[test]
fn test_wal_entry_encoding_roundtrip() {
    let entry = WALEntry::Write {
        run_id: RunId::new(),
        key: Key::new(/* ... */),
        value: Value::String("test".to_string()),
        version: 1,
    };

    let encoded = encode_entry(&entry).unwrap();
    let decoded = decode_entry(&encoded).unwrap();

    assert_eq!(entry, decoded);
}

#[test]
fn test_crc_validation() {
    let entry = WALEntry::Write { /* ... */ };
    let mut encoded = encode_entry(&entry).unwrap();

    // Flip a bit in the payload
    encoded[10] ^= 0b00000001;

    // Decoding should fail due to CRC mismatch
    let result = decode_entry(&encoded);
    assert!(matches!(result, Err(Error::CRCMismatch)));
}

#[test]
fn test_all_entry_types_encode() {
    let entries = vec![
        WALEntry::BeginTxn { txn_id: 1, run_id: RunId::new(), timestamp: Timestamp::now() },
        WALEntry::Write { /* ... */ },
        WALEntry::Delete { /* ... */ },
        WALEntry::CommitTxn { txn_id: 1, run_id: RunId::new() },
        WALEntry::AbortTxn { txn_id: 1, run_id: RunId::new() },
        WALEntry::Checkpoint { /* ... */ },
    ];

    for entry in entries {
        let encoded = encode_entry(&entry).unwrap();
        let decoded = decode_entry(&encoded).unwrap();
        assert_eq!(entry, decoded);
    }
}
```

**Step 3: Implement WAL operations with defensive coding (Stories #19-20)**

```rust
// durability/src/wal.rs

impl WAL {
    pub fn append(&mut self, entry: WALEntry) -> Result<()> {
        // Encode with CRC
        let encoded = encode_entry(&entry)?;

        // Write length prefix
        let len = encoded.len() as u32;
        self.file.write_all(&len.to_le_bytes())?;

        // Write payload + CRC
        self.file.write_all(&encoded)?;

        // Sync based on durability mode
        match self.durability_mode {
            DurabilityMode::Strict => {
                self.file.sync_data()?; // fsync immediately
            }
            DurabilityMode::Batched => {
                self.pending_commits += 1;
                if self.pending_commits >= 1000 || self.last_sync.elapsed() > Duration::from_millis(100) {
                    self.file.sync_data()?;
                    self.pending_commits = 0;
                    self.last_sync = Instant::now();
                }
            }
            DurabilityMode::Async => {
                // No sync, background thread handles it
            }
        }

        Ok(())
    }

    pub fn read_all(&self) -> Result<Vec<WALEntry>> {
        let mut entries = vec![];
        let mut reader = BufReader::new(File::open(&self.path)?);

        loop {
            // Read length
            let mut len_bytes = [0u8; 4];
            match reader.read_exact(&mut len_bytes) {
                Ok(_) => {},
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            }

            let len = u32::from_le_bytes(len_bytes) as usize;

            // Sanity check
            if len > 100 * 1024 * 1024 { // 100MB max entry size
                return Err(Error::Corruption("Entry size too large".to_string()));
            }

            // Read payload
            let mut payload = vec![0u8; len];
            reader.read_exact(&mut payload)?;

            // Decode and validate CRC
            match decode_entry(&payload) {
                Ok(entry) => entries.push(entry),
                Err(Error::CRCMismatch) => {
                    // Corruption detected - STOP (don't skip!)
                    return Err(Error::Corruption("CRC validation failed".to_string()));
                }
                Err(e) => return Err(e),
            }
        }

        Ok(entries)
    }
}
```

#### Critical WAL Tests

**Story #19 (File Operations):**

```rust
#[test]
fn test_wal_append_read_roundtrip() {
    let mut wal = WAL::create("test.wal").unwrap();
    let entry = WALEntry::Write { /* ... */ };

    wal.append(entry.clone()).unwrap();
    wal.sync().unwrap();

    let wal2 = WAL::open("test.wal").unwrap();
    let entries = wal2.read_all().unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0], entry);
}

#[test]
fn test_wal_multiple_entries() {
    let mut wal = WAL::create("test.wal").unwrap();

    for i in 0..1000 {
        wal.append(WALEntry::Write {
            run_id: RunId::new(),
            key: Key::new(/* ... */),
            value: Value::I64(i),
            version: i as u64,
        }).unwrap();
    }

    wal.sync().unwrap();

    let wal2 = WAL::open("test.wal").unwrap();
    let entries = wal2.read_all().unwrap();
    assert_eq!(entries.len(), 1000);
}
```

**Story #20 (Durability Modes):**

```rust
#[test]
fn test_strict_mode_survives_crash() {
    let mut wal = WAL::create_with_mode("test.wal", DurabilityMode::Strict).unwrap();

    wal.append(WALEntry::Write { /* ... */ }).unwrap();
    // Strict mode syncs immediately, so even if we crash here...

    // Simulate crash (drop WAL without graceful shutdown)
    drop(wal);

    // Should recover the write
    let wal2 = WAL::open("test.wal").unwrap();
    let entries = wal2.read_all().unwrap();
    assert_eq!(entries.len(), 1);
}

#[test]
fn test_batched_mode_loses_recent_writes() {
    let mut wal = WAL::create_with_mode("test.wal", DurabilityMode::Batched).unwrap();

    wal.append(WALEntry::Write { /* ... */ }).unwrap();
    // Batched mode hasn't synced yet

    // Simulate crash
    simulate_power_loss(); // Forcefully terminate without flushing OS buffers

    // May or may not recover the write (acceptable for batched mode)
    let wal2 = WAL::open("test.wal").unwrap();
    let entries = wal2.read_all().unwrap();
    assert!(entries.len() <= 1); // 0 or 1, both valid
}
```

### Phase 4: Recovery (Epic 4, Stories #23-27)

**Approach: Property-Based Testing**

#### Why Property-Based?
- Recovery must work for ALL transaction sequences
- Example-based tests miss edge cases
- QuickCheck/proptest generates thousands of scenarios

#### Property-Based Test Examples

**Story #26 (Crash Simulation):**

```rust
// durability/tests/crash_simulation.rs

use proptest::prelude::*;

proptest! {
    #[test]
    fn test_recovery_preserves_committed_transactions(
        ops in prop::collection::vec(operation_strategy(), 10..100)
    ) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("db");

        // Execute operations and track committed state
        let committed_state = {
            let db = Database::open(&db_path).unwrap();
            let mut committed = HashMap::new();

            for op in &ops {
                match op {
                    Op::Put(key, value) => {
                        db.put(key.clone(), value.clone()).unwrap();
                        committed.insert(key.clone(), value.clone());
                    }
                    Op::Delete(key) => {
                        db.delete(key).unwrap();
                        committed.remove(key);
                    }
                    Op::Crash => {
                        drop(db);
                        simulate_crash(&db_path);
                        break; // Stop at crash point
                    }
                }
            }

            committed
        };

        // Recover and verify state matches committed
        let db = Database::open(&db_path).unwrap();
        for (key, expected_value) in committed_state {
            let actual = db.get(&key).unwrap();
            assert_eq!(actual, Some(expected_value));
        }
    }
}

fn operation_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        (any::<String>(), any::<String>()).prop_map(|(k, v)| Op::Put(k, v)),
        any::<String>().prop_map(Op::Delete),
        Just(Op::Crash),
    ]
}

#[derive(Debug, Clone)]
enum Op {
    Put(String, String),
    Delete(String),
    Crash,
}
```

**Story #23 (WAL Replay):**

```rust
#[test]
fn test_replay_committed_transaction() {
    let store = UnifiedStore::new();
    let wal_entries = vec![
        WALEntry::BeginTxn { txn_id: 1, run_id: RunId::new(), timestamp: Timestamp::now() },
        WALEntry::Write { run_id: RunId::new(), key: Key::new(/* ... */), value: Value::I64(42), version: 1 },
        WALEntry::CommitTxn { txn_id: 1, run_id: RunId::new() },
    ];

    replay_wal(&store, &wal_entries).unwrap();

    let value = store.get(&Key::new(/* ... */)).unwrap().unwrap();
    assert_eq!(value.value, Value::I64(42));
}

#[test]
fn test_discard_incomplete_transaction() {
    let store = UnifiedStore::new();
    let wal_entries = vec![
        WALEntry::BeginTxn { txn_id: 1, run_id: RunId::new(), timestamp: Timestamp::now() },
        WALEntry::Write { run_id: RunId::new(), key: Key::new(/* ... */), value: Value::I64(42), version: 1 },
        // No CommitTxn - transaction incomplete!
    ];

    replay_wal(&store, &wal_entries).unwrap();

    // Write should be discarded
    assert!(store.get(&Key::new(/* ... */)).unwrap().is_none());
}

#[test]
fn test_replay_multiple_transactions() {
    let store = UnifiedStore::new();
    let wal_entries = vec![
        // Transaction 1 (committed)
        WALEntry::BeginTxn { txn_id: 1, run_id: RunId::new(), timestamp: Timestamp::now() },
        WALEntry::Write { run_id: RunId::new(), key: Key::new(/* k1 */), value: Value::I64(1), version: 1 },
        WALEntry::CommitTxn { txn_id: 1, run_id: RunId::new() },

        // Transaction 2 (incomplete)
        WALEntry::BeginTxn { txn_id: 2, run_id: RunId::new(), timestamp: Timestamp::now() },
        WALEntry::Write { run_id: RunId::new(), key: Key::new(/* k2 */), value: Value::I64(2), version: 2 },
        // No commit

        // Transaction 3 (committed)
        WALEntry::BeginTxn { txn_id: 3, run_id: RunId::new(), timestamp: Timestamp::now() },
        WALEntry::Write { run_id: RunId::new(), key: Key::new(/* k3 */), value: Value::I64(3), version: 3 },
        WALEntry::CommitTxn { txn_id: 3, run_id: RunId::new() },
    ];

    replay_wal(&store, &wal_entries).unwrap();

    assert!(store.get(&Key::new(/* k1 */)).unwrap().is_some()); // Committed
    assert!(store.get(&Key::new(/* k2 */)).unwrap().is_none());  // Incomplete
    assert!(store.get(&Key::new(/* k3 */)).unwrap().is_some()); // Committed
}
```

### Phase 5: Database Engine (Epic 5, Stories #28-32)

**Approach: Integration Tests**

#### Why Integration Tests?
- Engine orchestrates all layers - unit tests insufficient
- Need end-to-end validation (write → restart → read)
- Tests serve as usage examples

#### Integration Test Examples

**Story #32 (End-to-End Integration):**

```rust
// tests/integration_test.rs

#[test]
fn test_write_restart_read() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("db");

    // Phase 1: Write data
    {
        let db = Database::open(&db_path).unwrap();
        let run_id = RunId::new();

        db.begin_run(run_id, RunMetadata::default()).unwrap();
        db.kv().put(run_id, "key1", "value1").unwrap();
        db.kv().put(run_id, "key2", "value2").unwrap();
        db.end_run(run_id).unwrap();

        // Graceful shutdown
    }

    // Phase 2: Restart and verify
    {
        let db = Database::open(&db_path).unwrap();
        let run_id = RunId::new();
        db.begin_run(run_id, RunMetadata::default()).unwrap();

        assert_eq!(db.kv().get(run_id, "key1").unwrap(), Some("value1".to_string()));
        assert_eq!(db.kv().get(run_id, "key2").unwrap(), Some("value2".to_string()));
    }
}

#[test]
fn test_crash_recovery() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("db");

    // Phase 1: Write and crash
    {
        let db = Database::open(&db_path).unwrap();
        let run_id = RunId::new();

        db.begin_run(run_id, RunMetadata::default()).unwrap();
        db.kv().put(run_id, "key1", "value1").unwrap();

        // Simulate crash (drop without graceful shutdown)
        std::mem::forget(db);
    }

    // Phase 2: Recover
    {
        let db = Database::open(&db_path).unwrap();
        let run_id = RunId::new();
        db.begin_run(run_id, RunMetadata::default()).unwrap();

        // Data should be recovered
        assert_eq!(db.kv().get(run_id, "key1").unwrap(), Some("value1".to_string()));
    }
}

#[test]
fn test_run_lifecycle() {
    let db = Database::open_in_memory().unwrap();
    let run_id = RunId::new();

    // Begin run
    db.begin_run(run_id, RunMetadata {
        description: "test run".to_string(),
        ..Default::default()
    }).unwrap();

    // Write some data
    db.kv().put(run_id, "status", "running").unwrap();

    // Query run metadata
    let metadata = db.run_index().get_run(run_id).unwrap().unwrap();
    assert_eq!(metadata.status, RunStatus::Running);

    // End run
    db.end_run(run_id).unwrap();

    // Verify status updated
    let metadata = db.run_index().get_run(run_id).unwrap().unwrap();
    assert_eq!(metadata.status, RunStatus::Completed);
}
```

### Phase 6: Primitives (M3, After M1)

**Approach: Facade Tests**

#### Why Facade Tests?
- Primitives are stateless wrappers
- They should delegate to engine
- Tests verify API contracts, not re-test engine

```rust
// primitives/src/kv.rs

#[cfg(test)]
mod tests {
    #[test]
    fn test_kv_put_get() {
        let db = Database::open_in_memory().unwrap();
        let kv = KVStore::new(&db);
        let run_id = RunId::new();

        db.begin_run(run_id, RunMetadata::default()).unwrap();

        kv.put(run_id, "key", "value").unwrap();
        assert_eq!(kv.get(run_id, "key").unwrap(), Some("value".to_string()));
    }

    #[test]
    fn test_kv_delete() {
        let db = Database::open_in_memory().unwrap();
        let kv = KVStore::new(&db);
        let run_id = RunId::new();

        db.begin_run(run_id, RunMetadata::default()).unwrap();

        kv.put(run_id, "key", "value").unwrap();
        kv.delete(run_id, "key").unwrap();

        assert!(kv.get(run_id, "key").unwrap().is_none());
    }

    #[test]
    fn test_kv_list() {
        let db = Database::open_in_memory().unwrap();
        let kv = KVStore::new(&db);
        let run_id = RunId::new();

        db.begin_run(run_id, RunMetadata::default()).unwrap();

        kv.put(run_id, "key1", "value1").unwrap();
        kv.put(run_id, "key2", "value2").unwrap();

        let entries = kv.list(run_id, "key").unwrap();
        assert_eq!(entries.len(), 2);
    }
}
```

## Testing Best Practices

### 1. Test Naming Convention

```rust
#[test]
fn test_{component}_{behavior}_{expected_outcome}()

// Good examples:
fn test_storage_put_increments_version()
fn test_wal_corrupted_entry_detected()
fn test_recovery_discards_incomplete_transactions()

// Bad examples:
fn test1()
fn test_storage()
fn it_works()
```

### 2. Arrange-Act-Assert Pattern

```rust
#[test]
fn test_storage_delete_removes_key() {
    // Arrange - set up test data
    let store = UnifiedStore::new();
    let key = Key::new(/* ... */);
    store.put(key.clone(), Value::I64(42), None).unwrap();

    // Act - perform the operation
    store.delete(&key).unwrap();

    // Assert - verify the outcome
    assert!(store.get(&key).unwrap().is_none());
}
```

### 3. Test One Thing Per Test

```rust
// Bad - testing multiple concerns
#[test]
fn test_storage_operations() {
    let store = UnifiedStore::new();
    store.put(/* ... */).unwrap();  // Testing put
    store.get(/* ... */).unwrap();  // Testing get
    store.delete(/* ... */).unwrap(); // Testing delete
    // Which one failed?
}

// Good - separate tests
#[test]
fn test_storage_put_succeeds() { /* ... */ }

#[test]
fn test_storage_get_returns_value() { /* ... */ }

#[test]
fn test_storage_delete_removes_key() { /* ... */ }
```

### 4. Use Descriptive Assertions

```rust
// Bad
assert!(value.is_some());

// Good
assert!(value.is_some(), "Expected value to exist for key {:?}", key);

// Better
assert_eq!(value.unwrap().version, 42,
    "Version mismatch: expected 42, got {}. This suggests version counter reset.",
    value.unwrap().version);
```

### 5. Test Edge Cases

```rust
#[test]
fn test_storage_empty_key() { /* ... */ }

#[test]
fn test_storage_large_value() { /* ... */ }

#[test]
fn test_storage_max_version_overflow() { /* ... */ }

#[test]
fn test_wal_zero_length_entry() { /* ... */ }

#[test]
fn test_wal_max_size_entry() { /* ... */ }
```

## Test Coverage Goals

| Component | Unit Test Coverage | Integration Tests | Special Tests |
|-----------|-------------------|-------------------|---------------|
| Core Types | 100% | N/A | Serialization roundtrip |
| Storage | 95%+ | Via Engine | Concurrent access |
| WAL | 95%+ | Via Recovery | Corruption simulation |
| Recovery | 90%+ | End-to-end | Crash simulation, property-based |
| Engine | 80%+ | End-to-end | Multi-run scenarios |
| Primitives | 80%+ | Via Engine | API contract tests |

**Overall M1 Goal: >90% test coverage**

## Running Tests

### Run all tests
```bash
cargo test --all
```

### Run specific crate tests
```bash
cargo test -p in-mem-core
cargo test -p in-mem-storage
cargo test -p in-mem-durability
```

### Run specific test
```bash
cargo test test_storage_put_increments_version
```

### Run tests with output
```bash
cargo test -- --nocapture
```

### Run tests in parallel
```bash
cargo test -- --test-threads=4
```

### Run ignored tests (long-running)
```bash
cargo test -- --ignored
```

### Generate coverage report
```bash
cargo install cargo-tarpaulin
cargo tarpaulin --all --out Html
open tarpaulin-report.html
```

## When to Write Tests

### BEFORE Implementation (Pure TDD)
- ✅ Storage operations (Epic 2)
- ✅ Secondary indices (Epic 2)
- ✅ WAL encoding/decoding (Epic 3)
- ✅ Recovery logic (Epic 4)

### ALONGSIDE Implementation
- ✅ Core types (Epic 1)
- ✅ Database engine (Epic 5)
- ✅ Primitives (M3)

### EARLY in Phase (Before Other Code)
- ✅ Corruption simulation tests (Epic 3)
- ✅ Crash simulation tests (Epic 4)

## Summary

**TDD Approach by Layer:**

1. **Core Types**: Define → Test → Implement
2. **Storage**: Pure TDD (Red-Green-Refactor)
3. **WAL**: Corruption tests EARLY, then TDD
4. **Recovery**: Property-based + crash simulation
5. **Engine**: Integration tests define behavior
6. **Primitives**: Facade tests verify contracts

**Key Principles:**
- ✅ Tests are specifications, not afterthoughts
- ✅ Corruption/crash tests force defensive design
- ✅ Property-based tests catch edge cases
- ✅ Integration tests prove it works end-to-end
- ✅ >90% coverage goal for M1

This methodology ensures correctness while maintaining development velocity.
