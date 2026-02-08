# Strata Roadmap

Other databases store data for AI. Strata *is* AI infrastructure. It doesn't just hold agent memory — it organizes it, understands it, and enables reasoning over it.

The roadmap has three acts:

1. **Branches as the unit of agent thought** — fork, explore, diff, merge, time-travel, sandboxing
2. **The database that understands itself** — auto-embedding, natural language search, cross-primitive knowledge graph
3. **Agents that think in branches** — parallel planning, speculative execution, evaluation harness

Each is individually differentiated. Together, they're a paradigm shift.

---

## What's Shipped

### v0.1 — Foundation

6 primitives (KV, Event, State, JSON, Vector, Branch), OCC transactions, snapshot isolation, 3 durability modes, WAL with crash recovery, branch bundles, hybrid search with BM25 + RRF, 7-crate architecture.

### v0.2–v0.4 — Hardening + Vector Intelligence

HNSW index backend (~95%+ recall, built from scratch), advanced metadata filters (8 operators), batch vector upsert, collection statistics, reserved `_system_*` namespace, read-only access mode (`strata-security` crate), BTreeSet prefix index, transaction fast paths.

### v0.5 — MVP

Spaces (lightweight namespace isolation within branches), structured logging with `tracing`, CLI with REPL mode. Branch power operations: fork (snapshot copy with metadata), diff (compute delta between branches), merge (apply changes with conflict detection, last-writer-wins or strict strategies). MCP server exposing 47 tools.

### v0.6 — Python and Node SDKs

PyO3-based Python SDK ([strata-python](https://github.com/strata-systems/strata-python)) and NAPI-RS Node.js SDK ([strata-node](https://github.com/strata-systems/strata-node)). Both expose all six primitives with comprehensive test suites.

---

## Roadmap

| Version | Theme | Depends on |
|---------|-------|------------|
| [**v0.5: MVP**](v0.5-mvp.md) | Spaces, branch operations, structured logging, MCP server | Foundation | **Shipped** |
| [**v0.6: SDKs**](v0.6-sdks.md) | Python (PyO3) and Node.js (NAPI-RS) | v0.5 | **Shipped** |
| [**v0.7: Auto-Embedding**](v0.7-auto-embedding.md) | MiniLM auto-embedding pipeline | v0.5 |
| [**v0.8: Enhanced Hybrid Search**](v0.8-enhanced-hybrid-search.md) | MiniLM vectors in RRF, new retrieval signals, internal knowledge graph | v0.7 |
| [**v0.12: NL Search (Basic)**](v0.12-nl-search-basic.md) | Qwen3 NL→query decomposition | v0.8 |
| [**v0.13: NL Search (Advanced)**](v0.13-nl-search-advanced.md) | Query expansion, result summarization, multi-step retrieval | v0.12 |
| [**v0.11: Advanced Branch Workflows**](v0.11-advanced-branch-workflows.md) | Time-travel queries (v0.11.1), branch sandboxing (v0.11.2) | v0.5 |
| [**Document Primitive**](markdown-documents.md) | Composite document store with section-level search via existing `Search` | v0.7 |
| [**v0.14: Sophisticated Intelligence**](v0.14-sophisticated-intelligence.md) | Fine-tuned models, multi-turn context, agentic workflows | v0.13 |
| [**v1.0: Stable Release**](v1.0-stable.md) | Storage efficiency, engine optimizations, format freeze, semver | v0.5–v0.14 |
| [**Strata Cloud Sync**](strata-cloud-sync.md) | StrataHub, push/pull branches, multi-agent collaboration | v0.5 |
| [**Post-1.0: Scaling**](post-1.0-scaling.md) | Server mode, replication, sharding, agent runtime, WASM | v1.0 |

---

## Sequencing

```
                     SHIPPED
            ┌─────────────────────┐
            │  v0.1 Foundation    │
            │  v0.2-v0.4 Vector   │
            │  + Security         │
            └──────────┬──────────┘
                       │
                    v0.5.1
              Spaces + Logging
                       │
                    v0.5.2
              Branch Operations
             (fork, diff, merge)
                       │
                    v0.5.3
                  MCP Server
                       │
              ┌────────┴────────┐
              │                 │
            v0.6            v0.11.1          v0.11.2
            SDKs         Time-Travel       Sandboxing
       (Python/Node)    (WAL index +      (Policy guard
              │          historical         + scoped
              │           queries)           merges)
              │
         ┌────┴────┐
         │         │
       v0.7      v0.8
   Auto-Embedding  Enhanced
      (MiniLM)   Hybrid Search
         │     + Knowledge Graph
         │            │
      Document      v0.12
     Primitive   NL Search
                  (Basic)
                     │
                  v0.13
               NL Search
               (Advanced)
                     │
                  v0.14
              Sophisticated
              Intelligence
                     │
                ┌────┘
                │
              v1.0
           Stable Release
                │
            Post-1.0
             Scaling
```

**Critical path**: v0.5 → v0.6 → v0.7 → v0.8 → v0.12 → v0.13 → v0.14 → v1.0

**Independent tracks**: v0.11.1 (time-travel) and v0.11.2 (sandboxing) can proceed in parallel once v0.5 is complete; v0.11.1 recommended first as the WAL timestamp index is foundational. Document Primitive can proceed in parallel once v0.7 is complete.

> **Surface update policy**: Each v0.11 sub-release requires follow-up updates to CLI, MCP server (strata-mcp), Python SDK (strata-python), and Node.js SDK (strata-node) to expose new features.

---

## Design Principles

1. **Embedded-first**: Strata runs in-process. Cloud sync is an extension, not a replacement.
2. **Branches are the primitive**: Every feature composes with branch isolation. If it doesn't work per-branch, it's not designed right.
3. **Intelligence is opt-in**: Auto-embedding, NL search, and the inference runtime are feature-flagged. Without them, Strata is a fast, correct embedded database with zero model overhead.
4. **Command/Output protocol**: Every operation is a serializable command in, serializable output out. SDKs are thin, testing is uniform.
5. **Deterministic by default**: Seeded RNGs, sorted data structures, fixed tie-breaking. Same inputs, same outputs.
6. **No premature abstraction**: Build what's needed now.

---

## Shipped References

| Document | Description |
|----------|-------------|
| [v0.4 Vector Enhancements](v0.4-vector-enhancements.md) | HNSW, advanced filters, batch upsert, collection stats |
| [v0.5 MVP](v0.5-mvp.md) | Spaces, branch operations, structured logging, MCP server |
| [v0.6 SDKs](v0.6-sdks.md) | Python (PyO3) and Node.js (NAPI-RS) bindings |
