# StrataDB

**The embedded database with git-like powers.**

Branch, time-travel, merge, and search — across six built-in data primitives. Zero dependencies. Built in Rust.

[Documentation](https://stratadb.org/docs/getting-started) | [Playground](https://stratadb.org/playground) | [Changelog](https://stratadb.org/changelog)

---

## Install

```bash
pip install stratadb            # Python
npm install @stratadb/core      # Node.js
cargo install strata-cli        # Rust CLI
brew install stratadb/tap/strata  # Homebrew
```

## Quick Start

```python
from stratadb import Strata

db = Strata.open("./mydb")          # persistent (or Strata.cache() for in-memory)

# KV — working memory
db.kv.put("user:1", {"name": "Alice", "role": "engineer"})
db.kv.get("user:1")                 # {"name": "Alice", "role": "engineer"}

# Event log — immutable audit trail
db.events.append("actions", {"tool": "search", "query": "docs"})

# State cell — CAS-based coordination
db.state.set("status", "idle")
db.state.cas("status", "running", expected="idle")

# JSON — structured documents with path-level mutations
db.json.set("config", "$.model", "gpt-4")
db.json.get("config", "$.model")    # "gpt-4"

# Vectors — similarity search
coll = db.vectors.create("docs", dimension=384)
coll.upsert("d1", embedding, metadata={"title": "Hello"})
coll.search(query_vec, k=5)

# Branches — git-like data isolation
db.branches.create("experiment")
db.checkout("experiment")
# ...make changes safely, then merge or delete
db.merge("experiment")
```

## Six Primitives

| Primitive | Purpose | Example |
|-----------|---------|---------|
| **KV Store** | Versioned key-value pairs with prefix scan | `db.kv.put("key", value)` |
| **Event Log** | Append-only streams for replay and audit | `db.events.append("stream", event)` |
| **State Cell** | Compare-and-swap for locks and counters | `db.state.cas("lock", new, expected=old)` |
| **JSON Store** | Documents with atomic path-level updates | `db.json.set("doc", "$.path", value)` |
| **Vector Store** | HNSW-indexed embeddings + metadata filtering | `coll.search(query_vec, k=10)` |
| **Branch** | Fork, diff, and merge entire data states | `db.branches.create("experiment")` |

Every primitive supports **version history** and **time-travel queries** out of the box.

## Key Features

### Branch and Merge

Fork your data state in microseconds. Run experiments in isolation. Merge successful results back — production data stays untouched.

```python
db.branches.create("redesign")
db.checkout("redesign")

db.kv.put("config", {"theme": "new-look"})
# main is completely untouched

db.merge("redesign")  # worked? merge it
```

### Time Travel

Query any past state with point-in-time snapshots. Every change is versioned with timestamps.

```python
snapshot = db.at(yesterday)
old_config = snapshot.kv.get("config")

db.kv.history("config")
# [{"value": ..., "version": 3, "timestamp": ...}, ...]
```

### Intelligent Search

Natural language search across all data types with automatic query expansion and reranking.

```python
db.search("what changed before the deploy failed?",
          mode="hybrid", expand=True, rerank=True)
```

### Auto Embedding

One flag makes every write searchable via built-in MiniLM embeddings.

```python
db = Strata.open("./mydb", auto_embed=True)

db.kv.put("user:1", {"name": "Alice", "role": "engineer"})
db.search("who is an engineer?")  # finds user:1
```

### Transactions

OCC with snapshot isolation. Atomic commits across multiple primitives.

```python
with db.transaction():
    db.kv.put("balance:alice", 50)
    db.kv.put("balance:bob", 150)
    db.events.append("transfers", {"from": "bob", "to": "alice", "amount": 100})
```

## Performance

| Durability Mode | Throughput | Latency | Data Loss on Crash |
|-----------------|-----------|---------|-------------------|
| **Cache** | 250K+ ops/sec | <3 us | All (in-memory) |
| **Standard** | 50K+ ops/sec | <30 us | Last ~100ms |
| **Always** | ~500 ops/sec | ~2 ms | None |

- **P99 read latency:** <10 microseconds
- **Branch creation:** <1 millisecond
- **Multi-threaded:** 800K+ ops/sec across 4 threads

## Architecture

```
+-----------------------------------------------------------+
|  Strata API (KV, Event, State, JSON, Vector, Branch)      |
+-----------------------------------------------------------+
|  Executor (Command dispatch, Session management)          |
+-----------------------------------------------------------+
|  Engine (Database, Primitives, Transaction coordination)  |
+-----+-----------------------+-----------------------------+
      |                       |                       |
+-----v-------+  +------------v----------+  +---------v----+
| Concurrency |  |  Durability           |  | Intelligence |
| (OCC, CAS)  |  |  (WAL, Snapshots)     |  | (BM25, RRF)  |
+------+------+  +----------+------------+  +--------------+
       |                     |
+------v---------------------v---------+  +--------------+
|  Storage (ShardedStore, DashMap)     |  |  Security    |
+--------------------------------------+  +--------------+
|  Core (Value types, BranchId, etc.)  |
+--------------------------------------+
```

**Design choices:** unified storage for multi-primitive atomicity, branch-tagged keys for O(branch) isolation, optimistic concurrency (lock-free transactions), pluggable vector indexing (brute-force or HNSW per collection).

## SDKs

| Language | Package | Install |
|----------|---------|---------|
| **Python** | [stratadb](https://pypi.org/project/stratadb/) | `pip install stratadb` |
| **Node.js** | [@stratadb/core](https://www.npmjs.com/package/@stratadb/core) | `npm install @stratadb/core` |
| **Rust** | [stratadb](https://crates.io/crates/stratadb) | `cargo add stratadb` |

## Documentation

- [Getting Started](https://stratadb.org/docs/getting-started) — installation and first database
- [Concepts](docs/concepts/index.md) — branches, primitives, transactions, durability, time-travel
- [Guides](docs/guides/index.md) — per-primitive walkthroughs
- [Cookbook](docs/cookbook/index.md) — agent state, multi-agent coordination, RAG, A/B testing, replay
- [API Reference](docs/reference/api-quick-reference.md) — every method at a glance
- [Python SDK](docs/reference/python-sdk.md) | [Node.js SDK](docs/reference/node-sdk.md) | [MCP Server](docs/reference/mcp-reference.md)
- [Architecture](docs/architecture/index.md) — storage engine, durability, concurrency model
- [FAQ](docs/faq.md) | [Troubleshooting](docs/troubleshooting.md)
- [Contributing](CONTRIBUTING.md) — development setup and PR process
- [Roadmap](roadmap/README.md)

## License

[Apache License 2.0](LICENSE)
