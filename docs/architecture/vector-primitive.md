# Vector Primitive - Architecture Reference

## Overview

The Vector primitive provides semantic search over embedding vectors organized into named collections. Each collection has a fixed dimension and distance metric. Vectors are stored in both a persistent KV layer (for durability and metadata) and an in-memory index backend (for search performance).

- **Version semantics**: `Version::Counter(u64)` - per-vector counter starting at 1, incremented on upsert
- **Key construction**:
  - Vector entry: `Key { namespace: Namespace::for_branch(branch_id), type_tag: TypeTag::Vector (0x10), user_key: "collection/key".as_bytes() }`
  - Collection config: `Key { ..., type_tag: TypeTag::VectorConfig (0x12), user_key: collection_name.as_bytes() }`
- **Storage format**: `Value::Bytes(MessagePack)` wrapping `VectorRecord` or `CollectionRecord`
- **Transactional**: No - vector operations bypass the Session transaction layer entirely

## Layer Architecture

```
+------------------------------------------------------------------+
|  CLIENT                                                          |
|  Command::VectorUpsert { branch, collection, key, vector, meta } |
+------------------------------------------------------------------+
        |
        v
+------------------------------------------------------------------+
|  SESSION (session.rs)                                            |
|  ALWAYS routes to executor (non-transactional)                   |
|  Even if a transaction is active, vectors bypass it              |
+------------------------------------------------------------------+
        |
        v
+------------------------------------------------------------------+
|  EXECUTOR (executor.rs)                                          |
|  Dispatches to: crate::handlers::vector::vector_upsert(...)      |
+------------------------------------------------------------------+
        |
        v
+------------------------------------------------------------------+
|  HANDLER (handlers/vector.rs + bridge.rs)                        |
|  1. to_core_branch_id(&branch) -> core::BranchId                |
|  2. validate_not_internal_collection(&collection)                |
|  3. Auto-create collection if not exists                         |
|  4. Convert metadata: Value -> serde_json::Value                 |
|  5. Call primitives.vector.insert(...)                            |
|  6. Return Output::Version(u64)                                  |
+------------------------------------------------------------------+
        |
        v
+------------------------------------------------------------------+
|  ENGINE PRIMITIVE (primitives/vector/store.rs - VectorStore)     |
|  Dual storage:                                                   |
|  1. KV layer: MessagePack-encoded VectorRecord -> persistence    |
|  2. In-memory backend: raw f32 embeddings -> search performance  |
|                                                                  |
|  For insert:                                                     |
|  - Validate dimension matches collection config                  |
|  - Allocate VectorId (monotonic per collection)                  |
|  - Insert into in-memory backend                                 |
|  - Serialize VectorRecord to MessagePack                         |
|  - Write to KV via db.transaction()                              |
+------------------------------------------------------------------+
        |
        v
+------------------------------------------------------------------+
|  VECTOR INDEX BACKEND (primitives/vector/brute_force.rs)         |
|  BruteForceBackend:                                              |
|  - VectorHeap: BTreeMap<VectorId, Vec<f32>>                      |
|  - O(n) search: compute similarity for all vectors               |
|  - Sort by (score desc, VectorId asc)                            |
+------------------------------------------------------------------+
        |
        v
+------------------------------------------------------------------+
|  STORAGE (storage/sharded.rs)                                    |
|  Persistent storage for VectorRecord and CollectionRecord        |
|  Used for durability and restart recovery                        |
+------------------------------------------------------------------+
```

## Operation Flows

### VectorUpsert

```
Client               Handler             Engine (VectorStore)  Backend             Storage
  |                    |                   |                    |                   |
  |-- VectorUpsert --->|                   |                    |                   |
  | {branch,coll,key,  |                   |                    |                   |
  |  vector,metadata}  |                   |                    |                   |
  |                    |                   |                    |                   |
  |                    |-- validate ------>|                    |                   |
  |                    |   not internal    |                    |                   |
  |                    |   collection      |                    |                   |
  |                    |                   |                    |                   |
  |                    |-- auto-create --->|                    |                   |
  |                    |   collection      |                    |                   |
  |                    |   (ignore exists) |                    |                   |
  |                    |                   |                    |                   |
  |                    |-- convert meta -->|                    |                   |
  |                    |   Value->JsonVal  |                    |                   |
  |                    |                   |                    |                   |
  |                    |                   |-- validate dim --->|                   |
  |                    |                   |   embedding.len()  |                   |
  |                    |                   |   == config.dim    |                   |
  |                    |                   |                    |                   |
  |                    |                   |-- ensure loaded -->|                   |
  |                    |                   |   collection       |                   |
  |                    |                   |                    |                   |
  |                    |                   |-- check exists? -->|                   |
  |                    |                   |   get_vector_      |                   |-- read KV ------->|
  |                    |                   |   record_by_key    |                   |                   |
  |                    |                   |                    |                   |                   |
  |                    |                   |  NEW VECTOR:       |                   |                   |
  |                    |                   |-- allocate_id ---->|                   |                   |
  |                    |                   |   (monotonic)      |   VectorId(n)     |                   |
  |                    |                   |                    |                   |                   |
  |                    |                   |-- backend.insert ->|                   |                   |
  |                    |                   |   (id, embedding)  |-- store in heap ->|                   |
  |                    |                   |                    |   BTreeMap insert  |                   |
  |                    |                   |                    |                   |                   |
  |                    |                   |  UPDATE VECTOR:    |                   |                   |
  |                    |                   |  (keep same id)    |                   |                   |
  |                    |                   |-- backend.insert ->|-- heap upsert --->|                   |
  |                    |                   |   (existing id,    |                   |                   |
  |                    |                   |    new embedding)  |                   |                   |
  |                    |                   |                    |                   |                   |
  |                    |                   |-- serialize ------>|                   |                   |
  |                    |                   |   VectorRecord     |                   |                   |
  |                    |                   |   to MessagePack   |                   |                   |
  |                    |                   |                    |                   |                   |
  |                    |                   |-- db.transaction ->|                   |-- write KV ------>|
  |                    |                   |   txn.put(kv_key,  |                   |   Value::Bytes    |
  |                    |                   |     Value::Bytes)  |                   |                   |
  |                    |                   |                    |                   |                   |
  |<-- Output::Version |<- extract u64 ----|<- Counter(ver) ----|                   |                   |
```

**Steps:**

1. **Handler**: Validates collection name is not internal (`_` prefix). Auto-creates collection if it doesn't exist (uses vector dimension and Cosine metric by default). Converts metadata `Value` to `serde_json::Value`.
2. **Engine (VectorStore)**:
   - Validates embedding dimension matches collection config
   - Ensures collection is loaded in memory (`ensure_collection_loaded`)
   - Checks if vector already exists by reading the KV key
   - **New vector**: Allocates a `VectorId` from the backend's monotonic counter, inserts embedding into in-memory heap
   - **Update vector**: Keeps the same `VectorId`, updates embedding in heap
   - Serializes `VectorRecord` to MessagePack, writes to KV storage via `db.transaction()`
3. **Backend (BruteForceBackend)**: Inserts/updates the embedding in `VectorHeap` (a `BTreeMap<VectorId, Vec<f32>>`)

**Key format**: `Key::new_vector(namespace, collection, key)` where `user_key = "{collection}/{key}".as_bytes()`

---

### VectorGet

```
Client               Handler             Engine (VectorStore)  Backend             Storage
  |                    |                   |                    |                   |
  |-- VectorGet ------>|                   |                    |                   |
  | {branch,coll,key}  |                   |                    |                   |
  |                    |-- validate ------>|                    |                   |
  |                    |                   |                    |                   |
  |                    |                   |-- ensure loaded -->|                   |
  |                    |                   |                    |                   |
  |                    |                   |-- snapshot.get --->|                   |-- read chain ---->|
  |                    |                   |   Key::new_vector  |                   |   latest version  |
  |                    |                   |   (ns, coll, key)  |                   |                   |
  |                    |                   |                    |                   |                   |
  |                    |                   |   NOT FOUND:       |                   |                   |
  |                    |                   |   return None      |                   |                   |
  |                    |                   |                    |                   |                   |
  |                    |                   |   FOUND:           |                   |                   |
  |                    |                   |<- Value::Bytes ----|                   |                   |
  |                    |                   |                    |                   |                   |
  |                    |                   |-- deserialize ---->|                   |                   |
  |                    |                   |   VectorRecord     |                   |                   |
  |                    |                   |   from MessagePack |                   |                   |
  |                    |                   |                    |                   |                   |
  |                    |                   |-- backend.get ---->|                   |                   |
  |                    |                   |   (vector_id)      |-- read from heap -|                   |
  |                    |                   |                    |   return &[f32]    |                   |
  |                    |                   |                    |                   |                   |
  |                    |                   |-- build entry ---->|                   |                   |
  |                    |                   |   VectorEntry {    |                   |                   |
  |                    |                   |     key, embedding,|                   |                   |
  |                    |                   |     metadata,      |                   |                   |
  |                    |                   |     vector_id,     |                   |                   |
  |                    |                   |     version }      |                   |                   |
  |                    |                   |                    |                   |                   |
  |                    |<- Versioned -------|                    |                   |                   |
  |                    |   <VectorEntry>   |                    |                   |                   |
  |                    |                   |                    |                   |                   |
  |                    |-- convert to ---->|                    |                   |                   |
  |                    |   VersionedVector |                    |                   |                   |
  |                    |   Data            |                    |                   |                   |
  |                    |                   |                    |                   |                   |
  |<-- VectorData -----|                   |                    |                   |                   |
  |  Some({embedding,  |                   |                    |                   |                   |
  |   metadata,version,|                   |                    |                   |                   |
  |   timestamp})      |                   |                    |                   |                   |
```

**Steps:**

1. **Handler**: Validates collection name. Calls `primitives.vector.get()`. Converts to `VersionedVectorData` containing embedding, metadata, version, timestamp.
2. **Engine (VectorStore)**: Ensures collection loaded. Creates a snapshot view of storage. Reads the KV key. Deserializes `VectorRecord` from MessagePack. Reads the embedding from the in-memory backend using the `vector_id`. Builds a `VectorEntry` combining both sources.

**Note**: Get reads from **both** KV storage (metadata, version) and in-memory backend (embedding). The `VectorRecord` also stores the embedding for recovery, but the live read comes from the heap.

---

### VectorDelete

```
Client               Handler             Engine (VectorStore)  Backend             Storage
  |                    |                   |                    |                   |
  |-- VectorDelete --->|                   |                    |                   |
  | {branch,coll,key}  |                   |                    |                   |
  |                    |-- validate ------>|                    |                   |
  |                    |                   |                    |                   |
  |                    |                   |-- ensure loaded -->|                   |
  |                    |                   |                    |                   |
  |                    |                   |-- get record ----->|                   |-- read KV ------->|
  |                    |                   |   by kv_key        |                   |                   |
  |                    |                   |                    |                   |                   |
  |                    |                   |   NOT FOUND:       |                   |                   |
  |                    |                   |   return false     |                   |                   |
  |                    |                   |                    |                   |                   |
  |                    |                   |   FOUND:           |                   |                   |
  |                    |                   |   extract vector_id|                   |                   |
  |                    |                   |                    |                   |                   |
  |                    |                   |-- backend.delete ->|                   |                   |
  |                    |                   |   (vector_id)      |-- remove from  -->|                   |
  |                    |                   |                    |   heap             |                   |
  |                    |                   |                    |                   |                   |
  |                    |                   |-- db.transaction ->|                   |-- tombstone ----->|
  |                    |                   |   txn.delete       |                   |   in KV chain     |
  |                    |                   |   (kv_key)         |                   |                   |
  |                    |                   |                    |                   |                   |
  |<-- Output::Bool ---|<- true -----------|                    |                   |                   |
```

**Steps:**

1. **Handler**: Validates collection. Calls `primitives.vector.delete()`. Returns `Output::Bool`.
2. **Engine (VectorStore)**: Ensures collection loaded. Reads the KV record to get the `vector_id`. Deletes from in-memory backend. Deletes from KV storage via `txn.delete()`.

**Dual delete**: Removes from both in-memory heap (immediate effect on search) and persistent KV (durability). Order: backend first, then KV.

---

### VectorSearch

```
Client               Handler             Engine (VectorStore)  Backend             Storage
  |                    |                   |                    |                   |
  |-- VectorSearch --->|                   |                    |                   |
  | {branch,coll,      |                   |                    |                   |
  |  query,k,filter?,  |                   |                    |                   |
  |  metric?}          |                   |                    |                   |
  |                    |-- validate ------>|                    |                   |
  |                    |   convert filter  |                    |                   |
  |                    |   (metric IGNORED)|                    |                   |
  |                    |                   |                    |                   |
  |                    |                   |-- validate dim --->|                   |
  |                    |                   |   query.len() ==   |                   |
  |                    |                   |   config.dim       |                   |
  |                    |                   |                    |                   |
  |                    |                   |-- ensure loaded -->|                   |
  |                    |                   |                    |                   |
  |                    |                   |-- backend.search ->|                   |
  |                    |                   |   (query, k)       |                   |
  |                    |                   |                    |                   |
  |                    |                   |                    |-- for each vec:   |
  |                    |                   |                    |   compute         |
  |                    |                   |                    |   similarity      |
  |                    |                   |                    |   (cosine/eucl/   |
  |                    |                   |                    |    dotprod)        |
  |                    |                   |                    |                   |
  |                    |                   |                    |-- sort by ------->|
  |                    |                   |                    |  (score desc,     |
  |                    |                   |                    |   id asc)         |
  |                    |                   |                    |                   |
  |                    |                   |                    |-- truncate(k) --->|
  |                    |                   |                    |                   |
  |                    |                   |<- Vec<(VectorId, ->|                   |
  |                    |                   |    score)>         |                   |
  |                    |                   |                    |                   |
  |                    |                   |== FOR EACH RESULT =====================|
  |                    |                   |                    |                   |
  |                    |                   |-- load metadata -->|                   |-- read KV ------->|
  |                    |                   |   from KV by id    |                   |                   |
  |                    |                   |                    |                   |                   |
  |                    |                   |-- apply filter --->|                   |                   |
  |                    |                   |   (if specified)   |                   |                   |
  |                    |                   |   skip if no match |                   |                   |
  |                    |                   |                    |                   |                   |
  |                    |                   |-- resolve key ---->|                   |                   |
  |                    |                   |   VectorId -> key  |                   |                   |
  |                    |                   |                    |                   |                   |
  |                    |                   |== END LOOP ================================|               |
  |                    |                   |                    |                   |                   |
  |                    |<- Vec<VectorMatch>-|                    |                   |                   |
  |                    |  [{key,score,meta}]|                    |                   |                   |
  |                    |                   |                    |                   |                   |
  |<-- VectorMatches --|                   |                    |                   |                   |
```

**Steps:**

1. **Handler**: Validates collection. Converts metadata filter. **Ignores** the `metric` parameter (uses collection's configured metric). Calls `primitives.vector.search()`.
2. **Engine (VectorStore)**: Validates query dimension matches collection config. Ensures collection loaded. Calls `backend.search(query, k)`.
3. **Backend (BruteForceBackend)**: Computes similarity for every vector in the collection (O(n)). Sorts by `(score desc, VectorId asc)` for determinism. Truncates to top-k. Returns `Vec<(VectorId, f32)>`.
4. **Post-search**: For each result, loads metadata from KV storage. Applies metadata filter if specified (post-filter, not pre-filter). Resolves `VectorId` back to user key string.

**Distance metrics** (all normalized so higher = more similar):
- **Cosine**: `dot(a,b) / (||a|| * ||b||)` - range [-1, 1]
- **Euclidean**: `1 / (1 + distance(a,b))` - range (0, 1]
- **DotProduct**: `dot(a,b)` - unbounded (assumes pre-normalized vectors)

**Important**: Metadata filtering is **post-filter** - the backend returns k results, then metadata is loaded and filtered. This means fewer than k results may be returned if filters eliminate some matches.

## Storage Format

```
Vector entries:
  TypeTag:         0x10 (Vector)
  Key format:      Namespace + TypeTag::Vector + "collection/key".as_bytes()
  Value format:    Value::Bytes(MessagePack) containing VectorRecord

Collection configs:
  TypeTag:         0x12 (VectorConfig)
  Key format:      Namespace + TypeTag::VectorConfig + collection_name.as_bytes()
  Value format:    Value::Bytes(MessagePack) containing CollectionRecord
```

### VectorRecord (stored as MessagePack)

```
VectorRecord {
    vector_id:   u64                    // Internal ID for backend
    embedding:   Vec<f32>               // Full embedding (for recovery)
    metadata:    Option<serde_json::Value>
    version:     u64                    // Per-vector counter
    created_at:  u64                    // Microseconds
    updated_at:  u64                    // Microseconds
    source_ref:  Option<EntityRef>      // Cross-reference to source entity
}
```

### CollectionRecord (stored as MessagePack)

```
CollectionRecord {
    config:     VectorConfigSerde       // { dimension, metric }
    created_at: u64                     // Microseconds
}
```

### In-Memory Backend State

```
BruteForceBackend {
    heap: VectorHeap {
        data:       BTreeMap<VectorId, Vec<f32>>  // Embeddings
        dimension:  usize
        metric:     DistanceMetric
        next_id:    u64                            // Monotonic ID allocator
        free_slots: Vec<usize>                     // Reusable IDs from deletions
    }
}
```

## Transaction Behavior

| Aspect | Behavior |
|--------|----------|
| Transactional | **No** - bypasses Session transaction layer |
| Isolation | None (direct writes) |
| Engine transactions | Used internally for KV persistence only |
| In-memory consistency | Backend writes are immediate |
| Crash recovery | Rebuild in-memory index from KV records on restart |
| Search metric | Fixed per collection at creation time |

## Consistency Notes

- Vector is the only primitive with **dual storage**: in-memory heap (for search) + persistent KV (for durability). All other primitives go through the standard transaction -> storage path.
- Vector operations are **non-transactional** at the Session level. Even within an active Session transaction, vector operations execute immediately and are not rolled back on `TxnRollback`. This is a design choice for performance.
- The `metric` parameter on `VectorSearch` is **ignored** - the collection's configured metric (set at creation) is always used. The parameter exists for API compatibility.
- **Auto-creation**: `VectorUpsert` auto-creates collections with Cosine metric and the dimension of the first vector. Other primitives do not auto-create their containers.
- Unlike KV (which uses `Version::Txn`), Vector uses `Version::Counter` per-vector, similar to State. But unlike State, Vector has no CAS operation.
- **Post-filter search**: Metadata filtering happens after the brute-force search returns top-k results. This can return fewer than k results. Pre-filtering would require passing filters into the backend, which the current interface doesn't support.
- The `VectorId` is an internal monotonic counter per collection. It's separate from the user-provided key string. The mapping between `VectorId` and user key is maintained through the KV-stored `VectorRecord`.
- Collection names starting with `_` are reserved for internal use and rejected by the handler's `validate_not_internal_collection()` check.
