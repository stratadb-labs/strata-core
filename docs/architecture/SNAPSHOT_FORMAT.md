# Snapshot Format Specification

## Overview

Snapshots are single-file point-in-time captures of database state. They enable bounded recovery time by avoiding full WAL replay from the beginning.

## File Format (v1)

```
+----------------------+
| Magic (10 bytes)     |  "INMEM_SNAP"
+----------------------+
| Version (4 bytes)    |  u32, little-endian, currently 1
+----------------------+
| Timestamp (8 bytes)  |  u64, microseconds since epoch
+----------------------+
| WAL Offset (8 bytes) |  u64, WAL position at snapshot time
+----------------------+
| Tx Count (8 bytes)   |  u64, transactions included in snapshot
+----------------------+
| Prim Count (1 byte)  |  u8, number of primitive sections
+----------------------+
| Section 1            |  Primitive section (variable size)
+----------------------+
| Section 2            |  ...
+----------------------+
| ...                  |
+----------------------+
| Section N            |
+----------------------+
| CRC32 (4 bytes)      |  Checksum of all bytes above
+----------------------+
```

### Header Fields

The header is a fixed 38-byte structure:

| Field | Offset | Size | Type | Description |
|-------|--------|------|------|-------------|
| Magic | 0 | 10 | bytes | "INMEM_SNAP" (ASCII) |
| Version | 10 | 4 | u32 | Format version (currently 1) |
| Timestamp | 14 | 8 | u64 | Snapshot creation time (microseconds since epoch) |
| WAL Offset | 22 | 8 | u64 | WAL position covered by this snapshot |
| Tx Count | 30 | 8 | u64 | Number of committed transactions |

**Fixed header size: 38 bytes** (`SNAPSHOT_HEADER_SIZE` constant)

### Post-Header Field

Immediately after the fixed header:

| Field | Offset | Size | Type | Description |
|-------|--------|------|------|-------------|
| Prim Count | 38 | 1 | u8 | Number of primitive sections |

**Note**: The Prim Count is written separately after the header, not as part of it. This allows the header to remain a fixed 38-byte structure for simpler parsing. The minimum snapshot size is 43 bytes (38 header + 1 prim count + 4 CRC32).

## Primitive Section Format

Each primitive section has a header followed by data:

```
+----------------------+
| Type ID (1 byte)     |  u8, primitive type identifier
+----------------------+
| Length (8 bytes)     |  u64, data length in bytes
+----------------------+
| Data (variable)      |  Serialized primitive state
+----------------------+
```

### Section Header

| Field | Offset | Size | Type | Description |
|-------|--------|------|------|-------------|
| Type ID | 0 | 1 | u8 | Primitive type identifier |
| Length | 1 | 8 | u64 | Data length in bytes (little-endian) |

Section header size: 9 bytes

### Primitive Type IDs

| ID | Primitive | Status |
|----|-----------|--------|
| 1 | KV | FROZEN |
| 2 | JSON | FROZEN |
| 3 | Event | FROZEN |
| 4 | State | FROZEN |
| 5 | Trace | FROZEN |
| 6 | Run | FROZEN |
| 7 | Vector | RESERVED |
| 8-255 | Future | AVAILABLE |

## Data Format per Primitive

### KV (Type 1)

Serialized using bincode:

```rust
// Key-value pairs
Vec<(Key, VersionedValue)>
```

Key structure:
- Namespace
- User key bytes
- Type tag

### JSON (Type 2)

Serialized using bincode:

```rust
// Document ID -> Document content
Vec<(JsonDocId, JsonDocument)>
```

JsonDocument includes:
- Content (serde_json::Value)
- Version
- Last modified timestamp

### Event (Type 3)

Serialized using bincode:

```rust
// Log key -> Events
Vec<(Key, Vec<Event>)>
```

Event includes:
- Event type string
- Timestamp
- Sequence number
- Payload
- Hash (for chain verification)
- Previous hash

### State (Type 4)

Serialized using bincode:

```rust
// Cell key -> State value
Vec<(Key, StateValue)>
```

StateValue includes:
- Current value
- Version
- Last transition timestamp

### Trace (Type 5)

Serialized using bincode:

```rust
// Trace key -> Spans
Vec<(Key, Vec<Span>)>
```

Span includes:
- Trace ID
- Span ID
- Parent ID
- Name
- Start/end timestamps
- Attributes

### Run (Type 6)

Serialized using bincode:

```rust
// Run metadata
Vec<RunMetadata>
```

RunMetadata includes:
- Run ID
- Status (Active, Completed, Orphaned)
- Start/end timestamps
- Event count
- WAL offsets

## CRC32 Checksum

The CRC32 checksum is computed over all bytes from Magic (offset 0) through the last primitive section (before CRC32).

Algorithm: CRC-32/ISO-HDLC (same as used in Ethernet, gzip)
- Polynomial: 0x04C11DB7
- Initial value: 0xFFFFFFFF
- Final XOR: 0xFFFFFFFF

## Validation

To validate a snapshot file:

1. **Size check**: File must be at least 43 bytes (38 header + 1 prim count + 4 CRC32)
2. **Magic check**: First 10 bytes must be "INMEM_SNAP"
3. **Version check**: Version must be 1 (reject unknown versions)
4. **CRC32 check**: Computed CRC must match stored CRC
5. **Section check**: Sum of section lengths must match file size

```rust
fn validate_snapshot(path: &Path) -> Result<(), SnapshotError> {
    let data = std::fs::read(path)?;

    // Size check
    if data.len() < 43 {
        return Err(SnapshotError::TooShort);
    }

    // Magic check
    if &data[0..10] != b"INMEM_SNAP" {
        return Err(SnapshotError::InvalidMagic);
    }

    // Version check
    let version = u32::from_le_bytes(data[10..14].try_into().unwrap());
    if version != 1 {
        return Err(SnapshotError::UnsupportedVersion(version));
    }

    // CRC32 check
    let stored_crc = u32::from_le_bytes(data[data.len()-4..].try_into().unwrap());
    let computed_crc = crc32fast::hash(&data[..data.len()-4]);
    if stored_crc != computed_crc {
        return Err(SnapshotError::CrcMismatch { stored: stored_crc, computed: computed_crc });
    }

    Ok(())
}
```

## Version Evolution

The version field allows format evolution:

| Version | Description | Status |
|---------|-------------|--------|
| 1 | Current format | ACTIVE |
| 2 | Reserved for compression | FUTURE |
| 3 | Reserved for encryption | FUTURE |

Readers should reject unknown versions with a clear error message.

## Adding a New Primitive

To add a primitive to snapshots:

1. Allocate a type ID (see Primitive Type IDs above)
2. Implement `PrimitiveStorageExt::snapshot_serialize()`
3. Implement `PrimitiveStorageExt::snapshot_deserialize()`
4. Register in `PrimitiveRegistry`

The snapshot writer/reader will automatically include the new primitive.

```rust
impl PrimitiveStorageExt for VectorStore {
    fn primitive_type_id(&self) -> u8 { 7 }

    fn snapshot_serialize(&self) -> Result<Vec<u8>, PrimitiveExtError> {
        // Serialize vector data (NOT HNSW index - that's derived)
        bincode::serialize(&self.vectors)
            .map_err(|e| PrimitiveExtError::Serialization(e.to_string()))
    }

    fn snapshot_deserialize(&mut self, data: &[u8]) -> Result<(), PrimitiveExtError> {
        self.vectors = bincode::deserialize(data)
            .map_err(|e| PrimitiveExtError::Deserialization(e.to_string()))?;
        // Note: HNSW index is rebuilt by rebuild_indexes()
        Ok(())
    }
}
```

## Important Notes

### Indexes Are NOT Included

Snapshots contain only primary data, not derived indexes. Indexes are rebuilt during recovery via `rebuild_indexes()`.

This keeps snapshots smaller and ensures indexes are always consistent with the underlying data.

### Ordering

Primitive sections are written in type ID order (1, 2, 3, ...) for deterministic output.

### Atomicity

Snapshot files are written atomically using write-then-rename pattern:
1. Write to temporary file (`.snap.tmp`)
2. Sync to disk
3. Rename to final name (`.snap`)

This ensures partial snapshots are never visible.

## Code References

- Snapshot types: `crates/durability/src/snapshot_types.rs`
- Snapshot writer: `crates/durability/src/snapshot.rs`
- Primitive type IDs: `crates/storage/src/primitive_ext.rs` (primitive_type_ids module)
