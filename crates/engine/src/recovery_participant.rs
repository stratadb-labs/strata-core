//! Recovery Participant Registry
//!
//! Defines the interface for primitives that need to participate in
//! Database recovery. This is for primitives with runtime state that
//! lives outside ShardedStore (e.g., VectorStore's in-memory backends).
//!
//! ## Design Philosophy
//!
//! Recovery is a cold-path, correctness-critical system. The design is:
//! - Explicit, boring, and obvious
//! - Minimal trait surface
//! - No over-generalization
//!
//! ## How It Works
//!
//! 1. Database opens and performs KV recovery via RecoveryCoordinator
//! 2. Database calls `recover_all_participants()` which invokes each registered participant
//! 3. Each participant replays its WAL entries into its runtime state
//! 4. Database is ready for use
//!
//! ## Registration
//!
//! Primitives register their recovery functions at startup:
//!
//! ```ignore
//! use in_mem_engine::{register_recovery_participant, RecoveryParticipant};
//!
//! // Called once at initialization
//! register_recovery_participant(RecoveryParticipant::new("vector", recover_vector_state));
//! ```

use in_mem_core::error::Result;
use parking_lot::RwLock;
use tracing::info;

/// Function signature for primitive recovery
///
/// Takes a reference to the Database and performs recovery for a specific
/// primitive's runtime state. The function should:
/// 1. Access the WAL via `db.wal()`
/// 2. Replay relevant entries into the primitive's extension state
/// 3. Return Ok(()) on success or an error on failure
///
/// Recovery functions are stateless - they use the Database's extension
/// mechanism to access shared state.
pub type RecoveryFn = fn(&super::Database) -> Result<()>;

/// Registry entry for a recovery participant
#[derive(Clone)]
pub struct RecoveryParticipant {
    /// Human-readable name for logging
    pub name: &'static str,
    /// Recovery function to call
    pub recover: RecoveryFn,
}

impl RecoveryParticipant {
    /// Create a new recovery participant
    pub const fn new(name: &'static str, recover: RecoveryFn) -> Self {
        Self { name, recover }
    }
}

impl std::fmt::Debug for RecoveryParticipant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecoveryParticipant")
            .field("name", &self.name)
            .finish()
    }
}

/// Global registry of recovery participants
///
/// Uses lazy initialization with a RwLock for thread-safe access.
static RECOVERY_REGISTRY: once_cell::sync::Lazy<RwLock<Vec<RecoveryParticipant>>> =
    once_cell::sync::Lazy::new(|| RwLock::new(Vec::new()));

/// Register a recovery participant
///
/// This should be called once during application initialization,
/// before any Database is opened.
///
/// # Thread Safety
///
/// This function is thread-safe and can be called from multiple threads,
/// though typically it's called once during startup.
pub fn register_recovery_participant(participant: RecoveryParticipant) {
    let mut registry = RECOVERY_REGISTRY.write();
    // Avoid duplicate registration
    if !registry.iter().any(|p| p.name == participant.name) {
        info!(name = participant.name, "Registered recovery participant");
        registry.push(participant);
    }
}

/// Run recovery for all registered participants
///
/// Called by Database::open_with_mode after KV recovery completes.
/// Each participant's recovery function is called in registration order.
///
/// # Errors
///
/// Returns the first error encountered. If a participant fails,
/// subsequent participants are not called.
pub fn recover_all_participants(db: &super::Database) -> Result<()> {
    let registry = RECOVERY_REGISTRY.read();

    for participant in registry.iter() {
        info!(name = participant.name, "Running primitive recovery");
        (participant.recover)(db)?;
        info!(name = participant.name, "Primitive recovery complete");
    }

    Ok(())
}

/// Clear the recovery registry (for testing only)
#[cfg(test)]
pub fn clear_recovery_registry() {
    let mut registry = RECOVERY_REGISTRY.write();
    registry.clear();
}
