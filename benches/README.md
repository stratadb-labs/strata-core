# Strata Benchmarks

This directory contains Criterion benchmark suites for measuring Strata performance.

## Benchmark Files

| File | Milestone | Description |
|------|-----------|-------------|
| `m1_storage.rs` | M1 | Core storage layer benchmarks (ShardedStore, get/put/scan) |
| `m2_transactions.rs` | M2 | Transaction benchmarks (OCC, commit, rollback) |
| `m3_primitives.rs` | M3 | Primitive facade benchmarks (KV, EventLog, StateCell, etc.) |
| `m4_contention.rs` | M4 | Contention benchmarks (multi-threaded access patterns) |
| `m4_facade_tax.rs` | M4 | Facade tax benchmarks (overhead at each layer) |
| `m4_performance.rs` | M4 | General performance benchmarks |
| `m5_performance.rs` | M5 | JSON primitive benchmarks |
| `m6_search.rs` | M6 | Search benchmarks (BM25, hybrid search) |
| `m8_vector.rs` | M8 | Vector primitive benchmarks (similarity search) |
| `comprehensive_benchmarks.rs` | All | Comprehensive benchmarks across all primitives |
| `cross_primitive.rs` | All | Cross-primitive transaction benchmarks |
| `industry_comparison.rs` | All | Comparison with other databases (redb, LMDB, SQLite) |
| `bench_env.rs` | Utility | Environment capture and latency collection utilities |

## Running Benchmarks

### Using the Benchmark Runner (Recommended)

```bash
# Run ALL benchmarks with optimization tracking
./scripts/bench_runner.sh --full --tag=baseline --notes="M8 baseline"

# Run specific milestone
./scripts/bench_runner.sh --m3
./scripts/bench_runner.sh --m6

# Run with filters
./scripts/bench_runner.sh --m6 --filter="search_kv"
```

### Using Cargo Directly

```bash
# Run a specific benchmark
cargo bench --bench m3_primitives

# Run with filter
cargo bench --bench m3_primitives -- kvstore

# Run industry comparison (requires feature flag)
cargo bench --bench industry_comparison --features=comparison-benchmarks
```

## Results

Benchmark results are stored in `target/benchmark-results/` with the following structure:

```
target/benchmark-results/
├── INDEX.md                    # Global index of all runs
├── run_YYYY-MM-DD_HH-MM-SS_commit/
│   ├── FULL_SUMMARY.md         # Consolidated summary
│   ├── run_metadata.json       # Tag, notes, decision
│   ├── all_benchmarks.json     # All metrics in one file
│   ├── m1_storage.txt/json     # Per-benchmark results
│   ├── m2_transactions.txt/json
│   └── ...
```

## Documentation

- [BENCHMARKS.md](../docs/benchmarks/BENCHMARKS.md) - Benchmark design and targets
- [BENCHMARK_EXECUTION.md](../docs/benchmarks/BENCHMARK_EXECUTION.md) - Execution guide
- [PERFORMANCE_BASELINE.md](../docs/benchmarks/PERFORMANCE_BASELINE.md) - Performance targets

## M9 Optimization Workflow

1. **Establish baseline**: `./scripts/bench_runner.sh --full --tag=baseline`
2. **Make optimization**: Edit code
3. **Run benchmarks**: `./scripts/bench_runner.sh --full --tag=your-opt --notes="Description"`
4. **Compare results**: Check `target/benchmark-results/INDEX.md`
5. **Update decision**: Edit `run_metadata.json` to mark `keep` or `reject`
6. **Repeat**: Build on kept optimizations
