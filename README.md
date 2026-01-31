# StrataDB

**An embedded database for AI agents — six primitives, run isolation, and deterministic replay.**

```rust
use stratadb::Strata;

let db = Strata::open_temp()?;
db.kv_put("agent:status", "thinking")?;
db.event_append("tool_call", serde_json::json!({"tool": "search", "query": "docs"}))?;
db.state_cas("lock", None, "acquired")?;
```

## Primitives

| Primitive | Purpose | Key Methods |
|-----------|---------|-------------|
| **KV Store** | Working memory, config, scratchpads | `kv_put`, `kv_get`, `kv_delete`, `kv_list` |
| **Event Log** | Immutable audit trail, tool call history | `event_append`, `event_read`, `event_read_by_type` |
| **State Cell** | CAS-based coordination, counters, locks | `state_set`, `state_read`, `state_cas`, `state_init` |
| **JSON Store** | Structured documents with path-level mutations | `json_set`, `json_get`, `json_delete`, `json_list` |
| **Vector Store** | Embeddings and similarity search | `vector_upsert`, `vector_search`, `vector_create_collection` |
| **Run** | Data isolation (like git branches) | `create_run`, `set_run`, `list_runs`, `delete_run` |

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
stratadb = "0.1"
```

Minimum Rust version: **1.70**

## Quick Example

```rust
use stratadb::Strata;
use stratadb::Value;

fn main() -> stratadb::Result<()> {
    // Open a persistent database
    let mut db = Strata::open("./my-data")?;

    // All data lives in a "run" (like a git branch)
    // You start on the "default" run automatically
    db.kv_put("user:name", "Alice")?;
    db.kv_put("user:score", 42i64)?;

    // Create an isolated run for an experiment
    db.create_run("experiment-1")?;
    db.set_run("experiment-1")?;

    // Data is isolated — "user:name" doesn't exist here
    assert!(db.kv_get("user:name")?.is_none());

    // Switch back to default
    db.set_run("default")?;
    assert_eq!(db.kv_get("user:name")?, Some(Value::String("Alice".into())));

    Ok(())
}
```

## Durability

Choose your speed/safety trade-off:

| Mode | Latency | Throughput | Data Loss on Crash |
|------|---------|------------|-------------------|
| **InMemory** | <3 us | 250K+ ops/sec | All |
| **Buffered** | <30 us | 50K+ ops/sec | Last ~100ms |
| **Strict** | ~2 ms | ~500 ops/sec | None |

## Architecture

```
+-----------------------------------------------------------+
|  Strata API (KV, Event, State, JSON, Vector, Run)         |
+-----------------------------------------------------------+
|  Executor (Command dispatch) / Session (Transactions)     |
+-----------------------------------------------------------+
|  Engine (Database, Primitives, Transaction coordination)  |
+-----+-----------------------+-----------------------------+
      |                       |
+-----v-------+  +------------v----------+  +--------------+
| Concurrency |  |  Durability           |  | Intelligence |
| (OCC, CAS)  |  |  (WAL, Snapshots)     |  | (Search,BM25)|
+-------------+  +----------+------------+  +--------------+
                             |
                   +---------v---------+
                   |  Storage          |
                   |  (ShardedStore)   |
                   +-------------------+
```

**Key design choices:**

- **Unified storage** — all primitives share one sharded map, enabling atomic multi-primitive transactions
- **Run-tagged keys** — every key includes its run ID, making replay O(run size)
- **Optimistic concurrency** — lock-free transactions via compare-and-swap; agents rarely conflict
- **Batched durability** — fsync batched by default; losing 100ms of work is acceptable for most agents

## Documentation

- [Documentation Hub](docs/index.md) — start here
- [Getting Started](docs/getting-started/installation.md) — installation and first database
- [Concepts](docs/concepts/index.md) — runs, primitives, value types, transactions, durability
- [Guides](docs/guides/kv-store.md) — per-primitive walkthroughs
- [Cookbook](docs/cookbook/index.md) — real-world patterns
- [API Reference](docs/reference/api-quick-reference.md) — every method at a glance
- [Architecture](docs/architecture/index.md) — how StrataDB works internally
- [Contributing](CONTRIBUTING.md) — development setup and PR process

## License

[Apache License 2.0](LICENSE)
