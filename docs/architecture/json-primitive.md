# JSON Primitive - Architecture Reference

## Overview

The JSON primitive provides document storage with JSONPath-based read and write operations. Each document is a named JSON value that can be queried and mutated at arbitrary paths. Documents are stored as MessagePack-encoded `JsonDoc` structs with per-document version counters.

- **Version semantics**: `Version::Counter(u64)` - per-document counter starting at 1, incremented on every mutation (`doc.touch()`)
- **Key construction**: `Key { namespace: Namespace::for_branch(branch_id), type_tag: TypeTag::Json (0x11), user_key: doc_id.as_bytes() }`
- **Storage format**: `Value::Bytes(MessagePack)` wrapping `JsonDoc { id, value, version, created_at, updated_at }`
- **Transactional**: Yes - all operations run inside engine transactions; supports Session-level multi-command transactions

## Layer Architecture

```
+------------------------------------------------------------------+
|  CLIENT                                                          |
|  Command::JsonSet { branch, key, path, value }                   |
+------------------------------------------------------------------+
        |
        v
+------------------------------------------------------------------+
|  SESSION (session.rs)                                            |
|  Routes to executor OR active transaction context                |
|  - If txn active + root path: ctx.get() direct read              |
|  - If txn active + sub-path: Transaction.json_get_path()         |
|  - If no txn: executor.execute(cmd)                              |
+------------------------------------------------------------------+
        |
        v
+------------------------------------------------------------------+
|  EXECUTOR (executor.rs)                                          |
|  Dispatches to: crate::handlers::json::json_set(...)             |
+------------------------------------------------------------------+
        |
        v
+------------------------------------------------------------------+
|  HANDLER (handlers/json.rs + bridge.rs)                          |
|  1. to_core_branch_id(&branch) -> core::BranchId                |
|  2. validate_key(&key)                                           |
|  3. parse_path(&path) -> JsonPath                                |
|  4. value_to_json(value) -> JsonValue                            |
|  5. Auto-create document if needed:                              |
|     - !exists + root: json.create(branch, key, json_value)       |
|     - !exists + sub-path: create empty {}, then set at path      |
|     - exists: json.set(branch, key, &path, json_value)           |
|  6. extract_version(&version) -> u64                             |
+------------------------------------------------------------------+
        |
        v
+------------------------------------------------------------------+
|  ENGINE PRIMITIVE (primitives/json.rs - JsonStore)               |
|  1. Validate path (length limit) and value (size, depth, arrays) |
|  2. db.transaction(branch_id, |txn| {                            |
|       - txn.get(key): load existing JsonDoc                      |
|       - Deserialize from MessagePack                             |
|       - Apply mutation at path (set_at_path / delete_at_path)    |
|       - doc.touch(): version++, updated_at = now                 |
|       - Serialize back to MessagePack                            |
|       - txn.put(key, Value::Bytes(msgpack))                      |
|     })                                                           |
|  3. Return Version::Counter(doc.version)                         |
+------------------------------------------------------------------+
        |
        v
+------------------------------------------------------------------+
|  TRANSACTION / STORAGE (same as other primitives)                |
|  Single write per mutation: Key::new_json(ns, doc_id) -> bytes   |
+------------------------------------------------------------------+
```

## Operation Flows

### JsonSet

```
Client               Handler             Engine (JsonStore)   Transaction          Storage
  |                    |                   |                    |                   |
  |-- JsonSet -------->|                   |                    |                   |
  | {branch, key,      |                   |                    |                   |
  |  path, value}      |                   |                    |                   |
  |                    |                   |                    |                   |
  |                    |-- validate ------>|                    |                   |
  |                    |   key, parse path |                    |                   |
  |                    |   value_to_json   |                    |                   |
  |                    |                   |                    |                   |
  |                    |-- json.exists? -->|                    |                   |
  |                    |                   |                    |                   |
  |                    |   NOT EXISTS +    |                    |                   |
  |                    |   ROOT PATH:      |                    |                   |
  |                    |-- json.create --->|-- begin txn ------>|                   |
  |                    |                   |   check !exists    |                   |
  |                    |                   |   JsonDoc::new()   |                   |
  |                    |                   |   serialize msgpk  |                   |
  |                    |                   |-- txn.put -------->|-- write_set ----->|
  |                    |                   |-- commit --------->|-- persist ------->|
  |                    |                   |                    |                   |
  |                    |   NOT EXISTS +    |                    |                   |
  |                    |   SUB PATH:       |                    |                   |
  |                    |-- json.create({}) |   (empty object)   |                   |
  |                    |-- json.set(path)  |   (then set at path)|                  |
  |                    |                   |                    |                   |
  |                    |   EXISTS:         |                    |                   |
  |                    |-- json.set ------>|-- begin txn ------>|                   |
  |                    |                   |                    |                   |
  |                    |                   |-- txn.get -------->|-- read chain ---->|
  |                    |                   |   Key::new_json    |                   |
  |                    |                   |                    |                   |
  |                    |                   |<- Value::Bytes ----|                   |
  |                    |                   |   (msgpack)        |                   |
  |                    |                   |                    |                   |
  |                    |                   |-- deserialize ---->|                   |
  |                    |                   |   JsonDoc from     |                   |
  |                    |                   |   MessagePack      |                   |
  |                    |                   |                    |                   |
  |                    |                   |-- set_at_path ---->|                   |
  |                    |                   |   navigate path,   |                   |
  |                    |                   |   set value        |                   |
  |                    |                   |                    |                   |
  |                    |                   |-- doc.touch() ---->|                   |
  |                    |                   |   version++        |                   |
  |                    |                   |   updated_at=now   |                   |
  |                    |                   |                    |                   |
  |                    |                   |-- serialize ------>|                   |
  |                    |                   |   JsonDoc to       |                   |
  |                    |                   |   MessagePack      |                   |
  |                    |                   |                    |                   |
  |                    |                   |-- txn.put -------->|-- write_set ----->|
  |                    |                   |   Value::Bytes     |                   |
  |                    |                   |                    |                   |
  |                    |                   |-- commit --------->|-- persist ------->|
  |                    |                   |                    |                   |
  |<-- Output::Version-|<- extract u64 ----|<- Counter(ver) ----|                   |
```

**Steps:**

1. **Handler**: Converts branch, validates key, parses path string to `JsonPath`, converts executor `Value` to `JsonValue`. Checks if document exists:
   - **Not exists + root path** (`"$"` or `""`): Calls `json.create()` to create a new document with the value as the root.
   - **Not exists + sub-path**: Creates an empty `{}` document first, then calls `json.set()` at the sub-path.
   - **Exists**: Calls `json.set()` directly.
2. **Engine (JsonStore)**: Validates path (length limit) and value (max 16MB size, max 100 depth, max 1M array elements). Opens transaction. Loads existing document from MessagePack bytes. Applies `set_at_path()` to navigate the JSON tree and set the value. Calls `doc.touch()` to increment version and update timestamp. Re-serializes to MessagePack. Writes back.

**Path parsing** (`bridge.rs::parse_path`):
- `""` or `"$"` -> root path (empty segments)
- `"$"` prefix stripped, then parsed: `"user.name"` -> `[Key("user"), Key("name")]`
- `"items[0]"` -> `[Key("items"), Index(0)]`

---

### JsonGet

```
Client               Handler             Engine (JsonStore)   Transaction          Storage
  |                    |                   |                    |                   |
  |-- JsonGet -------->|                   |                    |                   |
  | {branch, key, path}|                   |                    |                   |
  |                    |                   |                    |                   |
  |                    |-- validate ------>|                    |                   |
  |                    |   key, parse path |                    |                   |
  |                    |                   |                    |                   |
  |                    |                   |-- begin txn ------>|                   |
  |                    |                   |                    |                   |
  |                    |                   |-- txn.get -------->|-- read chain ---->|
  |                    |                   |   Key::new_json    |                   |
  |                    |                   |                    |                   |
  |                    |                   |   NOT FOUND:       |                   |
  |                    |                   |   return None      |                   |
  |                    |                   |                    |                   |
  |                    |                   |   FOUND:           |                   |
  |                    |                   |<- Value::Bytes ----|                   |
  |                    |                   |                    |                   |
  |                    |                   |-- deserialize ---->|                   |
  |                    |                   |   JsonDoc from     |                   |
  |                    |                   |   MessagePack      |                   |
  |                    |                   |                    |                   |
  |                    |                   |-- get_at_path ---->|                   |
  |                    |                   |   navigate to path |                   |
  |                    |                   |   return value     |                   |
  |                    |                   |   (or None if not  |                   |
  |                    |                   |    found at path)  |                   |
  |                    |                   |                    |                   |
  |                    |<- Option<JsonVal> -|                    |                   |
  |                    |                   |                    |                   |
  |                    |-- json_to_value ->|                    |                   |
  |                    |   JsonValue ->    |                    |                   |
  |                    |   executor Value  |                    |                   |
  |                    |                   |                    |                   |
  |<-- Output::Maybe --|                   |                    |                   |
  |   Option<Value>    |                   |                    |                   |
```

**Steps:**

1. **Handler**: Validates key, parses path. Calls `primitives.json.get()`. Converts returned `JsonValue` back to executor `Value` via `json_to_value()`.
2. **Engine (JsonStore)**: Validates path. Opens transaction. Reads key. If not found, returns `None`. If found, deserializes `JsonDoc` from MessagePack. Navigates to `path` via `get_at_path()`. Returns the value at that path (or `None` if the path doesn't exist in the document).

**Session-transaction path**: When a Session transaction is active, `JsonGet` has two sub-paths:
- **Root path** (`"$"` or `""`): Uses `ctx.get(&full_key)` directly, deserializes `Value::String` JSON (note: different format than engine's MessagePack for Session reads)
- **Sub-path**: Uses `Transaction::new(ctx, ns).json_get_path()` which handles the path navigation

---

### JsonDelete

```
Client               Handler             Engine (JsonStore)   Transaction          Storage
  |                    |                   |                    |                   |
  |-- JsonDelete ----->|                   |                    |                   |
  | {branch, key, path}|                   |                    |                   |
  |                    |                   |                    |                   |
  |                    |-- validate ------>|                    |                   |
  |                    |   key, parse path |                    |                   |
  |                    |                   |                    |                   |
  |                    |   ROOT PATH:      |                    |                   |
  |                    |-- json.destroy -->|-- begin txn ------>|                   |
  |                    |                   |   txn.get (check)  |-- read chain ---->|
  |                    |                   |   txn.delete ------>|-- delete_set ---->|
  |                    |                   |   commit ---------> |-- tombstone ---->|
  |                    |                   |                    |                   |
  |                    |<-- bool (existed) -|                    |                   |
  |<-- Output::Uint ---|   1 or 0          |                    |                   |
  |                    |                   |                    |                   |
  |                    |   SUB PATH:       |                    |                   |
  |                    |-- json.delete_at  |                    |                   |
  |                    |   _path() ------->|-- begin txn ------>|                   |
  |                    |                   |   txn.get --------> |-- read chain --->|
  |                    |                   |   deserialize       |                   |
  |                    |                   |   delete_at_path    |                   |
  |                    |                   |   doc.touch()       |                   |
  |                    |                   |   serialize         |                   |
  |                    |                   |   txn.put ---------> |-- write_set --->|
  |                    |                   |   commit ----------> |-- persist ----->|
  |                    |                   |                    |                   |
  |<-- Output::Uint ---|<---- 1 -----------|                    |                   |
```

**Steps:**

1. **Handler**: Validates key, parses path. Two paths:
   - **Root path** (`"$"` or `""`): Calls `json.destroy()` which deletes the entire document. Returns `Output::Uint(1)` if existed, `Output::Uint(0)` if not.
   - **Sub-path**: Calls `json.delete_at_path()` which loads the document, removes the value at the path, increments version, and writes back. Returns `Output::Uint(1)`.
2. **Engine (JsonStore) - destroy**: Opens transaction, checks existence, calls `txn.delete()`. Returns boolean.
3. **Engine (JsonStore) - delete_at_path**: Opens transaction, loads document, applies `delete_at_path()` to remove the path from the JSON tree, calls `doc.touch()`, re-serializes, writes back.

**Session-transaction path**: `JsonDelete` in session uses `Transaction::new(ctx, ns).json_delete()` which calls `txn.delete()` on the key directly (destroys entire document).

---

### JsonList

```
Client               Handler             Engine (JsonStore)   Transaction          Storage
  |                    |                   |                    |                   |
  |-- JsonList ------->|                   |                    |                   |
  | {branch, prefix?,  |                   |                    |                   |
  |  cursor?, limit}   |                   |                    |                   |
  |                    |-- validate ------>|                    |                   |
  |                    |                   |                    |                   |
  |                    |                   |-- begin txn ------>|                   |
  |                    |                   |                    |                   |
  |                    |                   |-- scan_prefix ---->|-- scan DashMap -->|
  |                    |                   |  Key::new_json_    |   merge write_set |
  |                    |                   |  prefix(ns)        |   exclude deletes |
  |                    |                   |                    |                   |
  |                    |                   |<- Vec<(Key,Val)> --|                   |
  |                    |                   |                    |                   |
  |                    |                   |-- for each entry:  |                   |
  |                    |                   |   deserialize doc  |                   |
  |                    |                   |   apply cursor     |                   |
  |                    |                   |   apply prefix     |                   |
  |                    |                   |   collect doc_ids  |                   |
  |                    |                   |   up to limit+1    |                   |
  |                    |                   |                    |                   |
  |                    |                   |-- if > limit:      |                   |
  |                    |                   |   next_cursor =    |                   |
  |                    |                   |   last doc_id      |                   |
  |                    |                   |                    |                   |
  |<-- JsonListResult -|<- {keys, cursor} -|                    |                   |
```

**Steps:**

1. **Handler**: Converts branch. Calls `primitives.json.list()` with prefix, cursor, and limit. Returns `Output::JsonListResult { keys, cursor }`.
2. **Engine (JsonStore)**: Opens transaction. Scans all JSON keys in the branch via `scan_prefix(Key::new_json_prefix(ns))`. For each entry:
   - Deserializes `JsonDoc` from MessagePack
   - Skips entries until past the cursor (if provided)
   - Applies prefix filter on `doc.id`
   - Collects up to `limit + 1` doc IDs
   - If more than `limit`, pops the last and uses the second-to-last as `next_cursor`
3. **Pagination**: Cursor-based using document IDs. The cursor is the ID of the last document in the current page.

**Returns**: `Output::JsonListResult { keys: Vec<String>, cursor: Option<String> }`

## Storage Format

```
TypeTag:           0x11 (Json)
Key format:        Namespace::for_branch(branch_id) + TypeTag::Json + doc_id_bytes
Value format:      Value::Bytes(MessagePack) containing JsonDoc
Version in chain:  Version::Txn(commit_version) from transaction coordinator
Version in doc:    u64 counter, incremented by doc.touch()
```

### JsonDoc Struct (stored as MessagePack)

```
JsonDoc {
    id:         String     // Document identifier (user key)
    value:      JsonValue  // Newtype wrapper around serde_json::Value
    version:    u64        // Document-level version counter (1, 2, 3, ...)
    created_at: u64        // Creation timestamp (microseconds)
    updated_at: u64        // Last modification (microseconds)
}
```

### JsonValue Validation Limits

```
Max document size:     16 MB
Max nesting depth:     100 levels
Max array elements:    1,000,000
```

### Path Syntax

```
"$" or ""                -> root (entire document)
"user.name"              -> Key("user") + Key("name")
"items[0]"               -> Key("items") + Index(0)
"user.address[0].city"   -> Key("user") + Key("address") + Index(0) + Key("city")
```

## Transaction Behavior

| Aspect | Behavior |
|--------|----------|
| Isolation | Snapshot isolation |
| Concurrency control | OCC (single attempt, no retry) |
| Writes per mutation | 1 (entire document re-serialized) |
| Serialization | MessagePack (binary, compact) |
| Read-your-writes | Yes within transaction |
| Multi-command txn | Yes via Session |

## Consistency Notes

- JSON uses **MessagePack** serialization (`rmp_serde`), unlike State which uses JSON strings and KV which stores values directly. This makes JSON documents more compact in storage but requires encode/decode on every access.
- JSON has **auto-creation** semantics in the handler: setting a path on a non-existent document creates the document. This is unlike State's `init` which is a separate explicit operation.
- JSON has **path-based operations** for fine-grained reads and writes. Other primitives operate on whole values.
- JSON `delete` has two modes: root path destroys the entire document (tombstone), sub-path removes a field/element and writes back the modified document.
- JSON uses `Version::Counter(u64)` like State, but the counter is per-document rather than per-cell. The version is part of the `JsonDoc` struct and incremented by `doc.touch()`.
- JSON `list` supports **cursor-based pagination**, unlike KV `list` which returns all keys at once.
- The **Session transaction path** for JSON reads differs by path type: root path reads use `ctx.get()` directly (fast path), while sub-path reads create a `Transaction` wrapper for path navigation logic.
