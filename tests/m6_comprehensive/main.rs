//! M6 Comprehensive Test Suite
//!
//! Tests for the Retrieval Surfaces semantic guarantees.
//!
//! ## Test Tier Structure
//!
//! - **Tier 1: Architectural Rule Invariants** (sacred, must never break)
//! - **Tier 2: Search Correctness** (determinism, exhaustiveness, filters)
//! - **Tier 3: Budget Semantics** (truncation, ordering, isolation)
//! - **Tier 4: Scoring Accuracy** (BM25-lite correctness)
//! - **Tier 5: Fusion Correctness** (RRF, determinism, tiebreak)
//! - **Tier 6: Cross-Primitive Identity** (DocRef policy, deduplication policy)
//! - **Tier 7: Index Consistency** (index matches scan)
//! - **Tier 8: Cross-Primitive Search** (hybrid orchestration)
//! - **Tier 9: Result Explainability** (provenance, score explanation)
//! - **Tier 10: Property-Based/Fuzzing** (catch edge cases)
//! - **Tier 11: Stress/Scale** (correctness under load)
//! - **Tier 12: Non-Regression** (M4/M5 targets maintained)
//! - **Tier 13: Spec Conformance** (spec → test)
//!
//! ## Running Tests
//!
//! ```bash
//! # Run all M6 comprehensive tests
//! cargo test --test m6_comprehensive
//!
//! # Run specific tier
//! cargo test --test m6_comprehensive tier1
//!
//! # Run stress tests (slow, opt-in)
//! cargo test --test m6_comprehensive stress -- --ignored
//! ```

mod test_utils;

// Tier 1: Architectural Rule Invariants
mod tier1_architectural_invariants;

// Tier 2: Search Correctness
mod tier2_search_correctness;

// Tier 3: Budget Semantics
mod tier3_budget_semantics;

// Tier 4: Scoring Accuracy
mod tier4_scoring;

// Tier 5: Fusion Correctness
mod tier5_fusion;

// Tier 6: Cross-Primitive Identity
mod tier6_identity;

// Tier 7: Index Consistency
mod tier7_indexing;

// Tier 8: Cross-Primitive Search
mod tier8_hybrid;

// Tier 9: Result Explainability
mod tier9_explainability;

// Tier 10: Stress & Scale (use #[ignore])
mod tier10_stress;
