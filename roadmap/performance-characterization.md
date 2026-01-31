# Performance Characterization

**Theme**: Understand StrataDB's performance envelope before optimizing.

This is a characterization effort, not a feature milestone. The goal is to establish baselines, identify bottlenecks, and understand how performance scales with hardware — before committing to optimization work.

## 1. Comprehensive Benchmark Suite

Systematic benchmarks across all primitives and durability modes.

### Matrix

Every cell in this matrix gets a benchmark:

|                        | InMemory | Buffered | Strict |
|------------------------|----------|----------|--------|
| **KV put**             |          |          |        |
| **KV get**             |          |          |        |
| **KV delete**          |          |          |        |
| **KV list (prefix)**   |          |          |        |
| **State set**          |          |          |        |
| **State read**         |          |          |        |
| **State CAS**          |          |          |        |
| **Event append**       |          |          |        |
| **Event read**         |          |          |        |
| **Event read by type** |          |          |        |
| **JSON set (root)**    |          |          |        |
| **JSON set (path)**    |          |          |        |
| **JSON get**           |          |          |        |
| **JSON list**          |          |          |        |
| **Vector upsert**      |          |          |        |
| **Vector search**      |          |          |        |
| **Vector get**         |          |          |        |
| **Branch create**      |          |          |        |
| **Branch switch**      |          |          |        |
| **Branch delete**      |          |          |        |

### Metrics per cell

- **Throughput**: ops/sec (single-threaded)
- **Latency**: p50, p95, p99 (microseconds)
- **Memory**: RSS delta per 10K operations

### Workload parameters

- KV: 100-byte keys, 1KB values
- Events: 512-byte JSON payloads
- JSON: 10-field documents, 3-level nesting
- Vectors: 128-dimensional, cosine similarity, 10K collection size
- State: 64-byte values
- All benchmarks run with 10K warmup operations, then 100K measured operations

### Performance target

**InMemory mode should achieve Redis-class throughput**: 100K+ ops/sec for KV get/put on commodity hardware. Redis single-threaded GET/SET benchmarks at ~100-150K ops/sec — StrataDB's InMemory mode should be in the same ballpark since both are in-process data structure operations with no I/O.

### Comparison baselines

The `comparison-benchmarks` feature flag already supports redb, LMDB, and SQLite. Benchmarks should include these for KV workloads to contextualize StrataDB's numbers:

- **redb**: Rust-native embedded DB (closest competitor)
- **LMDB**: Memory-mapped B-tree (gold standard for read-heavy embedded)
- **SQLite**: Ubiquitous baseline

### Implementation

- Extend existing `criterion` benchmarks in `crates/engine/benches/`
- One benchmark file per primitive: `benches/kv_bench.rs`, `benches/event_bench.rs`, etc.
- Shared harness that parameterizes durability mode
- Output as JSON for downstream analysis and plotting
- CI integration: run on every merge to main, alert on >10% regression

## 2. Hardware Scaling Study

Characterize how StrataDB performance scales across hardware tiers — from resource-constrained edge devices to high-core-count servers.

### Goal

Plot performance curves (ops/sec) as a function of:
- CPU cores (1 to 64+)
- Available memory (512MB to 256GB+)
- Storage speed (SD card to NVMe)

This is observational. The goal is to understand the scaling curve, not to change the architecture.

### Hardware tiers

| Tier | Representative hardware | Cores | RAM | Storage |
|------|------------------------|-------|-----|---------|
| **Edge** | Raspberry Pi 4 | 4 (ARM Cortex-A72) | 4 GB | microSD |
| **Laptop** | M-series MacBook / Ryzen laptop | 8-12 | 16-32 GB | NVMe SSD |
| **Workstation** | Desktop Ryzen 9 / i9 | 16-24 | 64 GB | NVMe SSD |
| **Server** | Xeon / EPYC rack server | 48-128 | 256+ GB | NVMe array |

### What to measure at each tier

1. **Single-threaded throughput**: Same benchmark suite from Section 1, run on each hardware tier. This reveals how raw single-core performance scales (clock speed, cache hierarchy, memory bandwidth).

2. **Concurrent throughput**: Run N threads issuing operations against the same database, sweep N from 1 to (2 * core count). Measure aggregate ops/sec. This reveals:
   - Whether the OCC transaction model scales with cores
   - Where lock contention appears (DashMap shards, WAL writer, etc.)
   - The concurrency sweet spot per hardware tier

3. **Working set vs. memory**: Load increasing amounts of data (1K, 10K, 100K, 1M, 10M keys) and measure read throughput. This reveals when the working set exceeds cache/memory and performance degrades.

4. **Storage-bound workloads**: For Buffered and Strict modes, measure how storage speed affects write throughput. Compare microSD vs. SATA SSD vs. NVMe.

### Expected outcome

A set of charts:
- **ops/sec vs. core count** (one line per primitive, one chart per durability mode)
- **ops/sec vs. concurrent clients** (saturation curves)
- **ops/sec vs. dataset size** (memory pressure curves)
- **write latency vs. storage tier** (for Buffered and Strict modes)

These charts inform future optimization priorities: if the bottleneck at scale is OCC contention, that's different work than if it's WAL write throughput or memory allocation pressure.

### Execution approach

- Use the same benchmark binary from Section 1, parameterized by thread count and dataset size
- Run on actual hardware (not VMs) for tiers where possible
- For server-tier, cloud instances (e.g., `c7i.16xlarge` or `c7g.16xlarge`) are acceptable proxies
- Automate with a runner script that sweeps parameters and collects results
- Results stored as CSV/JSON, visualized with a simple plotting script (Python matplotlib or similar)

## Ordering

1. **Benchmark suite first** — this can run on any single machine and establishes the baseline
2. **Hardware scaling study second** — requires access to multiple hardware tiers and more setup

## Dependencies

- Black-box test suite should be complete first (validates correctness before measuring performance)
- No code changes required — this is measurement of the existing v0.1 codebase
