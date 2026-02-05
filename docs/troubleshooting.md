# Troubleshooting

Common issues and their solutions.

## Data Not Visible After Writing

**Symptom:** You wrote data with `kv put` but `kv get` returns `(nil)`.

**Likely cause:** You are on a different branch or space than where you wrote the data.

**Fix:** Check your current branch and space — the prompt shows them:

```
strata:default/default>
       ^^^^^^^  ^^^^^^^
       branch   space
```

All data in StrataDB is branch-scoped and space-scoped. Data written in one branch is invisible from another. See [Branches](concepts/branches.md).

## TransactionConflict Error

**Symptom:** `TransactionConflict` error when committing a transaction.

**Cause:** Another transaction modified data that your transaction read between your begin and commit.

**Fix:** Retry the entire transaction:

```
$ strata --db ./data
strata:default/default> begin
OK
strata:default/default> kv put key value
(version) 1
strata:default/default> commit
(error) TransactionConflict: key was modified by another transaction
strata:default/default> begin
OK
strata:default/default> kv put key value
(version) 1
strata:default/default> commit
OK
```

In scripts, use a retry loop:

```bash
for attempt in 1 2 3 4 5; do
    strata --db ./data <<'EOF' && break
begin
kv put key value
commit
EOF
    echo "Conflict on attempt $attempt, retrying..."
done
```

See [Transactions](concepts/transactions.md).

## DimensionMismatch Error

**Symptom:** `DimensionMismatch` error when upserting a vector.

**Cause:** The vector you are inserting has a different number of dimensions than the collection was created with.

**Fix:** Ensure your vector length matches the collection's dimension:

```
$ strata --cache
strata:default/default> vector create col 384 --metric cosine
OK
strata:default/default> vector upsert col key [0.0,0.0,...,0.0]
OK
```

The vector must have exactly the same number of elements as the collection's dimension (384 in this example).

## Cannot Delete Current Branch

**Symptom:** `ConstraintViolation` error when deleting a branch.

**Cause:** You are trying to delete the branch you are currently on, or the "default" branch.

**Fix:** Switch to a different branch before deleting:

```
$ strata --cache
strata:my-branch/default> use default
strata:default/default> branch del my-branch
OK
```

The "default" branch cannot be deleted.

## BranchNotFound When Switching

**Symptom:** `BranchNotFound` error when running `use`.

**Cause:** The branch doesn't exist yet.

**Fix:** Create it first:

```
strata:default/default> branch create my-branch
OK
strata:default/default> use my-branch
strata:my-branch/default>
```

## BranchExists When Creating

**Symptom:** `BranchExists` error when running `branch create`.

**Cause:** A branch with that name already exists.

**Fix:** Check existence first, or ignore the error:

```bash
# Shell: ignore the error if branch already exists
strata --cache branch create my-branch 2>/dev/null || true
```

```
$ strata --cache
strata:default/default> branch exists my-branch
true
```

## Event Append Fails

**Symptom:** Error when running `event append`.

**Cause:** Event payloads must be JSON objects. Passing a plain string or number will fail.

**Fix:** Wrap your data in a JSON object:

```
$ strata --cache
strata:default/default> event append log '{"message":"hello"}'
(seq) 1
```

## CollectionNotFound for Vectors

**Symptom:** `CollectionNotFound` error when upserting or searching vectors.

**Cause:** The vector collection hasn't been created yet, or you are on a different branch.

**Fix:** Create the collection first in the current branch:

```
$ strata --cache
strata:default/default> vector create my-collection 384 --metric cosine
OK
```

Collections are branch-scoped. Creating a collection in one branch doesn't make it available in another.

## Getting Help

If your issue isn't listed here:
- Check the [FAQ](faq.md)
- Check the [Error Reference](reference/error-reference.md) for your specific error variant
- File an issue at [GitHub Issues](https://github.com/anibjoshi/strata/issues)
