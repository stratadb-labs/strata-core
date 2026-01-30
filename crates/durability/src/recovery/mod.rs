//! Recovery module
//!
//! - `coordinator`: Recovery coordinator (MANIFEST + snapshot + WAL recovery)
//! - `replayer`: WAL segment replayer (WalReplayer, WalReplayError)

pub mod coordinator;
pub mod replayer;

// Recovery coordinator types (primary API)
pub use coordinator::{
    RecoveryCoordinator, RecoveryError, RecoveryResult,
    RecoverySnapshot, RecoveryPlan,
};
pub use replayer::{WalReplayer, WalReplayError};
