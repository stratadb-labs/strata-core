# MCP Server Reference

The StrataDB MCP (Model Context Protocol) server exposes StrataDB as a tool provider for AI assistants like Claude.

## Installation

```bash
# From crates.io
cargo install strata-mcp

# From source
git clone https://github.com/stratadb-labs/strata-core
cd strata-core
cargo install --path crates/mcp
```

## Configuration

### Claude Desktop

Add to your Claude Desktop configuration (`~/Library/Application Support/Claude/claude_desktop_config.json` on macOS):

```json
{
  "mcpServers": {
    "stratadb": {
      "command": "strata-mcp",
      "args": ["/path/to/data"]
    }
  }
}
```

### In-Memory Mode

For ephemeral databases:

```json
{
  "mcpServers": {
    "stratadb": {
      "command": "strata-mcp",
      "args": ["--memory"]
    }
  }
}
```

## Command Line Options

| Option | Description |
|--------|-------------|
| `<PATH>` | Path to the database directory |
| `--memory` | Use ephemeral in-memory database |
| `-h, --help` | Show help |
| `-V, --version` | Show version |

---

## Tools

The MCP server exposes the following tools to AI assistants:

### Database Tools

#### strata_ping

Check database connectivity.

**Parameters:** None

**Returns:**
```json
{"version": "0.5.1"}
```

#### strata_info

Get database information.

**Parameters:** None

**Returns:**
```json
{
  "version": "0.5.1",
  "uptime_secs": 3600,
  "branch_count": 3,
  "total_keys": 1500
}
```

#### strata_flush

Flush pending writes to disk.

**Parameters:** None

#### strata_compact

Trigger database compaction.

**Parameters:** None

---

### KV Store Tools

#### strata_kv_put

Store a key-value pair.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `key` | string | Yes | The key to store |
| `value` | any | Yes | The value (string, number, boolean, object, array) |

**Returns:** `{"version": 1}`

#### strata_kv_get

Get a value by key.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `key` | string | Yes | The key to retrieve |

**Returns:** The value, or `null` if not found

#### strata_kv_delete

Delete a key.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `key` | string | Yes | The key to delete |

**Returns:** `{"deleted": true}` or `{"deleted": false}`

#### strata_kv_list

List keys with optional prefix filter.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `prefix` | string | No | Filter keys by prefix |
| `limit` | number | No | Maximum keys to return |
| `cursor` | string | No | Pagination cursor |

**Returns:** `{"keys": ["key1", "key2", ...]}`

#### strata_kv_history

Get version history for a key.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `key` | string | Yes | The key |

**Returns:** Array of `{value, version, timestamp}`

---

### State Cell Tools

#### strata_state_set

Set a state cell value.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `cell` | string | Yes | The cell name |
| `value` | any | Yes | The value to set |

**Returns:** `{"version": 1}`

#### strata_state_get

Get a state cell value.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `cell` | string | Yes | The cell name |

**Returns:** The value, or `null` if not found

#### strata_state_init

Initialize a state cell only if it doesn't exist.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `cell` | string | Yes | The cell name |
| `value` | any | Yes | The initial value |

**Returns:** `{"version": 1}`

#### strata_state_cas

Compare-and-swap update.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `cell` | string | Yes | The cell name |
| `value` | any | Yes | The new value |
| `expected_version` | number | No | Expected current version |

**Returns:** `{"version": 2}` on success, `{"failed": true}` on CAS failure

#### strata_state_delete

Delete a state cell.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `cell` | string | Yes | The cell name |

**Returns:** `{"deleted": true}` or `{"deleted": false}`

#### strata_state_list

List state cell names.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `prefix` | string | No | Filter by prefix |

**Returns:** `{"cells": ["cell1", "cell2", ...]}`

---

### Event Log Tools

#### strata_event_append

Append an event to the log.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `event_type` | string | Yes | The event type |
| `payload` | object | Yes | The event payload (must be an object) |

**Returns:** `{"sequence": 0}`

#### strata_event_get

Get an event by sequence number.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `sequence` | number | Yes | The sequence number |

**Returns:** `{value, version, timestamp}` or `null`

#### strata_event_list

List events by type.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `event_type` | string | Yes | The event type |
| `limit` | number | No | Maximum events |
| `after` | number | No | Return events after this sequence |

**Returns:** Array of events

#### strata_event_len

Get total event count.

**Parameters:** None

**Returns:** `{"count": 100}`

---

### JSON Store Tools

#### strata_json_set

Set a value at a JSONPath.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `key` | string | Yes | The document key |
| `path` | string | Yes | JSONPath (use `$` for root) |
| `value` | any | Yes | The value to set |

**Returns:** `{"version": 1}`

#### strata_json_get

Get a value at a JSONPath.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `key` | string | Yes | The document key |
| `path` | string | Yes | JSONPath |

**Returns:** The value, or `null` if not found

#### strata_json_delete

Delete a value at a JSONPath.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `key` | string | Yes | The document key |
| `path` | string | Yes | JSONPath |

**Returns:** `{"deleted": 1}`

#### strata_json_list

List JSON document keys.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `prefix` | string | No | Filter by prefix |
| `limit` | number | Yes | Maximum keys |
| `cursor` | string | No | Pagination cursor |

**Returns:** `{"keys": [...], "cursor": "..."}`

---

### Vector Store Tools

#### strata_vector_create_collection

Create a vector collection.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `collection` | string | Yes | Collection name |
| `dimension` | number | Yes | Vector dimension |
| `metric` | string | No | `cosine` (default), `euclidean`, `dot_product` |

**Returns:** `{"version": 1}`

#### strata_vector_delete_collection

Delete a vector collection.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `collection` | string | Yes | Collection name |

**Returns:** `{"deleted": true}`

#### strata_vector_list_collections

List all vector collections.

**Parameters:** None

**Returns:** Array of collection info

#### strata_vector_upsert

Insert or update a vector.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `collection` | string | Yes | Collection name |
| `key` | string | Yes | Vector key |
| `vector` | number[] | Yes | Vector embedding |
| `metadata` | object | No | Optional metadata |

**Returns:** `{"version": 1}`

#### strata_vector_get

Get a vector by key.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `collection` | string | Yes | Collection name |
| `key` | string | Yes | Vector key |

**Returns:** `{key, embedding, metadata, version, timestamp}` or `null`

#### strata_vector_delete

Delete a vector.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `collection` | string | Yes | Collection name |
| `key` | string | Yes | Vector key |

**Returns:** `{"deleted": true}`

#### strata_vector_search

Search for similar vectors.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `collection` | string | Yes | Collection name |
| `query` | number[] | Yes | Query vector |
| `k` | number | Yes | Number of results |
| `metric` | string | No | Override distance metric |
| `filter` | object[] | No | Metadata filters |

**Filter format:**
```json
[
  {"field": "category", "op": "eq", "value": "science"},
  {"field": "year", "op": "gte", "value": 2020}
]
```

**Filter operators:** `eq`, `ne`, `gt`, `gte`, `lt`, `lte`, `in`, `contains`

**Returns:** Array of `{key, score, metadata}`

#### strata_vector_batch_upsert

Batch insert/update vectors.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `collection` | string | Yes | Collection name |
| `entries` | object[] | Yes | Array of `{key, vector, metadata?}` |

**Returns:** `{"versions": [1, 2, 3, ...]}`

---

### Branch Tools

#### strata_branch_create

Create a new empty branch.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `name` | string | Yes | Branch name |

#### strata_branch_get

Get branch metadata.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `name` | string | Yes | Branch name |

**Returns:** Branch info or `null`

#### strata_branch_list

List all branches.

**Parameters:** None

**Returns:** `{"branches": ["default", "feature", ...]}`

#### strata_branch_exists

Check if a branch exists.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `name` | string | Yes | Branch name |

**Returns:** `{"exists": true}`

#### strata_branch_delete

Delete a branch.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `name` | string | Yes | Branch name |

#### strata_branch_fork

Fork a branch with all its data.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `source` | string | Yes | Source branch |
| `destination` | string | Yes | New branch name |

**Returns:** `{"source", "destination", "keys_copied"}`

#### strata_branch_diff

Compare two branches.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `branch_a` | string | Yes | First branch |
| `branch_b` | string | Yes | Second branch |

**Returns:** Diff summary

#### strata_branch_merge

Merge branches.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `source` | string | Yes | Source branch |
| `target` | string | Yes | Target branch |
| `strategy` | string | No | `last_writer_wins` (default) or `strict` |

**Returns:** Merge result with conflicts

#### strata_branch_use

Switch current branch context.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `name` | string | Yes | Branch name |

#### strata_current_branch

Get current branch name.

**Parameters:** None

**Returns:** `{"branch": "default"}`

---

### Space Tools

#### strata_space_create

Create a new space.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `name` | string | Yes | Space name |

#### strata_space_list

List all spaces.

**Parameters:** None

**Returns:** `{"spaces": ["default", ...]}`

#### strata_space_exists

Check if a space exists.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `name` | string | Yes | Space name |

**Returns:** `{"exists": true}`

#### strata_space_delete

Delete an empty space.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `name` | string | Yes | Space name |

#### strata_space_use

Switch current space context.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `name` | string | Yes | Space name |

#### strata_current_space

Get current space name.

**Parameters:** None

**Returns:** `{"space": "default"}`

---

### Transaction Tools

#### strata_txn_begin

Begin a new transaction.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `read_only` | boolean | No | Read-only transaction |

#### strata_txn_commit

Commit the current transaction.

**Parameters:** None

**Returns:** `{"version": 5}`

#### strata_txn_rollback

Rollback the current transaction.

**Parameters:** None

#### strata_txn_info

Get current transaction info.

**Parameters:** None

**Returns:** `{id, status, started_at}` or `null`

#### strata_txn_is_active

Check if a transaction is active.

**Parameters:** None

**Returns:** `{"active": true}`

---

### Search Tools

#### strata_search

Search across multiple primitives.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `query` | string | Yes | Search query |
| `k` | number | No | Maximum results |
| `primitives` | string[] | No | Primitives to search |

**Returns:** Array of `{entity, primitive, score, rank, snippet}`

---

### Bundle Tools

#### strata_branch_export

Export a branch to a bundle file.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `branch` | string | Yes | Branch name |
| `path` | string | Yes | Output file path |

**Returns:** Export result

#### strata_branch_import

Import a branch from a bundle file.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `path` | string | Yes | Bundle file path |

**Returns:** Import result

#### strata_branch_validate

Validate a bundle file.

**Parameters:**
| Name | Type | Required | Description |
|------|------|----------|-------------|
| `path` | string | Yes | Bundle file path |

**Returns:** Validation result

---

## Error Handling

Tool errors are returned in the standard MCP error format:

```json
{
  "error": {
    "code": -32000,
    "message": "Branch not found: nonexistent"
  }
}
```

Common error codes:
- `-32000`: Application error (invalid operation, not found, etc.)
- `-32602`: Invalid parameters
- `-32603`: Internal error

---

## Usage with Claude

Once configured, Claude can use StrataDB tools naturally:

**User:** "Store my name as Alice"

**Claude:** I'll store that for you.
*[Calls strata_kv_put with key="name", value="Alice"]*

Done! I've stored your name as "Alice" in the database.

**User:** "Create a branch called 'experiment' and switch to it"

**Claude:** I'll create that branch and switch to it.
*[Calls strata_branch_create with name="experiment"]*
*[Calls strata_branch_use with name="experiment"]*

Done! Created the "experiment" branch and switched to it. Any data operations will now happen on this branch.
