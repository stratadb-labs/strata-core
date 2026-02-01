# State Primitive - Architecture Reference

## Overview

The State primitive provides named cells with compare-and-swap (CAS) semantics. Each cell holds a single value with a per-entity version counter that increments on every write. CAS enables safe concurrent updates by requiring the caller to specify the expected version.

- **Version semantics**: `Version::Counter(u64)` - per-cell counter starting at 1, incremented on every write
- **Key construction**: `Key { namespace: Namespace::for_branch(branch_id), type_tag: TypeTag::State (0x03), user_key: cell_name.as_bytes() }`
- **Storage format**: `Value::String(JSON)` wrapping `State { value, version, updated_at }` struct
- **Transactional**: Yes - all operations run inside engine transactions; supports Session-level multi-command transactions

## Layer Architecture

```
+------------------------------------------------------------------+
|  CLIENT                                                          |
|  Command::StateInit { branch, cell, value }                      |
+------------------------------------------------------------------+
        |
        v
+------------------------------------------------------------------+
|  SESSION (session.rs)                                            |
|  Routes to executor OR active transaction context                |
|  - If txn active: Transaction::new(ctx, ns).state_init()         |
|  - If no txn: executor.execute(cmd)                              |
+------------------------------------------------------------------+
        |
        v
+------------------------------------------------------------------+
|  EXECUTOR (executor.rs)                                          |
|  Dispatches to: crate::handlers::state::state_init(...)          |
+------------------------------------------------------------------+
        |
        v
+------------------------------------------------------------------+
|  HANDLER (handlers/state.rs + bridge.rs)                         |
|  1. to_core_branch_id(&branch) -> core::BranchId                |
|  2. validate_key(&cell) -> check empty/NUL/_strata/1024         |
|  3. Call primitives.state.init(&branch_id, &cell, value)         |
|  4. extract_version(&version) -> u64                             |
+------------------------------------------------------------------+
        |
        v
+------------------------------------------------------------------+
|  ENGINE PRIMITIVE (primitives/state.rs - StateCell)              |
|  1. Build Key::new_state(namespace, cell_name)                   |
|  2. db.transaction(branch_id, |txn| {                            |
|       - txn.get(key): check if exists                            |
|       - If exists: return existing version (idempotent)          |
|       - If not: State::new(value) with Counter(1)                |
|       - to_stored_value(&state) -> Value::String(JSON)           |
|       - txn.put(key, serialized_state)                           |
|     })                                                           |
|  3. Return Versioned<Version>                                    |
+------------------------------------------------------------------+
        |
        v
+------------------------------------------------------------------+
|  TRANSACTION (concurrency crate)                                 |
|  Single write: Key::new_state(ns, cell) -> State JSON            |
|  OCC validation on commit                                        |
+------------------------------------------------------------------+
        |
        v
+------------------------------------------------------------------+
|  STORAGE (storage/sharded.rs)                                    |
|  DashMap<Key, VersionChain>                                      |
|  State is a single versioned value per cell key                  |
|  Multiple versions retained for history (readv)                  |
+------------------------------------------------------------------+
```

## Operation Flows

### StateInit (idempotent creation)

```
Client               Handler             Engine (StateCell)   Transaction          Storage
  |                    |                   |                    |                   |
  |-- StateInit ------>|                   |                    |                   |
  | {branch,cell,val}  |                   |                    |                   |
  |                    |                   |                    |                   |
  |                    |-- validate ------>|                    |                   |
  |                    |   cell name       |                    |                   |
  |                    |   branch->UUID    |                    |                   |
  |                    |                   |                    |                   |
  |                    |                   |-- begin txn ------>|                   |
  |                    |                   |                    |                   |
  |                    |                   |-- txn.get -------->|-- read chain ---->|
  |                    |                   |   Key::new_state   |                   |
  |                    |                   |   (ns, cell)       |                   |
  |                    |                   |                    |                   |
  |                    |                   |   IF EXISTS:       |                   |
  |                    |                   |   deserialize ->   |                   |
  |                    |                   |   State struct     |                   |
  |                    |                   |   return existing  |                   |
  |                    |                   |   version (noop)   |                   |
  |                    |                   |                    |                   |
  |                    |                   |   IF NOT EXISTS:   |                   |
  |                    |                   |   State::new(val)  |                   |
  |                    |                   |   version=Ctr(1)   |                   |
  |                    |                   |   to_stored_value  |                   |
  |                    |                   |   -> JSON string   |                   |
  |                    |                   |                    |                   |
  |                    |                   |-- txn.put -------->|-- write_set ----->|
  |                    |                   |                    |                   |
  |                    |                   |-- commit --------->|-- persist ------->|
  |                    |                   |                    |                   |
  |<-- Output::Version-|<- extract u64 ----|<- Versioned -------|                   |
  |   Counter(1) or    |                   |   <Version>        |                   |
  |   existing counter |                   |                    |                   |
```

**Steps:**

1. **Handler**: Converts branch, validates cell name, calls `primitives.state.init()`.
2. **Engine (StateCell)**: Constructs key. Opens transaction. Checks if cell exists.
   - **If exists**: Deserializes the stored `State` struct, returns its existing `version`. No write occurs. This makes `init` idempotent.
   - **If not exists**: Creates `State::new(value)` which sets `version = Counter(1)` and `updated_at = now()`. Serializes to JSON string via `to_stored_value()`. Writes to storage.
3. **Returns**: The version counter (1 for new cells, existing counter for already-initialized cells).

---

### StateRead

```
Client               Handler             Engine (StateCell)   Transaction          Storage
  |                    |                   |                    |                   |
  |-- StateRead ------>|                   |                    |                   |
  | {branch, cell}     |                   |                    |                   |
  |                    |-- validate ------>|                    |                   |
  |                    |                   |                    |                   |
  |                    |                   |-- begin txn ------>|                   |
  |                    |                   |                    |                   |
  |                    |                   |-- txn.get -------->|-- read chain ---->|
  |                    |                   |   Key::new_state   |                   |
  |                    |                   |   (ns, cell)       |                   |
  |                    |                   |                    |                   |
  |                    |                   |   IF FOUND:        |                   |
  |                    |                   |   deserialize ->   |                   |
  |                    |                   |   State struct     |                   |
  |                    |                   |   extract .value   |                   |
  |                    |                   |                    |                   |
  |                    |                   |   IF NOT FOUND:    |                   |
  |                    |                   |   return None      |                   |
  |                    |                   |                    |                   |
  |<-- Output::Maybe --|<- Option<Value> --|<- Option<Value> ---|                   |
```

**Steps:**

1. **Handler**: Converts branch, validates cell name, calls `primitives.state.read()`.
2. **Engine (StateCell)**: Constructs key. Opens transaction. Calls `txn.get()`. If found, deserializes `State` from JSON string, returns `Some(state.value)`. If not found, returns `None`.
3. **Note**: Only the `.value` field is returned to the caller. The version counter and timestamp are discarded. Use `StateReadv` to get version information.

**Session-transaction path**: When a Session transaction is active, `StateRead` is handled in `execute_in_txn()` via `ctx.get(&full_key)`. The result is deserialized from `Value::String` to get the `State` struct, then `state.value` is returned. This provides read-your-writes for state cells within the transaction.

---

### StateCas (Compare-and-Swap)

```
Client               Handler             Engine (StateCell)   Transaction          Storage
  |                    |                   |                    |                   |
  |-- StateCas ------->|                   |                    |                   |
  | {branch, cell,     |                   |                    |                   |
  |  expected, value}   |                   |                    |                   |
  |                    |                   |                    |                   |
  |                    |-- validate ------>|                    |                   |
  |                    |                   |                    |                   |
  |                    |-- expected=None? -|                    |                   |
  |                    |   YES: init path  |                    |                   |
  |                    |   NO: cas path    |                    |                   |
  |                    |                   |                    |                   |
  |                    |  === CAS PATH === |                    |                   |
  |                    |                   |                    |                   |
  |                    |                   |-- begin txn ------>|                   |
  |                    |                   |                    |                   |
  |                    |                   |-- txn.get -------->|-- read chain ---->|
  |                    |                   |   Key::new_state   |                   |
  |                    |                   |                    |                   |
  |                    |                   |<- State struct ----|                   |
  |                    |                   |                    |                   |
  |                    |                   |-- compare -------->|                   |
  |                    |                   |   current.version  |                   |
  |                    |                   |   == Counter(exp)? |                   |
  |                    |                   |                    |                   |
  |                    |                   |   MISMATCH:        |                   |
  |                    |                   |   Err(conflict)    |                   |
  |                    |                   |                    |                   |
  |                    |                   |   MATCH:           |                   |
  |                    |                   |   new_ver =        |                   |
  |                    |                   |   Counter(exp+1)   |                   |
  |                    |                   |   State { value,   |                   |
  |                    |                   |     new_ver, now } |                   |
  |                    |                   |                    |                   |
  |                    |                   |-- txn.put -------->|-- write_set ----->|
  |                    |                   |                    |                   |
  |                    |                   |-- commit --------->|-- persist ------->|
  |                    |                   |                    |                   |
  |<-- MaybeVersion ---|<-----------------|<- new_version ------|                   |
  |  Some(exp+1) or    |  (None on fail)  |                    |                   |
  |  None              |                   |                    |                   |
```

**Steps:**

1. **Handler**: Two paths based on `expected_counter`:
   - **`expected_counter = None`**: Init semantics. First reads the cell. If it already exists, returns `Output::MaybeVersion(None)`. If it doesn't exist, calls `state.init()`.
   - **`expected_counter = Some(n)`**: CAS path. Calls `state.cas()` with `Version::Counter(n)`.
2. **Engine (StateCell) - CAS path**: Opens transaction. Reads current state. If cell doesn't exist, returns error. Compares `current.version` with `expected_version`. If mismatch, returns `StrataError::conflict`. If match, increments version (`Counter(n) -> Counter(n+1)`), creates new `State` with updated value and timestamp, writes to storage.
3. **Handler wrapping**: The handler catches CAS errors and converts them to `Output::MaybeVersion(None)` rather than propagating the error. Success returns `Output::MaybeVersion(Some(new_version))`.

**Version increment rule**: `Counter(n).increment() -> Counter(n+1)`. The counter is strictly per-entity and increments by 1 on every successful write.

---

### StateSet (unconditional write)

```
Client               Handler             Engine (StateCell)   Transaction          Storage
  |                    |                   |                    |                   |
  |-- StateSet ------->|                   |                    |                   |
  | {branch,cell,val}  |                   |                    |                   |
  |                    |-- validate ------>|                    |                   |
  |                    |                   |                    |                   |
  |                    |                   |-- begin txn ------>|                   |
  |                    |                   |                    |                   |
  |                    |                   |-- txn.get -------->|-- read chain ---->|
  |                    |                   |   Key::new_state   |                   |
  |                    |                   |                    |                   |
  |                    |                   |   IF EXISTS:       |                   |
  |                    |                   |   deserialize      |                   |
  |                    |                   |   new_ver =        |                   |
  |                    |                   |   cur.ver + 1      |                   |
  |                    |                   |                    |                   |
  |                    |                   |   IF NOT EXISTS:   |                   |
  |                    |                   |   new_ver =        |                   |
  |                    |                   |   Counter(1)       |                   |
  |                    |                   |                    |                   |
  |                    |                   |   State { value,   |                   |
  |                    |                   |     new_ver, now } |                   |
  |                    |                   |                    |                   |
  |                    |                   |-- txn.put -------->|-- write_set ----->|
  |                    |                   |                    |                   |
  |                    |                   |-- commit --------->|-- persist ------->|
  |                    |                   |                    |                   |
  |                    |                   |-- index update --->|                   |
  |                    |                   |   (if enabled)     |                   |
  |                    |                   |                    |                   |
  |<-- Output::Version-|<- extract u64 ----|<- Versioned -------|                   |
  |   Counter(n+1)     |                   |   <Version>        |                   |
```

**Steps:**

1. **Handler**: Converts branch, validates cell name, calls `primitives.state.set()`. Returns `Output::Version`.
2. **Engine (StateCell)**: Opens transaction. Reads current state. If exists, increments version counter. If not exists, starts at `Counter(1)`. Creates new `State { value, version, updated_at }`. Serializes and writes. Updates inverted index if enabled.
3. **Unlike CAS**: `set` does not check the expected version - it always succeeds (barring OCC conflicts). It reads the current value only to determine the next version counter.

**Returns**: `Output::Version(u64)` - the new version counter after the write.

## Storage Format

```
TypeTag:           0x03 (State)
Key format:        Namespace::for_branch(branch_id) + TypeTag::State + cell_name_bytes
Value format:      Value::String(JSON) containing State { value, version, updated_at }
Version in chain:  Version::Txn(commit_version) from transaction coordinator
Version in struct: Version::Counter(n) per-entity counter
```

### State Struct (stored as JSON)

```
State {
    value:      Value              // The user's value
    version:    Version::Counter   // Per-cell counter (1, 2, 3, ...)
    updated_at: u64                // Timestamp in microseconds
}
```

**Double versioning**: State has two version numbers:
1. `Version::Txn(commit_id)` in the storage `VersionChain` - used for MVCC snapshot isolation
2. `Version::Counter(n)` inside the `State` struct - used for CAS semantics and returned to users

## Transaction Behavior

| Aspect | Behavior |
|--------|----------|
| Isolation | Snapshot isolation |
| Concurrency control | OCC (single attempt, no retry) |
| CAS semantics | Application-level via version counter check in engine |
| Writes per operation | 1 (state cell) |
| Read-your-writes | Yes within transaction |
| Multi-command txn | Yes via Session |

## Consistency Notes

- State is the only primitive with **CAS (Compare-and-Swap)** semantics via explicit version counter
- State wraps user values in a `State { value, version, updated_at }` struct, unlike KV which stores values directly
- The version counter is **per-entity** (per cell), unlike KV's global transaction IDs or Event's global sequence numbers
- `StateInit` is **idempotent** - calling it on an existing cell returns the existing version without modifying the cell
- `StateCas` with `expected_counter = None` in the handler layer acts as init-if-not-exists, checking existence before calling `state.init()`
- The handler converts CAS failures to `Output::MaybeVersion(None)` rather than returning an error, making it safe to use in optimistic loops
- State has a `readv` operation for version history (not documented above as it's not one of the main 4, but it uses `db.get_history()` to retrieve all versions from the `VersionChain`)
