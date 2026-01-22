# Performance Baseline Documentation

## Overview

This document establishes the M8 performance baseline for the in-mem database. It serves as a reference for M9 optimizations and ensures no regressions are introduced.

**Status**: Established after M8 completion (includes Vector primitive)

## Performance Targets

### Storage Layer (M1-M4)

| Operation | InMemory Mode | Buffered Mode | Strict Mode |
|-----------|---------------|---------------|-------------|
| KV Get | <1µs | <2µs | <2µs |
| KV Put | <3µs | <30µs | ~2ms (fsync) |
| CAS | <5µs | <50µs | ~2ms (fsync) |
| Delete | <3µs | <30µs | ~2ms (fsync) |

### Throughput Targets

| Workload | InMemory | Buffered | Strict |
|----------|----------|----------|--------|
| KV Get (single thread) | 250K+ ops/s | 200K+ ops/s | 200K+ ops/s |
| KV Put (single thread) | 250K+ ops/s | 50K+ ops/s | ~500 ops/s |
| Mixed read-write (10:1) | 200K+ ops/s | 100K+ ops/s | ~5K ops/s |

### Snapshot System (M7)

| Operation | Target | Acceptable |
|-----------|--------|------------|
| Snapshot write (1M keys) | <100ms | <500ms |
| Snapshot load (1M keys) | <200ms | <1s |
| WAL truncation | <10ms | <100ms |

### Recovery System (M7)

| Operation | Target | Acceptable |
|-----------|--------|------------|
| WAL replay (1M entries) | <300ms | <1s |
| Snapshot + WAL recovery | <500ms | <2s |
| Index rebuild (1M keys) | <100ms | <500ms |

### Replay System (M7)

| Operation | Target | Acceptable |
|-----------|--------|------------|
| replay_run() (1K events) | <10ms | <50ms |
| replay_run() (10K events) | <100ms | <500ms |
| diff_runs() (1K keys each) | <5ms | <50ms |

### Vector Primitive (M8)

| Operation | Target | Acceptable |
|-----------|--------|------------|
| Insert single (384d) | <50µs | <100µs |
| Insert batch 100 (384d) | <5ms | <10ms |
| Insert batch 1000 (384d) | <50ms | <100ms |
| Search cosine 10K (384d) | <10ms | <50ms |
| Search euclidean 10K (384d) | <10ms | <50ms |
| Search dot product 10K (384d) | <10ms | <50ms |
| Search with filter 10K | <15ms | <60ms |
| Collection create | <100µs | <500µs |
| Collection delete | <100µs | <500µs |
| Collection list | <50µs | <200µs |

### Vector Dimension Scaling (M8)

| Dimension | Search 10K Target | Search 10K Acceptable |
|-----------|-------------------|----------------------|
| 128d | <5ms | <20ms |
| 384d | <10ms | <50ms |
| 768d | <20ms | <80ms |
| 1536d | <40ms | <150ms |

### Vector Collection Scaling (M8)

| Collection Size | Search Target | Search Acceptable |
|-----------------|---------------|-------------------|
| 10K vectors | <10ms | <50ms |
| 100K vectors | <100ms | <500ms |

## Durability Mode Characteristics

### InMemory Mode
- No WAL writes
- No fsync overhead
- Maximum throughput
- Data loss on crash
- Best for: Testing, ephemeral workloads

### Buffered Mode
- Background WAL writer
- Batched fsync (configurable interval)
- Good throughput with durability
- May lose recent data on crash
- Best for: Development, non-critical workloads

### Strict Mode
- Synchronous fsync on every commit
- Lowest throughput
- No data loss (if fsync completes)
- Best for: Production, critical data

## Benchmark Suite

### Running Benchmarks

```bash
# Run ALL benchmarks with M9 tracking (RECOMMENDED)
./scripts/bench_runner.sh --full --tag=baseline --notes="M9 baseline"

# Run specific milestone
./scripts/bench_runner.sh --m1
./scripts/bench_runner.sh --m6
./scripts/bench_runner.sh --m8

# Run with filters
./scripts/bench_runner.sh --m8 --filter="vector_search"

# Using cargo directly
cargo bench --bench comprehensive_benchmarks
cargo bench --bench m4_performance
cargo bench --bench m6_search
cargo bench --bench m8_vector

# Run specific categories
cargo bench --bench comprehensive_benchmarks -- kv_microbenchmarks
cargo bench --bench comprehensive_benchmarks -- concurrency
cargo bench --bench comprehensive_benchmarks -- recovery
cargo bench --bench comprehensive_benchmarks -- durability
cargo bench --bench m8_vector -- vector_insert
cargo bench --bench m8_vector -- vector_search
```

### Benchmark Categories

#### Tier 1: Microbenchmarks
Per-primitive, per-operation, pure compute+memory:
- KV get/put/delete latency
- Event append/scan throughput
- State cell transitions
- Trace recording

#### Tier 2: Concurrency
Transaction throughput under contention:
- Concurrent reads (no contention)
- Concurrent writes (same key)
- Mixed read-write workloads
- Transaction conflict rates

#### Tier 3: Recovery
WAL replay and snapshot performance:
- WAL entry write throughput
- WAL replay throughput
- Snapshot write time
- Snapshot load time
- Recovery time (snapshot + WAL)

#### Tier 4: Durability
fsync overhead and batching tradeoffs:
- Per-operation fsync cost
- Batched fsync throughput
- WAL truncation overhead

#### Tier 5: Memory
Heap usage and overhead:
- Per-key memory overhead
- Index memory usage
- Snapshot memory footprint

#### Tier 6: Scenarios
Agent-like workloads:
- Typical agent run simulation
- Long-running agent behavior
- Burst write patterns

#### Tier 7: Vector Operations (M8)
Vector primitive performance:
- Insert latency (single and batch)
- Similarity search (cosine, euclidean, dot product)
- Dimension scaling (128d to 1536d)
- Collection management (create, delete, list)
- Metadata filtering overhead
- Concurrent access patterns

## Key Metrics to Monitor

### Latency Percentiles

For production workloads, monitor:
- p50 (median)
- p95
- p99
- p99.9

### Recovery Time Metrics

For durability compliance:
- Time from crash to ready
- WAL entries processed per second
- Snapshot load time

### Memory Metrics

For resource planning:
- Heap usage per 1M keys
- WAL file size growth rate
- Snapshot file size

## Known Performance Characteristics

### WAL Entry Format

```
| Length (4) | Type (1) | Version (1) | Payload | CRC32 (4) |
```

Per-entry overhead: 10 bytes + serialized payload

### Snapshot Format

```
| Header (39 bytes) | Section 1 | Section 2 | ... | CRC32 (4) |
```

Section overhead: 9 bytes per primitive (type ID + length)

### Recovery Process

1. Find latest valid snapshot
2. Load snapshot into memory (O(snapshot size))
3. Replay WAL from snapshot offset (O(WAL entries))
4. Rebuild indexes (O(data size))
5. Detect orphaned runs

Total: O(snapshot) + O(WAL since snapshot) + O(index rebuild)

## Future Optimization Opportunities

### M9 Optimization Targets

The following are candidates for M9 performance tuning:

1. **SIMD Vector Operations**: Use SIMD for distance calculations (significant speedup for cosine/euclidean)
2. **Compression**: Snapshot and WAL compression (reserved in format)
3. **Incremental snapshots**: Only changed data since last snapshot
4. **Parallel recovery**: Multi-threaded WAL replay
5. **Index persistence**: Save indexes in snapshot (trade-off: larger snapshots)
6. **Memory-mapped IO**: For large datasets
7. **Async WAL writer**: Non-blocking WAL append
8. **Vector indexing**: HNSW or similar approximate nearest neighbor index
9. **Batch optimization**: Amortize transaction overhead across batch operations
10. **Lock-free reads**: Remove read locks for fast-path operations

### Performance Tuning Guidelines

1. **For latency**: Use InMemory mode for hot paths, Buffered for persistence
2. **For throughput**: Batch operations when possible
3. **For durability**: Use Strict mode only when necessary
4. **For recovery time**: Increase snapshot frequency (reduces WAL size)

## Regression Testing

### CI Performance Gates

Each CI run should verify:
1. No 2x regression in microbenchmark latency
2. No 50% regression in throughput
3. Recovery time within acceptable bounds

### Baseline Comparison

Compare against tagged baseline:

```bash
# Using bench_runner.sh (recommended for M9)
./scripts/bench_runner.sh --full --tag=baseline --notes="M8 baseline before optimization"
# Make changes...
./scripts/bench_runner.sh --full --tag=opt-name --notes="Description of optimization"
# Check target/benchmark-results/INDEX.md for comparison

# Using Criterion directly
cargo bench --bench comprehensive_benchmarks -- --save-baseline m8
cargo bench --bench m8_vector -- --save-baseline m8

# Compare against current
cargo bench --bench comprehensive_benchmarks -- --baseline m8
cargo bench --bench m8_vector -- --baseline m8
```

## Appendix: Raw Benchmark Data

### Hardware Specification

Record benchmarks on standardized hardware:
- CPU: Model, cores, frequency
- RAM: Size, speed
- Storage: SSD/NVMe type, specifications
- OS: Version, kernel

### Sample Results

*To be populated after benchmark runs on reference hardware*

Example format:

```
Benchmark: kv_get (InMemory)
  Mean:    0.8µs
  StdDev:  0.1µs
  Min:     0.5µs
  Max:     2.1µs
  p50:     0.7µs
  p95:     1.2µs
  p99:     1.8µs
```

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-17 | Initial M7 baseline established |
| 2.0 | 2026-01-18 | Updated for M8 Vector primitive, added M9 optimization targets |
