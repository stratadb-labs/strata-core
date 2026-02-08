# Strata Cloud Sync: Edge Architecture

**Companion to**: [strata-cloud-sync.md](./strata-cloud-sync.md)

**Theme**: Mitigate the cons of the dumb hub without building a server database. Stateless compute at the boundary, intelligence in the client, durability in object storage.

**Design boundary**: This architecture is optimized for **agent-to-agent collaboration** — autonomous agents doing batch-style work (run task, push results, pull context). It is not designed for real-time human collaboration where two users expect low-latency visibility into each other's edits. Explicit push/pull will feel slow for interactive multi-user applications regardless of webhook notifications. That's not a flaw — it's a deliberate scope decision that keeps the system simple and offline-first.

---

## The problem with two extremes

The [cloud sync proposal](./strata-cloud-sync.md) describes a **dumb hub** — blob storage plus API key auth. All merge and sync logic runs on the client. This is simple and cheap but leaves gaps: no server-side validation, no compaction, no event notifications, limited access control, slow clones at scale.

The opposite — a **full server mode** — would require Strata to build connection pooling, wire protocols, encryption, user management, session handling, caching, and latency management. An enormous surface area that doesn't serve the core use case.

There's a third path.

---

## The third way: stateless edge compute

Keep R2/S3 as the storage layer. Keep all merge logic on the client. But let the hub run **short-lived, stateless functions** on ingest and retrieval — Cloudflare Workers, Lambda@Edge, or similar. No long-running process, no connection state, no Strata runtime on the server.

The Workers are disposable. They validate, transform, and route, but hold no state. If you delete every Worker and redeploy, nothing is lost. The blobs in R2 are the only source of truth.

---

## Sync logic lives in strata-core, not the SDKs

A key architectural point: the sync protocol is **not** something each SDK reimplements. The layered crate structure already handles this:

```
strata-core (Rust)
    └── strata-sync crate
        ├── delta computation (diff against origin mirror)
        ├── BranchlogPayload serialization / deserialization
        ├── origin mirror management
        ├── remote config storage
        ├── push / pull orchestration
        └── HTTP client (ureq)

Python SDK ─── thin FFI wrapper ───┐
Node.js SDK ── thin FFI wrapper ───┤── calls into strata-sync (Rust)
MCP server ─── thin wrapper ───────┘
```

Each SDK exposes four methods — `remote_add`, `push`, `pull`, `sync_status` — as thin wrappers over the Rust implementation. The serialization format, version negotiation, origin mirror bookkeeping, and HTTP transport are implemented once in Rust and shared across all surfaces.

This eliminates the "client complexity burden" concern from the dumb hub analysis. There is one implementation of the sync protocol, maintained alongside the storage engine, tested in CI, and consumed by every SDK through FFI.

---

## What the edge layer adds

### 1. Push validation without a server

The hub already receives blobs on push. A Worker deserializes the `BranchlogPayload` header (not the full data), validates structure, and rejects malformed pushes. No Strata runtime needed.

```
PUT /push → Worker validates:
    from_version == cursor.version    no version gaps
    entry_count > 0                   no empty pushes
    CRC32 matches                     integrity check
    blob_size < limit                 abuse prevention
```

This is validation at the boundary, not merge logic. Stateless, fast, milliseconds per request.

### 2. Delta compaction as a background job

A scheduled Worker (cron trigger) concatenates small delta blobs into larger ones. This is raw blob concatenation with a new header — no Strata runtime, no merge logic.

```
Scheduled: compact(branch)
    blobs = list_blobs(branch)              0001-0002, 0003-0004, 0005-0010
    if blobs.count > threshold:
        merged = concat_with_header(blobs)  single blob: 0001-0010.branchlog
        write merged
        update manifest
        delete originals after GC delay
```

Over time, a branch that accumulated 200 small delta files gets compacted into a handful of large ones. Download fewer files, faster pulls.

**Concurrent pull safety.** If a client is mid-pull (downloading blob 3 of 5) and compaction deletes blob 3 by merging it into a larger file, the client gets a 404. Two mitigations:

1. **Immutable blobs with GC delay.** Compaction writes the merged blob but keeps originals alive for a grace period (e.g., 10 minutes). A separate GC pass deletes originals after the grace window. Clients that started a pull before compaction still find the old blobs.
2. **Manifest-based resolution.** Instead of clients listing blobs directly, the cursor (or a manifest file) lists the blob names and version ranges to download. Clients resolve against the manifest, not the bucket listing. Compaction atomically updates the manifest to point to the new merged blob. Clients that already fetched the old manifest continue downloading old blobs (still alive during GC delay); new clients get the compacted view.

The manifest approach is more robust and should be the default. The cursor.json already exists — it can be extended to include a blob list:

```json
{
    "version": 47,
    "blobs": ["0001-0030.branchlog", "0031-0047.branchlog"]
}
```

### 3. Pre-built snapshots for fast clones

Same pattern as compaction. A background job periodically builds a full branch bundle from all deltas. New clones download one blob instead of dozens.

The client already knows how to import bundles — the `.branchbundle.tar.zst` format exists today. The hub pre-computes what the client would otherwise reconstruct from deltas.

```
R2: acme/agent-memory/main/
    snapshot-v0047.branchbundle     full state at version 47
    0048-0052.branchlog             deltas since snapshot
    cursor.json                     { "version": 52, "snapshot": 47 }
```

Clone path: download snapshot + any deltas after it. Incremental pull path unchanged.

### 4. Event-driven sync via webhooks

On push, the Worker emits an event — webhook POST, Cloudflare Queue, or SQS message. Agents subscribe and auto-pull instead of polling.

```
POST push → store blob → emit event:
    { project: "acme/agent-memory", branch: "main", version: 47, pushed_by: "agent-a" }
```

This isn't real-time sync (no persistent connection, no CRDT). It's "something changed, pull when ready." Cheap, stateless, and covers 80% of real-time use cases. The client-side implementation is simple: a listener that calls `db.pull()` when an event arrives.

**Delivery guarantees.** Reliable webhook delivery is notoriously hard (retries, ordering, deduplication, dead endpoints). For v1, treat this as **best-effort notification** — fire and forget. Clients must fall back to polling if they miss an event. Don't promise at-least-once delivery until a proper retry queue (Cloudflare Queue, SQS with DLQ) is in place. The protocol should be designed so that missed webhooks are harmless — a pull always reconciles to the correct state regardless of whether the trigger was a webhook or a manual poll.

### 5. Scoped access tokens without user management

Instead of building RBAC with a user database, generate tokens with embedded capabilities. Signed JWTs with claims — the Worker validates the signature (stateless, no database lookup).

```json
{
    "sub": "agent-b",
    "scope": "acme/agent-memory",
    "branches": ["main"],
    "actions": ["pull"],
    "exp": 1720000000
}
```

This enables:
- **Read-only tokens** — agents that can pull but not push
- **Branch-scoped tokens** — limit access to specific branches
- **Time-limited tokens** — expire after a task completes
- **Per-agent identity** — audit log shows which agent pushed what

No session management, no user table, no OAuth. Just cryptographic verification at the edge.

**Revocation caveat.** Stateless JWTs cannot be revoked without either short expiry windows (forcing frequent token rotation) or a small revocation list checked at the edge (reintroducing state). For agent workloads with short-lived tasks, short-expiry tokens (minutes to hours) are sufficient — the orchestrator mints a token per task, and it expires when the task is done. For long-running agents, consider a lightweight revocation check against a KV store (Cloudflare KV, R2 metadata) — a single key lookup per request, not a full user database.

### 7. NL query inference at the edge

Natural language search (v0.12–v0.13) requires a decoder LLM (Qwen3-1.3B) to decompose user queries into typed sub-queries. Running this model inside the embedded database costs ~1.4 GB RAM and forces a build-time choice between inference runtimes (candle vs llama.cpp). Moving inference to the edge resolves both problems.

**The key insight: query decomposition is a stateless function.** It takes a natural language string and returns typed sub-queries. It doesn't need access to the database — the client executes the sub-queries locally against its own data.

```
Client (embedded)                       StrataHub Edge
    │                                        │
    ├── NL query: "what tools did the        │
    │    agent use in the last hour?" ──────►│
    │                                        ├── Qwen3 inference
    │                                        │   (Worker AI / GPU endpoint)
    │   ◄── sub-queries: ───────────────────┤
    │       event: type=tool_call, time>1h   │
    │       KV: prefix "agent:tool:"         │
    │       vector: "tools agent used"       │
    │                                        │
    ├── Execute locally (hybrid search)      │
    └── Return results                       │
```

**What this buys:**

- **Embedded footprint stays small.** MiniLM (~80 MB) stays on-device for auto-embed on writes. Qwen3 (~1.4 GB) moves to the edge, needed only at query time.
- **The candle vs llama.cpp decision goes away.** The edge runs whatever runtime it wants. Swap models, upgrade quantization, scale GPU — none of it affects the client binary.
- **v0.13 features (expansion, summarization, multi-step retrieval) become cheaper.** Multi-step retrieval means multiple LLM calls. On-device that's expensive; on the edge it's a few HTTP round-trips.
- **Feature flag simplification.** `intelligence-llm` becomes a client-side HTTP call to the edge, not a 1 GB model download. The feature flag controls whether the client calls the edge, not whether it loads a model.

**What it costs:**

- **Online dependency for NL search.** If the edge is unreachable, NL search is unavailable. Keyword + vector search still work locally. This is an acceptable degradation — you lose the query understanding layer, not the search layer itself.
- **Latency.** One HTTP round-trip (~50-100ms) before search begins. For multi-step retrieval (v0.13), 2-3 round-trips. Still fast for interactive use.
- **Query privacy.** The natural language query leaves the device. The data never does (search executes locally), but the query itself goes to the edge. For sensitive workloads, an on-device fallback (loading Qwen3 locally with the `intelligence-llm-local` feature flag) should remain an option.

**Edge deployment options:**

- Cloudflare Workers AI (built-in model hosting, same auth boundary as sync)
- A dedicated inference endpoint behind the same JWT auth layer
- Rate-limited per API key to control cost

**Scoped JWT for inference:**

```json
{
    "sub": "agent-a",
    "scope": "acme/agent-memory",
    "actions": ["pull", "infer"],
    "exp": 1720000000
}
```

The `infer` action authorizes NL query decomposition. Agents without it can still search — they just can't use natural language queries.

**API endpoint:**

```
POST /v1/projects/:owner/:name/infer/decompose
    Body: { "query": "what tools did the agent use?", "context": { "branch": "main" } }
    Response: { "sub_queries": [...], "model": "qwen3-1.3b-q4", "latency_ms": 45 }
```

The endpoint is stateless. It receives a query string, runs Qwen3, returns structured sub-queries. No database access, no blob storage interaction. It shares the auth layer with sync endpoints but is otherwise independent.

### 8. Origin mirror cost reduction (future optimization)

> **Status: deferred.** v1 should use a full branch copy for the origin mirror. This is simple, correct, and consistent with how branch_diff already works. The hash manifest optimization introduces a second consistency model (hashes vs. full data) and adds risk of subtle diff bugs. Revisit once there is real-world data on branch sizes and storage pressure.

The idea: instead of storing the origin mirror as a full branch copy (doubling local storage), store it as a **version watermark plus hash manifest**:

```
_origin:hub:main → {
    version: 47,
    hashes: {
        "config": 0x1A2B3C4D,
        "task:1": 0x5E6F7A8B,
        "task:2": 0x9C0D1E2F,
        ...
    }
}
```

Diff computation checks hashes instead of comparing full values. Trades CPU (rehashing on push) for storage (no duplicate data). For large branches with big values, this could cut local storage overhead significantly.

The tradeoff: on push, the client must read and hash current values to compute the diff. For branches with many large entries, this adds latency to push. This also means the origin mirror can no longer be used as a data source — only as a comparison reference. Needs benchmarking with real workloads before committing.

---

## Architecture diagram

```
Client (embedded Strata)           Edge Workers (stateless)          Object Storage (R2/S3)
    │                                    │                                │
    │  strata-sync crate                 │                                │
    │  (diff, serialize, mirror)         │                                │
    │                                    │                                │
    ├── push ───────────────────────►    │                                │
    │                                    ├── validate header              │
    │                                    ├── check version continuity     │
    │                                    ├── store blob ─────────────────►│
    │                                    ├── emit webhook event           │
    │                                    │                                │
    │                               [cron]── compact old blobs ─────────►│
    │                               [cron]── build snapshot bundle ─────►│
    │                                    │                                │
    │   ◄─────────────────── pull ───────┤◄── serve blob ────────────────┤
    │                                    ├── validate scoped JWT          │
    │                                    │                                │
    │  NL search:                        │                                │
    ├── "what tools did agent use?" ────►│                                │
    │                                    ├── Qwen3 inference              │
    │   ◄── sub-queries ────────────────┤   (Workers AI / GPU)           │
    ├── execute locally (hybrid search)  │                                │
    │                                    │                                │
    │  Python SDK ── FFI ──► strata-sync │                                │
    │  Node SDK ─── FFI ──► strata-sync  │                                │
    │  MCP ──────── FFI ──► strata-sync  │                                │
```

---

## What the hub still doesn't do

Even with the edge layer, the hub never:

- Runs a Strata database instance
- Executes merge logic
- Parses full `BranchlogPayload` contents (only headers)
- Maintains persistent connections
- Stores session state
- Accesses or indexes user data (NL inference operates on query text only, never on database contents)

The principle is refined: **durability in object storage, data intelligence in the client, query understanding at the edge, operational concerns at the boundary.** MiniLM stays embedded (it needs data access for auto-embed on writes). Qwen3 moves to the edge (it only needs the query string). The Workers add operational smarts and NL capabilities without crossing into database server territory.

---

## Comparison

| Concern | Dumb hub | Edge architecture | Server mode |
|---|---|---|---|
| Push validation | None | Header + version check | Full schema validation |
| Compaction | Manual / never | Automated background job | Transparent |
| Clone speed | Download all deltas | Snapshot + recent deltas | Stream from server |
| Event notification | Polling only | Webhooks on push | WebSocket / SSE |
| Access control | API key (all-or-nothing) | Scoped JWTs | Full RBAC |
| NL search inference | On-device (~1.4 GB) | Edge inference (0 MB client) | Server-side |
| Sync logic location | strata-core | strata-core | Server + client |
| Server runtime | None | Stateless functions | Long-running process |
| Strata on server | No | No | Yes |
| Operational cost | Near zero | Low (pay per invocation) | Moderate (always-on) |

---

## Implementation impact

This doesn't change the phasing from the [main proposal](./strata-cloud-sync.md). The edge capabilities layer on top:

- **Phase 1 (delta protocol)** — unchanged, strata-core work
- **Phase 2 (HTTP client)** — unchanged, lives in strata-sync crate
- **Phase 3 (StrataHub)** — the Worker gains validation and JWT checks instead of bare API key auth
- **Phase 3.5 (edge jobs)** — add compaction and snapshot cron jobs
- **Phase 4 (SDK surface)** — simplified, since SDKs are thin FFI wrappers over strata-sync
- **Phase 5 (webhooks)** — emit events on push, provide subscription endpoint
- **Phase 6 (NL inference)** — deploy Qwen3 at the edge, wire client-side NL search to call the inference endpoint, add `infer` JWT scope
