# StrataDB Roadmap

StrataDB is an embedded database designed for AI agents, providing six primitives (KV, Event, State, JSON, Vector, Branch) with ACID transactions, snapshot isolation, and crash-safe durability.

**Current release**: v0.1.0 (embedded library)

**Direction**: Embedded-first, with intelligence features, server mode, and multi-language SDKs planned.

## Feature milestones

| Version | Theme | Status |
|---------|-------|--------|
| [v0.2](v0.2-branch-operations.md) | Branch operations | Planned |
| [v0.3](v0.3-storage-efficiency.md) | Storage efficiency | Planned |
| [v0.4](v0.4-vector-enhancements.md) | Vector search at scale | Planned |
| [v0.5](v0.5-advanced-queries.md) | Advanced queries | Planned |
| [v0.6](v0.6-server-mode.md) | Server mode | Planned |
| [v0.7](v0.7-observability.md) | Observability | Planned |
| [v1.0](v1.0-stable.md) | Stable release | Planned |
| [Future](future.md) | Post-1.0 exploration | Planned |

## Cross-cutting initiatives

These are not versioned milestones — they cut across releases and have their own sequencing.

| Initiative | Summary |
|------------|---------|
| [Performance characterization](performance-characterization.md) | Benchmark suite (all primitives x all durability modes) and hardware scaling study (RPi to Xeon) |
| [Engine & storage optimization](engine-storage-optimization.md) | Data-driven rewrites based on benchmark results |
| [strata-security](strata-security.md) | Access control crate — read-only/read-write now, per-connection auth for server mode later |
| [Intelligence: Indexing](intelligence-indexing.md) | Reduce search latency through indexing (index type TBD, tracking SOTA) |
| [Intelligence: Internal graph](intelligence-graph.md) | Internal graph structure for relationship-aware queries across primitives |
| [Intelligence: Embedded inference](intelligence-inference.md) | Nano inference runtime — MiniLM for auto-embedding, Qwen3 for natural language search |
| [SDKs and MCP server](sdks-and-mcp.md) | Python SDK (PyO3), Node SDK (NAPI-RS), MCP server — thin wrappers over Command/Output |
| [Websites](websites.md) | stratadb.org (docs, benchmarks) and stratadb.ai (live WASM demos) |

## Sequencing

```
Black-box tests
    │
    ▼
Performance characterization ──→ Engine & storage optimization
    │
    ▼
strata-security (phase 1)
    │
    ├──→ Intelligence: Indexing ──→ Intelligence: Graph
    │                                    │
    │                                    ▼
    │                           Intelligence: Inference
    │
    ├──→ SDKs and MCP server
    │
    └──→ Websites: stratadb.org ──→ WASM build ──→ stratadb.ai
```

## What shipped in v0.1.0

- 6 primitives: KV, Event, State, JSON, Vector, Branch
- OCC transactions with snapshot isolation
- 3 durability modes: InMemory, Buffered, Strict
- WAL with CRC32 checksums and crash recovery
- Periodic snapshots with bounded recovery time
- Branch isolation (data separated by branch)
- Branch bundles (export/import as `.branchbundle.tar.zst`)
- Hybrid search with BM25 scoring and Reciprocal Rank Fusion
- 7-crate workspace architecture

## How to read this roadmap

Each document describes:
- **Theme**: The unifying goal
- **Features/Scope**: What will be built, with references to existing code where applicable
- **Dependencies**: What must ship first
- **Open questions**: What we don't know yet

Milestones are ordered by priority but not committed to specific dates. Features may shift between milestones as development progresses.
