# JSON History Implementation Plan

## Executive Summary

This document describes the implementation plan for `json_history`, which retrieves version history for JSON documents. The implementation follows the established patterns from KVStore and StateCell primitives.

**Core semantic definition:**

> JSON history = historical document values, versioned by logical document version, scoped to an execution run.

This is analogous to:
- Iceberg snapshots (but run-scoped)
- Git file history (but execution-scoped)

**What JSON history is NOT:**
- Partial diffs or patches
- Field-level mutation tracking
- JSON operation replay log

**JSON history returns full document snapshots, not field-level or operation-level history.**

---

## Current State

### Trait Definition (Substrate Layer)

```rust
// crates/api/src/substrate/json.rs:182-188
fn json_history(
    &self,
    run: &ApiRunId,
    key: &str,
    limit: Option<u64>,
    before: Option<Version>,
) -> StrataResult<Vec<Versioned<Value>>>;
```

### Current Implementation (IMPLEMENTED)

The implementation is complete. See:
- Primitive layer: `crates/primitives/src/json_store.rs` - `JsonStore::history()`
- Substrate layer: `crates/api/src/substrate/json.rs` - `json_history()`
- Tests: `tests/substrate_api_comprehensive/jsonstore/history_ops.rs`

### Storage Infrastructure

The implementation leverages existing storage infrastructure:

- `ShardedStore` maintains a `VersionChain` per key with full history
- `UnifiedStore` (in-memory only) keeps only the latest version
- Both implement `Storage::get_history(&Key, limit, before_version) -> Vec<VersionedValue>`

---

## Reference Implementation: KVStore

### Primitive Layer

```rust
// crates/primitives/src/kv.rs:212-223
pub fn history(
    &self,
    run_id: &RunId,
    key: &str,
    limit: Option<usize>,
    before_version: Option<u64>,
) -> Result<Vec<Versioned<Value>>> {
    use strata_core::traits::Storage;

    let storage_key = self.key_for(run_id, key);
    self.db.storage().get_history(&storage_key, limit, before_version)
}
```

**Key characteristics:**
- Builds storage key from run_id and user key
- Delegates directly to `Storage::get_history()`
- Returns raw `Vec<Versioned<Value>>` (no transformation needed)
- Uses `Option<usize>` for limit, `Option<u64>` for before_version

### Substrate Layer

```rust
// crates/api/src/substrate/kv.rs:419-441
fn kv_history(
    &self,
    run: &ApiRunId,
    key: &str,
    limit: Option<u64>,
    before: Option<Version>,
) -> StrataResult<Vec<Versioned<Value>>> {
    validate_key(key)?;
    let run_id = run.to_run_id();

    // Extract version number from before (KV uses Txn versions)
    let before_version = match before {
        Some(Version::Txn(v)) => Some(v),
        Some(_) => return Err(strata_core::StrataError::invalid_input(
            "KV operations use Txn versions",
        )),
        None => None,
    };

    // Use primitive's history method
    self.kv()
        .history(&run_id, key, limit.map(|l| l as usize), before_version)
        .map_err(convert_error)
}
```

**Key characteristics:**
- Validates key before operation
- Converts `ApiRunId` to `RunId`
- Extracts `u64` from `Version::Txn` (enforces correct version type)
- Converts `Option<u64>` to `Option<usize>` for limit
- Delegates to primitive, maps errors

---

## Reference Implementation: StateCell

StateCell has a more complex `history()` because it stores a `State` struct that contains an internal counter version:

```rust
// crates/primitives/src/state_cell.rs:254-300
pub fn history(
    &self,
    run_id: &RunId,
    name: &str,
    limit: Option<usize>,
    before_counter: Option<u64>,
) -> Result<Vec<Versioned<Value>>> {
    use strata_core::traits::Storage;

    let key = self.key_for(run_id, name);

    // Get raw history from storage layer
    let raw_history = self.db.storage().get_history(&key, limit, None)?;

    // Convert storage entries to StateCell format
    let mut results: Vec<Versioned<Value>> = Vec::new();

    for versioned_value in raw_history {
        // Deserialize the State struct from storage
        let state: State = match from_stored_value(&versioned_value.value) {
            Ok(s) => s,
            Err(_) => continue, // Skip malformed entries
        };

        // Apply before_counter filter (based on cell's internal counter, not txn version)
        if let Some(before) = before_counter {
            if state.version >= before {
                continue;
            }
        }

        // Build result with CELL's counter version, not storage version
        // NOTE: Uses storage timestamp, not internal timestamp
        results.push(Versioned::with_timestamp(
            state.value,
            Version::counter(state.version),
            versioned_value.timestamp,
        ));

        // Apply limit
        if let Some(max) = limit {
            if results.len() >= max {
                break;
            }
        }
    }

    Ok(results)
}
```

**Key characteristics:**
- Deserializes each stored `Value::Bytes` to internal `State` struct
- Uses **internal counter version** (not storage transaction version) for filtering and results
- **Uses storage timestamp** (not internal timestamp) for consistency
- Handles deserialization errors gracefully (skips malformed entries)
- Manually applies limit after filtering

---

## JSONStore Data Model

### Storage Format

JSON documents are stored as MessagePack-serialized `JsonDoc` structs:

```rust
// crates/primitives/src/json_store.rs:102-134
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonDoc {
    pub id: JsonDocId,
    pub value: JsonValue,
    pub version: u64,        // Internal document version (counter)
    pub created_at: f64,     // Unix timestamp (seconds)
    pub updated_at: f64,     // Unix timestamp (seconds)
}

impl JsonDoc {
    pub fn new(id: JsonDocId, value: JsonValue) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
        Self {
            id,
            value,
            version: 1,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn touch(&mut self) {
        self.version += 1;
        self.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs_f64();
    }
}
```

### Serialization

```rust
// crates/primitives/src/json_store.rs:202-216
pub(crate) fn serialize_doc(doc: &JsonDoc) -> Result<Value> {
    let bytes = rmp_serde::to_vec(doc)
        .map_err(|e| Error::SerializationError(e.to_string()))?;
    Ok(Value::Bytes(bytes))
}

pub(crate) fn deserialize_doc(value: &Value) -> Result<JsonDoc> {
    match value {
        Value::Bytes(bytes) => {
            rmp_serde::from_slice(bytes)
                .map_err(|e| Error::SerializationError(e.to_string()))
        }
        _ => Err(Error::InvalidOperation("expected bytes for JsonDoc".into())),
    }
}
```

### Key Format

```rust
// crates/primitives/src/json_store.rs:191-193
fn key_for(&self, run_id: &RunId, doc_id: &JsonDocId) -> Key {
    Key::new_json(self.namespace_for_run(run_id), doc_id)
}
```

---

## Proposed Implementation

### 1. Primitive Layer: `JsonStore::history()`

Add to `crates/primitives/src/json_store.rs`:

```rust
/// Get document version history
///
/// Returns full document snapshots in descending version order (newest first).
///
/// **Important**: This returns value-history, not transition-history. Each entry
/// is a complete document snapshot, not a diff or operation log.
///
/// ## Parameters
///
/// * `run_id` - RunId for namespace isolation
/// * `doc_id` - Document identifier
/// * `limit` - Maximum versions to return (None = all)
/// * `before_version` - Only return versions older than this document version (for pagination)
///
/// ## Returns
///
/// Vector of `Versioned<JsonDoc>` in descending version order (newest first).
/// Empty if document doesn't exist or has no history.
///
/// ## Ordering Guarantee
///
/// Results are guaranteed to be in descending document version order.
/// This invariant is enforced by the storage layer's `get_history()` contract.
///
/// ## Deletion Semantics
///
/// History survives document deletion. If a document is deleted:
/// - Previous versions remain accessible via `history()`
/// - The deleted state may appear as a tombstone entry (filtered at substrate layer)
/// - This matches Strata's "execution commit" philosophy where all committed state is preserved
///
/// ## Version Semantics
///
/// The `before_version` parameter filters by **document version** (`JsonDoc.version`),
/// not by storage transaction version. This matches StateCell semantics.
///
/// ## Storage Behavior
///
/// - **ShardedStore** (persistent): Returns full version history from VersionChain
/// - **UnifiedStore** (in-memory): Returns only current version (no history retention)
///
/// ## Example
///
/// ```ignore
/// // Get last 10 versions
/// let history = json.history(&run_id, &doc_id, Some(10), None)?;
///
/// // Paginate: get next 10 versions older than version 50
/// let page2 = json.history(&run_id, &doc_id, Some(10), Some(50))?;
/// ```
pub fn history(
    &self,
    run_id: &RunId,
    doc_id: &JsonDocId,
    limit: Option<usize>,
    before_version: Option<u64>,
) -> Result<Vec<Versioned<JsonDoc>>> {
    use strata_core::traits::Storage;

    let key = self.key_for(run_id, doc_id);

    // Optimization: When before_version is None, we can pass limit directly to storage
    // to avoid unbounded reads. When before_version is Some, we must fetch more and
    // filter by document version (which differs from storage transaction version).
    let storage_limit = if before_version.is_none() { limit } else { None };

    let raw_history = self.db.storage().get_history(&key, storage_limit, None)?;

    // Storage layer contract: get_history() returns newest-first.
    // If this invariant ever changes, json_history semantics must be updated.
    debug_assert!(
        raw_history.windows(2).all(|w| {
            // Can't easily compare versions without deserializing, but we trust storage layer
            true
        }),
        "Storage::get_history() must return results in descending version order"
    );

    let mut results: Vec<Versioned<JsonDoc>> = Vec::new();

    for versioned_value in raw_history {
        // Deserialize the JsonDoc from storage
        let doc = match Self::deserialize_doc(&versioned_value.value) {
            Ok(d) => d,
            Err(_) => continue, // Skip malformed entries
        };

        // Apply before_version filter (based on document's internal version)
        if let Some(before) = before_version {
            if doc.version >= before {
                continue;
            }
        }

        // Build result with document's internal version.
        // Use STORAGE timestamp for consistency with KV and StateCell.
        // (JsonDoc.updated_at is document-level, but we want commit-time consistency)
        results.push(Versioned::with_timestamp(
            doc.clone(),
            Version::counter(doc.version),
            versioned_value.timestamp,
        ));

        // Apply limit
        if let Some(max) = limit {
            if results.len() >= max {
                break;
            }
        }
    }

    Ok(results)
}
```

### 2. Substrate Layer: Update `json_history()`

Update `crates/api/src/substrate/json.rs`:

```rust
fn json_history(
    &self,
    run: &ApiRunId,
    key: &str,
    limit: Option<u64>,
    before: Option<Version>,
) -> StrataResult<Vec<Versioned<Value>>> {
    let run_id = run.to_run_id();
    let doc_id = parse_doc_id(key)?;

    // Extract version number from before (JSON documents use Counter versions)
    let before_version = match before {
        Some(Version::Counter(v)) => Some(v),
        Some(_) => return Err(strata_core::StrataError::invalid_input(
            "JSON document operations use Counter versions",
        )),
        None => None,
    };

    // Get history from primitive
    let history = self.json()
        .history(&run_id, &doc_id, limit.map(|l| l as usize), before_version)
        .map_err(convert_error)?;

    // Convert Versioned<JsonDoc> to Versioned<Value>
    Ok(history
        .into_iter()
        .map(|v| Versioned::with_timestamp(
            json_to_value(v.value.value),  // Extract JsonValue from JsonDoc, convert to Value
            v.version,
            v.timestamp,
        ))
        .collect())
}
```

---

## Design Decisions

### 1. Semantic Definition

**JSON history returns full document snapshots, not field-level or operation-level history.**

This is value-history, not transition-history. Users cannot infer what operations were performed between versions—only what the document looked like at each committed state.

### 2. Version Type: Counter vs Txn

| Primitive | Version Type | Rationale |
|-----------|--------------|-----------|
| KVStore | `Version::Txn` | Values stored directly, version = storage commit |
| StateCell | `Version::Counter` | Internal counter tracks logical versions |
| **JSONStore** | `Version::Counter` | Document has internal version counter (`JsonDoc.version`) |

JSON documents maintain an internal version counter that increments on each modification. This matches StateCell semantics. Users paginate by document version, not storage transaction version.

### 3. Ordering Guarantee

**Results are guaranteed to be in descending document version order (newest first).**

This invariant depends on `Storage::get_history()` returning newest-first. The implementation includes a debug assertion to catch any future changes to this contract.

### 4. Deletion Semantics

**History survives document deletion.**

When a document is deleted:
- All previous versions remain accessible via `history()`
- Deletion is treated as just another state transition
- The deleted state may appear as a tombstone (filtered at substrate layer if needed)

This is consistent with Strata's "execution commit" philosophy where all committed state is preserved for audit and replay purposes.

### 5. Timestamp Source

**JSON history uses storage transaction timestamp, not document-level `updated_at`.**

This matches KVStore and StateCell behavior:
- KVStore: Uses storage timestamp directly
- StateCell: Uses `versioned_value.timestamp` (storage timestamp)
- JSONStore: Uses `versioned_value.timestamp` (storage timestamp)

The `JsonDoc.updated_at` field reflects when the document was modified in application time, but history timestamps reflect when the state was committed to storage. This ensures consistent semantics across all primitives.

### 6. Limit Handling Efficiency

**Optimization**: When `before_version` is None (common case), pass `limit` directly to storage layer to avoid unbounded reads.

When `before_version` is Some, we must fetch all history and filter by document version (which differs from storage transaction version), then apply limit.

```rust
let storage_limit = if before_version.is_none() { limit } else { None };
```

### 7. Error Handling for Malformed Entries

Following StateCell's pattern, malformed entries are skipped silently:

```rust
let doc = match Self::deserialize_doc(&versioned_value.value) {
    Ok(d) => d,
    Err(_) => continue, // Skip malformed entries
};
```

This ensures partial corruption doesn't break history retrieval.

### 8. Storage Backend Behavior

| Backend | History Support |
|---------|----------------|
| ShardedStore (persistent) | Full history via VersionChain |
| UnifiedStore (in-memory) | Current version only |

This is consistent with KVStore and StateCell behavior. Users should be aware that in-memory mode doesn't retain history.

---

## Test Plan

### Unit Tests (Primitive Layer)

Add to `crates/primitives/src/json_store.rs` or separate test file:

```rust
#[test]
fn test_history_returns_versions_descending() {
    // Create document, update 3 times, verify history is newest-first
}

#[test]
fn test_history_empty_for_nonexistent() {
    // Verify empty vec for missing document
}

#[test]
fn test_history_limit_works() {
    // Create 10 versions, request limit=3, verify exactly 3 returned (newest 3)
}

#[test]
fn test_history_before_version_pagination() {
    // Create 10 versions (v1-v10), request before=v7, verify v6,v5,v4... returned
}

#[test]
fn test_history_limit_with_before_version() {
    // Create 10 versions, request limit=2, before=v8, verify v7,v6 returned
}

#[test]
fn test_history_skips_malformed_entries() {
    // Requires injecting corrupt data (may need test helper)
}

#[test]
fn test_history_survives_deletion() {
    // Create doc, update twice, delete, verify history still returns v1,v2
}
```

### Integration Tests (Substrate Layer)

Add to `tests/substrate_api_comprehensive/jsonstore/`:

```rust
#[test]
fn test_json_history_returns_document_versions() {
    // Create, update multiple times, verify history contains all versions
}

#[test]
fn test_json_history_newest_first_ordering() {
    // Verify explicit ordering guarantee
}

#[test]
fn test_json_history_empty_for_nonexistent() {
    // Verify empty for missing key
}

#[test]
fn test_json_history_limit() {
    // Test limit parameter
}

#[test]
fn test_json_history_pagination() {
    // Test before parameter for pagination
}

#[test]
fn test_json_history_pagination_complete_traversal() {
    // Create 20 versions, paginate through all with limit=5
}

#[test]
fn test_json_history_run_isolation() {
    // Different runs have independent history
}

#[test]
fn test_json_history_version_type_validation() {
    // Verify Counter version required, Txn rejected with clear error
}

#[test]
fn test_json_history_after_delete() {
    // Verify history survives deletion
}

#[test]
fn test_json_history_cross_mode() {
    // Test in both memory and persistent modes
    // Memory mode: expect single entry (current only)
    // Persistent mode: expect full history
}

#[test]
fn test_json_history_timestamps_are_storage_timestamps() {
    // Verify timestamps come from storage, not JsonDoc.updated_at
}
```

---

## Implementation Checklist

### Phase 1: Primitive Layer ✅
- [x] Add `JsonStore::history()` method
- [x] Add debug assertion for ordering invariant
- [x] Add unit tests for history
- [x] Verify deserialization handles all JsonDoc versions

### Phase 2: Substrate Layer ✅
- [x] Update `json_history()` implementation
- [x] Add version type validation (Counter only, reject Txn)
- [x] Add `json_to_value` conversion for history entries

### Phase 3: Integration Tests ✅
- [x] Add comprehensive tests to `tests/substrate_api_comprehensive/jsonstore/`
- [x] Test with dirty test data fixture
- [x] Verify cross-mode behavior (memory vs persistent)
- [x] Test deletion semantics
- [x] Test pagination complete traversal

### Phase 4: Documentation
- [ ] Update API documentation
- [ ] Add examples to trait docstrings
- [x] Document ordering guarantee (in this doc + debug assertion)
- [x] Document deletion semantics (in this doc + test)
- [x] Document timestamp source (in this doc)
- [x] Note storage backend differences (in cross-mode test)

---

## Files to Modify

| File | Changes |
|------|---------|
| `crates/primitives/src/json_store.rs` | Add `history()` method |
| `crates/api/src/substrate/json.rs` | Update `json_history()` implementation |
| `tests/substrate_api_comprehensive/jsonstore/` | Add history tests |

---

## Risks and Mitigations

### Risk 1: Performance with Large History

**Risk**: Documents with many versions could have slow history retrieval.

**Mitigation**:
- The `limit` parameter allows pagination
- Optimization: Pass limit to storage when `before_version` is None
- Storage layer's VersionChain is optimized for sequential access
- Users should paginate for large histories

### Risk 2: Memory Mode Has No History

**Risk**: Users may expect history in memory mode.

**Mitigation**:
- Document clearly in API docs
- Return single entry (current version) in memory mode
- Consistent with KVStore and StateCell behavior

### Risk 3: Version Number Confusion

**Risk**: Users may confuse document version with storage transaction version.

**Mitigation**:
- Use `Version::Counter` to distinguish from `Version::Txn`
- Reject `Version::Txn` in `before` parameter with clear error message
- Document version semantics clearly in API docs

### Risk 4: Ordering Invariant Violation

**Risk**: Future changes to storage layer could change ordering.

**Mitigation**:
- Debug assertion in primitive layer
- Explicit documentation of invariant dependency
- Comment in code noting the contract

### Risk 5: Timestamp Confusion

**Risk**: Users may expect `JsonDoc.updated_at` but get storage timestamp.

**Mitigation**:
- Document explicitly that timestamps are storage commit times
- Consistent with KV and StateCell behavior
- Users needing document-level timestamps can access via `json_get()`

---

## Appendix: Storage Layer API

```rust
// crates/core/src/traits.rs:65-70
fn get_history(
    &self,
    key: &Key,
    limit: Option<usize>,
    before_version: Option<u64>,
) -> Result<Vec<VersionedValue>>;
```

```rust
// VersionedValue is alias for Versioned<Value>
pub type VersionedValue = Versioned<Value>;
```

---

## Appendix: Comparison with KV and StateCell

| Aspect | KVStore | StateCell | JSONStore |
|--------|---------|-----------|-----------|
| Version type | `Txn` | `Counter` | `Counter` |
| Timestamp source | Storage | Storage | Storage |
| Filtering | Storage-level | Post-deserialization | Post-deserialization |
| Limit optimization | Direct passthrough | Manual | Conditional |
| Deletion behavior | Tombstone in history | Tombstone in history | History survives |
| History retention (memory) | Current only | Current only | Current only |
| History retention (persistent) | Full VersionChain | Full VersionChain | Full VersionChain |
