# Development Workflow for Parallel Claude Development

## Branch Structure

```
main (protected)
  └── develop (integration branch)
      ├── epic-1-workspace-core-types
      ├── epic-2-storage-layer
      ├── epic-3-wal-implementation
      ├── epic-4-basic-recovery
      └── epic-5-database-engine
```

## Branches Explained

### `main` (Protected)
- **Purpose**: Production-ready code only
- **Rules**:
  - No direct commits
  - Only accepts PRs from `develop`
  - Requires all CI checks to pass
  - Requires code review
- **When to merge**: End of milestone (M1, M2, etc.)

### `develop` (Integration)
- **Purpose**: Integration branch for ongoing work
- **Rules**:
  - No direct commits
  - Accepts PRs from epic branches
  - All tests must pass
  - Automatic CI runs on every push
- **When to merge**: When epic is complete and tested

### Epic Branches (e.g., `epic-1-workspace-core-types`)
- **Purpose**: Long-lived feature branch for an entire epic
- **Rules**:
  - Created from `develop`
  - Accepts PRs from story branches
  - Can have failing tests while in progress
  - Merged to `develop` when epic is complete
- **Lifetime**: Duration of epic (3-5 days typically)

### Story Branches (e.g., `epic-1-story-6-cargo-workspace`)
- **Purpose**: Short-lived branch for a single user story
- **Rules**:
  - Created from epic branch
  - ONE Claude works on ONE story branch
  - Merged to epic branch when story is complete
  - Must pass all tests before merge
- **Lifetime**: Few hours to 1 day

## Workflow for Multiple Claudes

### Starting a New Story

**Claude 1 wants to work on Story #6:**

```bash
# Start from develop
git checkout develop
git pull origin develop

# Create epic branch (if it doesn't exist)
git checkout -b epic-1-workspace-core-types
git push -u origin epic-1-workspace-core-types

# Create story branch
git checkout -b epic-1-story-6-cargo-workspace
git push -u origin epic-1-story-6-cargo-workspace

# Work on the story...
# (implement code, write tests, etc.)

# Commit and push
git add .
git commit -m "Implement story #6: Setup Cargo workspace

- Add workspace Cargo.toml with 6 crates
- Define crate structure and dependencies
- Add initial crate manifests

Closes #6

Co-Authored-By: Claude Sonnet 4.5 <noreply@anthropic.com>"
git push
```

**Claude 2 works on Story #7 (parallel):**

```bash
# Start from the SAME epic branch
git checkout develop
git pull origin develop
git checkout epic-1-workspace-core-types  # Same epic!
git pull origin epic-1-workspace-core-types

# Create DIFFERENT story branch
git checkout -b epic-1-story-7-runid-namespace
git push -u origin epic-1-story-7-runid-namespace

# Work independently...
```

### Creating Pull Requests

**Story Branch → Epic Branch:**

```bash
# From story branch
gh pr create \
  --base epic-1-workspace-core-types \
  --head epic-1-story-6-cargo-workspace \
  --title "Story #6: Setup Cargo workspace" \
  --body "Implements #6

## Changes
- Add workspace Cargo.toml
- Create 6 crate directories
- Define dependencies

## Testing
- [ ] Workspace builds: \`cargo build --all\`
- [ ] No warnings: \`cargo check --all\`

## Checklist
- [x] Code written
- [x] Tests added (N/A for workspace setup)
- [x] Documentation updated
- [x] CI passes"
```

**Epic Branch → Develop:**

```bash
# When ALL stories in epic are complete
git checkout epic-1-workspace-core-types
git pull origin epic-1-workspace-core-types

gh pr create \
  --base develop \
  --head epic-1-workspace-core-types \
  --title "Epic #1: Workspace & Core Types (Complete)" \
  --body "Completes Epic #1

## Stories Completed
- #6 Setup Cargo workspace
- #7 Define RunId and Namespace types
- #8 Define Key and TypeTag enums
- #9 Define Value enum and VersionedValue
- #10 Define Error types
- #11 Define Storage and SnapshotView traits

## Testing
All unit tests pass. Core types are ready for use by Epic #2.

Closes #1"
```

**Develop → Main:**

```bash
# When entire milestone is complete
git checkout develop
git pull origin develop

gh pr create \
  --base main \
  --head develop \
  --title "Milestone 1: Foundation (Complete)" \
  --body "Completes M1

## Epics Completed
- Epic #1: Workspace & Core Types
- Epic #2: Storage Layer
- Epic #3: WAL Implementation
- Epic #4: Basic Recovery
- Epic #5: Database Engine Shell

## Deliverables
- ✅ Can store/retrieve KV pairs
- ✅ WAL appends entries
- ✅ Recovery from WAL works
- ✅ All 27 user stories complete
- ✅ Integration test passes

## Test Results
- Unit tests: 147 passed
- Integration tests: 12 passed
- Crash simulation: 5 passed
- Corruption simulation: 3 passed

Ready for M2."
```

## Parallel Work Assignment

### Epic 1: Workspace & Core Types (Can parallelize)

| Story | Claude | Branch | Dependencies |
|-------|--------|--------|--------------|
| #6 Workspace | Claude 1 | `epic-1-story-6-cargo-workspace` | None |
| #11 Traits | Claude 1 | `epic-1-story-11-storage-trait` | After #6 |
| #7 RunId/Namespace | Claude 2 | `epic-1-story-7-runid-namespace` | After #6 |
| #8 Key/TypeTag | Claude 3 | `epic-1-story-8-key-typetag` | After #6 |
| #9 Value/VersionedValue | Claude 4 | `epic-1-story-9-value-enum` | After #6 |
| #10 Error types | Claude 2 | `epic-1-story-10-error-types` | After #6 |

**Strategy**: Claude 1 does #6 first (workspace), then all others work in parallel on #7-#11.

### Epic 2: Storage Layer (Sequential dependencies)

| Story | Claude | Branch | Dependencies |
|-------|--------|--------|--------------|
| #12 UnifiedStore | Claude 1 | `epic-2-story-12-unified-store` | Epic 1 complete |
| #13 Indices | Claude 1 | `epic-2-story-13-indices` | After #12 |
| #14 TTL | Claude 2 | `epic-2-story-14-ttl` | After #12 (parallel with #13) |
| #15 Snapshot | Claude 3 | `epic-2-story-15-snapshot-view` | After #12 (parallel) |
| #16 Tests | Claude 4 | `epic-2-story-16-storage-tests` | After #12-15 |

**Strategy**: #12 blocks everything. Then #13, #14, #15 can run in parallel. #16 waits for all.

### Epic 3: WAL Implementation (Can parallelize)

| Story | Claude | Branch | Dependencies |
|-------|--------|--------|--------------|
| #17 WAL entries | Claude 1 | `epic-3-story-17-wal-entries` | Epic 1 complete |
| #18 Encoding | Claude 2 | `epic-3-story-18-encoding` | After #17 |
| #19 File ops | Claude 3 | `epic-3-story-19-file-ops` | After #17 |
| #20 Durability modes | Claude 4 | `epic-3-story-20-durability` | After #19 |
| #21 CRC | Claude 2 | `epic-3-story-21-crc` | After #18 (parallel with #19) |
| #22 Corruption tests | Claude 1 | `epic-3-story-22-corruption-tests` | After #21 |

### Epic 4: Basic Recovery (Sequential)

| Story | Claude | Branch | Dependencies |
|-------|--------|--------|--------------|
| #23 WAL replay | Claude 1 | `epic-4-story-23-wal-replay` | Epic 2 + Epic 3 |
| #24 Incomplete txns | Claude 2 | `epic-4-story-24-incomplete-txns` | After #23 |
| #25 Database::open | Claude 1 | `epic-4-story-25-database-open` | After #24 |
| #26 Crash simulation | Claude 3 | `epic-4-story-26-crash-sim` | After #25 |
| #27 Large WAL | Claude 4 | `epic-4-story-27-large-wal` | After #25 (parallel) |

### Epic 5: Database Engine (Some parallelization)

| Story | Claude | Branch | Dependencies |
|-------|--------|--------|--------------|
| #28 Database struct | Claude 1 | `epic-5-story-28-database-struct` | Epic 4 complete |
| #29 Run tracking | Claude 2 | `epic-5-story-29-run-tracking` | After #28 |
| #30 Put/Get | Claude 3 | `epic-5-story-30-put-get` | After #28 (parallel with #29) |
| #31 KV primitive | Claude 4 | `epic-5-story-31-kv-primitive` | After #30 |
| #32 Integration test | Claude 1 | `epic-5-story-32-integration-test` | After all |

## Merge Conflicts (How to Handle)

### If two Claudes modify the same epic branch:

**Example**: Claude 1 merged Story #7, Claude 2 has Story #8 in progress

```bash
# Claude 2's workflow
git checkout epic-1-story-8-key-typetag
git pull origin epic-1-workspace-core-types  # Get Claude 1's changes

# If conflicts:
git merge origin/epic-1-workspace-core-types
# Resolve conflicts
git add .
git commit -m "Merge latest epic-1 changes"
git push

# Continue working...
```

### If develop moves ahead:

**Example**: Epic 1 merged to develop, Epic 2 needs to rebase

```bash
git checkout epic-2-storage-layer
git pull origin develop  # Get latest develop
git rebase develop
# Resolve conflicts if any
git push --force-with-lease
```

## CI/CD Pipeline

Every PR runs:
1. ✅ `cargo build --all` (workspace builds)
2. ✅ `cargo test --all` (all tests pass)
3. ✅ `cargo fmt --all -- --check` (formatting)
4. ✅ `cargo clippy --all -- -D warnings` (linting)

## GitHub Branch Protection Rules

### For `main`:
```bash
gh api repos/anibjoshi/in-mem/branches/main/protection -X PUT -f required_status_checks='{"strict":true,"contexts":["test","build"]}' -f enforce_admins=true -f required_pull_request_reviews='{"required_approving_review_count":1}' -f restrictions=null
```

### For `develop`:
```bash
gh api repos/anibjoshi/in-mem/branches/develop/protection -X PUT -f required_status_checks='{"strict":true,"contexts":["test","build"]}' -f enforce_admins=false -f required_pull_request_reviews=null -f restrictions=null
```

## Quick Reference Commands

### Start new story:
```bash
./scripts/start-story.sh <epic-number> <story-number> <description>
# Example: ./scripts/start-story.sh 1 6 cargo-workspace
```

### Complete story (create PR):
```bash
./scripts/complete-story.sh <story-number>
# Example: ./scripts/complete-story.sh 6
```

### Sync with epic branch:
```bash
./scripts/sync-epic.sh <epic-number>
# Example: ./scripts/sync-epic.sh 1
```

## Communication Between Claudes

Use GitHub issue comments to coordinate:

**Example**: Claude 1 on Story #12 (UnifiedStore)

```
Comment on Issue #13: "I'm adding the Storage trait in #12.
It will have these methods: get, put, delete, scan_prefix, scan_by_run.
You can assume these exist when you work on secondary indices."
```

**Claude 2 on Story #13 sees this and can work confidently.**

## Best Practices

1. **One Claude, One Story**: Never have two Claudes on the same story branch
2. **Epic Coordination**: Use epic branch as integration point
3. **Frequent Sync**: Pull from epic branch before pushing to story branch
4. **Small PRs**: Story branches should be merged within hours, not days
5. **Test Early**: Don't wait for all stories to finish before testing integration
6. **Document Dependencies**: Comment on issues if your work affects other stories
7. **Rebase, Don't Merge**: Use rebase to keep history clean

## Troubleshooting

### "My branch is behind develop"
```bash
git checkout <your-branch>
git pull origin develop
git rebase develop
git push --force-with-lease
```

### "Two stories modified the same file"
- This is expected! Epic branch will handle the merge.
- When merging to epic, resolve conflicts carefully.
- Run full test suite after resolving.

### "CI is failing on my PR"
```bash
# Run locally first
cargo test --all
cargo fmt --all
cargo clippy --all -- -D warnings

# Fix issues, then push
git add .
git commit -m "Fix CI issues"
git push
```

## Summary

- **4-5 Claudes can work in parallel** on different stories within same epic
- **Epic branches** are integration points
- **Develop** is the M1 integration branch
- **Main** only gets completed milestones
- **Use GitHub issues** to communicate dependencies between stories
- **CI runs on every PR** to catch issues early

This workflow allows maximum parallelization while maintaining code quality and avoiding merge chaos.
