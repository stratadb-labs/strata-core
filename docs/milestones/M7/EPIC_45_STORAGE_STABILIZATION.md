# Epic 45: Storage Stabilization

**Goal**: Freeze storage APIs for future primitives

**Dependencies**: Epic 41 (Crash Recovery), Epic 42 (WAL Enhancement)

---

## Scope

- PrimitiveStorageExt trait for new primitives
- Primitive registry for dynamic handling
- Extension point documentation
- WAL entry type allocation
- Snapshot section format

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #321 | PrimitiveStorageExt Trait | FOUNDATION |
| #322 | Primitive Registry Implementation | CRITICAL |
| #323 | Extension Point Documentation | HIGH |
| #324 | WAL Entry Type Allocation | HIGH |
| #325 | Snapshot Section Format | HIGH |

---

## Story #321: PrimitiveStorageExt Trait

**File**: `crates/storage/src/primitive_ext.rs` (NEW)

**Deliverable**: Trait for new primitives to implement

### Implementation

```rust
use crate::wal::{WalEntry, WalEntryType};

/// Trait that new primitives must implement for storage integration
///
/// After M7, adding a primitive requires implementing this trait.
/// The recovery engine, snapshot system, and replay engine use this
/// trait to handle primitives generically.
///
/// Example for Vector (M8):
/// ```rust
/// impl PrimitiveStorageExt for VectorStore {
///     fn primitive_type_id(&self) -> u8 { 7 }
///     fn wal_entry_types(&self) -> &'static [u8] { &[0x70, 0x71, 0x72] }
///     // ...
/// }
/// ```
pub trait PrimitiveStorageExt: Send + Sync {
    /// Unique identifier for this primitive type
    ///
    /// Used in snapshot sections. Must be unique and stable.
    /// Core primitives use 1-6. Vector (M8) will use 7.
    fn primitive_type_id(&self) -> u8;

    /// WAL entry types this primitive uses
    ///
    /// Used during recovery to route entries to the right primitive.
    /// Must be from the primitive's allocated range.
    fn wal_entry_types(&self) -> &'static [u8];

    /// Serialize primitive state for snapshot
    ///
    /// Called during snapshot creation. Should serialize all
    /// state needed to restore this primitive.
    fn snapshot_serialize(&self) -> Result<Vec<u8>, StorageError>;

    /// Deserialize primitive state from snapshot
    ///
    /// Called during recovery. Should restore all state
    /// from the serialized data.
    fn snapshot_deserialize(&mut self, data: &[u8]) -> Result<(), StorageError>;

    /// Apply a WAL entry during recovery
    ///
    /// Called for each entry with an entry type in wal_entry_types().
    /// Must be deterministic and idempotent.
    fn apply_wal_entry(&mut self, entry: &WalEntry) -> Result<(), StorageError>;

    /// Create WAL entries for an operation
    ///
    /// Called when writing to this primitive. Returns entries
    /// to be written to WAL as part of a transaction.
    fn to_wal_entries(&self, op: &dyn std::any::Any) -> Result<Vec<WalEntry>, StorageError>;

    /// Rebuild indexes from recovered data
    ///
    /// Called after recovery completes. Optional - only implement
    /// if this primitive has indexes.
    fn rebuild_indexes(&mut self) -> Result<(), StorageError> {
        Ok(())  // Default: no indexes
    }

    /// Get primitive name for logging/debugging
    fn name(&self) -> &'static str;
}

/// Storage errors
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Deserialization error: {0}")]
    Deserialization(String),

    #[error("Unknown entry type: {0}")]
    UnknownEntryType(u8),

    #[error("Invalid operation")]
    InvalidOperation,
}
```

### Acceptance Criteria

- [ ] Trait defines all required methods
- [ ] Clear documentation with examples
- [ ] Default implementation for optional methods
- [ ] StorageError covers all error cases

---

## Story #322: Primitive Registry Implementation

**File**: `crates/storage/src/registry.rs` (NEW)

**Deliverable**: Registry for dynamic primitive handling

### Implementation

```rust
use std::collections::HashMap;
use std::sync::Arc;

/// Registry of primitives for recovery/snapshot
///
/// Allows dynamic registration of new primitives without
/// changing core recovery/snapshot code.
pub struct PrimitiveRegistry {
    /// Type ID -> Primitive instance
    primitives: HashMap<u8, Arc<dyn PrimitiveStorageExt>>,
    /// Entry type -> Type ID mapping
    entry_type_map: HashMap<u8, u8>,
}

impl PrimitiveRegistry {
    /// Create registry with built-in primitives
    pub fn new() -> Self {
        let mut registry = PrimitiveRegistry {
            primitives: HashMap::new(),
            entry_type_map: HashMap::new(),
        };

        // Register built-in primitives
        // (In actual implementation, these would be passed in)

        registry
    }

    /// Register a primitive
    pub fn register<P: PrimitiveStorageExt + 'static>(&mut self, primitive: P) {
        let type_id = primitive.primitive_type_id();
        let entry_types = primitive.wal_entry_types();

        // Map entry types to this primitive
        for &entry_type in entry_types {
            self.entry_type_map.insert(entry_type, type_id);
        }

        self.primitives.insert(type_id, Arc::new(primitive));
    }

    /// Get primitive by type ID
    pub fn get(&self, type_id: u8) -> Option<Arc<dyn PrimitiveStorageExt>> {
        self.primitives.get(&type_id).cloned()
    }

    /// Get primitive for a WAL entry type
    pub fn for_entry_type(&self, entry_type: u8) -> Option<Arc<dyn PrimitiveStorageExt>> {
        self.entry_type_map
            .get(&entry_type)
            .and_then(|type_id| self.primitives.get(type_id))
            .cloned()
    }

    /// Get all registered primitives
    pub fn all(&self) -> Vec<Arc<dyn PrimitiveStorageExt>> {
        self.primitives.values().cloned().collect()
    }

    /// Get all type IDs
    pub fn type_ids(&self) -> Vec<u8> {
        self.primitives.keys().cloned().collect()
    }

    /// Check if an entry type is known
    pub fn knows_entry_type(&self, entry_type: u8) -> bool {
        self.entry_type_map.contains_key(&entry_type)
    }
}

impl Default for PrimitiveRegistry {
    fn default() -> Self {
        Self::new()
    }
}
```

### Acceptance Criteria

- [ ] Register primitives dynamically
- [ ] Look up by type ID
- [ ] Look up by entry type
- [ ] all() returns all registered primitives
- [ ] knows_entry_type() for unknown type handling

---

## Story #323: Extension Point Documentation

**File**: `docs/architecture/STORAGE_EXTENSION.md` (NEW)

**Deliverable**: Documentation for adding new primitives

### Implementation

```markdown
# Storage Extension Guide

## Adding a New Primitive to in-mem

This guide explains how to add a new primitive type to the database.
After M7, the storage APIs are frozen - adding a primitive only requires
implementing extension points.

## Prerequisites

1. Allocated WAL entry type range (see WAL_ENTRY_TYPES.md)
2. Allocated primitive type ID (see PRIMITIVE_IDS.md)
3. Understanding of the existing primitive pattern

## Steps

### 1. Implement PrimitiveStorageExt

```rust
impl PrimitiveStorageExt for YourPrimitive {
    fn primitive_type_id(&self) -> u8 {
        YOUR_TYPE_ID  // Allocated for you
    }

    fn wal_entry_types(&self) -> &'static [u8] {
        &[YOUR_ENTRY_TYPE_1, YOUR_ENTRY_TYPE_2]
    }

    fn snapshot_serialize(&self) -> Result<Vec<u8>, StorageError> {
        // Serialize your state
        bincode::serialize(&self.data)
            .map_err(|e| StorageError::Serialization(e.to_string()))
    }

    fn snapshot_deserialize(&mut self, data: &[u8]) -> Result<(), StorageError> {
        // Deserialize your state
        self.data = bincode::deserialize(data)
            .map_err(|e| StorageError::Deserialization(e.to_string()))?;
        Ok(())
    }

    fn apply_wal_entry(&mut self, entry: &WalEntry) -> Result<(), StorageError> {
        match entry.entry_type {
            YOUR_ENTRY_TYPE_1 => {
                // Handle operation 1
            }
            YOUR_ENTRY_TYPE_2 => {
                // Handle operation 2
            }
            _ => return Err(StorageError::UnknownEntryType(entry.entry_type)),
        }
        Ok(())
    }

    fn name(&self) -> &'static str {
        "your-primitive"
    }
}
```

### 2. Register Your Primitive

```rust
// In your primitive's module
pub fn register(registry: &mut PrimitiveRegistry) {
    registry.register(YourPrimitive::new());
}
```

### 3. Add to Database Initialization

The Database will automatically discover and register primitives
through the registry.

## What You Do NOT Need to Change

After M7, you do NOT need to modify:
- WAL core format
- Snapshot core format
- Recovery engine
- Replay engine

The extension points handle everything.

## Testing

1. Unit test your PrimitiveStorageExt implementation
2. Integration test recovery with your primitive
3. Test cross-primitive transactions
4. Verify snapshot/restore roundtrip

## Example: Vector Primitive (M8)

See `crates/primitives/src/vector.rs` for a complete example
of a primitive implemented using the extension points.
```

### Acceptance Criteria

- [ ] Clear step-by-step guide
- [ ] Code examples
- [ ] Explains what NOT to change
- [ ] Testing guidance
- [ ] Reference to example

---

## Story #324: WAL Entry Type Allocation

**File**: `docs/architecture/WAL_ENTRY_TYPES.md` (NEW)

**Deliverable**: Documentation of WAL entry type allocation

### Implementation

```markdown
# WAL Entry Type Allocation

## Overview

WAL entry types are single bytes (0x00-0xFF) that identify the type
of each entry. Ranges are allocated to prevent conflicts.

## Allocation Table

| Range | Owner | Status |
|-------|-------|--------|
| 0x00-0x0F | Core (transaction control) | FROZEN |
| 0x10-0x1F | KV Primitive | FROZEN |
| 0x20-0x2F | JSON Primitive | FROZEN |
| 0x30-0x3F | Event Primitive | FROZEN |
| 0x40-0x4F | State Primitive | FROZEN |
| 0x50-0x5F | Trace Primitive | FROZEN |
| 0x60-0x6F | Run Primitive | FROZEN |
| 0x70-0x7F | Vector Primitive (M8) | RESERVED |
| 0x80-0x8F | Reserved | AVAILABLE |
| 0x90-0x9F | Reserved | AVAILABLE |
| 0xA0-0xAF | Reserved | AVAILABLE |
| 0xB0-0xBF | Reserved | AVAILABLE |
| 0xC0-0xCF | Reserved | AVAILABLE |
| 0xD0-0xDF | Reserved | AVAILABLE |
| 0xE0-0xEF | Reserved | AVAILABLE |
| 0xF0-0xFF | Reserved (future internal) | RESERVED |

## Core Entry Types (0x00-0x0F)

| Value | Name | Description |
|-------|------|-------------|
| 0x00 | TransactionCommit | Marks transaction as committed |
| 0x01 | TransactionAbort | Marks transaction as aborted |
| 0x02 | SnapshotMarker | Records snapshot was taken |
| 0x03-0x0F | Reserved | For future core use |

## KV Entry Types (0x10-0x1F)

| Value | Name | Description |
|-------|------|-------------|
| 0x10 | KvPut | Put key-value pair |
| 0x11 | KvDelete | Delete key |
| 0x12-0x1F | Reserved | For future KV use |

## JSON Entry Types (0x20-0x2F)

| Value | Name | Description |
|-------|------|-------------|
| 0x20 | JsonCreate | Create document |
| 0x21 | JsonSet | Set document |
| 0x22 | JsonDelete | Delete document |
| 0x23 | JsonPatch | Apply patch to document |
| 0x24-0x2F | Reserved | For future JSON use |

## Event Entry Types (0x30-0x3F)

| Value | Name | Description |
|-------|------|-------------|
| 0x30 | EventAppend | Append event |
| 0x31-0x3F | Reserved | For future Event use |

## State Entry Types (0x40-0x4F)

| Value | Name | Description |
|-------|------|-------------|
| 0x40 | StateInit | Initialize state |
| 0x41 | StateSet | Set state value |
| 0x42 | StateTransition | State machine transition |
| 0x43-0x4F | Reserved | For future State use |

## Trace Entry Types (0x50-0x5F)

| Value | Name | Description |
|-------|------|-------------|
| 0x50 | TraceRecord | Record trace span |
| 0x51-0x5F | Reserved | For future Trace use |

## Run Entry Types (0x60-0x6F)

| Value | Name | Description |
|-------|------|-------------|
| 0x60 | RunCreate | Create run |
| 0x61 | RunUpdate | Update run metadata |
| 0x62 | RunEnd | End run |
| 0x63 | RunBegin | Begin run |
| 0x64-0x6F | Reserved | For future Run use |

## Vector Entry Types (0x70-0x7F) - M8

| Value | Name | Description |
|-------|------|-------------|
| 0x70 | VectorInsert | Insert vector |
| 0x71 | VectorDelete | Delete vector |
| 0x72 | VectorUpdate | Update vector |
| 0x73-0x7F | Reserved | For future Vector use |

## Requesting a Range

To request a new range for a primitive:
1. Open an issue with your primitive design
2. Specify how many entry types you need
3. We will allocate a range

## Versioning

Each entry type has a version field. This allows evolving the
payload format without changing the entry type.

Current versions:
- All core types: v1
- All primitive types: v1
```

### Acceptance Criteria

- [ ] Complete allocation table
- [ ] All existing types documented
- [ ] Vector (M8) range reserved
- [ ] Clear process for requesting ranges
- [ ] Versioning explained

---

## Story #325: Snapshot Section Format

**File**: `docs/architecture/SNAPSHOT_FORMAT.md` (NEW)

**Deliverable**: Documentation of snapshot section format

### Implementation

```markdown
# Snapshot Format Specification

## Overview

Snapshots are single-file point-in-time captures of database state.
They enable bounded recovery time by avoiding full WAL replay.

## File Format (v1)

```
+------------------+
| Magic (10 bytes) |  "INMEM_SNAP"
+------------------+
| Version (4)      |  u32, little-endian, currently 1
+------------------+
| Timestamp (8)    |  u64, microseconds since epoch
+------------------+
| WAL Offset (8)   |  u64, WAL position covered
+------------------+
| Tx Count (8)     |  u64, transactions included
+------------------+
| Prim Count (1)   |  u8, number of primitive sections
+------------------+
| Section 1        |  Primitive section
+------------------+
| Section 2        |
+------------------+
| ...              |
+------------------+
| CRC32 (4)        |  Checksum of all above
+------------------+
```

## Primitive Section Format

Each primitive section:

```
+------------------+
| Type ID (1)      |  u8, primitive type identifier
+------------------+
| Length (8)       |  u64, data length
+------------------+
| Data (variable)  |  Primitive-specific serialized data
+------------------+
```

## Primitive Type IDs

| ID | Primitive | Status |
|----|-----------|--------|
| 1 | KV | FROZEN |
| 2 | JSON | FROZEN |
| 3 | Event | FROZEN |
| 4 | State | FROZEN |
| 5 | Trace | FROZEN |
| 6 | Run | FROZEN |
| 7 | Vector | RESERVED (M8) |
| 8-255 | Future | AVAILABLE |

## Data Format per Primitive

### KV (Type 1)
```rust
// Serialized with bincode
Vec<(Key, Value)>
```

### JSON (Type 2)
```rust
// Serialized with bincode
Vec<(Key, JsonDoc)>
```

### Event (Type 3)
```rust
// Serialized with bincode
Vec<(Key, Vec<Event>)>  // Key is log key
```

### State (Type 4)
```rust
// Serialized with bincode
Vec<(Key, StateValue)>
```

### Trace (Type 5)
```rust
// Serialized with bincode
Vec<(Key, Vec<Span>)>
```

### Run (Type 6)
```rust
// Serialized with bincode
Vec<RunMetadata>
```

## CRC32

CRC32 is computed over all bytes from Magic to (but not including)
the CRC32 field itself. Uses CRC-32/ISO-HDLC polynomial.

## Validation

To validate a snapshot:
1. Check file size >= 14 bytes (magic + crc)
2. Verify magic bytes
3. Verify CRC32
4. Parse and validate header
5. Verify section lengths sum correctly

## Version Evolution

The version field allows format evolution:
- v1: Current format (M7)
- v2: Reserved for compression (M9)
- v3: Reserved for encryption (M11)

Readers should reject unknown versions.

## Adding a New Primitive

To add a primitive to snapshots:
1. Allocate a type ID (see above)
2. Implement PrimitiveStorageExt::snapshot_serialize()
3. Implement PrimitiveStorageExt::snapshot_deserialize()
4. The snapshot writer/reader will handle it automatically
```

### Acceptance Criteria

- [ ] Complete format specification
- [ ] Byte-level layout documented
- [ ] All primitive formats documented
- [ ] CRC32 specification
- [ ] Version evolution plan

---

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_primitive_registry() {
        let mut registry = PrimitiveRegistry::new();

        // Mock primitive
        struct MockPrimitive;
        impl PrimitiveStorageExt for MockPrimitive {
            fn primitive_type_id(&self) -> u8 { 99 }
            fn wal_entry_types(&self) -> &'static [u8] { &[0x99] }
            fn snapshot_serialize(&self) -> Result<Vec<u8>, StorageError> { Ok(vec![]) }
            fn snapshot_deserialize(&mut self, _: &[u8]) -> Result<(), StorageError> { Ok(()) }
            fn apply_wal_entry(&mut self, _: &WalEntry) -> Result<(), StorageError> { Ok(()) }
            fn to_wal_entries(&self, _: &dyn std::any::Any) -> Result<Vec<WalEntry>, StorageError> { Ok(vec![]) }
            fn name(&self) -> &'static str { "mock" }
        }

        registry.register(MockPrimitive);

        assert!(registry.get(99).is_some());
        assert!(registry.for_entry_type(0x99).is_some());
        assert!(registry.knows_entry_type(0x99));
        assert!(!registry.knows_entry_type(0x98));
    }

    #[test]
    fn test_unknown_entry_type_handled() {
        let registry = PrimitiveRegistry::new();

        // Unknown entry type should return None, not panic
        assert!(registry.for_entry_type(0xFF).is_none());
    }

    #[test]
    fn test_snapshot_with_new_primitive() {
        let mut registry = PrimitiveRegistry::new();
        // ... register primitives ...

        let db = create_test_db_with_registry(&registry);

        // Snapshot should include all registered primitives
        let snapshot = db.snapshot().unwrap();

        // Verify primitive sections
        let reader = SnapshotReader::read(&snapshot.path).unwrap();
        assert_eq!(reader.sections.len(), registry.all().len());
    }
}
```

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/storage/src/primitive_ext.rs` | CREATE - PrimitiveStorageExt trait |
| `crates/storage/src/registry.rs` | CREATE - PrimitiveRegistry |
| `crates/storage/src/lib.rs` | MODIFY - Export new modules |
| `docs/architecture/STORAGE_EXTENSION.md` | CREATE - Extension guide |
| `docs/architecture/WAL_ENTRY_TYPES.md` | CREATE - Entry type allocation |
| `docs/architecture/SNAPSHOT_FORMAT.md` | CREATE - Snapshot format spec |
