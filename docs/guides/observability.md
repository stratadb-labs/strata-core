# Observability Guide

StrataDB uses the [`tracing`](https://docs.rs/tracing) crate for structured logging. Instrumentation is built into the engine but produces zero overhead unless you wire up a subscriber in your application.

## How It Works

StrataDB emits `tracing` spans and events on key code paths. Your application controls what gets collected by configuring a `tracing` subscriber. If no subscriber is set up, all instrumentation is compiled away to no-ops.

```rust
use tracing_subscriber::EnvFilter;

// Wire up a subscriber before opening the database
tracing_subscriber::fmt()
    .with_env_filter(EnvFilter::from_default_env())
    .init();

let db = Strata::open("/data/myapp")?;
// Now you'll see structured log output
```

## Subsystem Targets

StrataDB defines 10 subsystem targets for fine-grained log control:

| Target | Subsystem | What Gets Logged |
|--------|-----------|------------------|
| `strata::branch` | Branch management | Branch create, delete, switch |
| `strata::space` | Space management | Space registration, deletion |
| `strata::command` | Command dispatch | Command execution, routing |
| `strata::txn` | Transactions | Begin, commit, abort, validation, conflicts |
| `strata::db` | Database | Database-level operations, lifecycle |
| `strata::wal` | Write-Ahead Log | Record append, rotation, flush, sync |
| `strata::snapshot` | Snapshots | Snapshot writing, atomic operations |
| `strata::recovery` | Recovery | Participant registration, WAL replay |
| `strata::compaction` | Compaction | WAL compaction operations |
| `strata::vector` | Vector store | Upsert, search, collection management |

## Log Levels

Each subsystem uses standard `tracing` levels:

| Level | When Used |
|-------|-----------|
| `error` | Unrecoverable failures — corruption detected, recovery failure |
| `warn` | Recoverable issues — transaction conflicts, retries, degraded performance |
| `info` | Key lifecycle events — branch created, transaction committed, snapshot written |
| `debug` | Detailed operational data — individual key operations, search parameters |

## Configuration

Use the `RUST_LOG` environment variable to control which targets and levels are active:

```bash
# All strata logs at info level
RUST_LOG=info

# Transaction debugging
RUST_LOG=strata::txn=debug

# Multiple subsystems
RUST_LOG=strata::txn=debug,strata::wal=info,strata::recovery=debug

# Everything at debug (verbose)
RUST_LOG=strata=debug

# Only errors
RUST_LOG=strata=error
```

## Setting Up a Subscriber

### Basic Console Output

```rust
use tracing_subscriber::EnvFilter;

tracing_subscriber::fmt()
    .with_env_filter(EnvFilter::from_default_env())
    .init();
```

### JSON Output

For structured log consumption (log aggregators, monitoring):

```rust
use tracing_subscriber::EnvFilter;

tracing_subscriber::fmt()
    .json()
    .with_env_filter(EnvFilter::from_default_env())
    .init();
```

### Custom Subscriber

```rust
use tracing_subscriber::{fmt, EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

tracing_subscriber::registry()
    .with(fmt::layer().with_target(true))
    .with(EnvFilter::new("strata::txn=debug,strata::wal=info"))
    .init();
```

## Example Output

With `RUST_LOG=strata::txn=debug,strata::branch=info`:

```
2026-02-04T10:30:00Z  INFO strata::branch: branch created name="experiment-1"
2026-02-04T10:30:00Z DEBUG strata::txn: transaction begun txn_id=42 branch="default"
2026-02-04T10:30:00Z DEBUG strata::txn: transaction committed txn_id=42 writes=3
```

## Zero Overhead

When no subscriber is configured, `tracing` macros compile to no-ops. There is no runtime cost — no string formatting, no allocation, no I/O. You only pay for what you collect.

## Next

- [Database Configuration](database-configuration.md) — durability modes and opening methods
- [Error Handling](error-handling.md) — error categories and patterns
