# Cross-Primitive Atomicity Design

## Problem Statement

The current system lacks true cross-primitive atomicity:

| Primitive | Transactional? | WAL Behavior |
|-----------|----------------|--------------|
| KV | ✅ Yes | BeginTxn → Write → CommitTxn |
| JSON | ⚠️ Partial | Stored as KV bytes (inherits KV behavior) |
| Vector | ❌ No | Direct WAL append, no transaction framing |
| Event | ❌ No | Not implemented |
| State | ❌ No | Not implemented |

**Goal:** Enable atomic transactions like:
```rust
db.transaction(run_id, |txn| {
    txn.kv_put("user:123", user_data)?;
    txn.json_set("profile:123", &path, profile)?;
    txn.vector_upsert("embeddings", "user:123", embedding)?;
    txn.event_append("audit", event_data)?;
    Ok(())
})?;  // All succeed or all fail
```

## Current Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    TransactionContext                        │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐          │
│  │  write_set  │  │ delete_set  │  │   cas_set   │          │
│  │  (KV puts)  │  │ (KV deletes)│  │  (CAS ops)  │          │
│  └─────────────┘  └─────────────┘  └─────────────┘          │
│                                                              │
│  JsonStoreExt: json_get/json_set → stored as KV bytes       │
└──────────────────────────┬──────────────────────────────────┘
                           │ commit()
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                         WAL                                  │
│  BeginTxn → Write → Write → ... → CommitTxn                 │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│                      VectorStore                             │
│  (Writes directly to WAL, bypasses transaction system)      │
│  VectorUpsert → WAL (no BeginTxn/CommitTxn framing)         │
└─────────────────────────────────────────────────────────────┘
```

## Proposed Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    TransactionContext                        │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐          │
│  │  write_set  │  │ delete_set  │  │   cas_set   │          │
│  │  (KV puts)  │  │ (KV deletes)│  │  (CAS ops)  │          │
│  └─────────────┘  └─────────────┘  └─────────────┘          │
│                                                              │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐          │
│  │ vector_ops  │  │ event_ops   │  │ state_ops   │          │
│  │  (buffered) │  │  (buffered) │  │  (buffered) │          │
│  └─────────────┘  └─────────────┘  └─────────────┘          │
│                                                              │
│  Traits: KvOps, JsonStoreExt, VectorTxnExt, EventTxnExt     │
└──────────────────────────┬──────────────────────────────────┘
                           │ commit()
                           ▼
┌─────────────────────────────────────────────────────────────┐
│                         WAL                                  │
│  BeginTxn → Write → VectorUpsert → EventAppend → CommitTxn  │
│            (all primitives within transaction framing)       │
└─────────────────────────────────────────────────────────────┘
```

## Implementation Phases

### Phase 1: Extend TransactionContext with Vector Support

**Files to modify:**
- `crates/concurrency/src/transaction.rs`

**Changes:**

```rust
// Add to TransactionContext struct
pub struct TransactionContext {
    // Existing fields...
    pub txn_id: u64,
    pub run_id: RunId,
    pub write_set: BTreeMap<Key, Value>,
    pub delete_set: HashSet<Key>,
    pub cas_set: Vec<CASOperation>,

    // NEW: Vector operations buffer
    vector_ops: Vec<VectorOperation>,
}

// New vector operation enum
#[derive(Debug, Clone)]
pub enum VectorOperation {
    CollectionCreate {
        collection: String,
        dimension: usize,
        metric: u8,
    },
    CollectionDelete {
        collection: String,
    },
    Upsert {
        collection: String,
        key: String,
        vector_id: u64,
        embedding: Vec<f32>,
        metadata: Option<Vec<u8>>,
        source_ref: Option<EntityRef>,
    },
    Delete {
        collection: String,
        key: String,
        vector_id: u64,
    },
}

// New trait for vector operations in transactions
pub trait VectorTxnExt {
    fn vector_upsert(
        &mut self,
        collection: &str,
        key: &str,
        embedding: Vec<f32>,
        metadata: Option<Vec<u8>>,
    ) -> Result<()>;

    fn vector_delete(&mut self, collection: &str, key: &str) -> Result<()>;

    fn vector_create_collection(
        &mut self,
        collection: &str,
        dimension: usize,
        metric: DistanceMetric,
    ) -> Result<()>;
}
```

**Key insight:** Vector operations are buffered in `vector_ops` during the transaction, then written to WAL during commit with proper transaction framing.

### Phase 2: Update WAL Writing

**Files to modify:**
- `crates/concurrency/src/transaction.rs` (write_to_wal method)
- `crates/concurrency/src/wal_writer.rs`

**Changes to `write_to_wal`:**

```rust
pub fn write_to_wal(
    &self,
    wal_writer: &mut TransactionWALWriter,
    commit_version: u64,
) -> Result<()> {
    // Existing: Write KV puts
    for (key, value) in &self.write_set {
        wal_writer.write_put(key.clone(), value.clone(), commit_version)?;
    }

    // Existing: Write KV deletes
    for key in &self.delete_set {
        wal_writer.write_delete(key.clone(), commit_version)?;
    }

    // NEW: Write vector operations
    for op in &self.vector_ops {
        match op {
            VectorOperation::Upsert { collection, key, vector_id, embedding, metadata, source_ref } => {
                wal_writer.write_vector_upsert(
                    collection.clone(),
                    key.clone(),
                    *vector_id,
                    embedding.clone(),
                    metadata.clone(),
                    commit_version,
                    source_ref.clone(),
                )?;
            }
            VectorOperation::Delete { collection, key, vector_id } => {
                wal_writer.write_vector_delete(
                    collection.clone(),
                    key.clone(),
                    *vector_id,
                    commit_version,
                )?;
            }
            // ... other operations
        }
    }

    Ok(())
}
```

### Phase 3: Update Recovery

**Files to modify:**
- `crates/durability/src/recovery.rs`

The recovery code already handles VectorUpsert/VectorDelete entries. The key change is ensuring they're grouped by transaction:

```rust
// In replay_wal_with_options, the match already includes vector ops:
WALEntry::VectorUpsert { run_id, .. }
| WALEntry::VectorDelete { run_id, .. } => {
    // Add to the currently active transaction for this run_id
    if let Some(&internal_id) = active_txn_per_run.get(run_id) {
        if let Some(txn) = transactions.get_mut(&internal_id) {
            txn.entries.push(entry.clone());
        }
    }
}
```

**Additional change needed:** `apply_transaction` must handle vector entries:

```rust
fn apply_transaction<S: Storage + ?Sized>(
    storage: &S,
    txn: &Transaction,
    stats: &mut ReplayStats,
    vector_store: Option<&VectorStore>,  // NEW parameter
) -> Result<()> {
    for entry in &txn.entries {
        match entry {
            // Existing KV handling...
            WALEntry::Write { key, value, version, .. } => { ... }
            WALEntry::Delete { key, version, .. } => { ... }

            // NEW: Vector handling
            WALEntry::VectorUpsert { collection, key, vector_id, embedding, metadata, version, source_ref, .. } => {
                if let Some(vs) = vector_store {
                    vs.replay_upsert(collection, key, *vector_id, embedding, metadata.clone(), *version, source_ref.clone())?;
                }
                stats.vectors_applied += 1;
            }
            WALEntry::VectorDelete { collection, key, vector_id, version, .. } => {
                if let Some(vs) = vector_store {
                    vs.replay_delete(collection, key, *vector_id, *version)?;
                }
                stats.vectors_applied += 1;
            }
            // ...
        }
    }
    Ok(())
}
```

### Phase 4: Migrate VectorStore

**Files to modify:**
- `crates/primitives/src/vector/store.rs`

**Current behavior (to remove):**
```rust
// VectorStore::upsert() currently does:
self.write_wal_entry(WALEntry::VectorUpsert { ... })?;  // Direct WAL write
```

**New behavior:**
```rust
// Option A: VectorStore takes a transaction context
impl VectorStore {
    pub fn upsert_in_txn(
        &self,
        txn: &mut TransactionContext,
        collection: &str,
        key: &str,
        embedding: Vec<f32>,
        metadata: Option<Vec<u8>>,
    ) -> VectorResult<VectorId> {
        // Validate, allocate vector_id, etc.
        let vector_id = self.allocate_vector_id(collection)?;

        // Buffer in transaction (NOT direct WAL write)
        txn.vector_upsert(collection, key, vector_id, embedding, metadata)?;

        Ok(vector_id)
    }
}

// Option B: Use VectorTxnExt trait on TransactionContext
impl VectorTxnExt for TransactionContext {
    fn vector_upsert(&mut self, collection: &str, key: &str, embedding: Vec<f32>, metadata: Option<Vec<u8>>) -> Result<()> {
        // Need access to VectorStore for ID allocation...
        // This is trickier - may need to pass store reference
    }
}
```

**Recommendation:** Option A is cleaner - have VectorStore methods that take `&mut TransactionContext`.

### Phase 5: Add Event and State Support (Future)

Same pattern as vectors:

1. Add `event_ops: Vec<EventOperation>` to TransactionContext
2. Add `state_ops: Vec<StateOperation>` to TransactionContext
3. Implement `EventTxnExt` and `StateTxnExt` traits
4. Update `write_to_wal` to emit EventAppend, StateSet entries
5. Update recovery to apply event/state entries

## Migration Strategy

### Step 1: Non-Breaking Addition
- Add `vector_ops` field to TransactionContext (empty by default)
- Add `VectorTxnExt` trait with default no-op implementation
- Existing code continues to work

### Step 2: Add Transactional Vector API
- Implement `VectorStore::upsert_in_txn()` alongside existing `upsert()`
- Both work, users can migrate gradually

### Step 3: Update Recovery
- Recovery handles both transactional and non-transactional vector entries
- Non-transactional entries (no BeginTxn) are applied immediately
- Transactional entries are grouped and applied atomically

### Step 4: Deprecate Direct WAL Writes
- Mark `VectorStore::upsert()` as deprecated
- Log warning when vectors are written outside transactions
- Eventually remove non-transactional path

## API Examples

### Before (Current)
```rust
// KV + JSON are transactional
db.transaction(run_id, |txn| {
    txn.put(kv_key, value)?;
    txn.json_set(&json_key, &path, json_value)?;
    Ok(())
})?;

// Vectors are separate, not atomic with above
vector_store.upsert(run_id, "collection", "key", embedding, None)?;
```

### After (Proposed)
```rust
// All primitives in one atomic transaction
db.transaction(run_id, |txn| {
    txn.put(kv_key, value)?;
    txn.json_set(&json_key, &path, json_value)?;
    vector_store.upsert_in_txn(txn, "collection", "key", embedding, None)?;
    Ok(())
})?;
```

## WAL Format

No changes to WAL entry format. The change is in **ordering**:

### Before
```
WAL:
  VectorUpsert { run_id, collection, key, ... }  // No transaction framing
  VectorUpsert { run_id, collection, key, ... }  // No transaction framing
  BeginTxn { txn_id: 1, run_id }
  Write { run_id, key, value, version }
  CommitTxn { txn_id: 1, run_id }
```

### After
```
WAL:
  BeginTxn { txn_id: 1, run_id }
  Write { run_id, key, value, version }
  VectorUpsert { run_id, collection, key, ... }  // Inside transaction
  CommitTxn { txn_id: 1, run_id }
```

Recovery already groups by `run_id` and applies only committed transactions, so this "just works" once vector ops are inside the framing.

## Testing Strategy

1. **Unit tests:** TransactionContext buffers vector ops correctly
2. **Integration tests:** Commit writes all ops atomically to WAL
3. **Recovery tests:** Incomplete transactions with vector ops are discarded
4. **Crash tests:** Simulate crash mid-transaction, verify atomicity

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| Vector ID allocation in transaction | Allocate ID optimistically, rollback on abort |
| Large embeddings in memory | Consider streaming/chunked writes for very large batches |
| Backward compatibility | Keep non-transactional API working during migration |
| Recovery performance | Vector ops already in recovery code path |

## Timeline Estimate

| Phase | Scope |
|-------|-------|
| Phase 1 | Extend TransactionContext |
| Phase 2 | Update WAL writing |
| Phase 3 | Update recovery |
| Phase 4 | Migrate VectorStore |
| Phase 5 | Event/State (future) |

## Open Questions

1. **Vector ID allocation:** Should IDs be allocated at buffer time or commit time?
   - Buffer time: Simpler, but IDs may be "wasted" on abort
   - Commit time: More complex, but no wasted IDs

2. **Read-your-writes for vectors:** Should `vector_search()` see uncommitted upserts?
   - Current JSON behavior: Yes (reads from buffer)
   - Vectors are more complex (index structures)

3. **Conflict detection for vectors:** How to detect concurrent modifications?
   - KV uses version-based OCC
   - Vectors may need collection-level or key-level locking

## Conclusion

The path forward is clear:
1. Extend TransactionContext with primitive-specific operation buffers
2. Write all buffered operations within BeginTxn/CommitTxn framing
3. Recovery already handles the grouping - just needs to apply vector ops

No new WAL system needed. The existing WAL entry types and transaction framing are sufficient.
