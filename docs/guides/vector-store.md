# Vector Store Guide

The Vector Store holds embedding vectors in named collections and supports similarity search. Use it for RAG context, agent memory, and any workflow that involves finding similar items.

## API Overview

| Method | Signature | Returns |
|--------|-----------|---------|
| `vector_create_collection` | `(collection: &str, dimension: u64, metric: DistanceMetric) -> Result<u64>` | Version |
| `vector_delete_collection` | `(collection: &str) -> Result<bool>` | Whether it existed |
| `vector_list_collections` | `() -> Result<Vec<CollectionInfo>>` | All collections |
| `vector_collection_stats` | `(collection: &str) -> Result<CollectionInfo>` | Collection details |
| `vector_upsert` | `(collection: &str, key: &str, vector: Vec<f32>, metadata: Option<Value>) -> Result<u64>` | Version |
| `vector_batch_upsert` | `(collection: &str, entries: Vec<BatchVectorEntry>) -> Result<Vec<u64>>` | Versions |
| `vector_get` | `(collection: &str, key: &str) -> Result<Option<VersionedVectorData>>` | Vector data |
| `vector_delete` | `(collection: &str, key: &str) -> Result<bool>` | Whether it existed |
| `vector_search` | `(collection: &str, query: Vec<f32>, k: u64) -> Result<Vec<VectorMatch>>` | Top-k matches |

## Collections

Before storing vectors, create a collection with a fixed dimension and distance metric:

```rust
use stratadb::DistanceMetric;

let db = Strata::cache()?;

// 384-dimensional vectors with cosine similarity
db.vector_create_collection("embeddings", 384, DistanceMetric::Cosine)?;

// Euclidean distance
db.vector_create_collection("positions", 3, DistanceMetric::Euclidean)?;

// Dot product
db.vector_create_collection("scores", 128, DistanceMetric::DotProduct)?;
```

### Distance Metrics

| Metric | Best For | Score Range |
|--------|----------|-------------|
| `Cosine` | Text embeddings, normalized vectors | [-1, 1] (higher = more similar) |
| `Euclidean` | Spatial data, positions | (0, 1] (higher = more similar) |
| `DotProduct` | Pre-normalized embeddings, scoring | Unbounded (higher = more similar) |

All metrics are normalized so that **higher scores = more similar**.

### List Collections

```rust
let collections = db.vector_list_collections()?;
for c in &collections {
    println!("{}: {} dimensions, {:?} metric, {} vectors",
        c.name, c.dimension, c.metric, c.count);
}
```

### Collection Statistics

Get detailed information about a specific collection:

```rust
let stats = db.vector_collection_stats("embeddings")?;
println!("Name: {}", stats.name);
println!("Vectors: {}", stats.count);
println!("Dimension: {}", stats.dimension);
println!("Metric: {:?}", stats.metric);
println!("Index type: {}", stats.index_type);      // "brute_force" or "hnsw"
println!("Memory: {} bytes", stats.memory_bytes);
```

### Delete a Collection

```rust
db.vector_delete_collection("old-collection")?;
```

## Index Backends

The Vector Store supports two index backends:

| Backend | Complexity | Recall | Best For |
|---------|-----------|--------|----------|
| **Brute Force** | O(n) exact | 100% | Collections < 10K vectors |
| **HNSW** | O(log n) approximate | ~95%+ | Collections 10K+ vectors |

The default backend is **Brute Force**. HNSW can be selected per collection at creation time via the engine-level `IndexBackendFactory::Hnsw(HnswConfig)`.

### HNSW Configuration

The HNSW backend accepts the following parameters:

| Parameter | Default | Description |
|-----------|---------|-------------|
| `m` | 16 | Max connections per graph layer |
| `ef_construction` | 200 | Beam width during index build (higher = better recall, slower build) |
| `ef_search` | 50 | Beam width during search (higher = better recall, slower search) |
| `ml` | 1/ln(m) | Level multiplier for probabilistic layer assignment |

## Storing Vectors

Use `vector_upsert` to insert or update a vector by key:

```rust
let db = Strata::cache()?;
db.vector_create_collection("docs", 4, DistanceMetric::Cosine)?;

// Simple upsert (no metadata)
db.vector_upsert("docs", "doc-1", vec![1.0, 0.0, 0.0, 0.0], None)?;

// Upsert with metadata
let metadata: Value = serde_json::json!({
    "source": "conversation",
    "timestamp": 1234567890
}).into();
db.vector_upsert("docs", "doc-2", vec![0.0, 1.0, 0.0, 0.0], Some(metadata))?;
```

The dimension of the vector must match the collection's dimension. A mismatch returns a `DimensionMismatch` error.

### Batch Upsert

For bulk loading, use `vector_batch_upsert` to insert multiple vectors atomically in a single transaction:

```rust
use stratadb::BatchVectorEntry;

let entries = vec![
    BatchVectorEntry {
        key: "chunk-0".into(),
        vector: vec![1.0, 0.0, 0.0, 0.0],
        metadata: Some(serde_json::json!({"page": 1})),
    },
    BatchVectorEntry {
        key: "chunk-1".into(),
        vector: vec![0.0, 1.0, 0.0, 0.0],
        metadata: Some(serde_json::json!({"page": 2})),
    },
    BatchVectorEntry {
        key: "chunk-2".into(),
        vector: vec![0.0, 0.0, 1.0, 0.0],
        metadata: Some(serde_json::json!({"page": 3})),
    },
];

let versions = db.vector_batch_upsert("docs", entries)?;
println!("Inserted {} vectors", versions.len());
```

Batch upsert validates all entries before committing. If any entry has an invalid dimension, the entire batch fails atomically (no partial writes).

## Retrieving Vectors

```rust
let data = db.vector_get("docs", "doc-1")?;
if let Some(versioned) = data {
    println!("Key: {}", versioned.key);
    println!("Embedding: {:?}", versioned.data.embedding);
    println!("Metadata: {:?}", versioned.data.metadata);
}
```

## Searching

Search for the `k` most similar vectors to a query:

```rust
let db = Strata::cache()?;
db.vector_create_collection("items", 4, DistanceMetric::Cosine)?;

db.vector_upsert("items", "a", vec![1.0, 0.0, 0.0, 0.0], None)?;
db.vector_upsert("items", "b", vec![0.9, 0.1, 0.0, 0.0], None)?;
db.vector_upsert("items", "c", vec![0.0, 1.0, 0.0, 0.0], None)?;

// Find 2 most similar to [1.0, 0.0, 0.0, 0.0]
let results = db.vector_search("items", vec![1.0, 0.0, 0.0, 0.0], 2)?;

for m in &results {
    println!("{}: score={}", m.key, m.score);
}
// Output: a: score=1.0, b: score=0.995...
```

### VectorMatch Fields

| Field | Type | Description |
|-------|------|-------------|
| `key` | `String` | The vector's key |
| `score` | `f32` | Similarity score (higher = more similar) |
| `metadata` | `Option<Value>` | The vector's metadata (if stored) |

### Metadata Filtering

Search results can be filtered by metadata using 8 operators:

| Operator | Description | Example |
|----------|-------------|---------|
| `Eq` | Equals | `source == "docs"` |
| `Ne` | Not equals | `status != "archived"` |
| `Gt` | Greater than | `score > 0.5` |
| `Gte` | Greater than or equal | `version >= 2` |
| `Lt` | Less than | `priority < 10` |
| `Lte` | Less than or equal | `age <= 30` |
| `In` | Value in set | `category in ["a", "b"]` |
| `Contains` | String contains substring | `name contains "test"` |

Metadata filtering is **post-filter** — the backend returns candidates, then metadata is loaded and filtered. The engine uses adaptive over-fetch (3x, 6x, 12x multipliers) to ensure enough results survive filtering.

## Deleting Vectors

```rust
let existed = db.vector_delete("docs", "doc-1")?;
assert!(existed);
```

## Common Patterns

### RAG Context Store

```rust
let db = Strata::cache()?;
db.vector_create_collection("knowledge", 384, DistanceMetric::Cosine)?;

// Bulk-index document chunks
let entries: Vec<BatchVectorEntry> = chunks.iter().enumerate()
    .map(|(i, chunk)| BatchVectorEntry {
        key: format!("chunk-{}", i),
        vector: embed(chunk),
        metadata: Some(serde_json::json!({
            "text": chunk,
            "source": "docs",
            "chunk_index": i
        })),
    })
    .collect();
db.vector_batch_upsert("knowledge", entries)?;

// Search for relevant context
let query_embedding = embed("How does StrataDB handle concurrency?");
let results = db.vector_search("knowledge", query_embedding, 5)?;

for m in &results {
    println!("Relevant chunk: {} (score: {})", m.key, m.score);
}
```

## Branch Isolation

Vector collections and their data are isolated by branch.

## Space Isolation

Within a branch, vector collections are scoped to the current space. Each space has its own independent set of collections:

```rust
let mut db = Strata::cache()?;

db.vector_create_collection("docs", 4, DistanceMetric::Cosine)?;
db.vector_upsert("docs", "item-1", vec![1.0, 0.0, 0.0, 0.0], None)?;

db.set_space("other")?;
let collections = db.vector_list_collections()?;
assert!(collections.is_empty()); // separate collections per space
```

See [Spaces](spaces.md) for the full guide.

## Transactions

Vector operations **do not** participate in transactions. They are executed immediately and are always visible, even within a session that has an active transaction.

## Next

- [Branch Management](branch-management.md) — creating and managing branches
- [Cookbook: RAG with Vectors](../cookbook/rag-with-vectors.md) — full RAG pattern
