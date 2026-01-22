# Strata Primitive Contract

> **Status**: Substrate Constitution
> **Stability**: Changes require RFC process
> **Scope**: Invariants that define what it means to be a Strata primitive

---

## Purpose

This document defines the **invariants** that every Strata primitive must obey. These are not suggestions. They are the rules that make Strata coherent.

If a primitive violates any of these rules, it is not a Strata primitive.

---

## The Core Invariant

**There is one interaction model, not seven.**

Users learn one mental model. That model applies to all primitives. The primitives may have different operations internally, but externally they must feel the same.

---

## Invariant 1: Everything is Addressable

Every entity in Strata has a **stable identity** that can be:
- Referenced
- Stored
- Passed between systems
- Used to retrieve the entity later

### The Rule

If something exists in Strata, it has an address. If it doesn't have an address, it doesn't exist as a first-class entity.

### What This Means

- Every KV entry has an identity (run + key)
- Every event has an identity (run + sequence)
- Every state cell has an identity (run + name)
- Every trace has an identity (run + trace_id)
- Every run has an identity (run_id)
- Every JSON document has an identity (run + doc_id)
- Every vector has an identity (run + collection + vector_id)

### What This Does NOT Specify

- The format of identity (URI, struct, string, etc.)
- How identities are serialized
- Whether identities are human-readable

---

## Invariant 2: Everything is Versioned

Every mutation in Strata produces a **version**. Time is not optional. History is not optional.

### The Rule

When you read an entity, you must be able to know:
1. **Which version** you are looking at
2. **When** that version came into existence
3. **That versions are ordered** within an entity

### What This Means

- A KV put creates a new version of that key
- An event append creates a new version (the sequence number)
- A state cell set creates a new version
- A trace record creates a version (the trace itself)
- A run transition creates a new version of the run
- A JSON set creates a new document version
- A vector upsert creates a new version

### What This Does NOT Specify

- The representation of versions (counter, timestamp, hash, etc.)
- Whether old versions are retained or discarded
- How versions are compared across primitives
- Whether version history is queryable

### Clarification: Append-Only vs Mutable

Some primitives are append-only (EventLog, TraceStore). Some are mutable (KVStore, StateCell).

This invariant does not require them to behave the same way internally.

It requires that **externally**, both have the property: "I can know what version I'm looking at."

For append-only primitives, the version is the position in the log.
For mutable primitives, the version is a counter or transaction ID.

The user does not need to care about this distinction.

---

## Invariant 3: Everything is Transactional

All primitives participate in transactions **the same way**.

### The Rule

If Strata has transactions (it does), then:
1. Every primitive can participate in a transaction
2. Multiple primitives can participate in the **same** transaction
3. Either all operations commit, or none do
4. There is no primitive that is "outside" the transaction system

### What This Means

A user can write:
```
BEGIN TRANSACTION
  write to KV
  append to EventLog
  update StateCell
  record Trace
COMMIT
```

And either all four operations succeed, or none do.

### What This Does NOT Specify

- Transaction isolation level (we use snapshot isolation, but that's an implementation choice)
- Retry semantics
- Timeout behavior
- Whether transactions can span runs (they cannot, but that's a separate constraint)

### The Non-Negotiable

There is no such thing as a "non-transactional primitive" in Strata.

If we add a new primitive that cannot participate in transactions, we have broken the model.

---

## Invariant 4: Everything Has a Lifecycle

Every entity follows the same **lifecycle shape**.

### The Rule

An entity can:
1. **Come into existence** (be created)
2. **Exist** (be readable)
3. **Evolve** (be mutated, if mutable)
4. **Cease to exist** (be destroyed, if destructible)

### What This Means

| Primitive | Create | Exists | Evolve | Destroy |
|-----------|--------|--------|--------|---------|
| KVStore | put | get | put | delete |
| EventLog | (implicit) | read | append | (immutable) |
| StateCell | init | read | set/cas | delete |
| TraceStore | record | read | (immutable) | (immutable) |
| RunIndex | create_run | get_run | transition | delete_run |
| JsonStore | create | get | set | destroy |
| VectorStore | upsert | get | upsert | delete |

### What This Does NOT Specify

- Whether "destroy" means hard delete or soft delete
- Whether destroyed entities can be re-created
- The exact names of lifecycle operations

### The Non-Negotiable

If a primitive has operations that don't fit into this lifecycle model, those operations must be expressible in terms of it.

---

## Invariant 5: Everything Exists Within a Run

All data in Strata is scoped to a **run** (execution context).

### The Rule

1. Every entity belongs to exactly one run
2. A run is the unit of isolation
3. Operations specify which run they operate on
4. Cross-run operations are explicit and limited

### What This Means

- `kv.get(run_id, key)` - the run is always explicit
- There is no "global" namespace that ignores runs (except for run metadata itself)
- Transactions are scoped to a single run

### What This Does NOT Specify

- What a run represents semantically (agent execution, user session, etc.)
- Run lifecycle states
- Whether runs can be forked or merged

### Exception: Run Metadata

The RunIndex primitive manages runs themselves. It operates in a "meta" namespace. This is the only exception to "everything is scoped to a run."

---

## Invariant 6: Everything is Introspectable

Users can ask questions about any entity's existence and state.

### The Rule

For any entity, a user can ask:
1. **Does it exist?**
2. **What is its current state?**
3. **What version am I looking at?**

### What This Means

Every primitive must support:
- An existence check
- A read operation that returns the current state
- Version information attached to the read result

### What This Does NOT Specify

- Whether historical versions are queryable
- Whether diffs are computable
- Whether provenance is tracked
- The shape of introspection APIs beyond basic reads

### Clarification

This invariant requires **basic** introspection: "what is it now?"

It does not require **advanced** introspection: "what was it before?", "what changed?", "why did it change?"

Those are valuable capabilities, but they are not invariants. They are features built on top of the invariants.

---

## Invariant 7: Reads and Writes Have Consistent Semantics

The meaning of "read" and "write" is the same across all primitives.

### The Rule

1. A **read** never modifies state
2. A **write** always produces a new version
3. Within a transaction, reads see a consistent snapshot
4. Within a transaction, reads see prior writes (read-your-writes)

### What This Means

- `kv.get()` is a read
- `kv.put()` is a write
- `event.read()` is a read
- `event.append()` is a write
- There is no operation that is "sometimes a read, sometimes a write"

### What This Does NOT Specify

- Whether reads are fast-pathed outside transactions
- Whether writes are buffered
- Lock granularity

---

## The Seven Primitives: Conformance

| Invariant | KV | Event | State | Trace | Run | Json | Vector |
|-----------|----|----|----|----|----|----|-----|
| Addressable | key | sequence | name | trace_id | run_id | doc_id | collection/id |
| Versioned | txn_id | sequence | counter | txn_id | txn_id | counter | txn_id |
| Transactional | Yes | Yes | Yes | Yes | Yes | Yes | Yes |
| Lifecycle | CRUD | CR | CRUD | CR | CRUD | CRUD | CRUD |
| Run-scoped | Yes | Yes | Yes | Yes | (meta) | Yes | Yes |
| Introspectable | Yes | Yes | Yes | Yes | Yes | Yes | Yes |
| Read/Write | Yes | Yes | Yes | Yes | Yes | Yes | Yes |

All seven primitives conform to all seven invariants.

---

## What This Document Does NOT Cover

This document defines **what must be true**.

It does not define:

- **API shape**: How these invariants are expressed in code
- **Search**: How entities are discovered (that's a feature, not an invariant)
- **History/Diff**: How past versions are queried (that's a feature)
- **Provenance**: How causality is tracked (that's a feature)
- **Wire protocol**: How operations are serialized (that's implementation)
- **CLI/SDK**: How users invoke operations (that's product surface)

Those are covered in separate documents.

---

## How to Use This Document

### When Adding a New Primitive

Ask:
1. Does it have a stable identity? (Invariant 1)
2. Does every mutation produce a version? (Invariant 2)
3. Can it participate in transactions with other primitives? (Invariant 3)
4. Does it follow the create/exist/evolve/destroy lifecycle? (Invariant 4)
5. Is it scoped to a run? (Invariant 5)
6. Can users check existence and read current state? (Invariant 6)
7. Are reads and writes clearly separated? (Invariant 7)

If any answer is "no," either fix the primitive or don't add it.

### When Designing an API

The API must make these invariants **expressible**. It does not need to make them **explicit** in every call.

For example:
- Version information can be in a wrapper type or a separate call
- Run scoping can be in a context object or a parameter
- Transaction participation can be implicit or explicit

The invariants constrain what is possible. They do not constrain syntax.

### When Evaluating a Feature Request

Ask: "Does this feature depend on any invariant?"

If yes, the feature is likely sound.

If it requires violating an invariant, reject it or find a different design.

---

## Document History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-01-19 | Initial invariants extracted from Universal Primitive Protocol |
