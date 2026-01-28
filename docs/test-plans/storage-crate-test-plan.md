# Storage Crate Test Plan

## Overview

This document outlines the test improvement plan for `strata-storage`, the in-memory MVCC storage layer.

## Current State

### Unit Tests (crates/storage/src/) - KEEP
- **314 tests** across 13 modules
- Quality is strong with adversarial coverage already present
- Key strengths:
  - MVCC adversarial tests in `sharded.rs` (version wrapping, concurrent snapshot isolation)
  - CRC corruption detection in `format/wal_record.rs` and `disk_snapshot/reader.rs`
  - Tombstone lifecycle in `compaction/tombstone.rs`
  - Reference model for crash testing in `testing/`

### Integration Tests (tests/storage/) - DELETE & REWRITE
- **446 compilation errors** across 18 files
- Missing `common` module
- Uses wrong/outdated APIs
- Files to delete:
  - `conformance.rs` (90KB)
  - `snapshot_invariants.rs` (58KB)
  - `heap_*.rs` (5 files)
  - `vectorid_*.rs`, `run_isolation.rs`, etc.

## Architecture Summary

### Key Components

```
ShardedStore (DashMap<RunId, Shard>)
  └─ Shard (FxHashMap<Key, VersionChain>)
      └─ VersionChain (VecDeque<StoredValue>, newest-first)
          └─ StoredValue(VersionedValue, Option<TTL>)
```

### Critical Invariants

1. **MVCC Invariants**
   - Version chains store newest-first
   - `get_at_version(v)` finds value <= v
   - Snapshots see store state at captured version
   - Concurrent snapshots don't block each other
   - Expired values filtered at read time
   - Tombstones preserved in chains but not returned

2. **Sharding Invariants**
   - Per-run isolation (no cross-run contention)
   - O(1) key lookups within shard
   - Lock-free reads via DashMap

3. **Compaction Invariants**
   - Version IDs never change
   - Deterministic (same input → same output)
   - Retained data read results unchanged
   - WAL-only removes only segments covered by snapshot watermark

4. **Format Invariants**
   - Magic bytes: "SNAP" for snapshots, "STRA" for WAL
   - CRC32 validation on all records
   - Watermark is inclusive

5. **Retention Invariants**
   - GC never removes the latest version
   - KeepFor uses timestamp >= cutoff
   - Composite policies fall back to default

## Test Plan

### Phase 1: Delete Broken Integration Tests

Delete all files in `tests/storage/`:
- `btreemap_source.rs`
- `conformance.rs`
- `dimension.rs`
- `heap_delete.rs`, `heap_free_slot_reuse.rs`, `heap_get.rs`, `heap_insert.rs`, `heap_iteration.rs`
- `heap_kv_consistency.rs`
- `main.rs`
- `metric.rs`
- `reconstructibility.rs`
- `run_isolation.rs`
- `snapshot_invariants.rs`
- `snapshot_wal_equiv.rs`
- `vectorid_never_reused.rs`
- `vectorid_stable.rs`

### Phase 2: Create New Integration Test Structure

```
tests/storage/
├── main.rs
├── mvcc_invariants.rs       # MVCC semantics
├── snapshot_isolation.rs    # Snapshot isolation guarantees
├── compaction.rs            # Compaction correctness
├── retention_policy.rs      # Retention enforcement
├── run_isolation.rs         # Per-run isolation
├── format_validation.rs     # WAL/Snapshot format
└── stress.rs                # Stress tests (#[ignore])
```

### Phase 3: Test Implementation

#### 3.1 mvcc_invariants.rs (~15 tests)

**Version Chain Semantics:**
```rust
#[test] fn version_chain_stores_newest_first()
#[test] fn get_at_version_returns_value_lte_version()
#[test] fn get_at_version_before_first_returns_none()
#[test] fn version_chain_preserves_all_versions()
```

**TTL Semantics:**
```rust
#[test] fn expired_values_filtered_at_read_time()
#[test] fn ttl_expiration_uses_creation_timestamp()
#[test] fn future_timestamp_never_expires()  // clock backward protection
```

**Tombstone Semantics:**
```rust
#[test] fn tombstone_preserves_snapshot_isolation()
#[test] fn tombstone_not_returned_to_user()
#[test] fn tombstone_at_higher_version_than_value()
```

**Version Counter:**
```rust
#[test] fn version_counter_monotonically_increases()
#[test] fn version_counter_wraps_at_u64_max()
#[test] fn concurrent_increments_are_unique()
```

#### 3.2 snapshot_isolation.rs (~12 tests)

**Snapshot Acquisition:**
```rust
#[test] fn snapshot_captures_current_version()
#[test] fn snapshot_acquisition_is_o1()  // timing test
#[test] fn multiple_snapshots_independent()
```

**Isolation Guarantees:**
```rust
#[test] fn snapshot_ignores_concurrent_writes()
#[test] fn snapshot_sees_pre_delete_value()
#[test] fn repeated_reads_return_same_value()
#[test] fn multi_key_consistency_within_snapshot()
```

**Concurrent Access:**
```rust
#[test] fn concurrent_readers_dont_block()
#[test] fn snapshot_survives_store_modifications()
#[test] fn snapshot_cache_race_condition()
```

#### 3.3 compaction.rs (~10 tests)

**WAL-Only Compaction:**
```rust
#[test] fn wal_only_removes_covered_segments()
#[test] fn wal_only_never_removes_active_segment()
#[test] fn wal_only_preserves_all_version_history()
```

**Version ID Stability:**
```rust
#[test] fn version_ids_never_change_after_compaction()
#[test] fn compaction_is_deterministic()
```

**Tombstone Cleanup:**
```rust
#[test] fn tombstone_cleanup_respects_retention()
#[test] fn tombstone_cleanup_before_cutoff()
```

#### 3.4 retention_policy.rs (~10 tests)

**Policy Types:**
```rust
#[test] fn keep_all_retains_everything()
#[test] fn keep_last_n_retains_n_versions()
#[test] fn keep_for_duration_uses_timestamp_gte()
#[test] fn composite_policy_uses_per_type_overrides()
#[test] fn composite_falls_back_to_default()
```

**Safety Invariants:**
```rust
#[test] fn gc_never_removes_latest_version()
#[test] fn policy_serialization_roundtrip()
#[test] fn zero_keep_last_panics()
#[test] fn zero_keep_for_panics()
```

#### 3.5 run_isolation.rs (~8 tests)

**Isolation:**
```rust
#[test] fn different_runs_never_share_locks()
#[test] fn clear_run_only_affects_target_run()
#[test] fn run_index_tracks_keys_per_run()
#[test] fn type_index_tracks_keys_per_type()
```

**Cross-Run:**
```rust
#[test] fn concurrent_writes_to_different_runs()
#[test] fn snapshot_only_sees_own_run()
```

#### 3.6 format_validation.rs (~10 tests)

**WAL Format:**
```rust
#[test] fn wal_segment_has_correct_magic()
#[test] fn wal_record_crc_detects_corruption()
#[test] fn wal_record_roundtrip()
#[test] fn wal_segment_header_validation()
```

**Snapshot Format:**
```rust
#[test] fn snapshot_has_correct_magic()
#[test] fn snapshot_crc_detects_corruption()
#[test] fn snapshot_watermark_is_inclusive()
#[test] fn snapshot_section_types_valid()
```

#### 3.7 stress.rs (~6 tests, all #[ignore])

```rust
#[test] #[ignore] fn stress_concurrent_writers_readers()
#[test] #[ignore] fn stress_rapid_snapshot_creation()
#[test] #[ignore] fn stress_version_chain_growth()
#[test] #[ignore] fn stress_ttl_expiration_cleanup()
#[test] #[ignore] fn stress_compaction_under_load()
#[test] #[ignore] fn stress_many_runs_concurrent()
```

## Implementation Notes

### Test Utilities Needed

The tests will use `tests/common/mod.rs` with:
- `ShardedStore::new()` for in-memory store
- `Key::new_kv()`, `Key::new_event()` etc. for key creation
- `Value::Int()`, `Value::String()` for values
- `Timestamp::from_micros()` for timestamps
- `RunId::new()` for run IDs

### No Database Integration

These tests focus on the storage layer directly:
- Use `ShardedStore` directly, not `Database`
- No WAL/snapshot file I/O in most tests
- Format tests use `tempfile` for isolated I/O

### Assertion Patterns

```rust
// MVCC: version ordering
assert!(chain[0].version > chain[1].version, "Newest first");

// Snapshot isolation
let before = snapshot.get(&key);
store.put(key, new_value);
let after = snapshot.get(&key);
assert_eq!(before, after, "Snapshot should not see new write");

// TTL expiration
assert!(stored.is_expired(), "Should be expired");
assert!(store.get(&key).is_none(), "Expired not returned");
```

## Success Criteria

1. All 314 unit tests continue to pass
2. New integration tests compile and pass
3. Coverage of all 5 invariant categories
4. Stress tests complete without panics
5. No false positives from timing-sensitive tests

## Verification

```bash
# Unit tests
cargo test -p strata-storage

# Integration tests
cargo test --test storage

# Stress tests (optional)
cargo test --test storage stress -- --ignored
```
