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
| Vector | 9 | Vector store operations |
| Branch | 5 | Branch lifecycle operations |
| Space | 4 | Space management operations |
| Transaction | 5 | Transaction control |
| Retention | 3 | Retention policy |
| Database | 4 | Database-level operations |
| Bundle | 3 | Branch export/import |
| Intelligence | 1 | Cross-primitive search |

## KV Commands

| Command | Fields | Output |
|---------|--------|--------|
| `KvPut` | `branch?`, `space?`, `key`, `value` | `Version(u64)` |
| `KvGet` | `branch?`, `space?`, `key` | `Maybe(Option<Value>)` |
| `KvDelete` | `branch?`, `space?`, `key` | `Bool(existed)` |
| `KvList` | `branch?`, `space?`, `prefix?` | `Keys(Vec<String>)` |
| `KvGetv` | `branch?`, `space?`, `key` | `VersionHistory(Option<Vec<VersionedValue>>)` |

## JSON Commands

| Command | Fields | Output |
|---------|--------|--------|
| `JsonSet` | `branch?`, `space?`, `key`, `path`, `value` | `Version(u64)` |
| `JsonGet` | `branch?`, `space?`, `key`, `path` | `Maybe(Option<Value>)` |
| `JsonDelete` | `branch?`, `space?`, `key`, `path` | `Uint(count)` |
| `JsonGetv` | `branch?`, `space?`, `key` | `VersionHistory(Option<Vec<VersionedValue>>)` |
| `JsonList` | `branch?`, `space?`, `prefix?`, `cursor?`, `limit` | `JsonListResult { keys, cursor }` |

## Event Commands

| Command | Fields | Output |
|---------|--------|--------|
| `EventAppend` | `branch?`, `space?`, `event_type`, `payload` | `Version(u64)` |
| `EventRead` | `branch?`, `space?`, `sequence` | `MaybeVersioned(Option<VersionedValue>)` |
| `EventReadByType` | `branch?`, `space?`, `event_type` | `VersionedValues(Vec<VersionedValue>)` |
| `EventLen` | `branch?`, `space?` | `Uint(count)` |

## State Commands

| Command | Fields | Output |
|---------|--------|--------|
| `StateSet` | `branch?`, `space?`, `cell`, `value` | `Version(u64)` |
| `StateRead` | `branch?`, `space?`, `cell` | `Maybe(Option<Value>)` |
| `StateCas` | `branch?`, `space?`, `cell`, `expected_counter?`, `value` | `MaybeVersion(Option<u64>)` |
| `StateInit` | `branch?`, `space?`, `cell`, `value` | `Version(u64)` |
| `StateReadv` | `branch?`, `space?`, `cell` | `VersionHistory(Option<Vec<VersionedValue>>)` |

## Vector Commands

| Command | Fields | Output |
|---------|--------|--------|
| `VectorCreateCollection` | `branch?`, `space?`, `collection`, `dimension`, `metric` | `Version(u64)` |
| `VectorDeleteCollection` | `branch?`, `space?`, `collection` | `Bool(existed)` |
| `VectorListCollections` | `branch?`, `space?` | `VectorCollectionList(Vec<CollectionInfo>)` |
| `VectorCollectionStats` | `branch?`, `space?`, `collection` | `VectorCollectionList(Vec<CollectionInfo>)` |
| `VectorUpsert` | `branch?`, `space?`, `collection`, `key`, `vector`, `metadata?` | `Version(u64)` |
| `VectorBatchUpsert` | `branch?`, `space?`, `collection`, `entries` | `Versions(Vec<u64>)` |
| `VectorGet` | `branch?`, `space?`, `collection`, `key` | `VectorData(Option<VersionedVectorData>)` |
| `VectorDelete` | `branch?`, `space?`, `collection`, `key` | `Bool(existed)` |
| `VectorSearch` | `branch?`, `space?`, `collection`, `query`, `k`, `filter?`, `metric?` | `VectorMatches(Vec<VectorMatch>)` |

## Branch Commands

| Command | Fields | Output |
|---------|--------|--------|
| `RunCreate` | `branch_id?`, `metadata?` | `RunWithVersion { info, version }` |
| `BranchGet` | `branch` | `BranchInfoVersioned(info)` or `Maybe(None)` |
| `RunList` | `state?`, `limit?`, `offset?` | `RunInfoList(Vec<VersionedBranchInfo>)` |
| `BranchExists` | `branch` | `Bool(exists)` |
| `BranchDelete` | `branch` | `Unit` |

## Space Commands

| Command | Fields | Output |
|---------|--------|--------|
| `SpaceList` | `branch?` | `SpaceList(Vec<String>)` |
| `SpaceCreate` | `branch?`, `space` | `Unit` |
| `SpaceDelete` | `branch?`, `space`, `force` | `Unit` |
| `SpaceExists` | `branch?`, `space` | `Bool(exists)` |

## Transaction Commands

| Command | Fields | Output |
|---------|--------|--------|
| `TxnBegin` | `branch?`, `options?` | `TxnBegun` |
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
| `RunExport` | `branch_id`, `path` | `RunExported(result)` |
| `RunImport` | `path` | `RunImported(result)` |
| `RunBundleValidate` | `path` | `BundleValidated(result)` |

## Retention Commands

| Command | Fields | Output |
|---------|--------|--------|
| `RetentionApply` | `branch?` | (retention result) |
| `RetentionStats` | `branch?` | (retention stats) |
| `RetentionPreview` | `branch?` | (retention preview) |

## Intelligence Commands

| Command | Fields | Output |
|---------|--------|--------|
| `Search` | `branch?`, `query`, `k?`, `primitives?` | `SearchResults(Vec<SearchResultHit>)` |

## Branch Field Convention

Data-scoped commands have an optional `branch` field. When `None`, it defaults to the "default" branch. Branch lifecycle commands (BranchGet, BranchDelete, etc.) have a required `branch` field.

## Space Field Convention

Data-scoped commands have an optional `space` field. When `None`, it defaults to the current space on the handle (initially `"default"`). Space lifecycle commands (`SpaceList`, `SpaceCreate`, `SpaceDelete`, `SpaceExists`) do not have a `space` field — they operate on spaces within the specified branch.

## Serialization

All commands implement `Serialize` and `Deserialize` with `deny_unknown_fields`. The format uses serde's externally tagged representation:

```json
{"KvPut": {"key": "foo", "value": {"Int": 42}}}
{"KvGet": {"key": "foo"}}
{"TxnCommit": null}
```
