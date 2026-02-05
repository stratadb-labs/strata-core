# KV Store Guide

The KV Store is StrataDB's most general-purpose primitive. It maps string keys to arbitrary values with simple put/get/delete semantics.

## Command Overview

| Command | Syntax | Returns |
|---------|--------|---------|
| `kv put` | `kv put <key> <value>` | Version number |
| `kv get` | `kv get <key>` | The value, or `(nil)` |
| `kv del` | `kv del <key>` | OK |
| `kv list` | `kv list [--prefix P] [--limit N] [--cursor C]` | Matching key names |
| `kv history` | `kv history <key>` | Version history |

## Put

`kv put` creates or overwrites a key. It returns the version number of the write.

```
$ strata --cache
strata:default/default> kv put name Alice
(version) 1
strata:default/default> kv put age 30
(version) 1
strata:default/default> kv put score 99.5
(version) 1
strata:default/default> kv put active true
(version) 1
strata:default/default> kv put counter 1
(version) 1
strata:default/default> kv put counter 2
(version) 2
```

The CLI auto-detects types from input format (strings, integers, floats, booleans).

## Get

`kv get` returns the latest value for a key, or `(nil)` if the key doesn't exist.

```
$ strata --cache
strata:default/default> kv put key value
(version) 1
strata:default/default> kv get key
"value"
strata:default/default> kv get nonexistent
(nil)
```

## Delete

`kv del` removes a key.

```
$ strata --cache
strata:default/default> kv put key value
(version) 1
strata:default/default> kv del key
OK
strata:default/default> kv get key
(nil)
```

## List Keys

`kv list` returns all keys, optionally filtered by prefix.

```
$ strata --cache
strata:default/default> kv put user:1 Alice
(version) 1
strata:default/default> kv put user:2 Bob
(version) 1
strata:default/default> kv put task:1 Review
(version) 1
strata:default/default> kv list
task:1 = "Review"
user:1 = "Alice"
user:2 = "Bob"
strata:default/default> kv list --prefix user:
user:1 = "Alice"
user:2 = "Bob"
```

## Key Naming Conventions

Use colon-separated namespaces for organized key spaces:

```
$ strata --cache
strata:default/default> kv put user:123:name Alice
(version) 1
strata:default/default> kv put user:123:email alice@example.com
(version) 1
strata:default/default> kv put config:model gpt-4
(version) 1
strata:default/default> kv put config:temperature 0.7
(version) 1
strata:default/default> kv list --prefix user:123:
user:123:email = "alice@example.com"
user:123:name = "Alice"
strata:default/default> kv list --prefix config:
config:model = "gpt-4"
config:temperature = 0.7
```

## Branch Isolation

KV data is isolated by branch. See [Branches](../concepts/branches.md) for details.

```
$ strata --cache
strata:default/default> kv put key default-value
(version) 1
strata:default/default> branch create other
OK
strata:default/default> use other
strata:other/default> kv get key
(nil)
```

## Space Isolation

Within a branch, KV data is further organized by space. Each space has independent keys:

```
$ strata --cache
strata:default/default> kv put config default-value
(version) 1
strata:default/default> use default experiments
strata:default/experiments> kv get config
(nil)
strata:default/experiments> kv put config experiment-value
(version) 1
strata:default/experiments> use default
strata:default/default> kv get config
"default-value"
```

See [Spaces](spaces.md) for the full guide.

## Transactions

KV operations participate in transactions. Within a transaction, reads and writes are atomic:

```
$ strata --cache
strata:default/default> begin
OK
strata:default/default> kv put a 1
(version) 1
strata:default/default> kv put b 2
(version) 1
strata:default/default> commit
OK
```

Both writes become visible atomically. See [Sessions and Transactions](sessions-and-transactions.md) for the full guide.

## Next

- [Event Log](event-log.md) — append-only event streams
- [API Quick Reference](../reference/api-quick-reference.md) — all methods at a glance
