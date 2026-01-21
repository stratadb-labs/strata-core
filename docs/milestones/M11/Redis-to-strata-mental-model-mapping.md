Redis → Strata Mental Model Compatibility Layer

Status: Draft
Scope: MVP Facade Contract
Purpose: Human compatibility, not substrate exposure

1. Design Goal

Strata is not Redis. But Redis is the most successful mental model for a programmable KV substrate.

We want:

• Redis users to feel immediately comfortable
• Redis-like operations to map cleanly onto Strata primitives
• No semantic lies
• No leaky abstractions
• No attempt to “out-Redis Redis”
• Power features always available, never hidden

This facade must be:

A reversible mapping, not a forked semantics.

Every facade operation MUST map 1:1 onto a substrate operation.

2. Core Differences (We Must Not Hide These)
Redis	Strata
Global namespace	Run-scoped namespace
No versions	Versioned by default
Implicit history loss	Explicit retention
Single primitive	7 primitives
Eventually consistent replication	Deterministic replay
Mutable overwrites	Versioned writes
Opaque semantics	Substrate semantics

We must not pretend these differences do not exist.

The facade hides friction, not truth.

3. The Default Run Illusion

Redis has a single global namespace.

Strata has runs.

Facade Rule

All facade operations implicitly use:

DEFAULT_RUN_ID


This is the Redis illusion.

Substrate Mapping
Facade	Substrate
set("k", v)	kv.set(DEFAULT_RUN_ID, "k", v)
get("k")	kv.get(DEFAULT_RUN_ID, "k")
del("k")	kv.del(DEFAULT_RUN_ID, "k")

Users never see run_id until they ask for it.

4. Redis Data Types → Strata Mapping
4.1 Redis String

Redis:

SET k v
GET k


Strata:

Value::String
Value::Bytes
Value::Int
Value::Float


Mapping:

Redis	Strata
String	Value::String or Value::Bytes
Integer	Value::Int
Float	Value::Float

Facade:

db.set("k", "v")
db.get("k") -> Option<Value>


Substrate:

db.kv().set(DEFAULT_RUN_ID, "k", Value::String("v"))
db.kv().get(DEFAULT_RUN_ID, "k") -> Option<Versioned<Value>>

4.2 Redis Hash → JsonStore

Redis:

HSET user name "alice"
HGET user name


Strata:

JsonStore document

{
  "name": "alice"
}


Facade:

db.hset("user", "name", "alice")
db.hget("user", "name")


Substrate:

db.json().patch(DEFAULT_RUN_ID, "user", [
  SetPath("$.name", "alice")
])
db.json().get_path(DEFAULT_RUN_ID, "user", "$.name")


We do NOT fake hashes. We map them to JSON documents.

4.3 Redis Lists → EventLog or Array Value

Redis lists are ambiguous: sometimes queues, sometimes logs.

Strata makes this explicit.

Use case	Redis	Strata
Append-only log	LPUSH/RPUSH	EventLog
Mutable list	LSET	Value::Array

Facade MUST choose the log interpretation by default.

4.4 Redis Streams → EventLog

Redis:

XADD mystream * type login user alice


Strata:

Event {
  event_type: "login",
  payload: { "user": "alice" }
}


Substrate:

db.events().append(DEFAULT_RUN_ID, "mystream", event)

4.5 Redis Pub/Sub → NOT MVP

Strata does not pretend to be a pub/sub broker.

That’s a different problem.

4.6 Redis JSON → JsonStore

1:1 mapping.

But Strata JSON is versioned.

4.7 Redis Vector → VectorStore

Redis:

HNSW / FLAT index


Strata:

VectorStore


We do NOT copy Redis vector query semantics. We only match mental shape.

5. Redis Commands → Strata Facade
5.1 Basic KV
Redis	Strata Facade	Substrate
SET k v	set(k, v)	kv.set(run, k, v)
GET k	get(k)	kv.get(run, k)
DEL k	del(k)	kv.del(run, k)
EXISTS k	exists(k)	kv.exists(run, k)
MGET	mget	kv.mget
MSET	mset	kv.mset

Facade returns raw Value, not Versioned.

5.2 Version Awareness (Strata-only)

Redis users do not expect this. But we expose it cleanly:

db.get_with_meta("k")


returns:

{
  value: ...,
  version: 42,
  timestamp: ...
}


Substrate:

kv.get(run, k) -> Versioned<Value>

5.3 History

Redis cannot do this.

Strata:

db.history("k", limit=10)
db.get_at("k", version=123)


Substrate:

kv.history(run, k)
kv.get_at(run, k, version)

6. Transactions (Redis Has None)

Redis MULTI/EXEC is not real isolation.

Strata transactions are real.

Facade:

tx = db.begin()
tx.set("a", 1)
tx.set("b", 2)
tx.commit()


Substrate:

txn = db.txn().begin()
txn.kv().set(run, "a", 1)
txn.kv().set(run, "b", 2)
txn.commit()

7. Redis Errors → Strata Errors

Redis has weak error contracts.

Strata will not.

Every error has:

• Stable code
• Stable category
• Structured payload

Facade may collapse errors into simple messages, but substrate is precise.

8. What Redis Does Not Have (Strata Must Not Hide)

These are exposed gently:

8.1 Runs
db.runs().create({ name: "experiment-1" })
db.use_run("experiment-1")


Now all facade ops use that run.

8.2 Versioning
db.get_version("k")
db.get_at("k", v)

8.3 Retention
db.set_retention(KeepLast(5))

8.4 Deterministic Replay

This will matter later.

9. Facade API Shape
9.1 Default Facade
db.set(key, value)
db.get(key) -> Option<Value>
db.del(key)
db.exists(key)

db.hset(doc, path, value)
db.hget(doc, path)

db.append(stream, event_type, payload)

db.begin()
db.commit()

9.2 Power Facade
db.use_run(run_id)
db.get_with_meta(key) -> Versioned<Value>
db.history(key)
db.get_at(key, version)

9.3 Substrate Escape Hatch

Always available:

db.substrate().kv().get(run, key)

10. Rules
Rule 1

Every facade call MUST desugar to a substrate call.

Rule 2

No facade-only semantics.

Rule 3

No irreversible sugar.

Rule 4

No hiding of versions, only deferring.

Rule 5

Redis familiarity must not compromise Strata’s core invariants.

11. Why This Matters

If this layer is wrong:

• Strata feels alien
• Adoption dies
• Power features become niche
• People misuse the system

If this layer is right:

• Redis users onboard instantly
• Power unfolds gradually
• Strata feels intuitive
• Substrate power becomes discoverable
• Strata becomes the agent database