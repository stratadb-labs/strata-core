//! M11 Comprehensive Test Suite: Public API & SDK Contract
//!
//! This test suite validates the M11 (Public API & SDK Contract) milestone.
//! Because M11 freezes the public contract that all downstream surfaces depend on,
//! **any bug that escapes testing becomes a permanent liability**.
//!
//! ## Milestone Split
//!
//! M11 is split into two sub-milestones:
//!
//! - **M11a**: Core Contract & API (Value Model, Wire Encoding, Error Model, Facade API, Substrate API)
//! - **M11b**: Consumer Surfaces (CLI, SDK Foundation)
//!
//! **Critical**: M11a must be fully validated before M11b work begins.
//!
//! ## Test Organization
//!
//! ### Part I: M11a Core Contract Tests
//!
//! - `value_model_tests.rs` - Value construction, equality, no coercion, size limits
//! - `wire_encoding_tests.rs` - JSON encoding, wrappers, round-trip
//! - `facade_api_tests.rs` - KV, JSON, Event, Vector, CAS, History operations
//! - `substrate_api_tests.rs` - Explicit run_id, versioned returns, run lifecycle
//! - `desugaring_tests.rs` - Facade→Substrate parity verification
//! - `error_model_tests.rs` - Error codes, wire shape, details
//! - `versioned_tests.rs` - Versioned<T> structure and semantics
//! - `run_semantics_tests.rs` - Run isolation and lifecycle
//! - `transaction_tests.rs` - Isolation, atomicity, conflict handling
//! - `history_tests.rs` - History ordering, pagination, retention
//! - `determinism_tests.rs` - Same ops → same state
//!
//! ### Part II: M11b Consumer Surface Tests
//!
//! - `cli_tests.rs` - Argument parsing, output formatting, commands
//!
//! ## Running Tests
//!
//! ```bash
//! # Run all M11 tests
//! cargo test --test m11_comprehensive
//!
//! # Run M11a core contract tests only
//! cargo test --test m11_comprehensive value_model
//! cargo test --test m11_comprehensive wire_encoding
//! cargo test --test m11_comprehensive facade_api
//! cargo test --test m11_comprehensive substrate_api
//! cargo test --test m11_comprehensive desugaring
//! cargo test --test m11_comprehensive error_model
//!
//! # Run specific test categories
//! cargo test --test m11_comprehensive val_  # Value construction
//! cargo test --test m11_comprehensive flt_  # Float edge cases
//! cargo test --test m11_comprehensive eq_   # Equality tests
//! cargo test --test m11_comprehensive nc_   # No coercion tests
//! cargo test --test m11_comprehensive je_   # JSON encoding
//! cargo test --test m11_comprehensive kv_   # KV operations
//! cargo test --test m11_comprehensive cas_  # CAS operations
//! cargo test --test m11_comprehensive det_  # Determinism
//! ```
//!
//! ## Test ID Conventions
//!
//! - VAL-xxx: Value model construction
//! - FLT-xxx: Float edge cases
//! - EQ-xxx: Value equality
//! - NC-xxx: No coercion
//! - SL-xxx: Size limits
//! - KV-xxx: Key validation
//! - JE-xxx: JSON encoding
//! - WR-xxx: Wire wrappers
//! - ENV-xxx: Request/response envelope
//! - VER-xxx: Version encoding
//! - KV-SET/GET/etc: KV facade operations
//! - JS-xxx: JSON operations
//! - EV-xxx: Event operations
//! - VEC-xxx: Vector operations
//! - CAS-xxx: CAS operations
//! - HIST-xxx: History operations
//! - SUB-xxx: Substrate operations
//! - DS-xxx: Desugaring parity
//! - ERR-xxx: Error model
//! - DET-xxx: Determinism

// =============================================================================
// Part I: M11a Core Contract Tests
// =============================================================================

mod value_model_tests;
mod wire_encoding_tests;
mod facade_api_tests;
mod substrate_api_tests;
mod desugaring_tests;
mod error_model_tests;
mod versioned_tests;
mod run_semantics_tests;
mod transaction_tests;
mod history_tests;
mod determinism_tests;

// =============================================================================
// Part II: M11b Consumer Surface Tests
// =============================================================================

mod cli_tests;

// =============================================================================
// Common test utilities
// =============================================================================

pub mod test_utils;
