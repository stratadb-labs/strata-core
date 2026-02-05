# Spaces Guide

Spaces are an organizational layer within branches. Each branch contains one or more spaces, and each space has its own independent instance of every primitive (KV, Event, State, JSON, Vector). Think of spaces like schemas in PostgreSQL — they organize data within a database (branch) without creating full isolation boundaries.

## Command Overview

| Command | Syntax | Returns |
|---------|--------|---------|
| `use` | `use <branch> [space]` | Switches branch and/or space |
| `space list` | `space list` | All space names in current branch |
| `space create` | `space create <name>` | Creates a space |
| `space del` | `space del <name> [--force]` | Deletes a space |
| `space exists` | `space exists <name>` | Whether the space exists |

## Default Space

Every branch starts with a `default` space. When you open the CLI, all operations target this space automatically. You never need to create or switch to it explicitly.

```
$ strata --cache
strata:default/default> kv put key value
(version) 1
strata:default/default> event append log '{"msg":"hello"}'
(seq) 1
```

The `default` space cannot be deleted.

## Creating and Switching Spaces

Use `use <branch> <space>` to switch to a space. Spaces are auto-registered on first write — no explicit create step is needed:

```
$ strata --cache
strata:default/default> use default conversations
strata:default/conversations> kv put msg_001 hello
(version) 1
strata:default/conversations> use default tool-results
strata:default/tool-results> kv put task_42 done
(version) 1
strata:default/tool-results> space list
- conversations
- default
- tool-results
```

You can also create a space explicitly:

```
strata:default/default> space create my-space
OK
```

## Data Isolation Between Spaces

Each space has its own independent data. The same key in different spaces refers to different values:

```
$ strata --cache
strata:default/default> kv put config default-config
(version) 1
strata:default/default> use default experiments
strata:default/experiments> kv put config experiment-config
(version) 1
strata:default/experiments> use default
strata:default/default> kv get config
"default-config"
strata:default/default> use default experiments
strata:default/experiments> kv get config
"experiment-config"
```

This applies to all primitives — events, state cells, JSON documents, and vector collections are all space-scoped.

## Cross-Space Transactions

Transactions can span multiple spaces within the same branch. This is useful when you need atomic operations across organizational boundaries:

```
$ strata --cache
strata:default/default> begin
OK
strata:default/default> use default billing
strata:default/billing> kv put credits 99
(version) 1
strata:default/billing> use default api-logs
strata:default/api-logs> event append api_call '{"endpoint":"/search"}'
(seq) 1
strata:default/api-logs> commit
OK
```

## Space Naming Rules

Space names follow these conventions:

| Rule | Details |
|------|---------|
| **Start with** | Lowercase letter `[a-z]` |
| **Allowed characters** | Lowercase letters, digits, hyphens, underscores `[a-z0-9_-]` |
| **Max length** | 64 characters |
| **Reserved prefix** | `_system_` (reserved for internal use) |
| **Reserved name** | `default` (cannot be deleted) |

Valid names: `conversations`, `tool-results`, `agent_memory_v2`

Invalid names: `Conversations` (uppercase), `123-invalid` (starts with digit), `_system_internal` (reserved prefix)

## Deleting Spaces

Delete a space with `space del` (must be empty) or `space del --force` (deletes all data):

```
$ strata --cache
strata:default/default> use default temp
strata:default/temp> kv put key value
(version) 1
strata:default/temp> use default
strata:default/default> space del temp
(error) space is non-empty
strata:default/default> space del temp --force
OK
```

The `default` space cannot be deleted.

## Common Patterns

### Agent Memory Organization

```
$ strata --db ./data
strata:default/default> use default conversations
strata:default/conversations> event append user_message '{"content":"What is the weather?"}'
(seq) 1
strata:default/conversations> event append tool_call '{"tool":"weather_api"}'
(seq) 2
strata:default/conversations> use default tool-results
strata:default/tool-results> kv put weather_api:call_1 '{"temp":72,"conditions":"sunny"}'
(version) 1
strata:default/tool-results> use default user-context
strata:default/user-context> state set preferences '{"units":"fahrenheit"}'
(version) 1
```

### Multi-Tenant Data

```bash
#!/bin/bash
set -euo pipefail

for tenant in acme-corp globex initech; do
    strata --db ./data --space "$tenant" kv put config '{"plan":"enterprise"}'
    strata --db ./data --space "$tenant" vector create docs 384 --metric cosine
done

strata --db ./data space list
```

### Experiment Tracking

```
$ strata --db ./data
strata:default/default> use default hyperparams
strata:default/hyperparams> kv put config '{"lr":0.001,"epochs":10}'
(version) 1
strata:default/hyperparams> use default experiment-001
strata:default/experiment-001> kv put metrics '{"loss":0.42,"accuracy":0.87}'
(version) 1
strata:default/experiment-001> use default experiment-002
strata:default/experiment-002> kv put metrics '{"loss":0.38,"accuracy":0.89}'
(version) 1
```

## Spaces vs Branches

| | Branches | Spaces |
|--|----------|--------|
| **Purpose** | Isolation | Organization |
| **Data visibility** | Fully isolated | Fully visible within branch |
| **Transactions** | Cannot span branches | Can span spaces |
| **Analogy** | Git branches | PostgreSQL schemas |
| **Use case** | Separate experiments, sessions | Organize data within a session |

Use branches when you need full data isolation. Use spaces when you need to organize related data within a single branch.

## Next

- [KV Store](kv-store.md) — key-value operations
- [Branch Management](branch-management.md) — branch isolation
- [Sessions and Transactions](sessions-and-transactions.md) — cross-space atomicity
