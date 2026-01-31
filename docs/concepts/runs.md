# Runs

A **run** is an isolated namespace for data. All data in StrataDB lives inside a run. Runs are the core isolation mechanism — they keep data from different agent sessions, experiments, or workflows separate from each other.

## The Git Analogy

If you know git, you already understand runs:

| Git | StrataDB | Description |
|-----|----------|-------------|
| Repository | `Database` | The whole storage, opened once per path |
| Working directory | `Strata` | Your view into the database with a current run |
| Branch | Run | An isolated namespace for data |
| HEAD | `current_run()` | The run all operations target |
| `main` | `"default"` | The auto-created run you start on |

Just as git branches isolate file changes, runs isolate data changes. Switching runs changes which data you see, without copying anything.

## How Runs Work

When you open a database, you start on the **default run**:

```rust
let db = Strata::open_temp()?;
assert_eq!(db.current_run(), "default");

db.kv_put("key", "value")?; // Written to the "default" run
```

You can create additional runs and switch between them:

```rust
let mut db = Strata::open_temp()?;

// Write to default
db.kv_put("key", "default-value")?;

// Create and switch to a new run
db.create_run("experiment")?;
db.set_run("experiment")?;

// "key" doesn't exist here — runs are isolated
assert!(db.kv_get("key")?.is_none());

// Write to the experiment run
db.kv_put("key", "experiment-value")?;

// Switch back — default data is intact
db.set_run("default")?;
assert_eq!(db.kv_get("key")?, Some(Value::String("default-value".into())));
```

## Data Isolation

Every primitive (KV, EventLog, StateCell, JSON, Vector) is isolated by run. Data written in one run is invisible from another:

```rust
let mut db = Strata::open_temp()?;

// Write data in default
db.kv_put("kv-key", 1i64)?;
db.state_set("cell", "active")?;
db.event_append("log", payload)?;

// Switch to a different run
db.create_run("isolated")?;
db.set_run("isolated")?;

// Nothing from default is visible
assert!(db.kv_get("kv-key")?.is_none());
assert!(db.state_read("cell")?.is_none());
assert_eq!(db.event_len()?, 0);
```

## Run Lifecycle

| Operation | Method | Notes |
|-----------|--------|-------|
| Create | `create_run("name")` | Creates an empty run. Stays on current run. |
| Switch | `set_run("name")` | Switches current run. Run must exist. |
| List | `list_runs()` | Returns all run names. |
| Delete | `delete_run("name")` | Deletes run and all its data. Cannot delete current or default run. |
| Check current | `current_run()` | Returns the name of the current run. |

### Safety Rules

- You **cannot delete the current run**. Switch to a different run first.
- You **cannot delete the "default" run**. It always exists.
- You **cannot switch to a run that doesn't exist**. Create it first.
- Creating a run does **not** switch to it. You must call `set_run()` explicitly.

## When to Use Runs

| Scenario | Pattern |
|----------|---------|
| Each agent session gets its own state | One run per session ID |
| A/B testing different strategies | One run per variant |
| Safe experimentation | Fork-like: create run, experiment, delete if bad |
| Audit trail | Keep completed runs around for review |
| Multi-tenant isolation | One run per tenant |

## Power API

For advanced run operations, use `db.runs()`:

```rust
// List all runs
for run in db.runs().list()? {
    println!("Run: {}", run);
}

// Check if a run exists
if db.runs().exists("my-run")? {
    db.runs().delete("my-run")?;
}
```

## Run Internals

Under the hood, every key in storage is prefixed with its run ID. When you call `db.kv_put("key", value)`, the storage layer stores it under `{run_id}:kv:key`. This makes run isolation automatic — no filtering needed, because the keys are simply different.

This also means:
- Deleting a run is O(run size), scanning only that run's keys
- Runs share no state, so they cannot conflict with each other
- Cross-run operations (like fork and diff) are planned but not yet implemented

## Next

- [Primitives](primitives.md) — the six data types
- [Run Management Guide](../guides/run-management.md) — complete API walkthrough
