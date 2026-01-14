# M1+M2 Test Execution Prompt

Use this prompt to systematically execute the test suite and open issues for any failures.

---

## Execution Prompt

```
Execute the M1+M2 comprehensive test suite in the following order. Do NOT fix issues inline - instead, open GitHub issues for each failure.

## Phase 1: Compilation Check

Run:
```bash
cargo build --test m1_m2_comprehensive
```

If compilation fails, open an issue with label `bug`, `compilation`, `priority:critical`.

## Phase 2: Tier 1 - Core Invariants

These are sacred. Every failure here is critical.

Run each category and open issues for failures:

### WAL Invariants (M1.1-M1.8)
```bash
cargo test --test m1_m2_comprehensive wal_invariant -- --nocapture
```

### Snapshot Invariants (M2.2-M2.13)
```bash
cargo test --test m1_m2_comprehensive snapshot_invariant -- --nocapture
```

### ACID Properties (M2.1, M2.6, M2.14-M2.16)
```bash
cargo test --test m1_m2_comprehensive acid_property -- --nocapture
```

**For any Tier 1 failure:**
- Label: `bug`, `tier-1-invariant`, `priority:critical`
- Title: `[INVARIANT VIOLATION] {invariant_id}: {test_name}`
- Body must include:
  - Which invariant was violated (e.g., M2.3)
  - Full test output
  - Expected vs actual behavior

## Phase 3: Tier 2 - Behavioral Scenarios

Run:
```bash
cargo test --test m1_m2_comprehensive database_api -- --nocapture
cargo test --test m1_m2_comprehensive transaction_context -- --nocapture
cargo test --test m1_m2_comprehensive transaction_workflow -- --nocapture
cargo test --test m1_m2_comprehensive recovery -- --nocapture
cargo test --test m1_m2_comprehensive edge_case -- --nocapture
cargo test --test m1_m2_comprehensive error_handling -- --nocapture
```

**For any Tier 2 failure:**
- Label: `bug`, `tier-2-behavioral`, `priority:high`
- Title: `[TEST FAILURE] {test_module}: {test_name}`

## Phase 4: Tier 3 - Stress Tests (Optional)

Only run if Tier 1 and Tier 2 pass:
```bash
cargo test --test m1_m2_comprehensive concurrent_stress -- --ignored --nocapture
```

**For any Tier 3 failure:**
- Label: `bug`, `tier-3-stress`, `priority:medium`, `flaky?`
- Title: `[STRESS] {test_name}`
- Note: May need multiple runs to reproduce

## Issue Template

```markdown
## Test Information
- **Test**: `{test_name}`
- **File**: `tests/m1_m2_comprehensive/{file}.rs`
- **Invariant**: {invariant_id} (if applicable)
- **Tier**: {1|2|3}

## Failure Output
```
{paste full output}
```

## Expected Behavior
{what should have happened}

## Actual Behavior
{what actually happened}

## Reproduction
```bash
cargo test --test m1_m2_comprehensive {test_name} -- --nocapture
```
```

## Success Criteria

- [ ] All Tier 1 tests pass (0 invariant violations)
- [ ] All Tier 2 tests pass (0 behavioral failures)
- [ ] Tier 3 tests documented (failures tracked, not blocking)

Do NOT proceed to fix issues. Document them all first, then prioritize based on tier.
```

---

## Quick Reference

### Running All Tests
```bash
# Tier 1 + Tier 2 (default)
cargo test --test m1_m2_comprehensive

# Only Tier 1 core invariants
cargo test --test m1_m2_comprehensive invariant

# Only Tier 3 stress tests
cargo test --test m1_m2_comprehensive stress -- --ignored
```

### Issue Labels

| Label | Meaning |
|-------|---------|
| `tier-1-invariant` | Core invariant violation - critical |
| `tier-2-behavioral` | Workflow test failure - high priority |
| `tier-3-stress` | Stress test failure - medium priority |
| `priority:critical` | Blocks release |
| `priority:high` | Must fix soon |
| `priority:medium` | Fix when possible |
| `flaky?` | May be non-deterministic |

### Invariant Quick Reference

See [INVARIANTS.md](./INVARIANTS.md) for full definitions.

| ID | Name |
|----|------|
| M1.1-M1.8 | WAL semantics |
| M2.1 | Atomicity |
| M2.2 | No dirty reads |
| M2.3 | Repeatable reads |
| M2.4 | Read-your-writes |
| M2.5 | Snapshot consistency |
| M2.6 | No partial visibility |
| M2.7 | Read-write conflict |
| M2.8 | CAS conflict |
| M2.9 | First-committer-wins |
| M2.10 | Blind write behavior |
| M2.11 | CAS read-set independence |
| M2.12 | Version 0 semantics |
| M2.13 | Tombstone semantics |
| M2.14 | Incomplete txn discard |
| M2.15 | Lost update prevention |
| M2.16 | Write skew allowance |
