# Phase 1a: Production Panic Audit

Date: 2026-02-04
Status: Complete

## Summary

**11 panic-inducing calls** found in production (non-test) code across 5 crates.
- **9 JUSTIFIED** — true invariant violations that indicate bugs or system failure
- **2 CONVERT** — should return `Result`/`Option` instead of panicking
- **0 STUB** — no unimplemented stubs found
- **0 REVIEW** — no ambiguous cases

**MVP Readiness: PASS** — the two CONVERT items are low-risk edge cases.

---

## Complete Inventory

### strata-concurrency

| File | Line | Code | Classification |
|------|------|------|----------------|
| `manager.rs` | 125 | `.expect("transaction ID overflow: u64::MAX reached")` | JUSTIFIED |
| `manager.rs` | 149 | `.expect("version counter overflow: u64::MAX reached")` | JUSTIFIED |
| `payload.rs` | 37 | `.expect("TransactionPayload serialization should not fail")` | JUSTIFIED |

**Analysis**: All three are genuine invariant violations. Transaction ID and version counter overflow at `u64::MAX` requires ~10^19 operations — indicates memory corruption or logic bug, not a recoverable failure. Payload serialization uses `#[derive(Serialize)]` on a simple struct; failure indicates a bug in `rmp-serde`, not a production error.

### strata-storage

| File | Line | Code | Classification |
|------|------|------|----------------|
| `sharded.rs` | 291 | `.unwrap()` on `fetch_update` with `Some` | JUSTIFIED |
| `sharded.rs` | 369 | `.unwrap()` on `delete_with_version()` | JUSTIFIED |

**Analysis**: Line 291 — `fetch_update` closure returns `Some(v.wrapping_add(1))` for all inputs, so `unwrap` is mathematically guaranteed. The comment documents this. Line 369 — `delete_with_version()` always returns a value (never an error).

### strata-engine

| File | Line | Code | Classification |
|------|------|------|----------------|
| `database/mod.rs` | 263 | `.unwrap()` on `OPEN_DATABASES.lock()` | JUSTIFIED |
| `database/mod.rs` | 335 | `.expect("Failed to spawn WAL flush thread")` | JUSTIFIED |
| `database/mod.rs` | 525 | `.expect("extension type mismatch - this is a bug")` | JUSTIFIED |
| `database/mod.rs` | 776 | `.unwrap_or_else()` in retry exhaustion | **CONVERT** |

**Analysis**: Lines 263, 335, 525 are justified — mutex poisoning means the system is already in an unrecoverable state; thread spawn failure means the OS cannot allocate resources; type mismatch is a programming error. Line 776 should explicitly return the accumulated error rather than conflating "no error captured" with "max retries exceeded."

### strata-intelligence

| File | Line | Code | Classification |
|------|------|------|----------------|
| `fuser.rs` | 233 | `.unwrap()` on `HashMap::remove()` | **CONVERT** |

**Analysis**: Assumes `doc_ref` exists in `hit_data`. While logically it should exist (populated in a prior loop), defensive programming requires handling the missing case gracefully.

### strata-core

| File | Line | Code | Classification |
|------|------|------|----------------|
| `contract/branch_name.rs` | 145 | `.unwrap()` on `chars().next()` after length check | JUSTIFIED |

**Analysis**: Prior check ensures `!name.is_empty()`. Any non-empty string has at least one char.

---

## Recommendations

### Priority 1 — Convert to Result (2 items)

1. **`crates/engine/src/database/mod.rs:776`** — Retry exhaustion should return the last captured error with context, not use `unwrap_or_else` to manufacture a generic conflict error.

2. **`crates/intelligence/src/fuser.rs:233`** — `HashMap::remove(&doc_ref)` should use `.ok_or_else(|| ...)` to propagate a proper error if the key is unexpectedly missing.

### Priority 2 — Documentation (optional)

All 9 JUSTIFIED instances already have either inline comments or self-documenting `expect()` messages. No additional documentation needed.

---

## Methodology

Searched all non-test production code for: `panic!()`, `expect(`, `unwrap()`, `unimplemented!()`, `todo!()`, `unreachable!()`. Excluded all `#[cfg(test)]` modules and `#[test]` functions. Read surrounding context for each hit to classify.
