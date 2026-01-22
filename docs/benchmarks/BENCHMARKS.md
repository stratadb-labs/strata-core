# in-mem Benchmark Suite - Semantic Regression Harness

**Philosophy:** Benchmarks exist to detect semantic regressions, not chase arbitrary numbers.
MVP success is semantic correctness first, performance second.

---

## Benchmark Path Types (Layer Labels)

The prefix indicates the **primary semantic being exercised**, not which module owns the code path.

| Prefix | Primary Semantic | What It Exercises |
|--------|-----------------|-------------------|
| `engine_*` | End-to-end API path | Full Database API (includes WAL, locks, whatever engine includes at that milestone) |
| `wal_*` | Recovery/durability | WAL replay, crash recovery (no engine runtime path) |
| `txn_*` | Transaction lifecycle | Begin, operations, validate, commit (M2 only) |
| `snapshot_*` | Snapshot semantics | Point-in-time consistent reads (M2 only) |
| `conflict_*` | Concurrency patterns | Multi-thread contention, first-committer-wins (M2 only) |
| `json_*` | JSON primitive operations | Create, get, set, delete, exists, version (M5 only) |
| `search_*` | Search operations | Keyword search, hybrid search, result retrieval (M6 only) |
| `index_*` | Index operations | Lookup, indexing, IDF computation (M6 only) |
| `hybrid_*` | Hybrid search | Cross-primitive search, RRF fusion (M6 only) |
| `vector_*` | Vector primitive operations | Insert, search, collection management (M8 only) |
| `facade_*` | Facade tax measurement | Overhead at each layer (A0 hashmap, A1 storage, B facade) |

**Why this taxonomy:**
- `txn_*` separates "commit cost" from "snapshot read cost"
- Prefixes stay stable as the engine grows
- You won't accidentally compare snapshot costs to transaction costs

---

## Durability Mode Labels

All write benchmarks explicitly label their durability mode:

| Label | Meaning |
|-------|---------|
| `dur_strict` | fsync on every write (current M1 default) |
| `dur_batched_Nms` | Batched fsync every N milliseconds (future) |
| `dur_async` | WAL append, no fsync (future) |

**Why this matters:** If you change durability mode and forget to update benchmarks, baselines become meaningless. The label prevents self-deception.

---

## Key Access Patterns

Benchmarks explicitly label their access pattern:

| Pattern | Description | Real Agent Use Case |
|---------|-------------|---------------------|
| `hot_key` | Single key, repeated access | Config reads, counters |
| `hot_doc` | Single document, repeated access (M5) | JSON config reads |
| `hot_query` | Same search query, repeated (M6) | Cached search results |
| `hot_vector` | Same vector query, repeated (M8) | Repeated similarity search |
| `uniform` | Random keys from full keyspace | Arbitrary state access |
| `working_set_N` | Small subset (N keys/docs) | Frequently accessed subset |
| `miss` | Key/document not found | Error path, existence checks |
| `rotating` | Sequential through keyspace | Cache miss testing |

**Why this matters:** Hot-key benchmarks lie about real-world performance. Uniform benchmarks reveal actual BTreeMap and memory hierarchy costs.

---

## Deterministic Randomness

All "random" access patterns use a fixed seed (`BENCH_SEED = 0xDEADBEEF_CAFEBABE`).

**Why this matters:** Non-seeded RNG causes run-to-run variance. Baseline diffs become noisy. When you see a regression, you want to reproduce it exactly.

---

## Benchmark Structure

```
benches/
  # Milestone-specific benchmarks
  m1_storage.rs           # M1: Storage + WAL primitives
  m2_transactions.rs      # M2: OCC + Snapshot Isolation
  m3_primitives.rs        # M3: Primitive facade benchmarks (KV, EventLog, StateCell, etc.)
  m4_contention.rs        # M4: Contention benchmarks (multi-threaded access patterns)
  m4_facade_tax.rs        # M4: Facade tax benchmarks (overhead at each layer)
  m4_performance.rs       # M4: General performance benchmarks
  m5_performance.rs       # M5: JSON Primitive operations
  m6_search.rs            # M6: Search + Retrieval Surfaces
  m8_vector.rs            # M8: Vector primitive benchmarks (similarity search)

  # Cross-cutting benchmarks
  comprehensive_benchmarks.rs  # Comprehensive benchmarks across all primitives
  cross_primitive.rs           # Cross-primitive transaction benchmarks
  industry_comparison.rs       # Comparison with other databases (requires --features=comparison-benchmarks)

  # Utilities
  bench_env.rs            # Environment capture and latency collection utilities
  README.md               # Quick reference for running benchmarks

docs/benchmarks/
  BENCHMARKS.md           # This file - benchmark design and targets
  BENCHMARK_EXECUTION.md  # Execution guide with phases
  PERFORMANCE_BASELINE.md # Performance targets and baseline documentation
```

### Why milestone-scoped benchmarks?

1. **Focus:** Only benchmark what's implemented
2. **Avoid distraction:** Don't optimize for features that don't exist yet
3. **Clear ownership:** Each benchmark file maps to a feature set
4. **Regression detection:** Changes to M1 run M1 benchmarks

---

## What Each Benchmark Proves

### M1 Storage Benchmarks

| Benchmark | Semantic Guarantee | Regression Detection | Agent Pattern |
|-----------|-------------------|----------------------|---------------|
| `engine_get/hot_key` | Returns latest committed version | Lock overhead | Config reads |
| `engine_get/uniform` | Returns correct value for any key | BTreeMap scaling | State lookups |
| `engine_get/working_set_100` | Returns correct value from subset | Cache behavior | Frequent state |
| `engine_get/miss` | Returns None for non-existent key | Miss path cost | Existence checks |
| `engine_put/insert/dur_strict/*` | New key persisted before return | fsync cost | New state creation |
| `engine_put/overwrite/dur_strict/hot_key` | Update replaces old value | Update path | Counter updates |
| `engine_put/overwrite/dur_strict/uniform` | Updates persisted correctly | Write distribution | State updates |
| `engine_delete/existing/dur_strict` | Delete makes key unreadable | Tombstone cost | Cleanup |
| `engine_delete/nonexistent` | Delete of missing key is no-op | Miss handling | Idempotent cleanup |
| `engine_value_size/put_bytes/dur_strict/*` | Large values persisted correctly | Serialization scaling | Blob storage |
| `engine_key_scaling/get_rotating/*` | O(log n) lookup holds at scale | BTreeMap + cache effects | Large databases |
| `wal_recovery/insert_only/*` | All keys readable after recovery | Recovery scaling | Normal restart |
| `wal_recovery/overwrite_heavy` | Only latest version after recovery | MVCC overhead | Long-running agent |
| `wal_recovery/delete_heavy` | Deleted keys return None | Tombstone replay | Cleanup-heavy workload |

### M2 Transaction Benchmarks

| Benchmark | Semantic Guarantee | Regression Detection | Agent Pattern |
|-----------|-------------------|----------------------|---------------|
| `txn_commit/single_put` | Single-key write is atomic | OCC minimal cost | Single state update |
| `txn_commit/multi_put/*` | Multi-key write is atomic | Write-set scaling | Related state updates |
| `txn_commit/read_modify_write` | Read + write is atomic | Read-set + write cost | Counter increment |
| `txn_commit/readN_write1/*` | N reads + 1 write is atomic | Read-set validation | **Canonical agent workload** |
| `txn_cas/success_sequential` | CAS succeeds when version matches | Version check cost | Optimistic updates |
| `txn_cas/failure_version_mismatch` | CAS fails when version mismatched | Fast failure path | Stale read detection |
| `txn_cas/create_new_key` | CAS v0 creates key atomically | Insert-if-absent | Resource claiming |
| `txn_cas/retry_until_success` | CAS retry converges | Retry overhead | Coordination |
| `snapshot/single_read` | Read sees consistent snapshot | Snapshot creation | State query |
| `snapshot/multi_read_10` | All reads see same snapshot | Read-set tracking | Gathering state |
| `snapshot/after_versions/*` | Snapshot cost constant vs history | MVCC overhead | Long-running system |
| `snapshot/read_your_writes` | Transaction sees own writes | Pending write lookup | Build-up before commit |
| `snapshot/read_only_10` | Pure read has no write-set | No conflict possible | Query-only |
| `conflict/disjoint_keys/*` | No conflicts when keys don't overlap | Parallel scaling | Partitioned agents |
| `conflict/same_key/*` | Conflict causes abort, not partial | Conflict resolution | Global counter |
| `conflict/cas_one_winner` | Exactly one CAS winner | First-committer-wins | Lock acquisition |

### M5 JSON Benchmarks

| Benchmark | Semantic Guarantee | Regression Detection | Agent Pattern |
|-----------|-------------------|----------------------|---------------|
| `json_create/small/*` | Small document stored | Base creation cost | Simple state |
| `json_create/large/*` | Large document stored | Serialization scaling | Complex state |
| `json_create/depth_*/*` | Nested document stored | Structure overhead | Hierarchical state |
| `json_create/keys_*/*` | Wide object stored | Field count scaling | Flat state |
| `json_get/hot_doc` | Same document, repeated | Lock overhead | Config reads |
| `json_get/uniform` | Random documents | BTreeMap scaling | State lookups |
| `json_get/working_set_100` | Subset of documents | Cache behavior | Frequent state |
| `json_get/miss` | Non-existent document | Miss path cost | Existence checks |
| `json_get/depth/*` | Path traversal depth | Nested access cost | Deep state |
| `json_set/hot_path/*` | Same path, repeated | Update path | Counter updates |
| `json_set/uniform_docs/*` | Random documents | Write distribution | State updates |
| `json_set/uniform_paths/*` | Random paths in doc | Field update cost | Multi-field |
| `json_set/depth/*` | Deep path write | Path traversal cost | Nested updates |
| `json_set/value_size/*` | Various value sizes | Serialization | Blob updates |
| `json_delete/existing_key/*` | Delete removes value | Deletion cost | State cleanup |
| `json_delete/depth/*` | Delete at depth | Path traversal | Nested cleanup |
| `json_destroy/small/*` | Document removed | Destroy cost | State cleanup |
| `json_destroy/large/*` | Large doc removed | Cleanup scaling | Heavy cleanup |
| `json_exists/hit` | Fast existence check | Existence overhead | Conditional ops |
| `json_exists/miss` | Non-existent check | Miss path | Create-if-absent |
| `json_version/after_updates/*` | Version retrieval | History overhead | OCC checks |
| `json_contention/disjoint_docs/*` | No conflicts for different docs | Parallel scaling | Partitioned state |
| `json_contention/same_doc_different_paths/*` | Document-level conflicts | Conflict cost | Shared state |
| `json_doc_scaling/get_rotating/*` | O(log n) lookup at scale | BTreeMap + cache | Large databases |

### M6 Search Benchmarks

| Benchmark | Semantic Guarantee | Regression Detection | Agent Pattern |
|-----------|-------------------|----------------------|---------------|
| `search_kv/hot_query` | Same query, repeated search | Search cache overhead | Repeated queries |
| `search_kv/uniform` | Random queries across dataset | BM25 scoring cost | Varied queries |
| `search_kv/dataset_*` | Search at different scales | O(n) scan cost | Large datasets |
| `search_hybrid/all_primitives` | Search across all primitives | Cross-primitive overhead | Global search |
| `search_hybrid/filtered` | Search with primitive filter | Filter efficiency | Scoped search |
| `search_hybrid/with_budget` | Budget-constrained search | Budget enforcement | Latency-bounded |
| `search_result_size/k_*` | Different result sizes | Result assembly cost | Top-k queries |
| `index_lookup/term` | Single term lookup | Index overhead | Keyword lookup |
| `index_document/small` | Index a document | Indexing cost | Write + index |
| `index_compute_idf/*` | IDF computation | Term frequency cost | Scoring |
| `index_scaling/*` | Lookup at different index sizes | O(log n) index lookup | Large indices |
| `search_overhead/index_disabled` | Search without index | Baseline scan cost | No index fallback |

### M8 Vector Benchmarks

| Benchmark | Semantic Guarantee | Regression Detection | Agent Pattern |
|-----------|-------------------|----------------------|---------------|
| `vector_insert/single` | Vector stored with metadata | Insert latency | Memory storage |
| `vector_insert/batch_100` | Batch insertion throughput | Bulk load performance | RAG context loading |
| `vector_insert/batch_1000` | Large batch insertion | Batch overhead | Knowledge base updates |
| `vector_search/cosine/10k` | Cosine similarity search | Search latency | Semantic retrieval |
| `vector_search/euclidean/10k` | Euclidean distance search | Distance computation | Clustering queries |
| `vector_search/dot_product/10k` | Dot product search | Scoring speed | Relevance ranking |
| `vector_search/with_filter` | Filtered similarity search | Filter overhead | Scoped retrieval |
| `vector_search/result_size/k_*` | Top-k result assembly | Result scaling | Variable result sizes |
| `vector_collection/create` | Collection creation | Setup overhead | Context initialization |
| `vector_collection/delete` | Collection deletion | Cleanup cost | Context teardown |
| `vector_collection/list` | Collection enumeration | Management overhead | Inventory queries |
| `vector_dimension/128` | Low-dimensional vectors | Dimension scaling | Simple embeddings |
| `vector_dimension/384` | Medium-dimensional vectors | Common embedding size | Sentence transformers |
| `vector_dimension/768` | High-dimensional vectors | BERT-style embeddings | Large models |
| `vector_dimension/1536` | Very high-dimensional | OpenAI embeddings | GPT embeddings |
| `vector_scaling/10k` | 10K vector collection | Small collection | Single conversation |
| `vector_scaling/100k` | 100K vector collection | Medium collection | Extended context |
| `vector_contention/insert/4` | Concurrent inserts | Write contention | Parallel embedding |
| `vector_contention/search/4` | Concurrent searches | Read scalability | Multi-agent queries |

---

## Target Performance (Per Access Pattern)

### Important Context

These targets assume:
- Single-process, in-memory
- RwLock-based concurrency
- BTreeMap-backed storage
- WAL-logged mutations (fsync per operation in `dur_strict` mode)
- Versioned values with snapshot isolation

**Stretch goals are optimistic.** Initial implementations may be 2-5x slower. That's fine. Correctness first.

### M1: Storage + WAL

#### engine_get (by access pattern)

| Access Pattern | Stretch | Acceptable | Concern |
|---------------|---------|------------|---------|
| hot_key | >1M ops/s | >200K ops/s | <100K ops/s |
| working_set_100 | >500K ops/s | >100K ops/s | <50K ops/s |
| uniform (10K keys) | >200K ops/s | >50K ops/s | <25K ops/s |
| miss | >500K ops/s | >100K ops/s | <50K ops/s |

#### engine_put (by operation type)

| Operation | Stretch | Acceptable | Concern |
|-----------|---------|------------|---------|
| insert/dur_strict | >10K ops/s | >1K ops/s | <500 ops/s |
| overwrite/dur_strict/hot_key | >50K ops/s | >10K ops/s | <5K ops/s |
| overwrite/dur_strict/uniform | >20K ops/s | >5K ops/s | <2K ops/s |

#### engine_key_scaling/get_rotating

| Key Count | Stretch | Acceptable | Concern |
|-----------|---------|------------|---------|
| 10K keys | <500ns | <1µs | >2µs |
| 100K keys | <1µs | <2µs | >5µs |
| 1M keys | <2µs | <5µs | >10µs |

#### wal_recovery

| Workload | Stretch | Acceptable | Concern |
|----------|---------|------------|---------|
| insert_only/50K | <500ms | <2s | >5s |
| overwrite_heavy | <500ms | <2s | >5s |
| delete_heavy | <500ms | <2s | >5s |

### M2: Transactions + OCC

| Benchmark | Stretch | Acceptable | Concern |
|-----------|---------|------------|---------|
| txn_commit/single_put | >5K txns/s | >1K txns/s | <500 txns/s |
| txn_commit/readN_write1/10 | >3K txns/s | >500 txns/s | <200 txns/s |
| txn_commit/readN_write1/100 | >1K txns/s | >200 txns/s | <100 txns/s |
| txn_cas/success_sequential | >50K ops/s | >10K ops/s | <5K ops/s |
| snapshot/single_read | >50K ops/s | >10K ops/s | <5K ops/s |
| conflict/disjoint_keys (4 threads) | >80% scaling | >50% scaling | <30% scaling |
| conflict/same_key (4 threads) | >2K txns/s | >500 txns/s | <200 txns/s |

### M5: JSON Primitive

#### json_create (by document type)

| Document Type | Stretch | Acceptable | Concern |
|---------------|---------|------------|---------|
| small (100 bytes) | >50K ops/s | >10K ops/s | <5K ops/s |
| medium (1KB) | >30K ops/s | >5K ops/s | <2K ops/s |
| large (10KB) | >10K ops/s | >2K ops/s | <1K ops/s |
| depth_10 | >20K ops/s | >5K ops/s | <2K ops/s |
| keys_100 | >10K ops/s | >2K ops/s | <1K ops/s |

#### json_get (by access pattern)

| Access Pattern | Stretch | Acceptable | Concern |
|---------------|---------|------------|---------|
| hot_doc | >200K ops/s | >50K ops/s | <20K ops/s |
| uniform | >100K ops/s | >20K ops/s | <10K ops/s |
| working_set_100 | >150K ops/s | >30K ops/s | <15K ops/s |
| miss | >200K ops/s | >50K ops/s | <20K ops/s |
| depth/10 | >50K ops/s | >10K ops/s | <5K ops/s |

#### json_set (by operation type)

| Operation | Stretch | Acceptable | Concern |
|-----------|---------|------------|---------|
| hot_path/dur_strict | >30K ops/s | >5K ops/s | <2K ops/s |
| uniform_docs/dur_strict | >20K ops/s | >5K ops/s | <2K ops/s |
| uniform_paths/dur_strict | >20K ops/s | >5K ops/s | <2K ops/s |
| depth/dur_strict/10 | >15K ops/s | >3K ops/s | <1K ops/s |
| value_size/dur_strict/1024 | >20K ops/s | >5K ops/s | <2K ops/s |
| value_size/dur_strict/65536 | >5K ops/s | >1K ops/s | <500 ops/s |

#### json_contention (concurrency)

| Scenario | Stretch | Acceptable | Concern |
|----------|---------|------------|---------|
| disjoint_docs (4 threads) | >80% scaling | >50% scaling | <30% scaling |
| same_doc_different_paths (4 threads) | >40% scaling | >20% scaling | <10% scaling |

#### json_doc_scaling/get_rotating

| Document Count | Stretch | Acceptable | Concern |
|----------------|---------|------------|---------|
| 10K docs | <2µs | <5µs | >10µs |
| 100K docs | <5µs | <10µs | >20µs |

### M6: Search + Retrieval

#### search_kv (by access pattern)

| Access Pattern | Stretch | Acceptable | Concern |
|---------------|---------|------------|---------|
| hot_query (100 docs) | <50µs | <100µs | >200µs |
| uniform (100 docs) | <100µs | <200µs | >500µs |
| dataset_1000 | <500µs | <1ms | >2ms |
| dataset_10000 | <2ms | <5ms | >10ms |

#### search_hybrid (cross-primitive)

| Scenario | Stretch | Acceptable | Concern |
|----------|---------|------------|---------|
| all_primitives | <200µs | <500µs | >1ms |
| filtered | <100µs | <200µs | >500µs |
| with_budget (50ms) | <50ms | budget-bounded | >budget |

#### index_operations

| Operation | Stretch | Acceptable | Concern |
|-----------|---------|------------|---------|
| lookup/term | <5µs | <10µs | >20µs |
| index_document/small | <50µs | <100µs | >200µs |
| compute_idf (1K terms) | <10µs | <20µs | >50µs |

#### index_scaling/lookup

| Index Size | Stretch | Acceptable | Concern |
|------------|---------|------------|---------|
| 1K terms | <5µs | <10µs | >20µs |
| 10K terms | <10µs | <20µs | >50µs |
| 100K terms | <20µs | <50µs | >100µs |

#### search_result_size

| Result Size (k) | Stretch | Acceptable | Concern |
|-----------------|---------|------------|---------|
| k=1 | <50µs | <100µs | >200µs |
| k=10 | <100µs | <200µs | >400µs |
| k=100 | <200µs | <500µs | >1ms |
| k=500 | <500µs | <1ms | >2ms |

### M8: Vector Primitive

#### vector_insert (by operation type)

| Operation | Stretch | Acceptable | Concern |
|-----------|---------|------------|---------|
| single (384d) | <50µs | <100µs | >200µs |
| batch_100 (384d) | <5ms | <10ms | >20ms |
| batch_1000 (384d) | <50ms | <100ms | >200ms |

#### vector_search (by distance metric)

| Metric + Scale | Stretch | Acceptable | Concern |
|----------------|---------|------------|---------|
| cosine/10k (384d) | <10ms | <50ms | >100ms |
| euclidean/10k (384d) | <10ms | <50ms | >100ms |
| dot_product/10k (384d) | <10ms | <50ms | >100ms |
| with_filter/10k | <15ms | <60ms | >120ms |

#### vector_dimension (scaling with dimension)

| Dimension | Stretch | Acceptable | Concern |
|-----------|---------|------------|---------|
| 128d search/10k | <5ms | <20ms | >50ms |
| 384d search/10k | <10ms | <50ms | >100ms |
| 768d search/10k | <20ms | <80ms | >150ms |
| 1536d search/10k | <40ms | <150ms | >300ms |

#### vector_scaling (collection size)

| Collection Size | Stretch | Acceptable | Concern |
|-----------------|---------|------------|---------|
| 10k vectors | <10ms | <50ms | >100ms |
| 100k vectors | <100ms | <500ms | >1s |

#### vector_contention (concurrency)

| Scenario | Stretch | Acceptable | Concern |
|----------|---------|------------|---------|
| insert/4 threads | >80% scaling | >50% scaling | <30% scaling |
| search/4 threads | >90% scaling | >70% scaling | <50% scaling |

---

## Running Benchmarks

### M1 Storage Benchmarks

```bash
# All M1 benchmarks
cargo bench --bench m1_storage

# By category
cargo bench --bench m1_storage -- "engine_get"
cargo bench --bench m1_storage -- "engine_put"
cargo bench --bench m1_storage -- "engine_delete"
cargo bench --bench m1_storage -- "engine_value_size"
cargo bench --bench m1_storage -- "engine_key_scaling"
cargo bench --bench m1_storage -- "wal_recovery"
```

### M2 Transaction Benchmarks

```bash
# All M2 benchmarks
cargo bench --bench m2_transactions

# By category
cargo bench --bench m2_transactions -- "txn_commit"
cargo bench --bench m2_transactions -- "txn_cas"
cargo bench --bench m2_transactions -- "snapshot"
cargo bench --bench m2_transactions -- "conflict"
```

### M5 JSON Benchmarks

```bash
# All M5 benchmarks
cargo bench --bench m5_performance

# By category
cargo bench --bench m5_performance -- "json_create"
cargo bench --bench m5_performance -- "json_get"
cargo bench --bench m5_performance -- "json_set"
cargo bench --bench m5_performance -- "json_delete"
cargo bench --bench m5_performance -- "json_destroy"
cargo bench --bench m5_performance -- "json_exists"
cargo bench --bench m5_performance -- "json_version"
cargo bench --bench m5_performance -- "json_contention"
cargo bench --bench m5_performance -- "json_doc_scaling"

# By access pattern
cargo bench --bench m5_performance -- "hot_doc"
cargo bench --bench m5_performance -- "uniform"
cargo bench --bench m5_performance -- "depth"

# Using bench_runner.sh
./scripts/bench_runner.sh --m5
./scripts/bench_runner.sh --m5 --filter="json_get"
./scripts/bench_runner.sh --m5 --all-modes
```

### M6 Search Benchmarks

```bash
# All M6 benchmarks
cargo bench --bench m6_search

# By category
cargo bench --bench m6_search -- "search_kv"
cargo bench --bench m6_search -- "search_hybrid"
cargo bench --bench m6_search -- "search_result_size"
cargo bench --bench m6_search -- "index_"
cargo bench --bench m6_search -- "search_overhead"

# By access pattern
cargo bench --bench m6_search -- "hot_query"
cargo bench --bench m6_search -- "uniform"
cargo bench --bench m6_search -- "dataset"

# Using bench_runner.sh
./scripts/bench_runner.sh --m6
./scripts/bench_runner.sh --m6 --filter="search_kv"
./scripts/bench_runner.sh --m6 --filter="index_"
```

### M8 Vector Benchmarks

```bash
# All M8 benchmarks
cargo bench --bench m8_vector

# By category
cargo bench --bench m8_vector -- "vector_insert"
cargo bench --bench m8_vector -- "vector_search"
cargo bench --bench m8_vector -- "vector_collection"
cargo bench --bench m8_vector -- "vector_dimension"
cargo bench --bench m8_vector -- "vector_scaling"
cargo bench --bench m8_vector -- "vector_contention"

# By access pattern
cargo bench --bench m8_vector -- "cosine"
cargo bench --bench m8_vector -- "euclidean"
cargo bench --bench m8_vector -- "dot_product"

# Using bench_runner.sh
./scripts/bench_runner.sh --m8
./scripts/bench_runner.sh --m8 --filter="vector_search"
```

### Using bench_runner.sh (Recommended)

The `bench_runner.sh` script provides comprehensive benchmark execution with:
- Environment capture (CPU, memory, system state)
- Automatic result organization
- M9 optimization tracking (tags, notes, decisions)
- JSON metrics extraction for comparison

```bash
# Run ALL benchmarks (recommended for M9 baseline)
./scripts/bench_runner.sh --full --tag=baseline --notes="M9 initial baseline"

# Run specific milestone
./scripts/bench_runner.sh --m1
./scripts/bench_runner.sh --m2
./scripts/bench_runner.sh --m3
./scripts/bench_runner.sh --m4
./scripts/bench_runner.sh --m5
./scripts/bench_runner.sh --m6
./scripts/bench_runner.sh --m8

# Run with filters
./scripts/bench_runner.sh --m6 --filter="search_kv"

# M9 optimization workflow
./scripts/bench_runner.sh --full --tag=opt-name --notes="Description of optimization"

# Results are stored in:
# target/benchmark-results/run_{TIMESTAMP}_{COMMIT}/
#   ├── FULL_SUMMARY.md          # Consolidated summary
#   ├── run_metadata.json        # Tag, notes, decision
#   ├── all_benchmarks.json      # All metrics
#   └── {benchmark_name}.txt/json # Per-benchmark results
```

### Comparison Mode

```bash
# Save baseline
cargo bench --bench m1_storage -- --save-baseline main
cargo bench --bench m2_transactions -- --save-baseline main
cargo bench --bench m5_performance -- --save-baseline main
cargo bench --bench m6_search -- --save-baseline main
cargo bench --bench m8_vector -- --save-baseline main

# Compare against baseline
cargo bench --bench m1_storage -- --baseline main
cargo bench --bench m2_transactions -- --baseline main
cargo bench --bench m5_performance -- --baseline main
cargo bench --bench m6_search -- --baseline main
cargo bench --bench m8_vector -- --baseline main
```

---

## Interpreting Results

### Criterion Output

```
engine_get/hot_key
                        time:   [200.45 ns 201.23 ns 202.01 ns]
                        thrpt:  [4.9502 Melem/s 4.9694 Melem/s 4.9887 Melem/s]
```

- Three numbers: [lower bound, estimate, upper bound] at 95% confidence
- `thrpt` = throughput in elements/second
- 4.97M ops/s = well above "acceptable" (>200K ops/s for hot_key)

### Conflict Benchmark Output

```
conflict/same_key/4: 1234 commits, 567 aborts (68.5% success) in 2.00s
```

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

### What to Do About Regressions

1. **<5%:** Noise, likely acceptable
2. **5-15%:** Investigate, may be acceptable tradeoff
3. **>15%:** Likely real regression, prioritize investigation
4. **>50%:** Something is seriously wrong

---

## Benchmark Honesty Checklist

For every benchmark, verify:

1. **All setup is outside the timed loop**
   - No key allocation in `b.iter()`
   - No value construction in `b.iter()`
   - No random number generation in `b.iter()` (LCG state mutation is fine)

2. **Access pattern is explicitly labeled**
   - `hot_key`, `uniform`, `working_set`, `miss`, `rotating`, or `hot_query`

3. **Layer is explicitly labeled**
   - `engine_`, `wal_`, `txn_`, `snapshot_`, `conflict_`, `search_`, `index_`, or `hybrid_`

4. **Durability mode is labeled for writes**
   - `dur_strict`, `dur_batched_*`, or `dur_async`

5. **Fixed seed for reproducibility**
   - Uses `BENCH_SEED` for any randomness in setup

6. **Four questions answered:**
   - What semantic guarantee does this exercise?
   - What layer does it measure?
   - What regression would it detect?
   - What real agent pattern does it approximate?

---

## Invariant Validation

**Performance without correctness is meaningless.**

### Contract

1. **Benchmarks do NOT assert invariants inline** - this keeps overhead out of the timed loop
2. **Invariants are validated in separate tests** - run after benchmarks
3. **If you change semantics, update invariants BEFORE updating benchmarks**

### Validation Procedure

After running benchmarks, validate invariants:

```bash
cargo test --test m1_m2_comprehensive invariant -- --nocapture
```

If benchmarks pass but invariant tests fail, the benchmarks are measuring a broken system.

---

## What's NOT Benchmarked (Yet)

### Tail Latency
- P95, P99 latency under load
- Jitter during concurrent access
- Worst-case pauses

**Why:** Requires more sophisticated harnesses. Add when correctness is proven.

### Comparison to Other Systems
- Redis, SQLite, RocksDB, etc.

**Why:** Comparisons are only meaningful after our system is stable.

---

## Adding New Benchmarks

### Template

```rust
// --- Benchmark: descriptive_name ---
// Semantic: What guarantee does this exercise? (testable property)
// Real pattern: What agent behavior does this simulate?
{
    // Setup OUTSIDE bench_function
    let temp_dir = TempDir::new().unwrap();
    let db = Database::open(temp_dir.path().join("db")).unwrap();
    let keys = pregenerate_keys(&ns, "prefix", COUNT); // Pre-allocate!

    group.bench_function("descriptive_name", |b| {
        let mut rng_state = BENCH_SEED; // Fixed seed!
        b.iter(|| {
            // ONLY the operation under test
            let idx = (lcg_next(&mut rng_state) as usize) % COUNT;
            black_box(db.get(&keys[idx]).unwrap())
        });
    });
}
```

### Checklist for New Benchmarks

- [ ] Layer labeled in name (`engine_`, `wal_`, `txn_`, `snapshot_`, `conflict_`, `search_`, `index_`, `hybrid_`, `vector_`, `facade_`)
- [ ] Access pattern labeled if applicable (`hot_key`, `uniform`, `hot_query`, etc.)
- [ ] Durability mode labeled for writes (`dur_strict`, etc.)
- [ ] All setup outside timed loop
- [ ] Fixed seed (`BENCH_SEED`) for any randomness
- [ ] Comment explains semantic guarantee (testable property)
- [ ] Comment explains real agent pattern
- [ ] Four questions can be answered
