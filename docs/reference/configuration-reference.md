# Configuration Reference

## Config File: `strata.toml`

StrataDB uses a config file in the data directory. On first `Database::open()`, a default `strata.toml` is created automatically. To change settings, edit the file and restart.

```toml
# Strata database configuration
#
# Durability mode: "standard" (default) or "always"
#   "standard" = periodic fsync (~100ms), may lose last interval on crash
#   "always"   = fsync every commit, zero data loss
durability = "standard"
```

### Config Fields

| Field | Type | Default | Values | Description |
|-------|------|---------|--------|-------------|
| `durability` | string | `"standard"` | `"standard"`, `"always"` | WAL sync policy |

### Behavior

- Created automatically with defaults on first `Database::open()` if not present
- Parsed on every `open()` call
- Invalid config returns an error (database does not open)
- Cache mode (`Database::cache()`) has no config file (no data directory)

## Durability Modes

| Mode | Config Value | Description | Data Loss on Crash |
|------|-------------|-------------|-------------------|
| **Cache** | *(in-memory only)* | No persistence | All data |
| **Standard** | `"standard"` | Periodic fsync (~100ms / ~1000 writes) | Last ~100ms |
| **Always** | `"always"` | Immediate fsync per commit | None |

Default: `"standard"`

## Opening Methods

| Method | Durability | Disk Files | Use Case |
|--------|-----------|------------|----------|
| `Strata::open(path)` | Per `strata.toml` | Yes | Production |
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
