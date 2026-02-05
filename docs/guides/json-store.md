# JSON Store Guide

The JSON Store holds structured documents that you can read and write at specific JSON paths. Instead of replacing an entire document to change one field, you can update just `$.config.temperature`.

## Command Overview

| Command | Syntax | Returns |
|---------|--------|---------|
| `json set` | `json set <key> <path> <value>` | Version number |
| `json get` | `json get <key> [path]` | Value at path, or `(nil)` |
| `json del` | `json del <key> <path>` | OK |
| `json list` | `json list [--prefix P] [--cursor C] [--limit N]` | Keys + next cursor |
| `json history` | `json history <key>` | Version history |

## Creating Documents

Use `json set` with the root path `$` to create a document:

```
$ strata --cache
strata:default/default> json set config $ '{"model":"gpt-4","temperature":0.7,"settings":{"max_tokens":1000,"stream":true}}'
(version) 1
```

## Reading Documents

### Read the Whole Document

```
$ strata --cache
strata:default/default> json set config $ '{"model":"gpt-4","temperature":0.7}'
(version) 1
strata:default/default> json get config
{"model":"gpt-4","temperature":0.7}
```

### Read a Nested Path

```
$ strata --cache
strata:default/default> json set config $ '{"model":"gpt-4","temperature":0.7,"settings":{"max_tokens":1000}}'
(version) 1
strata:default/default> json get config $.temperature
0.7
strata:default/default> json get config $.settings.max_tokens
1000
```

### Path Syntax

Paths use a simple dot notation starting with `$`:

| Path | Selects |
|------|---------|
| `$` | The root document |
| `$.name` | Top-level field "name" |
| `$.settings.max_tokens` | Nested field |

## Updating Fields

Update a specific path without touching the rest of the document:

```
$ strata --cache
strata:default/default> json set config $ '{"model":"gpt-4","temperature":0.7,"settings":{"stream":true}}'
(version) 1
strata:default/default> json set config $.temperature 0.9
(version) 2
strata:default/default> json set config $.version 2.0
(version) 3
strata:default/default> json set config $.settings.stream false
(version) 4
```

## Deleting

### Delete a Field

```
$ strata --cache
strata:default/default> json set config $ '{"model":"gpt-4","deprecated_field":"old"}'
(version) 1
strata:default/default> json del config $.deprecated_field
OK
```

### Delete the Entire Document

```
$ strata --cache
strata:default/default> json set config $ '{"model":"gpt-4"}'
(version) 1
strata:default/default> json del config $
OK
strata:default/default> json get config
(nil)
```

## Listing Documents

List documents with optional prefix filtering and cursor-based pagination:

```
$ strata --cache
strata:default/default> json set user:1 $ '{"name":"Alice"}'
(version) 1
strata:default/default> json set user:2 $ '{"name":"Bob"}'
(version) 1
strata:default/default> json set config $ '{"debug":true}'
(version) 1
strata:default/default> json list
config
user:1
user:2
strata:default/default> json list --prefix user:
user:1
user:2
```

### Pagination

When there are more results than the limit, use the cursor to fetch the next page:

```bash
# Paginate through results
strata --cache json list --limit 10
# Use the returned cursor for the next page
strata --cache json list --limit 10 --cursor <cursor>
```

## Common Patterns

### Conversation History

```
$ strata --cache
strata:default/default> json set conversation:001 $ '{"messages":[{"role":"user","content":"Hello"},{"role":"assistant","content":"Hi there!"}],"metadata":{"model":"gpt-4","created_at":1234567890}}'
(version) 1
```

### Agent Configuration

```
$ strata --cache
strata:default/default> json set agent:001:config $ '{"agent_id":"agent-001","model":"gpt-4","tools":["search","calculator","code_interpreter"],"constraints":{"max_steps":10,"timeout_seconds":30}}'
(version) 1
strata:default/default> json set agent:001:config $.model gpt-4-turbo
(version) 2
```

## Branch Isolation

JSON documents are isolated by branch, like all primitives.

## Space Isolation

Within a branch, JSON documents are scoped to the current space:

```
$ strata --cache
strata:default/default> json set config $ '{"debug":true}'
(version) 1
strata:default/default> use default other
strata:default/other> json get config
(nil)
```

See [Spaces](spaces.md) for the full guide.

## Transactions

JSON set, get, and delete operations participate in transactions. Path-level updates to different fields of the same document can be made by concurrent transactions — sibling paths don't conflict.

See [Sessions and Transactions](sessions-and-transactions.md) for details.

## Next

- [Vector Store](vector-store.md) — embeddings and similarity search
- [Value Types](../concepts/value-types.md) — the 8-variant type system
