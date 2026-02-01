# Configuration Reference

## Durability Modes

| Mode | Enum Value | Description | Data Loss on Crash |
|------|-----------|-------------|-------------------|
| **Cache** | `DurabilityMode::Cache` | No persistence | All data |
| **Standard** | `DurabilityMode::Standard` | Periodic fsync (~100ms / ~1000 writes) | Last ~100ms |
| **Always** | `DurabilityMode::Always` | Immediate fsync per commit | None |

Default: `Standard`

## Opening Methods

| Method | Durability | Disk Files | Use Case |
|--------|-----------|------------|----------|
| `Strata::open(path)` | Configurable | Yes | Production |
| `Strata::open_temp()` | Cache (in-memory) | No | Testing |
| `Strata::from_database(db)` | Inherited | Depends | Shared DB |

## Database Info

The `DatabaseInfo` struct returned by `db.info()`:

| Field | Type | Description |
|-------|------|-------------|
| `version` | `String` | StrataDB version |
| `uptime_secs` | `u64` | Seconds since database opened |
| `branch_count` | `u64` | Number of branches |
| `total_keys` | `u64` | Total key count across all primitives |

## Distance Metrics (Vector Store)

| Metric | Enum Value | Description |
|--------|-----------|-------------|
| Cosine | `DistanceMetric::Cosine` | Cosine similarity (default) |
| Euclidean | `DistanceMetric::Euclidean` | L2 distance |
| Dot Product | `DistanceMetric::DotProduct` | Inner product |

## Branch Status

| Status | Enum Value | Description |
|--------|-----------|-------------|
| Active | `BranchStatus::Active` | Currently in use |

## Transaction Options

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `read_only` | `bool` | `false` | If true, transaction only reads (no writes) |

## Metadata Filter Operations (Vector Search)

| Operation | Enum Value | Description |
|-----------|-----------|-------------|
| Equal | `FilterOp::Eq` | Field equals value |
| Not Equal | `FilterOp::Ne` | Field does not equal value |
| Greater Than | `FilterOp::Gt` | Field > value |
| Greater or Equal | `FilterOp::Gte` | Field >= value |
| Less Than | `FilterOp::Lt` | Field < value |
| Less or Equal | `FilterOp::Lte` | Field <= value |
| In | `FilterOp::In` | Field is in set |
| Contains | `FilterOp::Contains` | Field contains value |

## Retention Policies

| Policy | Enum Value | Description |
|--------|-----------|-------------|
| Keep All | `RetentionPolicyInfo::KeepAll` | No version pruning (default) |
| Keep Last N | `RetentionPolicyInfo::KeepLast { count }` | Keep only the last N versions |
| Keep For Duration | `RetentionPolicyInfo::KeepFor { duration_secs }` | Keep versions within time window |

## Performance Targets

| Metric | Target |
|--------|--------|
| InMemory put | <3 us |
| InMemory throughput (1 thread) | 250K ops/sec |
| InMemory throughput (4 threads) | 800K+ ops/sec |
| Buffered put | <30 us |
| Buffered throughput | 50K ops/sec |
| Fast path read | <10 us |
| Vector search (10K vectors) | <50 ms |
| Vector insert | <100 us |

## Workspace Feature Flags

| Feature | Description |
|---------|-------------|
| `default` | Core database functionality |
| `perf-trace` | Per-layer timing instrumentation |
| `comparison-benchmarks` | Enable SOTA comparison benchmarks (redb, LMDB, SQLite) |
| `usearch-enabled` | Enable USearch for vector comparisons (requires C++ tools) |
