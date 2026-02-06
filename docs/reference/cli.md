# CLI Reference

The StrataDB CLI (`strata`) provides a Redis-inspired command-line interface for interacting with StrataDB databases.

## Installation

```bash
# From crates.io
cargo install strata-cli

# From source
git clone https://github.com/stratadb-labs/strata-core
cd strata-core
cargo install --path crates/cli
```

## Quick Start

```bash
# Open a database
strata /path/to/data

# Or use an in-memory database
strata --memory

# Run a single command
strata /path/to/data -c "kv put greeting hello"
```

## Command Line Options

| Option | Description |
|--------|-------------|
| `<PATH>` | Path to the database directory |
| `--memory` | Use ephemeral in-memory database |
| `-c, --command <CMD>` | Execute command and exit |
| `--json` | Output in JSON format |
| `--raw` | Output raw values (no formatting) |
| `-h, --help` | Show help |
| `-V, --version` | Show version |

---

## Database Commands

### ping

Check database connectivity.

```
ping
```

**Returns:** `PONG` with version string

### info

Get database information.

```
info
```

**Returns:** Database statistics including version, uptime, branch count, and total keys.

### flush

Flush pending writes to disk.

```
flush
```

### compact

Trigger database compaction.

```
compact
```

---

## KV Store Commands

### kv put

Store one or more key-value pairs.

```
kv put <key> <value> [<key> <value> ...]
```

**Examples:**
```bash
kv put name "Alice"
kv put counter 42
kv put config '{"debug": true}'
kv put a 1 b 2 c 3  # Multiple pairs
```

**Returns:** Version number(s)

### kv get

Get one or more values by key.

```
kv get <key> [<key> ...]
kv get <key> --with-version
```

**Options:**
| Option | Description |
|--------|-------------|
| `--with-version`, `-v` | Include version and timestamp |

**Examples:**
```bash
kv get name
kv get a b c
kv get config --with-version
```

**Returns:** Value(s) or `(nil)` if not found

### kv del

Delete one or more keys.

```
kv del <key> [<key> ...]
```

**Examples:**
```bash
kv del name
kv del a b c
```

**Returns:** `(integer) 1` if deleted, `(integer) 0` if not found

### kv list

List keys with optional prefix filter.

```
kv list [--prefix <prefix>] [--limit <n>] [--cursor <cursor>] [--all]
```

**Options:**
| Option | Description |
|--------|-------------|
| `--prefix`, `-p` | Filter keys by prefix |
| `--limit`, `-n` | Maximum keys to return |
| `--cursor`, `-c` | Pagination cursor |
| `--all`, `-a` | Fetch all keys (auto-pagination) |

**Examples:**
```bash
kv list
kv list --prefix "user:"
kv list --prefix "session:" --limit 100
kv list --all
```

### kv history

Get version history for a key.

```
kv history <key>
```

**Returns:** Array of versioned values with timestamps

---

## State Cell Commands

### state set

Set a state cell value.

```
state set <cell> <value>
```

**Examples:**
```bash
state set counter 0
state set config '{"mode": "production"}'
```

**Returns:** Version number

### state get

Get a state cell value.

```
state get <cell>
state get <cell> --with-version
```

**Returns:** Value or `(nil)` if not found

### state init

Initialize a state cell only if it doesn't exist.

```
state init <cell> <value>
```

**Returns:** Version number (same version if already exists)

### state cas

Compare-and-swap update based on version.

```
state cas <cell> <new_value> [--expect <version>]
```

**Options:**
| Option | Description |
|--------|-------------|
| `--expect`, `-e` | Expected current version (omit for "must not exist") |

**Examples:**
```bash
state cas counter 1 --expect 0
state cas lock "acquired"  # Only if doesn't exist
```

**Returns:** New version if successful, error if CAS failed

### state del

Delete a state cell.

```
state del <cell>
```

**Returns:** `(integer) 1` if deleted, `(integer) 0` if not found

### state list

List state cell names.

```
state list [--prefix <prefix>]
```

### state history

Get version history for a state cell.

```
state history <cell>
```

---

## Event Log Commands

### event append

Append an event to the log.

```
event append <type> <payload>
event append <type> --file <path>
```

**Options:**
| Option | Description |
|--------|-------------|
| `--file`, `-f` | Read payload from JSON file (use `-` for stdin) |

**Examples:**
```bash
event append user_action '{"action": "click", "target": "button"}'
event append order_placed --file order.json
echo '{"data": 123}' | event append sensor_reading -f -
```

**Returns:** Sequence number

### event get

Get an event by sequence number.

```
event get <sequence>
```

**Returns:** Event with type, payload, and timestamp

### event list

List events by type.

```
event list <type> [--limit <n>] [--after <seq>]
```

**Options:**
| Option | Description |
|--------|-------------|
| `--limit`, `-n` | Maximum events to return |
| `--after`, `-a` | Return events after this sequence number |

**Examples:**
```bash
event list user_action
event list sensor_reading --limit 100 --after 500
```

### event len

Get total event count.

```
event len
```

**Returns:** Number of events in the log

---

## JSON Store Commands

### json set

Set a value at a JSONPath.

```
json set <key> <path> <value>
json set <key> <path> --file <path>
```

**Examples:**
```bash
json set user:123 $ '{"name": "Alice", "age": 30}'
json set user:123 $.email "alice@example.com"
json set config $ --file config.json
```

**Returns:** Version number

### json get

Get a value at a JSONPath.

```
json get <key> <path>
json get <key> <path> --with-version
```

**Examples:**
```bash
json get user:123 $
json get user:123 $.name
json get user:123 $.address.city
```

### json del

Delete a value at a JSONPath.

```
json del <key> <path>
```

**Returns:** Count of elements removed

### json list

List JSON document keys.

```
json list [--prefix <prefix>] [--limit <n>] [--cursor <cursor>]
```

### json history

Get version history for a JSON document.

```
json history <key>
```

---

## Vector Store Commands

### vector create

Create a vector collection.

```
vector create <collection> <dimension> [--metric <metric>]
```

**Options:**
| Option | Description |
|--------|-------------|
| `--metric`, `-m` | Distance metric: `cosine` (default), `euclidean`, `dot_product` |

**Examples:**
```bash
vector create embeddings 384
vector create images 512 --metric euclidean
```

### vector drop

Delete a vector collection.

```
vector drop <collection>
```

Alias: `vector del-collection`

### vector list

List all vector collections.

```
vector list
```

**Returns:** Collection info including dimension, metric, count, memory usage

### vector stats

Get detailed statistics for a collection.

```
vector stats <collection>
```

### vector upsert

Insert or update a vector.

```
vector upsert <collection> <key> <vector> [--metadata <json>]
```

**Examples:**
```bash
vector upsert embeddings doc-1 "[0.1, 0.2, 0.3, ...]"
vector upsert embeddings doc-2 "[...]" --metadata '{"title": "Hello"}'
```

### vector get

Get a vector by key.

```
vector get <collection> <key>
```

**Returns:** Vector embedding, metadata, version, timestamp

### vector del

Delete a vector.

```
vector del <collection> <key>
```

### vector search

Search for similar vectors.

```
vector search <collection> <query> <k> [--metric <metric>] [--filter <json>]
```

**Options:**
| Option | Description |
|--------|-------------|
| `--metric`, `-m` | Override distance metric for this search |
| `--filter`, `-f` | Metadata filter (JSON array) |

**Filter operators:** `eq`, `ne`, `gt`, `gte`, `lt`, `lte`, `in`, `contains`

**Examples:**
```bash
vector search embeddings "[0.1, 0.2, ...]" 10
vector search embeddings "[...]" 5 --filter '[{"field": "category", "op": "eq", "value": "science"}]'
```

**Returns:** Top-k matches with key, score, and metadata

### vector batch-upsert

Batch insert/update multiple vectors.

```
vector batch-upsert <collection> --file <path>
```

**File format:** JSON array of `{key, vector, metadata?}` objects

---

## Branch Commands

### branch create

Create a new empty branch.

```
branch create <name>
```

### branch info

Get branch metadata.

```
branch info <name>
```

Alias: `branch get`

### branch list

List all branches.

```
branch list
```

### branch exists

Check if a branch exists.

```
branch exists <name>
```

**Returns:** `(integer) 1` if exists, `(integer) 0` if not

### branch del

Delete a branch.

```
branch del <name>
```

### branch fork

Fork a branch with all its data.

```
branch fork <source> <destination>
```

**Returns:** Fork info with keys copied count

### branch diff

Compare two branches.

```
branch diff <branch_a> <branch_b>
```

**Returns:** Diff summary with added, removed, modified counts

### branch merge

Merge a branch into another.

```
branch merge <source> <target> [--strategy <strategy>]
```

**Options:**
| Option | Description |
|--------|-------------|
| `--strategy`, `-s` | `last_writer_wins` (default) or `strict` |

### branch use

Switch to a different branch.

```
branch use <name>
```

Alias: `use <name>`

### branch export

Export a branch to a bundle file.

```
branch export <name> <path>
```

### branch import

Import a branch from a bundle file.

```
branch import <path>
```

### branch validate

Validate a bundle file without importing.

```
branch validate <path>
```

---

## Space Commands

### space create

Create a new space.

```
space create <name>
```

### space list

List all spaces in current branch.

```
space list
```

### space exists

Check if a space exists.

```
space exists <name>
```

### space del

Delete an empty space.

```
space del <name>
```

### space del-force

Delete a space and all its data.

```
space del-force <name>
```

### space use

Switch to a different space.

```
space use <name>
```

---

## Transaction Commands

### txn begin

Begin a new transaction.

```
txn begin [--read-only]
```

### txn commit

Commit the current transaction.

```
txn commit
```

**Returns:** Commit version number

### txn rollback

Rollback the current transaction.

```
txn rollback
```

### txn info

Get current transaction info.

```
txn info
```

### txn active

Check if a transaction is active.

```
txn active
```

---

## Search Commands

### search

Search across multiple primitives.

```
search <query> [--k <n>] [--primitives <list>]
```

**Options:**
| Option | Description |
|--------|-------------|
| `--k`, `-k` | Maximum results (default: 10) |
| `--primitives`, `-p` | Comma-separated list: `kv,json,events,state` |

**Examples:**
```bash
search "hello world"
search "error" --k 20 --primitives kv,json
```

**Returns:** Hits with entity, primitive, score, rank, snippet

---

## REPL Commands

These commands are only available in interactive mode:

| Command | Description |
|---------|-------------|
| `help [command]` | Show help for a command |
| `clear` | Clear the screen |
| `quit` / `exit` | Exit the REPL |

### Tab Completion

Press `TAB` to autocomplete:
- Command names
- Subcommand names
- Flag names
- Branch names (after `branch use`)
- Space names (after `space use`)

---

## Output Formats

### Human (default)

Redis-style output optimized for readability:

```
> kv put name "Alice"
(integer) 1

> kv get name
"Alice"

> kv list
1) "name"
```

### JSON (`--json`)

Machine-readable JSON output:

```bash
strata /data --json -c "kv get name"
# {"value": "Alice"}
```

### Raw (`--raw`)

Unformatted values only:

```bash
strata /data --raw -c "kv get name"
# Alice
```

---

## Error Handling

Errors are displayed with descriptive messages:

```
> kv get nonexistent
(nil)

> branch use nonexistent
(error) Branch not found: nonexistent

> state cas counter 10 --expect 5
(error) CAS failed: expected version 5, found 7
```

In JSON mode, errors include an `error` field:

```json
{"error": "Branch not found: nonexistent"}
```
