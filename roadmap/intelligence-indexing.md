# Intelligence: Indexing

**Theme**: Reduce search latency across all primitives through indexing.

## Context

Today, searches in `strata-intelligence` scan data at query time. As dataset sizes grow, this becomes the dominant cost. An indexing layer that maintains precomputed structures can drop search latency by orders of magnitude.

The right index type is an open question. The embedded database space and the AI retrieval space are both moving fast — B-trees, LSM trees, HNSW, IVF, graph indexes, learned indexes, and hybrid structures all have trade-offs that depend on workload shape, update frequency, and hardware constraints.

## What we know

- Indexing needs to cover all searchable primitives: KV (prefix/range), JSON (path + text), Event (type + time), Vector (similarity), and cross-primitive hybrid search
- Indexes must be branch-scoped (consistent with data isolation model)
- Indexes must survive crash recovery (rebuilt from WAL/snapshots, or persisted alongside data)
- Write amplification matters — index maintenance cannot make writes dramatically slower

## What we don't know

- Which index structures best fit each primitive's access patterns
- Whether one general-purpose index (e.g., an LSM-backed inverted index) covers most cases, or whether each primitive needs a specialized structure
- How SOTA evolves — learned indexes, quantization-aware structures, and GPU-accelerated search are all active research areas

## Approach

Track state of the art. Let the benchmark and scaling data (from [performance characterization](performance-characterization.md)) quantify the current search latency baseline, then evaluate index structures against that baseline with real workload profiles.

## Dependencies

- Performance characterization (establishes the latency baseline to improve against)
- Engine & storage optimization (indexing builds on top of a stable, performant storage layer)
