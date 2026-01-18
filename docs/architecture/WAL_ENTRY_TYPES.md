# WAL Entry Type Allocation

## Overview

WAL entry types are single bytes (0x00-0xFF) that identify the type of each entry. Ranges are allocated to prevent conflicts between primitives.

## Allocation Table

| Range | Owner | Status | Description |
|-------|-------|--------|-------------|
| 0x00-0x0F | Core | FROZEN | Transaction control (commit, abort, snapshot) |
| 0x10-0x1F | KV | FROZEN | Key-value operations |
| 0x20-0x2F | JSON | FROZEN | JSON document operations |
| 0x30-0x3F | Event | FROZEN | Event log operations |
| 0x40-0x4F | State | FROZEN | State cell operations |
| 0x50-0x5F | Trace | FROZEN | Trace store operations |
| 0x60-0x6F | Run | FROZEN | Run lifecycle operations |
| 0x70-0x7F | Vector | FROZEN | Vector store operations (M8) |
| 0x80-0x8F | Reserved | AVAILABLE | For future primitives |
| 0x90-0x9F | Reserved | AVAILABLE | For future primitives |
| 0xA0-0xAF | Reserved | AVAILABLE | For future primitives |
| 0xB0-0xBF | Reserved | AVAILABLE | For future primitives |
| 0xC0-0xCF | Reserved | AVAILABLE | For future primitives |
| 0xD0-0xDF | Reserved | AVAILABLE | For future primitives |
| 0xE0-0xEF | Reserved | AVAILABLE | For future primitives |
| 0xF0-0xFF | Reserved | INTERNAL | Reserved for future internal use |

## Core Entry Types (0x00-0x0F)

| Value | Name | Description |
|-------|------|-------------|
| 0x00 | TransactionCommit | Marks transaction as committed |
| 0x01 | TransactionAbort | Marks transaction as aborted |
| 0x02 | SnapshotMarker | Records snapshot was taken |
| 0x03-0x0F | Reserved | For future core use |

These are transaction control entries used by the recovery engine.

## KV Entry Types (0x10-0x1F)

| Value | Name | Description |
|-------|------|-------------|
| 0x10 | KvPut | Put key-value pair |
| 0x11 | KvDelete | Delete key |
| 0x12-0x1F | Reserved | For future KV use |

### KvPut Payload Format

```
[key_len: u32][key: bytes][value_len: u32][value: bytes]
```

### KvDelete Payload Format

```
[key_len: u32][key: bytes]
```

## JSON Entry Types (0x20-0x2F)

| Value | Name | Description |
|-------|------|-------------|
| 0x20 | JsonCreate | Create new document |
| 0x21 | JsonSet | Set document value |
| 0x22 | JsonDelete | Delete document |
| 0x23 | JsonPatch | Apply JSON patch (RFC 6902) |
| 0x24-0x2F | Reserved | For future JSON use |

### JsonCreate Payload Format

```
[doc_id: 16 bytes][content: msgpack]
```

### JsonSet Payload Format

```
[doc_id: 16 bytes][content: msgpack]
```

### JsonDelete Payload Format

```
[doc_id: 16 bytes]
```

### JsonPatch Payload Format

```
[doc_id: 16 bytes][patch: msgpack]
```

## Event Entry Types (0x30-0x3F)

| Value | Name | Description |
|-------|------|-------------|
| 0x30 | EventAppend | Append event to log |
| 0x31-0x3F | Reserved | For future Event use |

### EventAppend Payload Format

```
[event_type_len: u16][event_type: string][timestamp: u64][payload: msgpack]
```

## State Entry Types (0x40-0x4F)

| Value | Name | Description |
|-------|------|-------------|
| 0x40 | StateInit | Initialize state cell |
| 0x41 | StateSet | Set state value |
| 0x42 | StateTransition | State machine transition |
| 0x43-0x4F | Reserved | For future State use |

### StateInit Payload Format

```
[cell_key_len: u32][cell_key: bytes][initial_value: msgpack]
```

### StateSet Payload Format

```
[cell_key_len: u32][cell_key: bytes][new_value: msgpack]
```

### StateTransition Payload Format

```
[cell_key_len: u32][cell_key: bytes][from_state: msgpack][to_state: msgpack]
```

## Trace Entry Types (0x50-0x5F)

| Value | Name | Description |
|-------|------|-------------|
| 0x50 | TraceRecord | Record trace span |
| 0x51-0x5F | Reserved | For future Trace use |

### TraceRecord Payload Format

```
[trace_id: 16 bytes][span_id: 8 bytes][parent_id: 8 bytes][name_len: u16][name: string][start: u64][end: u64][attributes: msgpack]
```

## Run Entry Types (0x60-0x6F)

| Value | Name | Description |
|-------|------|-------------|
| 0x60 | RunCreate | Create run entry |
| 0x61 | RunUpdate | Update run metadata |
| 0x62 | RunEnd | End run (mark completed) |
| 0x63 | RunBegin | Begin run (mark active) |
| 0x64-0x6F | Reserved | For future Run use |

### RunBegin Payload Format

```
[run_id: 16 bytes][timestamp: u64]
```

### RunEnd Payload Format

```
[run_id: 16 bytes][timestamp: u64][event_count: u64]
```

## Vector Entry Types (0x70-0x7F) - M8

| Value | Name | Description |
|-------|------|-------------|
| 0x70 | VectorCollectionCreate | Create a vector collection |
| 0x71 | VectorCollectionDelete | Delete a vector collection |
| 0x72 | VectorUpsert | Insert or update a vector |
| 0x73 | VectorDelete | Delete a vector |
| 0x74-0x7F | Reserved | For future Vector use |

### VectorCollectionCreate Payload Format (0x70)

```
[MessagePack encoded WalVectorCollectionCreate]
```

Fields:
- `run_id`: RunId (16 bytes UUID)
- `collection`: String (collection name)
- `config`: VectorConfigSerde
  - `dimension`: usize
  - `metric`: u8 (0=Cosine, 1=Euclidean, 2=DotProduct)
  - `storage_dtype`: u8 (0=F32, reserved: 1=F16, 2=Int8)
- `timestamp`: u64 (microseconds since epoch)

### VectorCollectionDelete Payload Format (0x71)

```
[MessagePack encoded WalVectorCollectionDelete]
```

Fields:
- `run_id`: RunId (16 bytes UUID)
- `collection`: String (collection name)
- `timestamp`: u64 (microseconds since epoch)

### VectorUpsert Payload Format (0x72)

```
[MessagePack encoded WalVectorUpsert]
```

Fields:
- `run_id`: RunId (16 bytes UUID)
- `collection`: String (collection name)
- `key`: String (user-provided key)
- `vector_id`: u64 (internal VectorId)
- `embedding`: Vec<f32> (full embedding - TEMPORARY M8 FORMAT)
- `metadata`: Option<serde_json::Value> (optional JSON metadata)
- `timestamp`: u64 (microseconds since epoch)

**Note**: Full embeddings in WAL is a temporary M8 format. M9 may optimize with external embedding storage or delta encoding.

### VectorDelete Payload Format (0x73)

```
[MessagePack encoded WalVectorDelete]
```

Fields:
- `run_id`: RunId (16 bytes UUID)
- `collection`: String (collection name)
- `key`: String (user-provided key)
- `vector_id`: u64 (internal VectorId)
- `timestamp`: u64 (microseconds since epoch)

### Relationship to Snapshots

Vector snapshots use a separate binary format (not WAL entries) for efficiency:
- Snapshot format is versioned independently (current: v1)
- Contains full collection state including embeddings
- WAL entries are applied on top of snapshots during recovery
- See `crates/primitives/src/vector/snapshot.rs` for snapshot format details

## Requesting a Range

To request a new range for a primitive:

1. Open an issue with your primitive design
2. Specify how many entry types you need
3. We will allocate a range from the available space

## Entry Versioning

Each WAL entry includes a format version in the envelope:

```
[magic: 4 bytes][version: 2 bytes][entry_type: 1 byte][tx_id: 16 bytes][payload_len: 4 bytes][payload: bytes][crc32: 4 bytes]
```

The version field allows evolving the payload format without changing the entry type.

Current format versions:
- All core types: v1
- All primitive types: v1

## Forward Compatibility

The recovery engine handles unknown entry types gracefully:

1. If entry type is in a known range but unknown value: log warning, skip entry
2. If entry type is in reserved range: log warning, skip entry
3. Entry length is always known, so skipping is always safe

## Code References

- Entry type definitions: `crates/durability/src/wal_entry_types.rs`
- WAL ranges: `crates/storage/src/primitive_ext.rs` (wal_ranges module)
- Primitive lookup: `primitive_for_wal_type()` function
