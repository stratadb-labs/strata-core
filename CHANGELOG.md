# Changelog

All notable changes to StrataDB are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.5.1] - 2026-02-04

### Added

- **Spaces**: organizational namespaces within branches. Each branch contains one or more spaces, each with independent instances of all primitives (KV, Event, State, JSON, Vector). API: `set_space`, `current_space`, `list_spaces`, `delete_space`, `delete_space_force`.
- **Space auto-registration**: spaces are created on first write — no explicit `create_space` needed. The `default` space always exists and cannot be deleted.
- **Space parameter on all data commands**: `KvPut`, `KvGet`, `KvDelete`, `KvList`, `KvGetv`, `JsonSet`, `JsonGet`, `JsonDelete`, `JsonGetv`, `JsonList`, `EventAppend`, `EventRead`, `EventReadByType`, `EventLen`, `StateSet`, `StateRead`, `StateCas`, `StateInit`, `StateReadv`, `VectorUpsert`, `VectorBatchUpsert`, `VectorGet`, `VectorDelete`, `VectorSearch`, `VectorCreateCollection`, `VectorDeleteCollection`, `VectorListCollections`, `VectorCollectionStats` all accept an optional `space` field. When `None`, defaults to the current space on the handle (initially `"default"`).
- **Space commands**: `SpaceList`, `SpaceCreate`, `SpaceDelete` (with `force` flag), `SpaceExists` command variants for SDK builders.
- **Structured logging**: `tracing` instrumentation across 10 subsystem targets — `strata::branch`, `strata::vector`, `strata::space`, `strata::db`, `strata::txn`, `strata::command`, `strata::wal`, `strata::snapshot`, `strata::recovery`, `strata::compaction`. Zero overhead unless a `tracing` subscriber is wired up by the caller. Configurable per-subsystem log levels via standard `RUST_LOG` filtering (e.g. `RUST_LOG=strata::txn=debug`).
- **`tracing` dependency**: added to executor and engine crates for structured span and event instrumentation.

## [0.4.0] - 2026-02-03

### Added

- **HNSW index backend**: O(log n) approximate nearest neighbor search built from scratch, verified against the Malkov & Yashunin paper (arXiv:1603.09320). Configurable M, ef_construction, ef_search parameters. Selectable per collection via `IndexBackendFactory`.
- **Advanced metadata filters**: 8 filter operators (Eq, Ne, Gt, Gte, Lt, Lte, In, Contains) with `FilterCondition` and `FilterOp` types in core. Full executor bridge support.
- **Batch vector upsert**: `VectorBatchUpsert` command and `vector_batch_upsert()` API for atomic bulk vector insertion in a single transaction.
- **Collection statistics**: `VectorCollectionStats` command and `vector_collection_stats()` API. CollectionInfo now includes `index_type` and `memory_bytes` fields. Backed by `index_type_name()` and `memory_usage()` on the `VectorIndexBackend` trait.
- **Reserved internal vector namespace**: `_system_*` collections for the intelligence layer with `validate_system_collection_name()` and internal `system_insert`/`system_search` methods. Hidden from `vector_list_collections`.
- **Shared distance functions**: Extracted distance computation into `distance.rs` module shared by both BruteForce and HNSW backends (cosine, euclidean, dot product).
- **strata-security crate**: Read-only access mode for database connections (from PR #1012).

## [0.1.0] - 2026-01-30

### Added

- **Six data primitives**: KV Store, Event Log, State Cell, JSON Store, Vector Store, Run
- **Value type system**: 8-variant `Value` enum (Null, Bool, Int, Float, String, Bytes, Array, Object) with strict typing rules
- **Run-based data isolation**: git-like branches for isolating agent sessions and experiments
- **OCC transactions**: optimistic concurrency control with snapshot isolation and read-your-writes semantics via the `Session` API
- **Three durability modes**: None, Buffered (default), and Strict
- **Write-ahead log (WAL)**: CRC32-checked entries for crash recovery
- **Snapshots**: periodic full-state captures for bounded recovery time
- **Run bundles**: export/import runs as portable `.runbundle.tar.zst` archives
- **Hybrid search**: BM25 keyword scoring with Reciprocal Rank Fusion across primitives
- **Vector store**: collection management, similarity search (Cosine, Euclidean, DotProduct), metadata support
- **JSON store**: path-level reads and writes with cursor-based pagination
- **Versioned reads**: `getv()`/`readv()` API for version history access
- **Typed Strata API**: high-level Rust API with `Into<Value>` ergonomics
- **Command/Output enums**: serializable instruction set for SDK builders
- **7-crate workspace**: core, storage, concurrency, durability, engine, intelligence, executor
