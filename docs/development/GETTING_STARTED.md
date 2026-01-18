# Getting Started with in-mem Development

Quick start guide for beginning work on this project with multiple Claude instances.

## Initial Setup (Do Once)

### 1. Clone Repository
```bash
git clone https://github.com/anibjoshi/in-mem.git
cd in-mem
```

### 2. Verify Prerequisites
```bash
# Rust 1.70+
rustc --version

# GitHub CLI
gh --version

# Authenticate if needed
gh auth status
```

### 3. Understand the Structure

**Key Documents:**
- [README.md](../../README.md) - Project overview
- [M1_ARCHITECTURE.md](../architecture/M1_ARCHITECTURE.md) - Complete M1 architecture spec
- [DEVELOPMENT_WORKFLOW.md](DEVELOPMENT_WORKFLOW.md) - Git workflow for parallel development
- [CLAUDE_COORDINATION.md](CLAUDE_COORDINATION.md) - How multiple Claudes coordinate

**Branch Structure:**
```
main (protected)
  └── develop (integration)
      └── epic-{N}-{name}
          └── epic-{N}-story-{M}-{description}
```

## Starting Your First Story

### Step 1: Choose a Story

Check [CLAUDE_COORDINATION.md](CLAUDE_COORDINATION.md) to see which stories are available.

**Epic 1 Stories (Start Here):**
- ✅ Story #6: Setup Cargo workspace (MUST DO FIRST)
- Story #7: Define RunId and Namespace types
- Story #8: Define Key and TypeTag enums
- Story #9: Define Value enum and VersionedValue
- Story #10: Define Error types
- Story #11: Define Storage and SnapshotView traits

**Rule**: Story #6 must complete before others can start.

### Step 2: Create Story Branch

```bash
# Example: Starting story #6
./scripts/start-story.sh 1 6 cargo-workspace

# This will:
# 1. Create epic-1-workspace-core-types (if needed)
# 2. Create epic-1-story-6-cargo-workspace
# 3. Check out the story branch
```

### Step 3: Read the User Story

```bash
# View the full story details
gh issue view 6
```

**Each story includes:**
- User story format ("As a... I want... So that...")
- Context and background
- Acceptance criteria
- Implementation guidance
- Testing requirements
- Effort estimate

### Step 4: Implement the Story

Follow the **Test-Driven Development** approach from [DEVELOPMENT_WORKFLOW.md](DEVELOPMENT_WORKFLOW.md):

**For Story #6 (Workspace):**
```bash
# 1. Create workspace structure
mkdir -p crates/{core,storage,concurrency,durability,primitives,engine,api}

# 2. Create workspace Cargo.toml
# (See issue #6 for complete code)

# 3. Create crate manifests
# (See issue #6 for all crate Cargo.toml files)

# 4. Verify workspace builds
cargo build --all
```

**For Stories #7-11 (Core Types):**
```bash
# 1. Write tests first (TDD)
# 2. Implement types to pass tests
# 3. Run tests: cargo test --all
# 4. Run formatting: cargo fmt --all
# 5. Run linting: cargo clippy --all -- -D warnings
```

**See [TDD_METHODOLOGY.md](TDD_METHODOLOGY.md) for complete testing strategy and examples.**

### Step 5: Complete the Story

```bash
# Runs all checks and creates PR
./scripts/complete-story.sh 6

# This will:
# 1. Run cargo test --all
# 2. Run cargo fmt --all -- --check
# 3. Run cargo clippy --all
# 4. Push to origin
# 5. Create PR to epic branch
```

### Step 6: Update Coordination

Post in GitHub issue:
```bash
gh issue comment 6 --body "✅ Story #6 complete!

PR created: (link)
Branch: epic-1-story-6-cargo-workspace
All checks passing.

Ready for next story."
```

## Working in Parallel (Multiple Claudes)

### Communication Protocol

**Before starting:**
```bash
# Claim the story
gh issue comment 7 --body "Starting work on Story #7.
ETA: 3-4 hours.
Branch: epic-1-story-7-runid-namespace"
```

**If blocked:**
```bash
# Ask for help
gh issue comment 7 --body "Blocked on Story #11 (Storage trait).
Can I assume the trait will have get/put/delete methods?
Or should I wait for #11 to merge?"
```

**When complete:**
```bash
# Notify completion
gh issue comment 7 --body "✅ Story #7 complete!
PR: #XYZ
Tests passing.
Next: Starting #10 (Error types)"
```

### Checking for Conflicts

Before merging, check if other stories have merged to epic:

```bash
# Pull latest epic branch
git checkout epic-1-workspace-core-types
git pull origin epic-1-workspace-core-types

# Switch back to your story
git checkout epic-1-story-7-runid-namespace

# Rebase onto latest epic
git rebase epic-1-workspace-core-types

# If conflicts, resolve and continue
git add <files>
git rebase --continue
```

### Viewing Other PRs

```bash
# See all PRs for epic 1
gh pr list --base epic-1-workspace-core-types

# View specific PR
gh pr view 42

# Check CI status
gh pr checks 42
```

## Common Tasks

### Sync with Epic Branch
```bash
./scripts/sync-epic.sh 1
```

### Run Full Test Suite
```bash
cargo test --all
```

### Run Formatting
```bash
cargo fmt --all
```

### Run Linting
```bash
cargo clippy --all -- -D warnings
```

### Build Release
```bash
cargo build --release --all
```

### View Current Work
```bash
# See what you're working on
git branch --show-current

# See what others are working on
gh pr list --base epic-1-workspace-core-types
```

### Switch to Different Story
```bash
# Complete current story first!
./scripts/complete-story.sh <current-story>

# Then start new story
./scripts/start-story.sh <epic> <story> <description>
```

## Recommended Story Order (Epic 1)

**Phase 1: Foundation (1 Claude)**
1. Story #6: Cargo workspace - **BLOCKS EVERYTHING**

**Phase 2: Core Types (4 Claudes in parallel)**
2. Story #11: Storage trait (Claude 1) - 2-3 hours
3. Story #7: RunId/Namespace (Claude 2) - 3-4 hours
4. Story #8: Key/TypeTag (Claude 3) - 4-5 hours
5. Story #9: Value/VersionedValue (Claude 4) - 4-5 hours

**Phase 3: Error Handling (1 Claude)**
6. Story #10: Error types - 2 hours - Waits for #7-9

**Total**: ~8-10 hours with 4 Claudes (vs. 20+ hours single Claude)

## Testing Your Work

### Unit Tests
```bash
# Run all tests
cargo test --all

# Run specific crate tests
cargo test -p in-mem-core

# Run specific test
cargo test test_run_id_creation
```

### Integration Tests
```bash
# Run integration tests (once Epic 5 complete)
cargo test --test '*'
```

### Check Coverage
```bash
# Install tarpaulin
cargo install cargo-tarpaulin

# Run coverage
cargo tarpaulin --all --out Html
```

## Troubleshooting

### "Tests are failing"
```bash
# Run tests locally first
cargo test --all

# Fix issues, then try again
./scripts/complete-story.sh <story>
```

### "Formatting issues"
```bash
# Auto-format
cargo fmt --all

# Check formatting
cargo fmt --all -- --check
```

### "Clippy warnings"
```bash
# See warnings
cargo clippy --all

# Fix warnings, or allow specific ones:
#[allow(clippy::large_enum_variant)]
```

### "My branch is behind epic"
```bash
# Sync with epic
./scripts/sync-epic.sh <epic-number>

# Push with force (safe because it's your branch)
git push --force-with-lease
```

### "Merge conflict"
```bash
# Sync first (will show conflict)
./scripts/sync-epic.sh <epic-number>

# Resolve conflicts in editor
# Look for <<<<<<< HEAD markers

# Mark as resolved
git add <files>

# Continue rebase
git rebase --continue

# Push
git push --force-with-lease
```

### "I'm blocked on another story"
Two options:

**Option 1: Mock the dependency**
```rust
// Assume the trait will exist
trait Storage {
    fn get(&self, key: &Key) -> Option<Value>;
    // ...
}

// Implement against assumed trait
// When #11 merges, you'll find out if you were right!
```

**Option 2: Work on different story**
```bash
# Check CLAUDE_COORDINATION.md for available stories
# Pick one without dependencies
```

## Epic Completion

When all stories in an epic are complete:

```bash
# Check all PRs merged to epic
gh pr list --base epic-1-workspace-core-types --state merged

# Create PR from epic to develop
git checkout epic-1-workspace-core-types
git pull origin epic-1-workspace-core-types

gh pr create \
  --base develop \
  --head epic-1-workspace-core-types \
  --title "Epic #1: Workspace & Core Types (Complete)" \
  --body "Completes Epic #1

## Stories
- #6 Cargo workspace
- #7 RunId/Namespace
- #8 Key/TypeTag
- #9 Value/VersionedValue
- #10 Error types
- #11 Storage trait

All tests passing. Ready for Epic #2."
```

## Next Steps

1. ✅ Read this guide
2. ✅ Choose a story from [CLAUDE_COORDINATION.md](CLAUDE_COORDINATION.md)
3. ✅ Run `./scripts/start-story.sh <epic> <story> <description>`
4. ✅ Read the issue: `gh issue view <story>`
5. ✅ Implement with TDD
6. ✅ Run `./scripts/complete-story.sh <story>`
7. ✅ Comment on issue with status
8. ✅ Pick next story!

## Questions?

- Read [M1_ARCHITECTURE.md](M1_ARCHITECTURE.md) for technical details
- Read [DEVELOPMENT_WORKFLOW.md](DEVELOPMENT_WORKFLOW.md) for Git workflow
- Read [CLAUDE_COORDINATION.md](CLAUDE_COORDINATION.md) for coordination
- Check GitHub issues for context
- Ask in issue comments if blocked

---

**Ready to start? Run:**
```bash
./scripts/start-story.sh 1 6 cargo-workspace
```
