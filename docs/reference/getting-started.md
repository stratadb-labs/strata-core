# Getting Started with Strata

**Strata** is a fast, durable, embedded database designed for AI agent workloads.

**Current Version**: 0.10.0 (M10 Storage Backend, Retention & Compaction)

## Installation

```toml
[dependencies]
strata = "0.10"
```

## Quick Start

```rust
use strata::{Database, DatabaseConfig, DurabilityMode};
use strata::primitives::KVStore;
use strata::Value;
use std::sync::Arc;

// Open database with configuration
let config = DatabaseConfig {
    durability_mode: DurabilityMode::Buffered {
        flush_interval_ms: 100,
        max_pending_writes: 1000,
    },
    ..Default::default()
};

let db = Arc::new(Database::open("./my-agent-db", config)?);

// Create KVStore
let kv = KVStore::new(db.clone());

// Begin a run
let run_id = db.begin_run();

// Store data
kv.put(&run_id, "key", Value::String("value".into()))?;

// Reads return Versioned<T> (M9)
let versioned = kv.get(&run_id, "key")?;
if let Some(v) = versioned {
    println!("Value: {:?}, Version: {:?}", v.value, v.version);
}

// Checkpoint for crash recovery (M10)
db.checkpoint()?;

// End run
db.end_run(run_id)?;

// Clean shutdown
db.close()?;
```

## Durability Modes

| Mode | Latency | Data Loss | Use Case |
|------|---------|-----------|----------|
| InMemory | <3µs | All | Tests, caches |
| Buffered | <30µs | ~100ms | Production |
| Strict | ~2ms | None | Audit logs |

## Using the Primitives

Strata has **seven primitives** as of M10:

### KVStore

```rust
let kv = KVStore::new(db.clone());
kv.put(&run_id, "key", Value::I64(42))?;

// Reads return Versioned<Value>
let versioned = kv.get(&run_id, "key")?;

// Batch reads
let values = kv.get_many(&run_id, &["key1", "key2"])?;
```

### EventLog

```rust
let events = EventLog::new(db.clone());
let versioned = events.append(&run_id, "user_action", Value::String("login".into()))?;
let event = events.read(&run_id, versioned.value.sequence)?;
```

### StateCell

```rust
let state = StateCell::new(db.clone());
state.init(&run_id, "counter", Value::I64(0))?;

// Atomic increment with retry
let (old, version) = state.transition(&run_id, "counter", |s| {
    let n = match &s.value { Value::I64(n) => *n, _ => 0 };
    Ok((Value::I64(n + 1), n))
})?;
```

### TraceStore

```rust
let traces = TraceStore::new(db.clone());
let trace_id = traces.record(
    &run_id,
    TraceType::ToolCall {
        tool_name: "search".into(),
        arguments: Value::String("query".into()),
        result: None,
        duration_ms: Some(150),
    },
    vec!["search".into()],
    Value::Null
)?;
```

### RunIndex

```rust
let runs = RunIndex::new(db.clone());
runs.create_run(&run_id.to_string())?;
runs.complete_run(&run_id.to_string())?;
let active = runs.query_by_status(RunStatus::Active)?;
```

### JsonStore (M5)

```rust
use strata::primitives::JsonStore;
use serde_json::json;

let json = JsonStore::new(db.clone());

// Store JSON document
json.put(&run_id, "config", json!({
    "model": "gpt-4",
    "temperature": 0.7,
    "max_tokens": 1000
}))?;

// Path-level mutations
json.set_path(&run_id, "config", "$.temperature", json!(0.9))?;
let temp = json.get_path(&run_id, "config", "$.temperature")?;

// Array operations
json.array_push(&run_id, "config", "$.history", json!({"role": "user"}))?;
```

### VectorStore (M8)

```rust
use strata::primitives::VectorStore;
use strata::DistanceMetric;

let vectors = VectorStore::new(db.clone());

// Insert embeddings with metadata
vectors.insert(
    &run_id,
    "doc1",
    vec![0.1, 0.2, 0.3, 0.4],  // embedding
    json!({"title": "Hello World", "category": "greeting"})
)?;

vectors.insert(
    &run_id,
    "doc2",
    vec![0.2, 0.3, 0.4, 0.5],
    json!({"title": "Goodbye World", "category": "farewell"})
)?;

// Semantic search
let query = vec![0.15, 0.25, 0.35, 0.45];
let results = vectors.search(
    &run_id,
    &query,
    10,  // top-k
    DistanceMetric::Cosine,
    None  // no filter
)?;

for result in results {
    println!("{}: score={:.4}", result.key, result.score);
}

// Search with metadata filter
use strata::MetadataFilter;

let filtered = vectors.search(
    &run_id,
    &query,
    10,
    DistanceMetric::Cosine,
    Some(MetadataFilter::Eq("category".into(), json!("greeting")))
)?;
```

## Search

### Hybrid Search (M6 + M8)

```rust
use strata::search::{SearchRequest, HybridSearch};

let search = HybridSearch::new(db.clone());

// Keyword-only search
let request = SearchRequest::new("find error logs")
    .with_limit(10)
    .with_budget_ms(50);

let response = search.search(&run_id, request)?;

// Hybrid search (keyword + vector)
let query_embedding = vec![0.1, 0.2, 0.3, 0.4];
let hybrid_request = SearchRequest::new("error handling")
    .with_vector(query_embedding)
    .with_limit(10);

let hybrid_response = search.search(&run_id, hybrid_request)?;

for result in hybrid_response.results {
    println!("Found: {:?} (score: {})", result.doc_ref, result.score);
}
```

## Storage Backend (M10)

### Database Configuration

```rust
use strata::{Database, DatabaseConfig, DurabilityMode, RetentionPolicy};

let config = DatabaseConfig {
    durability_mode: DurabilityMode::Buffered {
        flush_interval_ms: 100,
        max_pending_writes: 1000,
    },
    wal_segment_size: 64 * 1024 * 1024,  // 64 MB
    default_retention: RetentionPolicy::KeepAll,
    ..Default::default()
};

let db = Database::open("./strata.db", config)?;
```

### Checkpoints

```rust
// Create checkpoint for crash recovery
let checkpoint = db.checkpoint()?;
println!("Checkpoint at txn {}", checkpoint.watermark_txn);

// Safe to copy database after checkpoint
// cp -r ./strata.db ./backup.db
```

### Retention Policies (M10)

```rust
use strata::RetentionPolicy;
use std::time::Duration;

// Keep all versions (default)
db.set_retention_policy(&run_id, RetentionPolicy::KeepAll)?;

// Keep only last 10 versions per key
db.set_retention_policy(&run_id, RetentionPolicy::KeepLast(10))?;

// Keep versions from last 7 days
db.set_retention_policy(&run_id, RetentionPolicy::KeepFor(Duration::from_secs(7 * 24 * 3600)))?;

// Handling trimmed history
match kv.get_at(&run_id, "key", old_version) {
    Ok(value) => println!("Found: {:?}", value),
    Err(Error::HistoryTrimmed { requested, earliest_retained }) => {
        println!("Version {} trimmed, earliest is {:?}", requested, earliest_retained);
    }
    Err(e) => return Err(e),
}
```

### Compaction (M10)

```rust
use strata::CompactMode;

// WAL-only compaction (remove WAL covered by snapshot)
let info = db.compact(CompactMode::WALOnly)?;
println!("Reclaimed {} bytes, removed {} WAL segments",
    info.reclaimed_bytes, info.wal_segments_removed);

// Full compaction (WAL + retention enforcement)
let info = db.compact(CompactMode::Full)?;
println!("Removed {} old versions", info.versions_removed);
```

### Database Lifecycle

```rust
// Open (creates if doesn't exist)
let db = Database::open("./strata.db", config)?;

// Do work...

// Clean shutdown
db.close()?;

// Export for backup
db.export("./backup.db")?;

// Import is just open
let restored = Database::open("./backup.db", config)?;
```

## Snapshots (M7)

```rust
use strata::SnapshotConfig;

// Configure automatic snapshots
db.configure_snapshots(SnapshotConfig {
    wal_size_threshold: 50 * 1024 * 1024,  // 50 MB
    time_interval_minutes: 15,
    retention_count: 3,
    snapshot_on_shutdown: true,
});

// Manual snapshot
let info = db.snapshot()?;
println!("Snapshot created at {:?}", info.path);
```

## Recovery (M7)

```rust
use strata::{Database, RecoveryOptions};

// Open with custom recovery options
let db = Database::open_with_options(
    "./data",
    RecoveryOptions {
        max_corrupt_entries: 5,
        verify_all_checksums: true,
        rebuild_indexes: true,
    }
)?;

// Check recovery result
if let Some(result) = db.last_recovery_result() {
    println!("Recovered {} transactions", result.transactions_recovered);
    if result.corrupt_entries_skipped > 0 {
        eprintln!("Warning: {} corrupt entries skipped", result.corrupt_entries_skipped);
    }
}
```

## Run Lifecycle (M7)

```rust
// Explicit run lifecycle
let run_id = RunId::new();
db.begin_run(run_id)?;

// Do work within the run
kv.put(&run_id, "step", Value::String("started".into()))?;
events.append(&run_id, "task_begun", Value::Null)?;

// End run normally
db.end_run(run_id)?;

// Check for orphaned runs after restart
for orphan in db.orphaned_runs()? {
    println!("Orphaned run: {:?}", orphan);
}
```

## Replay (M7)

```rust
// Replay a completed run (read-only, side-effect free)
let view = db.replay_run(run_id)?;
println!("Run had {} KV entries", view.keys().count());
println!("Run had {} events", view.events().len());

// Compare two runs
let diff = db.diff_runs(run_a, run_b)?;
for entry in &diff.added {
    println!("Added: {:?}", entry.key);
}
for entry in &diff.modified {
    println!("Modified: {:?} ({:?} -> {:?})", entry.key, entry.value_a, entry.value_b);
}
```

## Cross-Primitive Transactions

```rust
use strata::primitives::{KVStoreExt, EventLogExt, StateCellExt, VectorStoreExt};

db.transaction(&run_id, |txn| {
    txn.kv_put("key", Value::String("value".into()))?;
    txn.event_append("event", Value::Null)?;
    txn.state_set("counter", Value::I64(1))?;
    txn.vector_insert("doc", vec![0.1, 0.2], json!({"indexed": true}))?;
    Ok(())
})?;
```

## Performance Tips

1. **Use fast path operations** for reads: `kv.get()`, `kv.exists()`, `kv.get_many()`
2. **Choose appropriate durability**: InMemory for tests, Buffered for production
3. **Use `transition()` for counters**: Handles retries automatically
4. **Set search budgets**: Use `with_budget_ms()` to control search latency
5. **Enable indexing selectively**: Inverted index is opt-in for memory efficiency
6. **Configure snapshot triggers**: Tune `wal_size_threshold` based on recovery time requirements
7. **Use explicit run lifecycle**: `begin_run()`/`end_run()` enables orphan detection
8. **Replay is cheap**: O(run size), not O(database size)
9. **Checkpoint before copying**: Use `checkpoint()` before copying database directory
10. **Compact periodically**: Use `compact()` to reclaim disk space after retention

## Directory Structure

After opening a database, you'll see:

```
strata.db/
├── MANIFEST              # Database metadata
├── WAL/
│   ├── wal-000001.seg    # WAL segments
│   └── ...
├── SNAPSHOTS/
│   └── snap-000010.chk   # Checkpoints
└── DATA/                 # Data files
```

**Portability**: Copy a closed database directory to create a valid clone.

## See Also

- [API Reference](api-reference.md)
- [Architecture](architecture.md)
- [Milestones](../milestones/MILESTONES.md)
