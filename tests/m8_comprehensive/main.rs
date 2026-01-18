//! M8 Comprehensive Test Suite
//!
//! Tests for Vector Primitive with full durability integration.
//!
//! ## Test Tier Structure
//!
//! - **Tier 1: Storage Invariants (S1-S9)** (sacred, must never break)
//! - **Tier 2: Search Invariants (R1-R10)** (deterministic, read-only)
//! - **Tier 3: Transaction Invariants (T1-T4)** (atomicity, monotonicity)
//! - **Tier 4: Distance Metric Correctness** (mathematical correctness)
//! - **Tier 5: Collection Management** (create, delete, list, get)
//! - **Tier 6: VectorHeap Operations** (insert, delete, get, iteration)
//! - **Tier 7: M6 Integration** (SearchRequest, SearchResponse, RRF)
//! - **Tier 8: WAL Integration** (entry format, write, replay)
//! - **Tier 9: Snapshot & Recovery** (format, next_id, free_slots)
//! - **Tier 10: Cross-Primitive Transactions** (KV + Vector atomicity)
//! - **Tier 11: Crash Scenarios** (crash during operations)
//! - **Tier 12: Determinism Tests** (reproducible results)
//! - **Tier 13: Stress & Scale** (performance, concurrent operations)
//! - **Tier 14: Non-Regression** (M7 durability, M6 search)
//! - **Tier 15: Spec Conformance** (direct spec-to-test mapping)
//!
//! ## Running Tests
//!
//! ```bash
//! # Run all M8 comprehensive tests
//! cargo test --test m8_comprehensive
//!
//! # Run specific tier
//! cargo test --test m8_comprehensive tier1
//!
//! # Run stress tests (slow, opt-in)
//! cargo test --test m8_comprehensive stress -- --ignored
//! ```

mod test_utils;

// Tier 1: Storage Invariants (HIGHEST PRIORITY)
mod tier1_storage_dimension;
mod tier1_storage_metric;
mod tier1_storage_vectorid_stable;
mod tier1_storage_vectorid_never_reused;
mod tier1_storage_heap_kv_consistency;
mod tier1_storage_run_isolation;
mod tier1_storage_btreemap_source;
mod tier1_storage_snapshot_wal_equiv;
mod tier1_storage_reconstructibility;

// Tier 2: Search Invariants
mod tier2_search_dimension_match;
mod tier2_search_score_normalization;
mod tier2_search_deterministic_order;
mod tier2_search_backend_tiebreak;
mod tier2_search_facade_tiebreak;
mod tier2_search_snapshot_consistency;
mod tier2_search_budget_enforcement;
mod tier2_search_single_threaded;
mod tier2_search_no_normalization;
mod tier2_search_readonly;

// Tier 3: Transaction Invariants
mod tier3_tx_atomic_visibility;
mod tier3_tx_conflict_detection;
mod tier3_tx_rollback_safety;
mod tier3_tx_vectorid_monotonicity;

// Tier 4: Distance Metric Correctness
mod tier4_distance_cosine;
mod tier4_distance_euclidean;
mod tier4_distance_dotproduct;
mod tier4_distance_edge_cases;

// Tier 5: Collection Management
mod tier5_collection_create;
mod tier5_collection_delete;
mod tier5_collection_list;
mod tier5_collection_get;
mod tier5_collection_config_persist;

// Tier 6: VectorHeap Operations
mod tier6_heap_insert;
mod tier6_heap_delete;
mod tier6_heap_get;
mod tier6_heap_iteration;
mod tier6_heap_free_slot_reuse;

// Tier 7: M6 Integration
mod tier7_m6_search_request;
mod tier7_m6_search_response;
mod tier7_m6_rrf_fusion;
mod tier7_m6_hybrid_search;
mod tier7_m6_budget_propagation;

// Tier 8: WAL Integration
mod tier8_wal_entry_format;
mod tier8_wal_write;
mod tier8_wal_replay;
mod tier8_wal_replay_determinism;

// Tier 9: Snapshot & Recovery
mod tier9_snapshot_format;
mod tier9_snapshot_next_id;
mod tier9_snapshot_free_slots;
mod tier9_snapshot_recovery;
mod tier9_snapshot_wal_combo;

// Tier 10: Cross-Primitive Transactions
mod tier10_cross_kv_vector;
mod tier10_cross_json_vector;
mod tier10_cross_all_primitives;
mod tier10_cross_crash_recovery;

// Tier 11: Crash Scenarios
mod tier11_crash_mid_upsert;
mod tier11_crash_mid_delete;
mod tier11_crash_collection_create;
mod tier11_crash_collection_delete;

// Tier 12: Determinism Tests
mod tier12_determinism_search;
mod tier12_determinism_recovery;
mod tier12_determinism_vectorid;
mod tier12_determinism_operations;

// Tier 13: Stress & Scale (use #[ignore] for slow tests)
mod tier13_stress_large_collections;
mod tier13_stress_many_collections;
mod tier13_stress_rapid_operations;
mod tier13_stress_high_dimension;
mod tier13_stress_snapshot_wal_size;

// Tier 14: Non-Regression
mod tier14_regression_known_bugs;
mod tier14_regression_edge_cases;

// Tier 15: Spec Conformance
mod tier15_spec_conformance;
