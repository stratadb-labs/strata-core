//! JSON document store operations (MVP).
//!
//! Provides document storage with path-based access.
//!
//! # Example
//!
//! ```ignore
//! use strata_executor::Strata;
//!
//! let db = Strata::open("/path/to/data")?;
//!
//! // Create/update a document
//! db.json_set("user:123", "$", json!({"name": "Alice", "age": 30}))?;
//!
//! // Get a value at a path
//! let name = db.json_get("user:123", "$.name")?;
//!
//! // Delete a document
//! db.json_delete("user:123", "$")?;
//!
//! // List documents
//! let (keys, cursor) = db.json_list(Some("user:".into()), None, 100)?;
//! ```

use super::Strata;
use crate::{Command, Error, Output, Result, Value};

impl Strata {
    // =========================================================================
    // JSON Operations (4 MVP)
    // =========================================================================

    /// Set a JSON value at a path.
    ///
    /// Creates the document if it doesn't exist, or updates the value at the
    /// specified path. Use "$" as the path for the root document.
    ///
    /// # Arguments
    ///
    /// * `key` - Document identifier
    /// * `path` - JSONPath to the value (use "$" for root)
    /// * `value` - Value to set
    ///
    /// # Returns
    ///
    /// The new version number.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Create a new document
    /// db.json_set("config", "$", json!({"debug": true}))?;
    ///
    /// // Update a nested path
    /// db.json_set("config", "$.debug", false)?;
    /// ```
    pub fn json_set(&self, key: &str, path: &str, value: impl Into<Value>) -> Result<u64> {
        match self.executor.execute(Command::JsonSet {
            run: self.branch_id(),
            key: key.to_string(),
            path: path.to_string(),
            value: value.into(),
        })? {
            Output::Version(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonSet".into(),
            }),
        }
    }

    /// Get a JSON value at a path.
    ///
    /// # Arguments
    ///
    /// * `key` - Document identifier
    /// * `path` - JSONPath to the value (use "$" for root)
    ///
    /// # Returns
    ///
    /// The versioned value if found, None otherwise.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Get the whole document
    /// let doc = db.json_get("config", "$")?;
    ///
    /// // Get a nested value
    /// let debug = db.json_get("config", "$.debug")?;
    /// ```
    pub fn json_get(&self, key: &str, path: &str) -> Result<Option<Value>> {
        match self.executor.execute(Command::JsonGet {
            run: self.branch_id(),
            key: key.to_string(),
            path: path.to_string(),
        })? {
            Output::Maybe(v) => Ok(v),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonGet".into(),
            }),
        }
    }

    /// Delete a JSON document or value at a path.
    ///
    /// Use "$" as the path to delete the entire document.
    ///
    /// # Arguments
    ///
    /// * `key` - Document identifier
    /// * `path` - JSONPath to delete (use "$" for whole document)
    ///
    /// # Returns
    ///
    /// The new version number (0 if document was deleted entirely).
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Delete a nested value
    /// db.json_delete("config", "$.deprecated_field")?;
    ///
    /// // Delete entire document
    /// db.json_delete("config", "$")?;
    /// ```
    pub fn json_delete(&self, key: &str, path: &str) -> Result<u64> {
        match self.executor.execute(Command::JsonDelete {
            run: self.branch_id(),
            key: key.to_string(),
            path: path.to_string(),
        })? {
            Output::Uint(count) => Ok(count),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonDelete".into(),
            }),
        }
    }

    /// List JSON documents with cursor-based pagination.
    ///
    /// # Arguments
    ///
    /// * `prefix` - Optional key prefix filter
    /// * `cursor` - Optional cursor for pagination (from previous call)
    /// * `limit` - Maximum number of keys to return
    ///
    /// # Returns
    ///
    /// Tuple of (keys, next_cursor). If next_cursor is Some, there are more results.
    ///
    /// # Example
    ///
    /// ```ignore
    /// // List all documents with prefix
    /// let (keys, cursor) = db.json_list(Some("user:".into()), None, 100)?;
    ///
    /// // Get next page if there are more
    /// if let Some(c) = cursor {
    ///     let (more_keys, _) = db.json_list(Some("user:".into()), Some(c), 100)?;
    /// }
    /// ```
    pub fn json_list(
        &self,
        prefix: Option<String>,
        cursor: Option<String>,
        limit: u64,
    ) -> Result<(Vec<String>, Option<String>)> {
        match self.executor.execute(Command::JsonList {
            run: self.branch_id(),
            prefix,
            cursor,
            limit,
        })? {
            Output::JsonListResult { keys, cursor } => Ok((keys, cursor)),
            _ => Err(Error::Internal {
                reason: "Unexpected output for JsonList".into(),
            }),
        }
    }
}
