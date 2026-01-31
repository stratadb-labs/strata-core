# Engine & Storage Optimization

**Theme**: Data-driven rewrites based on benchmark and scaling study results.

## Context

The [performance characterization](performance-characterization.md) work will produce throughput curves across hardware tiers (Raspberry Pi to Xeon), concurrency levels, and dataset sizes. Those curves will reveal where the current engine and storage layers hit their limits.

This milestone is intentionally left open until that data exists. The specific optimizations, their priority, and their scope will be determined by what the benchmarks show — not by speculation.

## Known areas to watch

- ShardedStore concurrency scaling
- WAL write serialization under high core counts
- Memory allocation pressure on constrained devices
- Vector search performance vs. collection size

## Scope

Partial rewrites of `strata-engine` and `strata-storage` internals. Public API (`strata-executor`) should remain unchanged.

## Dependencies

- Performance characterization (benchmark suite + hardware scaling study) must complete first
