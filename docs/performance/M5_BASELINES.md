# M5 Performance Baselines

## Test Environment

| Attribute | Value |
|-----------|-------|
| Hardware | [To be filled at measurement time] |
| CPU | |
| Memory | |
| Storage | |
| OS | |
| Rust Version | |
| Commit | |
| Date | |

## JSON Operation Baselines

### Create Performance

| Document Size | Target | Measured | Status |
|---------------|--------|----------|--------|
| 100 bytes | < 500µs | | |
| 1KB | < 1ms | | |
| 10KB | < 5ms | | |

### Get at Path Performance

| Path Depth | Target | Measured | Status |
|------------|--------|----------|--------|
| Depth 1 | < 50µs | | |
| Depth 5 | < 75µs | | |
| Depth 10 | < 100µs | | |

### Set at Path Performance

| Path Depth | Target | Measured | Status |
|------------|--------|----------|--------|
| Depth 1 | < 500µs | | |
| Depth 5 | < 750µs | | |
| Depth 10 | < 1ms | | |

### Delete at Path Performance

| Operation | Target | Measured | Status |
|-----------|--------|----------|--------|
| Object key | < 500µs | | |
| Array element | < 750µs | | |

## Non-Regression Verification

### M4 Baseline Comparison

| Operation | M4 Target | M4 Actual | M5 Measured | Delta | Status |
|-----------|-----------|-----------|-------------|-------|--------|
| KV put (InMemory) | < 3µs | | | | |
| KV put (Buffered) | < 30µs | | | | |
| KV get (fast path) | < 5µs | | | | |
| Event append | < 10µs | | | | |
| State read | < 5µs | | | | |
| Trace record | < 15µs | | | | |

### Regression Threshold

- **Acceptable**: < 5% regression
- **Warning**: 5-10% regression
- **Failure**: > 10% regression

## Mixed Workload Performance

| Workload | Target | Measured | Status |
|----------|--------|----------|--------|
| JSON + KV mixed | < 2ms per pair | | |
| Cross-primitive transaction | < 3ms | | |

## Methodology

### Benchmark Configuration

- **Warmup**: 100 iterations discarded
- **Measurement**: 1000 iterations minimum
- **Statistics**: p50, p95, p99 reported

### Running Benchmarks

```bash
# Run all M5 benchmarks
cargo bench --bench m5_performance

# Run specific benchmark group
cargo bench --bench m5_performance -- json_benches
cargo bench --bench m5_performance -- regression_benches
cargo bench --bench m5_performance -- mixed_benches

# Compare with baseline
cargo bench --bench m5_performance -- --save-baseline m5
cargo bench --bench m5_performance -- --baseline m4
```

### Memory Profiling

```bash
# Run with memory profiling
RUSTFLAGS="-C target-cpu=native" cargo bench --bench m5_performance -- --profile-time 30
```

## Known Limitations

1. **Path depth impact**: Deep paths (>10 levels) may have higher latency
2. **Large documents**: Documents >1MB may exceed targets
3. **Concurrent transactions**: High contention may increase conflict rate

## Recommendations

1. Keep documents under 1MB for best performance
2. Limit path depth to <10 for critical paths
3. Use batch operations for multiple updates to the same document
4. Consider document sharding for high-contention scenarios

## Conflict Detection Overhead

The M5 JSON primitive includes region-based conflict detection. Overhead depends on:

| Factor | Impact |
|--------|--------|
| Number of paths read | O(n) validation per write |
| Number of writes | O(n²) write-write conflict check |
| Path depth | Constant (path comparison is O(min(d1, d2))) |
| Document version check | O(1) per document |

### Conflict Detection Performance

| Check Type | Typical Overhead | Worst Case |
|------------|------------------|------------|
| Version mismatch | < 1µs | < 10µs |
| Write-write overlap | < 5µs | O(n²) for n writes |
| Path comparison | < 100ns | < 1µs |
