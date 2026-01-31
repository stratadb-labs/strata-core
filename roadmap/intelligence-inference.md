# Intelligence: Embedded Inference Runtime

**Theme**: A minimal inference runtime inside `strata-intelligence`, fixed to two models, turning StrataDB from a storage engine into a storage engine that understands its own data.

## Vision

Two models, two jobs:

| Model | Parameters | Role |
|-------|-----------|------|
| **all-MiniLM-L6-v2** | ~22M | Auto-embedding generation on insert |
| **Qwen3-1.3B** | ~1.3B | Natural language query interface for search and retrieval |

This is not a general-purpose inference server. It is a nano inference runtime — borrowing key ideas from vLLM (batched inference, KV cache reuse) — hardcoded to exactly these two models. No model zoo, no dynamic loading, no serving API.

## Auto-embedding pipeline

When data is inserted into any text-bearing primitive (KV string values, JSON documents, event payloads), the intelligence layer can automatically generate embeddings via MiniLM and populate a shadow vector collection.

```
User calls: db.kv_put("doc:123", "The agent completed the search task")
                │
                ▼
        ┌───────────────┐
        │ KV primitive   │  (stores the value)
        └───────┬───────┘
                │ if auto_embed is enabled
                ▼
        ┌───────────────┐
        │ MiniLM-L6-v2  │  (generates 384-dim embedding)
        └───────┬───────┘
                │
                ▼
        ┌───────────────┐
        │ Vector         │  (upserts to shadow collection)
        │ primitive      │
        └───────────────┘
```

### Configuration

Auto-embedding is opt-in, configurable per database or per branch:

```rust
let db = Strata::builder()
    .path("/data")
    .auto_embed(true)       // enable auto-embedding
    .embed_primitives(&[    // which primitives to embed
        "kv", "json", "event"
    ])
    .open()?;
```

When disabled, StrataDB behaves exactly as it does today — zero model overhead.

### Shadow collections

Auto-generated embeddings live in internal vector collections (e.g., `__auto_kv`, `__auto_json`) that are:
- Branch-scoped (same isolation as user data)
- Transparent to the user (search uses them automatically)
- Managed by the intelligence layer (created/deleted with the branch lifecycle)

## LLM-powered search

Qwen3-1.3B sits at the front of the search pipeline, turning natural language queries into multi-primitive search operations and synthesizing results.

### Query pipeline

```
User calls: db.search("what tools did the agent use in the last hour?")
                │
                ▼
        ┌───────────────┐
        │ Qwen3-1.3B    │  query understanding
        │                │  → decompose into sub-queries:
        │                │    1. event search: type=tool_call, recent
        │                │    2. KV search: prefix "agent:tool:"
        │                │    3. vector search: semantic similarity
        └───────┬───────┘
                │
                ▼
        ┌───────────────┐
        │ Multi-primitive│  (existing hybrid search + auto-embed vectors)
        │ search         │
        └───────┬───────┘
                │
                ▼
        ┌───────────────┐
        │ Qwen3-1.3B    │  result synthesis
        │                │  → re-rank, filter, summarize
        └───────┬───────┘
                │
                ▼
        Structured search results with relevance explanations
```

### What Qwen3 enables

- **Query decomposition**: Parse natural language into typed sub-queries across primitives
- **Result re-ranking**: Use language understanding to re-score results beyond BM25/cosine
- **Answer synthesis**: Combine results from multiple primitives into a coherent response
- **Temporal reasoning**: Understand "last hour", "before the error", "most recent" in query context

## Nano inference runtime

Key ideas borrowed from vLLM, implemented minimally:

### What to take from vLLM

- **Continuous batching**: If multiple embedding requests arrive (e.g., bulk insert), batch them through MiniLM in one forward pass instead of one-at-a-time
- **KV cache management**: For Qwen3, reuse key-value cache across the query-understanding and result-synthesis phases (same conversation context)
- **Quantization**: Run Qwen3 in Q4/Q8 quantization to fit in ~1-2GB RAM instead of ~2.6GB at f16

### What NOT to take from vLLM

- No PagedAttention (overkill for single-user embedded inference)
- No multi-GPU scheduling
- No HTTP serving layer
- No dynamic model loading
- No speculative decoding

### Runtime options

| Option | Pros | Cons |
|--------|------|------|
| **candle** (Rust-native) | Pure Rust, no C++ deps, compiles with cargo | Younger ecosystem, fewer optimized kernels |
| **llama.cpp** (via bindings) | Battle-tested, excellent quantization, broad hardware support | C++ build dependency, FFI complexity |
| **ONNX Runtime** (via bindings) | Mature, good MiniLM support, hardware abstraction | Large dependency, less suited for LLM generation |

The choice depends on how well each handles both the embedding model (MiniLM, encoder-only) and the generative model (Qwen3, decoder-only). A split approach (ONNX for MiniLM, llama.cpp for Qwen3) is also viable.

### Model distribution

- Models are **not** bundled in the crate (too large for crates.io)
- Downloaded on first use to a local cache directory (e.g., `~/.stratadb/models/`)
- Checksum-verified after download
- If models are not available, auto-embed and LLM search features are disabled — the database still works, just without intelligence features

## Memory footprint

| Component | RAM |
|-----------|-----|
| MiniLM-L6-v2 (f32) | ~80 MB |
| Qwen3-1.3B (Q4) | ~1 GB |
| Qwen3-1.3B (Q8) | ~1.5 GB |
| KV cache (Qwen3, short context) | ~100-200 MB |
| **Total (Q4 Qwen3)** | **~1.4 GB** |

This fits on every hardware tier from the scaling study except possibly a 2GB Raspberry Pi. Feature flags can disable the LLM components to run on constrained devices.

## Feature flags

```toml
[features]
default = []
intelligence-embed = []    # MiniLM auto-embedding only (~80MB RAM)
intelligence-llm = []      # Qwen3 search interface (~1-1.5GB additional RAM)
intelligence-full = ["intelligence-embed", "intelligence-llm"]
```

## Open questions

- **Sync vs. async inference**: Should embedding generation block the insert call, or happen asynchronously in a background thread? Async is better for write throughput but introduces eventual consistency for search.
- **Context window**: How much context to feed Qwen3 for query understanding — just the query string, or also recent conversation/search history?
- **Fine-tuning**: Are the off-the-shelf models good enough, or will Strata-specific fine-tuning be needed for query decomposition?
- **Model updates**: When better small models ship (there will be better ones), what's the upgrade path?

## Dependencies

- Intelligence indexing (auto-embeddings need efficient vector indexes to be useful at scale)
- Performance characterization (need to understand the latency budget — if a KV put takes 3us today, adding 1ms of embedding time is a 300x regression that must be async)
- Engine & storage optimization (inference puts sustained memory pressure on the system)
