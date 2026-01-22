# Epic 55: Transaction & Durability - Implementation Prompts

**Epic Goal**: Integrate with transaction system and M7 durability

**GitHub Issue**: [#393](https://github.com/anibjoshi/in-mem/issues/393)
**Status**: Ready after Epic 51 and Epic 52
**Dependencies**: Epic 51 (Vector Heap), Epic 52 (Index Backend)

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M8_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

### IMPORTANT: Naming Convention

**Do NOT use "M8" or "m8" in the codebase or comments.** M8 is an internal milestone indicator only. In code, use "Vector" prefix instead:
- Module names: `vector_wal`, `vector_snapshot`, `vector_recovery`
- Type names: `WalVectorUpsert`, `VectorCollectionSnapshot`
- WAL constants: `WAL_VECTOR_UPSERT`, not `WAL_M8_UPSERT`
- Test names: `test_vector_*`, not `test_m8_*`
- Comments: "Vector WAL" not "M8 WAL"

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M8_ARCHITECTURE.md`
2. **Epic Spec**: `docs/milestones/M8/EPIC_55_TRANSACTION_DURABILITY.md`
3. **Prompt Header**: `docs/prompts/M8/M8_PROMPT_HEADER.md` for the 7 architectural rules
4. **M7 Architecture**: `docs/architecture/M7_ARCHITECTURE.md` for durability patterns

---

## Epic 55 Overview

### Scope
- Vector WAL entry types (0x70-0x73)
- WAL write and replay for vector operations
- Snapshot serialization with next_id and free_slots
- Recovery from snapshot + WAL
- Cross-primitive transaction tests

### Critical Invariants

| Invariant | Description |
|-----------|-------------|
| **S8** | Snapshot-WAL equivalence: snapshot + WAL replay = pure WAL replay |
| **S9** | Heap-KV reconstructibility: both can be rebuilt from snapshot + WAL |
| **T4** | VectorId monotonicity across crashes: new ID > all previous IDs |

### WAL Entry Types

```rust
pub const WAL_VECTOR_COLLECTION_CREATE: u8 = 0x70;
pub const WAL_VECTOR_COLLECTION_DELETE: u8 = 0x71;
pub const WAL_VECTOR_UPSERT: u8 = 0x72;
pub const WAL_VECTOR_DELETE: u8 = 0x73;
```

### Component Breakdown
- **Story #421**: Vector WAL Entry Types - CRITICAL
- **Story #422**: Vector WAL Write and Replay - CRITICAL
- **Story #423**: Vector Snapshot Serialization - CRITICAL
- **Story #424**: Vector Recovery Implementation - CRITICAL
- **Story #425**: Cross-Primitive Transaction Tests - HIGH

---

## Story #421: Vector WAL Entry Types

**GitHub Issue**: [#421](https://github.com/anibjoshi/in-mem/issues/421)
**Estimated Time**: 1.5 hours
**Dependencies**: None
**Blocks**: #422

### Start Story

```bash
gh issue view 421
./scripts/start-story.sh 55 421 wal-entry-types
```

### Implementation

Modify `crates/durability/src/wal_types.rs`:

```rust
/// Vector WAL entry types (0x70-0x7F range)
///
/// Naming convention:
/// - COLLECTION_CREATE/DELETE: prefixed to distinguish from vector-level ops
/// - UPSERT (not INSERT): matches our semantic (insert overwrites if exists)
pub const WAL_VECTOR_COLLECTION_CREATE: u8 = 0x70;
pub const WAL_VECTOR_COLLECTION_DELETE: u8 = 0x71;
pub const WAL_VECTOR_UPSERT: u8 = 0x72;
pub const WAL_VECTOR_DELETE: u8 = 0x73;

/// WAL entry type enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WalEntryType {
    // Core (0x00-0x0F)
    TransactionCommit,
    TransactionAbort,
    SnapshotMarker,

    // KV (0x10-0x1F)
    KvPut,
    KvDelete,

    // JSON (0x20-0x2F)
    JsonCreate,
    JsonSet,
    JsonDelete,
    JsonPatch,

    // Event (0x30-0x3F)
    EventAppend,

    // State (0x40-0x4F)
    StateInit,
    StateSet,
    StateTransition,

    // Trace (0x50-0x5F)
    TraceRecord,

    // Run (0x60-0x6F)
    RunCreate,
    RunUpdate,
    RunEnd,
    RunBegin,

    // Vector (0x70-0x7F) - M8
    VectorCollectionCreate,
    VectorCollectionDelete,
    VectorUpsert,
    VectorDelete,
}

impl WalEntryType {
    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            // Core
            0x00 => Some(WalEntryType::TransactionCommit),
            0x01 => Some(WalEntryType::TransactionAbort),
            0x02 => Some(WalEntryType::SnapshotMarker),

            // KV
            0x10 => Some(WalEntryType::KvPut),
            0x11 => Some(WalEntryType::KvDelete),

            // JSON
            0x20 => Some(WalEntryType::JsonCreate),
            0x21 => Some(WalEntryType::JsonSet),
            0x22 => Some(WalEntryType::JsonDelete),
            0x23 => Some(WalEntryType::JsonPatch),

            // Event
            0x30 => Some(WalEntryType::EventAppend),

            // State
            0x40 => Some(WalEntryType::StateInit),
            0x41 => Some(WalEntryType::StateSet),
            0x42 => Some(WalEntryType::StateTransition),

            // Trace
            0x50 => Some(WalEntryType::TraceRecord),

            // Run
            0x60 => Some(WalEntryType::RunCreate),
            0x61 => Some(WalEntryType::RunUpdate),
            0x62 => Some(WalEntryType::RunEnd),
            0x63 => Some(WalEntryType::RunBegin),

            // Vector (M8)
            0x70 => Some(WalEntryType::VectorCollectionCreate),
            0x71 => Some(WalEntryType::VectorCollectionDelete),
            0x72 => Some(WalEntryType::VectorUpsert),
            0x73 => Some(WalEntryType::VectorDelete),

            _ => None,
        }
    }

    pub fn to_byte(&self) -> u8 {
        match self {
            // Core
            WalEntryType::TransactionCommit => 0x00,
            WalEntryType::TransactionAbort => 0x01,
            WalEntryType::SnapshotMarker => 0x02,

            // KV
            WalEntryType::KvPut => 0x10,
            WalEntryType::KvDelete => 0x11,

            // JSON
            WalEntryType::JsonCreate => 0x20,
            WalEntryType::JsonSet => 0x21,
            WalEntryType::JsonDelete => 0x22,
            WalEntryType::JsonPatch => 0x23,

            // Event
            WalEntryType::EventAppend => 0x30,

            // State
            WalEntryType::StateInit => 0x40,
            WalEntryType::StateSet => 0x41,
            WalEntryType::StateTransition => 0x42,

            // Trace
            WalEntryType::TraceRecord => 0x50,

            // Run
            WalEntryType::RunCreate => 0x60,
            WalEntryType::RunUpdate => 0x61,
            WalEntryType::RunEnd => 0x62,
            WalEntryType::RunBegin => 0x63,

            // Vector (M8)
            WalEntryType::VectorCollectionCreate => 0x70,
            WalEntryType::VectorCollectionDelete => 0x71,
            WalEntryType::VectorUpsert => 0x72,
            WalEntryType::VectorDelete => 0x73,
        }
    }

    pub fn primitive_kind(&self) -> Option<PrimitiveKind> {
        match self {
            WalEntryType::TransactionCommit |
            WalEntryType::TransactionAbort |
            WalEntryType::SnapshotMarker => None,

            WalEntryType::KvPut | WalEntryType::KvDelete => Some(PrimitiveKind::Kv),

            WalEntryType::JsonCreate |
            WalEntryType::JsonSet |
            WalEntryType::JsonDelete |
            WalEntryType::JsonPatch => Some(PrimitiveKind::Json),

            WalEntryType::EventAppend => Some(PrimitiveKind::Event),

            WalEntryType::StateInit |
            WalEntryType::StateSet |
            WalEntryType::StateTransition => Some(PrimitiveKind::State),

            WalEntryType::TraceRecord => Some(PrimitiveKind::Trace),

            WalEntryType::RunCreate |
            WalEntryType::RunUpdate |
            WalEntryType::RunEnd |
            WalEntryType::RunBegin => Some(PrimitiveKind::Run),

            WalEntryType::VectorCollectionCreate |
            WalEntryType::VectorCollectionDelete |
            WalEntryType::VectorUpsert |
            WalEntryType::VectorDelete => Some(PrimitiveKind::Vector),
        }
    }
}

/// Primitive kinds for routing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimitiveKind {
    Kv,
    Json,
    Event,
    State,
    Trace,
    Run,
    Vector,  // M8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vector_wal_types_roundtrip() {
        let types = [
            WalEntryType::VectorCollectionCreate,
            WalEntryType::VectorCollectionDelete,
            WalEntryType::VectorUpsert,
            WalEntryType::VectorDelete,
        ];

        for t in types {
            let byte = t.to_byte();
            let parsed = WalEntryType::from_byte(byte).unwrap();
            assert_eq!(t, parsed);
        }
    }

    #[test]
    fn test_vector_primitive_kind() {
        assert_eq!(
            WalEntryType::VectorUpsert.primitive_kind(),
            Some(PrimitiveKind::Vector)
        );
    }
}
```

### Acceptance Criteria

- [ ] Four entry types: COLLECTION_CREATE, COLLECTION_DELETE, UPSERT, DELETE
- [ ] Byte values in 0x70-0x73 range
- [ ] `from_byte()` and `to_byte()` conversions
- [ ] `primitive_kind()` returns Vector for all
- [ ] Add PrimitiveKind::Vector variant

### Complete Story

```bash
~/.cargo/bin/cargo test --workspace
./scripts/complete-story.sh 421
```

---

## Story #422: Vector WAL Write and Replay

**GitHub Issue**: [#422](https://github.com/anibjoshi/in-mem/issues/422)
**Estimated Time**: 3 hours
**Dependencies**: #421
**Blocks**: #424

### Start Story

```bash
gh issue view 422
./scripts/start-story.sh 55 422 wal-write-replay
```

### Implementation

Create `crates/durability/src/vector_wal.rs`:

```rust
//! WAL payloads and replay for Vector primitive

use crate::vector::{VectorConfig, VectorError, VectorResult, VectorId, DistanceMetric};
use crate::core::RunId;

/// WAL payload for COLLECTION_CREATE
#[derive(Debug, Clone)]
pub struct WalVectorCollectionCreate {
    pub run_id: RunId,
    pub name: String,
    pub dimension: u32,
    pub metric: u8,
    pub created_at: u64,
}

impl WalVectorCollectionCreate {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        // run_id (16 bytes for UUID)
        buf.extend_from_slice(self.run_id.as_bytes());

        // name (length-prefixed)
        let name_bytes = self.name.as_bytes();
        buf.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        buf.extend_from_slice(name_bytes);

        // dimension (4 bytes)
        buf.extend_from_slice(&self.dimension.to_le_bytes());

        // metric (1 byte)
        buf.push(self.metric);

        // created_at (8 bytes)
        buf.extend_from_slice(&self.created_at.to_le_bytes());

        buf
    }

    pub fn from_bytes(bytes: &[u8]) -> VectorResult<Self> {
        // Parse implementation
        todo!("Implement parsing")
    }
}

/// WAL payload for COLLECTION_DELETE
#[derive(Debug, Clone)]
pub struct WalVectorCollectionDelete {
    pub run_id: RunId,
    pub name: String,
}

impl WalVectorCollectionDelete {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(self.run_id.as_bytes());
        let name_bytes = self.name.as_bytes();
        buf.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        buf.extend_from_slice(name_bytes);
        buf
    }

    pub fn from_bytes(bytes: &[u8]) -> VectorResult<Self> {
        todo!("Implement parsing")
    }
}

/// WAL payload for UPSERT
///
/// Includes full embedding data. This is intentionally verbose for M8;
/// M9 may optimize with delta encoding or external storage.
#[derive(Debug, Clone)]
pub struct WalVectorUpsert {
    pub run_id: RunId,
    pub collection: String,
    pub key: String,
    pub vector_id: u64,  // CRITICAL: needed for deterministic replay
    pub embedding: Vec<f32>,
    pub metadata: Option<Vec<u8>>,  // Serialized JSON
}

impl WalVectorUpsert {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        // run_id
        buf.extend_from_slice(self.run_id.as_bytes());

        // collection (length-prefixed)
        let coll_bytes = self.collection.as_bytes();
        buf.extend_from_slice(&(coll_bytes.len() as u16).to_le_bytes());
        buf.extend_from_slice(coll_bytes);

        // key (length-prefixed)
        let key_bytes = self.key.as_bytes();
        buf.extend_from_slice(&(key_bytes.len() as u16).to_le_bytes());
        buf.extend_from_slice(key_bytes);

        // vector_id (8 bytes)
        buf.extend_from_slice(&self.vector_id.to_le_bytes());

        // embedding (count + raw f32s)
        buf.extend_from_slice(&(self.embedding.len() as u32).to_le_bytes());
        for v in &self.embedding {
            buf.extend_from_slice(&v.to_le_bytes());
        }

        // metadata (has_metadata + optional data)
        if let Some(ref meta) = self.metadata {
            buf.push(1);
            buf.extend_from_slice(&(meta.len() as u32).to_le_bytes());
            buf.extend_from_slice(meta);
        } else {
            buf.push(0);
        }

        buf
    }

    pub fn from_bytes(bytes: &[u8]) -> VectorResult<Self> {
        todo!("Implement parsing")
    }
}

/// WAL payload for DELETE
#[derive(Debug, Clone)]
pub struct WalVectorDelete {
    pub run_id: RunId,
    pub collection: String,
    pub key: String,
}

impl WalVectorDelete {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(self.run_id.as_bytes());

        let coll_bytes = self.collection.as_bytes();
        buf.extend_from_slice(&(coll_bytes.len() as u16).to_le_bytes());
        buf.extend_from_slice(coll_bytes);

        let key_bytes = self.key.as_bytes();
        buf.extend_from_slice(&(key_bytes.len() as u16).to_le_bytes());
        buf.extend_from_slice(key_bytes);

        buf
    }

    pub fn from_bytes(bytes: &[u8]) -> VectorResult<Self> {
        todo!("Implement parsing")
    }
}

/// WAL replay handler for Vector primitive
pub struct VectorWalHandler {
    // Reference to VectorStore or Database
}

impl VectorWalHandler {
    /// Replay a WAL entry during recovery
    ///
    /// CRITICAL: Replay must be deterministic. The vector_id in UPSERT
    /// entries must be used exactly as written, not allocated fresh.
    pub fn replay(&mut self, entry_type: WalEntryType, payload: &[u8]) -> VectorResult<()> {
        match entry_type {
            WalEntryType::VectorCollectionCreate => {
                let data = WalVectorCollectionCreate::from_bytes(payload)?;
                // Recreate collection with exact config
                self.replay_collection_create(data)
            }
            WalEntryType::VectorCollectionDelete => {
                let data = WalVectorCollectionDelete::from_bytes(payload)?;
                self.replay_collection_delete(data)
            }
            WalEntryType::VectorUpsert => {
                let data = WalVectorUpsert::from_bytes(payload)?;
                // CRITICAL: Use data.vector_id exactly
                self.replay_upsert(data)
            }
            WalEntryType::VectorDelete => {
                let data = WalVectorDelete::from_bytes(payload)?;
                self.replay_delete(data)
            }
            _ => Err(VectorError::Internal("Not a vector WAL entry".into())),
        }
    }

    fn replay_collection_create(&mut self, data: WalVectorCollectionCreate) -> VectorResult<()> {
        todo!("Implement collection creation replay")
    }

    fn replay_collection_delete(&mut self, data: WalVectorCollectionDelete) -> VectorResult<()> {
        todo!("Implement collection deletion replay")
    }

    fn replay_upsert(&mut self, data: WalVectorUpsert) -> VectorResult<()> {
        // CRITICAL: Must use data.vector_id, not allocate new
        todo!("Implement upsert replay with exact vector_id")
    }

    fn replay_delete(&mut self, data: WalVectorDelete) -> VectorResult<()> {
        todo!("Implement delete replay")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_upsert_roundtrip() {
        let upsert = WalVectorUpsert {
            run_id: RunId::new(),
            collection: "test".to_string(),
            key: "doc1".to_string(),
            vector_id: 42,
            embedding: vec![1.0, 2.0, 3.0, 4.0],
            metadata: Some(b"{}".to_vec()),
        };

        let bytes = upsert.to_bytes();
        // TODO: Test roundtrip when from_bytes is implemented
        assert!(!bytes.is_empty());
    }
}
```

### Acceptance Criteria

- [ ] Serializable payload structs for each entry type
- [ ] UPSERT payload includes vector_id for deterministic replay
- [ ] replay() handles all entry types
- [ ] Compact binary serialization
- [ ] Round-trip tests for all payloads

### Complete Story

```bash
~/.cargo/bin/cargo test --workspace
./scripts/complete-story.sh 422
```

---

## Story #423: Vector Snapshot Serialization

**GitHub Issue**: [#423](https://github.com/anibjoshi/in-mem/issues/423)
**Estimated Time**: 3 hours
**Dependencies**: Epic 51
**Blocks**: #424

### Start Story

```bash
gh issue view 423
./scripts/start-story.sh 55 423 snapshot-serialization
```

### Implementation

Create `crates/durability/src/vector_snapshot.rs`:

```rust
//! Snapshot serialization for Vector primitive
//!
//! CRITICAL: next_id and free_slots MUST be persisted!
//! Without these, recovery breaks VectorId monotonicity (T4).

use crate::vector::{VectorConfig, VectorError, VectorResult, VectorId, DistanceMetric};

/// Snapshot format version
pub const VECTOR_SNAPSHOT_VERSION: u8 = 0x01;

/// Vector collection snapshot format
///
/// CRITICAL: next_id and free_slots MUST be persisted!
/// Without these, recovery would break VectorId monotonicity (T4).
#[derive(Debug, Clone)]
pub struct VectorCollectionSnapshot {
    /// Collection name
    pub name: String,

    /// Collection configuration
    pub config: VectorConfig,

    /// Next VectorId to allocate (CRITICAL for T4)
    pub next_id: u64,

    /// Free storage slots (CRITICAL for correct slot reuse)
    pub free_slots: Vec<usize>,

    /// All vectors in the collection
    pub vectors: Vec<VectorSnapshot>,
}

/// Single vector in snapshot
#[derive(Debug, Clone)]
pub struct VectorSnapshot {
    pub key: String,
    pub vector_id: u64,
    pub embedding: Vec<f32>,
    pub metadata: Option<Vec<u8>>,  // Serialized JSON
}

impl VectorCollectionSnapshot {
    /// Serialize to bytes
    ///
    /// Format:
    /// - version (1 byte)
    /// - name_len (2 bytes) + name
    /// - dimension (4 bytes)
    /// - metric (1 byte)
    /// - next_id (8 bytes) - CRITICAL
    /// - free_slots_count (4 bytes) + free_slots data - CRITICAL
    /// - vector_count (4 bytes)
    /// - vectors...
    pub fn to_bytes(&self) -> VectorResult<Vec<u8>> {
        let mut buf = Vec::new();

        // Version
        buf.push(VECTOR_SNAPSHOT_VERSION);

        // Name
        let name_bytes = self.name.as_bytes();
        buf.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        buf.extend_from_slice(name_bytes);

        // Config
        buf.extend_from_slice(&(self.config.dimension as u32).to_le_bytes());
        buf.push(self.config.metric.to_byte());

        // next_id - CRITICAL for T4!
        buf.extend_from_slice(&self.next_id.to_le_bytes());

        // free_slots - CRITICAL for correct recovery!
        buf.extend_from_slice(&(self.free_slots.len() as u32).to_le_bytes());
        for slot in &self.free_slots {
            buf.extend_from_slice(&(*slot as u64).to_le_bytes());
        }

        // Vectors
        buf.extend_from_slice(&(self.vectors.len() as u32).to_le_bytes());
        for vec in &self.vectors {
            self.write_vector(&mut buf, vec)?;
        }

        Ok(buf)
    }

    fn write_vector(&self, buf: &mut Vec<u8>, vec: &VectorSnapshot) -> VectorResult<()> {
        // key
        let key_bytes = vec.key.as_bytes();
        buf.extend_from_slice(&(key_bytes.len() as u16).to_le_bytes());
        buf.extend_from_slice(key_bytes);

        // vector_id
        buf.extend_from_slice(&vec.vector_id.to_le_bytes());

        // embedding (as raw f32s)
        buf.extend_from_slice(&(vec.embedding.len() as u32).to_le_bytes());
        for v in &vec.embedding {
            buf.extend_from_slice(&v.to_le_bytes());
        }

        // metadata
        if let Some(ref meta) = vec.metadata {
            buf.push(1);
            buf.extend_from_slice(&(meta.len() as u32).to_le_bytes());
            buf.extend_from_slice(meta);
        } else {
            buf.push(0);
        }

        Ok(())
    }

    /// Deserialize from bytes
    pub fn from_bytes(bytes: &[u8]) -> VectorResult<Self> {
        let mut pos = 0;

        // Version check
        if bytes.is_empty() || bytes[0] != VECTOR_SNAPSHOT_VERSION {
            return Err(VectorError::Serialization(
                format!("Invalid snapshot version: {:?}", bytes.get(0))
            ));
        }
        pos += 1;

        // Name
        let name_len = u16::from_le_bytes([bytes[pos], bytes[pos + 1]]) as usize;
        pos += 2;
        let name = String::from_utf8(bytes[pos..pos + name_len].to_vec())
            .map_err(|e| VectorError::Serialization(e.to_string()))?;
        pos += name_len;

        // Config
        let dimension = u32::from_le_bytes(
            bytes[pos..pos + 4].try_into().unwrap()
        ) as usize;
        pos += 4;

        let metric = DistanceMetric::from_byte(bytes[pos])
            .ok_or_else(|| VectorError::Serialization("Invalid metric".into()))?;
        pos += 1;

        let config = VectorConfig::new(dimension, metric)?;

        // next_id - CRITICAL
        let next_id = u64::from_le_bytes(bytes[pos..pos + 8].try_into().unwrap());
        pos += 8;

        // free_slots - CRITICAL
        let free_slots_count = u32::from_le_bytes(
            bytes[pos..pos + 4].try_into().unwrap()
        ) as usize;
        pos += 4;

        let mut free_slots = Vec::with_capacity(free_slots_count);
        for _ in 0..free_slots_count {
            let slot = u64::from_le_bytes(bytes[pos..pos + 8].try_into().unwrap()) as usize;
            pos += 8;
            free_slots.push(slot);
        }

        // Vectors
        let vector_count = u32::from_le_bytes(
            bytes[pos..pos + 4].try_into().unwrap()
        ) as usize;
        pos += 4;

        let mut vectors = Vec::with_capacity(vector_count);
        for _ in 0..vector_count {
            let (vec, new_pos) = Self::read_vector(bytes, pos, dimension)?;
            pos = new_pos;
            vectors.push(vec);
        }

        Ok(VectorCollectionSnapshot {
            name,
            config,
            next_id,
            free_slots,
            vectors,
        })
    }

    fn read_vector(bytes: &[u8], mut pos: usize, expected_dim: usize) -> VectorResult<(VectorSnapshot, usize)> {
        // key
        let key_len = u16::from_le_bytes([bytes[pos], bytes[pos + 1]]) as usize;
        pos += 2;
        let key = String::from_utf8(bytes[pos..pos + key_len].to_vec())
            .map_err(|e| VectorError::Serialization(e.to_string()))?;
        pos += key_len;

        // vector_id
        let vector_id = u64::from_le_bytes(bytes[pos..pos + 8].try_into().unwrap());
        pos += 8;

        // embedding
        let emb_len = u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
        pos += 4;

        if emb_len != expected_dim {
            return Err(VectorError::DimensionMismatch {
                expected: expected_dim,
                got: emb_len,
            });
        }

        let mut embedding = Vec::with_capacity(emb_len);
        for _ in 0..emb_len {
            let v = f32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap());
            pos += 4;
            embedding.push(v);
        }

        // metadata
        let has_metadata = bytes[pos] != 0;
        pos += 1;

        let metadata = if has_metadata {
            let meta_len = u32::from_le_bytes(bytes[pos..pos + 4].try_into().unwrap()) as usize;
            pos += 4;
            let meta = bytes[pos..pos + meta_len].to_vec();
            pos += meta_len;
            Some(meta)
        } else {
            None
        };

        Ok((VectorSnapshot { key, vector_id, embedding, metadata }, pos))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_roundtrip() {
        let snapshot = VectorCollectionSnapshot {
            name: "test_collection".to_string(),
            config: VectorConfig::new(4, DistanceMetric::Cosine).unwrap(),
            next_id: 100,
            free_slots: vec![16, 32, 48],
            vectors: vec![
                VectorSnapshot {
                    key: "doc1".to_string(),
                    vector_id: 0,
                    embedding: vec![1.0, 2.0, 3.0, 4.0],
                    metadata: None,
                },
                VectorSnapshot {
                    key: "doc2".to_string(),
                    vector_id: 1,
                    embedding: vec![5.0, 6.0, 7.0, 8.0],
                    metadata: Some(b"{}".to_vec()),
                },
            ],
        };

        let bytes = snapshot.to_bytes().unwrap();
        let restored = VectorCollectionSnapshot::from_bytes(&bytes).unwrap();

        assert_eq!(restored.name, "test_collection");
        assert_eq!(restored.next_id, 100);
        assert_eq!(restored.free_slots, vec![16, 32, 48]);
        assert_eq!(restored.vectors.len(), 2);
        assert_eq!(restored.vectors[0].key, "doc1");
        assert_eq!(restored.vectors[1].vector_id, 1);
    }

    #[test]
    fn test_next_id_and_free_slots_persisted() {
        // CRITICAL: This test verifies invariant T4
        let snapshot = VectorCollectionSnapshot {
            name: "test".to_string(),
            config: VectorConfig::new(4, DistanceMetric::Cosine).unwrap(),
            next_id: 12345,
            free_slots: vec![0, 4, 8],
            vectors: vec![],
        };

        let bytes = snapshot.to_bytes().unwrap();
        let restored = VectorCollectionSnapshot::from_bytes(&bytes).unwrap();

        // These MUST match exactly for T4
        assert_eq!(restored.next_id, 12345, "next_id not persisted correctly!");
        assert_eq!(restored.free_slots, vec![0, 4, 8], "free_slots not persisted correctly!");
    }
}
```

### Acceptance Criteria

- [ ] Version byte (0x01) for format evolution
- [ ] next_id MUST be serialized
- [ ] free_slots MUST be serialized
- [ ] All vectors with embedding data
- [ ] Compact binary format
- [ ] Test: snapshot + WAL replay equals original state (S8)

### Complete Story

```bash
~/.cargo/bin/cargo test --workspace
./scripts/complete-story.sh 423
```

---

## Story #424: Vector Recovery Implementation

**GitHub Issue**: [#424](https://github.com/anibjoshi/in-mem/issues/424)
**Estimated Time**: 3 hours
**Dependencies**: #422, #423
**Blocks**: None

### Start Story

```bash
gh issue view 424
./scripts/start-story.sh 55 424 vector-recovery
```

### Implementation

Create `crates/durability/src/vector_recovery.rs`:

```rust
//! Vector recovery from snapshot + WAL

use crate::vector::{VectorStore, VectorHeap, VectorConfig, VectorError, VectorResult, VectorId};
use super::vector_snapshot::VectorCollectionSnapshot;
use super::vector_wal::{WalVectorUpsert, WalVectorDelete};

/// Vector recovery handler
pub struct VectorRecovery;

impl VectorRecovery {
    /// Recover vector state from snapshot + WAL
    ///
    /// 1. Load snapshot (includes next_id, free_slots)
    /// 2. Replay WAL entries from snapshot offset
    /// 3. Verify invariants
    pub fn recover_collection(
        snapshot: Option<VectorCollectionSnapshot>,
    ) -> VectorResult<(VectorHeap, std::collections::HashMap<String, VectorRecord>)> {
        if let Some(snap) = snapshot {
            // CRITICAL: Restore with persisted next_id and free_slots!
            let mut heap = VectorHeap::for_recovery(
                snap.config.clone(),
                snap.next_id,  // CRITICAL for T4
                snap.free_slots.clone(),  // CRITICAL for correct slot reuse
            );

            let mut records = std::collections::HashMap::new();

            // Restore vectors
            for vec in snap.vectors {
                heap.insert_with_id(
                    VectorId::new(vec.vector_id),
                    &vec.embedding,
                )?;

                // Restore record
                records.insert(vec.key.clone(), VectorRecord {
                    key: vec.key,
                    vector_id: VectorId::new(vec.vector_id),
                    metadata: vec.metadata.map(|m| {
                        serde_json::from_slice(&m).unwrap_or(serde_json::Value::Null)
                    }),
                    version: 1,
                    created_at: 0,
                    updated_at: 0,
                });
            }

            Ok((heap, records))
        } else {
            // Fresh start
            Err(VectorError::Internal("No snapshot provided".into()))
        }
    }

    /// Verify recovery invariants
    pub fn verify_invariants(
        heap: &VectorHeap,
        records: &std::collections::HashMap<String, VectorRecord>,
    ) -> VectorResult<()> {
        // T4: next_id > max existing vector_id
        let max_id = records.values()
            .map(|r| r.vector_id.as_u64())
            .max()
            .unwrap_or(0);

        if heap.next_id() <= max_id {
            return Err(VectorError::Internal(format!(
                "T4 violated: next_id {} <= max existing id {}",
                heap.next_id(), max_id
            )));
        }

        // S9: Heap-KV consistency
        for record in records.values() {
            if !heap.contains(record.vector_id) {
                return Err(VectorError::Internal(format!(
                    "S9 violated: record {} has vector_id {} not in heap",
                    record.key, record.vector_id
                )));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::DistanceMetric;

    #[test]
    fn test_recovery_preserves_next_id() {
        let snapshot = VectorCollectionSnapshot {
            name: "test".to_string(),
            config: VectorConfig::new(4, DistanceMetric::Cosine).unwrap(),
            next_id: 100,
            free_slots: vec![],
            vectors: vec![
                VectorSnapshot {
                    key: "key1".to_string(),
                    vector_id: 50,
                    embedding: vec![1.0, 2.0, 3.0, 4.0],
                    metadata: None,
                },
            ],
        };

        let (heap, _records) = VectorRecovery::recover_collection(Some(snapshot)).unwrap();

        // next_id should be preserved from snapshot
        assert_eq!(heap.next_id(), 100);
    }

    #[test]
    fn test_recovery_new_insert_uses_correct_id() {
        let snapshot = VectorCollectionSnapshot {
            name: "test".to_string(),
            config: VectorConfig::new(4, DistanceMetric::Cosine).unwrap(),
            next_id: 100,
            free_slots: vec![],
            vectors: vec![],
        };

        let (mut heap, _) = VectorRecovery::recover_collection(Some(snapshot)).unwrap();

        // New insert should use ID >= 100
        let new_id = heap.insert(&[1.0, 2.0, 3.0, 4.0]).unwrap();
        assert!(new_id.as_u64() >= 100, "New ID should be >= next_id from snapshot");
    }
}
```

### Acceptance Criteria

- [ ] Load snapshot with next_id and free_slots
- [ ] Replay WAL from snapshot offset
- [ ] Verify T4: VectorId monotonicity
- [ ] Verify S9: Heap-KV reconstructibility
- [ ] Test: crash after N operations, recover, verify state

### Complete Story

```bash
~/.cargo/bin/cargo test --workspace
./scripts/complete-story.sh 424
```

---

## Story #425: Cross-Primitive Transaction Tests

**GitHub Issue**: [#425](https://github.com/anibjoshi/in-mem/issues/425)
**Estimated Time**: 3 hours
**Dependencies**: #424
**Blocks**: None

### Start Story

```bash
gh issue view 425
./scripts/start-story.sh 55 425 cross-primitive-tests
```

### Implementation

Create `crates/primitives/tests/vector_transaction_tests.rs`:

```rust
//! Integration tests for cross-primitive transactions with Vector

use std::sync::Arc;
use tempfile::TempDir;

// #[test]
// fn test_vector_kv_atomic_transaction() {
//     let tmp = TempDir::new().unwrap();
//     let db = Database::open(tmp.path()).unwrap();
//
//     let run_id = RunId::new();
//
//     // Create atomic transaction: KV write + Vector upsert
//     db.transaction(|tx| {
//         // KV operation
//         tx.kv_set("docs", "doc_id", b"document content")?;
//
//         // Vector operation (linked)
//         tx.vector_upsert(
//             run_id.clone(),
//             "embeddings",
//             "doc_id",
//             &[0.1, 0.2, 0.3, 0.4],
//             None,
//         )?;
//
//         Ok(())
//     }).unwrap();
//
//     // Verify both committed
//     assert!(db.kv_get("docs", "doc_id").is_some());
//     assert!(db.vector_store().get(run_id, "embeddings", "doc_id").unwrap().is_some());
// }

// #[test]
// fn test_transaction_rollback() {
//     let tmp = TempDir::new().unwrap();
//     let db = Database::open(tmp.path()).unwrap();
//
//     let run_id = RunId::new();
//
//     // Create collection first
//     db.vector_store().create_collection(
//         run_id.clone(),
//         "embeddings",
//         VectorConfig::new(4, DistanceMetric::Cosine).unwrap(),
//     ).unwrap();
//
//     // Transaction that fails
//     let result = db.transaction(|tx| {
//         tx.kv_set("docs", "doc_id", b"content")?;
//         tx.vector_upsert(
//             run_id.clone(),
//             "embeddings",
//             "doc_id",
//             &[0.1, 0.2, 0.3, 0.4],
//             None,
//         )?;
//
//         // Force failure
//         Err(VectorError::Internal("Simulated failure".into()))
//     });
//
//     assert!(result.is_err());
//
//     // Both should be rolled back
//     assert!(db.kv_get("docs", "doc_id").is_none());
//     assert!(db.vector_store().get(run_id, "embeddings", "doc_id").unwrap().is_none());
// }

#[test]
fn test_crash_recovery_vector_kv() {
    // This test simulates crash recovery

    // Setup: Insert data, simulate crash, recover
    // Verify: All committed data is present
    // Verify: VectorId monotonicity (T4)
}

#[test]
fn test_snapshot_wal_equivalence() {
    // Invariant S8: state(snapshot) + replay(WAL) == state(pure WAL replay)

    // 1. Create database, insert vectors
    // 2. Take snapshot at point T
    // 3. Insert more vectors
    // 4. Recover from snapshot + WAL after T
    // 5. Recover from full WAL (no snapshot)
    // 6. Compare states - must be identical
}

#[test]
fn test_vector_id_monotonicity_across_crashes() {
    // Invariant T4: VectorId never reused even across crashes

    // 1. Insert 100 vectors, record max ID
    // 2. Simulate crash
    // 3. Recover
    // 4. Insert new vector
    // 5. New ID must be > max ID from step 1
}

#[test]
fn test_heap_kv_reconstructibility() {
    // Invariant S9: Both heap and KV can be rebuilt from WAL

    // 1. Insert vectors with metadata
    // 2. Delete some vectors
    // 3. Rebuild from WAL only (no snapshot)
    // 4. Verify heap matches expected state
    // 5. Verify KV records match expected state
}
```

### Acceptance Criteria

- [ ] Test atomic KV + Vector transactions
- [ ] Test rollback on partial failure
- [ ] Test crash recovery preserves both primitives
- [ ] Test VectorId monotonicity across crashes (T4)
- [ ] Test snapshot-WAL equivalence (S8)
- [ ] Test heap-KV reconstructibility (S9)

### Complete Story

```bash
~/.cargo/bin/cargo test --workspace
./scripts/complete-story.sh 425
```

---

## Epic 55 Completion Checklist

### Validation

```bash
# Full test suite
~/.cargo/bin/cargo test --workspace

# Durability-specific tests
~/.cargo/bin/cargo test vector_wal
~/.cargo/bin/cargo test vector_snapshot
~/.cargo/bin/cargo test vector_recovery
~/.cargo/bin/cargo test vector_transaction

# Clippy and format
~/.cargo/bin/cargo clippy --workspace -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### Critical Invariant Tests

Run these tests explicitly:

```bash
# T4: VectorId monotonicity
~/.cargo/bin/cargo test test_vector_id_monotonicity_across_crashes

# S8: Snapshot-WAL equivalence
~/.cargo/bin/cargo test test_snapshot_wal_equivalence

# S9: Heap-KV reconstructibility
~/.cargo/bin/cargo test test_heap_kv_reconstructibility
```

### Epic Merge

```bash
git checkout develop
git merge --no-ff epic-55-transaction-durability -m "Epic 55: Transaction & Durability complete"
git push origin develop

gh issue close 393 --comment "Epic 55 complete. All 5 stories merged and validated."
```

---

*End of Epic 55 Prompts*
