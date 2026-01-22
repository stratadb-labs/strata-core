# M2 Architecture Specification: Transactions

**Version**: 1.0
**Status**: Planning Phase
**Last Updated**: 2026-01-11

---

## Executive Summary

This document specifies the architecture for **Milestone 2 (M2): Transactions** of the in-memory agent database. M2 adds Optimistic Concurrency Control (OCC) with snapshot isolation, enabling concurrent multi-operation transactions with proper isolation guarantees.

**M2 Goals**:
- Implement OCC for non-blocking concurrent access
- Provide snapshot isolation for transactions
- Enable multi-operation atomic transactions
- Support compare-and-swap (CAS) operations
- Maintain M1's durability guarantees while improving concurrency

**Built on M1**:
- M1 provides: Storage, WAL, Recovery, Run lifecycle
- M2 adds: Transaction layer, OCC validation, Snapshot management
- M1's single-operation transactions replaced with explicit multi-operation transactions

**Non-Goals for M2**:
- Remaining primitives (Event Log, State Machine, Trace, Run Index) - M3
- Snapshots and WAL rotation - M4
- Deterministic replay - M5
- Vector store - M6
- Network layer - M7

---

## Table of Contents

1. [System Overview](#system-overview)
2. [Architecture Principles](#architecture-principles)
3. [Component Architecture](#component-architecture)
4. [OCC Transaction Model](#occ-transaction-model)
5. [Snapshot Isolation](#snapshot-isolation)
6. [Conflict Detection](#conflict-detection)
7. [Transaction Lifecycle](#transaction-lifecycle)
8. [API Design](#api-design)
9. [Performance Characteristics](#performance-characteristics)
10. [Testing Strategy](#testing-strategy)
11. [Migration from M1](#migration-from-m1)
12. [Known Limitations](#known-limitations)
13. [Future Extension Points](#future-extension-points)

---

## 1. System Overview

### 1.1 M2 Architecture Stack

```
┌─────────────────────────────────────────────────────────┐
│                    Application                          │
└────────────────────────┬────────────────────────────────┘
                         │
┌────────────────────────▼────────────────────────────────┐
│                  Primitives Layer                       │
│  ┌──────────┐  (M1: KVStore only)                      │
│  │ KVStore  │  (M3: EventLog, StateMachine, etc.)      │
│  └──────────┘                                           │
└────────────────────────┬────────────────────────────────┘
                         │
┌────────────────────────▼────────────────────────────────┐
│                   Engine Layer                          │
│  ┌──────────────────────────────────────────────┐      │
│  │  Database                                     │      │
│  │  - begin_transaction() ← NEW                 │      │
│  │  - Run Lifecycle (M1)                        │      │
│  │  - Transaction Coordination ← NEW            │      │
│  └──────────────────────────────────────────────┘      │
└────┬────────────────────────────────┬────────┬──────────┘
     │                                │        │
     ▼                                ▼        ▼
┌─────────────────┐      ┌─────────────────────────────────┐
│  Storage (M1)   │      │  Concurrency Layer ← NEW       │
│  - UnifiedStore │      │                                 │
│  - Indices      │      │  ┌──────────────────────────┐  │
│  - TTL          │      │  │ TransactionContext       │  │
└─────────────────┘      │  │ - read_set               │  │
                         │  │ - write_set              │  │
┌─────────────────┐      │  │ - cas_set                │  │
│ Durability (M1) │      │  └──────────────────────────┘  │
│  - WAL          │      │                                 │
│  - Recovery     │      │  ┌──────────────────────────┐  │
└─────────────────┘      │  │ Snapshot Management      │  │
                         │  │ - ClonedSnapshotView     │  │
                         │  │ - Version tracking       │  │
                         │  └──────────────────────────┘  │
                         │                                 │
                         │  ┌──────────────────────────┐  │
                         │  │ Conflict Detection       │  │
                         │  │ - Read validation        │  │
                         │  │ - Write conflict check   │  │
                         │  │ - CAS validation         │  │
                         │  └──────────────────────────┘  │
                         └─────────────────────────────────┘
```

### 1.2 What's New in M2

| Component | M1 Behavior | M2 Behavior |
|-----------|-------------|-------------|
| **Transactions** | Implicit single-operation | Explicit multi-operation with begin/commit |
| **Concurrency** | RwLock (blocking) | OCC (optimistic, non-blocking reads) |
| **Isolation** | None (each op atomic) | Snapshot isolation (read consistent view) |
| **Conflict Handling** | N/A (no conflicts) | Detect at commit, retry on conflict |
| **API** | `db.put()`, `db.get()` | `txn.put()`, `txn.get()`, `txn.commit()` |

---

## 2. Architecture Principles

### 2.1 M2-Specific Principles

1. **Optimism Over Pessimism**
   - Assume transactions won't conflict (agents rarely contend)
   - Validate at commit, not during execution
   - Retry on conflict, don't block preemptively

2. **Snapshot Isolation**
   - Each transaction sees consistent snapshot of data
   - Reads never block writers, writers never block readers
   - Trade-off: Possible phantom reads (acceptable for agents)

3. **Fail-Fast Validation**
   - Detect conflicts early in validation phase
   - Abort immediately on conflict, don't guess
   - Provide clear conflict information for retry logic

4. **Backwards Compatible with M1**
   - M1's single-operation API still works (implicit transactions)
   - Existing code doesn't break
   - Migration path: opt-in to explicit transactions

5. **Trait-Based Abstractions (Continued)**
   - `SnapshotView` trait: enables lazy snapshots in M3+
   - `ConflictDetector` trait: enables pluggable conflict strategies
   - Future-proof, simple implementations now

### 2.2 OCC Design Patterns

**Pattern: Three-Phase Commit**
```
BEGIN → READ/WRITE (build sets) → VALIDATE → COMMIT/ABORT
```

**Pattern: Version-Based Validation**
```
read_set: { key → version_read }
At commit: check current_version(key) == version_read
```

**Pattern: Snapshot Cloning (M2)**
```
Snapshot = deep clone of BTreeMap at version V
Reads come from snapshot (consistent)
```

**Pattern: Retry with Exponential Backoff**
```
loop {
    txn = begin_transaction()
    result = execute_txn(txn)
    match txn.commit() {
        Ok(_) => return result,
        Err(Conflict) => sleep(backoff), retry++
    }
}
```

---

## 3. Component Architecture

### 3.1 Concurrency Crate (`crates/concurrency`)

**Purpose**: Manage OCC transactions, snapshots, and conflict detection.

**New Files**:
- `src/transaction.rs` - TransactionContext lifecycle
- `src/snapshot.rs` - Snapshot creation and management
- `src/validation.rs` - Conflict detection logic
- `src/cas.rs` - Compare-and-swap operations

#### 3.1.1 TransactionContext

```rust
pub struct TransactionContext {
    // Identity
    pub txn_id: u64,
    pub run_id: RunId,

    // Snapshot isolation
    pub start_version: u64,
    pub snapshot: Box<dyn SnapshotView>,

    // Tracking for validation
    pub read_set: HashMap<Key, u64>,      // key → version read
    pub write_set: HashMap<Key, Value>,   // key → pending value
    pub delete_set: HashSet<Key>,         // keys to delete
    pub cas_set: Vec<CASOperation>,       // CAS operations

    // State
    pub status: TransactionStatus,
}

pub enum TransactionStatus {
    Active,
    Validating,
    Committed,
    Aborted { reason: String },
}

pub struct CASOperation {
    pub key: Key,
    pub expected_version: u64,
    pub new_value: Value,
}
```

**Lifecycle Methods**:

```rust
impl TransactionContext {
    /// Create new transaction with snapshot at current version
    pub fn new(
        txn_id: u64,
        run_id: RunId,
        storage: &dyn Storage,
    ) -> Result<Self> {
        let start_version = storage.current_version();
        let snapshot = ClonedSnapshotView::create(storage, start_version)?;

        Ok(TransactionContext {
            txn_id,
            run_id,
            start_version,
            snapshot: Box::new(snapshot),
            read_set: HashMap::new(),
            write_set: HashMap::new(),
            delete_set: HashSet::new(),
            cas_set: Vec::new(),
            status: TransactionStatus::Active,
        })
    }

    /// Read value (from snapshot or write set)
    pub fn get(&mut self, key: &Key) -> Result<Option<Value>> {
        // Check write set first (read-your-writes)
        if let Some(value) = self.write_set.get(key) {
            return Ok(Some(value.clone()));
        }

        // Check delete set
        if self.delete_set.contains(key) {
            return Ok(None);
        }

        // Read from snapshot
        let versioned = self.snapshot.get(key)?;

        // Track read for validation
        if let Some(ref v) = versioned {
            self.read_set.insert(key.clone(), v.version);
        }

        Ok(versioned.map(|v| v.value))
    }

    /// Write value (buffered until commit)
    pub fn put(&mut self, key: Key, value: Value) -> Result<()> {
        self.ensure_active()?;
        self.write_set.insert(key, value);
        Ok(())
    }

    /// Delete key (buffered until commit)
    pub fn delete(&mut self, key: Key) -> Result<()> {
        self.ensure_active()?;
        self.write_set.remove(&key);
        self.delete_set.insert(key);
        Ok(())
    }

    /// Compare-and-swap (buffered until commit)
    pub fn cas(&mut self, key: Key, expected_version: u64, new_value: Value) -> Result<()> {
        self.ensure_active()?;
        self.cas_set.push(CASOperation {
            key,
            expected_version,
            new_value,
        });
        Ok(())
    }

    /// Validate and commit
    pub fn commit(
        &mut self,
        storage: &dyn Storage,
        wal: &mut WAL,
    ) -> Result<()> {
        self.status = TransactionStatus::Validating;

        // Validation phase
        let conflicts = validate_transaction(self, storage)?;
        if !conflicts.is_empty() {
            self.status = TransactionStatus::Aborted {
                reason: format!("Conflicts: {:?}", conflicts),
            };
            return Err(Error::Concurrency(ConcurrencyError::WriteConflict { conflicts }));
        }

        // Apply phase (atomic)
        apply_transaction(self, storage, wal)?;

        self.status = TransactionStatus::Committed;
        Ok(())
    }

    fn ensure_active(&self) -> Result<()> {
        match self.status {
            TransactionStatus::Active => Ok(()),
            _ => Err(Error::Concurrency(ConcurrencyError::TransactionNotActive)),
        }
    }
}
```

#### 3.1.2 Snapshot Management

```rust
/// Trait for snapshot implementations
pub trait SnapshotView: Send + Sync {
    fn get(&self, key: &Key) -> Result<Option<VersionedValue>>;
    fn scan_prefix(&self, prefix: &Key) -> Result<Vec<(Key, VersionedValue)>>;
    fn version(&self) -> u64;
}

/// M2 Implementation: Cloned snapshot (deep copy)
pub struct ClonedSnapshotView {
    version: u64,
    data: Arc<BTreeMap<Key, VersionedValue>>,
}

impl ClonedSnapshotView {
    pub fn create(storage: &dyn Storage, version: u64) -> Result<Self> {
        // Acquire read lock on storage
        let data = storage.clone_data_at_version(version)?;

        Ok(ClonedSnapshotView {
            version,
            data: Arc::new(data),
        })
    }
}

impl SnapshotView for ClonedSnapshotView {
    fn get(&self, key: &Key) -> Result<Option<VersionedValue>> {
        Ok(self.data.get(key)
            .filter(|v| v.version <= self.version && !v.is_expired())
            .cloned())
    }

    fn scan_prefix(&self, prefix: &Key) -> Result<Vec<(Key, VersionedValue)>> {
        Ok(self.data
            .range(prefix..)
            .take_while(|(k, _)| k.starts_with_prefix(prefix))
            .filter(|(_, v)| v.version <= self.version && !v.is_expired())
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect())
    }

    fn version(&self) -> u64 {
        self.version
    }
}
```

**Known Limitation**: Cloning entire BTreeMap is expensive (memory + CPU). Acceptable for M2.

**Future (M3+)**: Lazy snapshot that reads from live store with version checks:

```rust
/// Future implementation (M3+)
pub struct LazySnapshotView {
    version: u64,
    storage: Arc<dyn Storage>,
}

impl SnapshotView for LazySnapshotView {
    fn get(&self, key: &Key) -> Result<Option<VersionedValue>> {
        self.storage.get_versioned(key, self.version)
    }
    // ... similar for scan_prefix
}
```

#### 3.1.3 Conflict Detection

```rust
/// Validate transaction against current storage state
pub fn validate_transaction(
    txn: &TransactionContext,
    storage: &dyn Storage,
) -> Result<Vec<ConflictInfo>> {
    let mut conflicts = Vec::new();

    // Phase 1: Validate read set (check versions unchanged)
    for (key, read_version) in &txn.read_set {
        if let Some(current) = storage.get(key)? {
            if current.version != *read_version {
                conflicts.push(ConflictInfo::ReadConflict {
                    key: key.clone(),
                    read_version: *read_version,
                    current_version: current.version,
                });
            }
        } else {
            // Key was deleted after we read it
            conflicts.push(ConflictInfo::ReadConflict {
                key: key.clone(),
                read_version: *read_version,
                current_version: 0, // deleted
            });
        }
    }

    // Phase 2: Validate write set (check no concurrent writes)
    for key in txn.write_set.keys() {
        if let Some(current) = storage.get(key)? {
            // Key exists in storage
            // Check if it was written after our snapshot
            if current.version > txn.start_version {
                // Someone wrote this key after our snapshot
                // Only conflict if we also read it
                if txn.read_set.contains_key(key) {
                    conflicts.push(ConflictInfo::WriteConflict {
                        key: key.clone(),
                        our_version: txn.start_version,
                        current_version: current.version,
                    });
                }
            }
        }
    }

    // Phase 3: Validate CAS operations
    for cas_op in &txn.cas_set {
        if let Some(current) = storage.get(&cas_op.key)? {
            if current.version != cas_op.expected_version {
                conflicts.push(ConflictInfo::CASConflict {
                    key: cas_op.key.clone(),
                    expected_version: cas_op.expected_version,
                    current_version: current.version,
                });
            }
        } else {
            // Key doesn't exist, but CAS expected a version
            conflicts.push(ConflictInfo::CASConflict {
                key: cas_op.key.clone(),
                expected_version: cas_op.expected_version,
                current_version: 0, // doesn't exist
            });
        }
    }

    Ok(conflicts)
}

#[derive(Debug, Clone)]
pub enum ConflictInfo {
    ReadConflict {
        key: Key,
        read_version: u64,
        current_version: u64,
    },
    WriteConflict {
        key: Key,
        our_version: u64,
        current_version: u64,
    },
    CASConflict {
        key: Key,
        expected_version: u64,
        current_version: u64,
    },
}
```

**Validation Strategy**:
- **Read conflicts**: Value changed since we read it
- **Write-write conflicts**: Concurrent write to same key we're writing
- **CAS conflicts**: Version doesn't match expected

**Conservative Approach**: Any conflict → abort transaction.

**Future**: Could allow some concurrent writes if they don't conflict with our reads (first-committer-wins).

#### 3.1.4 Transaction Application

```rust
/// Apply validated transaction to storage and WAL
pub fn apply_transaction(
    txn: &TransactionContext,
    storage: &dyn Storage,
    wal: &mut WAL,
) -> Result<()> {
    // Write to WAL first (durability)
    wal.append(&WALEntry::BeginTxn {
        txn_id: txn.txn_id,
        run_id: txn.run_id,
        timestamp: Timestamp::now(),
    })?;

    // Apply writes to storage
    for (key, value) in &txn.write_set {
        let version = storage.put(key.clone(), value.clone(), None)?;

        // Log each write
        wal.append(&WALEntry::Write {
            run_id: txn.run_id,
            key: key.clone(),
            value: value.clone(),
            version,
        })?;
    }

    // Apply deletes
    for key in &txn.delete_set {
        if let Some(old_value) = storage.delete(key)? {
            wal.append(&WALEntry::Delete {
                run_id: txn.run_id,
                key: key.clone(),
                version: old_value.version,
            })?;
        }
    }

    // Apply CAS operations
    for cas_op in &txn.cas_set {
        let version = storage.put(
            cas_op.key.clone(),
            cas_op.new_value.clone(),
            None
        )?;

        wal.append(&WALEntry::Write {
            run_id: txn.run_id,
            key: cas_op.key.clone(),
            value: cas_op.new_value.clone(),
            version,
        })?;
    }

    // Commit marker
    wal.append(&WALEntry::CommitTxn {
        txn_id: txn.txn_id,
        run_id: txn.run_id,
    })?;

    Ok(())
}
```

---

## 4. OCC Transaction Model

### 4.1 Transaction Phases

```
┌──────────────────────────────────────────────────────────┐
│                  OCC Transaction Lifecycle               │
└──────────────────────────────────────────────────────────┘

Phase 1: BEGIN
├─ Allocate txn_id
├─ Capture start_version
├─ Create snapshot (clone BTreeMap)
└─ Initialize read_set, write_set, cas_set

Phase 2: READ (from snapshot)
├─ txn.get(key)
│   ├─ Check write_set (read-your-writes)
│   ├─ Check delete_set
│   ├─ Read from snapshot
│   └─ Track in read_set { key → version }
└─ Repeatable reads (same snapshot)

Phase 3: WRITE (buffered)
├─ txn.put(key, value)
│   └─ Add to write_set (not visible to storage yet)
├─ txn.delete(key)
│   └─ Add to delete_set
└─ txn.cas(key, expected_version, value)
    └─ Add to cas_set

Phase 4: VALIDATE
├─ For each key in read_set:
│   └─ Check current_version == read_version
├─ For each key in write_set:
│   └─ Check no concurrent writes (if also in read_set)
└─ For each CAS operation:
    └─ Check current_version == expected_version

Phase 5: COMMIT (atomic)
├─ Acquire storage write lock
├─ Write to WAL:
│   ├─ BeginTxn
│   ├─ Write* (for each write_set entry)
│   ├─ Delete* (for each delete_set entry)
│   └─ CommitTxn
├─ Apply to storage:
│   ├─ put() for write_set
│   ├─ delete() for delete_set
│   └─ put() for cas_set
└─ Release lock

OR Phase 5: ABORT (on conflict)
└─ Discard write_set, delete_set, cas_set
```

### 4.2 Isolation Level: Snapshot Isolation

**Guarantees**:
1. **Repeatable Reads**: Reading same key twice returns same value
2. **No Dirty Reads**: Never see uncommitted writes from other transactions
3. **No Lost Updates**: CAS prevents concurrent overwrites

**Does NOT Guarantee**:
1. **Serializability**: Phantom reads possible (acceptable for agents)
2. **Write Skew Prevention**: Two transactions can update different keys based on same read

**Example**:
```rust
// Transaction 1
let txn1 = db.begin_transaction(run_id);
let x = txn1.get(b"x")?; // reads x=10
txn1.put(b"y", x + 5);   // writes y=15
txn1.commit()?;          // ✓ succeeds

// Transaction 2 (concurrent)
let txn2 = db.begin_transaction(run_id);
let x = txn2.get(b"x")?; // reads x=10 (same snapshot)
txn2.put(b"z", x + 3);   // writes z=13
txn2.commit()?;          // ✓ succeeds (no conflict: different keys)

// Both succeed because they write different keys
// If both wrote to same key, second would abort
```

### 4.3 Conflict Scenarios

**Scenario 1: Read-Write Conflict**
```
T1: read(x) = 10 (version 5)
T2: write(x, 20) → commit (version 6)
T1: write(y, 15) → commit
    ├─ Validation: read_set {x → 5}
    ├─ Current version of x = 6 (changed!)
    └─ ABORT (conflict)
```

**Scenario 2: Write-Write Conflict**
```
T1: write(x, 20)
T2: write(x, 30) → commit first
T1: commit
    ├─ Validation: write_set {x}
    ├─ x was written by T2 after T1's snapshot
    └─ ABORT (conflict)
```

**Scenario 3: CAS Conflict**
```
T1: cas(x, expected_version=5, new_value=20)
    ├─ Read x, version = 5
    └─ Buffer CAS operation
T2: write(x, 15) → commit (version 6)
T1: commit
    ├─ Validation: expected_version=5, current_version=6
    └─ ABORT (conflict)
```

**Scenario 4: No Conflict (Different Keys)**
```
T1: write(x, 10), write(y, 20) → commit
T2: write(z, 30), write(w, 40) → commit
    ├─ No overlapping keys
    └─ BOTH SUCCEED
```

---

## 5. Snapshot Isolation

### 5.1 Snapshot Creation

**M2 Approach: Clone Entire BTreeMap**

```rust
impl UnifiedStore {
    pub fn clone_data_at_version(&self, version: u64) -> Result<BTreeMap<Key, VersionedValue>> {
        let data = self.data.read();

        // Filter to only include entries <= version
        let snapshot: BTreeMap<_, _> = data
            .iter()
            .filter(|(_, v)| v.version <= version && !v.is_expired())
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        Ok(snapshot)
    }
}
```

**Cost**:
- **Memory**: O(data size) per concurrent transaction
- **CPU**: O(data size) to clone
- **Time**: ~10-100ms for 10K keys (acceptable for M2)

**Acceptable for M2 because**:
- Agents typically have < 10K keys per run
- Snapshots live only during transaction (short-lived)
- 2-3 concurrent transactions typical (not 100s)

### 5.2 Read-Your-Writes

Transactions see their own uncommitted writes:

```rust
impl TransactionContext {
    pub fn get(&mut self, key: &Key) -> Result<Option<Value>> {
        // Check write_set FIRST (read-your-writes)
        if let Some(value) = self.write_set.get(key) {
            return Ok(Some(value.clone()));
        }

        // Check delete_set
        if self.delete_set.contains(key) {
            return Ok(None);
        }

        // Fall back to snapshot
        self.snapshot.get(key).map(|opt| opt.map(|v| v.value))
    }
}
```

### 5.3 Snapshot Versioning

**Version Assignment**:
- `start_version`: Version at transaction begin (snapshot boundary)
- `commit_version`: Version after all writes applied (new state)

**Invariant**: All writes in a transaction get versions > start_version

**Example**:
```
Global version: 100

T1 begins → start_version = 100, snapshot at v100
T1 writes x, y, z (buffered)
T1 commits →
    x gets version 101
    y gets version 102
    z gets version 103
Global version: 103
```

---

## 6. Conflict Detection

### 6.1 Validation Algorithm

**Input**: TransactionContext, current Storage state

**Output**: List of conflicts (empty = valid)

**Steps**:

1. **Read Set Validation**
   ```
   For each (key, read_version) in txn.read_set:
       current = storage.get(key)
       if current.version != read_version:
           CONFLICT: key changed since we read it
   ```

2. **Write Set Validation**
   ```
   For each key in txn.write_set:
       if key also in txn.read_set:
           current = storage.get(key)
           if current.version > txn.start_version:
               CONFLICT: concurrent write to key we read
   ```

3. **CAS Set Validation**
   ```
   For each cas_op in txn.cas_set:
       current = storage.get(cas_op.key)
       if current.version != cas_op.expected_version:
           CONFLICT: version mismatch
   ```

### 6.2 First-Committer-Wins

**Strategy**: First transaction to commit wins, others abort.

**Example**:
```
T1 and T2 both write key "x"

T1 commits first:
    ├─ Validation passes
    ├─ x gets version 101
    └─ SUCCESS

T2 commits second:
    ├─ Validation: x version changed (was 100, now 101)
    └─ ABORT (conflict)
```

### 6.3 Retry Logic

**Automatic Retry** (caller's responsibility):

```rust
fn with_retry<F, T>(f: F) -> Result<T>
where
    F: Fn(&mut TransactionContext) -> Result<T>,
{
    let mut retries = 0;
    let max_retries = 5;

    loop {
        let mut txn = db.begin_transaction(run_id)?;

        match f(&mut txn) {
            Ok(result) => {
                match txn.commit() {
                    Ok(_) => return Ok(result),
                    Err(Error::Concurrency(ConcurrencyError::WriteConflict { .. })) => {
                        retries += 1;
                        if retries >= max_retries {
                            return Err(Error::Concurrency(
                                ConcurrencyError::MaxRetriesExceeded
                            ));
                        }

                        // Exponential backoff
                        let backoff = Duration::from_millis(10 * 2u64.pow(retries));
                        thread::sleep(backoff);
                        continue;
                    }
                    Err(e) => return Err(e),
                }
            }
            Err(e) => return Err(e),
        }
    }
}

// Usage
let result = with_retry(|txn| {
    let x = txn.get(b"x")?.unwrap_or(0);
    txn.put(b"x", x + 1)?;
    Ok(())
})?;
```

---

## 7. Transaction Lifecycle

### 7.1 Transaction States

```
Created
   │
   ├─ begin_transaction() allocates txn_id, creates snapshot
   │
   ▼
Active
   │
   ├─ get(), put(), delete(), cas() buffer operations
   │
   ├─ commit() triggers validation
   │
   ▼
Validating
   │
   ├─ Check read_set, write_set, cas_set against current storage
   │
   ├─ If valid: proceed to commit
   │
   ├─ If conflicts: abort
   │
   ▼
Committed  OR  Aborted
```

### 7.2 Example Transaction Flow

```rust
// 1. BEGIN
let mut txn = db.begin_transaction(run_id)?;
// txn.start_version = 100
// txn.snapshot = clone of BTreeMap at v100

// 2. READ
let x = txn.get(b"x")?.unwrap_or(0); // reads from snapshot, x=10
// txn.read_set = { "x" → version 99 }

// 3. WRITE (buffered)
txn.put(b"x", x + 5)?; // write_set = { "x" → 15 }
txn.put(b"y", x * 2)?; // write_set = { "x" → 15, "y" → 20 }

// 4. COMMIT
txn.commit()?;
// Validation:
//   - Check "x" still version 99 (✓)
// Apply:
//   - WAL: BeginTxn(42) → Write(x, 15, v101) → Write(y, 20, v102) → CommitTxn(42)
//   - Storage: put("x", 15, v101), put("y", 20, v102)
// Result: SUCCESS
```

### 7.3 Transaction Abortion

**Reasons**:
1. Conflict detected during validation
2. User calls `txn.abort()`
3. Error during transaction execution

**Cleanup**:
```rust
impl TransactionContext {
    pub fn abort(mut self) -> Result<()> {
        self.status = TransactionStatus::Aborted {
            reason: "User aborted".to_string(),
        };

        // Write AbortTxn to WAL (optional, for auditing)
        // No need to undo: writes were never applied to storage

        // Drop transaction (write_set, read_set discarded)
        Ok(())
    }
}
```

---

## 8. API Design

### 8.1 Database API (Updated)

```rust
impl Database {
    // M2: NEW - Explicit transactions
    pub fn begin_transaction(&self, run_id: RunId) -> Result<TransactionContext> {
        let txn_id = self.next_txn_id.fetch_add(1, Ordering::SeqCst);
        TransactionContext::new(txn_id, run_id, &*self.storage)
    }

    // M1: Backwards compatible - implicit single-operation transactions
    pub fn put(&self, run_id: RunId, key: &[u8], value: Value) -> Result<u64> {
        let mut txn = self.begin_transaction(run_id)?;
        let key = Key::new_kv(namespace_for_run(run_id), key);
        txn.put(key, value)?;
        txn.commit(&*self.storage, &mut *self.wal.lock())?;
        Ok(self.storage.current_version())
    }

    // M1: Backwards compatible
    pub fn get(&self, run_id: RunId, key: &[u8]) -> Result<Option<Value>> {
        // Direct read from storage (no transaction needed for single reads)
        let key = Key::new_kv(namespace_for_run(run_id), key);
        self.storage.get(&key).map(|opt| opt.map(|v| v.value))
    }
}
```

### 8.2 TransactionContext API

```rust
impl TransactionContext {
    // Read operations
    pub fn get(&mut self, key: &Key) -> Result<Option<Value>>;
    pub fn list(&mut self, prefix: &Key) -> Result<Vec<(Key, Value)>>;

    // Write operations (buffered)
    pub fn put(&mut self, key: Key, value: Value) -> Result<()>;
    pub fn put_with_ttl(&mut self, key: Key, value: Value, ttl: Duration) -> Result<()>;
    pub fn delete(&mut self, key: Key) -> Result<()>;

    // CAS operation
    pub fn cas(&mut self, key: Key, expected_version: u64, new_value: Value) -> Result<()>;

    // Commit/abort
    pub fn commit(self, storage: &dyn Storage, wal: &mut WAL) -> Result<()>;
    pub fn abort(self) -> Result<()>;

    // Introspection
    pub fn read_set_size(&self) -> usize;
    pub fn write_set_size(&self) -> usize;
    pub fn status(&self) -> &TransactionStatus;
}
```

### 8.3 Usage Examples

**Example 1: Simple Transaction**
```rust
let mut txn = db.begin_transaction(run_id)?;

let x = txn.get(&key_x)?.unwrap_or(0);
txn.put(key_y, x + 10)?;

txn.commit(&db.storage, &mut db.wal.lock())?;
```

**Example 2: CAS Operation**
```rust
let mut txn = db.begin_transaction(run_id)?;

let counter = txn.get(&counter_key)?;
let current_version = counter.map(|v| v.version).unwrap_or(0);

txn.cas(counter_key, current_version, counter.unwrap_or(0) + 1)?;

txn.commit(&db.storage, &mut db.wal.lock())?;
```

**Example 3: Multi-Key Atomic Update**
```rust
let mut txn = db.begin_transaction(run_id)?;

// Transfer between accounts (atomic)
let from_balance = txn.get(&from_key)?.unwrap();
let to_balance = txn.get(&to_key)?.unwrap();

txn.put(from_key, from_balance - amount)?;
txn.put(to_key, to_balance + amount)?;

txn.commit(&db.storage, &mut db.wal.lock())?;
```

**Example 4: Retry on Conflict**
```rust
fn increment_counter(db: &Database, run_id: RunId, key: Key) -> Result<()> {
    let mut retries = 0;

    loop {
        let mut txn = db.begin_transaction(run_id)?;

        let current = txn.get(&key)?.unwrap_or(0);
        txn.put(key.clone(), current + 1)?;

        match txn.commit(&db.storage, &mut db.wal.lock()) {
            Ok(_) => return Ok(()),
            Err(Error::Concurrency(ConcurrencyError::WriteConflict { .. })) => {
                retries += 1;
                if retries >= 5 {
                    return Err(Error::Concurrency(ConcurrencyError::MaxRetriesExceeded));
                }
                thread::sleep(Duration::from_millis(10 * 2u64.pow(retries)));
                continue;
            }
            Err(e) => return Err(e),
        }
    }
}
```

---

## 9. Performance Characteristics

### 9.1 Expected Performance (M2)

| Operation | M1 (RwLock) | M2 (OCC) | Notes |
|-----------|-------------|----------|-------|
| **Read (txn)** | <0.1ms | <0.1ms | From snapshot (no lock) |
| **Write (txn)** | Blocked by writers | <0.1ms | Buffered, no lock |
| **Commit** | N/A | 1-5ms | Validation + apply |
| **Conflict rate** | 0% | 1-5% | Depends on workload |
| **Concurrent reads** | Many | Unlimited | No blocking |
| **Concurrent writes** | Serialized | Parallel (until commit) | OCC benefit |

### 9.2 Snapshot Overhead

**M2 (ClonedSnapshotView)**:
- **Memory**: O(data size) × concurrent_txns
- **CPU**: O(data size) per snapshot creation
- **Time**: 10-100ms for 10K keys

**Example**:
- 10K keys, 1KB each = 10MB data
- 3 concurrent transactions = 30MB snapshots
- Clone time: ~50ms (one-time cost at txn begin)

**Acceptable for M2**: Agents typically < 10K keys, < 5 concurrent transactions.

### 9.3 Conflict Rates

**Low Conflict Workload** (typical for agents):
- Different agents write different keys
- Conflict rate: <1%
- Most transactions commit first try

**High Conflict Workload** (pathological):
- All transactions write same key (counter)
- Conflict rate: >50%
- Many retries (exponential backoff helps)

**Design**: Optimized for low conflict (agent workflows).

### 9.4 Bottlenecks (M2)

| Bottleneck | Impact | Mitigation |
|------------|--------|------------|
| **Snapshot cloning** | 10-100ms per txn begin | M3: Lazy snapshots |
| **Validation cost** | O(read_set + write_set) | Typical sets are small (<100 keys) |
| **Global version counter** | AtomicU64 contention | Acceptable for M2, shard in M4 |
| **Retry overhead** | Wasted work on conflict | Exponential backoff, low conflict expected |

---

## 10. Testing Strategy

### 10.1 Unit Tests

**Per Component**:
- **TransactionContext**: begin, read, write, commit, abort
- **Snapshot**: creation, read consistency, version filtering
- **Validation**: conflict detection, CAS validation
- **Conflict scenarios**: read-write, write-write, CAS conflicts

### 10.2 Concurrency Tests

**Scenarios**:
1. **Concurrent reads**: 10 threads read same keys (should succeed)
2. **Concurrent writes (different keys)**: Should all commit
3. **Concurrent writes (same key)**: First wins, others abort
4. **CAS contention**: Multiple threads CAS same counter (some abort)
5. **Long transaction**: Hold txn open, validate against concurrent commits

**Tools**:
- `std::thread` for multi-threading
- `crossbeam` for channels/synchronization
- `loom` for concurrency bug detection (optional)

### 10.3 Conflict Resolution Tests

**Test Cases**:
```rust
#[test]
fn test_read_write_conflict() {
    let db = Database::open_in_memory()?;
    let run_id = RunId::new();

    // T1: read x
    let mut txn1 = db.begin_transaction(run_id)?;
    let _x = txn1.get(&key_x)?;

    // T2: write x, commit
    let mut txn2 = db.begin_transaction(run_id)?;
    txn2.put(key_x.clone(), 20)?;
    txn2.commit(&db.storage, &mut db.wal.lock())?;

    // T1: write y, commit → should abort (read conflict on x)
    txn1.put(key_y, 15)?;
    let result = txn1.commit(&db.storage, &mut db.wal.lock());

    assert!(matches!(result, Err(Error::Concurrency(ConcurrencyError::WriteConflict { .. }))));
}
```

### 10.4 Snapshot Isolation Tests

**Verify**:
- Repeatable reads within transaction
- Transactions see snapshot, not live updates
- Read-your-writes within transaction

**Example**:
```rust
#[test]
fn test_snapshot_isolation() {
    let db = Database::open_in_memory()?;
    db.put(run_id, b"x", 10)?; // version 1

    let mut txn = db.begin_transaction(run_id)?;
    let x1 = txn.get(&key_x)?; // reads 10

    // Concurrent update
    db.put(run_id, b"x", 20)?; // version 2

    let x2 = txn.get(&key_x)?; // should still read 10 (snapshot)

    assert_eq!(x1, x2); // repeatable read
    assert_eq!(x1, Some(10));
}
```

### 10.5 Performance Benchmarks

**Metrics**:
- Transaction throughput (commits/sec)
- Conflict rate under various concurrency levels
- Snapshot creation time vs. data size
- Validation time vs. read_set/write_set size

**Target**:
- >1K transactions/sec (single-threaded)
- >5K transactions/sec (4 threads, low conflict)
- <1% conflict rate (typical agent workload)
- <100ms snapshot creation (10K keys)

---

## 11. Migration from M1

### 11.1 Backwards Compatibility

**M1 API still works** (implicit transactions):

```rust
// M1 code (unchanged)
db.put(run_id, b"key", value)?;
let val = db.get(run_id, b"key")?;

// Internally becomes:
let mut txn = db.begin_transaction(run_id)?;
txn.put(key, value)?;
txn.commit()?;
```

**No breaking changes** for existing code.

### 11.2 Migration Path

**Phase 1: M1 code continues working**
- Keep using `db.put()`, `db.get()`
- No changes required

**Phase 2: Opt-in to explicit transactions**
- Identify code that needs atomicity
- Refactor to use `begin_transaction()` / `commit()`

**Phase 3: Enable multi-operation transactions**
- Group related operations in same transaction
- Add retry logic for conflicts

**Example Migration**:

```rust
// Before (M1)
db.put(run_id, b"x", x_val)?;
db.put(run_id, b"y", y_val)?;
db.put(run_id, b"z", z_val)?;

// After (M2) - atomic multi-operation
let mut txn = db.begin_transaction(run_id)?;
txn.put(key_x, x_val)?;
txn.put(key_y, y_val)?;
txn.put(key_z, z_val)?;
txn.commit(&db.storage, &mut db.wal.lock())?;
```

### 11.3 Recovery Compatibility

**M1 WAL entries** are still valid:
- BeginTxn, Write, Delete, CommitTxn, AbortTxn

**M2 recovery** handles M1 WAL:
- Same replay logic (apply committed transactions)
- No format changes needed

**Forward compatible**: M2 WAL can be replayed by M2 recovery.

---

## 12. Known Limitations

### 12.1 M2 Limitations

| Limitation | Impact | Mitigation Plan |
|------------|--------|-----------------|
| **Snapshot cloning** | Memory + CPU cost | M3: Lazy snapshots (version-checked reads) |
| **No serializability** | Write skew possible | Acceptable for agents; use CAS if needed |
| **Global version counter** | AtomicU64 contention | M4: Per-namespace versions |
| **Retry overhead** | Wasted work on conflict | Exponential backoff; low conflict expected |
| **No deadlock detection** | Transactions can't wait | Not needed (OCC doesn't block) |

### 12.2 What M2 Does NOT Provide

- ❌ Event Log, State Machine, Trace primitives (M3)
- ❌ Snapshots and WAL rotation (M4)
- ❌ Deterministic replay (M5)
- ❌ Vector store (M6)
- ❌ Network layer (M7)

---

## 13. Future Extension Points

### 13.1 M3: Lazy Snapshots

Replace `ClonedSnapshotView` with `LazySnapshotView`:

```rust
pub struct LazySnapshotView {
    version: u64,
    storage: Arc<dyn Storage>,
}

impl SnapshotView for LazySnapshotView {
    fn get(&self, key: &Key) -> Result<Option<VersionedValue>> {
        self.storage.get_versioned(key, self.version)
    }
}
```

**Benefits**:
- No snapshot cloning (zero memory overhead)
- Faster txn begin (<1ms)

**Trade-off**:
- Every read hits storage (with version check)
- Read lock contention

### 13.2 M4: Optimistic Locking Improvements

**First-Writer-Wins** instead of First-Committer-Wins:
- Allow concurrent writes to different keys
- Only abort if conflicting keys

**Predicate Locking**:
- Track range scans, detect phantom reads
- Requires more complex validation

### 13.3 M5: Replay Integration

**Replay Transactions**:
- Replay txn_id instead of individual operations
- Use WAL txn boundaries for O(txn count) replay

---

## 14. Appendix

### 14.1 Crate Structure (Updated)

```
in-mem/
├── crates/
│   ├── core/                     # M1 (unchanged)
│   ├── storage/                  # M1 (minor updates)
│   │   └── src/
│   │       └── unified.rs        # Add clone_data_at_version()
│   ├── durability/               # M1 (unchanged)
│   ├── concurrency/              # M2 (NEW)
│   │   └── src/
│   │       ├── transaction.rs    # TransactionContext
│   │       ├── snapshot.rs       # ClonedSnapshotView
│   │       ├── validation.rs     # Conflict detection
│   │       └── cas.rs            # CAS operations
│   ├── engine/                   # M1 + M2
│   │   └── src/
│   │       └── database.rs       # Add begin_transaction()
│   └── primitives/               # M1 (unchanged in M2)
```

### 14.2 Dependencies (Updated)

**New Dependencies**:
- (None - M2 uses std library only)

**Internal Dependencies**:
```
concurrency → core, storage
engine → core, storage, durability, concurrency (NEW)
```

---

## Conclusion

M2 adds **Optimistic Concurrency Control** to the foundation built in M1:
- ✅ Non-blocking concurrent reads
- ✅ Snapshot isolation for transactions
- ✅ Multi-operation atomic transactions
- ✅ CAS operations for coordination
- ✅ Backwards compatible with M1

**Success Criteria**:
- Concurrent transactions with snapshot isolation
- Conflict detection and retry working
- CAS operations functional
- >1K txns/sec throughput (low conflict)
- Integration tests pass with multiple threads
- M1 code still works (backwards compatible)

**Next**: M3 adds Event Log, State Machine, Trace Store, and Run Index primitives.

---

**Document Version**: 1.0
**Status**: Planning Phase
**Date**: 2026-01-11
