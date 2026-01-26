# Test Suite Report

**Generated:** 2026-01-24
**Branch:** develop
**Commit:** fe7405d (Implement RunBundle MVP)

---

## Summary

| Category | Passed | Failed | Ignored | Status |
|----------|--------|--------|---------|--------|
| **Unit Tests** | 2,139 | 5 | 0 | ⚠️ |
| **Milestone Tests** | 2,177 | 28 | 14 | ⚠️ |
| **Cross-Milestone** | ~100+ | ~2 | 0 | ⏳ (timeout) |
| **Substrate API** | 776 | 0 | 1 | ✅ |
| **Facade API** | 72 | 0 | 0 | ✅ |

---

## Unit Tests (Per Crate)

| Crate | Passed | Failed | Status |
|-------|--------|--------|--------|
| strata-core | 427 | 0 | ✅ |
| strata-storage | 410 | 0 | ✅ |
| strata-concurrency | 278 | 0 | ✅ |
| strata-durability | 285 | 0 | ✅ |
| strata-engine | 148 | 0 | ✅ |
| strata-primitives | 424 | 4 | ⚠️ |
| strata-search | 58 | 0 | ✅ |
| strata-api | 85 | 1 | ⚠️ |

---

## Milestone Tests

| Milestone | Passed | Failed | Ignored | Status |
|-----------|--------|--------|---------|--------|
| M1-M2 Comprehensive | 320 | 0 | 0 | ✅ |
| M3 Comprehensive | 271 | 9 | 14 | ⚠️ |
| M4 Regression | 83 | 1 | 0 | ⚠️ |
| M5 Comprehensive | 195 | 0 | 4 | ✅ |
| M6 Comprehensive | 123 | 2 | 6 | ⚠️ |
| M7 Comprehensive | 182 | 0 | 10 | ✅ |
| M8 Comprehensive | 299 | 0 | 0 | ✅ |
| M9 Comprehensive | 439 | 7 | 0 | ⚠️ |
| M10 Conformance | 65 | 0 | 0 | ✅ |

---

## Failed Tests Analysis

### 1. Run Name Format Validation (13 failures)

**Affected Tests:**
- `strata-primitives::run_index::tests::test_delete_run`
- `strata-primitives::run_index::tests::integration_tests::*` (3 tests)
- `m3_comprehensive::runindex_lifecycle_tests::cascading_delete::*` (6 tests)
- `m3_comprehensive::run_isolation_comprehensive_tests::*` (2 tests)
- `m3_comprehensive::primitive_api_tests::runindex_api::test_delete_run`

**Error:**
```
InvalidOperation("Invalid run name format: test-run")
InvalidOperation("Invalid run name format: my-run")
InvalidOperation("Invalid run name format: integration-test-run")
```

**Root Cause:** Run name validation is rejecting hyphenated names that tests are using.

**Impact:** High - Affects run deletion and lifecycle tests

---

### 2. Primitive Type Count Mismatch (2 failures)

**Affected Tests:**
- `m6_comprehensive::tier1_architectural_invariants::test_tier1_primitive_type_count`
- `m6_comprehensive::tier1_architectural_invariants::test_tier1_primitive_types_distinct`

**Error:**
```
assertion `left == right` failed: Should have exactly 7 primitives
  left: 6
  right: 7
```

**Root Cause:** Test expects 7 primitives but only 6 are registered.

**Impact:** Medium - Architectural invariant check failing

---

### 3. Event Payload Validation (4 failures)

**Affected Tests:**
- `m9_comprehensive::tier8_run_handle::event_handle_append_returns_sequence`
- `m9_comprehensive::tier8_run_handle::event_handle_read_by_sequence`
- `m9_comprehensive::tier8_run_handle::run_handle_events_isolated`
- `m9_comprehensive::tier8_run_handle::run_handle_multiple_primitive_operations`

**Error:**
```
ValidationError("payload must be a JSON object")
```

**Root Cause:** Event append requires JSON object payloads but tests pass non-object values.

**Impact:** Medium - Tests need update to pass valid payloads

---

### 4. Entity Ref Invariants (3 failures)

**Affected Tests:**
- `m9_comprehensive::tier1_entity_ref_invariants::entity_ref_hashable_for_collections`
- `m9_comprehensive::tier5_seven_invariants::contract_types_are_complete`
- `m9_comprehensive::tier5_seven_invariants::invariant1_every_entity_has_stable_identity`

**Root Cause:** Entity reference type invariants not satisfied.

**Impact:** Medium - Type system constraints

---

### 5. Performance Threshold (1 failure)

**Affected Tests:**
- `m4_regression_tests::m4_red_flags::red_flag_facade_tax_b_a1`

**Error:**
```
RED FLAG: B/A1 ratio 8.9× > 8× threshold.
A1 (primitive): 9393ns, B (full stack): 83846ns
ACTION: Inline facade logic.
```

**Root Cause:** Facade overhead slightly exceeds 8× threshold.

**Impact:** Low - Performance regression flag

---

### 6. JSON to Value Conversion (1 failure)

**Affected Tests:**
- `strata-api::substrate::impl_::tests::test_json_to_value_conversion`

**Error:**
```
assertion `left == right` failed
  left: Object({"Int": Int(42)})
 right: Int(42)
```

**Root Cause:** JSON integer conversion wrapping value in object instead of converting directly.

**Impact:** Low - API conversion edge case

---

## Passing Test Suites (Highlights)

### Fully Passing:
- ✅ **M1-M2 Comprehensive** (320 tests) - Storage & Transactions
- ✅ **M5 Comprehensive** (195 tests) - JSON Store
- ✅ **M7 Comprehensive** (182 tests) - Recovery & Crash Scenarios
- ✅ **M8 Comprehensive** (299 tests) - Vector Store
- ✅ **M10 Conformance** (65 tests) - Portability & Format
- ✅ **Substrate API** (776 tests) - Full substrate test suite
- ✅ **Facade API** (72 tests) - Facade equivalence tests

### RunBundle Tests:
- ✅ Durability run_bundle: 45 tests
- ✅ Primitives export: 9 tests
- ✅ Primitives import: 18 tests
- ✅ Primitives verify: 12 tests
- **Total:** 84 RunBundle tests, all passing

---

## Recommendations

1. **High Priority:** Fix run name validation to accept hyphenated names, or update tests to use valid format
2. **Medium Priority:** Investigate primitive type count discrepancy (6 vs 7)
3. **Medium Priority:** Update event payload tests to use JSON objects
4. **Low Priority:** Review facade performance optimization (8.9× vs 8× threshold)
5. **Low Priority:** Fix JSON-to-Value conversion for integer types

---

## Test Commands

```bash
# Run all unit tests
cargo test --lib

# Run specific milestone
cargo test --test m1_m2_comprehensive
cargo test --test m3_comprehensive
# ... etc

# Run substrate tests
cargo test --test substrate_api_comprehensive

# Run RunBundle tests specifically
cargo test -p strata-primitives "tests::export_tests"
cargo test -p strata-primitives "tests::import_tests"
cargo test -p strata-primitives "tests::verify_tests"
cargo test -p strata-durability run_bundle
```
