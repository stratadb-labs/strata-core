//! Recovery module
//!
//! This module contains both the legacy WAL replay and the new recovery coordinator:
//!
//! - `legacy`: Original WAL replay logic (replay_wal, ReplayStats, etc.)
//! - `coordinator`: Recovery coordinator (MANIFEST + snapshot + WAL recovery)
//! - `replayer`: WAL segment replayer (WalReplayer, WalReplayError)

pub mod legacy;
pub mod coordinator;
pub mod replayer;

// Backward-compatible re-exports (unchanged API)
pub use legacy::{
    replay_wal, replay_wal_with_options, validate_transactions,
    ReplayOptions, ReplayProgress, ReplayStats, ValidationResult, ValidationWarning,
};

// New recovery types
pub use coordinator::{
    RecoveryCoordinator, RecoveryError, RecoveryResult,
    RecoverySnapshot, RecoveryPlan,
};
pub use replayer::{WalReplayer, WalReplayError};
// Note: replayer::ReplayStats NOT re-exported to avoid conflict with legacy ReplayStats
