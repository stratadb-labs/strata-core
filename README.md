# StrataDB

**An embedded database for AI agents — six primitives, branch isolation, and deterministic replay.**

```
$ strata --cache
strata:default/default> kv put agent:status thinking
(version) 1
strata:default/default> event append tool_call '{"tool":"search","query":"docs"}'
(seq) 1
strata:default/default> state cas lock none acquired
(version) 1
```

## Primitives

| Primitive | Purpose | CLI Commands |
|-----------|---------|-------------|
| **KV Store** | Working memory, config, scratchpads | `kv put`, `kv get`, `kv del`, `kv list` |
| **Event Log** | Immutable audit trail, tool call history | `event append`, `event get`, `event list` |
| **State Cell** | CAS-based coordination, counters, locks | `state set`, `state get`, `state cas`, `state init` |
| **JSON Store** | Structured documents with path-level mutations | `json set`, `json get`, `json del`, `json list` |
| **Vector Store** | Embeddings and similarity search (brute-force + HNSW) | `vector upsert`, `vector search`, `vector batch-upsert` |
| **Branch** | Data isolation (like git branches) | `branch create`, `branch list`, `branch del`, `use` |

## Spaces

Spaces are organizational namespaces within branches. Each space has its own independent instance of every primitive:

```
$ strata --cache
strata:default/default> kv put key value
(version) 1
strata:default/default> use default conversations
strata:default/conversations> kv put msg_001 hello
(version) 1
strata:default/conversations> space list
- conversations
- default
strata:default/conversations> use default
strata:default/default>
```

Spaces are auto-created on first write. The `default` space always exists.

## Installation

```bash
# Install the CLI (requires Rust toolchain)
cargo install strata-cli

# Or build from source
git clone https://github.com/anibjoshi/strata.git
cd strata
cargo build --release
```

## Quick Example

```
$ strata --db ./my-data
strata:default/default> kv put user:name Alice
(version) 1
strata:default/default> kv put user:score 42
(version) 1
strata:default/default> branch create experiment-1
OK
strata:default/default> use experiment-1
strata:experiment-1/default> kv get user:name
(nil)
strata:experiment-1/default> use default
strata:default/default> kv get user:name
"Alice"
```

Or from the shell:

```bash
strata --db ./my-data kv put user:name Alice
strata --db ./my-data kv put user:score 42
strata --db ./my-data branch create experiment-1
strata --db ./my-data --branch experiment-1 kv get user:name   # → (nil)
strata --db ./my-data kv get user:name                         # → "Alice"
```

## Vector Search

StrataDB includes a built-in vector store with two index backends for similarity search:

| Backend | Complexity | Best For |
|---------|-----------|----------|
| **Brute Force** | O(n) exact search | Small collections (< 10K vectors) |
| **HNSW** | O(log n) approximate search | Large collections (10K+ vectors) |

```
$ strata --cache
strata:default/default> vector create embeddings 384 --metric cosine
OK
strata:default/default> vector upsert embeddings doc-1 [0.1,0.2,...] --metadata '{"type":"article"}'
OK
strata:default/default> vector search embeddings [0.1,0.2,...] 10
key=doc-1 score=0.9823
...
strata:default/default> vector stats embeddings
count: 1, memory_bytes: 1536, index_type: flat
```

**Metadata filtering** supports 8 operators: `eq`, `ne`, `gt`, `gte`, `lt`, `lte`, `in`, `contains`.

## Durability

Choose your speed/safety trade-off:

| Mode | Latency | Throughput | Data Loss on Crash |
|------|---------|------------|-------------------|
| **Ephemeral** | <3 us | 250K+ ops/sec | All |
| **Buffered** | <30 us | 50K+ ops/sec | Last ~100ms |
| **Strict** | ~2 ms | ~500 ops/sec | None |

## Architecture

```
+-----------------------------------------------------------+
|  Strata API (KV, Event, State, JSON, Vector, Branch, Space)|
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
                   +---------v---------+  +--------------+
                   |  Storage          |  |  Security    |
                   |  (ShardedStore)   |  |  (Access)    |
                   +-------------------+  +--------------+
```

**Key design choices:**

- **Unified storage** — all primitives share one sharded map, enabling atomic multi-primitive transactions
- **Branch-tagged keys** — every key includes its branch ID, making replay O(branch size)
- **Optimistic concurrency** — lock-free transactions via compare-and-swap; agents rarely conflict
- **Batched durability** — fsync batched by default; losing 100ms of work is acceptable for most agents
- **Pluggable vector indexing** — swap between brute-force O(n) and HNSW O(log n) per collection
- **Space-scoped data** — within each branch, data is further organized into spaces, enabling logical separation without branch overhead

## Documentation

- [Documentation Hub](docs/index.md) — start here
- [Getting Started](docs/getting-started/installation.md) — installation and first database
- [Concepts](docs/concepts/index.md) — branches, primitives, value types, transactions, durability
- [Guides](docs/guides/kv-store.md) — per-primitive walkthroughs
- [Cookbook](docs/cookbook/index.md) — real-world patterns
- [API Reference](docs/reference/api-quick-reference.md) — every method at a glance
- [Architecture](docs/architecture/index.md) — how StrataDB works internally
- [Roadmap](roadmap/README.md) — feature roadmap from v0.2 to v1.0+
- [Contributing](CONTRIBUTING.md) — development setup and PR process

## License

[Apache License 2.0](LICENSE)
