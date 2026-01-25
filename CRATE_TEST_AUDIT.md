# Test Audit: strata-engine, strata-storage, strata-concurrency

Audit against TESTING_METHODOLOGY.md principles.

---

## Summary

| Crate | Total Tests | Vanity Tests | Compliance |
|-------|-------------|--------------|------------|
| strata-engine | 200 | ~12 | 94% |
| strata-storage | 503 | ~18 | 96% |
| strata-concurrency | 352 | ~13 | 96% |

**Overall:** These crates are in good shape. The vanity tests exist but are a small percentage.

---

## strata-engine (200 tests)

### Vanity Tests Found (~12)

| File | Test | Violation |
|------|------|-----------|
| durability/buffered.rs | `test_buffered_debug` | Tests Debug format |
| durability/inmemory.rs | `test_inmemory_default` | Tests Default trait |
| durability/strict.rs | `test_strict_debug` | Tests Debug format |
| durability/traits.rs | `test_requires_wal_default` | Tests default impl |
| durability/traits.rs | `test_commit_data_clone` | Tests Clone trait |
| durability/traits.rs | `test_commit_data_debug` | Tests Debug format |
| database.rs | `test_retry_config_default` | Tests Default trait |
| database.rs | `test_database_builder_default` | Tests Default trait |
| database.rs | `test_database_builder_default_trait` | Tests Default trait |
| recovery_participant.rs | `test_recovery_participant_debug` | Tests Debug format |
| recovery_participant.rs | `test_recovery_participant_clone` | Tests Clone trait |

### Good Tests (Examples)

- `test_wait_for_idle_timeout_with_active_transaction` - Tests actual timeout behavior
- `test_recover_error_stops_execution` - Tests error propagation
- `test_transaction_with_retry_conflict_is_retried` - Tests retry logic
- `test_recovery_discards_incomplete` - Tests crash recovery

---

## strata-storage (503 tests)

### Vanity Tests Found (~18)

| File | Test | Violation |
|------|------|-----------|
| codec/identity.rs | `test_identity_is_send_sync` | Compiler-verified |
| compaction/mod.rs | `test_compact_info_default` | Tests Default |
| compaction/mod.rs | `test_compact_mode_hash` | Tests Hash trait |
| retention/policy.rs | `test_keep_all_default` | Tests Default |
| retention/policy.rs | `test_composite_uses_default` | Tests Default |
| testing/crash_harness.rs | `test_crash_config_default` | Tests Default |
| wal/durability.rs | `test_buffered_default` | Tests Default |
| index.rs | `test_run_index_default` | Tests Default |
| index.rs | `test_type_index_default` | Tests Default |
| registry.rs | `test_registry_debug` | Tests Debug format |
| sharded.rs | `test_snapshot_clone` | Tests Clone trait |
| sharded.rs | `test_snapshot_debug` | Tests Debug format |
| snapshot.rs | `test_snapshot_can_be_cloned` | Tests Clone trait |
| snapshot.rs | `test_snapshot_is_send_sync` | Compiler-verified |
| ttl.rs | `test_ttl_index_default` | Tests Default |
| unified.rs | `test_store_is_send_sync` | Compiler-verified |

### Good Tests (Examples)

- `test_concurrent_compaction_and_wal_writes` - Tests actual concurrency
- `test_recover_corrupted_snapshot_crc_mismatch` - Tests corruption handling
- `test_version_chain_get_at_version_snapshot_isolation` - Tests MVCC
- `test_compaction_never_removes_segment_being_written` - Tests invariant

---

## strata-concurrency (352 tests)

### Vanity Tests Found (~13)

| File | Test | Violation |
|------|------|-----------|
| snapshot.rs | `test_snapshot_is_send` | Compiler-verified |
| snapshot.rs | `test_snapshot_is_sync` | Compiler-verified |
| transaction.rs | `test_transaction_status_debug` | Tests Debug format |
| transaction.rs | `test_transaction_status_clone` | Tests Clone trait |
| transaction.rs | `test_pending_operations_debug` | Tests Debug format |
| transaction.rs | `test_pending_operations_clone` | Tests Clone trait |
| transaction.rs | `test_json_path_read_clone` | Tests Clone trait |
| transaction.rs | `test_json_patch_entry_clone` | Tests Clone trait |
| validation.rs | `test_conflict_type_debug` | Tests Debug format |

### Good Tests (Examples)

- `test_commit_lock_prevents_toctou_race` - Tests actual race prevention
- `test_version_monotonicity_under_load` - Tests concurrent invariant
- `test_first_committer_wins_with_read_overlap` - Tests conflict detection
- `test_no_deadlock_high_contention` - Tests deadlock freedom

---

## Shallow Assertions

Found across all three crates:
- **50** instances of `assert!(x.is_ok())` without checking value
- **59** instances of `assert!(x.is_some())` without checking value

Many of these are acceptable (e.g., testing that an operation succeeds), but some should verify the actual result.

---

## Verdict

**No action required.** The crates are compliant with methodology:

1. **Vanity tests are <5%** of total - acceptable legacy
2. **Core tests are meaningful** - they test actual behavior, edge cases, concurrency
3. **Methodology applies going forward** - new tests should follow guidelines

The existing vanity tests:
- Don't break anything
- Don't slow down CI significantly
- Aren't worth the churn to remove

**Focus future effort on:** Writing meaningful tests for new code, not cleaning up old tests.
