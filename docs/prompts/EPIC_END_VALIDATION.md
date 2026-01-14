# Epic End Validation Plan

**Run this validation at the end of every epic before merging to develop.**

---

## Overview

Epic-end validation ensures:
1. All stories in the epic are complete and correct
2. Code quality meets standards
3. Implementation matches M3 architecture spec
4. Tests are comprehensive and passing
5. No regressions introduced

---

## Phase 1: Automated Checks (5 minutes)

### 1.1 Build & Test Suite

```bash
# Full workspace build
~/.cargo/bin/cargo build --workspace

# Full test suite
~/.cargo/bin/cargo test --workspace

# Release mode tests (catches optimization-related issues)
~/.cargo/bin/cargo test --workspace --release
```

### 1.2 Code Quality

```bash
# Clippy with strict warnings
~/.cargo/bin/cargo clippy --workspace -- -D warnings

# Format check
~/.cargo/bin/cargo fmt --check

# Documentation builds without warnings
~/.cargo/bin/cargo doc --workspace --no-deps
```

### 1.3 Automated Check Summary

| Check | Command | Pass Criteria |
|-------|---------|---------------|
| Build | `cargo build --workspace` | Zero errors |
| Tests | `cargo test --workspace` | All pass |
| Release Tests | `cargo test --workspace --release` | All pass |
| Clippy | `cargo clippy --workspace -- -D warnings` | Zero warnings |
| Format | `cargo fmt --check` | No changes needed |
| Docs | `cargo doc --workspace --no-deps` | Builds without warnings |

---

## Phase 2: Story Completion Verification (10 minutes)

### 2.1 Story Checklist

For EACH story in the epic, verify:

| Story | Files Created/Modified | Tests Added | Acceptance Criteria Met |
|-------|------------------------|-------------|-------------------------|
| #XXX | [ ] | [ ] | [ ] |
| #XXX | [ ] | [ ] | [ ] |
| ... | | | |

### 2.2 Verify Story Deliverables

```bash
# Check that expected files exist
ls -la crates/primitives/src/<expected_files>

# Check test count for the epic's module
~/.cargo/bin/cargo test --package primitives <module_name> -- --list 2>/dev/null | grep -c "test"
```

### 2.3 PR Status Check

```bash
# Verify all story PRs are merged to epic branch
gh pr list --state merged --base epic-<N>-<name> --json number,title

# Should match the number of stories in the epic
```

---

## Phase 3: Spec Compliance Review (15 minutes)

### 3.1 Architecture Spec Compliance

Open `docs/architecture/M3_ARCHITECTURE.md` and verify implementation matches:

| Section | Requirement | Implemented Correctly |
|---------|-------------|----------------------|
| Section 3 | Stateless facades | [ ] Primitives hold only `Arc<Database>` |
| Section 4-8 | Primitive APIs | [ ] All methods match spec signatures |
| Section 9 | Key design | [ ] TypeTags and key formats correct |
| Section 10 | Transaction integration | [ ] Extension traits work |
| Section 12 | Invariant enforcement | [ ] Primitives enforce their invariants |

### 3.2 Spec Deviation Check

Search for potential deviations:

```bash
# Look for TODOs or FIXMEs that might indicate spec deviations
grep -r "TODO\|FIXME\|HACK\|XXX" crates/primitives/src/

# Look for unwrap/expect that might indicate incomplete error handling
grep -r "\.unwrap()\|\.expect(" crates/primitives/src/ | wc -l
```

**Rule**: If ANY deviation from spec is found:
1. Document why
2. Get explicit approval
3. Create follow-up issue if needed

---

## Phase 4: Code Review Checklist (20 minutes)

### 4.1 Structural Review

| Item | Check | Status |
|------|-------|--------|
| **Module organization** | Files in correct locations | [ ] |
| **Public API** | Only intended items are `pub` | [ ] |
| **Dependencies** | No unnecessary dependencies added | [ ] |
| **Re-exports** | lib.rs exports what users need | [ ] |

### 4.2 Code Quality Review

| Item | Check | Status |
|------|-------|--------|
| **Error handling** | Uses `Result<T, Error>`, no panics in library code | [ ] |
| **Naming** | Follows Rust conventions (snake_case, CamelCase) | [ ] |
| **Documentation** | Public items have doc comments | [ ] |
| **No dead code** | No unused functions, structs, or imports | [ ] |
| **No debug code** | No `println!`, `dbg!`, or debug logging left in | [ ] |

### 4.3 Safety Review

| Item | Check | Status |
|------|-------|--------|
| **No unsafe** | No `unsafe` blocks (unless justified and documented) | [ ] |
| **No unwrap on user input** | All user-provided data validated | [ ] |
| **No panic paths** | Library code returns errors, doesn't panic | [ ] |
| **Thread safety** | Primitives are `Send + Sync` | [ ] |

### 4.4 Test Quality Review

| Item | Check | Status |
|------|-------|--------|
| **Happy path tested** | Normal operations work | [ ] |
| **Error cases tested** | Invalid inputs return appropriate errors | [ ] |
| **Edge cases tested** | Empty inputs, max values, boundaries | [ ] |
| **Concurrent access** | Thread safety verified where applicable | [ ] |
| **Integration tests** | Cross-component interactions tested | [ ] |

---

## Phase 5: Best Practices Verification (10 minutes)

### 5.1 Rust Best Practices

| Practice | Verified |
|----------|----------|
| Use `&str` for input, `String` for owned data | [ ] |
| Prefer iterators over manual loops | [ ] |
| Use `?` operator for error propagation | [ ] |
| Derive traits where appropriate (`Debug`, `Clone`, etc.) | [ ] |
| Use `#[must_use]` for functions with important return values | [ ] |

### 5.2 Project-Specific Best Practices

| Practice | Verified |
|----------|----------|
| Primitives are stateless (only hold `Arc<Database>`) | [ ] |
| All operations scoped to `RunId` | [ ] |
| Keys use correct `TypeTag` | [ ] |
| Extension traits delegate to primitive internals | [ ] |
| Tests follow TDD - written before implementation | [ ] |

### 5.3 Performance Considerations

| Item | Verified |
|------|----------|
| No unnecessary allocations in hot paths | [ ] |
| No holding locks across await points | [ ] |
| Efficient key construction | [ ] |
| Batch operations where appropriate | [ ] |

---

## Phase 6: Epic-Specific Validation

### For Each M3 Epic:

#### Epic 13: Foundation
```bash
# Verify Key helpers work correctly
~/.cargo/bin/cargo test --package primitives key_

# Verify TypeTags are correct
grep -r "TypeTag::" crates/primitives/src/
```

#### Epic 14: KVStore
```bash
# Verify CRUD operations
~/.cargo/bin/cargo test --package primitives kv_

# Verify list with prefix
~/.cargo/bin/cargo test --package primitives test_kv_list
```

#### Epic 15: EventLog
```bash
# Verify chain integrity
~/.cargo/bin/cargo test --package primitives test_event_chain

# Verify append-only (no update/delete)
grep -r "fn update\|fn delete" crates/primitives/src/event_log.rs
# Should show methods that return InvalidOperation error
```

#### Epic 16: StateCell
```bash
# Verify CAS semantics
~/.cargo/bin/cargo test --package primitives test_state_cas

# Verify transition purity documented
grep -r "purity\|pure\|Purity" crates/primitives/src/state_cell.rs
```

#### Epic 17: TraceStore
```bash
# Verify indices work
~/.cargo/bin/cargo test --package primitives test_trace_query

# Verify parent-child relationships
~/.cargo/bin/cargo test --package primitives test_trace_tree
```

#### Epic 18: RunIndex
```bash
# Verify status transitions
~/.cargo/bin/cargo test --package primitives test_status_transition

# Verify cascading delete
~/.cargo/bin/cargo test --package primitives test_delete_run
```

#### Epic 19: Integration
```bash
# Run all integration tests
~/.cargo/bin/cargo test --package primitives --test cross_primitive_tests
~/.cargo/bin/cargo test --package primitives --test run_isolation_tests
~/.cargo/bin/cargo test --package primitives --test recovery_tests

# Run benchmarks
~/.cargo/bin/cargo bench --package primitives
```

---

## Phase 7: Final Sign-Off

### 7.1 Completion Checklist

| Item | Status |
|------|--------|
| All automated checks pass (Phase 1) | [ ] |
| All stories verified complete (Phase 2) | [ ] |
| Spec compliance confirmed (Phase 3) | [ ] |
| Code review complete (Phase 4) | [ ] |
| Best practices verified (Phase 5) | [ ] |
| Epic-specific validation done (Phase 6) | [ ] |

### 7.2 Sign-Off

```
Epic: ___
Reviewer: ___
Date: ___

[ ] This epic is APPROVED for merge to develop

Notes:
_______________________
```

---

## Post-Validation: Merge to Develop

After all phases pass:

```bash
# 1. Ensure epic branch is up to date
git checkout epic-<N>-<name>
git pull origin epic-<N>-<name>

# 2. Final test run
~/.cargo/bin/cargo test --workspace

# 3. Merge to develop
git checkout develop
git pull origin develop
git merge --no-ff epic-<N>-<name> -m "Epic <N>: <Epic Name> complete

Delivered:
- Story #XXX: Description
- Story #XXX: Description
...

All validation phases passed."

# 4. Push
git push origin develop

# 5. Close epic issue
gh issue close <epic-issue-number> --comment "Epic complete. All stories delivered and validated."
```

---

## Validation Prompt Template

Use this prompt to run epic-end validation:

```
## Task: Epic End Validation

Run the complete epic-end validation for Epic <N>: <Epic Name>.

**Steps**:
1. Run Phase 1 automated checks
2. Verify all <X> stories are complete (Phase 2)
3. Verify spec compliance against M3_ARCHITECTURE.md (Phase 3)
4. Perform code review checklist (Phase 4)
5. Verify best practices (Phase 5)
6. Run epic-specific validation (Phase 6)
7. Provide final sign-off summary (Phase 7)

**Expected Output**:
- Pass/fail status for each phase
- Any issues found with recommendations
- Final sign-off or list of blockers

**Reference**:
- Epic prompt: docs/prompts/epic-<N>-claude-prompts.md
- M3 Architecture: docs/architecture/M3_ARCHITECTURE.md
- Stories: #XXX - #XXX
```

---

## Quick Reference: Validation Commands

```bash
# One-liner for Phase 1
~/.cargo/bin/cargo build --workspace && \
~/.cargo/bin/cargo test --workspace && \
~/.cargo/bin/cargo clippy --workspace -- -D warnings && \
~/.cargo/bin/cargo fmt --check && \
echo "Phase 1: PASS"

# Count tests in primitives
~/.cargo/bin/cargo test --package primitives -- --list 2>/dev/null | grep -c "test"

# Check for spec deviations
grep -r "TODO\|FIXME\|HACK" crates/primitives/src/
```

---

*End of Epic End Validation Plan*
