# Run API: Git-Like Semantics

## Overview

Strata is a data substrate for AI agents - an embedded memory module that agents attach to for persistent, structured storage. A **Run** represents one execution of an agent, containing all the data that agent worked with during that execution.

This document proposes aligning the Run API with git-like semantics to provide intuitive, powerful operations for managing agent execution state.

## Core Concept: Run = Agent Execution

```
┌─────────────────────────────────────────────────────────────┐
│  Run: "customer-support-agent-2024-01-28"                   │
│                                                             │
│  ├── KV Store         → config, preferences, context        │
│  ├── State Cells      → current status, counters, flags     │
│  ├── Event Log        → audit trail of what happened        │
│  ├── JSON Documents   → structured data, schemas            │
│  └── Vector Store     → embeddings for semantic memory      │
└─────────────────────────────────────────────────────────────┘
```

### Use Cases

1. **Remember past executions**: "Last time this error occurred, how did we resolve it?"
2. **Experiment safely**: "Let me try a different approach without losing current progress"
3. **Compare outcomes**: "What was different between the successful run and the failed one?"
4. **Replay and debug**: "Step through what the agent did in this run"

## Git Command Mapping

| Git | Strata | Description | Status |
|-----|--------|-------------|--------|
| `git init` | `RunCreate` | Create new empty run | ✅ Implemented |
| `git fork`/`clone` | `RunFork` | Copy all data from source run | ❌ **Missing** |
| `git checkout` | `run: RunId` | Switch execution context | ✅ Works (parameter) |
| `git status` | `RunGet` | Get run info and status | ✅ Implemented |
| `git diff` | `RunDiff` | Compare two runs | ❌ **Missing** |
| `git log` | `EventRange` | View execution history | ✅ Implemented |
| `git tag` | `RunAddTags` | Label runs for querying | ✅ Implemented |
| `git merge` | `RunMerge` | Combine data from runs | ❌ Future |
| `git reset` | `RunReset` | Rollback to earlier state | ❌ Future |
| `git archive` | `RunExport` | Export run to portable bundle | ✅ Implemented |
| `git gc` | `Compact` | Storage cleanup | ✅ Implemented |

## Proposed API Changes

### 1. RunFork (Priority: Critical)

Replace the broken `RunCreateChild` with a proper fork operation.

```rust
/// Fork a run, creating a complete copy of all its data.
///
/// This is the git equivalent of `git clone` or forking a repo.
/// The new run starts with an exact copy of the source run's:
/// - KV entries
/// - State cells
/// - Event log
/// - JSON documents
/// - Vector collections and vectors
///
/// Subsequent changes to source or fork do not affect each other.
RunFork {
    /// The run to fork from
    source: RunId,

    /// Name for the new run (auto-generated if None)
    name: Option<String>,

    /// Optional metadata for the new run
    metadata: Option<Value>,
}

// Returns: RunInfo for the newly created fork
```

**Example usage:**
```rust
// Agent encounters a situation similar to a past run
let fork = executor.execute(Command::RunFork {
    source: RunId::from("successful-resolution-jan-15"),
    name: Some("current-attempt".into()),
    metadata: None,
});

// Now work with the forked data as a starting point
executor.execute(Command::KvGet {
    run: Some(RunId::from("current-attempt")),
    key: "resolution-strategy".into(),
});
```

### 2. RunDiff (Priority: High)

Compare two runs to understand what changed.

```rust
/// Compare two runs and return their differences.
///
/// This is the git equivalent of `git diff <a> <b>`.
RunDiff {
    /// First run (typically the "before" or "base")
    base: RunId,

    /// Second run (typically the "after" or "current")
    target: RunId,

    /// Which primitives to compare
    include: Option<DiffScope>,
}

pub enum DiffScope {
    All,
    Only(Vec<PrimitiveType>),  // KV, State, Event, JSON, Vector
}

pub struct RunDiffResult {
    /// Keys/cells/docs that exist in target but not base
    pub added: DiffEntries,

    /// Keys/cells/docs that exist in base but not target
    pub removed: DiffEntries,

    /// Keys/cells/docs that exist in both but have different values
    pub modified: DiffEntries,

    /// Summary statistics
    pub stats: DiffStats,
}
```

**Example usage:**
```rust
// Compare successful vs failed run
let diff = executor.execute(Command::RunDiff {
    base: RunId::from("failed-run"),
    target: RunId::from("successful-run"),
    include: None,  // All primitives
});

// Returns:
// added: ["resolution-strategy", "escalation-contact"]
// removed: ["retry-count"]
// modified: ["config.timeout" (30 -> 60)]
```

### 3. RunMerge (Priority: Medium, Future)

Combine changes from one run into another.

```rust
/// Merge changes from source run into target run.
///
/// This is the git equivalent of `git merge`.
RunMerge {
    /// Run containing changes to merge
    source: RunId,

    /// Run to merge into
    target: RunId,

    /// How to handle conflicts
    strategy: MergeStrategy,
}

pub enum MergeStrategy {
    /// Source wins on conflict
    SourceWins,

    /// Target wins on conflict
    TargetWins,

    /// Fail if any conflicts exist
    FailOnConflict,

    /// Return conflicts for manual resolution
    Manual,
}
```

### 4. RunReset (Priority: Medium, Future)

Rollback a run to an earlier state.

```rust
/// Reset a run to an earlier point in time.
///
/// This is the git equivalent of `git reset`.
RunReset {
    run: RunId,

    /// Target point to reset to
    target: ResetTarget,
}

pub enum ResetTarget {
    /// Reset to a specific version
    Version(u64),

    /// Reset to state at a specific timestamp
    Timestamp(u64),

    /// Reset to match another run's current state
    MatchRun(RunId),
}
```

## Terminology Changes

| Old | New | Rationale |
|-----|-----|-----------|
| `RunCreateChild` | `RunFork` | "Fork" clearly implies data copying |
| `parent` field | `forked_from` | Clearer relationship |
| `RunGetChildren` | `RunGetForks` | Consistent with fork terminology |
| `RunGetParent` | `RunGetSource` | "Source" is clearer than "parent" |

## Implementation Notes

### RunFork Implementation

```rust
pub fn run_fork(source: RunId, name: String) -> Result<RunInfo> {
    // 1. Verify source exists
    let source_meta = run_index.get_run(&source)?;

    // 2. Create new run with forked_from reference
    let fork_meta = run_index.create_run_with_options(
        &name,
        Some(source.to_string()),  // forked_from
        source_meta.tags.clone(),
        source_meta.metadata.clone(),
    )?;

    // 3. Copy all primitive data (atomic transaction)
    db.transaction(|txn| {
        // Copy KV entries
        for (key, value) in kv.scan(&source, "", None)? {
            kv.put_in_txn(txn, &fork_id, &key, value)?;
        }

        // Copy State cells
        for (cell, value) in state.list(&source)? {
            state.set_in_txn(txn, &fork_id, &cell, value)?;
        }

        // Copy Events
        for event in event_log.range(&source, 0, None, None)? {
            event_log.append_in_txn(txn, &fork_id, &event.stream, event.payload)?;
        }

        // Copy JSON documents
        for (key, doc) in json.list(&source)? {
            json.set_in_txn(txn, &fork_id, &key, "$", doc)?;
        }

        // Copy Vector collections and vectors
        for collection in vector.list_collections(&source)? {
            vector.create_collection_in_txn(txn, &fork_id, &collection)?;
            for vector in vector.list(&source, &collection)? {
                vector.upsert_in_txn(txn, &fork_id, &collection, vector)?;
            }
        }

        Ok(())
    })?;

    Ok(fork_meta)
}
```

### Performance Considerations

For large runs, copying all data on fork could be expensive. Future optimization:

1. **Copy-on-Write (COW)**: Share data until modification
2. **Lazy copying**: Copy data on first access
3. **Shallow fork option**: Fork metadata only, share data read-only

## Migration Path

1. **Phase 1**: Implement `RunFork` with full data copy
2. **Phase 2**: Deprecate `RunCreateChild`, add compatibility shim
3. **Phase 3**: Implement `RunDiff` for comparison
4. **Phase 4**: Add `RunMerge` for combining runs
5. **Phase 5**: Optimize with copy-on-write

## Related Issues

- #780: RunCreateChild doesn't copy parent's data
- #781: RunDelete doesn't remove run's data

## Open Questions

1. Should `RunFork` copy the event log? Events are immutable history - maybe fork should start with empty events?

2. How to handle vector collections? They can be large - maybe offer shallow vs deep fork?

3. Should forking preserve version numbers or start fresh?
