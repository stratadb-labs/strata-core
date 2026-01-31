//! Run lifecycle types
//!
//! This module defines the run lifecycle types for durability and replay.
//! These are distinct from the run management types in `primitives::run_index`.
//!
//! ## Design
//!
//! - `BranchStatus`: Durability-focused lifecycle states (Active, Completed, Orphaned, NotFound)
//! - `BranchMetadata`: Run metadata for replay and recovery
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

use crate::types::BranchId;
use serde::{Deserialize, Serialize};

/// Run lifecycle status for durability and replay
///
/// This enum represents the lifecycle states relevant to durability:
/// - Active: Run in progress (begin_run called, end_run not yet called)
/// - Completed: Run finished normally (end_run called)
/// - Orphaned: Run was never ended (crash without end_run marker)
/// - NotFound: Run doesn't exist in the system
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BranchStatus {
    /// Run is active (begin_run called, end_run not yet called)
    Active,
    /// Run completed normally (end_run called)
    Completed,
    /// Run was never ended (orphaned - no end_run marker in WAL)
    Orphaned,
    /// Run doesn't exist
    NotFound,
}

impl BranchStatus {
    /// Check if run is still active
    pub fn is_active(&self) -> bool {
        matches!(self, BranchStatus::Active)
    }

    /// Check if run is completed
    pub fn is_completed(&self) -> bool {
        matches!(self, BranchStatus::Completed)
    }

    /// Check if run is orphaned
    pub fn is_orphaned(&self) -> bool {
        matches!(self, BranchStatus::Orphaned)
    }

    /// Check if run exists (any status except NotFound)
    pub fn exists(&self) -> bool {
        !matches!(self, BranchStatus::NotFound)
    }

    /// Get string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            BranchStatus::Active => "Active",
            BranchStatus::Completed => "Completed",
            BranchStatus::Orphaned => "Orphaned",
            BranchStatus::NotFound => "NotFound",
        }
    }
}

impl std::fmt::Display for BranchStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Run metadata for replay and recovery
///
/// Contains all information needed to replay a run and track its lifecycle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BranchMetadata {
    /// Run ID
    pub branch_id: BranchId,
    /// Current status
    pub status: BranchStatus,
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

impl BranchMetadata {
    /// Create metadata for a new run
    pub fn new(branch_id: BranchId, started_at: u64, begin_wal_offset: u64) -> Self {
        BranchMetadata {
            branch_id,
            status: BranchStatus::Active,
            started_at,
            ended_at: None,
            event_count: 0,
            begin_wal_offset,
            end_wal_offset: None,
        }
    }

    /// Mark run as completed
    pub fn complete(&mut self, ended_at: u64, end_wal_offset: u64) {
        self.status = BranchStatus::Completed;
        self.ended_at = Some(ended_at);
        self.end_wal_offset = Some(end_wal_offset);
    }

    /// Mark run as orphaned
    pub fn mark_orphaned(&mut self) {
        self.status = BranchStatus::Orphaned;
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
pub struct BranchEventOffsets {
    /// WAL offsets of events belonging to this run
    pub offsets: Vec<u64>,
}

impl BranchEventOffsets {
    /// Create new empty offsets
    pub fn new() -> Self {
        BranchEventOffsets {
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
        let branch_id = BranchId::new();
        let mut meta = BranchMetadata::new(branch_id, 1000, 0);
        assert_eq!(meta.status, BranchStatus::Active);
        assert!(meta.status.is_active());
        assert!(meta.status.exists());

        meta.complete(2000, 100);
        assert_eq!(meta.status, BranchStatus::Completed);
        assert!(meta.status.is_completed());
        assert_eq!(meta.duration_micros(), Some(1000));
        assert_eq!(meta.end_wal_offset, Some(100));
    }

    #[test]
    fn test_run_status_orphaned() {
        let branch_id = BranchId::new();
        let mut meta = BranchMetadata::new(branch_id, 1000, 0);
        assert_eq!(meta.status, BranchStatus::Active);

        meta.mark_orphaned();
        assert_eq!(meta.status, BranchStatus::Orphaned);
        assert!(meta.status.is_orphaned());
        assert!(meta.status.exists());
    }

    #[test]
    fn test_run_status_not_found() {
        let status = BranchStatus::NotFound;
        assert!(!status.exists());
        assert!(!status.is_active());
        assert!(!status.is_completed());
        assert!(!status.is_orphaned());
    }

    #[test]
    fn test_run_status_as_str() {
        assert_eq!(BranchStatus::Active.as_str(), "Active");
        assert_eq!(BranchStatus::Completed.as_str(), "Completed");
        assert_eq!(BranchStatus::Orphaned.as_str(), "Orphaned");
        assert_eq!(BranchStatus::NotFound.as_str(), "NotFound");
    }

    #[test]
    fn test_run_status_display() {
        assert_eq!(format!("{}", BranchStatus::Active), "Active");
        assert_eq!(format!("{}", BranchStatus::Completed), "Completed");
    }

    #[test]
    fn test_run_metadata_serialization() {
        let branch_id = BranchId::new();
        let meta = BranchMetadata::new(branch_id, 1000, 50);

        let json = serde_json::to_string(&meta).unwrap();
        let restored: BranchMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(meta, restored);
    }

    #[test]
    fn test_run_event_offsets() {
        let mut offsets = BranchEventOffsets::new();
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
        let branch_id = BranchId::new();
        let mut meta = BranchMetadata::new(branch_id, 1000, 0);
        assert_eq!(meta.event_count, 0);

        meta.increment_event_count();
        meta.increment_event_count();
        meta.increment_event_count();

        assert_eq!(meta.event_count, 3);
    }

    #[test]
    fn test_run_metadata_duration_active() {
        let branch_id = BranchId::new();
        let meta = BranchMetadata::new(branch_id, 1000, 0);
        // Active run has no duration
        assert!(meta.duration_micros().is_none());
    }

    #[test]
    fn test_run_event_offsets_serialization() {
        let mut offsets = BranchEventOffsets::new();
        offsets.push(10);
        offsets.push(20);

        let json = serde_json::to_string(&offsets).unwrap();
        let restored: BranchEventOffsets = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.as_slice(), &[10, 20]);
    }

    #[test]
    fn test_run_event_offsets_default() {
        let offsets = BranchEventOffsets::default();
        assert!(offsets.is_empty());
        assert_eq!(offsets.len(), 0);
    }

    #[test]
    fn test_run_status_serialization_roundtrip() {
        for status in [BranchStatus::Active, BranchStatus::Completed, BranchStatus::Orphaned, BranchStatus::NotFound] {
            let json = serde_json::to_string(&status).unwrap();
            let restored: BranchStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, restored);
        }
    }

    #[test]
    fn test_run_metadata_double_completion() {
        let branch_id = BranchId::new();
        let mut meta = BranchMetadata::new(branch_id, 1000, 0);
        meta.complete(2000, 100);
        assert_eq!(meta.status, BranchStatus::Completed);

        // Complete again with different values - should overwrite
        meta.complete(3000, 200);
        assert_eq!(meta.ended_at, Some(3000));
        assert_eq!(meta.end_wal_offset, Some(200));
    }

    #[test]
    fn test_run_metadata_orphaned_after_completed() {
        let branch_id = BranchId::new();
        let mut meta = BranchMetadata::new(branch_id, 1000, 0);
        meta.complete(2000, 100);
        meta.mark_orphaned();
        // Status transitions are unconditional - design decision
        assert_eq!(meta.status, BranchStatus::Orphaned);
    }

    #[test]
    fn test_run_metadata_ended_before_started() {
        let branch_id = BranchId::new();
        let mut meta = BranchMetadata::new(branch_id, 2000, 0);
        meta.complete(1000, 50);
        // duration_micros uses saturating_sub, so negative durations become 0
        assert_eq!(meta.duration_micros(), Some(0));
    }

    #[test]
    fn test_run_status_hash_uniqueness() {
        use std::collections::HashSet;
        let statuses = [BranchStatus::Active, BranchStatus::Completed, BranchStatus::Orphaned, BranchStatus::NotFound];
        let set: HashSet<BranchStatus> = statuses.iter().copied().collect();
        assert_eq!(set.len(), 4, "All BranchStatus variants must hash uniquely");
    }

    #[test]
    fn test_run_status_display_all_variants() {
        assert_eq!(format!("{}", BranchStatus::Active), "Active");
        assert_eq!(format!("{}", BranchStatus::Completed), "Completed");
        assert_eq!(format!("{}", BranchStatus::Orphaned), "Orphaned");
        assert_eq!(format!("{}", BranchStatus::NotFound), "NotFound");
    }

    #[test]
    fn test_run_event_offsets_ordering_not_enforced() {
        // Offsets are just a Vec - no ordering enforcement
        let mut offsets = BranchEventOffsets::new();
        offsets.push(300);
        offsets.push(100);
        offsets.push(200);
        assert_eq!(offsets.as_slice(), &[300, 100, 200]);
    }
}
