# Durability Modes

## Overview

StrataDB offers three durability modes that control the tradeoff between write performance and crash safety. The naming is designed to be self-explanatory — users should be able to pick the right mode without reading documentation.

| Mode | Data location | Crash behavior | Write latency target |
|------|-------------|----------------|---------------------|
| **Cache** | Memory only | All data lost on process exit | < 5 us |
| **Standard** | Disk + periodic WAL flush | May lose last flush interval on crash | < 10 us |
| **Always** | Disk + per-write WAL sync | Zero data loss on crash | ~6 ms (NVMe fsync) |

## Mode Details

### Cache

In-memory only. No disk persistence, no WAL. Data exists only for the lifetime of the process. Equivalent to a sophisticated in-process cache with full primitive support (KV, State, Event, JSON, Vector, Branch).

**Use cases**: Session stores, request-scoped scratch data, feature flag caches, real-time analytics accumulators, test suites, prototyping.

**Contract**: Data is available immediately after write. Data is gone when the process exits (clean or crash). No disk I/O occurs.

**API**:
```rust
let db = Database::cache();
let strata = Strata::from_database(db);
```

**Prior art**: Redis without persistence, memcached, in-process HashMaps.

### Standard

Disk-backed with background WAL flushing. Writes go to memory immediately and are appended to an in-memory WAL buffer. A background thread flushes the buffer to disk periodically (configurable interval, default: every second). Reads always see the latest in-memory state regardless of flush status.

**Use cases**: Most production workloads — web applications, APIs, content management, analytics, any workload where losing the last second of writes on a crash is acceptable.

**Contract**: Data survives clean shutdowns. On crash, writes from the last flush interval (up to ~1 second by default) may be lost. All other data is recoverable from the WAL.

**API**:
```rust
// Standard is the default when a path is provided
let db = Database::builder().path("/data/mydb").open()?;
let strata = Strata::from_database(db);

// Explicit
let db = Database::builder().path("/data/mydb").standard().open()?;
```

**Prior art**: Redis `appendfsync everysec`, SQLite `PRAGMA synchronous = NORMAL`, MySQL `innodb_flush_log_at_trx_commit = 2`.

### Always

Disk-backed with per-write WAL sync. Every write is fsynced to disk before the call returns. This provides the strongest durability guarantee: if the write call returned Ok, the data is on disk.

**Use cases**: Financial transactions, audit logs, billing systems, any workload where losing even one write is unacceptable.

**Contract**: Every write that returns successfully is durable on disk. Data survives both clean shutdowns and crashes with zero loss.

**API**:
```rust
let db = Database::builder().path("/data/mydb").always().open()?;
let strata = Strata::from_database(db);
```

**Prior art**: Redis `appendfsync always`, SQLite `PRAGMA synchronous = FULL`, PostgreSQL `synchronous_commit = on`.

## Design Principles

### Two axes, not one

Durability has two independent dimensions that were previously conflated:

1. **Storage**: Where does data live? (Memory only vs disk-backed)
2. **WAL policy**: When is data synced to disk? (Never, periodically, every write)

The three modes represent the useful combinations of these axes:

| | No WAL | Background WAL | Per-write WAL |
|---|--------|---------------|---------------|
| **Memory only** | **Cache** | *(not useful)* | *(not useful)* |
| **Disk** | *(dropped)* | **Standard** | **Always** |

The dropped combination (disk + no WAL) was considered but intentionally excluded:
- If Standard mode achieves near-Cache write latency (the design target), there is no performance reason to skip the WAL on disk-backed databases.
- Users who want fast disk writes without crash recovery are better served by Standard with a short flush interval than by a mode that offers no recovery at all.

### Read path is durability-agnostic

Read operations must never touch the durability layer. All reads are served from the in-memory store regardless of which mode is active. The durability mode only affects the write path.

This means:
- `kv_get`, `state_read`, `json_get`, `vector_get`, `vector_search`, `event_read` should all have identical latency across Cache, Standard, and Always.
- The only performance difference between modes is on write operations.

### Standard should be fast

The design target for Standard mode is write latency within 2-5x of Cache mode. Since writes go to memory + an in-memory buffer (no fsync in the hot path), the overhead should be minimal — just the cost of appending to the WAL buffer.

Current measured latencies (targets in parentheses):

| Operation | Cache | Standard (current) | Standard (target) |
|-----------|-------|--------------------|--------------------|
| kv/put (1KB) | 1.02 us | 6.12 ms | 2-5 us |
| state/set | 2.21 us | 6.13 ms | 4-10 us |
| event/append | 267 us | 6.53 ms | 300-500 us |

The current Standard mode is ~3,000-6,000x slower than target because it performs synchronous fsync on every write (same as Always). Fixing this is tracked in issue #969.

## Naming Rationale

The mode names were chosen for user understanding over technical accuracy:

- **Cache**: Users know what a cache is. It's fast, it's temporary, it might disappear. No explanation needed.
- **Standard**: "This is the normal one. Pick this if you're not sure." It's the default, it's what most people should use. The name deliberately avoids technical jargon.
- **Always**: Answers the user's question directly. "When is my data synced?" Always. One word, no ambiguity.

Previous naming (`NoDurability` / `Buffered` / `Strict`) was rejected because:
- `NoDurability` defines itself by negation — it says what's missing, not what it's for.
- `Buffered` describes the mechanism (we buffer writes), not the contract (your data is periodically synced).
- `Strict` sounds punitive, like you're being penalized for wanting safety.

## Migration

### Enum changes

```rust
// Before
pub enum DurabilityMode {
    None,
    Batched,
    Strict,
}

// After
pub enum DurabilityMode {
    Cache,
    Standard,
    Always,
}
```

### Builder changes

```rust
// Before
Database::ephemeral()
Database::builder().path(p).no_durability().open()
Database::builder().path(p).buffered().open()
Database::builder().path(p).strict().open()

// After
Database::cache()
Database::builder().path(p).open()              // standard is default
Database::builder().path(p).standard().open()   // explicit standard
Database::builder().path(p).always().open()
```
