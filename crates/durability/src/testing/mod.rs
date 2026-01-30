//! Testing utilities for storage
//!
//! This module provides tools for testing the storage layer:
//!
//! - **Crash Harness**: Framework for systematic crash testing with injection points
//! - **Reference Model**: In-memory model for expected state tracking
//!
//! # Example
//!
//! ```ignore
//! use strata_durability::testing::{CrashConfig, CrashPoint, ReferenceModel};
//!
//! // Track expected state with reference model
//! let mut model = ReferenceModel::new();
//! model.kv_put("run1", "key1", b"value1".to_vec());
//! ```

mod crash_harness;
mod reference_model;

pub use crash_harness::{
    CrashConfig, CrashPoint, CrashTestError, CrashTestResult, CrashType, DataState,
    VerificationResult,
};
pub use reference_model::{Operation, ReferenceModel, StateMismatch};
