//! Run tracking and metadata management
//!
//! This module provides the RunTracker struct for tracking active agent runs.
//! Active runs are tracked in-memory for validation and fast lookup,
//! while run metadata is persisted in storage for durability.

use in_mem_core::error::Result;
use in_mem_core::types::RunId;
use in_mem_core::value::RunMetadataEntry;
use std::collections::HashMap;
use std::sync::RwLock;

/// Run tracking and metadata management
///
/// Tracks active runs in-memory for fast lookup and validation.
/// Thread-safe via RwLock for concurrent access.
///
/// # Thread Safety
///
/// The RunTracker uses RwLock to allow concurrent reads and exclusive writes.
/// Multiple threads can check if a run is active simultaneously, while
/// begin_run and end_run require exclusive access.
pub struct RunTracker {
    /// Active runs (in-memory)
    active_runs: RwLock<HashMap<RunId, RunMetadataEntry>>,
}

impl RunTracker {
    /// Create a new RunTracker
    pub fn new() -> Self {
        Self {
            active_runs: RwLock::new(HashMap::new()),
        }
    }

    /// Register a new run as active
    ///
    /// Adds the run metadata to the active runs map.
    /// The run_id is extracted from the metadata.
    ///
    /// # Arguments
    ///
    /// * `metadata` - The run metadata entry to register
    ///
    /// # Returns
    ///
    /// Ok(()) on success
    pub fn begin_run(&self, metadata: RunMetadataEntry) -> Result<()> {
        let mut active = self.active_runs.write().unwrap();
        active.insert(metadata.run_id, metadata);
        Ok(())
    }

    /// Mark a run as ended
    ///
    /// Removes the run from the active runs map and returns the metadata.
    ///
    /// # Arguments
    ///
    /// * `run_id` - The ID of the run to end
    ///
    /// # Returns
    ///
    /// Ok(Some(metadata)) if the run was active, Ok(None) if not found
    pub fn end_run(&self, run_id: RunId) -> Result<Option<RunMetadataEntry>> {
        let mut active = self.active_runs.write().unwrap();
        Ok(active.remove(&run_id))
    }

    /// Get metadata for an active run
    ///
    /// Returns a clone of the metadata if the run is active.
    ///
    /// # Arguments
    ///
    /// * `run_id` - The ID of the run to look up
    ///
    /// # Returns
    ///
    /// Some(metadata) if active, None if not found
    pub fn get_active(&self, run_id: RunId) -> Option<RunMetadataEntry> {
        let active = self.active_runs.read().unwrap();
        active.get(&run_id).cloned()
    }

    /// List all active run IDs
    ///
    /// # Returns
    ///
    /// Vector of all currently active run IDs
    pub fn list_active(&self) -> Vec<RunId> {
        let active = self.active_runs.read().unwrap();
        active.keys().copied().collect()
    }

    /// Check if a run is currently active
    ///
    /// # Arguments
    ///
    /// * `run_id` - The ID of the run to check
    ///
    /// # Returns
    ///
    /// true if the run is active, false otherwise
    pub fn is_active(&self, run_id: RunId) -> bool {
        let active = self.active_runs.read().unwrap();
        active.contains_key(&run_id)
    }

    /// Get the count of active runs
    ///
    /// # Returns
    ///
    /// Number of currently active runs
    pub fn active_count(&self) -> usize {
        let active = self.active_runs.read().unwrap();
        active.len()
    }
}

impl Default for RunTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use in_mem_core::value::now;

    fn create_metadata(run_id: RunId) -> RunMetadataEntry {
        RunMetadataEntry {
            run_id,
            parent_run_id: None,
            status: "running".to_string(),
            created_at: now(),
            completed_at: None,
            first_version: 0,
            last_version: 0,
            tags: vec![],
        }
    }

    #[test]
    fn test_begin_run() {
        let tracker = RunTracker::new();
        let run_id = RunId::new();

        let metadata = create_metadata(run_id);
        tracker.begin_run(metadata.clone()).unwrap();

        assert!(tracker.is_active(run_id));
        assert_eq!(tracker.get_active(run_id).unwrap().run_id, run_id);
        assert_eq!(tracker.active_count(), 1);
    }

    #[test]
    fn test_end_run() {
        let tracker = RunTracker::new();
        let run_id = RunId::new();

        let metadata = create_metadata(run_id);
        tracker.begin_run(metadata).unwrap();
        assert!(tracker.is_active(run_id));

        let ended = tracker.end_run(run_id).unwrap();
        assert!(ended.is_some());
        assert_eq!(ended.unwrap().run_id, run_id);
        assert!(!tracker.is_active(run_id));
        assert_eq!(tracker.active_count(), 0);
    }

    #[test]
    fn test_end_run_not_found() {
        let tracker = RunTracker::new();
        let run_id = RunId::new();

        let ended = tracker.end_run(run_id).unwrap();
        assert!(ended.is_none());
    }

    #[test]
    fn test_list_active_runs() {
        let tracker = RunTracker::new();

        let run1 = RunId::new();
        let run2 = RunId::new();
        let run3 = RunId::new();

        tracker.begin_run(create_metadata(run1)).unwrap();
        tracker.begin_run(create_metadata(run2)).unwrap();
        tracker.begin_run(create_metadata(run3)).unwrap();

        let active = tracker.list_active();
        assert_eq!(active.len(), 3);
        assert!(active.contains(&run1));
        assert!(active.contains(&run2));
        assert!(active.contains(&run3));

        tracker.end_run(run2).unwrap();

        let active = tracker.list_active();
        assert_eq!(active.len(), 2);
        assert!(active.contains(&run1));
        assert!(!active.contains(&run2));
        assert!(active.contains(&run3));
    }

    #[test]
    fn test_get_active_not_found() {
        let tracker = RunTracker::new();
        let run_id = RunId::new();

        assert!(tracker.get_active(run_id).is_none());
    }

    #[test]
    fn test_is_active() {
        let tracker = RunTracker::new();
        let run_id = RunId::new();

        assert!(!tracker.is_active(run_id));

        tracker.begin_run(create_metadata(run_id)).unwrap();
        assert!(tracker.is_active(run_id));

        tracker.end_run(run_id).unwrap();
        assert!(!tracker.is_active(run_id));
    }

    #[test]
    fn test_run_tracker_default() {
        let tracker = RunTracker::default();
        assert_eq!(tracker.active_count(), 0);
    }

    #[test]
    fn test_metadata_with_tags() {
        let tracker = RunTracker::new();
        let run_id = RunId::new();

        let mut metadata = create_metadata(run_id);
        metadata.tags = vec![
            ("env".to_string(), "production".to_string()),
            ("version".to_string(), "1.0".to_string()),
        ];

        tracker.begin_run(metadata).unwrap();

        let retrieved = tracker.get_active(run_id).unwrap();
        assert_eq!(retrieved.tags.len(), 2);
        assert_eq!(
            retrieved.tags[0],
            ("env".to_string(), "production".to_string())
        );
    }

    #[test]
    fn test_metadata_with_parent() {
        let tracker = RunTracker::new();
        let parent_id = RunId::new();
        let child_id = RunId::new();

        let mut child_metadata = create_metadata(child_id);
        child_metadata.parent_run_id = Some(parent_id);

        tracker.begin_run(child_metadata).unwrap();

        let retrieved = tracker.get_active(child_id).unwrap();
        assert_eq!(retrieved.parent_run_id, Some(parent_id));
    }

    #[test]
    fn test_concurrent_access() {
        use std::sync::Arc;
        use std::thread;

        let tracker = Arc::new(RunTracker::new());
        let mut handles = vec![];

        // Spawn threads that begin runs
        for _ in 0..10 {
            let tracker = Arc::clone(&tracker);
            let handle = thread::spawn(move || {
                let run_id = RunId::new();
                tracker.begin_run(create_metadata(run_id)).unwrap();
                run_id
            });
            handles.push(handle);
        }

        // Collect all run IDs
        let run_ids: Vec<RunId> = handles.into_iter().map(|h| h.join().unwrap()).collect();

        // Verify all runs are active
        assert_eq!(tracker.active_count(), 10);
        for run_id in &run_ids {
            assert!(tracker.is_active(*run_id));
        }

        // End all runs concurrently
        let mut handles = vec![];
        for run_id in run_ids {
            let tracker = Arc::clone(&tracker);
            let handle = thread::spawn(move || {
                tracker.end_run(run_id).unwrap();
            });
            handles.push(handle);
        }

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(tracker.active_count(), 0);
    }
}
