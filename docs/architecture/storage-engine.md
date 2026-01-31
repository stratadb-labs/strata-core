# Storage Engine

The storage engine is the lowest data layer in StrataDB. It provides a concurrent, sharded key-value store that all primitives share.

## ShardedStore

The `ShardedStore` (in `strata-storage`) is backed by `DashMap` — a lock-free concurrent hash map. Data is sharded across multiple segments to reduce contention.

### Key Structure

Every key in storage is a composite of:

```
{run_id}:{primitive_type}:{user_key}
```

For example:
- `default:kv:user:name` — KV key "user:name" in the "default" run
- `experiment-1:event:0000001` — Event at sequence 1 in "experiment-1"
- `default:state:status` — State cell "status" in the "default" run
- `default:json:config` — JSON document "config" in the "default" run
- `default:vector:embeddings:doc-1` — Vector "doc-1" in collection "embeddings"

This encoding provides:
- **Run isolation** — keys from different runs never collide
- **Primitive isolation** — KV key "status" and state cell "status" are distinct
- **Prefix scanning** — list all keys in a run, or all keys of a primitive type in a run

### Namespace

The `Namespace` type encapsulates the run-scoped key prefix. When you call `db.kv_put("key", value)`, the executor:

1. Resolves the current run ID
2. Creates a `Namespace` for that run
3. Builds the full storage key: `{run}:kv:key`
4. Writes to the `ShardedStore`

### StoredValue

Values in the store are wrapped in `StoredValue`, which includes:
- The serialized `Value`
- Version metadata
- Timestamp

## MVCC (Multi-Version Concurrency Control)

StrataDB supports versioned reads via `getv()` / `readv()` operations. The storage layer retains version history for keys, allowing you to read the value at a specific version.

Version history is subject to the retention policy — old versions may be trimmed.

## Run Registry

The `RunRegistry` tracks all known runs and their metadata. It is consulted during:
- Run existence checks
- Run listing
- Run deletion (cascading key deletion)

## Inverted Index

The `InvertedIndex` indexes text content from KV values, event payloads, and JSON documents. It supports BM25 scoring for keyword search, used by the intelligence layer's hybrid search.

## Thread Safety

`ShardedStore` is fully thread-safe. Multiple threads can read and write concurrently without external synchronization. The DashMap sharding ensures that concurrent writes to different keys have no contention.

## Performance Characteristics

| Operation | Complexity | Notes |
|-----------|-----------|-------|
| Point read | O(1) | Hash lookup |
| Point write | O(1) | Hash insert |
| Prefix scan | O(n) | Scans matching prefix |
| Run deletion | O(n) | Scans and deletes all keys in run |

Where n is the number of keys matching the prefix/run.
