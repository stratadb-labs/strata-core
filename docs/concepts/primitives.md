# Primitives

StrataDB provides **six data primitives** — purpose-built data structures that cover the state management needs of AI agents. Each primitive has a specific shape and API, rather than forcing everything into a generic key-value model.

## The Six Primitives

| Primitive | Shape | Best For |
|-----------|-------|----------|
| **[KV Store](../guides/kv-store.md)** | Key → Value | Working memory, config, scratchpads |
| **[Event Log](../guides/event-log.md)** | Append-only sequence of typed events | Audit trails, tool call history, decision logs |
| **[State Cell](../guides/state-cell.md)** | Named cell with CAS | Coordination, counters, locks, state machines |
| **[JSON Store](../guides/json-store.md)** | Key → JSON document with path access | Structured config, conversation history |
| **[Vector Store](../guides/vector-store.md)** | Collection of keyed embeddings with pluggable indexing (brute-force or HNSW) | Similarity search, RAG context, agent memory |
| **[Branch](../guides/branch-management.md)** | Named isolated namespace | Session isolation, experiments, multi-tenancy |

## Choosing the Right Primitive

**"I need to store a simple value by key"** → KV Store

**"I need an immutable log of events"** → Event Log. Events are append-only. You cannot modify or delete individual events.

**"I need a value that multiple writers coordinate on"** → State Cell. The compare-and-swap (CAS) operation lets you update only if you have the latest version, preventing lost updates.

**"I need a structured document I can update at specific paths"** → JSON Store. You can read and write at JSON paths like `$.config.temperature` without replacing the whole document.

**"I need to store and search embeddings"** → Vector Store. Create collections with a fixed dimension and distance metric, then search by similarity. Supports brute-force (exact) and HNSW (approximate) indexing, batch upsert for bulk loading, and 8 metadata filter operators.

**"I need to isolate data between sessions or experiments"** → Branches. Every other primitive is scoped to a branch.

## Comparison

| Feature | KV | Event | State | JSON | Vector |
|---------|----|----|-------|------|--------|
| Read by key | Yes | By sequence | By cell name | By key + path | By collection + key |
| Write | Put (overwrite) | Append only | Set / CAS | Set at path | Upsert |
| Delete | Yes | No | No | Yes | Yes |
| List/scan | By prefix | By type | No | By prefix | By collection |
| Search | No | No | No | No | Similarity |
| Versioned | Yes | Yes (sequence) | Yes (counter) | Yes | Yes |
| Transactional | Yes | Yes | Yes | Yes | No* |

*Vector operations are not transactional — they bypass the session transaction system.

## All Primitives Are Branch-Scoped

Every primitive is isolated by the current branch. Data written in one branch is invisible from another. See [Branches](branches.md) for details.

### Space Organization Within Branches

Within a branch, primitives are further organized by **space**. Each space has its own independent instance of every primitive. The `default` space is implicit — all operations target it unless you switch with `set_space`.

```rust
let mut db = Strata::cache()?;

// Default space
db.kv_put("key", "default-value")?;

// Switch space — data is separate
db.set_space("experiments")?;
assert!(db.kv_get("key")?.is_none()); // not visible in this space
```

Spaces are organizational, not isolation boundaries. Transactions can span multiple spaces within the same branch. See [Spaces](../guides/spaces.md) for the full guide.

## All Primitives Use Value

Every primitive stores data as [`Value`](value-types.md) — StrataDB's 8-variant type system. You pass Rust types directly (strings, integers, bools) and they convert automatically via `Into<Value>`.

## Next

- [Value Types](value-types.md) — the 8-variant type system
- [Guides](../guides/index.md) — per-primitive API walkthroughs
