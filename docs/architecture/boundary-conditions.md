# Boundary Condition Tests

Systematic edge case analysis at every layer.

## 1. Key Validation Boundaries

### KV / State / JSON Keys — via `validate_key()` (bridge.rs:110-129)

| Input | Behavior | Correct? |
|-------|----------|----------|
| `""` (empty) | Rejected: "Key must not be empty" | Yes |
| `"a"` (1 char) | Accepted | Yes |
| 1024-byte string | Accepted (at limit) | Yes |
| 1025-byte string | Rejected: "Key exceeds maximum length" | Yes |
| `"hello\0world"` (NUL) | Rejected: "Key must not contain NUL bytes" | Yes |
| `"_strata/internal"` | Rejected: reserved prefix | Yes |
| `"_strata"` (no slash) | Accepted (prefix is `_strata/`) | Yes |
| Unicode (`"日本語"`) | Accepted | Yes |
| Whitespace-only (`"   "`) | Accepted | Debatable — not harmful |

**Verdict: KV/State/JSON key validation is thorough.**

### Vector Keys — via `validate_vector_key()` (collection.rs:66-82)

| Input | Behavior | Different from KV? |
|-------|----------|-------------------|
| `""` (empty) | **Accepted** | **Yes — KV rejects** |
| 1024-byte string | Accepted | Same |
| 1025-byte string | Rejected | Same |
| NUL bytes | Rejected | Same |
| `"_strata/internal"` | **Accepted** | **Yes — KV rejects** |

**Finding**: Vector key validation is more permissive than KV key validation. Empty keys and reserved-prefix keys are accepted. The comment at line 67 says "Empty string keys are allowed (consistent with other key-value stores)" — but this is inconsistent with the same codebase's own KV validation.

### Collection Names — via `validate_collection_name()` (collection.rs:19-57)

| Input | Behavior | Correct? |
|-------|----------|----------|
| `""` (empty) | Rejected | Yes |
| `"/"` (slash) | Rejected | Yes |
| NUL bytes | Rejected | Yes |
| `"_internal"` (underscore prefix) | Rejected (reserved) | Yes |
| > 256 chars | Rejected | Yes |

**Verdict: Collection name validation is thorough.**

### Event Types — via `validate_event_type()` (event.rs:177-185)

| Input | Behavior | Correct? |
|-------|----------|----------|
| `""` (empty) | Rejected: EmptyEventType | Yes |
| `"   "` (whitespace only) | Accepted | Debatable |
| Unicode | Accepted | Yes |
| Very long strings | Accepted (no length limit) | Missing limit |

**Verdict: Event type validation is minimal — only empty string rejected. No length limit.**

### Branch Names

| Input | Behavior | Correct? |
|-------|----------|----------|
| `""` (empty) | **Accepted** | **No — inconsistent** |
| `"default"` | Maps to nil UUID | Yes |
| Valid UUID string | Parsed directly | Yes |
| Any other string | UUID v5 generated | Yes |

**Finding**: `create_branch("")` succeeds. The engine's `BranchIndex::create_branch()` (index.rs:238) has no name validation. The executor's `to_core_branch_id()` (bridge.rs:82-93) generates a deterministic UUID v5 for any non-"default" string, including empty strings. Every other primitive rejects empty identifiers (keys, collection names, event types).

## 2. Numeric Overflow Boundaries

### Version::increment() — Overflow at u64::MAX

**Location**: `crates/core/src/contract/version.rs:139-145`

```rust
pub const fn increment(&self) -> Self {
    match self {
        Version::Txn(v) => Version::Txn(*v + 1),       // ← unchecked
        Version::Sequence(v) => Version::Sequence(*v + 1),
        Version::Counter(v) => Version::Counter(*v + 1),
    }
}
```

**Behavior**:
- **Debug mode**: Panics on overflow (`attempt to add with overflow`)
- **Release mode**: Wraps to 0 silently

**Impact**: Used in `StateCell::cas()` (state.rs:210) and `StateCell::set()` (state.rs:242). A state cell updated `u64::MAX` times would panic in debug or wrap to `Counter(0)` in release, breaking CAS semantics.

**Safe alternative exists**: `saturating_increment()` at version.rs:148-154 uses `saturating_add(1)`, but it is **never called in production code**.

### Global Version Counter — Overflow at u64::MAX

**Location**: `crates/concurrency/src/manager.rs:136-138`

```rust
pub fn allocate_version(&self) -> u64 {
    self.version.fetch_add(1, Ordering::SeqCst) + 1
}
```

**Behavior**: `fetch_add` wraps at u64::MAX. After `u64::MAX` versions:
- `fetch_add(1)` returns `u64::MAX`, function returns 0 (wraps)
- Next call returns 1, then 2, etc.
- These duplicate earlier version numbers, corrupting MVCC ordering

**Impact**: All MVCC snapshot reads, version chain ordering, and conflict detection rely on version monotonicity. Wrapping destroys this invariant globally.

**Practical risk**: At 1 billion transactions per second, overflow takes ~585 years. Low practical risk, but the invariant is architecturally fundamental.

### Transaction ID Counter — Overflow at u64::MAX

**Location**: `crates/concurrency/src/manager.rs:118-120`

```rust
pub fn next_txn_id(&self) -> u64 {
    self.next_txn_id.fetch_add(1, Ordering::SeqCst)
}
```

Same wrapping behavior as version counter. Transaction IDs would collide in WAL records, making recovery ambiguous.

### Event Sequence Counter — Overflow at u64::MAX

**Location**: `crates/engine/src/primitives/event.rs:374`

```rust
meta.next_sequence = sequence + 1;
```

**Behavior**: Unchecked addition. At u64::MAX, wraps to 0. The next event is stored at sequence 0, overwriting the very first event in the log.

**Impact**: Event key format encodes sequence as part of the storage key. Wrapping creates key collisions with existing events, silently overwriting historical data.

## 3. Vector Embedding Boundaries

### NaN and Infinity in Embeddings — Not Validated

**Location**: `crates/engine/src/primitives/vector/heap.rs:180-213` (upsert)

The `upsert()` method validates dimension match but does **not** check for NaN or Infinity values in the embedding vector. The embedding is stored directly:

```rust
if embedding.len() != self.config.dimension {
    return Err(VectorError::DimensionMismatch { ... });
}
// No NaN/Infinity check — goes straight to storage
```

**Impact**:
- NaN in embeddings produces NaN distances during search, corrupting result ordering
- Infinity in embeddings produces Infinity distances, pushing results to extremes
- Cosine similarity with NaN returns NaN, which is not comparable (all comparisons with NaN are false)
- Once stored, the corrupted vector affects every subsequent search across the collection

**Contrast**: Event payload validation (event.rs:186-209) explicitly rejects NaN and Infinity in JSON payloads. The vector subsystem has no equivalent guard.

### Dimension Bounds

| Input | Behavior | Correct? |
|-------|----------|----------|
| dimension = 0 | Rejected: `InvalidDimension` (store.rs:168) | Yes |
| dimension = 1 | Accepted | Yes |
| dimension = 1,000,000 | **Accepted** | **Missing upper bound** |

**Finding**: `create_collection()` validates dimension > 0 but has no upper bound. A dimension of 1 million means each vector requires 4MB (1M * 4 bytes). With 1000 vectors, that's 4GB in the heap alone. There is no guard against memory exhaustion.

### Embedding Dimension Mismatch

| Input | Behavior | Correct? |
|-------|----------|----------|
| Embedding matches config dimension | Accepted | Yes |
| Embedding shorter than config | Rejected: `DimensionMismatch` | Yes |
| Embedding longer than config | Rejected: `DimensionMismatch` | Yes |
| Empty embedding (dim=0 config) | Rejected at collection creation | Yes |

**Verdict: Dimension mismatch is properly validated.**

## 4. JSON Document Boundaries

### Nesting Depth

**Location**: `crates/core/src/primitives/json.rs:40`

```
MAX_NESTING_DEPTH = 100
```

| Input | Behavior | Correct? |
|-------|----------|----------|
| 99-level nested object | Accepted | Yes |
| 100-level nested object | Accepted (at limit) | Yes |
| 101-level nested object | Rejected | Yes |

### Document Size

**Location**: `crates/core/src/primitives/json.rs:34`

```
MAX_DOCUMENT_SIZE = 16 * 1024 * 1024  (16 MB)
```

| Input | Behavior | Correct? |
|-------|----------|----------|
| 16MB - 1 byte | Accepted | Yes |
| 16MB | Accepted (at limit) | Yes |
| 16MB + 1 byte | Rejected | Yes |

### JSON Path Edge Cases

| Input | Behavior | Correct? |
|-------|----------|----------|
| `"$"` (root) | Selects root document | Yes |
| `""` (empty) | Equivalent to `"$"` | Yes (json.rs:686-689) |
| `"$.nonexistent"` | Returns None/null | Yes |
| `"$[0]"` on non-array | Returns None/null | Yes |

**Verdict: JSON boundaries are well-guarded.**

## 5. Event Log Boundaries

### Sequence Numbering

| Scenario | Behavior | Correct? |
|----------|----------|----------|
| First event in log | sequence = 0 (0-indexed) | Yes |
| Read sequence 0 | Returns first event | Yes |
| Read negative offset | Not possible (u64) | Yes |
| Read beyond last sequence | Returns None | Yes |

### Event Payload Validation

| Input | Behavior | Correct? |
|-------|----------|----------|
| `{}` (empty object) | Accepted | Yes |
| `[]` (array) | Rejected: "Payload must be a JSON object" | Yes |
| `"string"` | Rejected: "Payload must be a JSON object" | Yes |
| `42` (number) | Rejected: "Payload must be a JSON object" | Yes |
| `null` | Rejected: "Payload must be a JSON object" | Yes |
| Object with NaN | Rejected: "NaN values not permitted" | Yes |
| Object with Infinity | Rejected: "Infinity values not permitted" | Yes |

**Verdict: Event payload validation is thorough.** Notable that this is stricter than vector embedding validation.

### Hash Chain at Boundary

| Scenario | Behavior | Correct? |
|----------|----------|----------|
| First event (no previous) | Hash includes `[0u8; 32]` as prev_hash | Yes |
| Event after gap | Not possible — sequences are contiguous | Yes |

## 6. Transaction Boundaries

### Empty Transaction

| Scenario | Behavior | Correct? |
|----------|----------|----------|
| Begin → Commit (no operations) | Succeeds, allocates version | Wasteful but harmless |
| Begin → Rollback (no operations) | Succeeds, no version allocated | Yes |

**Finding**: An empty transaction commit allocates a version number that serves no purpose. This creates version gaps (documented as intentional in manager.rs:126-135) but wastes version space. Not a bug per se, but a minor inefficiency.

### Transaction Size

No explicit limit on:
- Number of keys in write-set
- Total data size in write-set
- Number of operations per transaction

A transaction writing millions of keys would accumulate all data in memory (TransactionContext's write_set HashMap) before commit, potentially causing OOM.

### Transaction Timeout

**Location**: `crates/concurrency/src/transaction.rs`

A timeout field exists on `TransactionContext` but is **not enforced** in production code paths. There is no background task checking for expired transactions. A transaction that begins but never commits holds its allocated resources (TransactionContext) indefinitely — until `Session::drop()` returns it to the pool.

## 7. Branch Operation Boundaries

### Delete Default Branch

| Scenario | Behavior | Correct? |
|----------|----------|----------|
| Delete "default" branch | Rejected | Yes |

### Operations on Non-Existent Branch

| Scenario | Behavior | Correct? |
|----------|----------|----------|
| KV write to non-existent branch | Succeeds (creates shard lazily) | By design |
| TxnBegin on non-existent branch | Succeeds | By design (#853) |
| BranchGet on non-existent branch | Returns None | Yes |
| BranchDelete on non-existent branch | Returns error | Yes |

### Operations on Deleted Branch

| Scenario | Behavior | Correct? |
|----------|----------|----------|
| KV write to deleted branch | **Succeeds** | **No — creates orphaned data** |
| State write to deleted branch | **Succeeds** | **No — creates orphaned data** |
| Event append to deleted branch | **Succeeds** | **No — creates orphaned data** |

**Finding**: Branch deletion (`BranchIndex::delete_branch()` at index.rs:312-373) removes metadata and scans all data, but does not prevent future writes. Since branch IDs are deterministic UUIDs derived from names, a write to a deleted branch creates new data in a shard with no corresponding metadata. This data is invisible to `list_branches()` but exists in storage.

## 8. CAS (Compare-and-Swap) Boundaries

### State CAS Edge Cases

| Scenario | Behavior | Correct? |
|----------|----------|----------|
| CAS with expected=None, cell doesn't exist | Initializes cell | Yes |
| CAS with expected=None, cell exists | Returns conflict | Yes (after #836 fix) |
| CAS with expected=Counter(0) | Only works if cell has Counter(0) | Yes |
| CAS with expected=Counter(u64::MAX) | CAS succeeds, new version wraps | **No — overflow** |

### Version 0

| Context | Meaning of version 0 | Consistent? |
|---------|----------------------|-------------|
| State CAS expected=None | "Create if not exists" | Yes |
| MVCC version 0 | Never allocated (versions start at 1) | Yes |
| Event sequence 0 | First event in log | Yes — but different from MVCC |
| `Version::is_zero()` | Returns true for any variant with value 0 | Yes |

## 9. Summary

| # | Finding | Severity | Type |
|---|---------|----------|------|
| 1 | `Version::increment()` panics (debug) or wraps (release) at u64::MAX | Medium | Overflow bug |
| 2 | Global version counter wraps at u64::MAX — corrupts MVCC | Low | Overflow (theoretical) |
| 3 | Transaction ID counter wraps at u64::MAX | Low | Overflow (theoretical) |
| 4 | Event sequence counter wraps at u64::MAX — overwrites events | Low | Overflow (theoretical) |
| 5 | NaN/Infinity in vector embeddings not validated | Medium | Missing validation |
| 6 | No upper bound on vector dimension — allows memory exhaustion | Medium | Missing validation |
| 7 | Empty branch name accepted — inconsistent with other primitives | Low | Inconsistent validation |
| 8 | Operations on deleted branch create orphaned data | Medium | Missing guard |
| 9 | Transaction timeout not enforced | Low | Incomplete feature |
| 10 | Vector key validation inconsistent with KV key validation | Low | Inconsistent validation |

**Overall**: Input validation is strong at the KV/State/JSON/Event layers but has gaps in the Vector and Branch layers. The most practically impactful issues are NaN in embeddings (#5), missing dimension upper bound (#6), and orphaned data after branch delete (#8). The overflow issues (#1-4) are theoretical given realistic workloads but represent architectural invariant violations.
