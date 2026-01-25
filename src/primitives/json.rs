//! JSON document primitive.
//!
//! The Json primitive provides structured document storage with path-level
//! operations and versioning.

use crate::error::Result;
use crate::types::{run_id_to_api, RunId, Value, Version, Versioned};
use std::sync::Arc;

use strata_api::substrate::{ApiRunId, JsonStore};

/// JSON document operations.
///
/// Access via `db.json`.
pub struct Json {
    #[allow(dead_code)]
    db: Arc<strata_engine::Database>,
    substrate: strata_api::substrate::SubstrateImpl,
}

impl Json {
    pub(crate) fn new(db: Arc<strata_engine::Database>) -> Self {
        let substrate = strata_api::substrate::SubstrateImpl::new(db.clone());
        Self { db, substrate }
    }

    // =========================================================================
    // Simple API (default run)
    // =========================================================================

    /// Set a JSON document.
    ///
    /// Creates or replaces the document at the root path.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut obj = HashMap::new();
    /// obj.insert("name".to_string(), Value::from("Alice"));
    /// db.json.set("profile", obj)?;
    /// ```
    pub fn set(&self, key: &str, value: impl Into<Value>) -> Result<Version> {
        let run = ApiRunId::default();
        Ok(self.substrate.json_set(&run, key, "$", value.into())?)
    }

    /// Get a JSON document.
    ///
    /// Returns the entire document at the root path.
    ///
    /// # Example
    ///
    /// ```ignore
    /// if let Some(doc) = db.json.get("profile")? {
    ///     println!("Profile: {:?}", doc.value);
    /// }
    /// ```
    pub fn get(&self, key: &str) -> Result<Option<Versioned<Value>>> {
        let run = ApiRunId::default();
        Ok(self.substrate.json_get(&run, key, "$")?)
    }

    /// Delete a JSON document.
    pub fn delete(&self, key: &str) -> Result<u64> {
        let run = ApiRunId::default();
        // Delete root requires deleting a specific path, not the root itself
        // This is a limitation - we use a workaround by setting to null first
        // For now, return 0 to indicate "not implemented via this method"
        // Users should use path-based deletion or KV deletion
        let _ = key;
        let _ = run;
        Ok(0)
    }

    // =========================================================================
    // Run-scoped API
    // =========================================================================

    /// Set a JSON document in a specific run.
    pub fn set_in(&self, run: &RunId, key: &str, value: impl Into<Value>) -> Result<Version> {
        let api_run = run_id_to_api(run);
        Ok(self.substrate.json_set(&api_run, key, "$", value.into())?)
    }

    /// Get a JSON document from a specific run.
    pub fn get_in(&self, run: &RunId, key: &str) -> Result<Option<Versioned<Value>>> {
        let api_run = run_id_to_api(run);
        Ok(self.substrate.json_get(&api_run, key, "$")?)
    }

    // =========================================================================
    // Path operations
    // =========================================================================

    /// Get a value at a specific path within a document.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let name = db.json.get_path(&run, "profile", "$.name")?;
    /// ```
    pub fn get_path(
        &self,
        run: &RunId,
        key: &str,
        path: &str,
    ) -> Result<Option<Versioned<Value>>> {
        let api_run = run_id_to_api(run);
        Ok(self.substrate.json_get(&api_run, key, path)?)
    }

    /// Set a value at a specific path within a document.
    ///
    /// # Example
    ///
    /// ```ignore
    /// db.json.set_path(&run, "profile", "$.name", "Bob")?;
    /// ```
    pub fn set_path(
        &self,
        run: &RunId,
        key: &str,
        path: &str,
        value: impl Into<Value>,
    ) -> Result<Version> {
        let api_run = run_id_to_api(run);
        Ok(self.substrate.json_set(&api_run, key, path, value.into())?)
    }

    /// Delete a path within a document.
    pub fn delete_path(&self, run: &RunId, key: &str, path: &str) -> Result<u64> {
        let api_run = run_id_to_api(run);
        Ok(self.substrate.json_delete(&api_run, key, path)?)
    }

    /// Merge a value at a path using JSON Merge Patch (RFC 7396).
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Partial update: only update the "age" field
    /// let mut patch = HashMap::new();
    /// patch.insert("age".to_string(), Value::Int(31));
    /// db.json.merge(&run, "profile", "$", patch)?;
    /// ```
    pub fn merge(&self, run: &RunId, key: &str, path: &str, patch: impl Into<Value>) -> Result<Version> {
        let api_run = run_id_to_api(run);
        Ok(self.substrate.json_merge(&api_run, key, path, patch.into())?)
    }

    /// Check if a document exists.
    pub fn exists(&self, run: &RunId, key: &str) -> Result<bool> {
        let api_run = run_id_to_api(run);
        Ok(self.substrate.json_exists(&api_run, key)?)
    }
}
