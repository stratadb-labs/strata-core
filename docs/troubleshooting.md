# Troubleshooting

Common issues and their solutions.

## Data Not Visible After Writing

**Symptom:** You wrote data with `kv_put` but `kv_get` returns `None`.

**Likely cause:** You are on a different branch than where you wrote the data.

**Fix:** Check your current branch with `db.current_branch()` and make sure you are reading from the same branch where you wrote:

```rust
println!("Current branch: {}", db.current_branch());
```

All data in StrataDB is branch-scoped. Data written in one branch is invisible from another. See [Branches](concepts/branches.md).

## TransactionConflict Error

**Symptom:** `Error::TransactionConflict` when committing a transaction.

**Cause:** Another transaction modified data that your transaction read between your begin and commit.

**Fix:** Retry the entire transaction:

```rust
loop {
    session.execute(Command::TxnBegin { branch: None, options: None })?;
    // ... your operations ...
    match session.execute(Command::TxnCommit) {
        Ok(_) => break,
        Err(Error::TransactionConflict { .. }) => continue,
        Err(e) => return Err(e),
    }
}
```

See [Transactions](concepts/transactions.md).

## DimensionMismatch Error

**Symptom:** `Error::DimensionMismatch` when upserting a vector.

**Cause:** The vector you are inserting has a different number of dimensions than the collection was created with.

**Fix:** Ensure your vector length matches the collection's dimension:

```rust
// If collection was created with dimension 384:
db.vector_create_collection("col", 384, DistanceMetric::Cosine)?;

// Your vector must have exactly 384 elements:
let embedding = vec![0.0f32; 384]; // correct
db.vector_upsert("col", "key", embedding, None)?;
```

## Cannot Delete Current Branch

**Symptom:** `Error::ConstraintViolation` when deleting a branch.

**Cause:** You are trying to delete the branch you are currently on, or the "default" branch.

**Fix:** Switch to a different branch before deleting:

```rust
db.set_branch("default")?;
db.delete_branch("the-branch-to-delete")?;
```

The "default" branch cannot be deleted.

## BranchNotFound When Switching

**Symptom:** `Error::BranchNotFound` when calling `set_branch`.

**Cause:** The branch doesn't exist yet.

**Fix:** Create it first:

```rust
db.create_branch("my-branch")?;
db.set_branch("my-branch")?;
```

## BranchExists When Creating

**Symptom:** `Error::BranchExists` when calling `create_branch`.

**Cause:** A branch with that name already exists.

**Fix:** Check existence first, or ignore the error:

```rust
match db.create_branch("my-branch") {
    Ok(()) => {},
    Err(Error::BranchExists { .. }) => {}, // Already exists
    Err(e) => return Err(e),
}
```

## Event Append Fails

**Symptom:** Error when calling `event_append`.

**Cause:** Event payloads must be `Value::Object`. Passing a string, integer, or other type will fail.

**Fix:** Wrap your data in an object:

```rust
// Wrong: passing a string directly
// db.event_append("log", Value::String("hello".into()))?;

// Correct: wrap in an object
let payload: Value = serde_json::json!({"message": "hello"}).into();
db.event_append("log", payload)?;
```

## CollectionNotFound for Vectors

**Symptom:** `Error::CollectionNotFound` when upserting or searching vectors.

**Cause:** The vector collection hasn't been created yet, or you are on a different branch.

**Fix:** Create the collection first in the current branch:

```rust
db.vector_create_collection("my-collection", 384, DistanceMetric::Cosine)?;
```

Remember: collections are branch-scoped. Creating a collection in one branch doesn't make it available in another.

## NotImplemented Error

**Symptom:** `Error::NotImplemented` for `fork_branch` or `diff_branches`.

**Cause:** These features are planned but not yet available.

**Workaround:** For forking, create a new branch and manually copy the data you need. For diffing, read from both branches and compare in your application code.

## Getting Help

If your issue isn't listed here:
- Check the [FAQ](faq.md)
- Check the [Error Reference](reference/error-reference.md) for your specific error variant
- File an issue at [GitHub Issues](https://github.com/anibjoshi/strata/issues)
