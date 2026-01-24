//! M9 Comprehensive Test Suite
//!
//! This test suite verifies the M9 API Stabilization implementation.
//! It covers all 5 Epics and their stories.
//!
//! ## Test Tiers
//!
//! - **Tier 1**: Type invariant tests (one file per contract type) - Epic 60
//! - **Tier 2**: Cross-type integration tests - Epic 60
//! - **Tier 3**: Backwards compatibility tests - Epic 60
//! - **Tier 4**: Migration validation tests - Epic 60
//! - **Tier 5**: Seven invariants conformance tests - Epic 60
//! - **Tier 6**: Primitive conformance tests - Epic 64
//! - **Tier 7**: StrataError standardization tests - Epic 63
//! - **Tier 8**: RunHandle pattern tests - Epic 62 Story #478
//! - **Tier 9**: Cross-primitive transaction tests - Epic 62 Story #487
//! - **Tier 10**: M9 Architecture compliance tests - M9_ARCHITECTURE.md
//!
//! ## Running Tests
//!
//! ```bash
//! cargo test --test m9_comprehensive
//! ```

// Test modules
mod test_utils;

// Tier 1: Type Invariant Tests (Epic 60: Core Types)
mod tier1_entity_ref_invariants;
mod tier1_primitive_type_invariants;
mod tier1_run_name_invariants;
mod tier1_timestamp_invariants;
mod tier1_version_invariants;
mod tier1_versioned_invariants;

// Tier 2: Cross-Type Integration Tests (Epic 60)
mod tier2_cross_type_integration;

// Tier 3: Backwards Compatibility Tests (Epic 60)
mod tier3_backwards_compatibility;

// Tier 4: Migration Validation Tests (Epic 60)
mod tier4_migration_validation;

// Tier 5: Seven Invariants Conformance Tests (Epic 60)
mod tier5_seven_invariants;

// Tier 6: Primitive Conformance Tests (Epic 64: Conformance Testing)
mod tier6_kv_conformance;
mod tier6_event_conformance;
mod tier6_state_conformance;

// Tier 7: StrataError Tests (Epic 63: Error Standardization)
mod tier7_strata_error;

// Tier 8: RunHandle Pattern Tests (Epic 62: Story #478)
mod tier8_run_handle;

// Tier 9: Cross-Primitive Transaction Tests (Epic 62: Story #487)
mod tier9_cross_primitive;

// Tier 10: M9 Architecture Compliance Tests (M9_ARCHITECTURE.md)
mod tier10_architecture_compliance;
