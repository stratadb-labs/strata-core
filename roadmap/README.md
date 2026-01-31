# StrataDB Feature Roadmap

StrataDB is an embedded database designed for AI agents, providing six primitives (KV, Event, State, JSON, Vector, Branch) with ACID transactions, snapshot isolation, and crash-safe durability.

**Current release**: v0.1.0 (embedded library)

**Direction**: Embedded-first, with an optional server mode planned for multi-process access.

## Milestones

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

Each milestone file describes:
- **Theme**: The unifying goal for that release
- **Features**: What will be built, with references to existing code where applicable
- **Context**: Why this ordering makes sense
- **Dependencies**: What must ship before this milestone

Milestones are ordered by priority but not committed to specific dates. Features may shift between milestones as development progresses.
