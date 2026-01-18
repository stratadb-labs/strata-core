//! Cross-Milestone Comprehensive Test Suite
//!
//! This test suite validates the 23 issues identified in M1_M8_COMPREHENSIVE_REVIEW.md
//! and ensures cross-cutting concerns are properly tested.
//!
//! ## Test Tier Structure
//!
//! - **Tier 1: Critical Issue Validation** (ISSUE-001 through ISSUE-005)
//!   Tests that verify critical issues are addressed or fail if still present.
//!
//! - **Tier 2: High Priority Issue Validation** (ISSUE-006 through ISSUE-013)
//!   Tests for high-priority issues.
//!
//! - **Tier 3: Medium Priority Issue Validation** (ISSUE-014 through ISSUE-020)
//!   Tests for medium-priority issues.
//!
//! - **Tier 4: Cross-Primitive Atomicity** (All 7 primitives in single transaction)
//!   Comprehensive atomicity tests.
//!
//! - **Tier 5: Search Integration** (Searchable trait compliance)
//!   Verifies all primitives implement Searchable correctly.
//!
//! - **Tier 6: Durability Mode Consistency** (InMemory vs Buffered vs Strict)
//!   Verifies behavior is consistent across durability modes.
//!
//! - **Tier 7: Concurrent Operations** (Collection mutations during search)
//!   Tests concurrent access patterns.
//!
//! - **Tier 8: Mega-Scale Tests** (1M+ vectors, large documents)
//!   Scale testing beyond normal limits.
//!
//! ## Running Tests
//!
//! ```bash
//! # Run all cross-milestone comprehensive tests
//! cargo test --test cross_milestone_comprehensive
//!
//! # Run only critical issue tests
//! cargo test --test cross_milestone_comprehensive critical_issue
//!
//! # Run stress/scale tests (slow, opt-in)
//! cargo test --test cross_milestone_comprehensive mega_scale -- --ignored
//! ```

mod test_utils;

// Tier 1: Critical Issue Validation (ISSUE-001 through ISSUE-005)
mod critical_issue_001_vector_searchable;
mod critical_issue_002_replay_api_exposure;
mod critical_issue_003_vector_primitive_storage_ext;
mod critical_issue_004_snapshot_header_size;
mod critical_issue_005_json_limit_validation;

// Tier 2: High Priority Issue Validation (ISSUE-006 through ISSUE-013)
mod high_issue_006_wal_entry_0x23;
mod high_issue_007_vector_storage_dtype;
mod high_issue_008_buffered_thread_startup;
mod high_issue_009_readonly_view;
mod high_issue_010_facade_tax;
mod high_issue_011_lock_sharding;
mod high_issue_012_snapshot_traits;
mod high_issue_013_hybrid_search_vector;

// Tier 3: Medium Priority Issue Validation (ISSUE-014 through ISSUE-020)
mod medium_issue_014_rfc6902_operations;
mod medium_issue_015_json_path_validation;
mod medium_issue_016_vector_budget;
mod medium_issue_017_collection_config_recovery;
mod medium_issue_018_search_overfetch;
mod medium_issue_019_durability_handlers;
mod medium_issue_020_buffered_defaults;

// Tier 4: Cross-Primitive Atomicity
mod cross_primitive_all_seven;
mod cross_primitive_rollback_all;
mod cross_primitive_isolation;

// Tier 5: Search Integration
mod search_all_primitives;
mod search_hybrid_orchestration;
mod search_budget_enforcement;

// Tier 6: Durability Mode Consistency
mod durability_mode_equivalence;
mod durability_mode_recovery;
mod durability_buffered_flush;

// Tier 7: Concurrent Operations
mod concurrent_collection_mutations;
mod concurrent_search_during_write;
mod concurrent_multi_primitive;

// Tier 8: Mega-Scale Tests (use #[ignore] for slow tests)
mod mega_scale_vectors;
mod mega_scale_json;
mod mega_scale_events;
