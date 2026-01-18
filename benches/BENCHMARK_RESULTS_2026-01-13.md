# Benchmark Results - 2026-01-13

## Environment
- **OS**: Darwin 24.6.0 (macOS, Darwin Kernel arm64)
- **CPU**: Apple M1 Pro
- **Memory**: 16 GB
- **Rust version**: rustc 1.92.0 (ded5c06cf 2025-12-08)

## Pre-Benchmark Validation

**Status: FAILED (Tests Not Runnable)**

The invariant tests could not be run due to compilation errors. The test code is out of sync with the current API:
- Test files in `tests/m1_m2_comprehensive/` reference outdated API signatures
- 26 compilation errors including missing fields, wrong argument counts, and method signature mismatches
- Fixed module structure issue (removed duplicate `m1_m2_comprehensive.rs` file)

**Action Required**: Update test suite to match current API before next benchmark run.

---

## M1 Storage Results

All M1 benchmarks completed successfully.

| Benchmark | Time | Throughput | vs Acceptable | Status |
|-----------|------|------------|---------------|--------|
| kv_get/existing_key | 971.16 µs | 1.03K ops/s | -89.7% | **CRITICAL** |
| kv_get/nonexistent_key | 970.40 µs | 1.03K ops/s | -89.7% | **CRITICAL** |
| kv_get/position/early | 966.38 µs | 1.03K ops/s | -89.7% | **CRITICAL** |
| kv_get/position/middle | 983.84 µs | 1.02K ops/s | -89.8% | **CRITICAL** |
| kv_get/position/late | 982.70 µs | 1.02K ops/s | -89.8% | **CRITICAL** |
| kv_put/unique_keys | 799.45 µs | 1.25K ops/s | -75.0% | **CRITICAL** |
| kv_put/overwrite_same_key | 18.77 µs | 53.3K ops/s | +966% | **OK** |
| kv_put/delete | 28.91 µs | 34.6K ops/s | +592% | **OK** |

### Value Size Benchmarks

| Benchmark | Time | Throughput |
|-----------|------|------------|
| put_bytes/64 | 1.08 ms | 57.8 KiB/s |
| put_bytes/256 | 1.65 ms | 151.9 KiB/s |
| put_bytes/1024 | 1.22 ms | 818.5 KiB/s |
| put_bytes/4096 | 2.73 ms | 1.43 MiB/s |
| put_bytes/16384 | 3.28 ms | 4.77 MiB/s |

### WAL Replay Benchmarks

| Benchmark | Time | Throughput | vs Acceptable | Status |
|-----------|------|------------|---------------|--------|
| replay_ops/1000 | 800.56 µs | 1.25M elem/s | - | **OK** |
| replay_ops/10000 | 749.40 µs | 13.3M elem/s | - | **OK** |
| replay_ops/50000 | 959.71 µs | 52.1M elem/s | - | **OK** |
| replay_mixed_workload | 826.82 µs | 12.1M elem/s | - | **OK** |

**Note**: WAL replay of 50K ops completes in ~1ms, well under the 500ms stretch goal.

### Memory Overhead (Get at Scale)

| Scale | Time | Throughput |
|-------|------|------------|
| 1,000 keys | 85.15 µs | 11.7K ops/s |
| 10,000 keys | 1.04 ms | 963 ops/s |
| 100,000 keys | 23.86 ms | 41.9 ops/s |

---

## M2 Transaction Results

**Status: PARTIAL (Benchmark Bug)**

The M2 benchmark suite failed at `m2_cas/create_new_key` due to a bug in the benchmark code where the counter resets between Criterion's warmup and measurement phases.

### Completed M2 Benchmarks

| Benchmark | Time | Throughput | vs Acceptable | Status |
|-----------|------|------------|---------------|--------|
| transaction_commit/single_key_put | 830.94 µs | 1.20K txns/s | -40% | **CONCERN** |
| transaction_commit/multi_key_put/3 | 1.46 ms | 687 txns/s | - | - |
| transaction_commit/multi_key_put/5 | 3.31 ms | 302 txns/s | - | - |
| transaction_commit/multi_key_put/10 | 2.94 ms | 340 txns/s | - | - |
| transaction_commit/read_modify_write | 16.58 µs | 60.3K ops/s | - | **OK** |
| cas/sequential_success | 18.49 µs | 54.1K ops/s | +2605% | **OK** |
| cas/failure_wrong_version | 1.61 µs | 619.9K ops/s | - | **OK** |

### Not Run Due to Bug

- [ ] m2_cas/create_new_key
- [ ] m2_snapshot_read/*
- [ ] m2_conflict_detection/*
- [ ] m2_version_growth/*

---

## Observations

### Critical Performance Issues

1. **kv_get is extremely slow**: ~1K ops/s vs expected >10K ops/s (acceptable) or >50K ops/s (stretch)
   - This is 10x slower than acceptable threshold
   - Suggests O(n) lookup or excessive locking

2. **kv_put/unique_keys is slow**: ~1.25K ops/s vs expected >5K ops/s
   - 4x slower than acceptable threshold
   - Likely same root cause as kv_get

### Positive Results

1. **Overwrite operations are fast**: 53K ops/s for overwrite, 35K ops/s for delete
   - These exceed all thresholds significantly
   - Suggests the slow path is in key lookup/iteration

2. **WAL replay is excellent**: 50K ops replayed in <1ms
   - Well exceeds stretch goal of <500ms

3. **CAS operations are fast**: 54K ops/s for sequential success
   - Far exceeds 2K ops/s acceptable threshold

4. **Read-modify-write is fast**: 60K ops/s
   - Good transactional performance once key is found

### Hypothesis

The primary bottleneck appears to be **key lookup**. Operations that need to find a key in a populated store are slow, while operations on known/cached keys are fast. This suggests:
- Linear scan during key lookup
- Or inefficient data structure for key storage
- Or excessive lock contention during traversal

---

## Action Items

### Bugs to Fix (Priority: Critical)
- [ ] **Test Suite API Mismatch**: Update `tests/m1_m2_comprehensive/*.rs` to match current API
- [ ] **M2 Benchmark Bug**: Fix `create_new_key` benchmark counter reset issue

### Performance Investigation (Priority: High)
- [ ] Profile `kv_get` to identify bottleneck
- [ ] Profile `kv_put/unique_keys` to confirm same issue
- [ ] Review storage layer data structure for key lookup

### Follow-up
- [ ] Re-run full benchmark suite after test fixes
- [ ] Re-run M2 benchmarks after benchmark bug fix
- [ ] Save baseline once all benchmarks complete successfully

---

## Baseline Status

**NOT SAVED** - Cannot save baseline with incomplete M2 results and critical performance concerns.

---

## Raw Results Summary

```
M1 kv_get/existing_key:        971.16 µs (1.03K/s)
M1 kv_put/unique_keys:         799.45 µs (1.25K/s)
M1 kv_put/overwrite:           18.77 µs  (53.3K/s)
M1 wal_replay/50K:             959.71 µs
M2 txn_commit/single:          830.94 µs (1.20K/s)
M2 cas/sequential_success:     18.49 µs  (54.1K/s)
```
