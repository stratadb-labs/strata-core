# Branches

A **branch** is an isolated namespace for data. All data in StrataDB lives inside a branch. Branches are the core isolation mechanism — they keep data from different agent sessions, experiments, or workflows separate from each other.

## The Git Analogy

If you know git, you already understand branches:

| Git | StrataDB | Description |
|-----|----------|-------------|
| Repository | Database | The whole storage, opened once per path |
| Working directory | CLI session | Your view into the database with a current branch |
| Branch | Branch | An isolated namespace for data |
| HEAD | Current branch | The branch all operations target |
| `main` | `"default"` | The auto-created branch you start on |

Just as git branches isolate file changes, branches isolate data changes. Switching branches changes which data you see, without copying anything.

## How Branches Work

When you open the CLI, you start on the **default branch**:

```
$ strata --cache
strata:default/default> kv put key value
(version) 1
```

You can create additional branches and switch between them:

```
$ strata --cache
strata:default/default> kv put key default-value
(version) 1
strata:default/default> branch create experiment
OK
strata:default/default> use experiment
strata:experiment/default> kv get key
(nil)
strata:experiment/default> kv put key experiment-value
(version) 1
strata:experiment/default> use default
strata:default/default> kv get key
"default-value"
```

## Data Isolation

Every primitive (KV, EventLog, StateCell, JSON, Vector) is isolated by branch. Data written in one branch is invisible from another:

```
$ strata --cache
strata:default/default> kv put kv-key 1
(version) 1
strata:default/default> state set cell active
(version) 1
strata:default/default> event append log '{"msg":"hello"}'
(seq) 1
strata:default/default> branch create isolated
OK
strata:default/default> use isolated
strata:isolated/default> kv get kv-key
(nil)
strata:isolated/default> state get cell
(nil)
strata:isolated/default> event len
0
```

## Branch Lifecycle

| Operation | CLI Command | Notes |
|-----------|-------------|-------|
| Create | `branch create <name>` | Creates an empty branch. Stays on current branch. |
| Switch | `use <branch>` | Switches current branch. Branch must exist. |
| List | `branch list` | Returns all branch names. |
| Delete | `branch del <name>` | Deletes branch and all its data. Cannot delete current or default branch. |
| Check info | `branch info <name>` | Returns branch details. |
| Check existence | `branch exists <name>` | Returns whether the branch exists. |

### Safety Rules

- You **cannot delete the current branch**. Switch to a different branch first.
- You **cannot delete the "default" branch**. It always exists.
- You **cannot switch to a branch that doesn't exist**. Create it first.
- Creating a branch does **not** switch to it. You must call `use` explicitly.

## When to Use Branches

| Scenario | Pattern |
|----------|---------|
| Each agent session gets its own state | One branch per session ID |
| A/B testing different strategies | One branch per variant |
| Safe experimentation | Fork-like: create branch, experiment, delete if bad |
| Audit trail | Keep completed branches around for review |
| Multi-tenant isolation | One branch per tenant |

## Branch Operations

For advanced branch operations:

```
$ strata --cache
strata:default/default> branch create my-branch
OK
strata:default/default> branch list
- default
- my-branch
strata:default/default> branch exists my-branch
true
strata:default/default> branch del my-branch
OK
```

Or from the shell:

```bash
strata --cache branch create my-branch
strata --cache branch list
strata --cache branch exists my-branch
strata --cache branch del my-branch
```

## Branch Internals

Under the hood, every key in storage is prefixed with its branch ID. When you run `kv put key value`, the storage layer stores it under `{branch_id}:kv:key`. This makes branch isolation automatic — no filtering needed, because the keys are simply different.

This also means:
- Deleting a branch is O(branch size), scanning only that branch's keys
- Branches share no state, so they cannot conflict with each other
- Cross-branch operations (fork, diff, merge) are available — see the [Branch Management Guide](../guides/branch-management.md)

## Next

- [Primitives](primitives.md) — the six data types
- [Branch Management Guide](../guides/branch-management.md) — complete API walkthrough
