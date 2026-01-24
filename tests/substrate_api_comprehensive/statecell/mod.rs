//! StateCell Test Suite
//!
//! Comprehensive tests for the StateCell substrate API.
//!
//! ## Modules
//!
//! - `basic_ops`: Basic operations (get, set, delete, exists)
//! - `cas_ops`: Compare-and-swap operations
//! - `transitions`: Atomic state transitions (state_transition, state_transition_or_init)
//! - `durability`: Durability modes and crash recovery
//! - `concurrency`: Multi-threaded safety and contention
//! - `edge_cases`: Validation, constraints, cell names
//! - `invariants`: Tests for all 7 invariants from PRIMITIVE_CONTRACT.md

pub mod basic_ops;
pub mod cas_ops;
pub mod concurrency;
pub mod durability;
pub mod edge_cases;
pub mod invariants;
pub mod transitions;
