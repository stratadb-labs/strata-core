# Frequently Asked Questions

## General

### What is StrataDB?

StrataDB is an embedded database designed for AI agents. It provides six data primitives (KV, EventLog, StateCell, JSON, Vector, Branch) with branch-based data isolation, OCC transactions, and three durability modes.

### Is StrataDB a replacement for Redis or Postgres?

No. StrataDB complements traditional databases. Use Postgres for application data, Redis for caching, and a dedicated vector database for large-scale embedding search. Use StrataDB for agent state that needs branch isolation, atomicity across primitives, and deterministic replay.

### Why not just use SQLite?

SQLite is excellent for relational data but doesn't provide branch-scoped operations, purpose-built primitives for agent state (EventLog, StateCell, VectorStore), or multi-primitive transactions out of the box. You would build these features yourself on top of SQLite.

### Is StrataDB production-ready?

StrataDB is production-ready for embedded (in-process) use. It has comprehensive test coverage, verified crash recovery, and benchmarked performance. A network layer and distributed mode are planned for the future.

## Data Model

### What are branches?

Branches are isolated data namespaces, similar to git branches. All data in StrataDB lives in a branch. Data written in one branch is invisible from another. See [Concepts: Branches](concepts/branches.md).

### How many primitives are there?

Six: KV Store, Event Log, State Cell, JSON Store, Vector Store, and Branch. See [Concepts: Primitives](concepts/primitives.md).

### What value types are supported?

Eight: Null, Bool, Int (i64), Float (f64), String, Bytes, Array, and Object. There are no implicit type coercions. See [Concepts: Value Types](concepts/value-types.md).

### Can I store arbitrary Rust structs?

Not directly. Convert your struct to a `Value` (typically via `serde_json::json!()` for Object values, or use the `From` implementations for simple types). StrataDB's type system is intentionally simple — 8 types that map cleanly to JSON.

## Performance

### What throughput can I expect?

| Mode | Throughput |
|------|-----------|
| Ephemeral | 250K+ ops/sec (single thread), 800K+ (4 threads) |
| Buffered | 50K+ ops/sec |
| Strict | ~500 ops/sec |

### What is the read latency?

Fast path reads (in-memory, no transaction) are typically under 10 microseconds.

### How does the Vector Store compare to dedicated vector databases?

The Vector Store is designed for branch-scoped agent memory (hundreds to tens of thousands of vectors per collection), not large-scale similarity search. For million-scale embeddings, use a dedicated vector database. For agent context, working memory, and session-scoped RAG, use StrataDB's Vector Store.

## Transactions

### Are transactions required?

No. Individual operations (kv_put, kv_get, etc.) are already atomic. Use transactions when you need multi-operation atomicity (e.g., read-modify-write, or updating multiple keys together).

### Do vector operations participate in transactions?

No. Vector operations bypass the transaction system and execute immediately. All other data primitives (KV, Event, State, JSON) are transactional.

### What happens if my application crashes during a transaction?

Uncommitted transactions are automatically discarded during recovery. Only committed data survives a crash.

## Durability

### What durability mode should I use?

- **Testing:** None (`Strata::open_temp()`)
- **Production:** Buffered (default) — good balance of speed and safety
- **Critical data:** Strict — zero data loss, but slower

See [Concepts: Durability](concepts/durability.md).

### Can I change the durability mode after opening?

The durability mode is set when the database is opened and cannot be changed at runtime.

### How does crash recovery work?

On restart, StrataDB loads the latest snapshot (if any) and replays WAL entries after it. Only committed transactions are applied. See [Architecture: Durability and Recovery](architecture/durability-and-recovery.md).

## Scaling

### Can I use StrataDB from multiple threads?

Yes. Share an `Arc<Database>` between threads and create a separate `Strata` or `Session` per thread.

### Is there a network/server mode?

Not yet. StrataDB is currently embedded (in-process). A network layer is planned.

### Can I use this with LangChain or other agent frameworks?

Yes. StrataDB is a library that any Rust application can use. Agent frameworks can use it for state management.

## Contributing

### How do I contribute?

See [Contributing](../CONTRIBUTING.md) for development setup, running tests, and PR guidelines.

### Where do I report bugs?

File an issue at [GitHub Issues](https://github.com/anibjoshi/strata/issues).
