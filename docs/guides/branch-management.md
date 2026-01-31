# Branch Management Guide

This guide covers the complete API for creating, switching, listing, and deleting branches. For the conceptual overview, see [Concepts: Branches](../concepts/branches.md).

## Opening and Default Branch

When you open a database, a "default" branch is automatically created and set as current:

```rust
let db = Strata::open_temp()?;
assert_eq!(db.current_branch(), "default");
```

## Creating Branches

`create_branch` creates a new empty branch. It does **not** switch to it:

```rust
let db = Strata::open_temp()?;

db.create_branch("experiment-1")?;
assert_eq!(db.current_branch(), "default"); // Still on default

// Duplicate names fail
let result = db.create_branch("experiment-1");
assert!(result.is_err()); // BranchExists error
```

## Switching Branches

`set_branch` changes the current branch. All subsequent data operations target the new branch:

```rust
let mut db = Strata::open_temp()?;

db.create_branch("my-branch")?;
db.set_branch("my-branch")?;
assert_eq!(db.current_branch(), "my-branch");

// Switching to a nonexistent branch fails
let result = db.set_branch("nonexistent");
assert!(result.is_err()); // BranchNotFound error
```

## Listing Branches

`list_branches` returns all branch names:

```rust
let db = Strata::open_temp()?;

db.create_branch("branch-a")?;
db.create_branch("branch-b")?;

let branches = db.list_branches()?;
// Contains: "default", "branch-a", "branch-b"
assert!(branches.contains(&"default".to_string()));
assert!(branches.contains(&"branch-a".to_string()));
```

## Deleting Branches

`delete_branch` removes a branch and all its data (KV, Events, State, JSON, Vectors):

```rust
let db = Strata::open_temp()?;

db.create_branch("temp")?;
db.delete_branch("temp")?;
```

### Safety Rules

```rust
let mut db = Strata::open_temp()?;

// Cannot delete the current branch
db.create_branch("my-branch")?;
db.set_branch("my-branch")?;
let result = db.delete_branch("my-branch");
assert!(result.is_err()); // ConstraintViolation

// Switch away first, then delete
db.set_branch("default")?;
db.delete_branch("my-branch")?; // Works

// Cannot delete the default branch
let result = db.delete_branch("default");
assert!(result.is_err()); // ConstraintViolation
```

## Power API: `db.branches()`

The `branches()` method returns a `Branches` handle for advanced operations:

```rust
let db = Strata::open_temp()?;

// Create
db.branches().create("experiment")?;

// Check existence
assert!(db.branches().exists("experiment")?);

// List
let all = db.branches().list()?;

// Delete
db.branches().delete("experiment")?;
assert!(!db.branches().exists("experiment")?);
```

## Low-Level Branch API

For more control, use the lower-level `run_*` methods that return full `RunInfo`:

```rust
let db = Strata::open_temp()?;

// Create with explicit ID
let (info, version) = db.branch_create(Some("my-branch-id".to_string()), None)?;
println!("Created branch: {} at version {}", info.id.as_str(), version);

// Get branch info
let info = db.branch_get("my-branch-id")?;
if let Some(versioned) = info {
    println!("Status: {:?}", versioned.info.status);
    println!("Created at: {}", versioned.info.created_at);
}

// List with full info
let branches = db.branch_list(None, None, None)?;
for branch in &branches {
    println!("{}: {:?}", branch.info.id.as_str(), branch.info.status);
}

// Check existence
assert!(db.branch_exists("my-branch-id")?);

// Delete
db.branch_delete("my-branch-id")?;
```

## Future Features

These operations are planned but not yet implemented:

- **`fork_branch(destination)`** — Copy all data from the current branch to a new branch
- **`branches().diff(run1, run2)`** — Compare two branches and return their differences

Both currently return `NotImplemented` errors:

```rust
let result = db.fork_branch("copy");
assert!(matches!(result, Err(Error::NotImplemented { .. })));
```

## Next

- [Sessions and Transactions](sessions-and-transactions.md) — multi-operation atomicity
- [Branch Bundles](branch-bundles.md) — exporting and importing branches
