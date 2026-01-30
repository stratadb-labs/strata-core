# Engine Crate Integration Test Plan

## Crate Overview

`strata-engine` is the core orchestration layer providing:
- **Database**: Main database struct with transactions, durability, lifecycle
- **6 Primitives**: KVStore, EventLog, StateCell, JsonStore, VectorStore, RunIndex
- **Transaction Coordination**: OCC, retry, cross-primitive atomicity
- **Durability**: In-memory, buffered, strict modes

## Current State

- **Unit tests**: 675 (673 passing, 2 failing - bugs in crate)
- **Integration tests**: 53 files, 14k lines, 1529 compilation errors
- **Issue**: Tests reference non-existent `common.rs` module

## Test Organization

```
tests/engine/
├── main.rs                    # Entry point
├── database/
│   ├── lifecycle.rs           # Open, close, reopen, ephemeral
│   ├── transactions.rs        # Begin, commit, abort, retry
│   └── durability_modes.rs    # In-memory, buffered, strict equivalence
├── primitives/
│   ├── kv.rs                  # KVStore API
│   ├── eventlog.rs            # EventLog with hash chaining
│   ├── statecell.rs           # CAS operations
│   ├── jsonstore.rs           # JSON path operations
│   ├── vectorstore.rs         # Vector similarity search
│   └── runindex.rs            # Run lifecycle management
├── cross_primitive.rs         # Cross-primitive transactions
├── run_isolation.rs           # Run namespace isolation
├── acid_properties.rs         # ACID invariants
└── stress.rs                  # Heavy workload (ignored)
```

## Behavioral Invariants

### Database Invariants

1. **Lifecycle Safety**
   - `Database::ephemeral()` creates working in-memory database
   - `Database::open(path)` creates/reopens persistent database
   - Reopened database sees all committed data
   - Shutdown is graceful; pending transactions complete or abort

2. **Transaction Semantics**
   - `db.transaction(run_id, |txn| ...)` provides ACID guarantees
   - Transactions are retried on conflict (configurable)
   - Read-only transactions never conflict
   - Aborted transactions leave no trace

3. **Durability Mode Equivalence**
   - Same operations produce same results across all modes
   - Only persistence behavior differs

### Primitive Invariants

#### KVStore
- `get` on missing key → `None`
- `put` then `get` → returns value
- `put` overwrites existing value
- `delete` returns true iff key existed
- `list(prefix)` returns only matching keys
- Keys from different runs are isolated

#### EventLog
- Events are append-only (immutable after write)
- Sequence numbers are monotonically increasing per run
- Hash chain is verifiable and tamper-evident
- `read_range(start, end)` is inclusive of start, exclusive of end
- Empty log → `len() == 0`, `head() == None`

#### StateCell
- `init` fails if cell already exists
- `cas(name, expected_version, new_value)` succeeds iff version matches
- `set` always succeeds (unconditional write)
- `transition` atomically reads, transforms, writes
- Concurrent CAS on same cell → exactly one succeeds

#### JsonStore
- `create` fails if document exists
- Path operations (`get`, `set`, `delete_at_path`) work on nested JSON
- `merge` follows JSON Merge Patch semantics (RFC 7396)
- `cas` provides optimistic concurrency for documents
- Array operations (`array_push`, `array_pop`) are atomic

#### VectorStore
- Collection must be created before insert
- Dimension of vectors must match collection config
- Search returns vectors sorted by similarity (distance ascending or similarity descending)
- Delete collection removes all vectors
- Vector IDs are stable and never reused within a collection

#### RunIndex
- Run status follows state machine: Created → Running → {Completed, Failed, Cancelled}
- Invalid status transitions are rejected
- Deleted runs cannot be accessed
- Tags and metadata are mutable

### Cross-Cutting Invariants

1. **Run Isolation**
   - Data written to Run A is invisible to Run B
   - Each primitive enforces run isolation independently
   - Cross-run reads return `None`

2. **Cross-Primitive Transactions**
   - Single transaction can span multiple primitives
   - All operations commit or all abort
   - Conflict in any primitive aborts entire transaction

3. **ACID Properties**
   - **Atomicity**: Transaction is all-or-nothing
   - **Consistency**: Only valid state transitions allowed
   - **Isolation**: Concurrent transactions don't see uncommitted data
   - **Durability**: Committed data survives restart (non-ephemeral)

## Test Categories

### Tier 1: Database Core (~25 tests)
- Lifecycle (ephemeral, persistent, reopen)
- Transaction API (begin, commit, abort)
- Durability mode equivalence
- Retry configuration

### Tier 2: Primitive APIs (~60 tests, 10 per primitive)
- Basic CRUD for each primitive
- Edge cases (empty, missing, duplicate)
- Run-scoped operations

### Tier 3: Behavioral Invariants (~30 tests)
- EventLog hash chain verification
- StateCell CAS semantics
- VectorStore search ordering
- JsonStore merge semantics

### Tier 4: Cross-Cutting (~20 tests)
- Run isolation across primitives
- Cross-primitive transactions
- ACID property demonstrations

### Tier 5: Stress Tests (~5 tests, ignored)
- Concurrent transactions
- Large data volumes
- Long-running operations

**Total: ~140 tests**

## Key Test Patterns

```rust
// Standard test setup
let test_db = TestDb::new();
let run_id = test_db.run_id;

// Primitive access
let kv = test_db.kv();
let event = test_db.event();

// Cross-primitive transaction
test_db.db.transaction(run_id, |txn| {
    txn.kv_put("key", Value::Int(42))?;
    txn.event_append("type", payload)?;
    Ok(())
})?;

// Run isolation test
let run_a = RunId::new();
let run_b = RunId::new();
kv.put(&run_a, "key", value)?;
assert!(kv.get(&run_b, "key")?.is_none()); // Isolated

// Durability mode equivalence
test_across_modes("operation_x", |db| {
    // Same result regardless of durability mode
});
```

## Implementation Notes

1. Use existing `TestDb` from `tests/common/mod.rs`
2. Import via `#[path = "../common/mod.rs"]`
3. Focus on behavioral invariants, not implementation details
4. Keep individual tests focused (one assertion concept per test)
5. Use descriptive test names that document behavior
