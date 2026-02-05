# Your First Database

This tutorial walks through all six StrataDB primitives. Every example uses `strata --cache` so you can follow along without disk setup.

## Prerequisites

- [Strata CLI installed](installation.md)

## Step 1: Open a Database

```
$ strata --cache
strata:default/default>
```

The CLI starts you on the "default" branch in the "default" space. All data operations target the current branch and space.

For a persistent database, use `strata --db ./my-data` instead.

## Step 2: KV Store — Working Memory

The KV Store is the most general-purpose primitive. Store any value by key.

```
$ strata --cache
strata:default/default> kv put agent:name Alice
(version) 1
strata:default/default> kv put agent:score 95
(version) 1
strata:default/default> kv put agent:active true
(version) 1
strata:default/default> kv get agent:name
"Alice"
strata:default/default> kv list --prefix agent:
agent:active = true
agent:name = "Alice"
agent:score = 95
strata:default/default> kv del agent:score
OK
strata:default/default> kv get agent:score
(nil)
```

## Step 3: Event Log — Immutable History

The Event Log records events that cannot be modified after writing. Each event has a type and a JSON payload.

```
$ strata --cache
strata:default/default> event append tool_call '{"tool":"search","query":"weather"}'
(seq) 1
strata:default/default> event get 1
seq=1 type=tool_call payload={"tool":"search","query":"weather"}
strata:default/default> event list tool_call
seq=1 type=tool_call payload={"tool":"search","query":"weather"}
strata:default/default> event len
1
```

## Step 4: State Cell — Coordination

State cells provide mutable state with compare-and-swap (CAS) for safe concurrent updates.

```
$ strata --cache
strata:default/default> state init status idle
(version) 1
strata:default/default> state init status should-not-overwrite
(no-op)
strata:default/default> state get status
"idle"
strata:default/default> state set counter 0
(version) 1
strata:default/default> state cas counter 1 1
(version) 2
strata:default/default> state cas counter 999 2
(error) CAS conflict: expected version 999, current version 2
```

The `state cas` command takes the cell name, the expected version, and the new value. It succeeds only if the current version matches the expected version.

## Step 5: JSON Store — Structured Documents

Store JSON documents and mutate them at specific paths.

```
$ strata --cache
strata:default/default> json set config $ '{"model":"gpt-4","temperature":0.7,"max_tokens":1000}'
(version) 1
strata:default/default> json get config
{"model":"gpt-4","temperature":0.7,"max_tokens":1000}
strata:default/default> json set config $.temperature 0.9
(version) 2
strata:default/default> json get config $.temperature
0.9
strata:default/default> json list
config
strata:default/default> json del config $
OK
```

## Step 6: Vector Store — Similarity Search

Store embeddings and search by similarity.

```
$ strata --cache
strata:default/default> vector create embeddings 4 --metric cosine
OK
strata:default/default> vector upsert embeddings doc-1 [1.0,0.0,0.0,0.0]
OK
strata:default/default> vector upsert embeddings doc-2 [0.0,1.0,0.0,0.0]
OK
strata:default/default> vector upsert embeddings doc-3 [0.9,0.1,0.0,0.0]
OK
strata:default/default> vector search embeddings [1.0,0.0,0.0,0.0] 2
key=doc-1 score=1.0000
key=doc-3 score=0.9939
strata:default/default> vector del embeddings doc-2
OK
```

## Step 7: Branches — Data Isolation

Branches give you isolated namespaces for data, like git branches.

```
$ strata --cache
strata:default/default> kv put shared-key default-value
(version) 1
strata:default/default> branch create experiment
OK
strata:default/default> use experiment
strata:experiment/default> kv get shared-key
(nil)
strata:experiment/default> kv put shared-key experiment-value
(version) 1
strata:experiment/default> use default
strata:default/default> kv get shared-key
"default-value"
strata:default/default> branch list
- default
- experiment
strata:default/default> branch del experiment
OK
```

## Putting It All Together

Here is a REPL session that simulates an AI agent session using multiple primitives:

```
$ strata --cache
strata:default/default> branch create session-001
OK
strata:default/default> use session-001
strata:session-001/default> kv put config:model gpt-4
(version) 1
strata:session-001/default> kv put config:max_retries 3
(version) 1
strata:session-001/default> state init status started
(version) 1
strata:session-001/default> event append tool_call '{"tool":"web_search","query":"StrataDB docs"}'
(seq) 1
strata:session-001/default> json set conversation $ '{"messages":[{"role":"user","content":"What is StrataDB?"},{"role":"assistant","content":"An embedded database for AI agents."}]}'
(version) 1
strata:session-001/default> state set status completed
(version) 2
strata:session-001/default> event len
1
strata:session-001/default> state get status
"completed"
```

## Next Steps

- [Concepts](../concepts/index.md) — understand branches, value types, and transactions
- [Guides](../guides/index.md) — deep dives into each primitive
- [Cookbook](../cookbook/index.md) — real-world patterns
