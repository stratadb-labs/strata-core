# API Quick Reference

Every method on the `Strata` struct, grouped by category.

## Database

| Method | Signature | Returns |
|--------|-----------|---------|
| `open` | `(path: impl AsRef<Path>) -> Result<Self>` | New Strata instance |
| `open_temp` | `() -> Result<Self>` | Ephemeral Strata instance |
| `from_database` | `(db: Arc<Database>) -> Result<Self>` | Strata from existing DB |
| `ping` | `() -> Result<String>` | Version string |
| `info` | `() -> Result<DatabaseInfo>` | Database statistics |
| `flush` | `() -> Result<()>` | Flushes pending writes |
| `compact` | `() -> Result<()>` | Triggers compaction |

## Run Context

| Method | Signature | Returns |
|--------|-----------|---------|
| `current_run` | `() -> &str` | Current run name |
| `set_run` | `(name: &str) -> Result<()>` | Switches current run |
| `create_run` | `(name: &str) -> Result<()>` | Creates empty run |
| `list_runs` | `() -> Result<Vec<String>>` | All run names |
| `delete_run` | `(name: &str) -> Result<()>` | Deletes run + data |
| `fork_run` | `(dest: &str) -> Result<()>` | **Not yet implemented** |
| `runs` | `() -> Runs<'_>` | Power API handle |

## KV Store

| Method | Signature | Returns | Notes |
|--------|-----------|---------|-------|
| `kv_put` | `(key: &str, value: impl Into<Value>) -> Result<u64>` | Version | Creates or overwrites |
| `kv_get` | `(key: &str) -> Result<Option<Value>>` | Value or None | |
| `kv_delete` | `(key: &str) -> Result<bool>` | Whether key existed | |
| `kv_list` | `(prefix: Option<&str>) -> Result<Vec<String>>` | Key names | |

## Event Log

| Method | Signature | Returns | Notes |
|--------|-----------|---------|-------|
| `event_append` | `(event_type: &str, payload: Value) -> Result<u64>` | Sequence number | Payload must be Object |
| `event_read` | `(sequence: u64) -> Result<Option<VersionedValue>>` | Event or None | |
| `event_read_by_type` | `(event_type: &str) -> Result<Vec<VersionedValue>>` | All events of type | |
| `event_len` | `() -> Result<u64>` | Total event count | |

## State Cell

| Method | Signature | Returns | Notes |
|--------|-----------|---------|-------|
| `state_set` | `(cell: &str, value: impl Into<Value>) -> Result<u64>` | Version | Unconditional write |
| `state_read` | `(cell: &str) -> Result<Option<Value>>` | Value or None | |
| `state_init` | `(cell: &str, value: impl Into<Value>) -> Result<u64>` | Version | Only if absent |
| `state_cas` | `(cell: &str, expected: Option<u64>, value: impl Into<Value>) -> Result<Option<u64>>` | New version or None | CAS |

## JSON Store

| Method | Signature | Returns | Notes |
|--------|-----------|---------|-------|
| `json_set` | `(key: &str, path: &str, value: impl Into<Value>) -> Result<u64>` | Version | Use "$" for root |
| `json_get` | `(key: &str, path: &str) -> Result<Option<Value>>` | Value or None | |
| `json_delete` | `(key: &str, path: &str) -> Result<u64>` | Count deleted | |
| `json_list` | `(prefix: Option<String>, cursor: Option<String>, limit: u64) -> Result<(Vec<String>, Option<String>)>` | Keys + cursor | |

## Vector Store

| Method | Signature | Returns | Notes |
|--------|-----------|---------|-------|
| `vector_create_collection` | `(name: &str, dimension: u64, metric: DistanceMetric) -> Result<u64>` | Version | |
| `vector_delete_collection` | `(name: &str) -> Result<bool>` | Whether it existed | |
| `vector_list_collections` | `() -> Result<Vec<CollectionInfo>>` | All collections | |
| `vector_upsert` | `(collection: &str, key: &str, vector: Vec<f32>, metadata: Option<Value>) -> Result<u64>` | Version | |
| `vector_get` | `(collection: &str, key: &str) -> Result<Option<VersionedVectorData>>` | Vector data or None | |
| `vector_delete` | `(collection: &str, key: &str) -> Result<bool>` | Whether it existed | |
| `vector_search` | `(collection: &str, query: Vec<f32>, k: u64) -> Result<Vec<VectorMatch>>` | Top-k matches | |

## Run Operations (Low-Level)

| Method | Signature | Returns |
|--------|-----------|---------|
| `run_create` | `(run_id: Option<String>, metadata: Option<Value>) -> Result<(RunInfo, u64)>` | Info + version |
| `run_get` | `(run: &str) -> Result<Option<VersionedRunInfo>>` | Run info or None |
| `run_list` | `(state: Option<RunStatus>, limit: Option<u64>, offset: Option<u64>) -> Result<Vec<VersionedRunInfo>>` | Run info list |
| `run_exists` | `(run: &str) -> Result<bool>` | Whether run exists |
| `run_delete` | `(run: &str) -> Result<()>` | Deletes run |

## Bundle Operations

| Method | Signature | Returns |
|--------|-----------|---------|
| `run_export` | `(run_id: &str, path: &str) -> Result<RunExportResult>` | Export info |
| `run_import` | `(path: &str) -> Result<RunImportResult>` | Import info |
| `run_validate_bundle` | `(path: &str) -> Result<BundleValidateResult>` | Validation info |

## Runs Power API

Methods on the `Runs` handle returned by `db.runs()`:

| Method | Signature | Returns |
|--------|-----------|---------|
| `list` | `() -> Result<Vec<String>>` | Run names |
| `exists` | `(name: &str) -> Result<bool>` | Whether run exists |
| `create` | `(name: &str) -> Result<()>` | Creates empty run |
| `delete` | `(name: &str) -> Result<()>` | Deletes run |
| `fork` | `(dest: &str) -> Result<()>` | **Not yet implemented** |
| `diff` | `(run1: &str, run2: &str) -> Result<RunDiff>` | **Not yet implemented** |

## Session

| Method | Signature | Returns |
|--------|-----------|---------|
| `Session::new` | `(db: Arc<Database>) -> Self` | New session |
| `execute` | `(cmd: Command) -> Result<Output>` | Command result |
| `in_transaction` | `() -> bool` | Whether a txn is active |
