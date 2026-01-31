# Command Reference

The `Command` enum is the instruction set of StrataDB. Every operation that can be performed on the database is represented as a variant. Commands are self-contained, serializable, and typed.

This reference is primarily for SDK builders and contributors. Most users should use the typed `Strata` API instead.

## Command Categories

| Category | Count | Description |
|----------|-------|-------------|
| KV | 5 | Key-value operations |
| JSON | 5 | JSON document operations |
| Event | 4 | Event log operations |
| State | 5 | State cell operations |
| Vector | 7 | Vector store operations |
| Run | 5 | Run lifecycle operations |
| Transaction | 5 | Transaction control |
| Retention | 3 | Retention policy |
| Database | 4 | Database-level operations |
| Bundle | 3 | Run export/import |
| Intelligence | 1 | Cross-primitive search |

## KV Commands

| Command | Fields | Output |
|---------|--------|--------|
| `KvPut` | `run?`, `key`, `value` | `Version(u64)` |
| `KvGet` | `run?`, `key` | `Maybe(Option<Value>)` |
| `KvDelete` | `run?`, `key` | `Bool(existed)` |
| `KvList` | `run?`, `prefix?` | `Keys(Vec<String>)` |
| `KvGetv` | `run?`, `key` | `VersionHistory(Option<Vec<VersionedValue>>)` |

## JSON Commands

| Command | Fields | Output |
|---------|--------|--------|
| `JsonSet` | `run?`, `key`, `path`, `value` | `Version(u64)` |
| `JsonGet` | `run?`, `key`, `path` | `Maybe(Option<Value>)` |
| `JsonDelete` | `run?`, `key`, `path` | `Uint(count)` |
| `JsonGetv` | `run?`, `key` | `VersionHistory(Option<Vec<VersionedValue>>)` |
| `JsonList` | `run?`, `prefix?`, `cursor?`, `limit` | `JsonListResult { keys, cursor }` |

## Event Commands

| Command | Fields | Output |
|---------|--------|--------|
| `EventAppend` | `run?`, `event_type`, `payload` | `Version(u64)` |
| `EventRead` | `run?`, `sequence` | `MaybeVersioned(Option<VersionedValue>)` |
| `EventReadByType` | `run?`, `event_type` | `VersionedValues(Vec<VersionedValue>)` |
| `EventLen` | `run?` | `Uint(count)` |

## State Commands

| Command | Fields | Output |
|---------|--------|--------|
| `StateSet` | `run?`, `cell`, `value` | `Version(u64)` |
| `StateRead` | `run?`, `cell` | `Maybe(Option<Value>)` |
| `StateCas` | `run?`, `cell`, `expected_counter?`, `value` | `MaybeVersion(Option<u64>)` |
| `StateInit` | `run?`, `cell`, `value` | `Version(u64)` |
| `StateReadv` | `run?`, `cell` | `VersionHistory(Option<Vec<VersionedValue>>)` |

## Vector Commands

| Command | Fields | Output |
|---------|--------|--------|
| `VectorCreateCollection` | `run?`, `collection`, `dimension`, `metric` | `Version(u64)` |
| `VectorDeleteCollection` | `run?`, `collection` | `Bool(existed)` |
| `VectorListCollections` | `run?` | `VectorCollectionList(Vec<CollectionInfo>)` |
| `VectorUpsert` | `run?`, `collection`, `key`, `vector`, `metadata?` | `Version(u64)` |
| `VectorGet` | `run?`, `collection`, `key` | `VectorData(Option<VersionedVectorData>)` |
| `VectorDelete` | `run?`, `collection`, `key` | `Bool(existed)` |
| `VectorSearch` | `run?`, `collection`, `query`, `k`, `filter?`, `metric?` | `VectorMatches(Vec<VectorMatch>)` |

## Run Commands

| Command | Fields | Output |
|---------|--------|--------|
| `RunCreate` | `run_id?`, `metadata?` | `RunWithVersion { info, version }` |
| `RunGet` | `run` | `RunInfoVersioned(info)` or `Maybe(None)` |
| `RunList` | `state?`, `limit?`, `offset?` | `RunInfoList(Vec<VersionedRunInfo>)` |
| `RunExists` | `run` | `Bool(exists)` |
| `RunDelete` | `run` | `Unit` |

## Transaction Commands

| Command | Fields | Output |
|---------|--------|--------|
| `TxnBegin` | `run?`, `options?` | `TxnBegun` |
| `TxnCommit` | (none) | `TxnCommitted { version }` |
| `TxnRollback` | (none) | `TxnAborted` |
| `TxnInfo` | (none) | `TxnInfo(Option<TransactionInfo>)` |
| `TxnIsActive` | (none) | `Bool(active)` |

## Database Commands

| Command | Fields | Output |
|---------|--------|--------|
| `Ping` | (none) | `Pong { version }` |
| `Info` | (none) | `DatabaseInfo(info)` |
| `Flush` | (none) | `Unit` |
| `Compact` | (none) | `Unit` |

## Bundle Commands

| Command | Fields | Output |
|---------|--------|--------|
| `RunExport` | `run_id`, `path` | `RunExported(result)` |
| `RunImport` | `path` | `RunImported(result)` |
| `RunBundleValidate` | `path` | `BundleValidated(result)` |

## Retention Commands

| Command | Fields | Output |
|---------|--------|--------|
| `RetentionApply` | `run?` | (retention result) |
| `RetentionStats` | `run?` | (retention stats) |
| `RetentionPreview` | `run?` | (retention preview) |

## Intelligence Commands

| Command | Fields | Output |
|---------|--------|--------|
| `Search` | `run?`, `query`, `k?`, `primitives?` | `SearchResults(Vec<SearchResultHit>)` |

## Run Field Convention

Data-scoped commands have an optional `run` field. When `None`, it defaults to the "default" run. Run lifecycle commands (RunGet, RunDelete, etc.) have a required `run` field.

## Serialization

All commands implement `Serialize` and `Deserialize` with `deny_unknown_fields`. The format uses serde's externally tagged representation:

```json
{"KvPut": {"key": "foo", "value": {"Int": 42}}}
{"KvGet": {"key": "foo"}}
{"TxnCommit": null}
```
