# Durability Crate Test Plan

## Current Inventory

### Unit Tests (crates/durability/src/)

| Module | File | Tests | Quality |
|--------|------|-------|---------|
| WAL entries & file ops | wal.rs | 40 | Good - real file I/O, durability modes, corruption detection |
| Entry encoding/decoding | encoding.rs | 20 | Excellent - CRC adversarial, regression tests for issue #51 |
| WAL replay & recovery | recovery.rs | 48 | Excellent - interleaved txns, JSON recovery, filtering, callbacks |
| Snapshot writer/reader | snapshot.rs | 21 | Good - atomic writes, CRC, all primitives roundtrip |
| Snapshot format types | snapshot_types.rs | 22 | Good - header validation, negative cases, constants |
| Run bundle types | run_bundle/types.rs | 9 | Adequate - manifest, checksums, terminal states |
| Run bundle errors | run_bundle/error.rs | 3 | Minimal - display, constructors, io conversion |

**Total unit tests: ~163**

### Integration Tests (tests/durability/)

| File | Tests | Quality | Notes |
|------|-------|---------|-------|
| wal_invariants.rs | 28 | Excellent | 5 core WAL invariants |
| recovery.rs | 25+ | Good | Overlaps with wal_invariants.rs |
| recovery_comprehensive.rs | 20 | Good | Large state recovery |
| crash_recovery.rs | 18 | Good | Various crash points |
| crash_scenarios.rs | ~8 | Good | Simulated crashes |
| crash_mid_delete.rs | ~4 | Good | Mid-operation crashes |
| crash_mid_upsert.rs | ~4 | Good | Mid-operation crashes |
| crash_collection_create.rs | ~3 | Good | Collection lifecycle crash |
| crash_collection_delete.rs | ~3 | Good | Collection lifecycle crash |
| cross_primitive_atomicity.rs | ~6 | Good | Multi-primitive transactions |
| wal_replay.rs | 15 | Good | Replay correctness |
| wal_crc_validation.rs | 6 | Good | CRC validation with real files |
| wal_transaction_framing.rs | 6 | Good | Transaction atomicity |
| recovery_determinism.rs | ~8 | Good | Deterministic recovery |
| recovery_idempotent.rs | ~8 | Good | Idempotent replay |
| recovery_no_drop_committed.rs | ~8 | Good | Committed data preserved |
| recovery_no_invent.rs | ~8 | Good | No invented data |
| recovery_may_drop_uncommitted.rs | ~8 | Good | Uncommitted discarded |
| recovery_prefix.rs | ~8 | Good | Prefix consistency |
| replay_determinism.rs | ~6 | Good | Replay determinism |
| replay_idempotent.rs | ~6 | Good | Replay idempotency |
| replay_pure_function.rs | ~6 | Good | Pure function property |
| replay_derived_view.rs | ~6 | Good | Derived view property |
| replay_ephemeral.rs | ~6 | Good | Ephemeral property |
| replay_side_effect.rs | ~6 | Good | Side-effect free |
| snapshot_semantics.rs | 16 | Good | Snapshot behavior |
| snapshot_atomic_write.rs | 5 | Good | Atomic rename pattern |
| mode_equivalence.rs | 3 | Good | Cross-mode equivalence |
| mode_recovery.rs | 3 | Good | Per-mode recovery |
| storage_stabilization.rs | 10 | Good | Storage operations |
| run_lifecycle.rs | ~6 | Good | Run lifecycle |
| m8_wal_write.rs | 3 | Good | WAL write behavior |
| m8_wal_replay.rs | 2 | Good | Replay state identity |
| m8_wal_replay_determinism.rs | 2 | Good | Replay determinism |
| m8_snapshot_format.rs | 2 | Good | Snapshot persistence |
| m8_snapshot_free_slots.rs | 1 | Good | Free slot preservation |
| m8_snapshot_next_id.rs | 1 | Good | Next ID persistence |
| m8_snapshot_recovery.rs | 2 | Good | Snapshot recovery |
| m8_snapshot_wal_combo.rs | 2 | Good | Snapshot + WAL combo |
| m8_wal_entry_format.rs | 3 | Good | WAL entry byte codes |
| **issue_020_buffered_defaults.rs** | 2 | **Delete** | Both tests empty stubs |
| **issue_012_snapshot_traits.rs** | 2 | **Delete** | Both tests empty stubs |
| **issue_006_wal_entry_0x23.rs** | 3 | **Delete** | 2/3 have no assertions |
| **issue_002_replay_api_exposure.rs** | 7 | **Delete** | All test workarounds for missing API |
| **buffered_flush.rs** | 3 | **Partial delete** | 2/3 have no assertions |
| **snapshot_crc.rs** | 6 | **Partial delete** | 3/6 test stdlib, 1 has no assertions |
| **snapshot_discovery.rs** | 6 | **Partial delete** | 4/6 are trivial (assert len >= 0) |
| **snapshot_format.rs** | 6 | **Partial delete** | 4/6 are trivial constant checks |
| **issue_019_durability_handlers.rs** | 3 | **Partial delete** | 2/3 have no assertions |
| **issue_008_buffered_thread_startup.rs** | 3 | **Partial delete** | 2/3 weak (only .is_some()) |
| **issue_004_snapshot_header_size.rs** | 6 | **Partial delete** | 3/6 assert constants == constants |
| **stress.rs** | 10 | **Fix or delete** | All #[ignore], 5 have no assertions |

### Crate Integration Tests (crates/durability/tests/)

| File | Tests | Quality | Notes |
|------|-------|---------|-------|
| adversarial_tests.rs | ~15 | Good | Concurrent WAL, recovery edge cases |
| corruption_test.rs | ~8 | Good | CRC, truncation, partial writes |
| corruption_simulation_test.rs | ~10 | Good | Bit flips, corruption detection |
| incomplete_txn_test.rs | ~8 | Good | Incomplete transaction handling |
| recovery_invariants.rs | ~15 | Good | Core recovery invariants |
| replay_test.rs | ~12 | Good | WAL replay behavior |

---

## Phase 1: Delete Empty/Vanity Integration Tests

### Files to delete entirely

1. **`tests/durability/issue_020_buffered_defaults.rs`**
   - `test_default_buffered_params()`: empty body, just `let _test_db = TestDb::new()`
   - `test_custom_buffered_params()`: empty body, all commented-out code

2. **`tests/durability/issue_012_snapshot_traits.rs`**
   - `test_primitive_storage_ext_canonical()`: no assertions, just calls `.flush()`
   - `test_consistent_snapshot_interface()`: completely empty body

3. **`tests/durability/issue_006_wal_entry_0x23.rs`**
   - `test_wal_entry_0x23_semantics()`: only asserts `0x23 == 0x23` (constant identity)
   - `test_json_patch_logged_to_wal()`: calls operations but has no verification
   - `test_json_destroy_wal_entry()`: creates/destroys document, verifies nothing

4. **`tests/durability/issue_002_replay_api_exposure.rs`**
   - All 7 tests are workarounds for an API that doesn't exist. They test basic DB operations (put/get) with `.is_some()` checks, not replay API exposure. None test what they claim.

### Tests to delete from existing files

5. **`tests/durability/buffered_flush.rs`** - delete 2 of 3:
   - Keep: `test_explicit_flush()` (valid - puts, flushes, reopens, asserts)
   - Delete: `test_auto_flush_after_interval()` (no assertions, only TODO comment)
   - Delete: `test_flush_on_shutdown()` (no assertions, only comment)

6. **`tests/durability/snapshot_crc.rs`** - delete 4 of 6:
   - Keep: `test_snapshot_data_integrity()` (captures state, reopens, asserts equal)
   - Keep: `test_large_snapshot_integrity()` (1000 entries, reopens, asserts count)
   - Delete: `test_snapshot_corruption_detection_concept()` (corrupts file but has no assertion after)
   - Delete: `test_crc_catches_single_bit_error()` (tests `std::collections::hash_map::DefaultHasher`)
   - Delete: `test_crc_consistency()` (tests `DefaultHasher` determinism)
   - Delete: `test_snapshot_corruption_detection_concept()` (no assertion)

7. **`tests/durability/snapshot_discovery.rs`** - delete 4 of 6:
   - Keep: `test_newer_snapshots_preferred()` (real state hash comparison)
   - Keep: whichever test has real assertions about snapshot ordering
   - Delete: `test_discover_snapshots()` (asserts `snapshots.len() >= 0`, always true)
   - Delete: `test_snapshot_count()` (asserts `count >= 0`, always true)
   - Delete: `test_empty_snapshot_directory()` (trivial: empty dir has 0 snapshots)
   - Delete: `test_nonexistent_snapshot_directory()` (trivial: nonexistent dir has 0 snapshots)

8. **`tests/durability/snapshot_format.rs`** - delete 4 of 6:
   - Keep: `test_snapshot_magic_number()` (reads actual snapshot, asserts magic bytes)
   - Keep: whichever test verifies actual snapshot content
   - Delete: `test_snapshot_version_valid()` (asserts `SNAPSHOT_VERSION_1 >= 1`)
   - Delete: `test_snapshot_header_size()` (asserts constant > 0 && < 1024)
   - Delete: `test_snapshot_directory_created()` (asserts path string ends with "snapshots")
   - Delete: `test_large_data_snapshot_concept()` (only checks in-memory count, not snapshot)

9. **`tests/durability/issue_019_durability_handlers.rs`** - delete 2 of 3:
   - Keep: `test_strict_mode()` (puts, flushes, reopens, asserts data survives)
   - Delete: `test_in_memory_mode()` (no assertions, just `let _wal_dir`)
   - Delete: `test_buffered_mode()` (only calls `.flush()`, doesn't verify data persisted)

10. **`tests/durability/issue_008_buffered_thread_startup.rs`** - delete 2 of 3:
    - Keep: `test_buffered_no_data_loss()` (puts, flushes, reopens, asserts data present)
    - Delete: `test_buffered_auto_starts_flush_thread()` (only checks `.is_some()`, doesn't verify threading)
    - Delete: `test_buffered_flush_interval()` (only calls `assert_db_healthy()`, doesn't test interval)

11. **`tests/durability/issue_004_snapshot_header_size.rs`** - delete 3 of 6:
    - Keep: `test_prim_count_at_correct_offset()` (reads actual snapshot, asserts magic)
    - Keep: `test_read_snapshot_header_fields()` (reads actual snapshot, verifies fields)
    - Keep: `test_snapshot_version_field()` (reads actual snapshot, verifies version == 1)
    - Delete: `test_snapshot_header_size_matches_spec()` (asserts constant == constant)
    - Delete: `test_header_field_offsets()` (mathematical offset calc, not testing implementation)
    - Delete: `test_snapshot_validation_minimum_size()` (asserts constant == 43)

12. **`tests/durability/stress.rs`** - delete 5 of 10:
    - Keep (un-ignore): `stress_large_wal_recovery()`, `stress_concurrent_writes()`, `stress_concurrent_reads()`, `stress_large_values()`, `stress_recovery_after_churn()`
    - Delete: `stress_many_small_writes()` (only prints timing, no assertions)
    - Delete: `stress_many_runs()` (no assertions)
    - Delete: `stress_mixed_operations()` (no assertions)
    - Delete: `stress_sustained_load()` (only prints statistics)
    - Delete: `stress_concurrent_crash_simulation()` (only prints recovery counts)

### Summary of Phase 1

- **4 files deleted entirely** (14 empty/vanity tests)
- **~25 individual tests deleted** from 8 files
- **~39 tests removed total**
- **0 behavioral coverage lost** (none of these tested real behavior)

Remove the mod declarations from `tests/durability/main.rs` for deleted files.

---

## Phase 2: Strengthen Unit Tests in crates/durability/src/

### 2A: wal.rs - Add adversarial tests

The existing 40 unit tests cover happy paths well. Missing adversarial coverage:

**Add to wal.rs tests (~12 tests):**

1. `test_append_empty_wal_entry` - Write with empty key and empty value, verify roundtrip
2. `test_append_entry_with_max_u64_version` - Version u64::MAX, verify preserved
3. `test_append_entry_with_max_u64_txn_id` - TxnId u64::MAX in BeginTxn/CommitTxn
4. `test_read_from_offset_beyond_eof` - Read from offset > file size, should return empty vec
5. `test_read_from_offset_zero_on_nonempty` - Read all entries from start
6. `test_checkpoint_with_empty_active_runs` - Checkpoint with zero active runs
7. `test_checkpoint_with_many_active_runs` - Checkpoint with 100 active runs, verify roundtrip
8. `test_vector_upsert_entry_serialization` - VectorUpsert with metadata and source_ref, full roundtrip
9. `test_vector_delete_entry_serialization` - VectorDelete roundtrip
10. `test_vector_collection_create_entry_serialization` - Collection create with all metrics
11. `test_truncate_to_zero` - Truncate to offset 0, verify WAL is empty
12. `test_truncate_to_exact_entry_boundary` - Truncate at exact boundary between entries, verify partial entry not left

### 2B: encoding.rs - Add adversarial tests

The existing 20 tests are strong. Missing:

**Add to encoding.rs tests (~6 tests):**

1. `test_unknown_type_tag_returns_corruption` - Encode with valid CRC but unknown type tag (0xFF), verify Corruption error
2. `test_crc_mismatch_error_includes_offset` - Verify error message contains the file offset
3. `test_encode_decode_vector_upsert` - VectorUpsert entry encode/decode (currently missing from encode tests)
4. `test_encode_decode_vector_delete` - VectorDelete entry
5. `test_encode_decode_vector_collection_create` - VectorCollectionCreate
6. `test_encode_decode_vector_collection_delete` - VectorCollectionDelete

### 2C: recovery.rs - Add adversarial tests

The existing 48 tests are excellent. Missing edge cases:

**Add to recovery.rs tests (~10 tests):**

1. `test_replay_vector_upsert_recovery` - VectorUpsert in committed txn, verify storage receives put_with_version
2. `test_replay_vector_delete_recovery` - VectorDelete in committed txn
3. `test_replay_vector_collection_create_recovery` - Collection create replayed
4. `test_replay_vector_incomplete_txn_discarded` - Vector ops in incomplete txn discarded
5. `test_replay_mixed_kv_json_vector_in_single_txn` - All three primitive types in one transaction
6. `test_replay_json_set_on_nonexistent_doc_logs_warning` - JsonSet without prior JsonCreate, verify warning
7. `test_replay_json_destroy_idempotent` - Two JsonDestroy for same doc in sequence
8. `test_replay_with_version_zero` - Transaction with version 0, verify applied
9. `test_replay_duplicate_txn_ids_across_runs` - Same txn_id used by different runs, verify isolation
10. `test_validate_empty_wal` - validate_transactions on empty entry list

### 2D: snapshot.rs - Add adversarial tests

**Add to snapshot.rs tests (~6 tests):**

1. `test_snapshot_write_then_corrupt_crc_bytes` - Write valid snapshot, flip CRC bytes, verify validate_checksum fails
2. `test_snapshot_write_then_corrupt_header_magic` - Corrupt magic bytes, verify read_header fails with InvalidMagic
3. `test_snapshot_write_then_truncate` - Truncate snapshot file, verify read_envelope fails with TooShort
4. `test_snapshot_envelope_missing_primitive` - Envelope without KV section, verify get_section returns None
5. `test_snapshot_deserialize_skips_unknown_primitive` - Section with primitive_type 255, verify deserialization succeeds for known types
6. `test_snapshot_write_empty_sections` - Write with 0 sections, verify roundtrip

### 2E: snapshot_types.rs - Add adversarial tests

**Add to snapshot_types.rs tests (~5 tests):**

1. `test_snapshot_header_from_bytes_wrong_length` - Buffer of exactly SNAPSHOT_HEADER_SIZE-1 bytes, verify TooShort error
2. `test_snapshot_header_max_values` - Header with u64::MAX for wal_offset, transaction_count, timestamp
3. `test_primitive_section_empty_data` - Section with 0 bytes of data, verify serialized_size = 9 (type + length)
4. `test_snapshot_envelope_duplicate_primitive_types` - Two sections with same primitive_type, verify get_section returns first
5. `test_snapshot_error_display_all_variants` - Verify Display impl for every SnapshotError variant

### 2F: run_bundle - Add adversarial tests

**Add to run_bundle tests (~5 tests):**

1. `test_bundle_manifest_missing_checksums` - Manifest with empty checksums map
2. `test_bundle_run_info_non_terminal_state` - RunInfo with state "active", verify is_terminal_state() is false
3. `test_filter_wal_for_run_empty_entries` - filter_wal_for_run with empty vec
4. `test_filter_wal_for_run_no_matching_entries` - All entries for different run_id, verify empty result
5. `test_xxh3_hex_empty_input` - Empty byte slice, verify produces valid hex string

### Summary of Phase 2

- **~44 unit tests added**
- Focus: vector operation coverage, boundary conditions, corruption handling, cross-primitive recovery

---

## Phase 3: Fix Stress Tests

Un-ignore the 5 valid stress tests and ensure they have proper assertions:

1. `stress_large_wal_recovery` - Already has assertions, just un-ignore
2. `stress_concurrent_writes` - Add assertion that all writes are visible
3. `stress_concurrent_reads` - Already has assertions, just un-ignore
4. `stress_large_values` - Already has assertions, just un-ignore
5. `stress_recovery_after_churn` - Already has assertions, just un-ignore

Mark remaining valid but expensive stress tests with `#[ignore]` only if they take >30s. Remove the 5 that have no assertions (deleted in Phase 1).

---

## Phase 4: Consolidate Issue Files

After Phase 1 deletions, the remaining "issue" files each have 1-3 surviving tests. Consolidate:

- `issue_019_durability_handlers.rs` (1 test) -> merge into `mode_recovery.rs`
- `issue_008_buffered_thread_startup.rs` (1 test) -> merge into `buffered_flush.rs`
- `issue_004_snapshot_header_size.rs` (3 tests) -> merge into `snapshot_format.rs`

Delete the empty issue files and update `main.rs` mod declarations.

---

## Verification

After each phase:
- `cargo test -p strata-durability` for unit tests
- `cargo test --test durability` for integration tests
- Verify no `#[ignore]` tests without clear justification
- Verify no tests with zero assertions remain

Final: `cargo test` full suite to confirm no regressions.

---

## Expected Outcome

| Metric | Before | After |
|--------|--------|-------|
| Integration tests (vanity) | ~39 | 0 |
| Integration tests (real) | ~264 | ~264 |
| Unit tests | ~163 | ~207 |
| Empty/stub test files | 4 | 0 |
| Tests with zero assertions | ~25 | 0 |
| Tests testing stdlib | ~6 | 0 |
| Vector recovery unit tests | 0 | 5+ |
| Encoding adversarial tests | 4 | 10 |
