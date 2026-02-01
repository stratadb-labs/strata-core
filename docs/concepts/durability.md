# Durability

StrataDB offers three durability modes that trade off between speed and crash safety. You choose the mode when opening the database.

## The Three Modes

| Mode | Latency | Throughput | Data Loss on Crash | Use Case |
|------|---------|------------|-------------------|----------|
| **Ephemeral** | <3 us | 250K+ ops/sec | All data | Testing, ephemeral workloads |
| **Buffered** | <30 us | 50K+ ops/sec | Last ~100ms | Production default |
| **Strict** | ~2 ms | ~500 ops/sec | None | Financial, audit-critical |

### Ephemeral (None)

No persistence at all. Data exists only in memory and is lost when the process exits. Use `Strata::open_temp()` for this mode.

- Fastest possible performance
- No disk I/O
- Use for testing, benchmarks, and ephemeral agent sessions

### Buffered (Batched)

Writes go to a Write-Ahead Log (WAL) but `fsync` is batched — the OS is asked to flush to disk periodically (every ~100ms or ~1000 writes, whichever comes first).

- The default production mode
- If the process crashes, the OS may not have flushed the last batch of writes
- On restart, recovery replays the WAL to restore committed state
- Acceptable for most AI agent workloads where losing 100ms of work is tolerable

### Strict

Every commit triggers an immediate `fsync` to disk. The commit call does not return until data is durably stored.

- Slowest but safest
- Zero data loss even on power failure
- Use when every operation matters (financial records, audit logs)

## Write-Ahead Log (WAL)

Regardless of durability mode, all writes follow the WAL protocol:

1. **Write to WAL** — the change is recorded in the log file before anything else
2. **Apply to memory** — the in-memory data structures are updated
3. **Acknowledge** — the operation returns success

This ensures that committed data can always be recovered from the WAL, even if the in-memory state is lost.

### WAL Entry Format

Each entry is self-describing and integrity-checked:

```
[length: 4 bytes][type: 1 byte][payload: N bytes][crc32: 4 bytes]
```

CRC32 checksums detect corruption from bit flips, partial writes, and disk errors.

## Snapshots

Snapshots are periodic full-state captures that speed up recovery. Instead of replaying the entire WAL from the beginning, recovery loads the latest snapshot and replays only the WAL entries after it.

Benefits:
- **Bounded recovery time** — recovery is O(WAL entries since last snapshot), not O(total history)
- **WAL truncation** — entries before the snapshot can be removed, keeping disk usage bounded

## Crash Recovery

When a database opens, if a WAL file exists, StrataDB automatically runs recovery:

1. Load the latest snapshot (if any)
2. Read all WAL entries after the snapshot
3. Group entries by transaction ID
4. Apply only committed transactions (those with a `CommitTxn` entry)
5. Discard incomplete transactions (the process crashed mid-transaction)
6. Preserve exact version numbers from the WAL

Recovery is:
- **Deterministic** — same WAL + same snapshot = same state, always
- **Idempotent** — running recovery twice produces the same result
- **Prefix-consistent** — no partial transactions are visible

## Choosing a Mode

For most applications, **Buffered** is the right choice. It provides a good balance of performance and durability. Consider:

- **Testing?** → No Durability (`Strata::open_temp()`)
- **Production agent workloads?** → Buffered (default)
- **Cannot lose any data?** → Strict
- **Unsure?** → Start with Buffered and switch to Strict if needed

## Next

- [Database Configuration Guide](../guides/database-configuration.md) — how to configure durability modes
- [Architecture: Durability and Recovery](../architecture/durability-and-recovery.md) — deep dive into WAL format, snapshots, and recovery internals
