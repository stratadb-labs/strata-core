# Vector Store Guide

The Vector Store holds embedding vectors in named collections and supports similarity search. Use it for RAG context, agent memory, and any workflow that involves finding similar items.

## API Overview

| Method | Signature | Returns |
|--------|-----------|---------|
| `vector_create_collection` | `(collection: &str, dimension: u64, metric: DistanceMetric) -> Result<u64>` | Version |
| `vector_delete_collection` | `(collection: &str) -> Result<bool>` | Whether it existed |
| `vector_list_collections` | `() -> Result<Vec<CollectionInfo>>` | All collections |
| `vector_upsert` | `(collection: &str, key: &str, vector: Vec<f32>, metadata: Option<Value>) -> Result<u64>` | Version |
| `vector_get` | `(collection: &str, key: &str) -> Result<Option<VersionedVectorData>>` | Vector data |
| `vector_delete` | `(collection: &str, key: &str) -> Result<bool>` | Whether it existed |
| `vector_search` | `(collection: &str, query: Vec<f32>, k: u64) -> Result<Vec<VectorMatch>>` | Top-k matches |

## Collections

Before storing vectors, create a collection with a fixed dimension and distance metric:

```rust
use stratadb::DistanceMetric;

let db = Strata::open_temp()?;

// 384-dimensional vectors with cosine similarity
db.vector_create_collection("embeddings", 384, DistanceMetric::Cosine)?;

// Euclidean distance
db.vector_create_collection("positions", 3, DistanceMetric::Euclidean)?;

// Dot product
db.vector_create_collection("scores", 128, DistanceMetric::DotProduct)?;
```

### Distance Metrics

| Metric | Best For | Range |
|--------|----------|-------|
| `Cosine` | Text embeddings, normalized vectors | 0.0 (identical) to 2.0 (opposite) |
| `Euclidean` | Spatial data, positions | 0.0+ (lower = more similar) |
| `DotProduct` | Pre-normalized embeddings, scoring | Higher = more similar |

### List Collections

```rust
let collections = db.vector_list_collections()?;
for c in &collections {
    println!("{}: {} dimensions, {:?} metric, {} vectors",
        c.name, c.dimension, c.metric, c.count);
}
```

### Delete a Collection

```rust
db.vector_delete_collection("old-collection")?;
```

## Storing Vectors

Use `vector_upsert` to insert or update a vector by key:

```rust
let db = Strata::open_temp()?;
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
let db = Strata::open_temp()?;
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
| `score` | `f32` | Similarity score |
| `metadata` | `Option<Value>` | The vector's metadata (if stored) |

## Deleting Vectors

```rust
let existed = db.vector_delete("docs", "doc-1")?;
assert!(existed);
```

## Common Patterns

### RAG Context Store

```rust
let db = Strata::open_temp()?;
db.vector_create_collection("knowledge", 384, DistanceMetric::Cosine)?;

// Index document chunks
for (i, chunk) in chunks.iter().enumerate() {
    let embedding = embed(chunk); // Your embedding function
    let meta: Value = serde_json::json!({
        "text": chunk,
        "source": "docs",
        "chunk_index": i
    }).into();
    db.vector_upsert("knowledge", &format!("chunk-{}", i), embedding, Some(meta))?;
}

// Search for relevant context
let query_embedding = embed("How does StrataDB handle concurrency?");
let results = db.vector_search("knowledge", query_embedding, 5)?;

for m in &results {
    println!("Relevant chunk: {} (score: {})", m.key, m.score);
}
```

## Branch Isolation

Vector collections and their data are isolated by branch.

## Transactions

Vector operations **do not** participate in transactions. They are executed immediately and are always visible, even within a session that has an active transaction.

## Next

- [Branch Management](branch-management.md) — creating and managing branches
- [Cookbook: RAG with Vectors](../cookbook/rag-with-vectors.md) — full RAG pattern
