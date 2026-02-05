# Event Log Guide

The Event Log is an append-only sequence of typed events. Events are immutable once written — you cannot update or delete individual events. This makes it ideal for audit trails, tool call history, and decision logs.

## Command Overview

| Command | Syntax | Returns |
|---------|--------|---------|
| `event append` | `event append <type> <payload>` | Sequence number |
| `event get` | `event get <seq>` | Event at sequence |
| `event list` | `event list <type> [--limit N] [--after SEQ]` | All events of type |
| `event len` | `event len` | Total event count |

## Appending Events

Each event has a **type** (a string label) and a **payload** (must be a JSON object):

```
$ strata --cache
strata:default/default> event append tool_call '{"tool":"web_search","query":"rust embedded database","results":10}'
(seq) 1
```

### Payload Must Be a JSON Object

Event payloads must be JSON objects. This ensures events are structured and queryable:

```
$ strata --cache
strata:default/default> event append auth '{"action":"login","user_id":42}'
(seq) 1
```

## Reading Events

### By Sequence Number

Each event gets a unique sequence number (starting from 1):

```
$ strata --cache
strata:default/default> event append log '{"msg":"hello"}'
(seq) 1
strata:default/default> event get 1
seq=1 type=log payload={"msg":"hello"}
```

### By Event Type

Retrieve all events with a specific type label:

```
$ strata --cache
strata:default/default> event append tool_call '{"tool":"search"}'
(seq) 1
strata:default/default> event append decision '{"choice":"A"}'
(seq) 2
strata:default/default> event append tool_call '{"tool":"calculator"}'
(seq) 3
strata:default/default> event list tool_call
seq=1 type=tool_call payload={"tool":"search"}
seq=3 type=tool_call payload={"tool":"calculator"}
strata:default/default> event list decision
seq=2 type=decision payload={"choice":"A"}
```

## Event Count

Get the total number of events in the current branch:

```
$ strata --cache
strata:default/default> event len
0
strata:default/default> event append log '{"msg":"one"}'
(seq) 1
strata:default/default> event append log '{"msg":"two"}'
(seq) 2
strata:default/default> event len
2
```

## Common Patterns

### Audit Trail

Log every tool call with input and output:

```bash
strata --cache event append tool_call '{"tool":"search","input":"query","output":"results"}'
```

### Decision Log

Record agent decisions with reasoning:

```bash
strata --cache event append decision '{"decision":"use_tool_A","reason":"higher confidence","confidence":0.92}'
```

## Branch Isolation

Events are isolated by branch. `event len` returns 0 in a new branch even if other branches have events:

```
$ strata --cache
strata:default/default> event append log '{"msg":"in default"}'
(seq) 1
strata:default/default> branch create other
OK
strata:default/default> use other
strata:other/default> event len
0
```

## Space Isolation

Within a branch, events are scoped to the current space:

```
$ strata --cache
strata:default/default> event append log '{"msg":"in default space"}'
(seq) 1
strata:default/default> event len
1
strata:default/default> use default other
strata:default/other> event len
0
```

See [Spaces](spaces.md) for the full guide.

## Transactions

Event append operations participate in transactions. Within a transaction, appended events are only visible after commit.

See [Sessions and Transactions](sessions-and-transactions.md) for details.

## Next

- [State Cell](state-cell.md) — mutable state with CAS
- [Cookbook: Agent State Management](../cookbook/agent-state-management.md) — combining events with other primitives
