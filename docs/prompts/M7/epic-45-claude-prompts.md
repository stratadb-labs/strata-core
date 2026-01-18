# Epic 45: Storage Stabilization - Implementation Prompts

**Epic Goal**: Freeze storage APIs for future primitives

**GitHub Issue**: [#343](https://github.com/anibjoshi/in-mem/issues/343)
**Status**: Ready to begin (after Epic 41, 42 complete)
**Dependencies**: Epic 41 (Crash Recovery), Epic 42 (WAL Enhancement)

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M7_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M7_ARCHITECTURE.md`
2. **Epic Spec**: `docs/milestones/M7/EPIC_45_STORAGE_STABILIZATION.md`
3. **Prompt Header**: `docs/prompts/M7/M7_PROMPT_HEADER.md` for the 5 architectural rules

---

## Epic 45 Overview

### Scope
- PrimitiveStorageExt trait for new primitives
- Primitive registry for dynamic handling
- Extension point documentation
- WAL entry type allocations
- Snapshot section format documentation

### The Core Guarantee

> **Adding a primitive must NOT require changes to WAL core format, Snapshot core format, Recovery engine, or Replay engine. Only extension points.**

M8 will add the Vector primitive. After M7, adding Vector should require:
- Implementing PrimitiveStorageExt trait
- Registering in PrimitiveRegistry
- Using allocated WAL entry types (0x70-0x7F)

It should NOT require:
- Modifying WAL entry envelope format
- Modifying Snapshot envelope format
- Modifying RecoveryEngine
- Modifying ReplayEngine

### Success Criteria
- [ ] PrimitiveStorageExt trait defined and documented
- [ ] PrimitiveRegistry for dynamic primitive handling
- [ ] Clear extension point documentation
- [ ] WAL entry type ranges documented (0x70-0x7F for Vector)
- [ ] Snapshot section format documented
- [ ] Adding Vector (M8) requires NO changes to recovery engine

### Component Breakdown
- **Story #321 (GitHub #376)**: PrimitiveStorageExt Trait - FOUNDATION
- **Story #322 (GitHub #377)**: Primitive Registry Implementation - CRITICAL
- **Story #323 (GitHub #378)**: Extension Point Documentation - HIGH
- **Story #324 (GitHub #379)**: WAL Entry Type Allocation - HIGH
- **Story #325 (GitHub #380)**: Snapshot Section Format - HIGH

---

## Dependency Graph

```
Story #376 (Trait) ──> Story #377 (Registry) ──> Story #378 (Documentation)
                                │
                                v
                        Story #379 (WAL Types)
                                │
                                v
                        Story #380 (Snapshot Format)
```

---

## Story #376: PrimitiveStorageExt Trait

**GitHub Issue**: [#376](https://github.com/anibjoshi/in-mem/issues/376)
**Estimated Time**: 3 hours
**Dependencies**: None
**Blocks**: Story #377

### Start Story

```bash
gh issue view 376
./scripts/start-story.sh 45 376 primitive-ext-trait
```

### Implementation

Create `crates/storage/src/primitive_ext.rs`:

```rust
//! Extension trait for primitives to integrate with storage
//!
//! This trait must be implemented by any new primitive to participate in:
//! - WAL entry processing
//! - Snapshot serialization/deserialization
//! - Recovery
//!
//! After M7, adding a new primitive requires ONLY:
//! 1. Implementing this trait
//! 2. Registering in PrimitiveRegistry
//! 3. Using allocated WAL entry types
//!
//! NO changes to WAL format, Snapshot format, or Recovery engine required.

use crate::wal_types::{WalEntry, WalError};
use crate::snapshot_types::SnapshotError;

/// Extension trait for primitives to integrate with storage
///
/// # Example: Implementing for M8 Vector Primitive
///
/// ```rust
/// impl PrimitiveStorageExt for VectorStore {
///     fn wal_entry_types(&self) -> &'static [u8] {
///         &[0x70, 0x71, 0x72]  // VectorInsert, VectorDelete, VectorUpdate
///     }
///
///     fn snapshot_serialize(&self) -> Result<Vec<u8>, SnapshotError> {
///         bincode::serialize(&self.vectors)
///             .map_err(|e| SnapshotError::Serialize(e.to_string()))
///     }
///
///     fn snapshot_deserialize(&mut self, data: &[u8]) -> Result<(), SnapshotError> {
///         self.vectors = bincode::deserialize(data)
///             .map_err(|e| SnapshotError::Serialize(e.to_string()))?;
///         Ok(())
///     }
///
///     fn apply_wal_entry(&mut self, entry: &WalEntry) -> Result<(), WalError> {
///         match entry.entry_type as u8 {
///             0x70 => self.apply_insert(entry),
///             0x71 => self.apply_delete(entry),
///             0x72 => self.apply_update(entry),
///             _ => Ok(())  // Unknown type - skip
///         }
///     }
///
///     fn primitive_type_id(&self) -> u8 {
///         7  // After existing 6 primitives
///     }
///
///     fn primitive_name(&self) -> &'static str {
///         "vector"
///     }
/// }
/// ```
pub trait PrimitiveStorageExt: Send + Sync {
    /// WAL entry types this primitive uses (from its allocated range)
    ///
    /// Ranges:
    /// - 0x10-0x1F: KV
    /// - 0x20-0x2F: JSON
    /// - 0x30-0x3F: Event
    /// - 0x40-0x4F: State
    /// - 0x50-0x5F: Trace
    /// - 0x60-0x6F: Run
    /// - 0x70-0x7F: Vector (M8)
    /// - 0x80-0xFF: Future
    fn wal_entry_types(&self) -> &'static [u8];

    /// Serialize primitive state for snapshot
    ///
    /// Should serialize all data needed to reconstruct the primitive.
    /// Do NOT include derived data (indexes) - those are rebuilt.
    fn snapshot_serialize(&self) -> Result<Vec<u8>, SnapshotError>;

    /// Deserialize primitive state from snapshot
    ///
    /// Reconstruct primitive state from serialized bytes.
    /// Indexes will be rebuilt separately by recovery engine.
    fn snapshot_deserialize(&mut self, data: &[u8]) -> Result<(), SnapshotError>;

    /// Apply a WAL entry during recovery
    ///
    /// Called for each WAL entry with a type in wal_entry_types().
    /// Should apply the entry's effect to in-memory state.
    fn apply_wal_entry(&mut self, entry: &WalEntry) -> Result<(), WalError>;

    /// Create a WAL entry for an operation
    ///
    /// Each primitive operation should produce a WAL entry.
    fn to_wal_entry(&self, op: &dyn std::any::Any) -> Option<WalEntry> {
        None  // Default: no WAL entry (override for operations that need durability)
    }

    /// Primitive type ID (for snapshot sections)
    ///
    /// IDs:
    /// - 1: KV
    /// - 2: JSON
    /// - 3: Event
    /// - 4: State
    /// - 5: Trace
    /// - 6: Run
    /// - 7: Vector (M8)
    fn primitive_type_id(&self) -> u8;

    /// Primitive name (for logging/debugging)
    fn primitive_name(&self) -> &'static str;

    /// Rebuild indexes after recovery
    ///
    /// Called after all WAL entries are applied.
    /// Override if primitive has indexes that need rebuilding.
    fn rebuild_indexes(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        Ok(())  // Default: no indexes
    }
}
```

### Acceptance Criteria

- [ ] Trait has all required methods
- [ ] Documentation includes example for Vector
- [ ] WAL entry type ranges documented
- [ ] Primitive type IDs documented

### Complete Story

```bash
./scripts/complete-story.sh 376
```

---

## Story #377: Primitive Registry Implementation

**GitHub Issue**: [#377](https://github.com/anibjoshi/in-mem/issues/377)
**Estimated Time**: 3 hours
**Dependencies**: Story #376

### Start Story

```bash
gh issue view 377
./scripts/start-story.sh 45 377 primitive-registry
```

### Implementation

```rust
//! Primitive registry for dynamic primitive handling
//!
//! The registry allows new primitives to be added without modifying
//! the recovery engine or snapshot format.

use std::collections::HashMap;
use std::sync::Arc;

/// Registry of primitives for recovery/snapshot
pub struct PrimitiveRegistry {
    /// Primitives by type ID
    primitives: HashMap<u8, Arc<dyn PrimitiveStorageExt>>,
    /// WAL entry type to primitive mapping
    wal_type_to_primitive: HashMap<u8, u8>,
}

impl PrimitiveRegistry {
    /// Create new registry with built-in primitives
    pub fn new() -> Self {
        let mut registry = PrimitiveRegistry {
            primitives: HashMap::new(),
            wal_type_to_primitive: HashMap::new(),
        };

        // Register built-in primitives
        // These are registered by the database on construction

        registry
    }

    /// Register a primitive
    pub fn register(&mut self, primitive: Arc<dyn PrimitiveStorageExt>) {
        let type_id = primitive.primitive_type_id();

        // Map WAL entry types to this primitive
        for &wal_type in primitive.wal_entry_types() {
            self.wal_type_to_primitive.insert(wal_type, type_id);
        }

        self.primitives.insert(type_id, primitive);
    }

    /// Get primitive by type ID
    pub fn get(&self, type_id: u8) -> Option<Arc<dyn PrimitiveStorageExt>> {
        self.primitives.get(&type_id).cloned()
    }

    /// Get primitive for a WAL entry type
    pub fn get_for_wal_type(&self, wal_type: u8) -> Option<Arc<dyn PrimitiveStorageExt>> {
        self.wal_type_to_primitive
            .get(&wal_type)
            .and_then(|&type_id| self.primitives.get(&type_id))
            .cloned()
    }

    /// List all registered primitives
    pub fn list(&self) -> Vec<&dyn PrimitiveStorageExt> {
        self.primitives
            .values()
            .map(|p| p.as_ref())
            .collect()
    }

    /// Check if a primitive type is registered
    pub fn is_registered(&self, type_id: u8) -> bool {
        self.primitives.contains_key(&type_id)
    }

    /// Get all type IDs
    pub fn type_ids(&self) -> Vec<u8> {
        self.primitives.keys().copied().collect()
    }
}

impl Default for PrimitiveRegistry {
    fn default() -> Self {
        Self::new()
    }
}
```

### Usage in Recovery

```rust
impl RecoveryEngine {
    /// Apply WAL entry using registry
    fn apply_wal_entry_via_registry(
        registry: &PrimitiveRegistry,
        entry: &WalEntry,
    ) -> Result<(), RecoveryError> {
        // Get primitive for this entry type
        let wal_type = entry.entry_type as u8;

        if let Some(primitive) = registry.get_for_wal_type(wal_type) {
            // Primitive handles its own entry
            // This is done via the database's primitives, not via Arc
            Ok(())
        } else {
            // Unknown entry type - log warning but continue
            tracing::warn!("Unknown WAL entry type: 0x{:02x}", wal_type);
            Ok(())
        }
    }
}
```

### Acceptance Criteria

- [ ] Registry stores primitives by type ID
- [ ] WAL type to primitive mapping
- [ ] get_for_wal_type works correctly
- [ ] Unknown types handled gracefully

### Complete Story

```bash
./scripts/complete-story.sh 377
```

---

## Story #378: Extension Point Documentation

**GitHub Issue**: [#378](https://github.com/anibjoshi/in-mem/issues/378)
**Estimated Time**: 2 hours
**Dependencies**: Story #377

### Start Story

```bash
gh issue view 378
./scripts/start-story.sh 45 378 extension-docs
```

### Implementation

Create `docs/architecture/STORAGE_EXTENSION_GUIDE.md`:

```markdown
# Storage Extension Guide: Adding New Primitives

## Overview

After M7, the storage layer is stable. Adding a new primitive (like Vector in M8)
requires implementing specific extension points, but NO changes to:

- WAL entry envelope format
- Snapshot envelope format
- Recovery engine core logic
- Replay engine core logic

## Steps to Add a New Primitive

### 1. Allocate WAL Entry Types

Each primitive gets a 16-byte range of WAL entry types:

| Primitive | Range | Example Types |
|-----------|-------|---------------|
| KV | 0x10-0x1F | Put=0x10, Delete=0x11 |
| JSON | 0x20-0x2F | Create=0x20, Set=0x21, Delete=0x22, Patch=0x23 |
| Event | 0x30-0x3F | Append=0x30 |
| State | 0x40-0x4F | Init=0x40, Set=0x41, Transition=0x42 |
| Trace | 0x50-0x5F | Record=0x50 |
| Run | 0x60-0x6F | Create=0x60, Update=0x61, End=0x62, Begin=0x63 |
| **Vector** | **0x70-0x7F** | Insert=0x70, Delete=0x71, Update=0x72 |
| Future | 0x80-0xFF | Reserved |

### 2. Assign Primitive Type ID

Each primitive has a unique type ID for snapshot sections:

| Primitive | Type ID |
|-----------|---------|
| KV | 1 |
| JSON | 2 |
| Event | 3 |
| State | 4 |
| Trace | 5 |
| Run | 6 |
| **Vector** | **7** |

### 3. Implement PrimitiveStorageExt

```rust
impl PrimitiveStorageExt for VectorStore {
    fn wal_entry_types(&self) -> &'static [u8] {
        &[0x70, 0x71, 0x72]
    }

    fn snapshot_serialize(&self) -> Result<Vec<u8>, SnapshotError> {
        // Serialize vector data (not HNSW index - that's derived)
        bincode::serialize(&self.vectors)
            .map_err(|e| SnapshotError::Serialize(e.to_string()))
    }

    fn snapshot_deserialize(&mut self, data: &[u8]) -> Result<(), SnapshotError> {
        self.vectors = bincode::deserialize(data)?;
        Ok(())
    }

    fn apply_wal_entry(&mut self, entry: &WalEntry) -> Result<(), WalError> {
        match entry.entry_type as u8 {
            0x70 => { /* VectorInsert */ }
            0x71 => { /* VectorDelete */ }
            0x72 => { /* VectorUpdate */ }
            _ => {}
        }
        Ok(())
    }

    fn primitive_type_id(&self) -> u8 { 7 }

    fn primitive_name(&self) -> &'static str { "vector" }

    fn rebuild_indexes(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Rebuild HNSW index from vectors
        self.rebuild_hnsw_index()?;
        Ok(())
    }
}
```

### 4. Register in Database

```rust
impl Database {
    fn new() -> Self {
        let mut db = Database { ... };

        // Register primitives
        db.registry.register(Arc::new(db.kv.storage_ext()));
        db.registry.register(Arc::new(db.json.storage_ext()));
        // ... other primitives ...
        db.registry.register(Arc::new(db.vector.storage_ext()));  // M8

        db
    }
}
```

### 5. Add WAL Entry Types

```rust
// In wal_entry_types.rs
#[repr(u8)]
pub enum WalEntryType {
    // ... existing types ...

    // Vector (M8)
    VectorInsert = 0x70,
    VectorDelete = 0x71,
    VectorUpdate = 0x72,
}
```

## What NOT to Modify

When adding a new primitive, you should NOT need to modify:

- `WalEntry::serialize()` or `WalEntry::deserialize()`
- `SnapshotWriter::write()` or `SnapshotReader::read()`
- `RecoveryEngine::recover()`
- `ReplayEngine::replay()`

If you find yourself modifying these, stop and reconsider.

## Testing New Primitives

1. Unit tests for serialize/deserialize
2. WAL entry roundtrip tests
3. Recovery integration tests
4. Cross-primitive transaction tests
```

### Acceptance Criteria

- [ ] Clear step-by-step guide
- [ ] WAL entry type allocations documented
- [ ] Primitive type IDs documented
- [ ] Example implementation shown
- [ ] "What NOT to modify" section

### Complete Story

```bash
./scripts/complete-story.sh 378
```

---

## Story #379: WAL Entry Type Allocation

**GitHub Issue**: [#379](https://github.com/anibjoshi/in-mem/issues/379)
**Estimated Time**: 2 hours
**Dependencies**: Story #376

### Start Story

```bash
gh issue view 379
./scripts/start-story.sh 45 379 wal-type-alloc
```

### Implementation

Add constants and documentation to `wal_entry_types.rs`:

```rust
/// WAL entry type ranges
///
/// Each primitive is allocated a 16-byte range for its entry types.
/// This allows up to 16 different operations per primitive.
pub mod wal_ranges {
    /// Core transaction control (0x00-0x0F)
    pub const CORE_START: u8 = 0x00;
    pub const CORE_END: u8 = 0x0F;

    /// KV primitive (0x10-0x1F)
    pub const KV_START: u8 = 0x10;
    pub const KV_END: u8 = 0x1F;

    /// JSON primitive (0x20-0x2F)
    pub const JSON_START: u8 = 0x20;
    pub const JSON_END: u8 = 0x2F;

    /// Event primitive (0x30-0x3F)
    pub const EVENT_START: u8 = 0x30;
    pub const EVENT_END: u8 = 0x3F;

    /// State primitive (0x40-0x4F)
    pub const STATE_START: u8 = 0x40;
    pub const STATE_END: u8 = 0x4F;

    /// Trace primitive (0x50-0x5F)
    pub const TRACE_START: u8 = 0x50;
    pub const TRACE_END: u8 = 0x5F;

    /// Run primitive (0x60-0x6F)
    pub const RUN_START: u8 = 0x60;
    pub const RUN_END: u8 = 0x6F;

    /// Vector primitive - RESERVED for M8 (0x70-0x7F)
    pub const VECTOR_START: u8 = 0x70;
    pub const VECTOR_END: u8 = 0x7F;

    /// Future primitives (0x80-0xFF)
    pub const FUTURE_START: u8 = 0x80;
    pub const FUTURE_END: u8 = 0xFF;
}

/// Check which primitive a WAL entry type belongs to
pub fn primitive_for_wal_type(wal_type: u8) -> Option<&'static str> {
    use wal_ranges::*;
    match wal_type {
        CORE_START..=CORE_END => Some("core"),
        KV_START..=KV_END => Some("kv"),
        JSON_START..=JSON_END => Some("json"),
        EVENT_START..=EVENT_END => Some("event"),
        STATE_START..=STATE_END => Some("state"),
        TRACE_START..=TRACE_END => Some("trace"),
        RUN_START..=RUN_END => Some("run"),
        VECTOR_START..=VECTOR_END => Some("vector"),
        FUTURE_START..=FUTURE_END => Some("future"),
        _ => None,
    }
}
```

### Acceptance Criteria

- [ ] Range constants defined
- [ ] 0x70-0x7F reserved for Vector
- [ ] 0x80-0xFF reserved for future
- [ ] Helper function for range lookup

### Complete Story

```bash
./scripts/complete-story.sh 379
```

---

## Story #380: Snapshot Section Format

**GitHub Issue**: [#380](https://github.com/anibjoshi/in-mem/issues/380)
**Estimated Time**: 2 hours
**Dependencies**: Story #376

### Start Story

```bash
gh issue view 380
./scripts/start-story.sh 45 380 snapshot-section
```

### Implementation

Add documentation for snapshot section format:

```rust
//! Snapshot section format
//!
//! Each primitive's data in a snapshot follows this format:
//!
//! +------------------+
//! | Type (1 byte)    |  Primitive type ID
//! +------------------+
//! | Length (8 bytes) |  Data length in bytes
//! +------------------+
//! | Data (variable)  |  Serialized primitive state
//! +------------------+
//!
//! The primitive is responsible for serializing its own state.
//! Indexes are NOT included - they are rebuilt during recovery.

/// Primitive type IDs for snapshot sections
pub mod snapshot_primitive_ids {
    /// KV Store
    pub const KV: u8 = 1;
    /// JSON Store
    pub const JSON: u8 = 2;
    /// Event Log
    pub const EVENT: u8 = 3;
    /// State Cell
    pub const STATE: u8 = 4;
    /// Trace Store
    pub const TRACE: u8 = 5;
    /// Run Index
    pub const RUN: u8 = 6;
    /// Vector Store (M8)
    pub const VECTOR: u8 = 7;
}

/// Section header in snapshot
#[derive(Debug, Clone)]
pub struct SnapshotSectionHeader {
    /// Primitive type ID
    pub primitive_type: u8,
    /// Data length in bytes
    pub length: u64,
}

impl SnapshotSectionHeader {
    pub const SIZE: usize = 9;  // 1 + 8 bytes

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        buf[0] = self.primitive_type;
        buf[1..9].copy_from_slice(&self.length.to_le_bytes());
        buf
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, SnapshotError> {
        if data.len() < Self::SIZE {
            return Err(SnapshotError::TooShort);
        }
        Ok(SnapshotSectionHeader {
            primitive_type: data[0],
            length: u64::from_le_bytes(data[1..9].try_into().unwrap()),
        })
    }
}
```

### Acceptance Criteria

- [ ] Section format documented
- [ ] Type IDs defined (including Vector)
- [ ] Header serialization/deserialization
- [ ] Clear that indexes are NOT included

### Complete Story

```bash
./scripts/complete-story.sh 380
```

---

## Epic 45 Completion Checklist

### 1. Final Validation

```bash
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace -- -D warnings
~/.cargo/bin/cargo fmt --check
~/.cargo/bin/cargo doc --workspace --no-deps
```

### 2. Verify Extension Points

- [ ] PrimitiveStorageExt trait complete
- [ ] PrimitiveRegistry working
- [ ] Documentation complete
- [ ] WAL type ranges allocated
- [ ] Snapshot section format documented

### 3. Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-45-storage-stabilization -m "Epic 45: Storage Stabilization complete

Delivered:
- PrimitiveStorageExt trait
- PrimitiveRegistry implementation
- Extension point documentation
- WAL entry type allocation
- Snapshot section format

M8 Vector primitive can be added without modifying core storage.

Stories: #376, #377, #378, #379, #380
"
git push origin develop
gh issue close 343 --comment "Epic 45: Storage Stabilization - COMPLETE"
```

---

## Summary

Epic 45 freezes the storage APIs so that future primitives (like Vector in M8) can be added by implementing extension points only. No changes to WAL format, Snapshot format, or Recovery engine required.
