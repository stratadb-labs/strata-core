# Milestone 5 (M5) Architecture Specification: Native JSON Primitive

**Version**: 0.1 (Draft)
**Status**: Design Phase
**Purpose**: Define JSON as a first-class semantic primitive with correct transactional, snapshot, conflict, and durability semantics.

---

## 1. Motivation

The current primitive set (KVStore, EventLog, StateCell, TraceStore, RunIndex) does not support general-purpose mutable structured state.

Attempting to layer JSON on top of KV would lead to one of three architectural traps:

1. Whole-document writes → massive conflicts and poor performance
2. Path-to-key explosion → incoherent structure and brittle conflict detection
3. Shadow indexing layers → two state systems, impossible to reason about

JSON is not a value type. It defines **mutation semantics**.

Once JSON exists, it introduces:

* Path-level atomicity
* Path-level conflict detection
* Structural merging
* Patch-based WAL
* Subtree snapshots
* Nested CAS
* Partial reads

These semantics must be native and must be understood by:

* Snapshot system
* WAL
* Replay
* Diffing
* Conflict detection

Therefore, JSON must be introduced before durability formats stabilize.

---

## 2. Goals

### 2.1 Primary Goal

Introduce a native JSON primitive with correct transactional semantics.

### 2.2 Non-Goals

M5 is not a document database.

We explicitly do NOT target:

* Ad-hoc querying
* Indexes
* Aggregations
* Joins
* Full JSONPath filtering

These must be architecturally enabled, but not implemented in M5.

---

## 3. JSON Primitive API

```rust
struct JsonDocId(Uuid);

impl JsonStore {
    fn create(run_id, initial_value) -> JsonDocId;

    fn get(run_id, doc_id, path) -> JsonValue;
    fn get_subtree(run_id, doc_id, path) -> JsonSubtree;

    fn set(run_id, doc_id, path, value);
    fn delete(run_id, doc_id, path);
    fn patch(run_id, doc_id, patch: JsonPatch);

    fn cas(run_id, doc_id, path, expected_version, new_value);
}
```

Path syntax is restricted initially (dot + index), but internally must be represented as a structural path, not a string.

---

## 4. Internal Representation

### 4.1 Structural Tree

JSON must be represented as a tree, not a blob.

Each node:

```rust
struct Node {
    id: NodeId,
    kind: NodeKind,
    children: Vec<NodeId>,
}
```

Nodes must support:

* Structural sharing
* Subtree references
* Cheap cloning
* Cheap diffing

Arena-based or persistent representations are acceptable.

---

## 5. Versioning Semantics

Versioning is path-aware.

Each subtree has its own version.

```
Version(DocRoot)
  ├─ Version($.a)
  │    └─ Version($.a.b)
  └─ Version($.x)
```

Writes increment versions only on the mutated subtree and ancestors.

---

## 6. Conflict Detection

Conflict detection is region-based.

Two writes conflict if and only if:

* Their path regions overlap

Examples:

| Write A | Write B | Conflict |
| ------- | ------- | -------- |
| $.a.b   | $.a.c   | ❌        |
| $.a     | $.a.b   | ✅        |
| $.x     | $.y     | ❌        |

---

## 7. Snapshot Semantics

Snapshots capture:

* Root version
* Structural reference

Reads must be subtree-consistent.

Snapshots are lazy: they reference live structure with version bounds.

---

## 8. WAL Format

WAL entries must be patch-based.

```rust
enum JsonWalEntry {
    Set { doc, path, value },
    Delete { doc, path },
    Insert { doc, path, index, value },
    Remove { doc, path, index },
    ReplaceSubtree { doc, path, subtree },
}
```

Never log full documents.

---

## 9. Replay Semantics

Replay re-applies patches deterministically.

Replay is structural, not value-based.

---

## 10. Diff Semantics

Diff produces a patch set.

```rust
fn diff(a, b) -> Vec<JsonPatch>
```

Used for:

* Auditing
* Replication
* Debugging
* Time travel

---

## 11. Read Set Tracking

Reads must record structural regions.

```rust
ReadSet = {(doc_id, PathRegion, Version)}
```

---

## 12. Performance Constraints

| Operation   | Target   |
| ----------- | -------- |
| Path get    | ≤ 3× KV  |
| Path set    | ≤ 5× KV  |
| Subtree get | ≤ 10× KV |

---

## 13. Future Compatibility Hooks

JSON primitive must expose:

* Path iterators
* Subtree views
* Change streams
* Structural visitors

These allow later implementation of:

* Indexes
* Queries
* Projections
* Aggregations

---

## 14. Success Criteria

* Path-level atomicity
* Path-level conflict detection
* Patch-based WAL
* Lazy snapshots
* Structural diff
* Deterministic replay
* ≤ 5× KV latency

---

## 15. Risk

This is hard.

But retrofitting JSON later will be harder.

---

## 16. Conclusion

JSON is not a feature. It is a semantic substrate.

M5 defines mutation semantics.
M6+ builds on them.

Failure to do this now will permanently constrain the system.
