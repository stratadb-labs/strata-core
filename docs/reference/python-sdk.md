# Python SDK Reference

The StrataDB Python SDK provides native Python bindings via PyO3.

## Installation

```bash
pip install stratadb
```

## Quick Start

```python
from stratadb import Strata

# Open a database
db = Strata.open("/path/to/data")

# Store and retrieve data
db.kv_put("greeting", "Hello, World!")
print(db.kv_get("greeting"))  # "Hello, World!"

# Use transactions
with db.transaction():
    db.kv_put("a", 1)
    db.kv_put("b", 2)
# Auto-commits on success, auto-rollbacks on exception

# Use vector search with NumPy
import numpy as np
embedding = np.random.rand(384).astype(np.float32)
db.vector_create_collection("docs", 384)
db.vector_upsert("docs", "doc-1", embedding)
results = db.vector_search("docs", embedding, k=5)
```

---

## Opening a Database

### Strata.open(path)

Open a database at the given path.

```python
db = Strata.open("/path/to/data")
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `path` | str | Path to the database directory |

**Returns:** `Strata` instance

**Raises:** `RuntimeError` if the database cannot be opened

### Strata.cache()

Create an ephemeral in-memory database.

```python
db = Strata.cache()
```

**Returns:** `Strata` instance

---

## KV Store

### kv_put(key, value)

Store a key-value pair.

```python
version = db.kv_put("user:123", {"name": "Alice", "age": 30})
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `key` | str | The key |
| `value` | any | The value (str, int, float, bool, list, dict, bytes, None) |

**Returns:** `int` - Version number

### kv_get(key, as_of=None)

Get a value by key.

```python
value = db.kv_get("user:123")
if value is not None:
    print(value["name"])

# Time-travel: read historical value
historical = db.kv_get("user:123", as_of=1700002000)
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `key` | str | The key |
| `as_of` | int, optional | Timestamp (microseconds since epoch) for time-travel read |

**Returns:** Value or `None` if not found

### kv_delete(key)

Delete a key.

```python
deleted = db.kv_delete("user:123")
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `key` | str | The key |

**Returns:** `bool` - True if the key existed

### kv_list(prefix=None)

List keys with optional prefix filter.

```python
all_keys = db.kv_list()
user_keys = db.kv_list(prefix="user:")
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `prefix` | str, optional | Filter keys by prefix |

**Returns:** `list[str]` - Key names

### kv_history(key)

Get version history for a key.

```python
history = db.kv_history("user:123")
for entry in history:
    print(f"v{entry['version']}: {entry['value']}")
```

**Returns:** `list[dict]` with `value`, `version`, `timestamp`, or `None`

### kv_get_versioned(key)

Get a value with version info.

```python
result = db.kv_get_versioned("user:123")
if result:
    print(f"Value: {result['value']}, Version: {result['version']}")
```

**Returns:** `dict` with `value`, `version`, `timestamp`, or `None`

### kv_list_paginated(prefix=None, limit=None, cursor=None)

List keys with pagination.

```python
result = db.kv_list_paginated(prefix="user:", limit=100)
print(result["keys"])
```

**Returns:** `dict` with `keys` list

---

## State Cell

### state_set(cell, value)

Set a state cell value.

```python
version = db.state_set("counter", 0)
```

**Returns:** `int` - Version number

### state_get(cell, as_of=None)

Get a state cell value.

```python
value = db.state_get("counter")

# Time-travel: read historical value
historical = db.state_get("counter", as_of=1700002000)
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `cell` | str | The cell name |
| `as_of` | int, optional | Timestamp (microseconds since epoch) for time-travel read |

**Returns:** Value or `None`

### state_init(cell, value)

Initialize a state cell only if it doesn't exist.

```python
version = db.state_init("counter", 0)
```

**Returns:** `int` - Version number

### state_cas(cell, new_value, expected_version=None)

Compare-and-swap update.

```python
# Only update if version is 5
new_version = db.state_cas("counter", 10, expected_version=5)
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

### state_delete(cell)

Delete a state cell.

```python
deleted = db.state_delete("counter")
```

**Returns:** `bool` - True if the cell existed

### state_list(prefix=None)

List state cell names.

```python
cells = db.state_list()
cells = db.state_list(prefix="config:")
```

**Returns:** `list[str]`

### state_history(cell)

Get version history for a state cell.

```python
history = db.state_history("counter")
```

**Returns:** `list[dict]` or `None`

### state_get_versioned(cell)

Get a state cell with version info.

```python
result = db.state_get_versioned("counter")
```

**Returns:** `dict` with `value`, `version`, `timestamp`, or `None`

---

## Event Log

### event_append(event_type, payload)

Append an event to the log.

```python
seq = db.event_append("user_action", {"action": "click", "target": "button"})
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `event_type` | str | The event type |
| `payload` | any | The event payload |

**Returns:** `int` - Sequence number

### event_get(sequence)

Get an event by sequence number.

```python
event = db.event_get(0)
if event:
    print(event["value"])
```

**Returns:** `dict` with `value`, `version`, `timestamp`, or `None`

### event_list(event_type)

List events by type.

```python
events = db.event_list("user_action")
for event in events:
    print(event["value"])
```

**Returns:** `list[dict]`

### event_len()

Get total event count.

```python
count = db.event_len()
```

**Returns:** `int`

### event_list_paginated(event_type, limit=None, after=None)

List events with pagination.

```python
events = db.event_list_paginated("user_action", limit=100, after=500)
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `event_type` | str | The event type |
| `limit` | int, optional | Maximum events |
| `after` | int, optional | Return events after this sequence |

**Returns:** `list[dict]`

---

## JSON Store

### json_set(key, path, value)

Set a value at a JSONPath.

```python
# Set entire document
db.json_set("user:123", "$", {"name": "Alice", "age": 30})

# Set nested field
db.json_set("user:123", "$.email", "alice@example.com")
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `key` | str | Document key |
| `path` | str | JSONPath (use `$` for root) |
| `value` | any | The value |

**Returns:** `int` - Version number

### json_get(key, path, as_of=None)

Get a value at a JSONPath.

```python
doc = db.json_get("user:123", "$")
name = db.json_get("user:123", "$.name")

# Time-travel: read historical document
historical = db.json_get("config", "$", as_of=1700002000)
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `key` | str | Document key |
| `path` | str | JSONPath (use `$` for root) |
| `as_of` | int, optional | Timestamp (microseconds since epoch) for time-travel read |

**Returns:** Value or `None`

### json_delete(key, path)

Delete a value at a JSONPath.

```python
deleted_count = db.json_delete("user:123", "$.email")
```

**Returns:** `int` - Count of elements deleted

### json_list(limit, prefix=None, cursor=None)

List JSON document keys with pagination.

```python
result = db.json_list(100, prefix="user:")
keys = result["keys"]
next_cursor = result.get("cursor")
```

**Returns:** `dict` with `keys` and optional `cursor`

### json_history(key)

Get version history for a JSON document.

```python
history = db.json_history("user:123")
```

**Returns:** `list[dict]` or `None`

### json_get_versioned(key)

Get a JSON document with version info.

```python
result = db.json_get_versioned("user:123")
```

**Returns:** `dict` with `value`, `version`, `timestamp`, or `None`

---

## Vector Store

### vector_create_collection(collection, dimension, metric=None)

Create a vector collection.

```python
db.vector_create_collection("embeddings", 384)
db.vector_create_collection("images", 512, metric="euclidean")
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `collection` | str | Collection name |
| `dimension` | int | Vector dimension |
| `metric` | str, optional | `cosine` (default), `euclidean`, `dot_product` |

**Returns:** `int` - Version number

### vector_delete_collection(collection)

Delete a vector collection.

```python
deleted = db.vector_delete_collection("embeddings")
```

**Returns:** `bool`

### vector_list_collections()

List all vector collections.

```python
collections = db.vector_list_collections()
for c in collections:
    print(f"{c['name']}: {c['count']} vectors")
```

**Returns:** `list[dict]` with `name`, `dimension`, `metric`, `count`, `index_type`, `memory_bytes`

### vector_upsert(collection, key, vector, metadata=None)

Insert or update a vector.

```python
import numpy as np

embedding = np.random.rand(384).astype(np.float32)
db.vector_upsert("embeddings", "doc-1", embedding, {"title": "Hello"})

# Also accepts lists
db.vector_upsert("embeddings", "doc-2", [0.1, 0.2, 0.3, ...])
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `collection` | str | Collection name |
| `key` | str | Vector key |
| `vector` | ndarray or list | Vector embedding |
| `metadata` | dict, optional | Metadata |

**Returns:** `int` - Version number

### vector_get(collection, key)

Get a vector by key.

```python
result = db.vector_get("embeddings", "doc-1")
if result:
    print(result["embedding"])  # NumPy array
    print(result["metadata"])
```

**Returns:** `dict` with `key`, `embedding`, `metadata`, `version`, `timestamp`, or `None`

### vector_delete(collection, key)

Delete a vector.

```python
deleted = db.vector_delete("embeddings", "doc-1")
```

**Returns:** `bool`

### vector_search(collection, query, k, as_of=None)

Search for similar vectors.

```python
query = np.random.rand(384).astype(np.float32)
matches = db.vector_search("embeddings", query, k=10)
for match in matches:
    print(f"{match['key']}: {match['score']}")

# Time-travel: search as of a past timestamp
historical = db.vector_search("embeddings", query, k=10, as_of=1700002000)
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `collection` | str | Collection name |
| `query` | ndarray or list | Query vector |
| `k` | int | Number of results |
| `as_of` | int, optional | Timestamp (microseconds since epoch) for temporal search |

**Returns:** `list[dict]` with `key`, `score`, `metadata`

### vector_search_filtered(collection, query, k, metric=None, filter=None)

Search with filter and metric override.

```python
matches = db.vector_search_filtered(
    "embeddings",
    query,
    k=10,
    metric="euclidean",
    filter=[
        {"field": "category", "op": "eq", "value": "science"},
        {"field": "year", "op": "gte", "value": 2020}
    ]
)
```

**Filter operators:** `eq`, `ne`, `gt`, `gte`, `lt`, `lte`, `in`, `contains`

**Returns:** `list[dict]` with `key`, `score`, `metadata`

### vector_collection_stats(collection)

Get detailed collection statistics.

```python
stats = db.vector_collection_stats("embeddings")
print(f"Count: {stats['count']}, Memory: {stats['memory_bytes']} bytes")
```

**Returns:** `dict`

### vector_batch_upsert(collection, vectors)

Batch insert/update vectors.

```python
vectors = [
    {"key": "doc-1", "vector": [0.1, 0.2, ...], "metadata": {"title": "A"}},
    {"key": "doc-2", "vector": [0.3, 0.4, ...]},
]
versions = db.vector_batch_upsert("embeddings", vectors)
```

**Returns:** `list[int]` - Version numbers

---

## Branches

### current_branch()

Get the current branch name.

```python
branch = db.current_branch()
```

**Returns:** `str`

### set_branch(branch)

Switch to a different branch.

```python
db.set_branch("feature")
```

### create_branch(branch)

Create a new empty branch.

```python
db.create_branch("experiment")
```

### list_branches()

List all branches.

```python
branches = db.list_branches()
```

**Returns:** `list[str]`

### delete_branch(branch)

Delete a branch.

```python
db.delete_branch("experiment")
```

### branch_exists(name)

Check if a branch exists.

```python
exists = db.branch_exists("feature")
```

**Returns:** `bool`

### branch_get(name)

Get branch metadata.

```python
info = db.branch_get("default")
if info:
    print(f"Created: {info['created_at']}, Version: {info['version']}")
```

**Returns:** `dict` or `None`

### fork_branch(destination)

Fork the current branch with all its data.

```python
result = db.fork_branch("experiment-copy")
print(f"Copied {result['keys_copied']} keys")
```

**Returns:** `dict` with `source`, `destination`, `keys_copied`

### diff_branches(branch_a, branch_b)

Compare two branches.

```python
diff = db.diff_branches("default", "feature")
print(f"Added: {diff['summary']['total_added']}")
```

**Returns:** `dict` with diff details

### merge_branches(source, strategy=None)

Merge a branch into the current branch.

```python
result = db.merge_branches("feature", strategy="last_writer_wins")
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `source` | str | Source branch |
| `strategy` | str, optional | `last_writer_wins` (default) or `strict` |

**Returns:** `dict` with merge results

---

## Spaces

### current_space()

Get the current space name.

```python
space = db.current_space()
```

**Returns:** `str`

### set_space(space)

Switch to a different space.

```python
db.set_space("conversations")
```

### list_spaces()

List all spaces.

```python
spaces = db.list_spaces()
```

**Returns:** `list[str]`

### delete_space(space)

Delete an empty space.

```python
db.delete_space("old-space")
```

### delete_space_force(space)

Delete a space and all its data.

```python
db.delete_space_force("old-space")
```

### space_create(space)

Create a new space explicitly.

```python
db.space_create("archive")
```

### space_exists(space)

Check if a space exists.

```python
exists = db.space_exists("archive")
```

**Returns:** `bool`

---

## Transactions

### transaction(read_only=False)

Get a transaction context manager.

```python
# Read-write transaction
with db.transaction():
    db.kv_put("a", 1)
    db.kv_put("b", 2)
# Auto-commits on success

# Read-only transaction
with db.transaction(read_only=True):
    a = db.kv_get("a")
    b = db.kv_get("b")
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `read_only` | bool | Read-only transaction |

### begin(read_only=None)

Begin a transaction manually.

```python
db.begin()
try:
    db.kv_put("a", 1)
    db.commit()
except Exception:
    db.rollback()
    raise
```

### commit()

Commit the current transaction.

```python
version = db.commit()
```

**Returns:** `int` - Commit version

### rollback()

Rollback the current transaction.

```python
db.rollback()
```

### txn_info()

Get current transaction info.

```python
info = db.txn_info()
if info:
    print(f"Transaction {info['id']} is {info['status']}")
```

**Returns:** `dict` with `id`, `status`, `started_at`, or `None`

### txn_is_active()

Check if a transaction is active.

```python
active = db.txn_is_active()
```

**Returns:** `bool`

---

## Search

### search(query, k=None, primitives=None, time_range=None, mode=None, expand=None, rerank=None)

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

## Database Operations

### ping()

Check database connectivity.

```python
version = db.ping()
```

**Returns:** `str` - Version string

### info()

Get database information.

```python
info = db.info()
print(f"Version: {info['version']}")
print(f"Total keys: {info['total_keys']}")
```

**Returns:** `dict` with `version`, `uptime_secs`, `branch_count`, `total_keys`

### flush()

Flush pending writes to disk.

```python
db.flush()
```

### compact()

Trigger database compaction.

```python
db.compact()
```

### time_range(branch=None)

Get the available time-travel window for a branch.

```python
result = db.time_range()
if result:
    print(f"Data from {result['oldest_ts']} to {result['latest_ts']}")
```

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| `branch` | str, optional | Branch name (defaults to current branch) |

**Returns:** `dict` with `oldest_ts` and `latest_ts`, or `None` if branch has no data

---

## Bundle Operations

### branch_export(branch, path)

Export a branch to a bundle file.

```python
result = db.branch_export("default", "/tmp/backup.bundle")
print(f"Exported {result['entry_count']} entries")
```

**Returns:** `dict` with export details

### branch_import(path)

Import a branch from a bundle file.

```python
result = db.branch_import("/tmp/backup.bundle")
print(f"Imported to branch {result['branch_id']}")
```

**Returns:** `dict` with import details

### branch_validate_bundle(path)

Validate a bundle file without importing.

```python
result = db.branch_validate_bundle("/tmp/backup.bundle")
print(f"Valid: {result['checksums_valid']}")
```

**Returns:** `dict` with validation details

---

## Error Handling

All methods may raise `RuntimeError` for database errors:

```python
try:
    db.set_branch("nonexistent")
except RuntimeError as e:
    print(f"Error: {e}")
```

Common errors:
- Branch not found
- Collection not found
- CAS version mismatch
- Transaction already active
- Invalid input

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
db.vector_upsert("coll", "key", embedding)

# Retrieved embeddings are NumPy arrays
result = db.vector_get("coll", "key")
embedding = result["embedding"]  # np.ndarray
```
