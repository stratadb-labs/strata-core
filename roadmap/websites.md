# Websites: stratadb.org and stratadb.ai

**Theme**: Two domains, two audiences, one database.

## stratadb.org — The Engine

The authoritative home for StrataDB as an open-source embedded database.

### Purpose

Convince a senior engineer in 30 seconds that StrataDB is worth evaluating. Then give them everything they need to go from `cargo add` to production.

### Content

- **Landing page**: What it is, the six primitives table, the durability modes table, a code sample, install instructions
- **Documentation**: The `docs/` folder rendered as a proper doc site (getting started, concepts, guides, cookbook, API reference, architecture)
- **Benchmarks**: Published results from the performance characterization — throughput tables, latency percentiles, comparison charts against redb/LMDB/SQLite
- **Roadmap**: The `roadmap/` folder rendered as a timeline/status page
- **Blog**: Release notes, design decisions, benchmark deep-dives
- **GitHub link**: Prominent, not buried

### Tech

- Static site generator (mdBook, Docusaurus, or similar)
- Content sourced directly from the repo's `docs/` and `roadmap/` markdown
- Deployed on a CDN (Cloudflare Pages, Netlify, or similar)
- No backend, no database (ironic but correct — static is the right choice for docs)

## stratadb.ai — The Intelligence Layer

A live, interactive demo of what makes StrataDB different from every other embedded database.

### The headline

> This page is running a live instance of StrataDB.

The database runs in the browser via WebAssembly. Not a mock, not a simulation, not a proxy to a server. The actual Rust engine compiled to WASM, executing in the user's browser tab.

### What the user experiences

1. **Live REPL**: Type Strata commands and see results immediately. Pre-loaded with a sample dataset (agent conversation logs, tool call history, embeddings).

2. **Auto-embedding demo**: Insert a piece of text, watch the MiniLM embedding get generated and the shadow vector collection update in real time. Show the embedding dimensions, the similarity scores against existing data.

3. **Natural language search**: Type a question in plain English. Show the query decomposition (what Qwen3 does), the multi-primitive search fan-out, the result re-ranking, and the final synthesized answer. Make the pipeline visible, not just the result.

4. **Branch isolation demo**: Create a branch, mutate data, switch back, show isolation. Visual diff between branches.

5. **Performance counter**: A live ops/sec counter running in the corner while the user interacts. "Your browser just executed 50,000 KV operations per second."

### WASM build

The Rust codebase compiles to `wasm32-unknown-unknown` with some constraints:

- **InMemory mode only** (no filesystem access in the browser)
- **No std::fs, no WAL, no snapshots** — feature-gated out for the WASM target
- **Intelligence layer**: MiniLM can run in WASM (ONNX Runtime has a WASM backend, or use candle's WASM support). Qwen3 in-browser is harder — may need to proxy to a lightweight backend or use a smaller model for the demo.
- **Target size**: The WASM binary should be under 5MB (before gzip). The engine without intelligence features is likely well under this. With MiniLM, the model weights add ~80MB — loaded lazily after page load.

```toml
# Cargo.toml for WASM build
[lib]
crate-type = ["cdylib"]

[dependencies]
stratadb = { path = "../..", default-features = false, features = ["wasm"] }
wasm-bindgen = "0.2"
```

### Architecture

```
Browser tab
├── stratadb.wasm          (~2-5MB, the database engine)
├── minilm.onnx            (~80MB, loaded lazily for embedding demo)
├── UI (React/Svelte/vanilla)
│   ├── REPL component
│   ├── Embedding visualizer
│   ├── Search pipeline visualizer
│   ├── Branch diff viewer
│   └── Performance counter
└── JS glue (wasm-bindgen)
    └── Maps JS calls → Strata Command/Output
```

### Qwen3 in the demo

Running 1.3B parameters in a browser tab is possible but heavy. Options:

| Approach | Tradeoff |
|----------|----------|
| **Full in-browser** (WebGPU + llama.cpp WASM) | Impressive demo, requires WebGPU-capable browser, slow on older hardware |
| **Thin backend proxy** | Fast, reliable, but "running in your browser" claim is partially false |
| **Smaller model substitute** (Qwen3-0.6B or similar) | Honest in-browser, reduced quality |

The right choice depends on what's available when we build it. WebGPU adoption is accelerating.

### What stratadb.ai is NOT

- Not a hosted StrataDB service (the database runs client-side)
- Not a playground with a server backend pretending to be local
- Not a marketing site with screenshots (every demo is live)

## Shared concerns

### Domain separation

| | stratadb.org | stratadb.ai |
|--|-------------|-------------|
| **Audience** | Engineers evaluating a database | Anyone curious about AI-native databases |
| **Tone** | Technical, precise, referential | Interactive, visual, exploratory |
| **Content** | Docs, benchmarks, architecture | Live demos, visualizations |
| **Backend** | None (static) | None (WASM) or minimal (Qwen3 proxy) |
| **Update cadence** | Every release | When demos change |

### Cross-linking

- stratadb.org links to stratadb.ai for "try it live"
- stratadb.ai links to stratadb.org for "read the docs"
- Both link to GitHub

## Dependencies

- WASM build target requires feature-gating filesystem-dependent code (WAL, snapshots, durability)
- Intelligence inference runtime must support WASM (at least for MiniLM)
- The core engine and public API should be stable before building SDK/WASM wrappers around them
- Performance characterization must complete before publishing benchmark pages on stratadb.org

## Ordering

1. **stratadb.org first** — static doc site, can ship as soon as docs are stable
2. **WASM build target** — get the engine compiling to WASM with InMemory mode
3. **stratadb.ai** — build the interactive demos on top of the WASM build
