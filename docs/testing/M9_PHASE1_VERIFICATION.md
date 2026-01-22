# M9 Phase 1 Verification Plan

## Overview

This document outlines the verification strategy for M9 Phase 1 (Epic 60: Core Types).
The test suite is designed to be extended as Phase 2+ are implemented.

## What Was Implemented

M9 Phase 1 introduces the **contract module** (`crates/core/src/contract/`) containing:

| Type | File | Purpose | Invariant |
|------|------|---------|-----------|
| `EntityRef` | entity_ref.rs | Universal entity addressing | 1: Addressable |
| `Versioned<T>` | versioned.rs | Generic versioned wrapper | 2: Versioned |
| `Version` | version.rs | Version identifier enum | 2: Versioned |
| `Timestamp` | timestamp.rs | Microsecond precision | 2: Versioned |
| `PrimitiveType` | primitive_type.rs | Seven primitive enum | 6: Introspectable |
| `RunName` | run_name.rs | Semantic run identity | 5: Run-scoped |

### Key Changes

1. **Timestamp**: Changed from `type Timestamp = i64` (seconds) to `struct Timestamp(u64)` (microseconds)
2. **Version**: New enum with `TxnId(u64)`, `Sequence(u64)`, `Counter(u64)` variants
3. **Versioned<T>**: Replaces `VersionedValue` struct with generic wrapper
4. **EntityRef**: Unifies with `DocRef`, all variants now require explicit `run_id`
5. **PrimitiveType**: Unifies with `PrimitiveKind`
6. **RunName**: New type for semantic run identification

---

## Verification Strategy

### Level 1: Unit Tests (Already Exist)

The contract module has **149 unit tests** covering:
- All constructors and accessors
- Edge cases (empty, max length, invalid chars)
- Serialization/deserialization
- Trait implementations (Hash, Eq, Clone, Copy)
- Ordering and comparison semantics
- Display formatting

**Status**: ✅ All passing

### Level 2: Invariant Tests (NEW)

Test that each contract type correctly expresses its invariant:

| Invariant | Test Focus |
|-----------|-----------|
| 1: Addressable | EntityRef uniquely identifies any entity |
| 2: Versioned | Version types support comparison, incrementing |
| 5: Run-scoped | EntityRef.run_id() always returns valid RunId |
| 6: Introspectable | PrimitiveType covers all 7 primitives |

### Level 3: Cross-Type Integration Tests (NEW)

Test interactions between contract types:
- `Versioned<T>` with different `Version` variants
- `EntityRef` to `PrimitiveType` mapping consistency
- `Timestamp` integration with `Versioned<T>`

### Level 4: Backwards Compatibility Tests (NEW)

Test that existing code continues to work:
- `DocRef` alias works identically to `EntityRef`
- `PrimitiveKind` alias works identically to `PrimitiveType`
- `VersionedValue` alias works identically to `Versioned<Value>`

### Level 5: Migration Validation Tests (NEW)

Test that migrated code produces correct results:
- Timestamp: microseconds vs seconds conversion
- Version: comparison semantics across variants
- EntityRef: run_id extraction from all variants

---

## Test Suite Structure

```
tests/m9_comprehensive/
├── main.rs                          # Test harness
├── test_utils.rs                    # Shared utilities
├── tier1_entity_ref_invariants.rs   # EntityRef tests
├── tier1_version_invariants.rs      # Version tests
├── tier1_timestamp_invariants.rs    # Timestamp tests
├── tier1_versioned_invariants.rs    # Versioned<T> tests
├── tier1_primitive_type_invariants.rs # PrimitiveType tests
├── tier1_run_name_invariants.rs     # RunName tests
├── tier2_cross_type_integration.rs  # Cross-type tests
├── tier3_backwards_compatibility.rs # Alias compatibility tests
├── tier4_migration_validation.rs    # Migration correctness tests
└── tier5_seven_invariants.rs        # Full invariant conformance
```

---

## Test Categories

### Tier 1: Type Invariant Tests

#### EntityRef (Invariant 1: Addressable)

```rust
// Every entity has a stable identity
#[test] fn entity_ref_uniquely_identifies_kv_entry()
#[test] fn entity_ref_uniquely_identifies_event()
#[test] fn entity_ref_uniquely_identifies_state()
#[test] fn entity_ref_uniquely_identifies_trace()
#[test] fn entity_ref_uniquely_identifies_run()
#[test] fn entity_ref_uniquely_identifies_json_doc()
#[test] fn entity_ref_uniquely_identifies_vector()

// Same entity = same reference
#[test] fn same_entity_produces_equal_refs()
#[test] fn different_entities_produce_different_refs()

// EntityRef is hashable (for collections)
#[test] fn entity_ref_hashable_for_collections()
#[test] fn entity_ref_usable_as_map_key()
```

#### Version (Invariant 2: Versioned)

```rust
// Versions are comparable within same type
#[test] fn version_txn_id_comparable()
#[test] fn version_sequence_comparable()
#[test] fn version_counter_comparable()

// Versions increment correctly
#[test] fn version_increment_produces_higher_version()
#[test] fn version_saturating_increment_handles_overflow()

// Cross-variant comparison semantics
#[test] fn version_cross_variant_ordering_defined()
```

#### Timestamp (Invariant 2: Versioned - Temporal)

```rust
// Timestamps are ordered
#[test] fn timestamp_ordering_consistent()
#[test] fn timestamp_now_increases_over_time()

// Microsecond precision
#[test] fn timestamp_preserves_microsecond_precision()
#[test] fn timestamp_from_millis_correct()
#[test] fn timestamp_from_secs_correct()

// Duration operations
#[test] fn timestamp_duration_since_correct()
#[test] fn timestamp_add_subtract_duration()
```

#### Versioned<T> (Invariant 2: Versioned)

```rust
// Every read returns version info
#[test] fn versioned_always_has_version()
#[test] fn versioned_always_has_timestamp()

// Map preserves version info
#[test] fn versioned_map_preserves_version()
#[test] fn versioned_map_preserves_timestamp()

// Age calculations
#[test] fn versioned_age_calculated_correctly()
#[test] fn versioned_is_older_than_correct()
```

#### PrimitiveType (Invariant 6: Introspectable)

```rust
// All 7 primitives enumerated
#[test] fn primitive_type_has_exactly_seven_variants()
#[test] fn primitive_type_all_returns_all_variants()

// Each primitive has name and id
#[test] fn primitive_type_name_returns_human_readable()
#[test] fn primitive_type_id_returns_short_form()
#[test] fn primitive_type_from_id_roundtrips()

// CRUD vs append-only classification
#[test] fn primitive_type_crud_classification_correct()
#[test] fn primitive_type_append_only_classification_correct()
```

#### RunName (Invariant 5: Run-scoped)

```rust
// Validation rules enforced
#[test] fn run_name_rejects_empty()
#[test] fn run_name_rejects_too_long()
#[test] fn run_name_rejects_invalid_chars()
#[test] fn run_name_rejects_invalid_start()

// Valid names accepted
#[test] fn run_name_accepts_alphanumeric()
#[test] fn run_name_accepts_underscores()
#[test] fn run_name_accepts_dots()
#[test] fn run_name_accepts_hyphens()
```

### Tier 2: Cross-Type Integration

```rust
// EntityRef + PrimitiveType consistency
#[test] fn entity_ref_primitive_type_matches_variant()
#[test] fn all_primitive_types_have_entity_ref_variant()

// Versioned + Version integration
#[test] fn versioned_with_txn_id_version()
#[test] fn versioned_with_sequence_version()
#[test] fn versioned_with_counter_version()

// Versioned + Timestamp integration
#[test] fn versioned_timestamp_is_accurate()
#[test] fn versioned_new_uses_current_timestamp()

// EntityRef + RunId integration
#[test] fn all_entity_ref_variants_have_run_id()
#[test] fn entity_ref_run_id_extraction_consistent()
```

### Tier 3: Backwards Compatibility

```rust
// DocRef alias
#[test] fn doc_ref_is_entity_ref_alias()
#[test] fn doc_ref_variant_construction_works()
#[test] fn doc_ref_usable_in_existing_patterns()

// PrimitiveKind alias
#[test] fn primitive_kind_is_primitive_type_alias()
#[test] fn primitive_kind_deprecation_warning()

// VersionedValue alias
#[test] fn versioned_value_is_versioned_value_alias()
#[test] fn versioned_value_construction_works()
```

### Tier 4: Migration Validation

```rust
// Timestamp migration (seconds → microseconds)
#[test] fn timestamp_from_old_seconds_format()
#[test] fn timestamp_to_old_seconds_format()

// Version migration (u64 → Version enum)
#[test] fn version_from_raw_u64()
#[test] fn version_as_u64_for_comparison()

// EntityRef migration (embedded run_id → explicit)
#[test] fn entity_ref_run_id_always_accessible()
```

### Tier 5: Seven Invariants Conformance

Full end-to-end tests that each type correctly expresses its invariant:

```rust
// Invariant 1: Everything is Addressable
#[test] fn invariant1_every_entity_has_stable_identity()
#[test] fn invariant1_identity_survives_serialization()
#[test] fn invariant1_identity_usable_for_retrieval()

// Invariant 2: Everything is Versioned
#[test] fn invariant2_every_read_has_version()
#[test] fn invariant2_versions_comparable()
#[test] fn invariant2_timestamp_always_present()

// Invariant 5: Everything is Run-Scoped
#[test] fn invariant5_every_entity_has_run_id()
#[test] fn invariant5_run_name_validates_semantic_identity()

// Invariant 6: Everything is Introspectable
#[test] fn invariant6_all_primitives_enumerated()
#[test] fn invariant6_primitive_type_discoverable()
```

---

## Acceptance Criteria

### Must Pass

- [ ] All 149 existing unit tests pass
- [ ] All new invariant tests pass
- [ ] All backwards compatibility tests pass
- [ ] All migration validation tests pass
- [ ] `cargo test -p in-mem-core` passes
- [ ] `cargo clippy -p in-mem-core -- -D warnings` passes

### Coverage Targets

- Contract module: >95% line coverage
- All public methods tested
- All error paths tested
- All edge cases documented

---

## Extension Points for Phase 2+

This test suite is designed to grow:

### Phase 2: Versioned Returns (Epic 61)
- Tests for KV returning `Versioned<Value>`
- Tests for EventLog returning `Versioned<Event>`
- Tests for StateCell returning `Versioned<T>`

### Phase 3: Transaction Unification (Epic 62)
- Tests for Version in transaction context
- Tests for EntityRef in conflict detection

### Phase 4: Error Standardization (Epic 63)
- Tests for EntityRef in error messages
- Tests for error categorization

### Phase 5: Conformance Testing (Epic 64)
- Tests for all 7 primitives × 7 invariants
- Property-based testing for contract types
