# Strata Product Surfaces

> **Status**: MVP Planning
> **Stability**: Subject to change based on user feedback
> **Scope**: Capabilities planned for MVP (M1-M14)

---

## Purpose

This document describes **product features** planned for MVP. These are built on top of the Primitive Contract and Core API.

For post-MVP vision (replay, diff, branch, explain, temporal search), see [MAGIC_APIS.md](MAGIC_APIS.md).

---

## Relationship to Core

| Layer | Document | Stability |
|-------|----------|-----------|
| Invariants | PRIMITIVE_CONTRACT.md | Constitutional (changes require RFC) |
| API Shape | CORE_API_SHAPE.md | Stable (changes require migration path) |
| **Product Surfaces** | This document | Evolving (can change with user needs) |
| Post-MVP Vision | MAGIC_APIS.md | Aspirational |

---

## Relationship to Deployment Modes

**Strata is an embedded library.** Product surfaces work in both deployment modes:

| Surface | Embedded Mode | Server Mode |
|---------|---------------|-------------|
| Search | Direct API calls | Same API over wire protocol |
| CLI | N/A (use library) | Connects to server |
| Wire Protocol | N/A | Transport layer |
| Language SDKs | N/A (Rust is native) | Connect via wire protocol |
| Observability | Embedded metrics | Server metrics + distributed tracing |
| Administration | Programmatic | CLI + admin endpoints |

**All semantics live in the embedded library.** Server mode and client SDKs are transport—they expose the same features over the network.

---

## Surface 1: Search (Complete - M6)

### Current State

- Each primitive has a `search()` method
- `SearchRequest` specifies query, filters, budget
- `SearchResponse` returns ranked hits
- Hybrid search (keyword + semantic) via RRF fusion

### Contract

- Every primitive accepts a `SearchRequest` and returns a `SearchResponse`
- Search is a feature, not an invariant—primitives can return empty results
- Search results reflect a snapshot but may not be transactionally consistent with subsequent reads

---

## Surface 2: CLI (Planned - M10)

### Current State

- Not implemented
- Planned for M10 (Server & Wire Protocol)

### Design

The CLI connects to `strata-server`. It does not embed the library.

```
strata <global-options> <command> <subcommand> [args]

Global options:
  --server <addr>   Server address (default: 127.0.0.1:6380)
  --format <fmt>    Output format (human, json, compact)

Commands:
  run               Run lifecycle management
  kv                Key-value operations
  event             Event log operations
  state             State cell operations
  trace             Trace operations
  json              JSON document operations
  vector            Vector operations
  search            Cross-primitive search
  admin             Server administration (health, stats)
```

---

## Surface 3: Wire Protocol (Planned - M10)

### Current State

- Not implemented
- Strata is currently embedded-only

### Design Principles

1. **One protocol**: All primitives accessible through same protocol
2. **No new semantics**: Server adds transport, not features
3. **Binary efficient**: Not HTTP/JSON for hot paths
4. **Schema evolution**: Protocol can evolve without breaking clients

The wire protocol is a **faithful serialization** of the embedded API. If you can do it through the server, you can do it through the embedded API, and vice versa.

---

## Surface 4: Python SDK (Planned - M12)

### Current State

- Rust SDK (native, always available)
- Python SDK planned for M12

### Design Principles

1. **Idiomatic**: Context managers for transactions, type hints, asyncio
2. **Complete**: All primitive operations accessible
3. **Consistent**: Same concepts as Rust API

---

## Surface 5: Security & Multi-Tenancy (Planned - M13)

### Current State

- Not implemented
- Planned for M13

### Scope

**Embedded Mode**: Security is the application's responsibility. Library trusts the caller.

**Server Mode**:
- Authentication (API keys, JWT)
- Authorization (RBAC, primitive-level permissions)
- Multi-tenancy (tenant isolation, resource limits)
- TLS encryption

---

## Surface 6: Observability (Planned - M14)

### Current State

- Basic logging
- No metrics export
- No distributed tracing

### Scope

**Embedded Mode**: Library exposes metrics via callbacks or shared state.

**Server Mode**:
- `/metrics` endpoint (Prometheus)
- Distributed tracing
- Health checks (`/health`, `/ready`)

---

## Surface 7: Administration (Planned - M14)

### Current State

- Manual WAL management
- No online operations

### Scope

**Embedded Mode**: Programmatic control via Rust API.

**Server Mode**:
- Admin endpoints
- Backup/restore
- Graceful shutdown
- Configuration management

---

## Prioritization

| Surface | Status | Milestone |
|---------|--------|-----------|
| Search | **Done** | M6 |
| Wire Protocol | Planned | M10 |
| CLI | Planned | M10 |
| Python SDK | Planned | M12 |
| Security | Planned | M13 |
| Observability | Planned | M14 |
| Administration | Planned | M14 |

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-19 | Initial product surfaces extracted from Universal Primitive Protocol |
| 1.1 | 2026-01-19 | Added embedded-first context; aligned with M10-M14; added Security surface |
| 1.2 | 2026-01-19 | Integrated Magic APIs vision |
| 2.0 | 2026-01-19 | Simplified to MVP-only scope; moved future vision to MAGIC_APIS.md |
