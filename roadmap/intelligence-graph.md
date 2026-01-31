# Intelligence: Internal Graph Primitive

**Theme**: Relationship-aware queries powered by an internal graph structure.

## Motivation

The existing primitives store entities but not the relationships between them. A KV entry can reference another key by convention, but there's no way to ask "what is connected to X?" or "find all entities within 2 hops of Y" without the caller implementing traversal logic themselves.

A graph structure inside `strata-intelligence` can represent relationships across primitives — connecting KV entries to their related events, JSON documents to their vector embeddings, branches to their lineage — and make those relationships queryable. This makes the intelligence layer meaningfully smarter than a search index.

## Scope: internal only

This is **not** a new public primitive. It is an internal data structure within `strata-intelligence` that the intelligence layer uses to improve its own query capabilities. Users interact with it indirectly through richer search results, not through a `graph_add_edge` API.

Reasons to keep it internal:

- Exposing a full graph API invites scope creep toward a general-purpose graph database
- The value is in what the intelligence layer can *do* with relationships, not in letting users manage edges manually
- Internal means we can change the representation without breaking the public API

## What it enables

- **Cross-primitive correlation**: "Find events related to this KV entry" without the caller knowing the connection scheme
- **Relationship-weighted search**: Boost search results that are closely connected to the query context
- **Lineage queries**: Track how data flows across branches (e.g., which branch was forked from which)
- **Neighborhood context**: When retrieving an entity, optionally include its nearest neighbors in the graph

## What it is NOT

- Not NetworkX. No general-purpose graph algorithms library, no pagerank, no community detection, no visualization.
- Not a public primitive. No `graph_add_edge`, `graph_query`, or `graph_traverse` in the Strata API.
- Not a graph database. No Cypher, no SPARQL, no property graph query language.

The graph is a backing structure — like how an inverted index backs text search without being a user-facing primitive.

## Open questions

- **Edge population**: Are edges inferred automatically (e.g., shared keys, temporal proximity, embedding similarity) or do internal callers register them explicitly?
- **Persistence**: Does the graph persist, or is it rebuilt on startup from the existing primitives?
- **Representation**: Adjacency list, CSR, or something else? Depends on query patterns (traversal-heavy vs. lookup-heavy).
- **Scale**: How many edges per branch is realistic? This determines whether an in-memory representation is viable.

## Dependencies

- Intelligence indexing (the graph is one possible index structure, or works alongside indexes)
- Engine & storage optimization (graph traversal performance depends on underlying storage speed)
