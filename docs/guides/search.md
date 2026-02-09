# Search Guide

StrataDB provides intelligent cross-primitive search with BM25 keyword scoring, Reciprocal Rank Fusion (RRF), optional LLM-powered query expansion, and result reranking.

## Overview

The search system indexes data from multiple primitives (KV values, event payloads, JSON documents) and lets you query across all of them with a single structured query. When a model is configured, search transparently expands queries for better recall and reranks results for better precision.

## SearchQuery Structure

All search operations use a structured `SearchQuery` JSON object:

```json
{
  "query": "user authentication errors",
  "k": 10,
  "primitives": ["kv", "json", "event"],
  "time_range": {
    "start": "2026-02-07T00:00:00Z",
    "end": "2026-02-09T23:59:59Z"
  },
  "mode": "hybrid",
  "expand": true,
  "rerank": true
}
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `query` | string | *required* | Natural-language or keyword query |
| `k` | integer | 10 | Number of results to return |
| `primitives` | string[] | all | Restrict to specific primitives (`kv`, `json`, `event`, `state`, `branch`, `vector`) |
| `time_range` | object | none | Filter results to a time window |
| `time_range.start` | string | — | Range start (inclusive), ISO 8601 datetime |
| `time_range.end` | string | — | Range end (inclusive), ISO 8601 datetime |
| `mode` | string | `"hybrid"` | Search mode: `"keyword"` or `"hybrid"` |
| `expand` | boolean | auto | Enable query expansion (requires configured model) |
| `rerank` | boolean | auto | Enable result reranking (requires configured model) |

Only `query` is required. All other fields are optional with sensible defaults.

## Using Search

### CLI

```bash
# Basic search
strata --cache search "error handling" --k 10

# Filter by primitive
strata --cache search "configuration" --k 5 --primitives kv,json

# Time-scoped search
strata --cache search "user errors" \
  --time-start "2026-02-07T00:00:00Z" \
  --time-end "2026-02-09T00:00:00Z"

# Keyword-only mode, no expansion
strata --cache search "auth login" --mode keyword --expand false

# Force reranking on
strata --cache search "database issues" --rerank true
```

### Interactive Shell

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

## Search Result Fields

Each result contains:

| Field | Description |
|-------|-------------|
| `entity` | Identifier of the matched item |
| `primitive` | Which primitive produced the hit (e.g., `"kv"`, `"json"`) |
| `score` | Relevance score (higher = more relevant) |
| `rank` | Position in results (1-indexed) |
| `snippet` | Text snippet showing the match |

## How It Works

### BM25 Keyword Scoring

StrataDB maintains an inverted index of text content across primitives. When you search, the query is tokenized and matched against the index using BM25 scoring — the same algorithm used by search engines.

### Reciprocal Rank Fusion (RRF)

When results come from multiple primitives or multiple query variants (via expansion), RRF combines the rankings into a unified score:

```
RRF_score(d) = sum(1 / (k + rank_i(d))) for each ranking i
```

where `k` is a constant (typically 60) and `rank_i(d)` is the document's rank in ranking `i`.

### Search Modes

| Mode | Description |
|------|-------------|
| `hybrid` (default) | BM25 keyword scoring + vector similarity, fused via RRF |
| `keyword` | BM25 keyword scoring only |

## Intelligent Search Features

When a model is configured (via `configure_model`), search gains two additional capabilities. Both are enabled by default when a model is available, and can be controlled per-query via the `expand` and `rerank` fields.

### Query Expansion

The configured model generates alternative phrasings of the original query. Each expansion is searched independently and results are fused via RRF with a boost factor, improving recall for ambiguous or under-specified queries.

**Strong signal detection**: Before calling the model, a cheap BM25 probe runs first. If the top result has a high score with a clear gap over the second result, expansion is skipped entirely — avoiding unnecessary LLM calls when the keyword match is already strong.

### Result Reranking

After initial retrieval and fusion, the top candidates (up to 20) are sent to the model for relevance scoring. The reranker scores are blended with the original RRF scores using position-aware weights, improving precision in the final ranking.

Reranking is skipped when fewer than 3 snippets are available.

### Toggle Behavior

| `expand` / `rerank` value | Behavior |
|----------------------------|----------|
| absent (`null`) | Auto: enabled if model configured |
| `true` | Force on (silently skipped if no model) |
| `false` | Force off |

## Time Range Filtering

Time ranges use ISO 8601 datetime strings and filter results to data created within the specified window.

```json
{
  "query": "deployment failures",
  "time_range": {
    "start": "2026-02-01T00:00:00Z",
    "end": "2026-02-09T23:59:59Z"
  }
}
```

Supported formats include timezone offsets (e.g., `2026-02-07T15:30:00+05:30`). Timestamps before the Unix epoch are rejected.

When `time_range` is set:
- BM25 index hits are filtered by creation timestamp
- Vector shadow collection searches use temporal HNSW filtering
- Expansion sub-queries inherit the same time range

## Filtering by Primitive

Restrict search to specific primitives:

```bash
# Only search KV and JSON
strata --cache search "configuration" --k 10 --primitives kv,json
```

Available primitives: `kv`, `json`, `event`, `state`, `branch`, `vector`.

## Branch Isolation

Search results are scoped to the current branch. Data from other branches is not included.

## Next

- [Database Configuration](database-configuration.md) — opening methods and settings
- [Architecture: Intelligence](../architecture/index.md) — search internals
