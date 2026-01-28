//! Engine Crate Integration Tests
//!
//! Tests for Database, 6 Primitives, and cross-cutting concerns.

#[path = "../common/mod.rs"]
mod common;

mod database;
mod primitives;

mod acid_properties;
mod adversarial;
mod cross_primitive;
mod run_isolation;
mod stress;
