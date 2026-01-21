Redis → Strata mental mapping
Redis mental model compatibility layer (M11)

Version: 0.1
Status: Draft (Compatibility Reference)
Last Updated: 2026-01-20

Purpose

Make Redis users feel at home instantly, without turning Strata into Redis.

This document defines the Redis mental model compatibility layer: a set of ergonomic defaults, naming, return shapes, CLI affordances, and type conventions that match Redis instincts while preserving Strata’s core semantics: runs, versioned history, deterministic substrate primitives, and explicit power escapes.

Non-goal

This is not a Redis protocol implementation. We are not implementing RESP, Redis clustering, Redis modules, or Redis server semantics. We are implementing Redis familiarity in the Strata UX and SDK surface.

1) The compatibility thesis

Redis is successful because it gives developers:

a small set of primitives

command-shaped operations

predictable returns

explicit errors

a great CLI

Strata can match those ergonomics while keeping:

runs

versioned values

multi-primitive substrate

portable embedded artifact

deterministic semantics

So the compatibility layer is:

Redis-feeling facade over Strata primitives + default run + canonical Value model + stable return shapes.

2) Canonical compatibility principles
Principle A: Default mode must hide runs

Redis has no runs. In Strata default mode, users should not have to know runs exist.

Every operation in default mode implicitly targets DefaultRun.

“Advanced mode” exposes run_id and history explicitly.

Principle B: Default mode must feel key-first

Redis thinks: key → value.
Strata thinks: run → primitive → entity_ref → versions.

Compatibility layer exposes:

key: String as the primary handle

optional namespaces (like Redis key prefixes) for composition

Internally this maps to:

EntityRef anchored in KV (and other primitives) inside DefaultRun.

Principle C: Return shapes must match Redis instincts

Redis users expect:

GET returns value or nil

DEL returns count

EXISTS returns bool or int

multi-get returns array of maybe-values

Strata compatibility layer will standardize this across SDK + CLI.

Principle D: Keep Redis naming where it helps, deviate where it protects Strata

Use Redis names when they directly match a Strata primitive or operation.
Do not adopt Redis semantics where they contradict versioning, runs, or type stability.

3) Core conceptual mapping
3.1 Redis keyspace ↔ Strata DefaultRun

Redis: global keyspace
Strata: keyspace exists inside a run

Compatibility mapping:

Redis global keyspace → Strata DefaultRun KV namespace

Rule:

Default SDK and CLI treat the database as a single keyspace.

Internally all keys are stored under DefaultRun.

3.2 Redis “string” values ↔ Strata Value

Redis effectively stores bytes and exposes typed-ish behavior via commands.

Strata stores Value as the canonical representation:

Value::Null
Value::Bool
Value::Int(i64)
Value::Float(f64)
Value::String(String)
Value::Bytes(Vec<u8>)
Value::Array(Vec<Value>)
Value::Object(Map<String, Value>)


Compatibility mapping:

Redis string/bytes → Value::String or Value::Bytes

Redis integer → Value::Int

Redis float → Value::Float

Redis JSON → Value::Object / Value::Array

3.3 Redis modules ↔ Strata primitives

Redis “modules” (JSON, Search, Vector) are conceptually similar to Strata primitives.

Mapping:

Redis Concept	Redis Command Family	Strata Primitive	Notes
Strings	SET/GET	KVStore	Default facade maps here
Hashes	HSET/HGET	JsonStore or KVStore(Object)	We prefer JsonStore for structured docs
Streams	XADD/XRANGE	EventLog / TraceStore	Strata splits events vs traces
JSON	JSON.SET/GET	JsonStore	Path semantics are the key
Vectors	FT / Vector	VectorStore	Keep vector + metadata cleanly
Pub/Sub	PUBLISH/SUBSCRIBE	(Deferred)	Not in MVP
4) Operation mapping (the heart of compatibility)

This section defines the Redis mental mapping for the top operations. For each:

what a Redis user expects

what Strata does under the hood

what Strata returns in default mode

what power escape exists

4.1 SET ↔ strata.set
Redis expectation

SET key value writes and overwrites.

Strata compatibility

strata.set(key, value) writes to KV in DefaultRun.

Under the hood:

KV.put(DefaultRun, key, Value) creates a new version

“overwrite” is just “write a new version”, never destructive

Default return:

Redis returns OK

Strata returns Ok(()) (SDK) and prints OK (CLI)

Power escape:

strata.kv().put(run_id, key, value) for explicit run

strata.kv().history(key) to see versions (advanced mode)

4.2 GET ↔ strata.get
Redis expectation

Returns value or nil.

Strata compatibility

strata.get(key) returns current value (latest version) or None.

Under the hood:

KV.get_latest(DefaultRun, key) returns Option<Versioned<Value>>

compatibility layer unwraps to Option<Value>

Default return:

SDK: Option<Value>

CLI: prints value, or (nil)

Power escape:

strata.getv(key) returns Option<Versioned<Value>>

strata.get_at(key, version) for historical fetch (advanced)

4.3 DEL ↔ strata.delete
Redis expectation

Returns integer count of deleted keys.

Strata compatibility

strata.delete(keys...) returns count.

Under the hood:

delete is a write that creates tombstone semantics (implementation detail)

history is preserved unless retention compacts it away

Default return:

SDK: u64 count

CLI: (integer) N

Power escape:

delete_at_version is forbidden in default mode

history view still available

4.4 EXISTS ↔ strata.exists
Redis expectation

Returns integer (0/1 or count) depending on command variant.

Strata compatibility

We support two APIs:

exists(key) -> bool

exists_many(keys) -> u64 (count)

CLI mirrors Redis:

EXISTS key prints (integer) 0|1

EXISTS k1 k2 prints count

4.5 MGET ↔ strata.mget
Redis expectation

Returns array of values or nils aligned with keys.

Strata compatibility

mget(keys) -> Vec<Option<Value>>

Power escape:
mgetv(keys) -> Vec<Option<Versioned<Value>>>

4.6 INCR / DECR ↔ strata.incr
Redis expectation

Atomic increment on integer stored at key.

Strata compatibility

incr(key, delta=1) -> i64

Under the hood:

StateCell is the correct primitive for atomic CAS-like updates

Compatibility layer can implement INCR in terms of:

read latest

compare-and-swap loop

write new value
This must be implemented with engine-level atomicity, not client retries.

Default return:

SDK returns new integer

CLI prints (integer) new_value

Power escape:

Expose StateCell explicitly in advanced mode

Important: If this is too much for MVP, we can explicitly defer INCR. But Redis users will look for it.

5) JSON mapping (RedisJSON → Strata JsonStore)

RedisJSON is the strongest compatibility anchor because it already implies a canonical value model.

5.1 JSON.SET ↔ strata.json.set

Redis: JSON.SET key path value

Strata: json_set(key, path, Value) where root is a document

Return:

Redis often returns OK

Strata returns OK / () similarly

5.2 JSON.GET ↔ strata.json.get

returns Value at path or nil

5.3 Path semantics

RedisJSON uses JSONPath-ish semantics.
Strata must choose path semantics carefully because this will freeze the contract.

Compatibility default:

Keep it simple: $.a.b[0] style

Avoid fancy filters in MVP

Power escape:

advanced API may support richer path queries later, but do not lock in now unless sure.

6) Streams/events mapping (Redis Streams → Strata EventLog + TraceStore)

Redis Streams are a hybrid of event log + consumer group state.

Strata splits concerns:

EventLog: append-only domain events

TraceStore: structured reasoning traces (agent output, steps, metadata)

6.1 XADD ↔ event.append / trace.append

Compatibility default:

Provide xadd(stream, fields) facade that maps to EventLog with:

event_type = "stream_entry"

payload = object(fields)

But better:

Do not pretend Strata has consumer groups.

Only map the mental model of “append ordered events”.

Return:

Redis returns entry id

Strata returns Version or EventId (needs standardization)

Recommendation:

In compatibility layer, return Version as the stable “id”.

7) Vector mapping (Redis Vector/Search → Strata VectorStore)

Redis vector search is usually bundled with indexing and query ops.

Strata MVP should focus on:

storing vectors

attaching metadata

later: search and ranking (10c/12+)

Compatibility mapping:

VSET key vector meta (Strata) maps to VectorStore upsert

VGET key returns vector + metadata

Do not promise Redis FT.SEARCH compatibility. That is a trap.

8) Default facade API (SDK)

This is the developer-facing compatibility surface.

8.1 Key-value facade (Redis “strings”)

set(key, value) -> ()

get(key) -> Option<Value>

getv(key) -> Option<Versioned<Value>>

mget(keys) -> Vec<Option<Value>>

delete(keys) -> u64

exists(key) -> bool

exists_many(keys) -> u64

8.2 JSON facade

json_set(key, path, value) -> ()

json_get(key, path) -> Option<Value>

json_del(key, path) -> u64

json_merge(key, path, value) -> () (optional)

8.3 Events facade

xadd(stream, fields: Map<String, Value>) -> Version

xrange(stream, start?, end?, limit?) -> Vec<Versioned<Value>> (simple)

8.4 Vectors facade

vset(key, vec: Vec<f32>, meta: Value::Object) -> ()

vget(key) -> Option<{ vec: Vec<f32>, meta: Value }>

vdel(key) -> bool

8.5 Escape hatches

db.runs() (advanced)

db.kv(run_id) / db.json(run_id) etc

history(key) / get_at(key, version) (advanced)

9) CLI compatibility (Redis-like feel)

Command families (illustrative):

SET, GET, DEL, EXISTS, MGET

JSON.SET, JSON.GET, JSON.DEL

XADD, XRANGE

VSET, VGET, VDEL

CLI output conventions:

(integer) N for counts

(nil) for missing

OK for success

structured printing for objects/arrays

bytes printed as hex or base64 with a prefix (must be stable)

10) Where Strata must intentionally diverge from Redis
Divergence 1: Versioned truth exists even if hidden

Redis overwrites. Strata versions.

Default facade hides this, but you must not lie:

getv exists

history exists

retention governs how much history survives

Divergence 2: Type stability per primitive

A key in KV is always a KV entity.
A document in JsonStore is always a JSON-like Value.
A vector entry is always a vector + metadata.

No “WRONGTYPE Operation against a key holding the wrong kind of value” behavior driven by key reuse across types.

Divergence 3: No implicit server features

No pubsub, no consumer groups, no cluster semantics in MVP facade.

11) Compatibility success criteria

A Redis user should be able to:

Install Strata and immediately do:

SET/GET/DEL/EXISTS/MGET

Use JSON with intuitive path ops

Store vectors with metadata

Append events and read them back

Understand missing vs null instantly

Learn about versioning and runs progressively, not upfront

If they cannot do these in 10 minutes, the compatibility layer failed.

12) Open decisions you should lock during 10b/11

These are the “make-or-break” choices.

Default Value printing and parsing (CLI + JSON wire)

Bytes representation (base64? hex? tagged?)

Float rules (NaN allowed? -0.0? canonicalization?)

Path syntax for JsonStore (minimal JSONPath subset)

Vector payload wire shape (vec as array of float32, metadata as Value)

Return conventions (Option vs Null vs explicit Missing)

Error taxonomy (codes + stable payload fields)