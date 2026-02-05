# Search Guide

StrataDB provides hybrid search across primitives, combining BM25 keyword scoring with Reciprocal Rank Fusion (RRF) for result merging.

## Overview

The search system indexes data from multiple primitives (KV values, event payloads, JSON documents) and lets you query across all of them with a single text query.

## Using Search

```
$ strata --cache
strata:default/default> kv put doc:1 "error handling in production"
(version) 1
strata:default/default> kv put doc:2 "database configuration guide"
(version) 1
strata:default/default> search "error handling" --k 10
[kv] doc:1 (score: 0.892)
  error handling in production
```

From the shell:

```bash
strata --cache search "error handling" --k 10
strata --cache search "configuration" --k 5 --primitives kv,json
```

## Search Result Fields

Each result contains:

| Field | Description |
|-------|-------------|
| `entity` | Identifier of the matched item |
| `primitive` | Which primitive produced the hit (e.g., "kv", "json") |
| `score` | Relevance score (higher = more relevant) |
| `rank` | Position in results (1-indexed) |
| `snippet` | Text snippet showing the match |

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

```bash
# Only search KV and JSON
strata --cache search "configuration" --k 10 --primitives kv,json
```

## Branch Isolation

Search results are scoped to the current branch. Data from other branches is not included.

## Next

- [Database Configuration](database-configuration.md) — opening methods and settings
- [Architecture: Intelligence](../architecture/index.md) — search internals
