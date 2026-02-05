# Branch Management Guide

This guide covers the complete CLI for creating, switching, listing, and deleting branches. For the conceptual overview, see [Concepts: Branches](../concepts/branches.md).

## Opening and Default Branch

When you open the CLI, a "default" branch is automatically created and set as current:

```
$ strata --cache
strata:default/default>
```

## Creating Branches

`branch create` creates a new empty branch. It does **not** switch to it:

```
$ strata --cache
strata:default/default> branch create experiment-1
OK
strata:default/default> branch create experiment-1
(error) BranchExists: branch "experiment-1" already exists
```

## Switching Branches

`use <branch>` changes the current branch. All subsequent data operations target the new branch:

```
$ strata --cache
strata:default/default> branch create my-branch
OK
strata:default/default> use my-branch
strata:my-branch/default> use nonexistent
(error) BranchNotFound: branch "nonexistent" does not exist
```

## Listing Branches

`branch list` returns all branch names:

```
$ strata --cache
strata:default/default> branch create branch-a
OK
strata:default/default> branch create branch-b
OK
strata:default/default> branch list
- branch-a
- branch-b
- default
```

## Deleting Branches

`branch del` removes a branch and all its data (KV, Events, State, JSON, Vectors):

```
$ strata --cache
strata:default/default> branch create temp
OK
strata:default/default> branch del temp
OK
```

### Safety Rules

```
$ strata --cache
strata:default/default> branch create my-branch
OK
strata:default/default> use my-branch
strata:my-branch/default> branch del my-branch
(error) ConstraintViolation: cannot delete current branch
strata:my-branch/default> use default
strata:default/default> branch del my-branch
OK
strata:default/default> branch del default
(error) ConstraintViolation: cannot delete default branch
```

## Branch Info

Get detailed information about a branch:

```
$ strata --cache
strata:default/default> branch create my-branch
OK
strata:default/default> branch info my-branch
id: my-branch
status: active
```

## Branch Existence Check

```
$ strata --cache
strata:default/default> branch create experiment
OK
strata:default/default> branch exists experiment
true
strata:default/default> branch exists nonexistent
false
```

## Fork a Branch

Fork creates an exact copy of a branch, including all data across all primitives and spaces:

```
$ strata --cache
strata:default/default> kv put key value
(version) 1
strata:default/default> branch fork experiment-1
OK
strata:default/default> use experiment-1
strata:experiment-1/default> kv get key
"value"
```

## Diff Branches

Compare two branches to see what's different:

```bash
strata --cache branch diff branch-a branch-b
```

## Merge Branches

Merge data from one branch into another:

```bash
# Last-writer-wins: source values overwrite target on conflict
strata --cache branch merge source --strategy lww

# Strict: fails if any conflicts exist
strata --cache branch merge source --strategy strict
```

## Shell Mode

All branch operations work from the shell too:

```bash
strata --cache branch create experiment
strata --cache branch list
strata --cache branch exists experiment
strata --cache --branch experiment kv put key value
strata --cache branch del experiment
```

## Next

- [Sessions and Transactions](sessions-and-transactions.md) — multi-operation atomicity
- [Branch Bundles](branch-bundles.md) — exporting and importing branches
