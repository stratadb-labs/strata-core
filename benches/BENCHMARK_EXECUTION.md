# Benchmark Execution Prompt

Use this prompt to systematically execute the benchmark suite and document results.

---

## Execution Prompt

```
Execute the in-mem benchmark suite in the following order. Document all results.
Do NOT optimize during this run - just measure and record.
```

## Phase 1: Verify Correctness First

Before benchmarking, ensure the system is correct:

```bash
# Run invariant tests
cargo test --test m1_m2_comprehensive invariant -- --nocapture

# If any invariant test fails, STOP. Do not benchmark a broken system.
```

If tests fail, open an issue with label `bug`, `priority:critical` before proceeding.

## Phase 2: M1 Storage Benchmarks

Run M1 benchmarks (single-threaded, storage layer + WAL):

```bash
cargo bench --bench m1_storage -- --noplot
```

Record results for:

### engine_get (Read Performance)
- [ ] `engine_get/hot_key` - Single key repeated access
- [ ] `engine_get/uniform` - Random keys from full keyspace
- [ ] `engine_get/working_set_100` - Hot subset of 100 keys
- [ ] `engine_get/miss` - Key not found path

### engine_put (Write Performance - dur_strict)
- [ ] `engine_put/insert/dur_strict/uniform` - New key creation + WAL
- [ ] `engine_put/overwrite/dur_strict/hot_key` - Update single key
- [ ] `engine_put/overwrite/dur_strict/uniform` - Random updates

### engine_delete (Delete Performance)
- [ ] `engine_delete/existing/dur_strict` - Tombstone creation
- [ ] `engine_delete/nonexistent` - No-op efficiency

### engine_value_size (Serialization Scaling)
- [ ] `engine_value_size/put_bytes/dur_strict/64`
- [ ] `engine_value_size/put_bytes/dur_strict/256`
- [ ] `engine_value_size/put_bytes/dur_strict/1024`
- [ ] `engine_value_size/put_bytes/dur_strict/4096`
- [ ] `engine_value_size/put_bytes/dur_strict/65536`

### engine_key_scaling (Cache Boundary Tests)
- [ ] `engine_key_scaling/get_rotating/10000`
- [ ] `engine_key_scaling/get_rotating/100000`
- [ ] `engine_key_scaling/get_rotating/1000000`

### wal_recovery (Recovery Performance)
- [ ] `wal_recovery/insert_only/1000`
- [ ] `wal_recovery/insert_only/10000`
- [ ] `wal_recovery/insert_only/50000`
- [ ] `wal_recovery/overwrite_heavy` - Version history replay
- [ ] `wal_recovery/delete_heavy` - Tombstone replay

### M1 Expected Ranges

| Benchmark | Stretch | Acceptable | Concern |
|-----------|---------|------------|---------|
| engine_get/hot_key | >1M ops/s | >200K ops/s | <100K ops/s |
| engine_get/uniform | >200K ops/s | >50K ops/s | <25K ops/s |
| engine_put/insert/dur_strict | >10K ops/s | >1K ops/s | <500 ops/s |
| engine_put/overwrite/dur_strict/hot_key | >50K ops/s | >10K ops/s | <5K ops/s |
| wal_recovery/insert_only/50000 | <500ms | <2s | >5s |
| engine_key_scaling/get_rotating/1000000 | <2µs | <5µs | >10µs |

## Phase 3: M2 Transaction Benchmarks

Run M2 benchmarks (transactions, OCC, snapshots):

```bash
cargo bench --bench m2_transactions -- --noplot
```

Record results for:

### txn_commit (Transaction Overhead)
- [ ] `txn_commit/single_put` - Minimal txn cost
- [ ] `txn_commit/multi_put/3`
- [ ] `txn_commit/multi_put/5`
- [ ] `txn_commit/multi_put/10`
- [ ] `txn_commit/read_modify_write` - RMW atomicity
- [ ] `txn_commit/readN_write1/1` - Canonical agent workload
- [ ] `txn_commit/readN_write1/10` - **Key benchmark**
- [ ] `txn_commit/readN_write1/100` - Large read-set validation

### txn_cas (Compare-and-Swap)
- [ ] `txn_cas/success_sequential` - Happy path
- [ ] `txn_cas/failure_version_mismatch` - Fast failure
- [ ] `txn_cas/create_new_key` - Atomic creation
- [ ] `txn_cas/retry_until_success` - Retry pattern

### snapshot (MVCC Semantics)
- [ ] `snapshot/single_read` - Snapshot creation cost
- [ ] `snapshot/multi_read_10` - Multi-key reads
- [ ] `snapshot/after_versions/10`
- [ ] `snapshot/after_versions/100`
- [ ] `snapshot/after_versions/1000`
- [ ] `snapshot/read_your_writes` - Pending write lookup
- [ ] `snapshot/read_only_10` - Pure read transaction

### conflict (Concurrency - reports commits/aborts)
- [ ] `conflict/disjoint_keys/2`
- [ ] `conflict/disjoint_keys/4`
- [ ] `conflict/disjoint_keys/8`
- [ ] `conflict/same_key/2`
- [ ] `conflict/same_key/4`
- [ ] `conflict/cas_one_winner`

### M2 Expected Ranges

| Benchmark | Stretch | Acceptable | Concern |
|-----------|---------|------------|---------|
| txn_commit/single_put | >5K txns/s | >1K txns/s | <500 txns/s |
| txn_commit/readN_write1/10 | >3K txns/s | >500 txns/s | <200 txns/s |
| txn_cas/success_sequential | >50K ops/s | >10K ops/s | <5K ops/s |
| snapshot/single_read | >50K ops/s | >10K ops/s | <5K ops/s |
| conflict/disjoint_keys/4 | >80% scaling | >50% scaling | <30% scaling |
| conflict/same_key/4 | >2K txns/s | >500 txns/s | <200 txns/s |

## Phase 4: Save Baseline

If results are acceptable, save as baseline:

```bash
cargo bench --bench m1_storage -- --save-baseline current
cargo bench --bench m2_transactions -- --save-baseline current
```

## Phase 5: Document Results

Create a benchmark report with this format:

```markdown
# Benchmark Results - [DATE]

## Environment
- OS: [uname -a]
- CPU: [model, cores]
- Memory: [total RAM]
- Rust version: [rustc --version]
- BENCH_SEED: 0xDEADBEEF_CAFEBABE

## M1 Storage Results

| Benchmark | Result | vs Acceptable | Status |
|-----------|--------|---------------|--------|
| engine_get/hot_key | X ops/s | +Y% | OK/CONCERN |
| engine_get/uniform | X ops/s | +Y% | OK/CONCERN |
| engine_put/insert/dur_strict/uniform | X ops/s | +Y% | OK/CONCERN |
| wal_recovery/insert_only/50000 | Xms | +Y% | OK/CONCERN |
| engine_key_scaling/get_rotating/1000000 | Xµs | +Y% | OK/CONCERN |
| ... | ... | ... | ... |

## M2 Transaction Results

| Benchmark | Result | vs Acceptable | Status |
|-----------|--------|---------------|--------|
| txn_commit/single_put | X txns/s | +Y% | OK/CONCERN |
| txn_commit/readN_write1/10 | X txns/s | +Y% | OK/CONCERN |
| txn_cas/success_sequential | X ops/s | +Y% | OK/CONCERN |
| conflict/disjoint_keys/4 | X% scaling | +Y% | OK/CONCERN |
| conflict/same_key/4 | X commits, Y aborts (Z% success) | +W% | OK/CONCERN |
| ... | ... | ... | ... |

## Observations

- [Any unexpected results]
- [Bottlenecks identified]
- [Access pattern insights]
- [Conflict benchmark commit/abort ratios]

## Action Items

- [ ] [Any issues to investigate]
- [ ] [Optimizations to consider later]
```

## Phase 6: Re-verify Correctness

After benchmarking, run invariant tests again:

```bash
cargo test --test m1_m2_comprehensive invariant -- --nocapture
```

If tests pass: benchmark results are valid.
If tests fail: benchmark results are INVALID. Something broke during the run.

---

## Interpretation Guide

### Reading Criterion Output

```
engine_get/hot_key
                        time:   [200.45 ns 201.23 ns 202.01 ns]
                        thrpt:  [4.9502 Melem/s 4.9694 Melem/s 4.9887 Melem/s]
```

- Three numbers: [lower bound, estimate, upper bound] at 95% confidence
- Use the **middle number** (estimate) for reporting
- `thrpt` = throughput in elements/second
- 4.97M ops/s = well above "acceptable" (>200K ops/s for hot_key)

### Reading Conflict Benchmark Output

```
conflict/same_key/4: 1234 commits, 567 aborts (68.5% success) in 2.00s
```

- Logged once per sample via `eprintln!`
- Commits = successful transactions
- Aborts = conflict-induced rollbacks
- Success ratio indicates contention severity

### Regression Detection

```
Performance has regressed:
  time:   [200.45 ns 210.23 ns 220.01 ns]
                        change: [+15.234% +18.901% +22.345%] (p = 0.001 < 0.05)
```

- `change` shows percentage difference from baseline
- `p < 0.05` means statistically significant
- Investigate regressions >10% on critical paths

### Status Categories

- **OK**: Meets or exceeds "acceptable" threshold
- **MARGINAL**: Within 20% of "acceptable" threshold
- **CONCERN**: Below "acceptable" threshold
- **CRITICAL**: Below 50% of "acceptable" threshold

### What NOT to Do

1. Do NOT optimize based on a single benchmark run
2. Do NOT compare to other systems yet (we're not stable)
3. Do NOT chase "stretch" goals before "acceptable" is met
4. Do NOT ignore invariant test failures

---

## Quick Commands

```bash
# Full suite (both M1 and M2)
cargo bench --bench m1_storage --bench m2_transactions -- --noplot

# Just M1
cargo bench --bench m1_storage -- --noplot

# Just M2
cargo bench --bench m2_transactions -- --noplot

# By category
cargo bench --bench m1_storage -- "engine_get"
cargo bench --bench m1_storage -- "engine_put"
cargo bench --bench m1_storage -- "wal_recovery"
cargo bench --bench m2_transactions -- "txn_commit"
cargo bench --bench m2_transactions -- "txn_cas"
cargo bench --bench m2_transactions -- "snapshot"
cargo bench --bench m2_transactions -- "conflict"

# By access pattern
cargo bench --bench m1_storage -- "hot_key"
cargo bench --bench m1_storage -- "uniform"
cargo bench --bench m1_storage -- "dur_strict"

# The canonical agent workload benchmark
cargo bench --bench m2_transactions -- "readN_write1"

# Compare to baseline
cargo bench --bench m1_storage -- --baseline current
cargo bench --bench m2_transactions -- --baseline current

# Run with more samples (slower, more accurate)
cargo bench --bench m1_storage -- --sample-size 200

# Run invariant tests
cargo test --test m1_m2_comprehensive invariant
```

---

## Issue Template (for concerns)

If any benchmark shows "CONCERN" or "CRITICAL" status:

```markdown
## Benchmark Performance Issue

**Benchmark**: [name, e.g., engine_get/uniform]
**Result**: [X ops/s]
**Expected**: [>Y ops/s (acceptable)]
**Gap**: [Z% below acceptable]
**Layer**: [engine/wal/txn/snapshot/conflict]
**Access Pattern**: [hot_key/uniform/working_set/miss/rotating]
**Durability Mode**: [dur_strict/dur_async/N/A]

### Environment
- OS:
- Rust version:
- BENCH_SEED: 0xDEADBEEF_CAFEBABE

### Reproduction
```bash
cargo bench --bench [m1_storage|m2_transactions] -- "[benchmark_name]"
```

### Notes
[Any observations about the result]
```

Labels: `performance`, `needs-investigation`
```

---

## Success Criteria

A benchmark run is successful if:

- [ ] All invariant tests pass before AND after benchmarking
- [ ] All M1 benchmarks meet "acceptable" thresholds
- [ ] All M2 benchmarks meet "acceptable" thresholds
- [ ] No benchmark shows >20% regression from baseline (if baseline exists)
- [ ] Results are documented with layer, access pattern, and durability mode context
- [ ] Conflict benchmarks report commit/abort ratios

If any criterion is not met, document the gap and create issues for investigation.
Do NOT block on performance issues - correctness comes first.
