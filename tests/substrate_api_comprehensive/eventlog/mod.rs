//! EventLog Test Suite
//!
//! Comprehensive tests for the EventLog substrate API.
//!
//! ## Modules
//!
//! - `basic_ops`: Basic operations (append, get, range, len, latest_sequence)
//! - `streams`: Multi-stream operations and stream isolation
//! - `edge_cases`: Validation, constraints, empty streams, large payloads
//! - `durability`: Durability modes and crash recovery
//! - `concurrency`: Multi-threaded safety and ordering
//! - `recovery_invariants`: Recovery guarantees
//! - `immutability`: Append-only verification, no update/delete
//! - `invariants`: Tests for all 7 invariants from PRIMITIVE_CONTRACT.md

pub mod basic_ops;
pub mod concurrency;
pub mod durability;
pub mod edge_cases;
pub mod immutability;
pub mod invariants;
pub mod recovery_invariants;
pub mod streams;
