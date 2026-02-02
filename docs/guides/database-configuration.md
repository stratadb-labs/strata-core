# Database Configuration Guide

This guide covers the different ways to open a StrataDB database and configure its behavior.

## Opening Methods

### Ephemeral (In-Memory)

For testing and development. No files are created on disk.

```rust
let db = Strata::cache()?;
```

### Persistent

Creates or opens a database at the specified path:

```rust
let db = Strata::open("/path/to/data")?;
```

If the directory doesn't exist, it is created. If a database already exists at that path, it is opened and any WAL entries are replayed for recovery.

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

`Strata` is `Send` but not `Sync`. To use StrataDB from multiple threads, create a separate `Strata` instance per thread pointing to the same path:

```rust
let handle = std::thread::spawn(move || {
    let strata = Strata::open("./data").unwrap();
    strata.kv_put("from-thread", "hello").unwrap();
});

handle.join().unwrap();
```

## Next

- [Branch Bundles](branch-bundles.md) — exporting and importing branches
- [Error Handling](error-handling.md) — error categories
- [Configuration Reference](../reference/configuration-reference.md) — all options
