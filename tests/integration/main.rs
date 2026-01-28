//! Integration Tests
//!
//! Comprehensive cross-layer tests organized by test dimensions:
//! - Storage mode: persistent vs ephemeral
//! - Durability: none, batched, strict
//! - Primitives: single vs cross-primitive
//! - Scale: 1k, 10k, 100k records
//! - Branching: run isolation and forking

#[path = "../common/mod.rs"]
mod common;

mod modes;
mod primitives;
mod scale;
mod branching;
