# Vector Store Guide

The Vector Store holds embedding vectors in named collections and supports similarity search. Use it for RAG context, agent memory, and any workflow that involves finding similar items.

## Command Overview

| Command | Syntax | Returns |
|---------|--------|---------|
| `vector create` | `vector create <name> <dim> [--metric M]` | OK |
| `vector drop` | `vector drop <name>` | OK |
| `vector collections` | `vector collections` | All collections |
| `vector stats` | `vector stats <coll>` | Collection details |
| `vector upsert` | `vector upsert <coll> <key> <vector> [--metadata JSON]` | OK |
| `vector batch-upsert` | `vector batch-upsert <coll> <json>` | OK |
| `vector get` | `vector get <coll> <key>` | Vector data |
| `vector del` | `vector del <coll> <key>` | OK |
| `vector search` | `vector search <coll> <query> [k] [--metric M] [--filter JSON]` | Top-k matches |

## Collections

Before storing vectors, create a collection with a fixed dimension and distance metric:

```
$ strata --cache
strata:default/default> vector create embeddings 384 --metric cosine
OK
strata:default/default> vector create positions 3 --metric euclidean
OK
strata:default/default> vector create scores 128 --metric dot
OK
```

### Distance Metrics

| Metric | Best For | Score Range |
|--------|----------|-------------|
| `cosine` | Text embeddings, normalized vectors | [-1, 1] (higher = more similar) |
| `euclidean` | Spatial data, positions | (0, 1] (higher = more similar) |
| `dot` | Pre-normalized embeddings, scoring | Unbounded (higher = more similar) |

All metrics are normalized so that **higher scores = more similar**.

### List Collections

```
$ strata --cache
strata:default/default> vector create embeddings 384 --metric cosine
OK
strata:default/default> vector collections
embeddings: 384 dimensions, cosine metric, 0 vectors
```

### Collection Statistics

Get detailed information about a specific collection:

```
$ strata --cache
strata:default/default> vector create embeddings 384 --metric cosine
OK
strata:default/default> vector stats embeddings
name: embeddings
count: 0
dimension: 384
metric: cosine
index_type: flat
memory_bytes: 0
```

### Delete a Collection

```
$ strata --cache
strata:default/default> vector create old-collection 4 --metric cosine
OK
strata:default/default> vector drop old-collection
OK
```

## Index Backends

The Vector Store supports two index backends:

| Backend | Complexity | Recall | Best For |
|---------|-----------|--------|----------|
| **Brute Force** | O(n) exact | 100% | Collections < 10K vectors |
| **HNSW** | O(log n) approximate | ~95%+ | Collections 10K+ vectors |

The default backend is **Brute Force**. HNSW can be selected per collection at creation time via the engine-level configuration.

### HNSW Configuration

The HNSW backend accepts the following parameters:

| Parameter | Default | Description |
|-----------|---------|-------------|
| `m` | 16 | Max connections per graph layer |
| `ef_construction` | 200 | Beam width during index build (higher = better recall, slower build) |
| `ef_search` | 50 | Beam width during search (higher = better recall, slower search) |
| `ml` | 1/ln(m) | Level multiplier for probabilistic layer assignment |

## Storing Vectors

Use `vector upsert` to insert or update a vector by key:

```
$ strata --cache
strata:default/default> vector create docs 4 --metric cosine
OK
strata:default/default> vector upsert docs doc-1 [1.0,0.0,0.0,0.0]
OK
strata:default/default> vector upsert docs doc-2 [0.0,1.0,0.0,0.0] --metadata '{"source":"conversation","timestamp":1234567890}'
OK
```

The dimension of the vector must match the collection's dimension. A mismatch returns a `DimensionMismatch` error.

### Batch Upsert

For bulk loading, use `vector batch-upsert` to insert multiple vectors in a single operation:

```bash
strata --cache vector batch-upsert docs '[{"key":"chunk-0","vector":[1.0,0.0,0.0,0.0],"metadata":{"page":1}},{"key":"chunk-1","vector":[0.0,1.0,0.0,0.0],"metadata":{"page":2}},{"key":"chunk-2","vector":[0.0,0.0,1.0,0.0],"metadata":{"page":3}}]'
```

Batch upsert validates all entries before committing. If any entry has an invalid dimension, the entire batch fails atomically (no partial writes).

## Retrieving Vectors

```
$ strata --cache
strata:default/default> vector create docs 4 --metric cosine
OK
strata:default/default> vector upsert docs doc-1 [1.0,0.0,0.0,0.0]
OK
strata:default/default> vector get docs doc-1
key=doc-1 vector=[1.0,0.0,0.0,0.0] metadata=null
```

## Searching

Search for the `k` most similar vectors to a query:

```
$ strata --cache
strata:default/default> vector create items 4 --metric cosine
OK
strata:default/default> vector upsert items a [1.0,0.0,0.0,0.0]
OK
strata:default/default> vector upsert items b [0.9,0.1,0.0,0.0]
OK
strata:default/default> vector upsert items c [0.0,1.0,0.0,0.0]
OK
strata:default/default> vector search items [1.0,0.0,0.0,0.0] 2
key=a score=1.0000
key=b score=0.9939
```

### Search Result Fields

| Field | Description |
|-------|-------------|
| `key` | The vector's key |
| `score` | Similarity score (higher = more similar) |
| `metadata` | The vector's metadata (if stored) |

### Metadata Filtering

Search results can be filtered by metadata using 8 operators:

| Operator | Description | Example |
|----------|-------------|---------|
| `eq` | Equals | `source == "docs"` |
| `ne` | Not equals | `status != "archived"` |
| `gt` | Greater than | `score > 0.5` |
| `gte` | Greater than or equal | `version >= 2` |
| `lt` | Less than | `priority < 10` |
| `lte` | Less than or equal | `age <= 30` |
| `in` | Value in set | `category in ["a", "b"]` |
| `contains` | String contains substring | `name contains "test"` |

```bash
strata --cache vector search items [1.0,0.0,0.0,0.0] 10 --filter '{"source":{"eq":"docs"}}'
```

Metadata filtering is **post-filter** — the backend returns candidates, then metadata is loaded and filtered. The engine uses adaptive over-fetch (3x, 6x, 12x multipliers) to ensure enough results survive filtering.

## Deleting Vectors

```
$ strata --cache
strata:default/default> vector create docs 4 --metric cosine
OK
strata:default/default> vector upsert docs doc-1 [1.0,0.0,0.0,0.0]
OK
strata:default/default> vector del docs doc-1
OK
```

## Common Patterns

### RAG Context Store

```bash
#!/bin/bash
set -euo pipefail

# Create collection
strata --cache vector create knowledge 384 --metric cosine

# Bulk-index document chunks (generate embeddings externally)
strata --cache vector batch-upsert knowledge '[
  {"key":"chunk-0","vector":[...],"metadata":{"text":"...","source":"docs","chunk_index":0}},
  {"key":"chunk-1","vector":[...],"metadata":{"text":"...","source":"docs","chunk_index":1}}
]'

# Search for relevant context
strata --cache vector search knowledge "[0.1,0.2,...]" 5
```

## Branch Isolation

Vector collections and their data are isolated by branch.

## Space Isolation

Within a branch, vector collections are scoped to the current space. Each space has its own independent set of collections:

```
$ strata --cache
strata:default/default> vector create docs 4 --metric cosine
OK
strata:default/default> vector upsert docs item-1 [1.0,0.0,0.0,0.0]
OK
strata:default/default> use default other
strata:default/other> vector collections
(empty)
```

See [Spaces](spaces.md) for the full guide.

## Transactions

Vector operations **do not** participate in transactions. They are executed immediately and are always visible, even within a session that has an active transaction.

## Next

- [Branch Management](branch-management.md) — creating and managing branches
- [Cookbook: RAG with Vectors](../cookbook/rag-with-vectors.md) — full RAG pattern
