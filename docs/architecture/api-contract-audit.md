# API Contract Audit

For each command, verify: Does the return type match what the types promise?

## 1. Output Enum Inventory

**Location**: `crates/executor/src/output.rs`

42 Output variants defined. 9 are never used in production or test code.

### Unused Output Variants (Dead Code)

| Variant | Purpose (from comment) | Usage Count |
|---------|----------------------|-------------|
| `Value(Value)` | "Single value without version info" | 0 |
| `Values(Vec<Option<VersionedValue>>)` | "Multiple optional versioned values (mget)" | 0 |
| `JsonSearchHits(Vec<JsonSearchHit>)` | "JSON search hits" | 0 |
| `VectorMatchesWithExhausted { matches, exhausted }` | "Vector search with budget exhaustion flag" | 0 |
| `MaybeBranchId(Option<BranchId>)` | "Optional branch ID (for parent lookup)" | 0 |
| `TxnId(String)` | "Transaction ID" | 0 |
| `RetentionVersion(Option<RetentionVersionInfo>)` | "Retention version info" | 0 |
| `RetentionPolicy(RetentionPolicyInfo)` | "Retention policy" | 0 |
| `BranchInfo(BranchInfo)` | "Single branch info (unversioned)" | 0 |

These 9 variants are never constructed anywhere — not in handlers, not in session dispatch, not in tests, not in the API layer. They exist only as enum definitions.

### Test-Only Output Variants

| Variant | Production Uses | Test-Only Uses |
|---------|----------------|---------------|
| `Int(i64)` | 0 | serialization tests |
| `Float(f64)` | 0 | serialization tests |
| `Versioned(VersionedValue)` | 0 | serialization tests |
| `Strings(Vec<String>)` | 0 | serialization tests |
| `Versions(Vec<u64>)` | 0 | serialization tests |
| `KvScanResult { entries, cursor }` | 0 | serialization tests |

These 6 variants are constructed only in serialization round-trip tests, never by any command handler.

**Total**: 15 of 42 Output variants (36%) are unused in production code.

## 2. Complete Command → Output Contract Table

### KV Commands

| Command | Handler Returns | Correct? | Notes |
|---------|----------------|----------|-------|
| `KvPut` | `Output::Version(u64)` | Yes | Version is `extract_version(Txn(commit_id))` |
| `KvGet` | `Output::Maybe(Option<Value>)` | **Mismatch** | Docstring says `MaybeVersioned`, returns `Maybe` |
| `KvDelete` | `Output::Bool(bool)` | Yes | `true` if key existed |
| `KvList` | `Output::Keys(Vec<String>)` | Yes | |
| `KvGetv` | `Output::VersionHistory(Option<Vec<VersionedValue>>)` | Yes | |

### State Commands

| Command | Handler Returns | Correct? | Notes |
|---------|----------------|----------|-------|
| `StateSet` | `Output::Version(u64)` | Yes | Version is `extract_version(Counter(n))` |
| `StateRead` | `Output::Maybe(Option<Value>)` | **Mismatch** | Should return version for CAS workflow |
| `StateCas` | `Output::MaybeVersion(Option<u64>)` | Yes | `None` on failure (swallows errors — #926) |
| `StateInit` | `Output::Version(u64)` | Yes | |
| `StateReadv` | `Output::VersionHistory(Option<Vec<VersionedValue>>)` | Yes | |

### JSON Commands

| Command | Handler Returns | Correct? | Notes |
|---------|----------------|----------|-------|
| `JsonSet` | `Output::Version(u64)` | Yes | |
| `JsonGet` | `Output::Maybe(Option<Value>)` | **Mismatch** | Should return version for CAS workflow |
| `JsonDelete` | `Output::Uint(u64)` | Partial | Root: 0/1. Non-root: always 1 |
| `JsonList` | `Output::JsonListResult { keys, cursor }` | Yes | |
| `JsonGetv` | `Output::VersionHistory(Option<Vec<VersionedValue>>)` | **Silent error** | `filter_map` drops deserialization failures |

### Event Commands

| Command | Handler Returns | Correct? | Notes |
|---------|----------------|----------|-------|
| `EventAppend` | `Output::Version(u64)` | Yes | Version is `extract_version(Sequence(n))` |
| `EventRead` | `Output::MaybeVersioned(Option<VersionedValue>)` | Yes | Includes version and timestamp |
| `EventReadByType` | `Output::VersionedValues(Vec<VersionedValue>)` | **Fallback** | Non-Sequence versions → 0 (#943) |
| `EventLen` | `Output::Uint(u64)` | Yes | |

### Vector Commands

| Command | Handler Returns | Correct? | Notes |
|---------|----------------|----------|-------|
| `VectorUpsert` | `Output::Version(u64)` | Yes | Auto-creates collection silently |
| `VectorGet` | `Output::VectorData(Option<VersionedVectorData>)` | Yes | |
| `VectorDelete` | `Output::Bool(bool)` | Yes | `true` if existed |
| `VectorSearch` | `Output::VectorMatches(Vec<VectorMatch>)` | Yes | Scores not range-validated |
| `VectorCreateCollection` | `Output::Version(u64)` | Yes | |
| `VectorDeleteCollection` | `Output::Bool(bool)` | Yes | `false` if not found |
| `VectorListCollections` | `Output::VectorCollectionList(Vec<CollectionInfo>)` | Yes | Filters internal collections |

### Branch Commands

| Command | Handler Returns | Correct? | Notes |
|---------|----------------|----------|-------|
| `BranchCreate` | `Output::BranchWithVersion { info, version }` | Yes | |
| `BranchGet` | `BranchInfoVersioned(...)` or `Maybe(None)` | **Mixed** | Two different Output variants |
| `BranchList` | `Output::BranchInfoList(Vec<VersionedBranchInfo>)` | Yes | |
| `BranchExists` | `Output::Bool(bool)` | Yes | |
| `BranchDelete` | `Output::Unit` | Yes | Rejects default branch |
| `BranchExport` | `Output::BranchExported(BranchExportResult)` | Yes | |
| `BranchImport` | `Output::BranchImported(BranchImportResult)` | Yes | |
| `BranchBundleValidate` | `Output::BundleValidated(BundleValidateResult)` | Yes | |

### Transaction Commands (via Session)

| Command | Handler Returns | Correct? | Notes |
|---------|----------------|----------|-------|
| `TxnBegin` | `Output::TxnBegun` | Yes | |
| `TxnCommit` | `Output::TxnCommitted { version }` | Yes | Semantic variant, not plain Version |
| `TxnRollback` | `Output::TxnAborted` | Yes | Semantic variant, not plain Unit |
| `TxnInfo` | `Output::TxnInfo(Option<TransactionInfo>)` | Yes | |
| `TxnIsActive` | `Output::Bool(bool)` | Yes | |

### Database / Utility Commands

| Command | Handler Returns | Correct? | Notes |
|---------|----------------|----------|-------|
| `Ping` | `Output::Pong { version }` | Yes | |
| `Info` | `Output::DatabaseInfo(DatabaseInfo)` | Yes | |
| `Flush` | `Output::Unit` | Yes | |
| `Compact` | `Output::Unit` | Yes | |
| `Search` | `Output::SearchResults(Vec<SearchResultHit>)` | Yes | |
| `RetentionApply` | `Err(Internal)` | N/A | Not implemented |
| `RetentionStats` | `Err(Internal)` | N/A | Not implemented |
| `RetentionPreview` | `Err(Internal)` | N/A | Not implemented |

## 3. Problems Found

### Problem 1: Output docstring claims KvGet returns MaybeVersioned — it returns Maybe

**Severity**: Medium

**Location**: `crates/executor/src/output.rs:22-28`

```rust
/// match result {
///     Output::MaybeVersioned(Some(v)) => println!("Found: {:?}", v.value),
///     Output::MaybeVersioned(None) => println!("Not found"),
///     _ => unreachable!("KvGet always returns MaybeVersioned"),
/// }
```

But `kv_get()` (handlers/kv.rs:49) returns `Output::Maybe(result)` — which is `Maybe(Option<Value>)`, not `MaybeVersioned(Option<VersionedValue>)`. The docstring is wrong.

### Problem 2: KvGet, StateRead, JsonGet strip version metadata

**Severity**: Medium

Three read commands return `Output::Maybe(Option<Value>)` — the value without any version information:

| Command | Returns | Version Available? |
|---------|---------|-------------------|
| `KvGet` | `Maybe(Option<Value>)` | **No** |
| `StateRead` | `Maybe(Option<Value>)` | **No** |
| `JsonGet` | `Maybe(Option<Value>)` | **No** |
| `EventRead` | `MaybeVersioned(Option<VersionedValue>)` | **Yes** |
| `VectorGet` | `VectorData(Option<VersionedVectorData>)` | **Yes** |

This forces a suboptimal workflow for CAS operations:

```
StateRead("cell")  → gets value only, no version
StateCas("cell", ???, new_value)  → needs expected_counter, which isn't in the read response

User must call StateReadv("cell") → gets full history including versions
```

EventRead and VectorGet both return versioned data. KvGet, StateRead, and JsonGet do not. The inconsistency means some primitives support a natural read-then-CAS workflow while others require an extra round-trip.

### Problem 3: BranchGet returns two different Output variants

**Severity**: Low

**Location**: `crates/executor/src/handlers/branch.rs:77-82`

```rust
pub fn branch_get(p: &Arc<Primitives>, branch: BranchId) -> Result<Output> {
    let result = convert_result(p.branch.get_branch(branch.as_str()))?;
    match result {
        Some(v) => Ok(Output::BranchInfoVersioned(versioned_to_branch_info(v))),
        None => Ok(Output::Maybe(None)),
    }
}
```

Found → `Output::BranchInfoVersioned(...)`. Not found → `Output::Maybe(None)`. A client must pattern-match against two different variants. Every other "maybe" command returns a single variant with `None` inside it.

Should be: `Output::MaybeVersioned(None)` for not-found, or a new `MaybeBranchInfoVersioned(Option<VersionedBranchInfo>)` variant.

### Problem 4: JsonDelete returns different semantics for root vs non-root path

**Severity**: Low

**Location**: `crates/executor/src/handlers/json.rs:111-117`

```rust
if json_path.is_root() {
    let deleted = convert_result(p.json.destroy(&branch_id, &key))?;
    Ok(Output::Uint(if deleted { 1 } else { 0 }))  // 0 or 1
} else {
    convert_result(p.json.delete_at_path(&branch_id, &key, &json_path))?;
    Ok(Output::Uint(1))  // always 1
}
```

Root path: returns `0` if document didn't exist, `1` if it was deleted.
Non-root path: always returns `1` (or errors if document doesn't exist).

The in-transaction version (session.rs:357-360) is more consistent — it uses `Uint(if deleted { 1 } else { 0 })` for both paths.

### Problem 5: JsonGetv silently drops deserialization errors

**Severity**: Medium

**Location**: `crates/executor/src/handlers/json.rs:26-27`

```rust
.filter_map(|v| {
    let value = convert_result(json_to_value(v.value)).ok()?;
    Some(VersionedValue { ... })
})
```

`convert_result(...).ok()?` converts errors to `None`, silently dropping any version whose JSON value fails to deserialize. The client receives a history with missing versions and no error. If a stored document is corrupted, the corruption is invisible.

Contrast: `state_readv()` (state.rs:27) uses `.map(bridge::to_versioned_value)` — no silent dropping. `kv_getv()` (kv.rs:26) also uses `.map(to_versioned_value)` — no silent dropping. Only `json_getv()` silently discards errors.

### Problem 6: 15 unused Output variants (9 dead, 6 test-only)

**Severity**: Low

36% of the Output enum is never used in production. This inflates the API surface, confuses contributors reading the code, and increases binary size through unnecessary serde impls.

See Section 1 for the full list.

### Problem 7: VectorUpsert silently auto-creates collection

**Severity**: Low (existing #932)

**Location**: `crates/executor/src/handlers/vector.rs:86`

```rust
let _ = p.vector.create_collection(branch_id, &collection, config);
```

The `let _` discards ALL errors, not just `AlreadyExists`. If collection creation fails due to a storage error, the error is silently ignored, and the subsequent insert will fail with a confusing "collection not found" error.

Additionally, auto-creation hardcodes `Cosine` as the distance metric regardless of what the user intends. If the user later explicitly creates the collection with a different metric, it already exists with Cosine.

### Problem 8: VectorSearch scores not range-validated

**Severity**: Low

**Location**: `crates/executor/src/handlers/vector.rs:159`

Search results are passed through `to_vector_match()` (vector.rs:50-61) without validating that scores are in any documented range. The brute-force backend's `cosine_similarity` can produce scores in [-1.0, 1.0], but NaN embeddings (issue #948) would produce NaN scores that are passed directly to the client.

## 4. Version Semantics at the Boundary

All version types collapse to `u64` at the executor boundary:

```
Engine returns Version enum
     │
     │  extract_version() / version_to_u64()
     │  Strips variant tag → raw u64
     ▼
Client receives u64
```

| Primitive | Engine Returns | Variant | Executor Returns |
|-----------|---------------|---------|-----------------|
| KV | `Version::Txn(commit_id)` | Txn | `u64` |
| State | `Version::Counter(n)` | Counter | `u64` |
| JSON | `Version::Counter(n)` | Counter | `u64` |
| Event | `Version::Sequence(n)` | Sequence | `u64` |
| Vector | `Version::Counter(n)` | Counter | `u64` |
| Branch | `Version::Counter(n)` | Counter | `u64` |

The client receives the same `u64` type for all, with no indication of which version space it belongs to. This is documented in issue #930.

**Consistency within each primitive is correct**: All Counter types start at 1 and increment. All Sequence types start at 0 and increment. All Txn types use commit IDs.

## 5. Transaction vs Executor Consistency

Commands handled both by the executor (non-transactional) and by session dispatch (transactional) should return the same Output variant:

| Command | Executor Output | Session dispatch_in_txn Output | Match? |
|---------|----------------|-------------------------------|--------|
| `KvGet` | `Maybe(Option<Value>)` | `Maybe(Option<Value>)` | Yes |
| `KvPut` | `Version(u64)` | `Version(u64)` | Yes |
| `KvDelete` | `Bool(bool)` | `Bool(bool)` | Yes |
| `KvList` | `Keys(Vec<String>)` | `Keys(Vec<String>)` | Yes |
| `StateRead` | `Maybe(Option<Value>)` | `Maybe(Some/None)` | Yes |
| `StateSet` | `Version(u64)` | N/A (bypasses txn — #837) | N/A |
| `StateCas` | `MaybeVersion(Option<u64>)` | `MaybeVersion(Option<u64>)` | Yes |
| `StateInit` | `Version(u64)` | `Version(u64)` | Yes |
| `JsonGet` | `Maybe(Option<Value>)` | `Maybe(Some/None)` | Yes |
| `JsonSet` | `Version(u64)` | `Version(u64)` | Yes |
| `JsonDelete` | `Uint(0 or 1)` | `Uint(0 or 1)` | **Partial** |
| `EventAppend` | `Version(u64)` | `Version(u64)` | Yes |
| `EventRead` | `MaybeVersioned(...)` | `MaybeVersioned(...)` | Yes |
| `EventLen` | `Uint(u64)` | `Uint(u64)` | Yes |

JsonDelete has a subtle mismatch: the executor always returns `1` for non-root paths, while the session version checks deletion status. The session version is more correct.

## 6. Summary

| # | Finding | Severity | Type |
|---|---------|----------|------|
| 1 | Output docstring claims KvGet returns MaybeVersioned — actually returns Maybe | Medium | Documentation bug |
| 2 | KvGet, StateRead, JsonGet strip version metadata — breaks read-then-CAS workflow | Medium | API design gap |
| 3 | BranchGet returns two different Output variants (BranchInfoVersioned / Maybe) | Low | Inconsistent contract |
| 4 | JsonDelete returns different semantics for root vs non-root path | Low | Inconsistent contract |
| 5 | JsonGetv silently drops versions with deserialization errors | Medium | Silent data loss |
| 6 | 15 unused Output variants (9 dead, 6 test-only) — 36% of enum | Low | Dead code |
| 7 | VectorUpsert auto-creates collection silently with hardcoded metric (existing #932) | Low | Hidden side-effect |
| 8 | VectorSearch scores not range-validated | Low | Missing validation |

**Overall**: The contract mapping is largely consistent — most commands return the expected Output variant. The main issues are (a) three read commands stripping version info that clients need for CAS, (b) the Output docstring being factually wrong, and (c) JsonGetv silently dropping errors. The 15 unused Output variants indicate over-engineering in the type system.
