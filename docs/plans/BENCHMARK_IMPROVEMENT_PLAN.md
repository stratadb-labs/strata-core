# Benchmark Improvement Plan

## Executive Summary

This plan addresses the need to establish a comprehensive performance baseline for the M9 performance tuning milestone. The current benchmark suite has excellent foundations but needs improvements in coverage (VectorStore missing), baseline tracking, and regression detection.

---

## Current State Analysis

### Strengths ✓

1. **Solid Framework**: Criterion.rs with proper statistical rigor
2. **Good Taxonomy**: Layer prefixes (engine_, txn_, snapshot_, etc.) prevent false regressions
3. **Deterministic Randomness**: Fixed seed (`BENCH_SEED = 0xDEADBEEF_CAFEBABE`) ensures reproducibility
4. **Environment Capture**: Comprehensive system profiling in `bench_env.rs`
5. **Durability Mode Labels**: Explicit labeling prevents baseline confusion
6. **Multi-tier Architecture**: A0-D tiers enable abstraction cost analysis
7. **Excellent Runner Script**: `scripts/bench_runner.sh` provides:
   - Environment validation (governor, cores, turbo boost)
   - Multi-mode support (`--all-modes` for inmemory/batched/strict)
   - Perf integration (`--perf`, `--perf-record`)
   - Core pinning (`--cores=0-7`)
   - Run indexing with `INDEX.md`

### Existing Output Structure

Results are stored in `target/benchmark-results/`:

```
target/benchmark-results/
├── INDEX.md                           # Run index with links to all runs
├── run_YYYY-MM-DD_HH-MM-SS_commit/    # Per-run directories
│   ├── SUMMARY.md                     # Quick summary with key latencies
│   ├── bench_output.txt               # Raw criterion output
│   └── redis_comparison.txt           # Gate analysis
├── environment_*.md                   # Detailed environment reports
├── environment_*.json                 # Environment JSON for CI
├── benchmark_report_*.md              # Full results by tier
└── facade_tax_*.md                    # Abstraction overhead analysis
```

### Current Coverage

| File | Primitives | Operations | Lines |
|------|------------|------------|-------|
| m1_storage.rs | KVStore | get, put, delete, WAL replay | 536 |
| m2_transactions.rs | Transactions | commit, CAS, snapshots, conflicts | 807 |
| m3_primitives.rs | KV, Event, State, Trace, Run | All tier A0-D operations | 1,755 |
| m4_*.rs | Performance | Facade tax, contention | 843 |
| m5_performance.rs | JsonStore | create, get, set, delete, merge | 1,046 |
| m6_search.rs | Search | keyword, hybrid, indexing | 521 |
| comprehensive_benchmarks.rs | Multi-tier | 6-tier scenario suite | 1,173 |
| bench_env.rs | Environment | Capture, reporting | 1,623 |

**Total**: ~8,304 lines of benchmark code across 10 files

### Critical Gaps

| Gap | Severity | Impact |
|-----|----------|--------|
| **VectorStore benchmarks missing** | CRITICAL | Cannot measure vector primitive performance |
| **No --m8 flag in bench_runner.sh** | HIGH | Runner script doesn't support VectorStore |
| **No automated baseline comparison** | MEDIUM | Manual diff required between runs |
| **No p99 latency tracking** | MEDIUM | Tail latency invisible |
| **Cross-primitive benchmarks missing** | LOW | Agent patterns not measured |

---

## Improvement Plan

### Phase 1: Add VectorStore Benchmarks (CRITICAL)

#### 1.1 New File: `benches/m8_vector.rs`

```rust
// VectorStore Operations to Benchmark
//
// Tier B (Primitive Facade):
// - vector_create_collection
// - vector_insert (single)
// - vector_insert_batch (10, 100, 1000)
// - vector_get
// - vector_delete
// - vector_update_metadata
// - vector_count
// - vector_list_keys
//
// Tier C (Indexed/Search):
// - vector_search_k1
// - vector_search_k10
// - vector_search_k100
// - vector_search_with_filter
//
// Scaling Tests:
// - dimension_3d, dimension_128d, dimension_768d, dimension_1536d
// - collection_size_1k, 10k, 100k vectors
// - distance_cosine, distance_euclidean, distance_dotproduct
```

#### 1.2 Benchmark Taxonomy

Following existing conventions:

| Prefix | Semantic | Example |
|--------|----------|---------|
| `vector_create/` | Collection creation | `vector_create/dim_128` |
| `vector_insert/` | Vector insertion | `vector_insert/batch_100/dim_768` |
| `vector_search/` | Similarity search | `vector_search/k_10/size_10k` |
| `vector_scale/` | Scaling behavior | `vector_scale/collection_100k` |

#### 1.3 Performance Gates

| Operation | Target | Acceptable | Stretch |
|-----------|--------|------------|---------|
| vector_create | <1ms | <5ms | <500µs |
| vector_insert (single, 128d) | <50µs | <200µs | <20µs |
| vector_insert_batch (100, 128d) | <2ms | <10ms | <1ms |
| vector_search (k=10, 10k vectors, 128d) | <1ms | <5ms | <500µs |
| vector_search (k=10, 100k vectors, 128d) | <10ms | <50ms | <5ms |
| vector_get | <10µs | <50µs | <5µs |

### Phase 2: Update Runner Script

#### 2.1 Add `--m8` Flag to `scripts/bench_runner.sh`

```bash
# Add to option parsing:
--m8)
    RUN_M8=true
    shift
    ;;

# Add to milestone mapping:
elif [[ "$RUN_M8" == "true" ]]; then
    BENCH_TARGET="m8_vector"

# Add to build_release:
cargo build --release --bench m8_vector
```

#### 2.2 Add to Cargo.toml

```toml
[[bench]]
name = "m8_vector"
harness = false
```

### Phase 3: Baseline Comparison Tool

#### 3.1 JSON Baseline Format

Extend existing `environment_*.json` to include results:

```json
{
  "version": "2.0",
  "timestamp": "2026-01-17T12:00:00Z",
  "git_commit": "abc123",
  "git_branch": "develop",
  "environment": { /* existing env capture */ },
  "results": {
    "vector_insert/single/dim_128": {
      "mean_ns": 45000,
      "median_ns": 43000,
      "stddev_ns": 5000,
      "min_ns": 38000,
      "max_ns": 120000,
      "samples": 1000
    },
    "vector_search/k_10/size_10k": {
      "mean_ns": 850000,
      "median_ns": 820000,
      "stddev_ns": 50000,
      "samples": 500
    }
  },
  "gates": {
    "vector_search/k_10/size_10k": {
      "target_ns": 1000000,
      "actual_ns": 850000,
      "passed": true,
      "margin_pct": 15.0
    }
  }
}
```

#### 3.2 Comparison Script: `scripts/compare_baseline.sh`

```bash
#!/bin/bash
# Compare two benchmark runs and highlight regressions

BASELINE=$1
CURRENT=$2
THRESHOLD=${3:-5}  # Default 5% regression threshold

# Output:
# - Regressions (>threshold% slower)
# - Improvements (>10% faster)
# - Gate failures
# - Summary statistics
```

### Phase 4: Cross-Primitive Benchmarks

#### 4.1 New File: `benches/cross_primitive.rs`

Agent-realistic workload patterns:

```rust
// Scenarios:
//
// 1. agent_think_cycle
//    - Read state (StateCell)
//    - Query traces (TraceStore)
//    - Record thought (TraceStore)
//    - Update state (StateCell)
//
// 2. agent_tool_use
//    - Read config (KVStore)
//    - Log event (EventLog)
//    - Store result (JsonStore)
//    - Update run status (RunIndex)
//
// 3. embedding_workflow
//    - Store document (JsonStore)
//    - Store embedding (VectorStore)
//    - Semantic search (VectorStore)
//    - Retrieve document (JsonStore)
//
// 4. atomic_all_seven
//    - Single transaction touching all 7 primitives
```

### Phase 5: Latency Percentiles

#### 5.1 Custom Measurement with Histograms

Add to `bench_env.rs`:

```rust
pub struct LatencyHistogram {
    samples: Vec<u64>,  // nanoseconds
}

impl LatencyHistogram {
    pub fn percentile(&self, p: f64) -> u64 {
        let idx = ((self.samples.len() as f64) * p / 100.0) as usize;
        self.samples[idx.min(self.samples.len() - 1)]
    }

    pub fn report(&self) -> LatencyReport {
        LatencyReport {
            p50: self.percentile(50.0),
            p90: self.percentile(90.0),
            p95: self.percentile(95.0),
            p99: self.percentile(99.0),
            p999: self.percentile(99.9),
        }
    }
}
```

---

## Implementation Priority

### Immediate (Before M9 starts)

| Priority | Task | Effort | Deliverable |
|----------|------|--------|-------------|
| P0 | Add `m8_vector.rs` benchmarks | 4-6h | VectorStore coverage |
| P0 | Update `bench_runner.sh` for `--m8` | 1h | Runner integration |
| P0 | Add `[[bench]]` to Cargo.toml | 5min | Build integration |
| P1 | Run baseline with all 7 primitives | 1h | Baseline JSON |

### Short-term (During M9)

| Priority | Task | Effort | Deliverable |
|----------|------|--------|-------------|
| P1 | Create `compare_baseline.sh` | 2-3h | Regression detection |
| P2 | Add cross-primitive benchmarks | 3-4h | Agent patterns |
| P2 | Add p99 tracking | 2-3h | Tail latency visibility |

### Medium-term (Post M9)

| Priority | Task | Effort | Deliverable |
|----------|------|--------|-------------|
| P3 | CI integration | 2-3h | Automated regression gate |
| P3 | Flamegraph integration | 2h | Bottleneck visualization |

---

## Success Criteria

After Phase 1-2:

- [ ] `cargo bench --bench m8_vector` runs VectorStore benchmarks
- [ ] `./scripts/bench_runner.sh --m8` works correctly
- [ ] Results appear in `target/benchmark-results/`
- [ ] INDEX.md updated with M8 runs

After Phase 3-5:

- [ ] `./scripts/compare_baseline.sh old.json new.json` shows regressions
- [ ] P99 latencies visible in reports
- [ ] Cross-primitive benchmarks cover agent patterns

---

## Appendix A: Existing Benchmark Tiers

| Tier | Purpose | Redis Comparable? | Examples |
|------|---------|-------------------|----------|
| A0 | Raw data structure | ✓ Yes | `core/get_hot`, `core/put_hot` |
| A1 | Engine (snapshot+commit) | ✗ No (we have txns) | `engine/get_direct`, `engine/put_direct` |
| B | Primitive facades | ✗ No | `kvstore/get`, `eventlog/append` |
| C | Indexed operations | ✗ No | `tracestore/query_by_type` |
| D | Contention | ✗ No (Redis single-threaded) | `contention/same_key/4t` |

## Appendix B: VectorStore API Reference

```rust
// From crates/primitives/src/vector.rs
impl VectorStore {
    pub fn create_collection(run_id, name, config) -> Result<()>
    pub fn delete_collection(run_id, name) -> Result<()>
    pub fn insert(run_id, collection, key, vector, metadata) -> Result<()>
    pub fn get(run_id, collection, key) -> Result<Option<VectorEntry>>
    pub fn delete(run_id, collection, key) -> Result<bool>
    pub fn search(run_id, collection, query, k, filter) -> Result<Vec<SearchResult>>
    pub fn count(run_id, collection) -> Result<usize>
    pub fn list_keys(run_id, collection) -> Result<Vec<String>>
    pub fn get_collection(run_id, name) -> Result<Option<CollectionInfo>>
}
```

## Appendix C: Reference Platform

From `scripts/bench_runner.sh`:

```
Reference Platform:
  - Linux (Ubuntu 24.04.2 LTS)
  - AMD Ryzen 7 7800X3D 8-Core Processor (16 logical cores)
  - 64GB DDR5 RAM
  - Performance governor
  - Pinned cores for contention tests
```

Performance gates are only valid on reference platform. Non-reference runs (macOS, powersave governor) are for development only.
