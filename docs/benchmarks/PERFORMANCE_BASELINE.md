# Performance Baseline Documentation

## Overview

This document establishes the M7 performance baseline for the in-mem database. It serves as a reference for future optimizations and ensures no regressions are introduced.

**Status**: Established after M7 completion

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
# Run all benchmarks
cargo bench --bench comprehensive_benchmarks

# Run specific categories
cargo bench --bench comprehensive_benchmarks -- kv_microbenchmarks
cargo bench --bench comprehensive_benchmarks -- concurrency
cargo bench --bench comprehensive_benchmarks -- recovery
cargo bench --bench comprehensive_benchmarks -- durability

# M4 performance benchmarks
cargo bench --bench m4_performance

# M6 search benchmarks
cargo bench --bench m6_search
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

### M8+ Optimizations (Not in M7)

1. **Compression**: Snapshot and WAL compression (reserved in format)
2. **Incremental snapshots**: Only changed data since last snapshot
3. **Parallel recovery**: Multi-threaded WAL replay
4. **Index persistence**: Save indexes in snapshot (trade-off: larger snapshots)
5. **Memory-mapped IO**: For large datasets
6. **Async WAL writer**: Non-blocking WAL append

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
# Checkout baseline
git checkout m7_perf_baseline

# Run benchmarks
cargo bench --bench comprehensive_benchmarks -- --save-baseline m7

# Compare against current
git checkout main
cargo bench --bench comprehensive_benchmarks -- --baseline m7
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
