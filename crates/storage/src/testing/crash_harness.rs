//! Crash harness framework for testing storage durability
//!
//! Provides types and utilities for systematic crash testing of the storage layer.
//!
//! # Crash Points
//!
//! The framework defines specific crash injection points:
//! - WAL operations (before/after write, before/after fsync)
//! - Segment rotation
//! - Snapshot creation
//! - MANIFEST updates
//! - Compaction
//!
//! # Example
//!
//! ```ignore
//! use strata_storage::testing::{CrashConfig, CrashPoint, CrashType};
//!
//! let config = CrashConfig::default();
//! // Use with storage layer components for crash testing
//! ```

use std::time::Duration;

/// Configuration for crash injection
#[derive(Debug, Clone)]
pub struct CrashConfig {
    /// Probability of crash at each injection point (0.0 - 1.0)
    pub crash_probability: f64,
    /// Types of crashes to simulate
    pub crash_types: Vec<CrashType>,
    /// Maximum operations before forced crash
    pub max_operations: usize,
    /// Timeout for operations
    pub timeout: Duration,
}

impl Default for CrashConfig {
    fn default() -> Self {
        CrashConfig {
            crash_probability: 0.1,
            crash_types: vec![CrashType::ProcessKill, CrashType::ProcessAbort],
            max_operations: 1000,
            timeout: Duration::from_secs(30),
        }
    }
}

impl CrashConfig {
    /// Create config for deterministic crash at specific point
    pub fn deterministic(probability: f64) -> Self {
        CrashConfig {
            crash_probability: probability,
            ..Default::default()
        }
    }

    /// Create config that always crashes
    pub fn always_crash() -> Self {
        CrashConfig {
            crash_probability: 1.0,
            ..Default::default()
        }
    }

    /// Create config that never crashes (for baseline testing)
    pub fn never_crash() -> Self {
        CrashConfig {
            crash_probability: 0.0,
            ..Default::default()
        }
    }

    /// Set crash probability
    pub fn with_probability(mut self, probability: f64) -> Self {
        self.crash_probability = probability.clamp(0.0, 1.0);
        self
    }

    /// Set crash types
    pub fn with_crash_types(mut self, types: Vec<CrashType>) -> Self {
        self.crash_types = types;
        self
    }

    /// Set maximum operations
    pub fn with_max_operations(mut self, max: usize) -> Self {
        self.max_operations = max;
        self
    }
}

/// Types of crash simulation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CrashType {
    /// SIGKILL - immediate process termination
    ProcessKill,
    /// SIGABRT - abort signal
    ProcessAbort,
    /// SIGSEGV - segmentation fault simulation
    SegFault,
    /// Power loss simulation (kill without cleanup)
    PowerLoss,
}

impl CrashType {
    /// Get all crash types
    pub fn all() -> Vec<CrashType> {
        vec![
            CrashType::ProcessKill,
            CrashType::ProcessAbort,
            CrashType::SegFault,
            CrashType::PowerLoss,
        ]
    }

    /// Get description of crash type
    pub fn description(&self) -> &'static str {
        match self {
            CrashType::ProcessKill => "SIGKILL - immediate process termination",
            CrashType::ProcessAbort => "SIGABRT - abort signal",
            CrashType::SegFault => "SIGSEGV - segmentation fault",
            CrashType::PowerLoss => "Power loss - sudden termination without cleanup",
        }
    }
}

/// Crash injection points in the storage layer
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CrashPoint {
    /// Before writing WAL record to buffer
    BeforeWalWrite,
    /// After writing WAL record, before fsync
    AfterWalWriteBeforeFsync,
    /// After fsync, before returning
    AfterFsync,
    /// During WAL segment rotation
    DuringSegmentRotation,
    /// During snapshot creation (before atomic rename)
    DuringSnapshotBeforeRename,
    /// During snapshot creation (after atomic rename)
    DuringSnapshotAfterRename,
    /// During MANIFEST update
    DuringManifestUpdate,
    /// During compaction
    DuringCompaction,
}

impl CrashPoint {
    /// Get all crash points
    pub fn all() -> Vec<CrashPoint> {
        vec![
            CrashPoint::BeforeWalWrite,
            CrashPoint::AfterWalWriteBeforeFsync,
            CrashPoint::AfterFsync,
            CrashPoint::DuringSegmentRotation,
            CrashPoint::DuringSnapshotBeforeRename,
            CrashPoint::DuringSnapshotAfterRename,
            CrashPoint::DuringManifestUpdate,
            CrashPoint::DuringCompaction,
        ]
    }

    /// Get description of crash point
    pub fn description(&self) -> &'static str {
        match self {
            CrashPoint::BeforeWalWrite => "Before WAL record write",
            CrashPoint::AfterWalWriteBeforeFsync => "After WAL write, before fsync",
            CrashPoint::AfterFsync => "After fsync completed",
            CrashPoint::DuringSegmentRotation => "During WAL segment rotation",
            CrashPoint::DuringSnapshotBeforeRename => "During snapshot, before atomic rename",
            CrashPoint::DuringSnapshotAfterRename => "During snapshot, after atomic rename",
            CrashPoint::DuringManifestUpdate => "During MANIFEST update",
            CrashPoint::DuringCompaction => "During compaction",
        }
    }

    /// Expected data state after recovery at this crash point
    pub fn expected_data_state(&self) -> DataState {
        match self {
            CrashPoint::BeforeWalWrite => DataState::NotPresent,
            CrashPoint::AfterWalWriteBeforeFsync => DataState::MayBePresent,
            CrashPoint::AfterFsync => DataState::Present,
            CrashPoint::DuringSegmentRotation => DataState::Present,
            CrashPoint::DuringSnapshotBeforeRename => DataState::Present,
            CrashPoint::DuringSnapshotAfterRename => DataState::Present,
            CrashPoint::DuringManifestUpdate => DataState::Present,
            CrashPoint::DuringCompaction => DataState::Present,
        }
    }
}

/// Expected data state after recovery
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataState {
    /// Data should not be present (write not completed)
    NotPresent,
    /// Data may or may not be present (depends on OS/filesystem)
    MayBePresent,
    /// Data should be present (write completed and synced)
    Present,
}

/// Result of a crash test
#[derive(Debug)]
pub struct CrashTestResult {
    /// Whether the test scenario succeeded before crash
    pub scenario_succeeded: bool,
    /// Whether recovery succeeded after crash
    pub recovery_succeeded: bool,
    /// Crash point where crash was injected
    pub crash_point: Option<CrashPoint>,
    /// Number of operations completed before crash
    pub operations_completed: usize,
    /// Test duration
    pub duration: Duration,
    /// Verification result after recovery
    pub verification: VerificationResult,
}

/// Result of verifying state after recovery
#[derive(Debug)]
pub struct VerificationResult {
    /// Whether the state is valid
    pub is_valid: bool,
    /// Error message if any
    pub error: Option<String>,
    /// State mismatches found
    pub mismatches: Vec<StateMismatch>,
}

impl VerificationResult {
    /// Create successful verification
    pub fn success() -> Self {
        VerificationResult {
            is_valid: true,
            error: None,
            mismatches: vec![],
        }
    }

    /// Create failed verification with error
    pub fn error(msg: impl Into<String>) -> Self {
        VerificationResult {
            is_valid: false,
            error: Some(msg.into()),
            mismatches: vec![],
        }
    }

    /// Create failed verification with mismatches
    pub fn mismatches(mismatches: Vec<StateMismatch>) -> Self {
        VerificationResult {
            is_valid: mismatches.is_empty(),
            error: None,
            mismatches,
        }
    }
}

/// State mismatch found during verification
#[derive(Debug, Clone)]
pub struct StateMismatch {
    /// Entity identifier (e.g., "kv:run:key")
    pub entity: String,
    /// Expected value
    pub expected: String,
    /// Actual value
    pub actual: String,
}

/// Crash test errors
#[derive(Debug, thiserror::Error)]
pub enum CrashTestError {
    /// Simulated crash at injection point
    #[error("Simulated crash at {0:?}")]
    SimulatedCrash(CrashPoint),

    /// IO error during test
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Verification error
    #[error("Verification error: {0}")]
    Verification(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crash_config_default() {
        let config = CrashConfig::default();
        assert_eq!(config.crash_probability, 0.1);
        assert_eq!(config.max_operations, 1000);
    }

    #[test]
    fn test_crash_config_always_crash() {
        let config = CrashConfig::always_crash();
        assert_eq!(config.crash_probability, 1.0);
    }

    #[test]
    fn test_crash_config_never_crash() {
        let config = CrashConfig::never_crash();
        assert_eq!(config.crash_probability, 0.0);
    }

    #[test]
    fn test_crash_config_builder() {
        let config = CrashConfig::default()
            .with_probability(0.5)
            .with_max_operations(100);

        assert_eq!(config.crash_probability, 0.5);
        assert_eq!(config.max_operations, 100);
    }

    #[test]
    fn test_crash_types() {
        let types = CrashType::all();
        assert_eq!(types.len(), 4);
    }

    #[test]
    fn test_crash_points() {
        let points = CrashPoint::all();
        assert_eq!(points.len(), 8);
    }

    #[test]
    fn test_crash_point_data_state() {
        assert_eq!(
            CrashPoint::BeforeWalWrite.expected_data_state(),
            DataState::NotPresent
        );
        assert_eq!(
            CrashPoint::AfterFsync.expected_data_state(),
            DataState::Present
        );
        assert_eq!(
            CrashPoint::AfterWalWriteBeforeFsync.expected_data_state(),
            DataState::MayBePresent
        );
    }

    #[test]
    fn test_verification_result_success() {
        let result = VerificationResult::success();
        assert!(result.is_valid);
        assert!(result.error.is_none());
        assert!(result.mismatches.is_empty());
    }

    #[test]
    fn test_verification_result_error() {
        let result = VerificationResult::error("test error");
        assert!(!result.is_valid);
        assert_eq!(result.error, Some("test error".to_string()));
    }
}
