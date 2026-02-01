# KV Primitive - Architecture Reference

## Overview

The KV (Key-Value) primitive provides a simple associative store where string keys map to typed values. It supports put, get, list, and delete operations with full MVCC versioning.

- **Version semantics**: `Version::Txn(u64)` - globally monotonic transaction IDs assigned by the transaction coordinator
- **Key construction**: `Key { namespace: Namespace::for_branch(branch_id), type_tag: TypeTag::KV (0x01), user_key: key.as_bytes() }`
- **Storage format**: Values stored directly as `strata_core::value::Value` (no wrapper struct)
- **Transactional**: Yes - all operations run inside engine transactions; supports Session-level multi-command transactions with read-your-writes

## Layer Architecture

```
+------------------------------------------------------------------+
|  CLIENT                                                          |
|  Command::KvPut { branch, key, value }                           |
+------------------------------------------------------------------+
        |
        v
+------------------------------------------------------------------+
|  SESSION (session.rs)                                            |
|  Routes to executor OR active transaction context                |
|  - If txn active: execute_in_txn() with ctx.get()/ctx.put()     |
|  - If no txn: executor.execute(cmd)                              |
+------------------------------------------------------------------+
        |
        v
+------------------------------------------------------------------+
|  EXECUTOR (executor.rs)                                          |
|  Stateless dispatcher holding Arc<Primitives>                    |
|  Dispatches to: crate::handlers::kv::kv_put(...)                |
+------------------------------------------------------------------+
        |
        v
+------------------------------------------------------------------+
|  HANDLER (handlers/kv.rs + bridge.rs)                            |
|  1. to_core_branch_id(&branch) -> core::BranchId                |
|  2. validate_key(&key) -> check empty/NUL/_strata/1024          |
|  3. Call primitives.kv.put(&branch_id, &key, value)              |
|  4. extract_version(&version) -> u64                             |
+------------------------------------------------------------------+
        |
        v
+------------------------------------------------------------------+
|  ENGINE PRIMITIVE (primitives/kv.rs - KVStore)                   |
|  1. Build storage Key: Key::new_kv(namespace, user_key)          |
|  2. db.transaction(branch_id, |txn| { txn.put(key, value) })    |
|  3. Return Version::Txn(commit_version)                          |
+------------------------------------------------------------------+
        |
        v
+------------------------------------------------------------------+
|  TRANSACTION (concurrency crate - TransactionContext)             |
|  1. Allocate txn_id from global counter                          |
|  2. Write: add to write_set[key] = value                         |
|  3. Read: check write_set -> delete_set -> snapshot              |
|  4. On commit: OCC validation, persist to storage                |
+------------------------------------------------------------------+
        |
        v
+------------------------------------------------------------------+
|  STORAGE (storage/sharded.rs - ShardedStore)                     |
|  DashMap<Key, VersionChain>                                      |
|  VersionChain: VecDeque<StoredValue> (newest-first)              |
|  put: push_front(StoredValue { value, version, timestamp })      |
|  get: find first version <= snapshot_version                     |
+------------------------------------------------------------------+
```

## Operation Flows

### KvPut

```
Client                Session              Handler             Engine              Transaction          Storage
  |                     |                    |                   |                    |                   |
  |-- KvPut ----------->|                    |                   |                    |                   |
  |  {branch,key,value} |                    |                   |                    |                   |
  |                     |                    |                   |                    |                   |
  |                     |-- (no txn) ------->|                   |                    |                   |
  |                     |   executor.execute |                   |                    |                   |
  |                     |                    |                   |                    |                   |
  |                     |                    |-- validate ------>|                    |                   |
  |                     |                    |   key format      |                    |                   |
  |                     |                    |   branch -> UUID  |                    |                   |
  |                     |                    |                   |                    |                   |
  |                     |                    |                   |-- begin txn ------>|                   |
  |                     |                    |                   |   allocate txn_id  |                   |
  |                     |                    |                   |                    |                   |
  |                     |                    |                   |-- txn.put -------->|                   |
  |                     |                    |                   |   Key::new_kv()    |                   |
  |                     |                    |                   |   + Value          |-- write_set ----->|
  |                     |                    |                   |                    |   add to          |
  |                     |                    |                   |                    |                   |
  |                     |                    |                   |-- commit --------->|-- persist ------->|
  |                     |                    |                   |                    |   OCC validate    |
  |                     |                    |                   |                    |   push to chain   |
  |                     |                    |                   |                    |                   |
  |                     |                    |<-- Version::Txn --|                    |                   |
  |                     |                    |   (commit_version)|                    |                   |
  |                     |                    |                   |                    |                   |
  |<-- Output::Version -|<-- extract u64 ----|                   |                    |                   |
  |    (u64)            |                    |                   |                    |                   |
```

**Steps:**

1. **Session**: Checks if a transaction is active. If yes, routes to `execute_in_txn()` which uses `Transaction::new(ctx, ns)` then `txn.kv_put()`. If no, delegates to `executor.execute(cmd)`.
2. **Executor**: Unwraps the resolved branch, dispatches to `handlers::kv::kv_put()`.
3. **Handler**: Converts executor `BranchId` to `core::BranchId` via `to_core_branch_id()`. Validates the key (non-empty, no NUL bytes, no `_strata/` prefix, max 1024 bytes). Calls `primitives.kv.put()`.
4. **Engine (KVStore)**: Constructs the composite storage key with `Key::new_kv(Namespace::for_branch(branch_id), user_key)`. Opens a transaction via `db.transaction_with_version()`, calls `txn.put(storage_key, value)`.
5. **Transaction**: Allocates a globally monotonic `txn_id`. Adds `(key, value)` to the write set. On commit, performs OCC validation (checks read set for conflicts), then persists all writes to storage.
6. **Storage**: Pushes a new `StoredValue` to the front of the key's `VersionChain` in the `DashMap`.

**Error paths:**
- Invalid key format -> `Error::InvalidInput` at handler layer
- Branch not found -> propagated as `StrataError`
- OCC conflict on commit -> `Error::TransactionConflict`

---

### KvGet

```
Client                Session              Handler             Engine              Transaction          Storage
  |                     |                    |                   |                    |                   |
  |-- KvGet ----------->|                    |                   |                    |                   |
  |  {branch, key}      |                    |                   |                    |                   |
  |                     |                    |                   |                    |                   |
  |                     |-- (txn active?) -->|                   |                    |                   |
  |                     |                    |                   |                    |                   |
  |                     |                    |-- validate ------>|                    |                   |
  |                     |                    |   key + branch    |                    |                   |
  |                     |                    |                   |                    |                   |
  |                     |                    |                   |-- begin txn ------>|                   |
  |                     |                    |                   |                    |                   |
  |                     |                    |                   |-- txn.get -------->|                   |
  |                     |                    |                   |   Key::new_kv()    |-- 1. write_set -->|
  |                     |                    |                   |                    |-- 2. delete_set   |
  |                     |                    |                   |                    |-- 3. snapshot --->|
  |                     |                    |                   |                    |   find version <= |
  |                     |                    |                   |                    |   snapshot_ver    |
  |                     |                    |                   |                    |                   |
  |                     |                    |<-- Option<Value> -|<-- Option<Value> --|<-- StoredValue ---|
  |                     |                    |                   |                    |   (if found)      |
  |                     |                    |                   |                    |                   |
  |<-- Output::Maybe ---|<------------------|                   |                    |                   |
  |    Option<Value>    |                    |                   |                    |                   |
```

**Steps:**

1. **Session**: If transaction active, routes to `execute_in_txn()` which calls `ctx.get(&full_key)` directly (bypasses engine primitive). If no transaction, delegates to executor.
2. **Handler**: Converts branch, validates key, calls `primitives.kv.get()`.
3. **Engine (KVStore)**: Constructs key, opens read-only transaction, calls `txn.get(&storage_key)`.
4. **Transaction**: Read path is: (1) check write set, (2) check delete set (return None if found), (3) read from snapshot at `snapshot_version`. Records key in read set for conflict detection.
5. **Storage**: Scans `VersionChain` for first entry with version <= snapshot_version. Returns `StoredValue.value` or None.

**Key difference in session-transaction path**: When a Session transaction is active, `KvGet` is routed through `ctx.get(&Key::new_kv(ns, &key))` directly, which provides read-your-writes semantics from the transaction's write set.

---

### KvList

```
Client                Session              Handler             Engine              Transaction          Storage
  |                     |                    |                   |                    |                   |
  |-- KvList ---------->|                    |                   |                    |                   |
  |  {branch, prefix?}  |                    |                   |                    |                   |
  |                     |                    |                   |                    |                   |
  |                     |-- dispatch ------->|                   |                    |                   |
  |                     |                    |-- validate ------>|                    |                   |
  |                     |                    |   prefix (if set) |                    |                   |
  |                     |                    |                   |                    |                   |
  |                     |                    |                   |-- begin txn ------>|                   |
  |                     |                    |                   |                    |                   |
  |                     |                    |                   |-- scan_prefix ---->|                   |
  |                     |                    |                   |   Key::new_kv(ns,  |-- scan DashMap -->|
  |                     |                    |                   |     prefix || "")  |   merge write_set |
  |                     |                    |                   |                    |   exclude deletes |
  |                     |                    |                   |                    |                   |
  |                     |                    |                   |<-- Vec<(Key,Val)> -|<-- matched keys --|
  |                     |                    |                   |                    |                   |
  |                     |                    |                   |-- filter_map ----->|                   |
  |                     |                    |                   |   key.user_key_    |                   |
  |                     |                    |                   |   string()         |                   |
  |                     |                    |                   |                    |                   |
  |<-- Output::Keys ----|<-- Vec<String> ----|<-- Vec<String> ---|                    |                   |
```

**Steps:**

1. **Session**: If transaction active, routes to `execute_in_txn()` which calls `ctx.scan_prefix(&prefix_key)` directly, then extracts user key strings. If no transaction, delegates to executor.
2. **Handler**: Validates prefix if present (same rules as key validation). Calls `primitives.kv.list()`.
3. **Engine (KVStore)**: Constructs scan prefix key `Key::new_kv(ns, prefix || "")`. Opens transaction, calls `txn.scan_prefix()`. Extracts user key strings via `key.user_key_string()`.
4. **Transaction**: Scans snapshot for all keys matching the prefix. Merges with write set entries. Excludes keys in delete set.
5. **Storage**: Iterates DashMap entries matching the prefix. For each, returns the latest non-tombstone version at snapshot_version.

**Returns**: `Output::Keys(Vec<String>)` - sorted list of user key strings (not values).

---

### KvDelete

```
Client                Session              Handler             Engine              Transaction          Storage
  |                     |                    |                   |                    |                   |
  |-- KvDelete -------->|                    |                   |                    |                   |
  |  {branch, key}      |                    |                   |                    |                   |
  |                     |                    |                   |                    |                   |
  |                     |-- dispatch ------->|                   |                    |                   |
  |                     |                    |-- validate ------>|                    |                   |
  |                     |                    |                   |                    |                   |
  |                     |                    |                   |-- begin txn ------>|                   |
  |                     |                    |                   |                    |                   |
  |                     |                    |                   |-- txn.get -------->|                   |
  |                     |                    |                   |   (check exists)   |-- read chain ---->|
  |                     |                    |                   |                    |                   |
  |                     |                    |                   |-- txn.delete ----->|                   |
  |                     |                    |                   |   (if existed)     |-- tombstone ----->|
  |                     |                    |                   |                    |   write to chain  |
  |                     |                    |                   |                    |                   |
  |                     |                    |                   |-- commit --------->|-- persist ------->|
  |                     |                    |                   |                    |                   |
  |<-- Output::Bool ----|<-- bool -----------|<-- bool ---------|                    |                   |
  |    (existed?)       |                    |                   |                    |                   |
```

**Steps:**

1. **Session**: If transaction active, routes to `execute_in_txn()` which calls `ctx.exists(&full_key)` then `ctx.delete(full_key)` directly. If no transaction, delegates to executor.
2. **Handler**: Validates key, calls `primitives.kv.delete()`.
3. **Engine (KVStore)**: Constructs key. Opens transaction. First checks if key exists via `txn.get()`. If it exists, calls `txn.delete()`. Returns boolean indicating whether the key existed.
4. **Transaction**: On delete, adds key to `delete_set` and removes from `write_set` if present. On commit, writes a tombstone marker to storage.
5. **Storage**: Tombstone is pushed to the front of the `VersionChain`, making subsequent reads at this version return None.

**Returns**: `Output::Bool(true)` if the key existed before deletion, `Output::Bool(false)` if it didn't exist.

## Storage Format

```
TypeTag:           0x01 (KV)
Key format:        Namespace::for_branch(branch_id) + TypeTag::KV + user_key_bytes
Value format:      strata_core::value::Value (stored directly, no wrapper)
Version:           Version::Txn(commit_version) assigned by transaction coordinator
Tombstone:         Sentinel value in VersionChain marks deletion
MVCC:              Multiple versions per key in newest-first VecDeque
```

## Transaction Behavior

| Aspect | Behavior |
|--------|----------|
| Isolation | Snapshot isolation - reads see consistent point-in-time |
| Concurrency control | OCC (Optimistic Concurrency Control) with read-set tracking |
| Read-your-writes | Yes - write set checked before snapshot |
| Multi-command txn | Yes - via Session layer with TxnBegin/TxnCommit |
| Conflict detection | Read set validated against concurrent commits |
| Retry | Single attempt by default (no retry config) |

## Consistency Notes

- KV is the simplest primitive - values are stored directly without wrapping
- Unlike State (which wraps in `State { value, version, updated_at }`), KV stores raw `Value`
- Unlike JSON (which uses MessagePack), KV uses the native `Value` type
- Unlike Event (which uses `Version::Sequence`), KV uses `Version::Txn` from the transaction coordinator
- KV version numbers are global transaction IDs, not per-key counters
