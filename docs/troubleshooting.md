# Troubleshooting

Common issues and their solutions.

## Data Not Visible After Writing

**Symptom:** You wrote data with `kv_put` but `kv_get` returns `None`.

**Likely cause:** You are on a different run than where you wrote the data.

**Fix:** Check your current run with `db.current_run()` and make sure you are reading from the same run where you wrote:

```rust
println!("Current run: {}", db.current_run());
```

All data in StrataDB is run-scoped. Data written in one run is invisible from another. See [Runs](concepts/runs.md).

## TransactionConflict Error

**Symptom:** `Error::TransactionConflict` when committing a transaction.

**Cause:** Another transaction modified data that your transaction read between your begin and commit.

**Fix:** Retry the entire transaction:

```rust
loop {
    session.execute(Command::TxnBegin { run: None, options: None })?;
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

## Cannot Delete Current Run

**Symptom:** `Error::ConstraintViolation` when deleting a run.

**Cause:** You are trying to delete the run you are currently on, or the "default" run.

**Fix:** Switch to a different run before deleting:

```rust
db.set_run("default")?;
db.delete_run("the-run-to-delete")?;
```

The "default" run cannot be deleted.

## RunNotFound When Switching

**Symptom:** `Error::RunNotFound` when calling `set_run`.

**Cause:** The run doesn't exist yet.

**Fix:** Create it first:

```rust
db.create_run("my-run")?;
db.set_run("my-run")?;
```

## RunExists When Creating

**Symptom:** `Error::RunExists` when calling `create_run`.

**Cause:** A run with that name already exists.

**Fix:** Check existence first, or ignore the error:

```rust
match db.create_run("my-run") {
    Ok(()) => {},
    Err(Error::RunExists { .. }) => {}, // Already exists
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

**Cause:** The vector collection hasn't been created yet, or you are on a different run.

**Fix:** Create the collection first in the current run:

```rust
db.vector_create_collection("my-collection", 384, DistanceMetric::Cosine)?;
```

Remember: collections are run-scoped. Creating a collection in one run doesn't make it available in another.

## NotImplemented Error

**Symptom:** `Error::NotImplemented` for `fork_run` or `diff_runs`.

**Cause:** These features are planned but not yet available.

**Workaround:** For forking, create a new run and manually copy the data you need. For diffing, read from both runs and compare in your application code.

## Getting Help

If your issue isn't listed here:
- Check the [FAQ](faq.md)
- Check the [Error Reference](reference/error-reference.md) for your specific error variant
- File an issue at [GitHub Issues](https://github.com/anibjoshi/strata/issues)
