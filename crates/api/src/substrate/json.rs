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

    /// Check if a document exists
    ///
    /// Returns `true` if a document with the given key exists.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Key is invalid
    /// - `NotFound`: Run does not exist
    fn json_exists(&self, run: &ApiRunId, key: &str) -> StrataResult<bool>;

    /// Get the current version of a document
    ///
    /// Returns the version number of the document, or `None` if not found.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Key is invalid
    /// - `NotFound`: Run does not exist
    fn json_get_version(&self, run: &ApiRunId, key: &str) -> StrataResult<Option<u64>>;

    /// Search JSON documents
    ///
    /// Performs full-text search across document values.
    ///
    /// ## Parameters
    ///
    /// - `query`: Search query string
    /// - `k`: Maximum results to return
    ///
    /// ## Return Value
    ///
    /// Search results with document keys and relevance scores.
    ///
    /// ## Errors
    ///
    /// - `NotFound`: Run does not exist
    fn json_search(
        &self,
        run: &ApiRunId,
        query: &str,
        k: u64,
    ) -> StrataResult<Vec<JsonSearchHit>>;

    /// List documents with cursor-based pagination
    ///
    /// Returns document keys matching the optional prefix filter.
    /// Supports cursor-based pagination for iterating through large result sets.
    ///
    /// ## Parameters
    ///
    /// - `prefix`: Optional key prefix filter
    /// - `cursor`: Resume from this cursor (returned from previous call)
    /// - `limit`: Maximum number of keys to return
    ///
    /// ## Return Value
    ///
    /// List result containing document keys and optional next cursor.
    ///
    /// ## Errors
    ///
    /// - `NotFound`: Run does not exist
    fn json_list(
        &self,
        run: &ApiRunId,
        prefix: Option<&str>,
        cursor: Option<&str>,
        limit: u64,
    ) -> StrataResult<JsonListResult>;

    /// Compare-and-swap: atomically update if version matches
    ///
    /// Provides optimistic concurrency control for JSON documents.
    /// The update only succeeds if the document's current version matches
    /// the expected version.
    ///
    /// ## Parameters
    ///
    /// - `key`: Document key
    /// - `expected_version`: Version the caller believes the document has
    /// - `path`: Path within the document to update
    /// - `value`: New value to set at path
    ///
    /// ## Return Value
    ///
    /// New document version if update succeeded.
    ///
    /// ## Errors
    ///
    /// - `VersionConflict`: Version didn't match (includes actual version)
    /// - `InvalidKey`: Key is invalid
    /// - `InvalidPath`: Path syntax error
    /// - `NotFound`: Document or run does not exist
    fn json_cas(
        &self,
        run: &ApiRunId,
        key: &str,
        expected_version: u64,
        path: &str,
        value: Value,
    ) -> StrataResult<Version>;

    /// Query documents by exact field match
    ///
    /// Finds documents where the value at the specified path exactly matches
    /// the given value. Unlike search (fuzzy text matching), query performs
    /// exact equality comparison.
    ///
    /// ## Parameters
    ///
    /// - `path`: Path within documents to compare
    /// - `value`: Value to match exactly
    /// - `limit`: Maximum number of results to return
    ///
    /// ## Return Value
    ///
    /// Document keys with matching value at path.
    ///
    /// ## Errors
    ///
    /// - `InvalidPath`: Path syntax error
    /// - `NotFound`: Run does not exist
    fn json_query(
        &self,
        run: &ApiRunId,
        path: &str,
        value: Value,
        limit: u64,
    ) -> StrataResult<Vec<String>>;

    /// Count documents in the store
    ///
    /// Returns the total number of JSON documents for a run.
    ///
    /// ## Return Value
    ///
    /// Number of documents.
    ///
    /// ## Errors
    ///
    /// - `NotFound`: Run does not exist
    fn json_count(&self, run: &ApiRunId) -> StrataResult<u64>;

    /// Batch get multiple documents
    ///
    /// Retrieves multiple documents in a single operation.
    /// More efficient than multiple individual get() calls.
    ///
    /// ## Parameters
    ///
    /// - `keys`: Document keys to retrieve
    ///
    /// ## Return Value
    ///
    /// Documents in same order as input keys.
    /// Returns `None` for documents that don't exist.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: One or more keys are invalid
    /// - `NotFound`: Run does not exist
    fn json_batch_get(
        &self,
        run: &ApiRunId,
        keys: &[&str],
    ) -> StrataResult<Vec<Option<Versioned<Value>>>>;

    /// Batch create multiple documents atomically
    ///
    /// Creates multiple documents in a single atomic transaction.
    /// If any document fails to create, the entire operation fails.
    ///
    /// ## Parameters
    ///
    /// - `docs`: Key and value pairs to create
    ///
    /// ## Return Value
    ///
    /// Versions of created documents in same order as input.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: One or more keys are invalid
    /// - `ConstraintViolation`: One or more documents already exist
    /// - `NotFound`: Run does not exist
    fn json_batch_create(
        &self,
        run: &ApiRunId,
        docs: Vec<(&str, Value)>,
    ) -> StrataResult<Vec<Version>>;

    /// Atomically push values to an array at path
    ///
    /// Appends one or more values to an array within a document.
    ///
    /// ## Parameters
    ///
    /// - `key`: Document key
    /// - `path`: Path to the array
    /// - `values`: Values to append
    ///
    /// ## Return Value
    ///
    /// New array length.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Key is invalid
    /// - `InvalidPath`: Path syntax error or doesn't point to an array
    /// - `NotFound`: Document or run does not exist
    fn json_array_push(
        &self,
        run: &ApiRunId,
        key: &str,
        path: &str,
        values: Vec<Value>,
    ) -> StrataResult<usize>;

    /// Atomically increment a numeric value at path
    ///
    /// Adds a delta to a numeric value within a document.
    ///
    /// ## Parameters
    ///
    /// - `key`: Document key
    /// - `path`: Path to the number
    /// - `delta`: Value to add (can be negative for decrement)
    ///
    /// ## Return Value
    ///
    /// New numeric value.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Key is invalid
    /// - `InvalidPath`: Path syntax error or doesn't point to a number
    /// - `NotFound`: Document or run does not exist
    fn json_increment(
        &self,
        run: &ApiRunId,
        key: &str,
        path: &str,
        delta: f64,
    ) -> StrataResult<f64>;

    /// Atomically pop a value from an array at path
    ///
    /// Removes and returns the last element from an array.
    ///
    /// ## Parameters
    ///
    /// - `key`: Document key
    /// - `path`: Path to the array
    ///
    /// ## Return Value
    ///
    /// The popped value, or None if array was empty.
    ///
    /// ## Errors
    ///
    /// - `InvalidKey`: Key is invalid
    /// - `InvalidPath`: Path syntax error or doesn't point to an array
    /// - `NotFound`: Document or run does not exist
    fn json_array_pop(
        &self,
        run: &ApiRunId,
        key: &str,
        path: &str,
    ) -> StrataResult<Option<Value>>;
}

/// A search hit in JSON document search
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct JsonSearchHit {
    /// Document key
    pub key: String,
    /// Relevance score (higher = more relevant)
    pub score: f32,
}

/// Result of listing JSON documents
#[derive(Debug, Clone, PartialEq)]
pub struct JsonListResult {
    /// Document keys returned
    pub keys: Vec<String>,
    /// Cursor for next page, if more results exist
    pub next_cursor: Option<String>,
}

// =============================================================================
// Implementation
// =============================================================================

use strata_core::json::JsonValue;
use super::impl_::{
    SubstrateImpl, convert_error,
    value_to_json, json_to_value, parse_doc_id, parse_path,
};

impl JsonStore for SubstrateImpl {
    fn json_set(
        &self,
        run: &ApiRunId,
        key: &str,
        path: &str,
        value: Value,
    ) -> StrataResult<Version> {
        let run_id = run.to_run_id();
        let doc_id = parse_doc_id(key)?;
        let json_path = parse_path(path)?;
        let json_value = value_to_json(value)?;

        // Check if document exists
        let exists = self.json().exists(&run_id, &doc_id).map_err(convert_error)?;

        if !exists && json_path.is_root() {
            // Create new document at root
            self.json().create(&run_id, &doc_id, json_value).map_err(convert_error)
        } else if !exists {
            // Document doesn't exist and trying to set non-root path - create with empty object first
            self.json().create(&run_id, &doc_id, JsonValue::object()).map_err(convert_error)?;
            self.json().set(&run_id, &doc_id, &json_path, json_value).map_err(convert_error)
        } else {
            // Document exists, update at path
            self.json().set(&run_id, &doc_id, &json_path, json_value).map_err(convert_error)
        }
    }

    fn json_get(
        &self,
        run: &ApiRunId,
        key: &str,
        path: &str,
    ) -> StrataResult<Option<Versioned<Value>>> {
        let run_id = run.to_run_id();
        let doc_id = parse_doc_id(key)?;
        let json_path = parse_path(path)?;

        let result = self.json().get(&run_id, &doc_id, &json_path).map_err(convert_error)?;

        match result {
            Some(versioned) => {
                let value = json_to_value(versioned.value)?;
                Ok(Some(Versioned {
                    value,
                    version: versioned.version,
                    timestamp: versioned.timestamp,
                }))
            }
            None => Ok(None),
        }
    }

    fn json_delete(&self, run: &ApiRunId, key: &str, path: &str) -> StrataResult<u64> {
        let run_id = run.to_run_id();
        let doc_id = parse_doc_id(key)?;
        let json_path = parse_path(path)?;

        if json_path.is_root() {
            // Delete entire document
            let existed = self.json().destroy(&run_id, &doc_id).map_err(convert_error)?;
            Ok(if existed { 1 } else { 0 })
        } else {
            // Delete at path
            self.json().delete_at_path(&run_id, &doc_id, &json_path).map_err(convert_error)?;
            Ok(1)
        }
    }

    fn json_merge(
        &self,
        run: &ApiRunId,
        key: &str,
        path: &str,
        patch: Value,
    ) -> StrataResult<Version> {
        // Delegate to primitive's atomic merge implementation
        let run_id = run.to_run_id();
        let doc_id = parse_doc_id(key)?;
        let json_path = parse_path(path)?;
        let patch_json = value_to_json(patch)?;

        self.json()
            .merge(&run_id, &doc_id, &json_path, patch_json)
            .map_err(convert_error)
    }

    fn json_history(
        &self,
        _run: &ApiRunId,
        _key: &str,
        _limit: Option<u64>,
        _before: Option<Version>,
    ) -> StrataResult<Vec<Versioned<Value>>> {
        // History not yet implemented
        Ok(vec![])
    }

    fn json_exists(&self, run: &ApiRunId, key: &str) -> StrataResult<bool> {
        let run_id = run.to_run_id();
        let doc_id = parse_doc_id(key)?;
        self.json().exists(&run_id, &doc_id).map_err(convert_error)
    }

    fn json_get_version(&self, run: &ApiRunId, key: &str) -> StrataResult<Option<u64>> {
        let run_id = run.to_run_id();
        let doc_id = parse_doc_id(key)?;
        self.json().get_version(&run_id, &doc_id).map_err(convert_error)
    }

    fn json_search(
        &self,
        run: &ApiRunId,
        query: &str,
        k: u64,
    ) -> StrataResult<Vec<JsonSearchHit>> {
        let run_id = run.to_run_id();
        let request = strata_core::SearchRequest::new(run_id, query).with_k(k as usize);
        let response = self.json().search(&request).map_err(convert_error)?;

        Ok(response.hits.into_iter().map(|hit| {
            let key = match hit.doc_ref {
                strata_core::search_types::DocRef::Json { doc_id, .. } => doc_id.to_string(),
                _ => String::new(),
            };
            JsonSearchHit {
                key,
                score: hit.score,
            }
        }).collect())
    }

    fn json_list(
        &self,
        run: &ApiRunId,
        prefix: Option<&str>,
        cursor: Option<&str>,
        limit: u64,
    ) -> StrataResult<JsonListResult> {
        let run_id = run.to_run_id();
        let result = self.json()
            .list(&run_id, prefix, cursor, limit as usize)
            .map_err(convert_error)?;

        Ok(JsonListResult {
            keys: result.doc_ids.into_iter().map(|id| id.to_string()).collect(),
            next_cursor: result.next_cursor,
        })
    }

    fn json_cas(
        &self,
        run: &ApiRunId,
        key: &str,
        expected_version: u64,
        path: &str,
        value: Value,
    ) -> StrataResult<Version> {
        let run_id = run.to_run_id();
        let doc_id = parse_doc_id(key)?;
        let json_path = parse_path(path)?;
        let json_value = value_to_json(value)?;

        self.json()
            .cas(&run_id, &doc_id, expected_version, &json_path, json_value)
            .map_err(convert_error)
    }

    fn json_query(
        &self,
        run: &ApiRunId,
        path: &str,
        value: Value,
        limit: u64,
    ) -> StrataResult<Vec<String>> {
        let run_id = run.to_run_id();
        let json_path = parse_path(path)?;
        let json_value = value_to_json(value)?;

        let doc_ids = self.json()
            .query(&run_id, &json_path, &json_value, limit as usize)
            .map_err(convert_error)?;

        Ok(doc_ids.into_iter().map(|id| id.to_string()).collect())
    }

    fn json_count(&self, run: &ApiRunId) -> StrataResult<u64> {
        let run_id = run.to_run_id();
        self.json().count(&run_id).map_err(convert_error)
    }

    fn json_batch_get(
        &self,
        run: &ApiRunId,
        keys: &[&str],
    ) -> StrataResult<Vec<Option<Versioned<Value>>>> {
        let run_id = run.to_run_id();

        // Parse all doc_ids first
        let doc_ids: Vec<_> = keys
            .iter()
            .map(|k| parse_doc_id(k))
            .collect::<Result<_, _>>()?;

        let results = self.json()
            .batch_get(&run_id, &doc_ids)
            .map_err(convert_error)?;

        // Convert JsonDoc to Value
        results
            .into_iter()
            .map(|opt| {
                opt.map(|versioned| {
                    let value = json_to_value(versioned.value.value)?;
                    Ok(Versioned {
                        value,
                        version: versioned.version,
                        timestamp: versioned.timestamp,
                    })
                })
                .transpose()
            })
            .collect()
    }

    fn json_batch_create(
        &self,
        run: &ApiRunId,
        docs: Vec<(&str, Value)>,
    ) -> StrataResult<Vec<Version>> {
        let run_id = run.to_run_id();

        // Parse keys and convert values
        let typed_docs: Vec<_> = docs
            .into_iter()
            .map(|(key, value)| {
                let doc_id = parse_doc_id(key)?;
                let json_value = value_to_json(value)?;
                Ok((doc_id, json_value))
            })
            .collect::<StrataResult<Vec<_>>>()?;

        self.json()
            .batch_create(&run_id, typed_docs)
            .map_err(convert_error)
    }

    fn json_array_push(
        &self,
        run: &ApiRunId,
        key: &str,
        path: &str,
        values: Vec<Value>,
    ) -> StrataResult<usize> {
        let run_id = run.to_run_id();
        let doc_id = parse_doc_id(key)?;
        let json_path = parse_path(path)?;

        let json_values: Vec<_> = values
            .into_iter()
            .map(value_to_json)
            .collect::<Result<_, _>>()?;

        let (_version, len) = self.json()
            .array_push(&run_id, &doc_id, &json_path, json_values)
            .map_err(convert_error)?;

        Ok(len)
    }

    fn json_increment(
        &self,
        run: &ApiRunId,
        key: &str,
        path: &str,
        delta: f64,
    ) -> StrataResult<f64> {
        let run_id = run.to_run_id();
        let doc_id = parse_doc_id(key)?;
        let json_path = parse_path(path)?;

        let (_version, new_val) = self.json()
            .increment(&run_id, &doc_id, &json_path, delta)
            .map_err(convert_error)?;

        Ok(new_val)
    }

    fn json_array_pop(
        &self,
        run: &ApiRunId,
        key: &str,
        path: &str,
    ) -> StrataResult<Option<Value>> {
        let run_id = run.to_run_id();
        let doc_id = parse_doc_id(key)?;
        let json_path = parse_path(path)?;

        let (_version, popped) = self.json()
            .array_pop(&run_id, &doc_id, &json_path)
            .map_err(convert_error)?;

        match popped {
            Some(json_val) => Ok(Some(json_to_value(json_val)?)),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trait_is_object_safe() {
        fn _assert_object_safe(_: &dyn JsonStore) {}
    }
}
