//! Recovery module for branch lifecycle and primitive recovery
//!
//! This module contains:
//! - `replay`: Branch lifecycle management and deterministic replay
//! - `participant`: Recovery participant registry for primitives with runtime state

mod participant;
mod replay;

pub use participant::{
    recover_all_participants, register_recovery_participant, RecoveryFn, RecoveryParticipant,
};
pub use replay::{
    diff_views, DiffEntry, ReadOnlyView, ReplayError, BranchDiff, BranchError,
    BranchIndex as ReplayBranchIndex,
};

#[cfg(test)]
pub use participant::{clear_recovery_registry, recovery_registry_count};
