# Future: Post-1.0 Exploration

**Theme**: Scaling beyond single-node embedded use cases.

These features are on the radar but not committed to any milestone. They represent potential directions based on how the AI agent ecosystem evolves.

## Potential Features

### Replication

Primary-backup replication for durability beyond a single machine.

- Stream WAL entries from primary to one or more replicas
- Replicas serve read-only queries (read scaling)
- Automatic failover when primary becomes unavailable
- Consistent with the existing WAL-based recovery model

### Horizontal Sharding

Distribute branches across multiple nodes.

- Branch is the natural sharding unit -- branches are already fully isolated
- Route operations to the node hosting the target branch
- Branch migration between nodes for rebalancing
- No cross-branch transactions (already the case in v0.1)

### Time-Series Primitive

Specialized primitive for time-indexed data.

- Ordered by timestamp with efficient range queries
- Downsampling and rollup aggregation
- Retention policies per time resolution
- Useful for agent telemetry, metrics, and activity logs

### Graph Operations

Relationship traversal between entities.

- Define edges between entities across primitives
- Path queries (shortest path, reachability)
- Graph-aware search (find related entities within N hops)
- Useful for knowledge graphs and agent memory networks

### Python Client Library

First-class Python support for AI/ML workflows.

- PyO3-based bindings to the Rust client library
- Pythonic API (context managers, iterators, type hints)
- NumPy integration for vector operations
- pip-installable package

### JavaScript/TypeScript Client

Client library for web and Node.js applications.

- WebAssembly build for browser-embedded use
- Node.js native addon for server-side use
- TypeScript type definitions

### Cloud-Managed Service

Hosted StrataDB as a service.

- Multi-tenant with branch-level isolation
- Managed backups, scaling, and monitoring
- REST/gRPC API compatible with the self-hosted wire protocol
- Usage-based pricing

## How features move from here to a milestone

A feature moves from "Future" to a versioned milestone when:

1. There is demonstrated user demand
2. The prerequisite infrastructure exists
3. The design is well-understood enough to estimate scope
4. It aligns with the project's current priorities
