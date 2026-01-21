# Epic End Validation Plan

**Run this validation at the end of every epic before merging to develop.**

---

## Overview

Epic-end validation ensures:
1. All stories in the epic are complete and correct
2. Code quality meets standards
3. Implementation matches architecture spec
4. Tests are comprehensive and passing
5. No regressions introduced

---

## Phase 1: Automated Checks

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

## Phase 2: Story Completion Verification

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
ls -la crates/<package>/src/<expected_files>

# Check test count for the epic's module
~/.cargo/bin/cargo test --package <package> <module_name> -- --list 2>/dev/null | grep -c "test"
```

### 2.3 PR Status Check

```bash
# Verify all story PRs are merged to epic branch
gh pr list --state merged --base epic-<N>-<name> --json number,title

# Should match the number of stories in the epic
```

---

## Phase 3: Spec Compliance Review

### 3.1 Architecture Spec Compliance

Open the relevant architecture document for your milestone (e.g., `docs/architecture/M<N>_ARCHITECTURE.md`).

Verify implementation matches:

| Section | Requirement | Implemented Correctly |
|---------|-------------|----------------------|
| Key Design Decisions | All major decisions followed | [ ] |
| Public API | All methods match spec signatures | [ ] |
| Data Structures | Types and layouts match spec | [ ] |
| Invariants | All invariants enforced | [ ] |
| Error Handling | Error types match spec | [ ] |

### 3.2 Spec Deviation Check

Search for potential deviations:

```bash
# Look for TODOs or FIXMEs that might indicate spec deviations
grep -r "TODO\|FIXME\|HACK\|XXX" crates/<package>/src/

# Look for unwrap/expect that might indicate incomplete error handling
grep -r "\.unwrap()\|\.expect(" crates/<package>/src/ | wc -l
```

**Rule**: If ANY deviation from spec is found:
1. Document why
2. Get explicit approval
3. Create follow-up issue if needed

---

## Phase 4: Code Review Checklist

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
| **Thread safety** | Types are `Send + Sync` where required | [ ] |

### 4.4 Test Quality Review

| Item | Check | Status |
|------|-------|--------|
| **Happy path tested** | Normal operations work | [ ] |
| **Error cases tested** | Invalid inputs return appropriate errors | [ ] |
| **Edge cases tested** | Empty inputs, max values, boundaries | [ ] |
| **Concurrent access** | Thread safety verified where applicable | [ ] |
| **Integration tests** | Cross-component interactions tested | [ ] |

---

## Phase 5: Best Practices Verification

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

Run any milestone-specific validation tests and benchmarks as defined in the milestone's architecture document.

### 6.1 Run Epic-Specific Tests

```bash
# Run tests specific to the epic's module
~/.cargo/bin/cargo test --package <package> <module_name>

# Run integration tests if applicable
~/.cargo/bin/cargo test --test <integration_test_name>
```

### 6.2 Run Performance Benchmarks (if applicable)

```bash
# Run relevant benchmarks
~/.cargo/bin/cargo bench --bench <benchmark_name>
```

### 6.3 Verify Architectural Rules

Check that all architectural rules defined in the milestone's architecture document are followed. Use grep/search to verify patterns are correct.

### 6.4 Non-Regression Verification

```bash
# Verify previous milestone performance targets are maintained
~/.cargo/bin/cargo test --workspace

# Run any existing red flag tests
~/.cargo/bin/cargo test --test <red_flag_tests>
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
| Non-regression tests pass | [ ] |

### 7.2 Sign-Off

```
Epic: ___
Milestone: ___
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
3. Verify spec compliance against M<N>_ARCHITECTURE.md (Phase 3)
4. Perform code review checklist (Phase 4)
5. Verify best practices (Phase 5)
6. Run epic-specific validation (Phase 6)
7. Provide final sign-off summary (Phase 7)

**Expected Output**:
- Pass/fail status for each phase
- Any issues found with recommendations
- Final sign-off or list of blockers

**Reference**:
- Epic prompt: docs/prompts/<milestone>/epic-<N>-claude-prompts.md
- Architecture: docs/architecture/M<N>_ARCHITECTURE.md
- Stories: #XXX - #XXX
```

---

## Quick Reference: Validation Commands

### Quick Validation One-Liner

```bash
# One-liner for Phase 1
~/.cargo/bin/cargo build --workspace && \
~/.cargo/bin/cargo test --workspace && \
~/.cargo/bin/cargo clippy --workspace -- -D warnings && \
~/.cargo/bin/cargo fmt --check && \
echo "Phase 1: PASS"
```

### Count Tests

```bash
# Count tests in a package
~/.cargo/bin/cargo test --package <package> -- --list 2>/dev/null | grep -c "test"
```

### Check for Spec Deviations

```bash
grep -r "TODO\|FIXME\|HACK" crates/<package>/src/
```

---

*End of Epic End Validation Plan*
