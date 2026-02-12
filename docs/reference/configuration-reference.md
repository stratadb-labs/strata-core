# Configuration Reference

## Config File: `strata.toml`

StrataDB uses a config file in the data directory. On first `Strata::open()`, a default `strata.toml` is created automatically. To change settings, edit the file and restart.

```toml
# Strata database configuration
#
# Durability mode: "standard" (default) or "always"
#   "standard" = periodic fsync (~100ms), may lose last interval on crash
#   "always"   = fsync every commit, zero data loss
durability = "standard"

# Enable automatic text embedding for semantic search
# Requires the `embed` feature and model files (run `stratadb setup` first)
auto_embed = false

# [model]
# endpoint = "http://localhost:11434/v1"
# model = "qwen3:1.7b"
# api_key = "optional-token"
# timeout_ms = 5000
```

### Config Fields

| Field | Type | Default | Values | Description |
|-------|------|---------|--------|-------------|
| `durability` | string | `"standard"` | `"standard"`, `"always"` | WAL sync policy |
| `auto_embed` | bool | `false` | `true`, `false` | Automatic text embedding for semantic search |
| `[model].endpoint` | string | — | URL | OpenAI-compatible API endpoint |
| `[model].model` | string | — | model name | Model identifier (e.g. `"qwen3:1.7b"`) |
| `[model].api_key` | string? | — | token | Optional bearer token |
| `[model].timeout_ms` | integer | `5000` | milliseconds | Request timeout |

### Behavior

- Created automatically with defaults on first `Strata::open()` if not present
- Parsed on every `open()` call
- Invalid config returns an error (database does not open)
- Cache mode (`Strata::cache()`) has no config file (no data directory)
- `configure_model()` and `set_auto_embed()` persist changes to `strata.toml` automatically

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
| `Strata::open_with(path, opts)` | Per opts / `strata.toml` | Yes | Custom settings |
| `Strata::cache()` | Cache (in-memory) | No | Testing |

### OpenOptions

`OpenOptions` provides a builder for overriding config-file defaults at open time. Any field set to `Some` wins over the `strata.toml` value; `None` means "use the default".

| Field | Type | Description |
|-------|------|-------------|
| `access_mode` | `AccessMode` | `ReadWrite` (default) or `ReadOnly` |
| `auto_embed` | `Option<bool>` | Override auto-embed setting |
| `durability` | `Option<String>` | Override durability: `"standard"` or `"always"` |
| `model_endpoint` | `Option<String>` | Override model endpoint URL |
| `model_name` | `Option<String>` | Override model name |
| `model_api_key` | `Option<String>` | Override model API key |
| `model_timeout_ms` | `Option<u64>` | Override model request timeout |

## Programmatic Configuration

Configuration can be read and modified at runtime without editing `strata.toml` directly. Changes made via these methods are persisted to `strata.toml` for disk-backed databases.

```rust
// Read current config
let cfg = db.config();
println!("durability: {}", cfg.durability);
println!("auto_embed: {}", cfg.auto_embed);

// Configure an LLM model endpoint (persisted)
db.configure_model("http://localhost:11434/v1", "qwen3:1.7b", None, None)?;

// Toggle auto-embed (persisted)
db.set_auto_embed(true)?;

// Check auto-embed status
if db.auto_embed_enabled() {
    println!("auto-embed is on");
}
```

See the [API Quick Reference](api-quick-reference.md) for the full Configuration method table.

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
| `embed` | MiniLM-L6-v2 runtime for automatic text embedding |
| `expand` | LLM-powered query expansion for search |
| `rerank` | LLM-powered result reranking for search |
| `perf-trace` | Per-layer timing instrumentation |
| `comparison-benchmarks` | Enable SOTA comparison benchmarks (redb, LMDB, SQLite) |
| `usearch-enabled` | Enable USearch for vector comparisons (requires C++ tools) |
