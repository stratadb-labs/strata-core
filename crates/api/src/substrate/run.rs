//! RunIndex Substrate Operations
//!
//! The RunIndex manages run lifecycle and metadata.
//! **RunIndex is an execution index, not an execution engine.**
//!
//! It tracks execution contexts (runs) as first-class entities but does NOT
//! schedule, orchestrate, or execute anything.
//!
//! ## Run Model
//!
//! - Every entity belongs to exactly one run (Invariant 5)
//! - The "default" run always exists and cannot be closed/deleted/archived
//! - Custom runs are created with UUIDs
//! - Closed runs are read-only
//!
//! ## Canonical State Transition Table
//!
//! ```text
//! From State   → Valid Transitions
//! ─────────────────────────────────────────────────────────────
//! Active       → Completed | Failed | Cancelled | Paused | Archived
//! Paused       → Active | Cancelled | Archived
//! Completed    → Archived
//! Failed       → Archived
//! Cancelled    → Archived
//! Archived     → (TERMINAL - no transitions allowed)
//! ```
//!
//! **Invariants:**
//! - No resurrection: Once Completed/Failed/Cancelled, cannot return to Active
//! - Terminal is final: Archived accepts no further transitions
//! - Pause is reversible: Paused can resume to Active
//!
//! ## Versioning
//!
//! Run info uses transaction-based versioning (`Version::Txn`).

use super::types::{ApiRunId, RetentionPolicy, RunInfo, RunState};
use strata_core::{StrataResult, Value, Version, Versioned};

/// Reserved metadata key for retention policy storage.
///
/// This key is part of the `_strata_*` reserved namespace and should not
/// be used for user metadata.
pub const RETENTION_METADATA_KEY: &str = "_strata_retention";

/// RunIndex substrate operations
///
/// This trait defines the canonical run management operations.
///
/// ## Contract
///
/// - "default" run always exists
/// - "default" run cannot be closed
/// - Run IDs are unique (UUID or "default")
/// - Closed runs are read-only for data primitives
///
/// ## Error Handling
///
/// | Condition | Error |
/// |-----------|-------|
/// | Invalid run ID format | `InvalidKey` |
/// | Run already exists | `ConstraintViolation` |
/// | Run not found | `NotFound` |
/// | Cannot close default run | `ConstraintViolation` |
/// | Run already closed | `ConstraintViolation` |
pub trait RunIndex {
    /// Create a new run
    ///
    /// Creates a new run with optional metadata.
    /// Returns the run info and version.
    ///
    /// ## Parameters
    ///
    /// - `run_id`: Optional specific ID (if None, generates UUID)
    /// - `metadata`: Optional metadata (must be Object or Null)
    ///
    /// ## Return Value
    ///
    /// Returns `(run_info, version)`.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Run ID format is invalid
    /// - `ConstraintViolation`: Run already exists, or metadata not Object/Null
    fn run_create(
        &self,
        run_id: Option<&ApiRunId>,
        metadata: Option<Value>,
    ) -> StrataResult<(RunInfo, Version)>;

    /// Get run info
    ///
    /// Returns information about a run.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Run ID format is invalid
    /// - `NotFound`: Run does not exist
    fn run_get(&self, run: &ApiRunId) -> StrataResult<Option<Versioned<RunInfo>>>;

    /// List all runs
    ///
    /// Returns all runs matching the filters.
    ///
    /// ## Parameters
    ///
    /// - `state`: Filter by state (Active/Closed)
    /// - `limit`: Maximum runs to return
    /// - `offset`: Skip first N runs
    ///
    /// ## Return Value
    ///
    /// Vector of run info, ordered by creation time (newest first).
    fn run_list(
        &self,
        state: Option<RunState>,
        limit: Option<u64>,
        offset: Option<u64>,
    ) -> StrataResult<Vec<Versioned<RunInfo>>>;

    /// Close a run (mark as completed)
    ///
    /// Close is synonymous with successful completion. Transitions the run
    /// to Completed state. This is the happy-path termination.
    ///
    /// For other termination modes:
    /// - [`run_fail`](Self::run_fail) for failures (with error message)
    /// - [`run_cancel`](Self::run_cancel) for user-initiated cancellation
    /// - [`run_archive`](Self::run_archive) for soft delete (terminal)
    ///
    /// ## Semantics
    ///
    /// - Cannot close "default" run
    /// - Cannot close already-closed run
    /// - After closing, all write operations fail with `ConstraintViolation`
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Run ID format is invalid
    /// - `NotFound`: Run does not exist
    /// - `ConstraintViolation`: Cannot close default run, or already closed
    fn run_close(&self, run: &ApiRunId) -> StrataResult<Version>;

    /// Update run metadata
    ///
    /// Updates the metadata for a run.
    /// Returns the new version.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Run ID format is invalid
    /// - `NotFound`: Run does not exist
    /// - `ConstraintViolation`: Run is closed, or metadata not Object/Null
    fn run_update_metadata(&self, run: &ApiRunId, metadata: Value) -> StrataResult<Version>;

    /// Check if a run exists
    ///
    /// Returns `true` if the run exists (regardless of state).
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Run ID format is invalid
    fn run_exists(&self, run: &ApiRunId) -> StrataResult<bool>;

    /// Pause a run
    ///
    /// Transitions the run to Paused state. Can be resumed later.
    ///
    /// ## Errors
    ///
    /// - `NotFound`: Run does not exist
    /// - `ConstraintViolation`: Run is not in Active state
    fn run_pause(&self, run: &ApiRunId) -> StrataResult<Version>;

    /// Resume a paused run
    ///
    /// Transitions the run from Paused back to Active state.
    ///
    /// ## Errors
    ///
    /// - `NotFound`: Run does not exist
    /// - `ConstraintViolation`: Run is not in Paused state
    fn run_resume(&self, run: &ApiRunId) -> StrataResult<Version>;

    /// Fail a run with an error message
    ///
    /// Transitions the run to Failed state with an error message.
    ///
    /// ## Errors
    ///
    /// - `NotFound`: Run does not exist
    /// - `ConstraintViolation`: Run is in terminal state
    fn run_fail(&self, run: &ApiRunId, error: &str) -> StrataResult<Version>;

    /// Cancel a run
    ///
    /// Transitions the run to Cancelled state. This represents user-initiated
    /// termination, distinct from failure (error) or completion (success).
    ///
    /// ## Valid Transitions
    ///
    /// Can cancel from: Active, Paused
    /// Cannot cancel from: Completed, Failed, Cancelled, Archived
    ///
    /// ## Errors
    ///
    /// - `NotFound`: Run does not exist
    /// - `ConstraintViolation`: Run is in terminal or finished state
    fn run_cancel(&self, run: &ApiRunId) -> StrataResult<Version>;

    /// Archive a run (soft delete)
    ///
    /// Transitions the run to Archived state. This is a **terminal** state -
    /// no further transitions are allowed. Data is preserved but the run is
    /// considered "soft deleted".
    ///
    /// Use this instead of [`run_delete`](Self::run_delete) when you want to:
    /// - Hide runs from normal queries
    /// - Preserve data for compliance/auditing
    /// - Allow potential future recovery (manual)
    ///
    /// ## Valid Transitions
    ///
    /// Can archive from: Active, Paused, Completed, Failed, Cancelled
    /// Cannot archive from: Archived (already terminal)
    ///
    /// ## Errors
    ///
    /// - `NotFound`: Run does not exist
    /// - `ConstraintViolation`: Run is already archived, or is the default run
    fn run_archive(&self, run: &ApiRunId) -> StrataResult<Version>;

    /// Delete a run and all its data
    ///
    /// This operation is destructive and cascades to all run data.
    /// The default run cannot be deleted.
    ///
    /// ## Errors
    ///
    /// - `NotFound`: Run does not exist
    /// - `ConstraintViolation`: Cannot delete the default run
    fn run_delete(&self, run: &ApiRunId) -> StrataResult<()>;

    /// Query runs by status
    ///
    /// Returns all runs that are in the specified state.
    ///
    /// ## Errors
    ///
    /// - Storage errors
    fn run_query_by_status(&self, state: RunState) -> StrataResult<Vec<Versioned<RunInfo>>>;

    /// Query runs by tag
    ///
    /// Returns all runs that have the specified tag.
    /// Uses secondary index for efficient lookups.
    ///
    /// ## Parameters
    ///
    /// - `tag`: The tag to search for (exact match)
    ///
    /// ## Return Value
    ///
    /// Vector of run info for matching runs.
    fn run_query_by_tag(&self, tag: &str) -> StrataResult<Vec<Versioned<RunInfo>>>;

    /// Count runs
    ///
    /// Returns the total number of runs, optionally filtered by status.
    ///
    /// ## Parameters
    ///
    /// - `status`: Optional status filter. If None, counts all runs.
    fn run_count(&self, status: Option<RunState>) -> StrataResult<u64>;

    /// Search runs (metadata and index only)
    ///
    /// Searches run IDs, status, tags, and metadata fields.
    /// Does NOT search run contents (KV, Events, State, etc.).
    ///
    /// This is **RunIndex search**, not Strata-wide search. The search scope
    /// is strictly the RunMetadata fields.
    ///
    /// ## Parameters
    ///
    /// - `query`: Search query string
    /// - `limit`: Maximum results to return (default: 10)
    fn run_search(&self, query: &str, limit: Option<u64>) -> StrataResult<Vec<Versioned<RunInfo>>>;

    // =========================================================================
    // Tag Management
    // =========================================================================

    /// Add tags to a run
    ///
    /// Tags are used for categorization and querying via [`run_query_by_tag`](Self::run_query_by_tag).
    /// Duplicate tags are ignored (idempotent).
    ///
    /// ## Parameters
    ///
    /// - `run`: The run to tag
    /// - `tags`: Tags to add
    ///
    /// ## Errors
    ///
    /// - `NotFound`: Run does not exist
    fn run_add_tags(&self, run: &ApiRunId, tags: &[String]) -> StrataResult<Version>;

    /// Remove tags from a run
    ///
    /// Tags that don't exist on the run are ignored (idempotent).
    ///
    /// ## Parameters
    ///
    /// - `run`: The run to untag
    /// - `tags`: Tags to remove
    ///
    /// ## Errors
    ///
    /// - `NotFound`: Run does not exist
    fn run_remove_tags(&self, run: &ApiRunId, tags: &[String]) -> StrataResult<Version>;

    /// Get tags for a run
    ///
    /// Returns all tags currently assigned to the run.
    ///
    /// ## Errors
    ///
    /// - `NotFound`: Run does not exist
    fn run_get_tags(&self, run: &ApiRunId) -> StrataResult<Vec<String>>;

    // =========================================================================
    // Hierarchy (Parent-Child)
    // =========================================================================
    //
    // IMPORTANT: Parent-child relationships are INFORMATIONAL, not TRANSACTIONAL.
    //
    // This means:
    // - No cascading state: Parent state changes do NOT affect children
    // - No implicit propagation: Tags, metadata, retention do NOT inherit
    // - No shared lifecycle: Parent completion does NOT complete children
    // - No transactional coupling: Parent and child are independent execution contexts
    //
    // The parent pointer enables:
    // - "Show me all runs forked from X"
    // - "What was the parent of this run?"
    // - Hierarchical visualization
    //
    // Delete semantics: Deleting a parent does NOT delete children.
    // Children become orphaned (parent_run becomes dangling reference).

    /// Create a child run
    ///
    /// Creates a new run with a parent relationship.
    /// Useful for forked/nested runs.
    ///
    /// The parent-child relationship is informational only - no state
    /// inheritance or transactional coupling.
    ///
    /// ## Parameters
    ///
    /// - `parent`: The parent run (must exist)
    /// - `metadata`: Optional metadata for the new run
    ///
    /// ## Return Value
    ///
    /// Returns the new run info and version.
    ///
    /// ## Errors
    ///
    /// - `NotFound`: Parent run does not exist
    fn run_create_child(
        &self,
        parent: &ApiRunId,
        metadata: Option<Value>,
    ) -> StrataResult<(RunInfo, Version)>;

    /// Get child runs
    ///
    /// Returns all runs that have the specified run as their parent.
    ///
    /// ## Parameters
    ///
    /// - `parent`: The parent run
    fn run_get_children(&self, parent: &ApiRunId) -> StrataResult<Vec<Versioned<RunInfo>>>;

    /// Get parent run
    ///
    /// Returns the parent run ID if this run has a parent.
    /// Returns `None` if the run has no parent (is a root run).
    ///
    /// ## Errors
    ///
    /// - `NotFound`: Run does not exist
    fn run_get_parent(&self, run: &ApiRunId) -> StrataResult<Option<ApiRunId>>;

    // =========================================================================
    // Retention Policy
    // =========================================================================

    /// Set retention policy for a run
    ///
    /// Configures the history retention policy for a run.
    /// Returns the new version.
    ///
    /// ## Implementation
    ///
    /// Retention policy is stored in run metadata under the reserved key
    /// `_strata_retention`. This key is part of the `_strata_*` reserved
    /// namespace and must not be used for user metadata.
    ///
    /// ## Semantics
    ///
    /// - Policy applies to all primitives in the run
    /// - **Setting retention does NOT immediately delete data**
    /// - Enforcement occurs during compaction/garbage collection
    /// - Existing history beyond policy will be collected at next compaction
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Run ID format is invalid
    /// - `NotFound`: Run does not exist
    fn run_set_retention(&self, run: &ApiRunId, policy: RetentionPolicy) -> StrataResult<Version>;

    /// Get retention policy for a run
    ///
    /// Returns the current retention policy. If no policy has been set,
    /// returns `RetentionPolicy::KeepAll` (the default).
    ///
    /// ## Implementation
    ///
    /// Reads retention policy from run metadata under the reserved key
    /// `_strata_retention`.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Run ID format is invalid
    /// - `NotFound`: Run does not exist
    fn run_get_retention(&self, run: &ApiRunId) -> StrataResult<RetentionPolicy>;
}

// =============================================================================
// Implementation
// =============================================================================

use strata_core::StrataError;
use super::impl_::{SubstrateImpl, convert_error, api_run_id_to_string};

impl RunIndex for SubstrateImpl {
    fn run_create(
        &self,
        run_id: Option<&ApiRunId>,
        metadata: Option<Value>,
    ) -> StrataResult<(RunInfo, Version)> {
        let run_str = run_id.map(api_run_id_to_string).unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        let versioned = if let Some(meta) = metadata {
            self.run().create_run_with_options(&run_str, None, vec![], meta).map_err(convert_error)?
        } else {
            self.run().create_run(&run_str).map_err(convert_error)?
        };

        let api_run_id = run_id.cloned().unwrap_or_else(|| {
            ApiRunId::parse(&run_str).unwrap_or_else(|| ApiRunId::new())
        });

        let info = RunInfo {
            run_id: api_run_id,
            created_at: (versioned.value.created_at.max(0) as u64).saturating_mul(1000),
            metadata: versioned.value.metadata,
            state: convert_run_status(&versioned.value.status),
            error: versioned.value.error,
        };

        Ok((info, versioned.version))
    }

    fn run_get(&self, run: &ApiRunId) -> StrataResult<Option<Versioned<RunInfo>>> {
        let run_str = api_run_id_to_string(run);
        let meta = self.run().get_run(&run_str).map_err(convert_error)?;

        Ok(meta.map(|m| {
            let info = RunInfo {
                run_id: run.clone(),
                // Primitive stores created_at as i64 millis, convert to u64 micros
                created_at: (m.value.created_at.max(0) as u64).saturating_mul(1000),
                metadata: m.value.metadata,
                state: convert_run_status(&m.value.status),
                error: m.value.error,
            };
            Versioned {
                value: info,
                version: Version::Txn(0),
                // Convert i64 millis to Timestamp
                timestamp: strata_core::Timestamp::from_micros(m.value.created_at),
            }
        }))
    }

    fn run_list(
        &self,
        state: Option<RunState>,
        limit: Option<u64>,
        _offset: Option<u64>,
    ) -> StrataResult<Vec<Versioned<RunInfo>>> {
        let run_ids = if let Some(s) = state {
            let primitive_status = convert_run_state_to_status(s);
            self.run().query_by_status(primitive_status).map_err(convert_error)?
        } else {
            // Get all runs
            let ids = self.run().list_runs().map_err(convert_error)?;
            let mut runs = Vec::new();
            for id in ids {
                if let Some(versioned) = self.run().get_run(&id).map_err(convert_error)? {
                    runs.push(versioned.value);
                }
            }
            runs
        };

        let limited = match limit {
            Some(l) => run_ids.into_iter().take(l as usize).collect(),
            None => run_ids,
        };

        Ok(limited
            .into_iter()
            .map(|m| {
                let api_run_id = ApiRunId::parse(&m.run_id).unwrap_or_else(|| ApiRunId::new());
                let info = RunInfo {
                    run_id: api_run_id,
                    // Primitive stores created_at as i64 millis, convert to u64 micros
                    created_at: (m.created_at.max(0) as u64).saturating_mul(1000),
                    metadata: m.metadata,
                    state: convert_run_status(&m.status),
                    error: m.error,
                };
                Versioned {
                    value: info,
                    version: Version::Txn(0),
                    // Convert i64 millis to Timestamp
                    timestamp: strata_core::Timestamp::from_micros(m.created_at),
                }
            })
            .collect())
    }

    fn run_close(&self, run: &ApiRunId) -> StrataResult<Version> {
        if run.is_default() {
            return Err(StrataError::invalid_operation(
                strata_core::EntityRef::run(run.to_run_id()),
                "Cannot close the default run",
            ));
        }
        let run_str = api_run_id_to_string(run);
        let versioned = self.run().complete_run(&run_str).map_err(convert_error)?;
        Ok(versioned.version)
    }

    fn run_update_metadata(&self, run: &ApiRunId, metadata: Value) -> StrataResult<Version> {
        let run_str = api_run_id_to_string(run);
        let versioned = self.run().update_metadata(&run_str, metadata).map_err(convert_error)?;
        Ok(versioned.version)
    }

    fn run_exists(&self, run: &ApiRunId) -> StrataResult<bool> {
        let run_str = api_run_id_to_string(run);
        self.run().exists(&run_str).map_err(convert_error)
    }

    fn run_set_retention(&self, run: &ApiRunId, policy: RetentionPolicy) -> StrataResult<Version> {
        let run_str = api_run_id_to_string(run);

        // Get current run metadata
        let current = self.run().get_run(&run_str).map_err(convert_error)?
            .ok_or_else(|| strata_core::StrataError::not_found(
                strata_core::EntityRef::run(run.to_run_id())
            ))?;

        // Merge retention policy into metadata
        let mut metadata = match current.value.metadata {
            Value::Object(map) => map,
            Value::Null => std::collections::HashMap::new(),
            _ => std::collections::HashMap::new(),
        };

        // Serialize retention policy to JSON string and store as String value
        let retention_json = serde_json::to_string(&policy)
            .map_err(|e| strata_core::StrataError::serialization(e.to_string()))?;

        metadata.insert(RETENTION_METADATA_KEY.to_string(), Value::String(retention_json));

        // Update metadata with merged retention
        let versioned = self.run()
            .update_metadata(&run_str, Value::Object(metadata))
            .map_err(convert_error)?;

        Ok(versioned.version)
    }

    fn run_get_retention(&self, run: &ApiRunId) -> StrataResult<RetentionPolicy> {
        let run_str = api_run_id_to_string(run);

        let meta = self.run().get_run(&run_str).map_err(convert_error)?
            .ok_or_else(|| strata_core::StrataError::not_found(
                strata_core::EntityRef::run(run.to_run_id())
            ))?;

        // Extract retention policy from metadata (stored as JSON string)
        if let Value::Object(map) = &meta.value.metadata {
            if let Some(Value::String(retention_json)) = map.get(RETENTION_METADATA_KEY) {
                let policy: RetentionPolicy = serde_json::from_str(retention_json)
                    .map_err(|e| strata_core::StrataError::serialization(e.to_string()))?;
                return Ok(policy);
            }
        }

        // Default: KeepAll
        Ok(RetentionPolicy::KeepAll)
    }

    fn run_pause(&self, run: &ApiRunId) -> StrataResult<Version> {
        let run_str = api_run_id_to_string(run);
        let versioned = self.run().pause_run(&run_str).map_err(convert_error)?;
        Ok(versioned.version)
    }

    fn run_resume(&self, run: &ApiRunId) -> StrataResult<Version> {
        let run_str = api_run_id_to_string(run);
        let versioned = self.run().resume_run(&run_str).map_err(convert_error)?;
        Ok(versioned.version)
    }

    fn run_fail(&self, run: &ApiRunId, error: &str) -> StrataResult<Version> {
        let run_str = api_run_id_to_string(run);
        let versioned = self.run().fail_run(&run_str, error).map_err(convert_error)?;
        Ok(versioned.version)
    }

    fn run_cancel(&self, run: &ApiRunId) -> StrataResult<Version> {
        let run_str = api_run_id_to_string(run);
        let versioned = self.run().cancel_run(&run_str).map_err(convert_error)?;
        Ok(versioned.version)
    }

    fn run_archive(&self, run: &ApiRunId) -> StrataResult<Version> {
        if run.is_default() {
            return Err(StrataError::invalid_operation(
                strata_core::EntityRef::run(run.to_run_id()),
                "Cannot archive the default run",
            ));
        }
        let run_str = api_run_id_to_string(run);
        let versioned = self.run().archive_run(&run_str).map_err(convert_error)?;
        Ok(versioned.version)
    }

    fn run_delete(&self, run: &ApiRunId) -> StrataResult<()> {
        if run.is_default() {
            return Err(StrataError::invalid_operation(
                strata_core::EntityRef::run(run.to_run_id()),
                "Cannot delete the default run",
            ));
        }
        let run_str = api_run_id_to_string(run);
        self.run().delete_run(&run_str).map_err(convert_error)
    }

    fn run_query_by_status(&self, state: RunState) -> StrataResult<Vec<Versioned<RunInfo>>> {
        let primitive_status = convert_run_state_to_status(state);
        let runs = self.run().query_by_status(primitive_status).map_err(convert_error)?;
        Ok(runs.into_iter().map(metadata_to_versioned_info).collect())
    }

    fn run_query_by_tag(&self, tag: &str) -> StrataResult<Vec<Versioned<RunInfo>>> {
        let runs = self.run().query_by_tag(tag).map_err(convert_error)?;
        Ok(runs.into_iter().map(metadata_to_versioned_info).collect())
    }

    fn run_count(&self, status: Option<RunState>) -> StrataResult<u64> {
        match status {
            Some(s) => {
                let primitive_status = convert_run_state_to_status(s);
                let runs = self.run().query_by_status(primitive_status).map_err(convert_error)?;
                Ok(runs.len() as u64)
            }
            None => {
                let count = self.run().count().map_err(convert_error)?;
                Ok(count as u64)
            }
        }
    }

    fn run_search(&self, query: &str, limit: Option<u64>) -> StrataResult<Vec<Versioned<RunInfo>>> {
        use strata_core::{SearchRequest, SearchBudget};
        use strata_core::types::RunId;

        let req = SearchRequest {
            run_id: RunId::from_bytes([0; 16]), // Global namespace for RunIndex
            query: query.to_string(),
            k: limit.unwrap_or(10) as usize,
            budget: SearchBudget::default(),
            time_range: None,
            mode: Default::default(),
            primitive_filter: None,
            tags_any: vec![],
        };

        let response = self.run().search(&req).map_err(convert_error)?;

        // Convert hits back to RunInfo by looking up each run
        let mut results = Vec::new();
        for hit in response.hits {
            if let strata_core::search_types::EntityRef::Run { run_id } = hit.doc_ref {
                let api_run_id = ApiRunId::from_uuid(uuid::Uuid::from_bytes(*run_id.as_bytes()));
                if let Some(info) = self.run_get(&api_run_id)? {
                    results.push(info);
                }
            }
        }
        Ok(results)
    }

    fn run_add_tags(&self, run: &ApiRunId, tags: &[String]) -> StrataResult<Version> {
        let run_str = api_run_id_to_string(run);
        let versioned = self.run().add_tags(&run_str, tags.to_vec()).map_err(convert_error)?;
        Ok(versioned.version)
    }

    fn run_remove_tags(&self, run: &ApiRunId, tags: &[String]) -> StrataResult<Version> {
        let run_str = api_run_id_to_string(run);
        let versioned = self.run().remove_tags(&run_str, tags.to_vec()).map_err(convert_error)?;
        Ok(versioned.version)
    }

    fn run_get_tags(&self, run: &ApiRunId) -> StrataResult<Vec<String>> {
        let run_str = api_run_id_to_string(run);
        let meta = self.run().get_run(&run_str).map_err(convert_error)?
            .ok_or_else(|| StrataError::not_found(
                strata_core::EntityRef::run(run.to_run_id())
            ))?;
        Ok(meta.value.tags)
    }

    fn run_create_child(
        &self,
        parent: &ApiRunId,
        metadata: Option<Value>,
    ) -> StrataResult<(RunInfo, Version)> {
        let parent_str = api_run_id_to_string(parent);
        let child_id = uuid::Uuid::new_v4().to_string();

        let versioned = self.run().create_run_with_options(
            &child_id,
            Some(parent_str),
            vec![],
            metadata.unwrap_or(Value::Null),
        ).map_err(convert_error)?;

        let api_run_id = ApiRunId::parse(&child_id).unwrap_or_else(ApiRunId::new);
        let info = RunInfo {
            run_id: api_run_id,
            created_at: (versioned.value.created_at.max(0) as u64).saturating_mul(1000),
            metadata: versioned.value.metadata,
            state: convert_run_status(&versioned.value.status),
            error: versioned.value.error,
        };

        Ok((info, versioned.version))
    }

    fn run_get_children(&self, parent: &ApiRunId) -> StrataResult<Vec<Versioned<RunInfo>>> {
        let parent_str = api_run_id_to_string(parent);
        let children = self.run().get_child_runs(&parent_str).map_err(convert_error)?;
        Ok(children.into_iter().map(metadata_to_versioned_info).collect())
    }

    fn run_get_parent(&self, run: &ApiRunId) -> StrataResult<Option<ApiRunId>> {
        let run_str = api_run_id_to_string(run);
        let meta = self.run().get_run(&run_str).map_err(convert_error)?
            .ok_or_else(|| StrataError::not_found(
                strata_core::EntityRef::run(run.to_run_id())
            ))?;

        Ok(meta.value.parent_run.and_then(|p| ApiRunId::parse(&p)))
    }
}

fn convert_run_status(status: &strata_engine::RunStatus) -> RunState {
    match status {
        strata_engine::RunStatus::Active => RunState::Active,
        strata_engine::RunStatus::Completed => RunState::Completed,
        strata_engine::RunStatus::Failed => RunState::Failed,
        strata_engine::RunStatus::Cancelled => RunState::Cancelled,
        strata_engine::RunStatus::Paused => RunState::Paused,
        strata_engine::RunStatus::Archived => RunState::Archived,
    }
}

fn convert_run_state_to_status(state: RunState) -> strata_engine::RunStatus {
    match state {
        RunState::Active => strata_engine::RunStatus::Active,
        RunState::Completed => strata_engine::RunStatus::Completed,
        RunState::Failed => strata_engine::RunStatus::Failed,
        RunState::Cancelled => strata_engine::RunStatus::Cancelled,
        RunState::Paused => strata_engine::RunStatus::Paused,
        RunState::Archived => strata_engine::RunStatus::Archived,
    }
}

/// Convert primitive RunMetadata to substrate Versioned<RunInfo>
///
/// Note: RunMetadata has two ID fields:
/// - `name`: The user-provided key (what substrate calls "run_id")
/// - `run_id`: An internal UUID for namespacing
///
/// We use `name` here because that's what the substrate layer uses as the run identifier.
fn metadata_to_versioned_info(m: strata_engine::RunMetadata) -> Versioned<RunInfo> {
    // Use m.name, not m.run_id - name is the user-provided run identifier
    let api_run_id = ApiRunId::parse(&m.name).unwrap_or_else(ApiRunId::new);
    let info = RunInfo {
        run_id: api_run_id,
        created_at: (m.created_at.max(0) as u64).saturating_mul(1000),
        metadata: m.metadata,
        state: convert_run_status(&m.status),
        error: m.error,
    };
    Versioned {
        value: info,
        version: Version::Txn(0),
        timestamp: strata_core::Timestamp::from_micros(m.created_at),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trait_is_object_safe() {
        fn _assert_object_safe(_: &dyn RunIndex) {}
    }
}
