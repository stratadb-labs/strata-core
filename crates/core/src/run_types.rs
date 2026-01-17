//! Run lifecycle types
//!
//! This module defines the run lifecycle types for durability and replay.
//! These are distinct from the run management types in `primitives::run_index`.
//!
//! ## Design
//!
//! - `RunStatus`: Durability-focused lifecycle states (Active, Completed, Orphaned, NotFound)
//! - `RunMetadata`: Run metadata for replay and recovery
//!
//! ## Replay Invariants (P1-P6)
//!
//! | # | Invariant | Meaning |
//! |---|-----------|---------|
//! | P1 | Pure function | Over (Snapshot, WAL, EventLog) |
//! | P2 | Side-effect free | Does not mutate canonical store |
//! | P3 | Derived view | Not a new source of truth |
//! | P4 | Does not persist | Unless explicitly materialized |
//! | P5 | Deterministic | Same inputs = Same view |
//! | P6 | Idempotent | Running twice produces identical view |

use crate::types::RunId;
use serde::{Deserialize, Serialize};

/// Run lifecycle status for durability and replay
///
/// This enum represents the lifecycle states relevant to durability:
/// - Active: Run in progress (begin_run called, end_run not yet called)
/// - Completed: Run finished normally (end_run called)
/// - Orphaned: Run was never ended (crash without end_run marker)
/// - NotFound: Run doesn't exist in the system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RunStatus {
    /// Run is active (begin_run called, end_run not yet called)
    Active,
    /// Run completed normally (end_run called)
    Completed,
    /// Run was never ended (orphaned - no end_run marker in WAL)
    Orphaned,
    /// Run doesn't exist
    NotFound,
}

impl RunStatus {
    /// Check if run is still active
    pub fn is_active(&self) -> bool {
        matches!(self, RunStatus::Active)
    }

    /// Check if run is completed
    pub fn is_completed(&self) -> bool {
        matches!(self, RunStatus::Completed)
    }

    /// Check if run is orphaned
    pub fn is_orphaned(&self) -> bool {
        matches!(self, RunStatus::Orphaned)
    }

    /// Check if run exists (any status except NotFound)
    pub fn exists(&self) -> bool {
        !matches!(self, RunStatus::NotFound)
    }

    /// Get string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            RunStatus::Active => "Active",
            RunStatus::Completed => "Completed",
            RunStatus::Orphaned => "Orphaned",
            RunStatus::NotFound => "NotFound",
        }
    }
}

impl std::fmt::Display for RunStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Run metadata for replay and recovery
///
/// Contains all information needed to replay a run and track its lifecycle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunMetadata {
    /// Run ID
    pub run_id: RunId,
    /// Current status
    pub status: RunStatus,
    /// When run started (microseconds since epoch)
    pub started_at: u64,
    /// When run ended (if completed)
    pub ended_at: Option<u64>,
    /// Number of events in this run
    pub event_count: u64,
    /// WAL offset where run began
    pub begin_wal_offset: u64,
    /// WAL offset where run ended (if completed)
    pub end_wal_offset: Option<u64>,
}

impl RunMetadata {
    /// Create metadata for a new run
    pub fn new(run_id: RunId, started_at: u64, begin_wal_offset: u64) -> Self {
        RunMetadata {
            run_id,
            status: RunStatus::Active,
            started_at,
            ended_at: None,
            event_count: 0,
            begin_wal_offset,
            end_wal_offset: None,
        }
    }

    /// Mark run as completed
    pub fn complete(&mut self, ended_at: u64, end_wal_offset: u64) {
        self.status = RunStatus::Completed;
        self.ended_at = Some(ended_at);
        self.end_wal_offset = Some(end_wal_offset);
    }

    /// Mark run as orphaned
    pub fn mark_orphaned(&mut self) {
        self.status = RunStatus::Orphaned;
    }

    /// Duration in microseconds (if completed)
    pub fn duration_micros(&self) -> Option<u64> {
        self.ended_at.map(|e| e.saturating_sub(self.started_at))
    }

    /// Increment event count
    pub fn increment_event_count(&mut self) {
        self.event_count += 1;
    }
}

/// Event offsets for a run (for O(run size) replay)
///
/// Maps a run to its event offsets in the EventLog,
/// enabling efficient replay without scanning the entire log.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RunEventOffsets {
    /// WAL offsets of events belonging to this run
    pub offsets: Vec<u64>,
}

impl RunEventOffsets {
    /// Create new empty offsets
    pub fn new() -> Self {
        RunEventOffsets {
            offsets: Vec::new(),
        }
    }

    /// Add an offset
    pub fn push(&mut self, offset: u64) {
        self.offsets.push(offset);
    }

    /// Get all offsets
    pub fn as_slice(&self) -> &[u64] {
        &self.offsets
    }

    /// Get number of offsets
    pub fn len(&self) -> usize {
        self.offsets.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_status_transitions() {
        let run_id = RunId::new();
        let mut meta = RunMetadata::new(run_id, 1000, 0);
        assert_eq!(meta.status, RunStatus::Active);
        assert!(meta.status.is_active());
        assert!(meta.status.exists());

        meta.complete(2000, 100);
        assert_eq!(meta.status, RunStatus::Completed);
        assert!(meta.status.is_completed());
        assert_eq!(meta.duration_micros(), Some(1000));
        assert_eq!(meta.end_wal_offset, Some(100));
    }

    #[test]
    fn test_run_status_orphaned() {
        let run_id = RunId::new();
        let mut meta = RunMetadata::new(run_id, 1000, 0);
        assert_eq!(meta.status, RunStatus::Active);

        meta.mark_orphaned();
        assert_eq!(meta.status, RunStatus::Orphaned);
        assert!(meta.status.is_orphaned());
        assert!(meta.status.exists());
    }

    #[test]
    fn test_run_status_not_found() {
        let status = RunStatus::NotFound;
        assert!(!status.exists());
        assert!(!status.is_active());
        assert!(!status.is_completed());
        assert!(!status.is_orphaned());
    }

    #[test]
    fn test_run_status_as_str() {
        assert_eq!(RunStatus::Active.as_str(), "Active");
        assert_eq!(RunStatus::Completed.as_str(), "Completed");
        assert_eq!(RunStatus::Orphaned.as_str(), "Orphaned");
        assert_eq!(RunStatus::NotFound.as_str(), "NotFound");
    }

    #[test]
    fn test_run_status_display() {
        assert_eq!(format!("{}", RunStatus::Active), "Active");
        assert_eq!(format!("{}", RunStatus::Completed), "Completed");
    }

    #[test]
    fn test_run_metadata_serialization() {
        let run_id = RunId::new();
        let meta = RunMetadata::new(run_id, 1000, 50);

        let json = serde_json::to_string(&meta).unwrap();
        let restored: RunMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(meta, restored);
    }

    #[test]
    fn test_run_event_offsets() {
        let mut offsets = RunEventOffsets::new();
        assert!(offsets.is_empty());
        assert_eq!(offsets.len(), 0);

        offsets.push(100);
        offsets.push(200);
        offsets.push(300);

        assert!(!offsets.is_empty());
        assert_eq!(offsets.len(), 3);
        assert_eq!(offsets.as_slice(), &[100, 200, 300]);
    }

    #[test]
    fn test_run_metadata_event_count() {
        let run_id = RunId::new();
        let mut meta = RunMetadata::new(run_id, 1000, 0);
        assert_eq!(meta.event_count, 0);

        meta.increment_event_count();
        meta.increment_event_count();
        meta.increment_event_count();

        assert_eq!(meta.event_count, 3);
    }
}
