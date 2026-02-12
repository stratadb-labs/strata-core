# Python SDK Reference

The StrataDB Python SDK provides native Python bindings via PyO3, with a
Pythonic namespace API, context managers, and time-travel snapshots.

## Installation

```bash
pip install stratadb
```

## Quick Start

```python
from stratadb import Strata

db = Strata.open("/path/to/data")

# Namespace API
db.kv.put("greeting", "Hello, World!")
print(db.kv.get("greeting"))  # "Hello, World!"

# Transactions
with db.transaction():
    db.kv.put("a", 1)
    db.kv.put("b", 2)
# Auto-commits on success, auto-rollbacks on exception

# Vector search with NumPy
import numpy as np
coll = db.vectors.create("docs", dimension=384)
coll.upsert("doc-1", np.random.rand(384).astype(np.float32))
results = coll.search(np.random.rand(384).astype(np.float32), k=5)

# Time-travel
from datetime import datetime
snapshot = db.at(datetime(2024, 6, 15, 12, 0))
snapshot.kv.get("greeting")  # reads value as of that timestamp
```

---

## Opening a Database

### Strata.open(path, auto_embed=False, read_only=False)

Open a database at the given path.

```python
db = Strata.open("/path/to/data")

# With auto-embedding for semantic search
db = Strata.open("/path/to/data", auto_embed=True)

# Read-only mode
db = Strata.open("/path/to/data", read_only=True)
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `path` | str | Path to the database directory |
| `auto_embed` | bool | Enable automatic text embedding (default: `False`) |
| `read_only` | bool | Open in read-only mode (default: `False`) |

**Returns:** `Strata` instance

**Raises:** `StrataError` if the database cannot be opened

### Strata.cache()

Create an ephemeral in-memory database.

```python
db = Strata.cache()
```

**Returns:** `Strata` instance

### Strata.setup() / stratadb.setup()

Download model files for auto-embedding (~80MB MiniLM-L6-v2).

```python
import stratadb
stratadb.setup()  # pre-download during installation
```

**Returns:** `str` - Path where model files are stored

---

## KV Store (`db.kv`)

### kv.put(key, value)

Store a key-value pair.

```python
version = db.kv.put("user:123", {"name": "Alice", "age": 30})
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `key` | str | The key |
| `value` | any | The value (str, int, float, bool, list, dict, bytes, None) |

**Returns:** `int` - Version number

### kv.get(key, *, default=None, as_of=None)

Get a value by key.

```python
value = db.kv.get("user:123")
if value is not None:
    print(value["name"])

# With default
name = db.kv.get("missing", default="Unknown")

# Time-travel: read historical value
historical = db.kv.get("user:123", as_of=1700002000)
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `key` | str | The key |
| `default` | any, optional | Value to return if key not found (default: `None`) |
| `as_of` | int, optional | Timestamp (microseconds since epoch) for time-travel read |

**Returns:** Value, `default` if not found

### kv.delete(key)

Delete a key.

```python
deleted = db.kv.delete("user:123")
```

**Returns:** `bool` - True if the key existed

### kv.keys(*, prefix=None)

List keys with optional prefix filter.

```python
all_keys = db.kv.keys()
user_keys = db.kv.keys(prefix="user:")
```

**Returns:** `list[str]`

### kv.list(*, prefix=None, limit=None, as_of=None)

List keys with optional prefix, limit, and time-travel support.

```python
all_keys = db.kv.list()
user_keys = db.kv.list(prefix="user:")
page = db.kv.list(prefix="user:", limit=100)
past_keys = db.kv.list(prefix="user:", as_of=1700002000)
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `prefix` | str, optional | Filter keys by prefix |
| `limit` | int, optional | Maximum number of keys to return |
| `as_of` | int, optional | Timestamp (microseconds since epoch) for time-travel read |

**Returns:** `list[str]`

### kv.get_versioned(key)

Get a value with version info.

```python
result = db.kv.get_versioned("user:123")
if result:
    print(f"Value: {result['value']}, Version: {result['version']}")
```

**Returns:** `dict` with `value`, `version`, `timestamp`, or `None`

### kv.history(key)

Get version history for a key.

```python
history = db.kv.history("user:123")
for entry in history:
    print(f"v{entry['version']}: {entry['value']}")
```

**Returns:** `list[dict]` with `value`, `version`, `timestamp`, or `None`

---

## State Cell (`db.state`)

### state.set(cell, value)

Set a state cell value.

```python
version = db.state.set("counter", 0)
```

**Returns:** `int` - Version number

### state.get(cell, *, default=None, as_of=None)

Get a state cell value.

```python
value = db.state.get("counter")

# With default
count = db.state.get("counter", default=0)

# Time-travel
historical = db.state.get("counter", as_of=1700002000)
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `cell` | str | The cell name |
| `default` | any, optional | Value to return if cell not found (default: `None`) |
| `as_of` | int, optional | Timestamp (microseconds since epoch) for time-travel read |

**Returns:** Value, `default` if not found

### state.init(cell, value)

Initialize a state cell only if it doesn't exist.

```python
version = db.state.init("counter", 0)
```

**Returns:** `int` - Version number

### state.cas(cell, new_value, *, expected_version=None)

Compare-and-swap update.

```python
new_version = db.state.cas("counter", 10, expected_version=5)
if new_version is None:
    print("CAS failed - version mismatch")
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `cell` | str | The cell name |
| `new_value` | any | The new value |
| `expected_version` | int, optional | Expected current version |

**Returns:** `int` (new version) or `None` if CAS failed

### state.delete(cell)

Delete a state cell.

```python
deleted = db.state.delete("counter")
```

**Returns:** `bool` - True if the cell existed

### state.list(*, prefix=None, as_of=None)

List state cell names.

```python
cells = db.state.list()
cells = db.state.list(prefix="config:")
past_cells = db.state.list(prefix="config:", as_of=1700002000)
```

**Returns:** `list[str]`

### state.get_versioned(cell)

Get a state cell with version info.

```python
result = db.state.get_versioned("counter")
```

**Returns:** `dict` with `value`, `version`, `timestamp`, or `None`

### state.history(cell)

Get version history for a state cell.

```python
history = db.state.history("counter")
```

**Returns:** `list[dict]` or `None`

---

## Event Log (`db.events`)

### events.append(event_type, payload)

Append an event to the log.

```python
seq = db.events.append("user_action", {"action": "click", "target": "button"})
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `event_type` | str | The event type |
| `payload` | any | The event payload |

**Returns:** `int` - Sequence number

### events.get(sequence, *, as_of=None)

Get an event by sequence number.

```python
event = db.events.get(0)
if event:
    print(event["value"])

# Time-travel
past_event = db.events.get(0, as_of=1700002000)
```

**Returns:** `dict` with `value`, `version`, `timestamp`, or `None`

### events.list(event_type, *, limit=None, after=None, as_of=None)

List events by type with optional pagination and time-travel.

```python
events = db.events.list("user_action")
for event in events:
    print(event["value"])

# Paginated
page = db.events.list("user_action", limit=100, after=500)

# Time-travel
past_events = db.events.list("user_action", as_of=1700002000)
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `event_type` | str | The event type |
| `limit` | int, optional | Maximum events |
| `after` | int, optional | Return events after this sequence |
| `as_of` | int, optional | Timestamp (microseconds since epoch) for time-travel read |

**Returns:** `list[dict]`

### events.count

Get total event count (property).

```python
count = db.events.count
```

**Returns:** `int`

### len(db.events)

Get total event count via `__len__`.

```python
count = len(db.events)
```

---

## JSON Store (`db.json`)

### json.set(key, path, value)

Set a value at a JSONPath.

```python
# Set entire document
db.json.set("user:123", "$", {"name": "Alice", "age": 30})

# Set nested field
db.json.set("user:123", "$.email", "alice@example.com")
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `key` | str | Document key |
| `path` | str | JSONPath (use `$` for root) |
| `value` | any | The value |

**Returns:** `int` - Version number

### json.get(key, path="$", *, as_of=None)

Get a value at a JSONPath. Defaults to `"$"` (root).

```python
doc = db.json.get("user:123")          # path defaults to "$"
name = db.json.get("user:123", "$.name")

# Time-travel
historical = db.json.get("config", as_of=1700002000)
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `key` | str | Document key |
| `path` | str | JSONPath (default: `"$"`) |
| `as_of` | int, optional | Timestamp (microseconds since epoch) for time-travel read |

**Returns:** Value or `None`

### json.delete(key, path="$")

Delete a value at a JSONPath. Defaults to `"$"` (whole document).

```python
deleted_count = db.json.delete("user:123", "$.email")
db.json.delete("user:123")  # delete entire document
```

**Returns:** `int` - Count of elements deleted

### json.list(*, prefix=None, limit=100, cursor=None, as_of=None)

List JSON document keys with pagination.

```python
result = db.json.list(prefix="user:")
keys = result["keys"]
next_cursor = result.get("cursor")

# Time-travel
past_result = db.json.list(prefix="user:", as_of=1700002000)
```

**Returns:** `dict` with `keys` and optional `cursor`

### json.get_versioned(key)

Get a JSON document with version info.

```python
result = db.json.get_versioned("user:123")
```

**Returns:** `dict` with `value`, `version`, `timestamp`, or `None`

### json.history(key)

Get version history for a JSON document.

```python
history = db.json.history("user:123")
```

**Returns:** `list[dict]` or `None`

---

## Vector Store (`db.vectors`)

### vectors.create(name, *, dimension, metric="cosine")

Create a vector collection and return a `Collection` handle.

```python
coll = db.vectors.create("embeddings", dimension=384)
coll = db.vectors.create("images", dimension=512, metric="euclidean")
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `name` | str | Collection name |
| `dimension` | int | Vector dimension |
| `metric` | str | `"cosine"` (default), `"euclidean"`, `"dot_product"` |

**Returns:** `Collection` handle

### vectors.collection(name)

Get a handle to an existing collection.

```python
coll = db.vectors.collection("embeddings")
```

**Returns:** `Collection` handle

### vectors.drop(name)

Delete a vector collection.

```python
deleted = db.vectors.drop("embeddings")
```

**Returns:** `bool`

### vectors.list()

List all vector collections.

```python
collections = db.vectors.list()
for c in collections:
    print(f"{c['name']}: {c['count']} vectors")
```

**Returns:** `list[dict]` with `name`, `dimension`, `metric`, `count`, `index_type`, `memory_bytes`

### "name" in db.vectors

Check if a collection exists.

```python
if "embeddings" in db.vectors:
    print("Collection exists")
```

---

## Collection

A `Collection` handle is returned by `db.vectors.create()` or `db.vectors.collection()`.

### coll.upsert(key, vector, *, metadata=None)

Insert or update a vector.

```python
import numpy as np

embedding = np.random.rand(384).astype(np.float32)
coll.upsert("doc-1", embedding, metadata={"title": "Hello"})

# Also accepts lists
coll.upsert("doc-2", [0.1, 0.2, 0.3, ...])
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `key` | str | Vector key |
| `vector` | ndarray or list | Vector embedding |
| `metadata` | dict, optional | Metadata |

**Returns:** `int` - Version number

### coll.get(key, *, as_of=None)

Get a vector by key.

```python
result = coll.get("doc-1")
if result:
    print(result["embedding"])  # NumPy array
    print(result["metadata"])

# Time-travel
past = coll.get("doc-1", as_of=1700002000)
```

**Returns:** `dict` with `key`, `embedding`, `metadata`, `version`, `timestamp`, or `None`

### coll.delete(key)

Delete a vector.

```python
deleted = coll.delete("doc-1")
```

**Returns:** `bool`

### coll.search(query, *, k=10, filter=None, metric=None, as_of=None)

Search for similar vectors. If `filter` or `metric` is provided, uses filtered search.

```python
query = np.random.rand(384).astype(np.float32)
matches = coll.search(query, k=10)
for match in matches:
    print(f"{match['key']}: {match['score']}")

# With metadata filter and metric override
matches = coll.search(
    query,
    k=10,
    metric="euclidean",
    filter=[
        {"field": "category", "op": "eq", "value": "science"},
        {"field": "year", "op": "gte", "value": 2020},
    ],
)

# Time-travel search
past_matches = coll.search(query, k=10, as_of=1700002000)
```

**Filter operators:** `eq`, `ne`, `gt`, `gte`, `lt`, `lte`, `in`, `contains`

**Returns:** `list[dict]` with `key`, `score`, `metadata`

### coll.batch_upsert(vectors)

Batch insert/update vectors.

```python
vectors = [
    {"key": "doc-1", "vector": [0.1, 0.2, ...], "metadata": {"title": "A"}},
    {"key": "doc-2", "vector": [0.3, 0.4, ...]},
]
versions = coll.batch_upsert(vectors)
```

**Returns:** `list[int]` - Version numbers

### coll.stats()

Get detailed collection statistics.

```python
stats = coll.stats()
print(f"Count: {stats['count']}, Memory: {stats['memory_bytes']} bytes")
```

**Returns:** `dict` with `name`, `dimension`, `metric`, `count`, `index_type`, `memory_bytes`

### len(coll)

Get vector count via `__len__`.

```python
count = len(coll)
```

---

## Branches

### Properties and Methods

```python
db.branch                     # Current branch name (property)
db.checkout("feature")        # Switch to a branch
db.fork("experiment-copy")    # Fork current branch with all data
db.merge("feature")           # Merge into current branch
db.diff("default", "feature") # Compare two branches
```

### db.branch

Current branch name (read-only property).

```python
print(db.branch)  # "default"
```

### db.checkout(name)

Switch to a different branch.

```python
db.checkout("feature")
```

### db.fork(destination)

Fork the current branch with all its data.

```python
result = db.fork("experiment-copy")
print(f"Copied {result['keys_copied']} keys")
```

**Returns:** `dict` with `source`, `destination`, `keys_copied`

### db.merge(source, *, strategy="last_writer_wins")

Merge a branch into the current branch.

```python
result = db.merge("feature")
result = db.merge("feature", strategy="strict")
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `source` | str | Source branch |
| `strategy` | str | `"last_writer_wins"` (default) or `"strict"` |

**Returns:** `dict` with `keys_applied`, `spaces_merged`, `conflicts`

### db.diff(branch_a, branch_b)

Compare two branches.

```python
diff = db.diff("default", "feature")
print(f"Added: {diff['summary']['total_added']}")
```

**Returns:** `dict` with diff details

### db.on_branch(name)

Context manager to temporarily switch branch (restores on exit).

```python
with db.on_branch("experiment"):
    db.kv.put("temp", "value")
# Back on original branch
```

### Branch Management Namespace (`db.branches`)

```python
db.branches.create("experiment")
db.branches.delete("experiment")
db.branches.exists("feature")        # bool
db.branches.get("default")           # dict or None
db.branches.list()                    # list[str]
"feature" in db.branches             # __contains__
list(db.branches)                     # __iter__

# Bundle operations
db.branches.export_bundle("default", "/tmp/backup.bundle")
db.branches.import_bundle("/tmp/backup.bundle")
db.branches.validate_bundle("/tmp/backup.bundle")
```

---

## Spaces

### Properties and Methods

```python
db.space                      # Current space name (property)
db.use_space("conversations") # Switch to a space
```

### db.space

Current space name (read-only property).

```python
print(db.space)  # "default"
```

### db.use_space(name)

Switch to a different space.

```python
db.use_space("conversations")
```

### db.in_space(name)

Context manager to temporarily switch space (restores on exit).

```python
with db.in_space("tenant_42"):
    db.kv.put("key", "value")
# Back in original space
```

### Space Management Namespace (`db.spaces`)

```python
db.spaces.create("archive")
db.spaces.delete("old-space")              # delete empty space
db.spaces.delete("old-space", force=True)  # delete with all data
db.spaces.exists("archive")               # bool
db.spaces.list()                           # list[str]
"archive" in db.spaces                    # __contains__
list(db.spaces)                            # __iter__
```

---

## Transactions

### db.transaction(read_only=False)

Get a transaction context manager.

```python
# Read-write transaction
with db.transaction():
    db.kv.put("a", 1)
    db.kv.put("b", 2)
# Auto-commits on success, auto-rollbacks on exception

# Read-only transaction
with db.transaction(read_only=True):
    a = db.kv.get("a")
    b = db.kv.get("b")
```

### db.in_transaction

Whether a transaction is currently active (property).

```python
assert db.in_transaction is False
with db.transaction():
    assert db.in_transaction is True
```

### Manual Transaction Control

```python
db.begin()
try:
    db.kv.put("a", 1)
    db.commit()
except Exception:
    db.rollback()
    raise
```

- `begin(read_only=None)` - Begin a transaction
- `commit()` - Commit (returns version `int`)
- `rollback()` - Rollback
- `txn_info()` - Get transaction info (`dict` with `id`, `status`, `started_at`, or `None`)
- `txn_is_active()` - Check if active (`bool`)

---

## Time-Travel

### db.at(timestamp)

Create a read-only snapshot at a point in time. All reads on the snapshot
are automatically scoped to that timestamp.

```python
from datetime import datetime

# From a datetime
snapshot = db.at(datetime(2024, 6, 15, 12, 0))

# From microseconds since epoch
snapshot = db.at(1700002000)

# Read via snapshot namespaces (read-only)
snapshot.kv.get("user:123")
snapshot.kv.get("missing", default="fallback")
snapshot.kv.keys(prefix="user:")
snapshot.kv.list(prefix="user:", limit=100)

snapshot.state.get("counter")
snapshot.state.list(prefix="config:")

snapshot.events.get(0)
snapshot.events.list("click", limit=10)

snapshot.json.get("config")
snapshot.json.list(prefix="user:")

snap_coll = snapshot.vectors.collection("embeddings")
snap_coll.get("doc-1")
snap_coll.search(query_vector, k=10)
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `timestamp` | datetime or int | Point in time (`datetime` or microseconds since epoch) |

**Returns:** `Snapshot` - Read-only view with `kv`, `state`, `events`, `json`, `vectors` namespaces

### db.time_range

Get the available time-travel window for the current branch (property).

```python
result = db.time_range
if result["oldest_ts"] is not None:
    print(f"Data from {result['oldest_ts']} to {result['latest_ts']}")
```

**Returns:** `dict` with `oldest_ts` and `latest_ts` (microseconds, or `None` if no data)

---

## Search

### db.search(query, *, k=None, primitives=None, time_range=None, mode=None, expand=None, rerank=None)

Search across multiple primitives with optional time filtering, query expansion, and reranking.

```python
# Basic search
results = db.search("hello world", k=10, primitives=["kv", "json"])
for hit in results:
    print(f"{hit['entity']} ({hit['primitive']}): {hit['score']}")

# Time-scoped search
results = db.search(
    "deployment failures",
    k=10,
    time_range={
        "start": "2026-02-07T00:00:00Z",
        "end": "2026-02-09T23:59:59Z",
    },
)

# Keyword-only mode, disable expansion
results = db.search("auth login", mode="keyword", expand=False)

# Force reranking on
results = db.search("database issues", rerank=True)
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `query` | str | Search query |
| `k` | int, optional | Maximum results (default: 10) |
| `primitives` | list[str], optional | Primitives to search (e.g., `["kv", "json"]`) |
| `time_range` | dict, optional | `{"start": "ISO8601", "end": "ISO8601"}` time filter |
| `mode` | str, optional | `"hybrid"` (default) or `"keyword"` |
| `expand` | bool, optional | Enable query expansion (default: auto) |
| `rerank` | bool, optional | Enable result reranking (default: auto) |

**Returns:** `list[dict]` with `entity`, `primitive`, `score`, `rank`, `snippet`

When `expand` or `rerank` are not specified, they are automatically enabled if a model is configured via `configure_model`. Set to `False` to force off, or `True` to force on (silently skipped if no model).

---

## Configuration

### db.config()

Get the current database configuration as a snapshot.

```python
cfg = db.config()
print(cfg["durability"])    # "standard"
print(cfg["auto_embed"])    # False
if cfg["model"]:
    print(cfg["model"]["endpoint"])
```

**Returns:** `dict` with `"durability"` (str), `"auto_embed"` (bool), `"model"` (dict or `None`). Model dict has `"endpoint"`, `"model"`, `"api_key"`, `"timeout_ms"`.

### db.auto_embed_enabled

Whether automatic text embedding is currently enabled (read-only property).

```python
if db.auto_embed_enabled:
    print("Auto-embedding is on")
```

**Returns:** `bool`

### db.configure_model(endpoint, model, api_key=None, timeout_ms=None)

Configure an inference model endpoint for query expansion and reranking. Persisted to `strata.toml`.

```python
db.configure_model(
    "http://localhost:11434/v1",
    "qwen3:1.7b",
    api_key="optional-token",
    timeout_ms=5000,
)
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `endpoint` | str | OpenAI-compatible API endpoint URL |
| `model` | str | Model name |
| `api_key` | str, optional | Bearer token |
| `timeout_ms` | int, optional | Request timeout in milliseconds (default: 5000) |

### db.set_auto_embed(enabled)

Enable or disable automatic text embedding. Persisted to `strata.toml`.

```python
db.set_auto_embed(True)
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `enabled` | bool | Whether to enable auto-embed |

**Raises:** `IoError` if config cannot be written

---

## Database Operations

### db.ping()

Check database connectivity.

```python
version = db.ping()
```

**Returns:** `str` - Version string

### db.info()

Get database information.

```python
info = db.info()
print(f"Version: {info['version']}")
print(f"Total keys: {info['total_keys']}")
```

**Returns:** `dict` with `version`, `uptime_secs`, `branch_count`, `total_keys`

### db.flush()

Flush pending writes to disk.

### db.compact()

Trigger database compaction.

### db.retention_apply()

Apply retention policy (trigger garbage collection).

---

## Bundle Operations

Accessible via `db.branches`:

### branches.export_bundle(branch, path)

Export a branch to a bundle file.

```python
result = db.branches.export_bundle("default", "/tmp/backup.bundle")
print(f"Exported {result['entry_count']} entries")
```

**Returns:** `dict` with `branch_id`, `path`, `entry_count`, `bundle_size`

### branches.import_bundle(path)

Import a branch from a bundle file.

```python
result = db.branches.import_bundle("/tmp/backup.bundle")
print(f"Imported to branch {result['branch_id']}")
```

**Returns:** `dict` with `branch_id`, `transactions_applied`, `keys_written`

### branches.validate_bundle(path)

Validate a bundle file without importing.

```python
result = db.branches.validate_bundle("/tmp/backup.bundle")
print(f"Valid: {result['checksums_valid']}")
```

**Returns:** `dict` with `branch_id`, `format_version`, `entry_count`, `checksums_valid`

---

## Error Handling

StrataDB uses a structured exception hierarchy instead of generic `RuntimeError`:

```python
from stratadb import StrataError, NotFoundError, ValidationError

try:
    db.kv.get_versioned("nonexistent")
except NotFoundError as e:
    print(f"Not found: {e}")
except StrataError as e:
    print(f"Database error: {e}")
```

### Exception Hierarchy

| Exception | Description |
|-----------|-------------|
| `StrataError` | Base exception for all StrataDB errors |
| `NotFoundError` | Entity not found (key, branch, collection, etc.) |
| `ValidationError` | Invalid input or type mismatch |
| `ConflictError` | Version or concurrency conflict |
| `StateError` | Invalid state transition (e.g., duplicate branch, transaction already active) |
| `ConstraintError` | Constraint violation (dimension mismatch, limits, history trimmed) |
| `AccessDeniedError` | Access denied (e.g., write on read-only database) |
| `IoError` | I/O, serialization, or internal error |

All specific exceptions are subclasses of `StrataError`:

```python
assert issubclass(NotFoundError, StrataError)
```

---

## Type Reference

### Value Types

Python types map to StrataDB values:

| Python Type | StrataDB Type |
|-------------|---------------|
| `None` | Null |
| `bool` | Bool |
| `int` | Int |
| `float` | Float |
| `str` | String |
| `bytes` | Bytes |
| `list` | Array |
| `dict` | Object |

### NumPy Support

Vector operations accept NumPy arrays:

```python
import numpy as np

# Use np.float32 for best performance
embedding = np.random.rand(384).astype(np.float32)
coll.upsert("key", embedding)

# Retrieved embeddings are NumPy arrays
result = coll.get("key")
embedding = result["embedding"]  # np.ndarray
```
