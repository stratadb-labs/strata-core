# Database Configuration Guide

This guide covers the different ways to open a StrataDB database and configure its behavior.

## Opening Methods

### Ephemeral (In-Memory)

For testing and development. No files are created on disk.

```rust
let db = Strata::open_temp()?;
```

### Persistent

Creates or opens a database at the specified path:

```rust
let db = Strata::open("/path/to/data")?;
```

If the directory doesn't exist, it is created. If a database already exists at that path, it is opened and any WAL entries are replayed for recovery.

### From Existing Database

When you need more control over the database lifecycle (e.g., sharing between multiple `Strata` instances):

```rust
use std::sync::Arc;
use stratadb::strata_engine::Database;

let database = Arc::new(Database::open("/path/to/data")?);
let db = Strata::from_database(database)?;
```

## Database Operations

### Ping

Verify the database is responsive:

```rust
let version = db.ping()?;
println!("StrataDB version: {}", version);
```

### Info

Get database statistics:

```rust
let info = db.info()?;
println!("Version: {}", info.version);
println!("Uptime: {} seconds", info.uptime_secs);
println!("Branches: {}", info.branch_count);
println!("Total keys: {}", info.total_keys);
```

### Flush

Force pending writes to disk (relevant in Buffered durability mode):

```rust
db.flush()?;
```

### Compact

Trigger storage compaction:

```rust
db.compact()?;
```

## Thread Safety

The `Strata` struct is not `Sync`, but the underlying `Database` is thread-safe. To use StrataDB from multiple threads:

1. Share the `Arc<Database>` between threads
2. Create a separate `Strata` or `Session` per thread

```rust
use std::sync::Arc;

let database = Arc::new(Database::open("./data")?);

let handle = std::thread::spawn({
    let db = database.clone();
    move || {
        let strata = Strata::from_database(db).unwrap();
        strata.kv_put("from-thread", "hello").unwrap();
    }
});

handle.join().unwrap();
```

## Next

- [Branch Bundles](branch-bundles.md) — exporting and importing branches
- [Error Handling](error-handling.md) — error categories
- [Configuration Reference](../reference/configuration-reference.md) — all options
