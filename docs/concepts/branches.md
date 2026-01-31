# Branches

A **branch** is an isolated namespace for data. All data in StrataDB lives inside a branch. Branches are the core isolation mechanism — they keep data from different agent sessions, experiments, or workflows separate from each other.

## The Git Analogy

If you know git, you already understand branches:

| Git | StrataDB | Description |
|-----|----------|-------------|
| Repository | `Database` | The whole storage, opened once per path |
| Working directory | `Strata` | Your view into the database with a current branch |
| Branch | Branch | An isolated namespace for data |
| HEAD | `current_branch()` | The branch all operations target |
| `main` | `"default"` | The auto-created branch you start on |

Just as git branches isolate file changes, branches isolate data changes. Switching branches changes which data you see, without copying anything.

## How Branches Work

When you open a database, you start on the **default branch**:

```rust
let db = Strata::open_temp()?;
assert_eq!(db.current_branch(), "default");

db.kv_put("key", "value")?; // Written to the "default" branch
```

You can create additional branches and switch between them:

```rust
let mut db = Strata::open_temp()?;

// Write to default
db.kv_put("key", "default-value")?;

// Create and switch to a new branch
db.create_branch("experiment")?;
db.set_branch("experiment")?;

// "key" doesn't exist here — branches are isolated
assert!(db.kv_get("key")?.is_none());

// Write to the experiment branch
db.kv_put("key", "experiment-value")?;

// Switch back — default data is intact
db.set_branch("default")?;
assert_eq!(db.kv_get("key")?, Some(Value::String("default-value".into())));
```

## Data Isolation

Every primitive (KV, EventLog, StateCell, JSON, Vector) is isolated by branch. Data written in one branch is invisible from another:

```rust
let mut db = Strata::open_temp()?;

// Write data in default
db.kv_put("kv-key", 1i64)?;
db.state_set("cell", "active")?;
db.event_append("log", payload)?;

// Switch to a different branch
db.create_branch("isolated")?;
db.set_branch("isolated")?;

// Nothing from default is visible
assert!(db.kv_get("kv-key")?.is_none());
assert!(db.state_read("cell")?.is_none());
assert_eq!(db.event_len()?, 0);
```

## Branch Lifecycle

| Operation | Method | Notes |
|-----------|--------|-------|
| Create | `create_branch("name")` | Creates an empty branch. Stays on current branch. |
| Switch | `set_branch("name")` | Switches current branch. Branch must exist. |
| List | `list_branches()` | Returns all branch names. |
| Delete | `delete_branch("name")` | Deletes branch and all its data. Cannot delete current or default branch. |
| Check current | `current_branch()` | Returns the name of the current branch. |

### Safety Rules

- You **cannot delete the current branch**. Switch to a different branch first.
- You **cannot delete the "default" branch**. It always exists.
- You **cannot switch to a branch that doesn't exist**. Create it first.
- Creating a branch does **not** switch to it. You must call `set_branch()` explicitly.

## When to Use Branches

| Scenario | Pattern |
|----------|---------|
| Each agent session gets its own state | One branch per session ID |
| A/B testing different strategies | One branch per variant |
| Safe experimentation | Fork-like: create branch, experiment, delete if bad |
| Audit trail | Keep completed branches around for review |
| Multi-tenant isolation | One branch per tenant |

## Power API

For advanced branch operations, use `db.branches()`:

```rust
// List all branches
for branch in db.branches().list()? {
    println!("Branch: {}", branch);
}

// Check if a branch exists
if db.branches().exists("my-branch")? {
    db.branches().delete("my-branch")?;
}
```

## Branch Internals

Under the hood, every key in storage is prefixed with its branch ID. When you call `db.kv_put("key", value)`, the storage layer stores it under `{branch_id}:kv:key`. This makes branch isolation automatic — no filtering needed, because the keys are simply different.

This also means:
- Deleting a branch is O(branch size), scanning only that branch's keys
- Branches share no state, so they cannot conflict with each other
- Cross-branch operations (like fork and diff) are planned but not yet implemented

## Next

- [Primitives](primitives.md) — the six data types
- [Branch Management Guide](../guides/branch-management.md) — complete API walkthrough
