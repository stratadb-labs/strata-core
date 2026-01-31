# Run Management Guide

This guide covers the complete API for creating, switching, listing, and deleting runs. For the conceptual overview, see [Concepts: Runs](../concepts/runs.md).

## Opening and Default Run

When you open a database, a "default" run is automatically created and set as current:

```rust
let db = Strata::open_temp()?;
assert_eq!(db.current_run(), "default");
```

## Creating Runs

`create_run` creates a new empty run. It does **not** switch to it:

```rust
let db = Strata::open_temp()?;

db.create_run("experiment-1")?;
assert_eq!(db.current_run(), "default"); // Still on default

// Duplicate names fail
let result = db.create_run("experiment-1");
assert!(result.is_err()); // RunExists error
```

## Switching Runs

`set_run` changes the current run. All subsequent data operations target the new run:

```rust
let mut db = Strata::open_temp()?;

db.create_run("my-run")?;
db.set_run("my-run")?;
assert_eq!(db.current_run(), "my-run");

// Switching to a nonexistent run fails
let result = db.set_run("nonexistent");
assert!(result.is_err()); // RunNotFound error
```

## Listing Runs

`list_runs` returns all run names:

```rust
let db = Strata::open_temp()?;

db.create_run("run-a")?;
db.create_run("run-b")?;

let runs = db.list_runs()?;
// Contains: "default", "run-a", "run-b"
assert!(runs.contains(&"default".to_string()));
assert!(runs.contains(&"run-a".to_string()));
```

## Deleting Runs

`delete_run` removes a run and all its data (KV, Events, State, JSON, Vectors):

```rust
let db = Strata::open_temp()?;

db.create_run("temp")?;
db.delete_run("temp")?;
```

### Safety Rules

```rust
let mut db = Strata::open_temp()?;

// Cannot delete the current run
db.create_run("my-run")?;
db.set_run("my-run")?;
let result = db.delete_run("my-run");
assert!(result.is_err()); // ConstraintViolation

// Switch away first, then delete
db.set_run("default")?;
db.delete_run("my-run")?; // Works

// Cannot delete the default run
let result = db.delete_run("default");
assert!(result.is_err()); // ConstraintViolation
```

## Power API: `db.runs()`

The `runs()` method returns a `Runs` handle for advanced operations:

```rust
let db = Strata::open_temp()?;

// Create
db.runs().create("experiment")?;

// Check existence
assert!(db.runs().exists("experiment")?);

// List
let all = db.runs().list()?;

// Delete
db.runs().delete("experiment")?;
assert!(!db.runs().exists("experiment")?);
```

## Low-Level Run API

For more control, use the lower-level `run_*` methods that return full `RunInfo`:

```rust
let db = Strata::open_temp()?;

// Create with explicit ID
let (info, version) = db.run_create(Some("my-run-id".to_string()), None)?;
println!("Created run: {} at version {}", info.id.as_str(), version);

// Get run info
let info = db.run_get("my-run-id")?;
if let Some(versioned) = info {
    println!("Status: {:?}", versioned.info.status);
    println!("Created at: {}", versioned.info.created_at);
}

// List with full info
let runs = db.run_list(None, None, None)?;
for run in &runs {
    println!("{}: {:?}", run.info.id.as_str(), run.info.status);
}

// Check existence
assert!(db.run_exists("my-run-id")?);

// Delete
db.run_delete("my-run-id")?;
```

## Future Features

These operations are planned but not yet implemented:

- **`fork_run(destination)`** — Copy all data from the current run to a new run
- **`runs().diff(run1, run2)`** — Compare two runs and return their differences

Both currently return `NotImplemented` errors:

```rust
let result = db.fork_run("copy");
assert!(matches!(result, Err(Error::NotImplemented { .. })));
```

## Next

- [Sessions and Transactions](sessions-and-transactions.md) — multi-operation atomicity
- [Run Bundles](run-bundles.md) — exporting and importing runs
