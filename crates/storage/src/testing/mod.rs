//! Testing utilities for storage durability and crash recovery
//!
//! This module provides tools for testing the storage layer's resilience:
//!
//! - **Crash Harness**: Framework for systematic crash testing with injection points
//! - **Corruption**: WAL corruption simulation for recovery testing
//! - **Reference Model**: In-memory model for expected state tracking
//!
//! # Example
//!
//! ```ignore
//! use strata_storage::testing::{WalCorruptionTester, CrashConfig, CrashPoint, ReferenceModel};
//!
//! // Test recovery from WAL corruption
//! let tester = WalCorruptionTester::new("path/to/db");
//! tester.truncate_wal_tail(50)?;
//! let verification = tester.verify_recovery()?;
//! assert!(verification.recovered);
//!
//! // Track expected state with reference model
//! let mut model = ReferenceModel::new();
//! model.kv_put("run1", "key1", b"value1".to_vec());
//! ```

mod corruption;
mod crash_harness;
mod reference_model;

pub use corruption::{
    CorruptionResult, GarbageResult, RecoveryVerification, TruncationResult, WalCorruptionTester,
};
pub use crash_harness::{
    CrashConfig, CrashPoint, CrashTestError, CrashTestResult, CrashType, DataState,
    VerificationResult,
};
pub use reference_model::{Operation, ReferenceModel, StateMismatch};
