# Storage Extension Guide: Adding New Primitives

## Overview

After the storage stabilization milestone, the storage layer is stable. Adding a new primitive (like Vector) requires implementing specific extension points, but **NO changes to**:

- WAL entry envelope format
- Snapshot envelope format
- Recovery engine core logic
- Replay engine core logic

## The Core Guarantee

> **Adding a primitive must NOT require changes to WAL core format, Snapshot core format, Recovery engine, or Replay engine. Only extension points.**

## Steps to Add a New Primitive

### 1. Allocate WAL Entry Types

Each primitive is allocated a 16-byte range of WAL entry types. Request a range allocation by opening an issue.

| Primitive | Range | Example Types | Status |
|-----------|-------|---------------|--------|
| Core | 0x00-0x0F | Commit=0x00, Abort=0x01, Snapshot=0x02 | FROZEN |
| KV | 0x10-0x1F | Put=0x10, Delete=0x11 | FROZEN |
| JSON | 0x20-0x2F | Create=0x20, Set=0x21, Delete=0x22, Patch=0x23 | FROZEN |
| Event | 0x30-0x3F | Append=0x30 | FROZEN |
| State | 0x40-0x4F | Init=0x40, Set=0x41, Transition=0x42 | FROZEN |
| Trace | 0x50-0x5F | Record=0x50 | FROZEN |
| Run | 0x60-0x6F | Create=0x60, Update=0x61, End=0x62, Begin=0x63 | FROZEN |
| **Vector** | **0x70-0x7F** | Insert=0x70, Delete=0x71, Update=0x72 | RESERVED |
| Future | 0x80-0xFF | Available | AVAILABLE |

### 2. Assign Primitive Type ID

Each primitive has a unique type ID for snapshot sections:

| Primitive | Type ID | Status |
|-----------|---------|--------|
| KV | 1 | FROZEN |
| JSON | 2 | FROZEN |
| Event | 3 | FROZEN |
| State | 4 | FROZEN |
| Trace | 5 | FROZEN |
| Run | 6 | FROZEN |
| **Vector** | **7** | RESERVED |
| Future | 8-255 | AVAILABLE |

### 3. Implement PrimitiveStorageExt

Create your primitive and implement the `PrimitiveStorageExt` trait:

```rust
use in_mem_storage::{PrimitiveStorageExt, PrimitiveExtError};

impl PrimitiveStorageExt for VectorStore {
    fn primitive_type_id(&self) -> u8 {
        7  // Allocated type ID
    }

    fn wal_entry_types(&self) -> &'static [u8] {
        &[0x70, 0x71, 0x72]  // VectorInsert, VectorDelete, VectorUpdate
    }

    fn snapshot_serialize(&self) -> Result<Vec<u8>, PrimitiveExtError> {
        // Serialize vector data (NOT indexes - those are derived)
        bincode::serialize(&self.vectors)
            .map_err(|e| PrimitiveExtError::Serialization(e.to_string()))
    }

    fn snapshot_deserialize(&mut self, data: &[u8]) -> Result<(), PrimitiveExtError> {
        self.vectors = bincode::deserialize(data)
            .map_err(|e| PrimitiveExtError::Deserialization(e.to_string()))?;
        Ok(())
    }

    fn apply_wal_entry(&mut self, entry_type: u8, payload: &[u8]) -> Result<(), PrimitiveExtError> {
        match entry_type {
            0x70 => self.apply_insert(payload),
            0x71 => self.apply_delete(payload),
            0x72 => self.apply_update(payload),
            _ => Err(PrimitiveExtError::UnknownEntryType(entry_type)),
        }
    }

    fn primitive_name(&self) -> &'static str {
        "vector"
    }

    fn rebuild_indexes(&mut self) -> Result<(), PrimitiveExtError> {
        // Rebuild HNSW index from vectors after recovery
        self.rebuild_hnsw_index()
            .map_err(|e| PrimitiveExtError::IndexRebuild(e.to_string()))
    }
}
```

### 4. Register in PrimitiveRegistry

Register your primitive so the recovery and snapshot systems can find it:

```rust
use in_mem_storage::PrimitiveRegistry;

fn setup_database() {
    let mut registry = PrimitiveRegistry::new();

    // Register existing primitives
    registry.register(Arc::new(kv_storage_ext));
    registry.register(Arc::new(json_storage_ext));
    // ... etc ...

    // Register new Vector primitive
    registry.register(Arc::new(vector_storage_ext));
}
```

### 5. Add WAL Entry Types to wal_entry_types.rs

Add your entry types to the WAL entry type enum:

```rust
#[repr(u8)]
pub enum WalEntryType {
    // ... existing types ...

    // Vector Primitive (0x70-0x7F)
    VectorInsert = 0x70,
    VectorDelete = 0x71,
    VectorUpdate = 0x72,
}
```

## What NOT to Modify

When adding a new primitive, you should NOT need to modify:

| File | Reason |
|------|--------|
| `WalEntry::serialize()` | Entry format is primitive-agnostic |
| `WalEntry::deserialize()` | Entry format is primitive-agnostic |
| `SnapshotWriter::write()` | Uses PrimitiveStorageExt trait |
| `SnapshotReader::read()` | Uses PrimitiveStorageExt trait |
| `Recovery::recover()` | Routes via PrimitiveRegistry |
| `Replay::replay()` | Routes via PrimitiveRegistry |

If you find yourself modifying these, **stop and reconsider**. The extension points should handle everything.

## Testing New Primitives

### 1. Unit Tests

Test serialize/deserialize roundtrip:

```rust
#[test]
fn test_snapshot_roundtrip() {
    let mut store = VectorStore::new();
    store.insert(vec![1.0, 2.0, 3.0]);

    let serialized = store.snapshot_serialize().unwrap();

    let mut restored = VectorStore::new();
    restored.snapshot_deserialize(&serialized).unwrap();

    assert_eq!(store.len(), restored.len());
}
```

### 2. WAL Entry Tests

Test WAL entry roundtrip:

```rust
#[test]
fn test_wal_entry_roundtrip() {
    let mut store = VectorStore::new();

    // Create WAL entry
    let entry = store.create_insert_entry(vec![1.0, 2.0]);
    let serialized = entry.serialize().unwrap();
    let (deserialized, _) = WalEntry::deserialize(&serialized, 0).unwrap();

    // Apply entry
    store.apply_wal_entry(deserialized.entry_type as u8, &deserialized.payload).unwrap();

    assert_eq!(store.len(), 1);
}
```

### 3. Recovery Integration Tests

Test that your primitive survives recovery:

```rust
#[test]
fn test_vector_survives_recovery() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path();

    // Create database and add vectors
    {
        let db = Database::open(db_path).unwrap();
        let vectors = db.vectors();
        vectors.insert(&run_id, vec![1.0, 2.0, 3.0]).unwrap();
        db.shutdown().unwrap();
    }

    // Reopen and verify
    {
        let db = Database::open(db_path).unwrap();
        let vectors = db.vectors();
        assert!(vectors.get(&run_id, 0).is_some());
    }
}
```

### 4. Cross-Primitive Transaction Tests

Test atomic transactions across primitives:

```rust
#[test]
fn test_cross_primitive_atomicity() {
    let db = Database::open_in_memory().unwrap();

    db.transaction(|txn| {
        txn.kv().put(&run_id, "key", "value")?;
        txn.vectors().insert(&run_id, vec![1.0, 2.0])?;
        Ok(())
    }).unwrap();

    // Both should be visible
    assert!(db.kv().get(&run_id, "key").is_some());
    assert!(db.vectors().get(&run_id, 0).is_some());
}
```

## Checklist for New Primitives

- [ ] Allocated WAL entry type range
- [ ] Allocated primitive type ID
- [ ] Implemented `PrimitiveStorageExt` trait
- [ ] Registered in `PrimitiveRegistry`
- [ ] Added WAL entry types to enum
- [ ] Unit tests for serialize/deserialize
- [ ] WAL entry roundtrip tests
- [ ] Recovery integration tests
- [ ] Cross-primitive transaction tests
- [ ] Documentation updated
