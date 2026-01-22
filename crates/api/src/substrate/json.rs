//! JsonStore Substrate Operations
//!
//! The JsonStore provides structured JSON document storage with path-based access.
//! It enables partial updates and queries without reading/writing entire documents.
//!
//! ## Document Model
//!
//! - Documents are `Value::Object` at the root
//! - Paths use JSONPath-style syntax: `$.a.b[0].c`
//! - Paths can traverse objects (`.field`) and arrays (`[index]`)
//!
//! ## Path Syntax
//!
//! - `$` - Root (entire document)
//! - `$.field` - Object field access
//! - `$.array[0]` - Array index access
//! - `$.array[-]` - Array append (for `json_set` only)
//!
//! ## Versioning
//!
//! JSON documents use transaction-based versioning (`Version::Txn`).
//! Each document has a single version - subpaths don't have independent versions.

use super::types::ApiRunId;
use strata_core::{StrataResult, Value, Version, Versioned};

/// JsonStore substrate operations
///
/// This trait defines the canonical JSON document store operations.
/// All operations require explicit run_id and return versioned results.
///
/// ## Contract
///
/// - Documents must have `Value::Object` at the root
/// - Path syntax follows JSONPath conventions
/// - Version applies to entire document, not individual paths
///
/// ## Error Handling
///
/// | Condition | Error |
/// |-----------|-------|
/// | Invalid key | `InvalidKey` |
/// | Invalid path syntax | `InvalidPath` |
/// | Path targets non-existent intermediate | `InvalidPath` |
/// | Root set to non-Object | `ConstraintViolation` |
/// | Run not found | `NotFound` |
/// | Run is closed | `ConstraintViolation` |
pub trait JsonStore {
    /// Set a value at a path
    ///
    /// Creates or updates the value at the specified path.
    /// Returns the new document version.
    ///
    /// ## Semantics
    ///
    /// - If key doesn't exist, creates a new document with the path
    /// - If path doesn't exist, creates intermediate objects/arrays
    /// - If path exists, replaces the value
    ///
    /// ## Path Rules
    ///
    /// - `$` replaces entire document (must be Object)
    /// - `$.field` sets object field
    /// - `$.array[0]` sets array element at index
    /// - `$.array[-]` appends to array
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Key is invalid
    /// - `InvalidPath`: Path syntax error or targets impossible location
    /// - `ConstraintViolation`: Root set to non-Object, or run is closed
    /// - `NotFound`: Run does not exist
    fn json_set(
        &self,
        run: &ApiRunId,
        key: &str,
        path: &str,
        value: Value,
    ) -> StrataResult<Version>;

    /// Get a value at a path
    ///
    /// Returns the value at the specified path, or `None` if not found.
    ///
    /// ## Return Value
    ///
    /// - `Some(Versioned<Value>)`: Path exists, returns value with document version
    /// - `None`: Key doesn't exist or path doesn't exist in document
    ///
    /// ## Version Semantics
    ///
    /// The returned version is the **document-level version**, not the version
    /// when the specific path was last modified. Documents don't track per-path versions.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Key is invalid
    /// - `InvalidPath`: Path syntax error
    /// - `NotFound`: Run does not exist
    fn json_get(
        &self,
        run: &ApiRunId,
        key: &str,
        path: &str,
    ) -> StrataResult<Option<Versioned<Value>>>;

    /// Delete a value at a path
    ///
    /// Removes the value at the specified path.
    /// Returns the count of elements removed (0 or 1, or more for array wildcards).
    ///
    /// ## Semantics
    ///
    /// - For object fields: Removes the field entirely
    /// - For array elements: Removes and shifts subsequent elements
    /// - Deleting `$` (root) is **forbidden** - use regular key deletion
    ///
    /// ## Return Value
    ///
    /// Count of elements removed (typically 0 or 1).
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Key is invalid
    /// - `InvalidPath`: Path syntax error, or attempting to delete root
    /// - `NotFound`: Run does not exist
    /// - `ConstraintViolation`: Run is closed
    fn json_delete(&self, run: &ApiRunId, key: &str, path: &str) -> StrataResult<u64>;

    /// Merge a value at a path (RFC 7396)
    ///
    /// Applies JSON Merge Patch semantics to the value at the path.
    /// Returns the new document version.
    ///
    /// ## RFC 7396 Semantics
    ///
    /// - `null` in patch deletes the corresponding field
    /// - Objects merge recursively (patch keys override target keys)
    /// - Arrays replace entirely (no array merging)
    /// - Scalars replace the target value
    ///
    /// ## Examples
    ///
    /// ```text
    /// Target: {"a": 1, "b": 2}
    /// Patch:  {"b": null, "c": 3}
    /// Result: {"a": 1, "c": 3}
    /// ```
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Key is invalid
    /// - `InvalidPath`: Path syntax error
    /// - `NotFound`: Run does not exist
    /// - `ConstraintViolation`: Run is closed, or result root is not Object
    fn json_merge(
        &self,
        run: &ApiRunId,
        key: &str,
        path: &str,
        patch: Value,
    ) -> StrataResult<Version>;

    /// Get version history for a document
    ///
    /// Returns historical versions of the entire document, newest first.
    ///
    /// ## Parameters
    ///
    /// - `limit`: Maximum number of versions to return
    /// - `before`: Return versions older than this (exclusive)
    ///
    /// ## Note
    ///
    /// This returns the **document-level history**, not path-level history.
    /// There is no per-path history tracking.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Key is invalid
    /// - `NotFound`: Run does not exist
    fn json_history(
        &self,
        run: &ApiRunId,
        key: &str,
        limit: Option<u64>,
        before: Option<Version>,
    ) -> StrataResult<Vec<Versioned<Value>>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trait_is_object_safe() {
        fn _assert_object_safe(_: &dyn JsonStore) {}
    }
}
