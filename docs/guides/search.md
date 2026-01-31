# Search Guide

StrataDB provides hybrid search across primitives, combining BM25 keyword scoring with Reciprocal Rank Fusion (RRF) for result merging.

## Overview

The search system indexes data from multiple primitives (KV values, event payloads, JSON documents) and lets you query across all of them with a single text query.

## Using Search

Search is available via the `Command::Search` interface:

```rust
use stratadb::{Command, Output};

let output = db.executor().execute(Command::Search {
    branch: None,              // Uses current branch
    query: "error handling".into(),
    k: Some(10),            // Return top 10 results
    primitives: None,       // Search all primitives
})?;

if let Output::SearchResults(hits) = output {
    for hit in &hits {
        println!("[{}] {} (score: {:.3})", hit.primitive, hit.entity, hit.score);
        if let Some(snippet) = &hit.snippet {
            println!("  {}", snippet);
        }
    }
}
```

## Search Result Fields

Each `SearchResultHit` contains:

| Field | Type | Description |
|-------|------|-------------|
| `entity` | `String` | Identifier of the matched item |
| `primitive` | `String` | Which primitive produced the hit (e.g., "kv", "json") |
| `score` | `f32` | Relevance score (higher = more relevant) |
| `rank` | `u32` | Position in results (1-indexed) |
| `snippet` | `Option<String>` | Text snippet showing the match |

## How It Works

### BM25 Keyword Scoring

StrataDB maintains an inverted index of text content across primitives. When you search, the query is tokenized and matched against the index using BM25 scoring — the same algorithm used by search engines.

### Reciprocal Rank Fusion (RRF)

When results come from multiple primitives, RRF combines the rankings into a unified score. This avoids the problem of comparing raw scores across different scoring systems.

The RRF formula for a document `d`:

```
RRF_score(d) = sum(1 / (k + rank_i(d))) for each ranking i
```

where `k` is a constant (typically 60) and `rank_i(d)` is the document's rank in ranking `i`.

## Filtering by Primitive

Restrict search to specific primitives:

```rust
let output = db.executor().execute(Command::Search {
    branch: None,
    query: "configuration".into(),
    k: Some(10),
    primitives: Some(vec!["kv".into(), "json".into()]), // Only KV and JSON
})?;
```

## Branch Isolation

Search results are scoped to the current branch. Data from other branches is not included.

## Next

- [Database Configuration](database-configuration.md) — opening methods and settings
- [Architecture: Intelligence](../architecture/index.md) — search internals
