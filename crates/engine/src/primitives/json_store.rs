//! JsonStore: JSON document storage primitive
//!
//! ## Design: STATELESS FACADE
//!
//! JsonStore holds ONLY `Arc<Database>`. No internal state, no caches,
//! no maps, no locks. All data lives in ShardedStore via Key::new_json().
//!
//! ## Run Isolation
//!
//! All operations are scoped to a run_id. Keys are prefixed with the
//! run's namespace, ensuring complete isolation between runs.
//!
//! ## Thread Safety
//!
//! JsonStore is `Send + Sync` and can be safely shared across threads.
//! Multiple JsonStore instances on the same Database are safe.
//!
//! ## API
//!
//! - **Single-Operation API**: `get`, `create`, `set`, `delete_at_path`, `destroy`
//!   Each operation runs in its own implicit transaction.
//!
//! - **Fast Path Reads**: `get`, `exists`, `get_doc`
//!   Use SnapshotView directly for read-only access.
//!
//! ## Architectural Rules
//!
//! This implementation follows the architectural rules:
//! 1. JSON lives in ShardedStore via Key::new_json()
//! 2. JsonStore is stateless (Arc<Database> only)
//! 3. JSON extends TransactionContext (no separate type)
//! 4. Path semantics in API layer (not storage)
//! 5. WAL remains unified (entry types 0x20-0x23)
//! 6. JSON API feels like other primitives

use crate::primitives::extensions::JsonStoreExt;
use strata_concurrency::TransactionContext;
use strata_core::contract::{Timestamp, Version, Versioned};
use strata_core::error::Result;
use strata_core::primitives::json::{delete_at_path, get_at_path, set_at_path, JsonLimitError, JsonPath, JsonValue};
use strata_core::StrataError;
use strata_core::traits::SnapshotView;
use strata_core::types::{Key, Namespace, RunId};
use strata_core::value::Value;
use crate::database::Database;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::SystemTime;

// =============================================================================
// Limit Validation Helpers
// =============================================================================

/// Convert a JsonLimitError to a StrataError
fn limit_error_to_error(e: JsonLimitError) -> StrataError {
    StrataError::invalid_input(e.to_string())
}

// =============================================================================
// JsonDoc - Internal Document Representation
// =============================================================================

/// Internal representation of a JSON document
///
/// Stored as serialized bytes in ShardedStore.
/// Version is used for optimistic concurrency control.
///
/// # Design
///
/// - **Document-level versioning**: Single version for entire document
/// - **Timestamps**: Track creation and modification times
/// - **Serializable**: Uses MessagePack for efficient storage
///
/// # Example
///
/// ```rust
/// use strata_primitives::json_store::JsonDoc;
/// use strata_core::primitives::json::JsonValue;
///
/// let doc = JsonDoc::new("my-document", JsonValue::from(42i64));
/// assert_eq!(doc.version, 1);
/// assert_eq!(doc.id, "my-document");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonDoc {
    /// Document unique identifier (user-provided string key)
    pub id: String,
    /// The JSON value (root of document)
    pub value: JsonValue,
    /// Document version (increments on any change)
    pub version: u64,
    /// Creation timestamp (microseconds since epoch)
    pub created_at: u64,
    /// Last modification timestamp (microseconds since epoch)
    pub updated_at: u64,
}

/// Result of listing JSON documents
///
/// Contains document IDs and an optional cursor for pagination.
#[derive(Debug, Clone, PartialEq)]
pub struct JsonListResult {
    /// Document IDs returned (user-provided string keys)
    pub doc_ids: Vec<String>,
    /// Cursor for next page, if more results exist
    pub next_cursor: Option<String>,
}

impl JsonDoc {
    /// Create a new document with initial value
    ///
    /// Initializes version to 1 and sets timestamps to current time.
    pub fn new(id: impl Into<String>, value: JsonValue) -> Self {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        JsonDoc {
            id: id.into(),
            value,
            version: 1,
            created_at: now,
            updated_at: now,
        }
    }

    /// Increment version and update timestamp
    ///
    /// Call this after any modification to the document.
    pub fn touch(&mut self) {
        self.version += 1;
        self.updated_at = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;
    }
}

/// JSON document storage primitive
///
/// STATELESS FACADE over Database - all state lives in unified ShardedStore.
/// Multiple JsonStore instances on same Database are safe.
///
/// # Design
///
/// JsonStore does NOT own storage. It is a facade that:
/// - Uses `Arc<Database>` for all operations
/// - Stores documents via `Key::new_json()` in ShardedStore
/// - Uses SnapshotView for fast path reads
/// - Participates in cross-primitive transactions
///
/// # Example
///
/// ```ignore
/// use strata_primitives::JsonStore;
/// use crate::database::Database;
/// use strata_core::types::RunId;
/// use strata_core::primitives::json::JsonValue;
///
/// let db = Arc::new(Database::builder().in_memory().open_temp()?);
/// let json = JsonStore::new(db);
/// let run_id = RunId::new();
///
/// // Create and read document
/// json.create(&run_id, "my-doc", JsonValue::object())?;
/// let value = json.get(&run_id, "my-doc", &JsonPath::root())?;
/// ```
#[derive(Clone)]
pub struct JsonStore {
    db: Arc<Database>, // ONLY state: reference to database
}

impl JsonStore {
    /// Create new JsonStore instance
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Get the underlying database reference
    pub fn database(&self) -> &Arc<Database> {
        &self.db
    }

    /// Build namespace for run-scoped operations
    fn namespace_for_run(&self, run_id: &RunId) -> Namespace {
        Namespace::for_run(*run_id)
    }

    /// Build key for JSON document
    fn key_for(&self, run_id: &RunId, doc_id: &str) -> Key {
        Key::new_json(self.namespace_for_run(run_id), doc_id)
    }

    // ========================================================================
    // Serialization
    // ========================================================================

    /// Serialize document for storage
    ///
    /// Uses MessagePack for efficient binary serialization.
    pub(crate) fn serialize_doc(doc: &JsonDoc) -> Result<Value> {
        let bytes = rmp_serde::to_vec(doc).map_err(|e| StrataError::serialization(e.to_string()))?;
        Ok(Value::Bytes(bytes))
    }

    /// Deserialize document from storage
    ///
    /// Expects Value::Bytes containing MessagePack-encoded JsonDoc.
    pub(crate) fn deserialize_doc(value: &Value) -> Result<JsonDoc> {
        match value {
            Value::Bytes(bytes) => {
                rmp_serde::from_slice(bytes).map_err(|e| StrataError::serialization(e.to_string()))
            }
            _ => Err(StrataError::invalid_input("expected bytes for JsonDoc")),
        }
    }

    // ========================================================================
    // Document Operations
    // ========================================================================

    /// Create a new JSON document
    ///
    /// Creates a new document with version 1. Fails if a document with
    /// the same ID already exists.
    ///
    /// # Arguments
    ///
    /// * `run_id` - RunId for namespace isolation
    /// * `doc_id` - Unique document identifier
    /// * `value` - Initial JSON value for the document
    ///
    /// # Returns
    ///
    /// * `Ok(Version)` - Document created with version
    /// * `Err(InvalidOperation)` - Document already exists
    ///
    /// # Example
    ///
    /// ```ignore
    /// let version = json.create(&run_id, &doc_id, JsonValue::object())?;
    /// assert_eq!(version, Version::counter(1));
    /// ```
    pub fn create(&self, run_id: &RunId, doc_id: &str, value: JsonValue) -> Result<Version> {
        // Validate document limits (Issue #440)
        value.validate().map_err(limit_error_to_error)?;

        let key = self.key_for(run_id, doc_id);
        let doc = JsonDoc::new(doc_id, value);

        self.db.transaction(*run_id, |txn| {
            // Check if document already exists
            if txn.get(&key)?.is_some() {
                return Err(StrataError::invalid_input(format!(
                    "JSON document {} already exists",
                    doc_id
                )));
            }

            let serialized = Self::serialize_doc(&doc)?;
            txn.put(key.clone(), serialized)?;
            Ok(Version::counter(doc.version))
        })
    }

    // ========================================================================
    // Fast Path Reads
    // ========================================================================

    /// Get value at path in a document (FAST PATH)
    ///
    /// Uses SnapshotView directly for read-only access.
    /// Bypasses full transaction overhead:
    /// - Direct snapshot read
    /// - No transaction object allocation
    /// - No read-set recording
    /// - No commit validation
    ///
    /// # Arguments
    ///
    /// * `run_id` - RunId for namespace isolation
    /// * `doc_id` - Document to read from
    /// * `path` - Path within the document (use JsonPath::root() for whole doc)
    ///
    /// # Returns
    ///
    /// * `Ok(Some(Versioned<value>))` - Value at path with version info
    /// * `Ok(None)` - Document doesn't exist or path not found
    /// * `Err` - On deserialization error
    pub fn get(
        &self,
        run_id: &RunId,
        doc_id: &str,
        path: &JsonPath,
    ) -> Result<Option<Versioned<JsonValue>>> {
        // Validate path limits (Issue #440)
        path.validate().map_err(limit_error_to_error)?;

        let snapshot = self.db.storage().create_snapshot();
        let key = self.key_for(run_id, doc_id);

        match snapshot.get(&key)? {
            Some(vv) => {
                let doc = Self::deserialize_doc(&vv.value)?;
                match get_at_path(&doc.value, path).cloned() {
                    Some(value) => Ok(Some(Versioned::with_timestamp(
                        value,
                        Version::counter(doc.version),
                        Timestamp::from_micros(doc.updated_at),
                    ))),
                    None => Ok(None),
                }
            }
            None => Ok(None),
        }
    }

    /// Get the full document (FAST PATH)
    ///
    /// Returns the entire JsonDoc including metadata (version, timestamps).
    pub fn get_doc(&self, run_id: &RunId, doc_id: &str) -> Result<Option<Versioned<JsonDoc>>> {
        let snapshot = self.db.storage().create_snapshot();
        let key = self.key_for(run_id, doc_id);

        match snapshot.get(&key)? {
            Some(vv) => {
                let doc = Self::deserialize_doc(&vv.value)?;
                let versioned = Versioned::with_timestamp(
                    doc.clone(),
                    Version::counter(doc.version),
                    Timestamp::from_micros(doc.updated_at),
                );
                Ok(Some(versioned))
            }
            None => Ok(None),
        }
    }

    /// Get document version (FAST PATH)
    ///
    /// Efficient way to check document version without full deserialization.
    /// (In practice, we deserialize but could optimize later)
    pub fn get_version(&self, run_id: &RunId, doc_id: &str) -> Result<Option<u64>> {
        let snapshot = self.db.storage().create_snapshot();
        let key = self.key_for(run_id, doc_id);

        match snapshot.get(&key)? {
            Some(vv) => {
                let doc = Self::deserialize_doc(&vv.value)?;
                Ok(Some(doc.version))
            }
            None => Ok(None),
        }
    }

    /// Check if document exists (FAST PATH)
    ///
    /// Fastest way to check document existence.
    pub fn exists(&self, run_id: &RunId, doc_id: &str) -> Result<bool> {
        let snapshot = self.db.storage().create_snapshot();
        let key = self.key_for(run_id, doc_id);
        Ok(snapshot.get(&key)?.is_some())
    }

    /// Get document version history
    ///
    /// Returns full document snapshots in descending version order (newest first).
    ///
    /// **Important**: This returns value-history, not transition-history. Each entry
    /// is a complete document snapshot, not a diff or operation log.
    ///
    /// ## Parameters
    ///
    /// * `run_id` - RunId for namespace isolation
    /// * `doc_id` - Document identifier
    /// * `limit` - Maximum versions to return (None = all)
    /// * `before_version` - Only return versions older than this document version (for pagination)
    ///
    /// ## Returns
    ///
    /// Vector of `Versioned<JsonDoc>` in descending version order (newest first).
    /// Empty if document doesn't exist or has no history.
    ///
    /// ## Ordering Guarantee
    ///
    /// Results are guaranteed to be in descending document version order.
    /// This invariant is enforced by the storage layer's `get_history()` contract.
    ///
    /// ## Deletion Semantics
    ///
    /// History survives document deletion. If a document is deleted:
    /// - Previous versions remain accessible via `history()`
    /// - The deleted state may appear as a tombstone entry (filtered at substrate layer)
    /// - This matches Strata's "execution commit" philosophy where all committed state is preserved
    ///
    /// ## Version Semantics
    ///
    /// The `before_version` parameter filters by **document version** (`JsonDoc.version`),
    /// not by storage transaction version. This matches StateCell semantics.
    ///
    /// ## Storage Behavior
    ///
    /// - **ShardedStore** (persistent): Returns full version history from VersionChain
    /// - **UnifiedStore** (in-memory): Returns only current version (no history retention)
    ///
    /// ## Example
    ///
    /// ```ignore
    /// // Get last 10 versions
    /// let history = json.history(&run_id, &doc_id, Some(10), None)?;
    ///
    /// // Paginate: get next 10 versions older than version 50
    /// let page2 = json.history(&run_id, &doc_id, Some(10), Some(50))?;
    /// ```
    pub fn history(
        &self,
        run_id: &RunId,
        doc_id: &str,
        limit: Option<usize>,
        before_version: Option<u64>,
    ) -> Result<Vec<Versioned<JsonDoc>>> {
        use strata_core::traits::Storage;

        let key = self.key_for(run_id, doc_id);

        // Optimization: When before_version is None, we can pass limit directly to storage
        // to avoid unbounded reads. When before_version is Some, we must fetch more and
        // filter by document version (which differs from storage transaction version).
        let storage_limit = if before_version.is_none() { limit } else { None };

        let raw_history = self.db.storage().get_history(&key, storage_limit, None)?;

        // Storage layer contract: get_history() returns newest-first.
        // If this invariant ever changes, json_history semantics must be updated.
        debug_assert!(
            raw_history.windows(2).all(|w| w[0].version >= w[1].version),
            "Storage::get_history() must return results in descending version order"
        );

        let mut results: Vec<Versioned<JsonDoc>> = Vec::new();

        for versioned_value in raw_history {
            // Deserialize the JsonDoc from storage
            let doc = match Self::deserialize_doc(&versioned_value.value) {
                Ok(d) => d,
                Err(_) => continue, // Skip malformed entries
            };

            // Apply before_version filter (based on document's internal version)
            if let Some(before) = before_version {
                if doc.version >= before {
                    continue;
                }
            }

            // Build result with document's internal version.
            // Use STORAGE timestamp for consistency with KV and StateCell.
            // (JsonDoc.updated_at is document-level, but we want commit-time consistency)
            results.push(Versioned::with_timestamp(
                doc.clone(),
                Version::counter(doc.version),
                versioned_value.timestamp,
            ));

            // Apply limit
            if let Some(max) = limit {
                if results.len() >= max {
                    break;
                }
            }
        }

        Ok(results)
    }

    // ========================================================================
    // Mutations
    // ========================================================================

    /// Set value at path in a document
    ///
    /// Uses transaction for atomic read-modify-write.
    /// Increments document version on success.
    ///
    /// # Arguments
    ///
    /// * `run_id` - RunId for namespace isolation
    /// * `doc_id` - Document to modify
    /// * `path` - Path to set value at (creates intermediate objects/arrays)
    /// * `value` - New value to set
    ///
    /// # Returns
    ///
    /// * `Ok(Version)` - New document version after modification
    /// * `Err(InvalidOperation)` - Document doesn't exist
    /// * `Err` - On path error or serialization error
    pub fn set(
        &self,
        run_id: &RunId,
        doc_id: &str,
        path: &JsonPath,
        value: JsonValue,
    ) -> Result<Version> {
        // Validate path and value limits (Issue #440)
        path.validate().map_err(limit_error_to_error)?;
        value.validate().map_err(limit_error_to_error)?;

        let key = self.key_for(run_id, doc_id);

        self.db.transaction(*run_id, |txn| {
            // Load existing document
            let stored = txn.get(&key)?.ok_or_else(|| {
                StrataError::invalid_input(format!("JSON document {} not found", doc_id))
            })?;
            let mut doc = Self::deserialize_doc(&stored)?;

            // Apply mutation
            set_at_path(&mut doc.value, path, value)
                .map_err(|e| StrataError::invalid_input(format!("Path error: {}", e)))?;
            doc.touch();

            // Store updated document
            let serialized = Self::serialize_doc(&doc)?;
            txn.put(key.clone(), serialized)?;

            Ok(Version::counter(doc.version))
        })
    }

    /// Delete value at path in a document
    ///
    /// Uses transaction for atomic read-modify-write.
    /// Increments document version on success.
    ///
    /// # Arguments
    ///
    /// * `run_id` - RunId for namespace isolation
    /// * `doc_id` - Document to modify
    /// * `path` - Path to delete (must not be root)
    ///
    /// # Returns
    ///
    /// * `Ok(Version)` - New document version after deletion
    /// * `Err(InvalidOperation)` - Document doesn't exist or path error
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Remove a field from an object
    /// json.delete_at_path(&run_id, &doc_id, &"user.temp".parse().unwrap())?;
    /// ```
    pub fn delete_at_path(
        &self,
        run_id: &RunId,
        doc_id: &str,
        path: &JsonPath,
    ) -> Result<Version> {
        // Validate path limits (Issue #440)
        path.validate().map_err(limit_error_to_error)?;

        let key = self.key_for(run_id, doc_id);

        self.db.transaction(*run_id, |txn| {
            // Load existing document
            let stored = txn.get(&key)?.ok_or_else(|| {
                StrataError::invalid_input(format!("JSON document {} not found", doc_id))
            })?;
            let mut doc = Self::deserialize_doc(&stored)?;

            // Apply deletion
            delete_at_path(&mut doc.value, path)
                .map_err(|e| StrataError::invalid_input(format!("Path error: {}", e)))?;
            doc.touch();

            // Store updated document
            let serialized = Self::serialize_doc(&doc)?;
            txn.put(key.clone(), serialized)?;

            Ok(Version::counter(doc.version))
        })
    }

    /// Destroy (delete) an entire document
    ///
    /// Removes the document from storage. This operation is final.
    ///
    /// # Arguments
    ///
    /// * `run_id` - RunId for namespace isolation
    /// * `doc_id` - Document to destroy
    ///
    /// # Returns
    ///
    /// * `Ok(true)` - Document existed and was destroyed
    /// * `Ok(false)` - Document did not exist
    ///
    /// # Example
    ///
    /// ```ignore
    /// let existed = json.destroy(&run_id, &doc_id)?;
    /// assert!(existed);
    /// ```
    pub fn destroy(&self, run_id: &RunId, doc_id: &str) -> Result<bool> {
        let key = self.key_for(run_id, doc_id);

        self.db.transaction(*run_id, |txn| {
            // Check if document exists
            if txn.get(&key)?.is_none() {
                return Ok(false);
            }

            // Delete the document
            txn.delete(key.clone())?;
            Ok(true)
        })
    }

    // ========================================================================
    // Atomic Merge
    // ========================================================================

    /// Atomically merge a patch into a document using RFC 7396 JSON Merge Patch
    ///
    /// This operation is atomic - the read, merge, and write happen within
    /// a single transaction, preventing race conditions with concurrent updates.
    ///
    /// If the document doesn't exist, it will be created with the patch value.
    ///
    /// # Arguments
    ///
    /// * `run_id` - RunId for namespace isolation
    /// * `doc_id` - Document to merge into
    /// * `path` - Path within the document to merge at (use JsonPath::root() for whole doc)
    /// * `patch` - The patch to apply (RFC 7396 merge patch semantics)
    ///
    /// # Returns
    ///
    /// * `Ok(Version)` - New document version after merge
    /// * `Err` - On path error or serialization error
    ///
    /// # RFC 7396 Semantics
    ///
    /// - If patch is an object, recursively merge each key
    /// - If a key's value is null, remove that key from target
    /// - If patch is not an object, it replaces the target entirely
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Initial document: {"name": "Alice", "age": 30}
    /// // Patch: {"age": 31, "city": "NYC"}
    /// // Result: {"name": "Alice", "age": 31, "city": "NYC"}
    ///
    /// let version = json.merge(&run_id, &doc_id, &JsonPath::root(), patch)?;
    /// ```
    pub fn merge(
        &self,
        run_id: &RunId,
        doc_id: &str,
        path: &JsonPath,
        patch: JsonValue,
    ) -> Result<Version> {
        use strata_core::primitives::json::merge_patch;

        // Validate path and patch limits
        path.validate().map_err(limit_error_to_error)?;
        patch.validate().map_err(limit_error_to_error)?;

        let key = self.key_for(run_id, doc_id);

        self.db.transaction(*run_id, |txn| {
            // Load existing document or create new one
            let mut doc = match txn.get(&key)? {
                Some(stored) => Self::deserialize_doc(&stored)?,
                None => {
                    // Document doesn't exist - create it
                    if path.is_root() {
                        // At root, just create with patch value
                        let doc = JsonDoc::new(doc_id, patch.clone());
                        let serialized = Self::serialize_doc(&doc)?;
                        txn.put(key.clone(), serialized)?;
                        return Ok(Version::counter(doc.version));
                    } else {
                        // At path, create empty object first
                        JsonDoc::new(doc_id, JsonValue::object())
                    }
                }
            };

            // Get current value at path (or create if path doesn't exist yet)
            if path.is_root() {
                // Merge at root
                merge_patch(&mut doc.value, &patch);
            } else {
                // Get or create value at path
                match get_at_path(&doc.value, path).cloned() {
                    Some(mut current) => {
                        // Merge into existing value
                        merge_patch(&mut current, &patch);
                        set_at_path(&mut doc.value, path, current)
                            .map_err(|e| StrataError::invalid_input(format!("Path error: {}", e)))?;
                    }
                    None => {
                        // Path doesn't exist - set patch value directly
                        set_at_path(&mut doc.value, path, patch)
                            .map_err(|e| StrataError::invalid_input(format!("Path error: {}", e)))?;
                    }
                }
            }

            doc.touch();

            // Store updated document
            let serialized = Self::serialize_doc(&doc)?;
            txn.put(key.clone(), serialized)?;

            Ok(Version::counter(doc.version))
        })
    }

    // ========================================================================
    // Compare-and-Swap
    // ========================================================================

    /// Compare-and-swap: atomically update if version matches
    ///
    /// This operation provides optimistic concurrency control. It reads the
    /// document, checks that the version matches the expected version, and
    /// only then applies the update.
    ///
    /// # Arguments
    ///
    /// * `run_id` - RunId for namespace isolation
    /// * `doc_id` - Document to update
    /// * `expected_version` - The version the caller believes the document has
    /// * `path` - Path within the document to update
    /// * `value` - New value to set at path
    ///
    /// # Returns
    ///
    /// * `Ok(Version)` - Update succeeded, returns new version
    /// * `Err(VersionMismatch)` - Version didn't match
    /// * `Err(InvalidOperation)` - Document doesn't exist
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Read document and its version
    /// let versioned = json.get(&run_id, &doc_id, &JsonPath::root())?;
    /// let version = versioned.version; // e.g., 5
    ///
    /// // Try to update only if version is still 5
    /// match json.cas(&run_id, &doc_id, 5, &"name".parse()?, new_value) {
    ///     Ok(new_ver) => println!("Updated to version {}", new_ver),
    ///     Err(StrataError::Conflict { reason, .. }) => {
    ///         println!("Conflict! {}", reason);
    ///     }
    ///     Err(e) => return Err(e),
    /// }
    /// ```
    pub fn cas(
        &self,
        run_id: &RunId,
        doc_id: &str,
        expected_version: u64,
        path: &JsonPath,
        value: JsonValue,
    ) -> Result<Version> {
        // Validate path and value limits
        path.validate().map_err(limit_error_to_error)?;
        value.validate().map_err(limit_error_to_error)?;

        let key = self.key_for(run_id, doc_id);

        self.db.transaction(*run_id, |txn| {
            // Load existing document
            let stored = txn.get(&key)?.ok_or_else(|| {
                StrataError::invalid_input(format!("JSON document {} not found", doc_id))
            })?;
            let mut doc = Self::deserialize_doc(&stored)?;

            // Check version
            if doc.version != expected_version {
                return Err(StrataError::conflict(format!(
                    "Version mismatch: expected {}, got {}",
                    expected_version, doc.version
                )));
            }

            // Apply mutation
            set_at_path(&mut doc.value, path, value)
                .map_err(|e| StrataError::invalid_input(format!("Path error: {}", e)))?;
            doc.touch();

            // Store updated document
            let serialized = Self::serialize_doc(&doc)?;
            txn.put(key.clone(), serialized)?;

            Ok(Version::counter(doc.version))
        })
    }

    // ========================================================================
    // Introspection
    // ========================================================================

    /// List documents in the store with cursor-based pagination
    ///
    /// Supports Primitive Contract Invariant 6: Introspectable.
    /// Returns document IDs for a run, optionally filtered by prefix.
    ///
    /// # Arguments
    ///
    /// * `run_id` - RunId for namespace isolation
    /// * `prefix` - Optional prefix to filter document IDs (compared as strings)
    /// * `cursor` - Resume pagination from this cursor (doc_id to start after)
    /// * `limit` - Maximum number of results to return
    ///
    /// # Returns
    ///
    /// * `Ok(JsonListResult)` - Document IDs and optional next cursor
    ///
    /// # Example
    ///
    /// ```ignore
    /// // List first 10 documents
    /// let result = json.list(&run_id, None, None, 10)?;
    ///
    /// // List documents with prefix "user:"
    /// let result = json.list(&run_id, Some("user:"), None, 10)?;
    ///
    /// // Paginate through results
    /// let page1 = json.list(&run_id, None, None, 10)?;
    /// if let Some(cursor) = page1.next_cursor {
    ///     let page2 = json.list(&run_id, None, Some(&cursor), 10)?;
    /// }
    /// ```
    pub fn list(
        &self,
        run_id: &RunId,
        prefix: Option<&str>,
        cursor: Option<&str>,
        limit: usize,
    ) -> Result<JsonListResult> {
        let snapshot = self.db.storage().create_snapshot();
        let ns = self.namespace_for_run(run_id);
        let scan_prefix = Key::new_json_prefix(ns);

        let mut doc_ids = Vec::with_capacity(limit + 1);

        // Cursor is now a simple string key (no UUID parsing needed)
        let cursor_doc_id: Option<&str> = cursor;

        let mut past_cursor = cursor_doc_id.is_none();

        for (_key, versioned_value) in snapshot.scan_prefix(&scan_prefix)? {
            // Deserialize to get doc_id
            let doc = match Self::deserialize_doc(&versioned_value.value) {
                Ok(d) => d,
                Err(_) => continue, // Skip invalid documents
            };

            // Handle cursor: skip until we're past the cursor
            if !past_cursor {
                if cursor_doc_id == Some(doc.id.as_str()) {
                    past_cursor = true;
                }
                continue;
            }

            // Apply prefix filter if specified
            if let Some(p) = prefix {
                if !doc.id.starts_with(p) {
                    continue;
                }
            }

            doc_ids.push(doc.id);

            // Collect limit + 1 to detect if there are more
            if doc_ids.len() > limit {
                break;
            }
        }

        // If we have more than limit, pop the last and use it as cursor
        let next_cursor = if doc_ids.len() > limit {
            doc_ids.pop(); // Remove the extra item
            doc_ids.last().cloned()
        } else {
            None
        };

        Ok(JsonListResult { doc_ids, next_cursor })
    }

    /// Count documents in the store
    ///
    /// Returns the total number of JSON documents for a run.
    /// Supports Primitive Contract Invariant 6: Introspectable.
    ///
    /// # Arguments
    ///
    /// * `run_id` - RunId for namespace isolation
    ///
    /// # Returns
    ///
    /// * `Ok(u64)` - Number of documents
    ///
    /// # Example
    ///
    /// ```ignore
    /// let count = json.count(&run_id)?;
    /// println!("Store has {} documents", count);
    /// ```
    pub fn count(&self, run_id: &RunId) -> Result<u64> {
        let snapshot = self.db.storage().create_snapshot();
        let ns = self.namespace_for_run(run_id);
        let scan_prefix = Key::new_json_prefix(ns);

        let mut count = 0u64;
        for (_key, _versioned_value) in snapshot.scan_prefix(&scan_prefix)? {
            count += 1;
        }

        Ok(count)
    }

    /// Batch get multiple documents
    ///
    /// Retrieves multiple documents in a single operation.
    /// More efficient than multiple individual get() calls.
    ///
    /// # Arguments
    ///
    /// * `run_id` - RunId for namespace isolation
    /// * `doc_ids` - Document IDs to retrieve
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<Option<Versioned<JsonDoc>>>)` - Documents in same order as input
    ///   Returns `None` for documents that don't exist
    ///
    /// # Example
    ///
    /// ```ignore
    /// let doc_ids = vec![id1, id2, id3];
    /// let docs = json.batch_get(&run_id, &doc_ids)?;
    /// for (i, doc) in docs.iter().enumerate() {
    ///     match doc {
    ///         Some(v) => println!("Doc {} exists: {:?}", i, v.value),
    ///         None => println!("Doc {} not found", i),
    ///     }
    /// }
    /// ```
    pub fn batch_get<S: AsRef<str>>(
        &self,
        run_id: &RunId,
        doc_ids: &[S],
    ) -> Result<Vec<Option<Versioned<JsonDoc>>>> {
        let snapshot = self.db.storage().create_snapshot();

        let results: Vec<Option<Versioned<JsonDoc>>> = doc_ids
            .iter()
            .map(|doc_id| {
                let key = self.key_for(run_id, doc_id.as_ref());
                match snapshot.get(&key) {
                    Ok(Some(vv)) => {
                        match Self::deserialize_doc(&vv.value) {
                            Ok(doc) => Some(Versioned::with_timestamp(
                                doc.clone(),
                                Version::counter(doc.version),
                                Timestamp::from_micros(doc.updated_at),
                            )),
                            Err(_) => None,
                        }
                    }
                    Ok(None) => None,
                    Err(_) => None,
                }
            })
            .collect();

        Ok(results)
    }

    /// Batch create multiple documents atomically
    ///
    /// Creates multiple documents in a single atomic transaction.
    /// If any document fails to create (e.g., already exists), the entire
    /// operation fails and no documents are created.
    ///
    /// # Arguments
    ///
    /// * `run_id` - RunId for namespace isolation
    /// * `docs` - Document ID and value pairs to create
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<Version>)` - Versions of created documents in same order as input
    ///
    /// # Errors
    ///
    /// * `InvalidOperation` - If any document already exists
    ///
    /// # Example
    ///
    /// ```ignore
    /// let docs = vec![
    ///     (id1, JsonValue::from("value1")),
    ///     (id2, JsonValue::from("value2")),
    /// ];
    /// let versions = json.batch_create(&run_id, docs)?;
    /// ```
    pub fn batch_create<S: AsRef<str> + Clone>(
        &self,
        run_id: &RunId,
        docs: Vec<(S, JsonValue)>,
    ) -> Result<Vec<Version>> {
        // Validate all values first
        for (_doc_id, value) in &docs {
            value.validate().map_err(limit_error_to_error)?;
        }

        self.db.transaction(*run_id, |txn| {
            let mut versions = Vec::with_capacity(docs.len());

            for (doc_id, value) in &docs {
                let key = self.key_for(run_id, doc_id.as_ref());

                // Check if document already exists
                if txn.get(&key)?.is_some() {
                    return Err(StrataError::invalid_input(format!(
                        "JSON document {} already exists",
                        doc_id.as_ref()
                    )));
                }

                let doc = JsonDoc::new(doc_id.as_ref(), value.clone());
                let serialized = Self::serialize_doc(&doc)?;
                txn.put(key, serialized)?;
                versions.push(Version::counter(doc.version));
            }

            Ok(versions)
        })
    }

    // ========================================================================
    // Array Operations
    // ========================================================================

    /// Atomically push values to an array at path
    ///
    /// Appends one or more values to an array within a document.
    /// The operation is atomic - all values are appended or none.
    ///
    /// # Arguments
    ///
    /// * `run_id` - RunId for namespace isolation
    /// * `doc_id` - Document containing the array
    /// * `path` - Path to the array
    /// * `values` - Values to append
    ///
    /// # Returns
    ///
    /// * `Ok((Version, usize))` - New document version and new array length
    ///
    /// # Errors
    ///
    /// * `InvalidOperation` - If document doesn't exist
    /// * `InvalidOperation` - If path doesn't point to an array
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Append to array
    /// let (version, len) = json.array_push(
    ///     &run_id,
    ///     &doc_id,
    ///     &"items".parse()?,
    ///     vec![JsonValue::from("new_item")],
    /// )?;
    /// println!("Array now has {} items", len);
    /// ```
    pub fn array_push(
        &self,
        run_id: &RunId,
        doc_id: &str,
        path: &JsonPath,
        values: Vec<JsonValue>,
    ) -> Result<(Version, usize)> {
        // Validate path and values
        path.validate().map_err(limit_error_to_error)?;
        for value in &values {
            value.validate().map_err(limit_error_to_error)?;
        }

        let key = self.key_for(run_id, doc_id);

        self.db.transaction(*run_id, |txn| {
            // Load existing document
            let stored = txn.get(&key)?.ok_or_else(|| {
                StrataError::invalid_input(format!("JSON document {} not found", doc_id))
            })?;
            let mut doc = Self::deserialize_doc(&stored)?;

            // Get the array at path
            let arr = match get_at_path(&doc.value, path) {
                Some(val) => match val.as_array() {
                    Some(arr) => arr.clone(),
                    None => {
                        return Err(StrataError::invalid_input(format!(
                            "Path '{}' does not point to an array",
                            path
                        )));
                    }
                },
                None => {
                    return Err(StrataError::invalid_input(format!(
                        "Path '{}' does not exist",
                        path
                    )));
                }
            };

            // Create new array with appended values
            let mut new_arr = arr;
            new_arr.extend(values.into_iter().map(|v| v.into_inner()));
            let new_len = new_arr.len();

            // Update document
            let new_array = JsonValue::from(serde_json::Value::Array(new_arr));
            set_at_path(&mut doc.value, path, new_array)
                .map_err(|e| StrataError::invalid_input(format!("Path error: {}", e)))?;
            doc.touch();

            // Store updated document
            let serialized = Self::serialize_doc(&doc)?;
            txn.put(key.clone(), serialized)?;

            Ok((Version::counter(doc.version), new_len))
        })
    }

    /// Atomically increment a numeric value at path
    ///
    /// Adds a delta to a numeric value within a document.
    /// The operation is atomic - guaranteed not to lose increments.
    ///
    /// # Arguments
    ///
    /// * `run_id` - RunId for namespace isolation
    /// * `doc_id` - Document containing the number
    /// * `path` - Path to the number
    /// * `delta` - Value to add (can be negative for decrement)
    ///
    /// # Returns
    ///
    /// * `Ok((Version, f64))` - New document version and new value
    ///
    /// # Errors
    ///
    /// * `InvalidOperation` - If document doesn't exist
    /// * `InvalidOperation` - If path doesn't point to a number
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Increment counter
    /// let (version, new_val) = json.increment(
    ///     &run_id,
    ///     &doc_id,
    ///     &"views".parse()?,
    ///     1.0,
    /// )?;
    /// println!("View count is now {}", new_val);
    /// ```
    pub fn increment(
        &self,
        run_id: &RunId,
        doc_id: &str,
        path: &JsonPath,
        delta: f64,
    ) -> Result<(Version, f64)> {
        // Validate path
        path.validate().map_err(limit_error_to_error)?;

        let key = self.key_for(run_id, doc_id);

        self.db.transaction(*run_id, |txn| {
            // Load existing document
            let stored = txn.get(&key)?.ok_or_else(|| {
                StrataError::invalid_input(format!("JSON document {} not found", doc_id))
            })?;
            let mut doc = Self::deserialize_doc(&stored)?;

            // Get the current value at path
            let current_value = match get_at_path(&doc.value, path) {
                Some(val) => {
                    // Try to get as number
                    if let Some(n) = val.as_f64() {
                        n
                    } else if let Some(n) = val.as_i64() {
                        n as f64
                    } else {
                        return Err(StrataError::invalid_input(format!(
                            "Path '{}' does not point to a number",
                            path
                        )));
                    }
                }
                None => {
                    return Err(StrataError::invalid_input(format!(
                        "Path '{}' does not exist",
                        path
                    )));
                }
            };

            // Calculate new value
            let new_value = current_value + delta;

            // Update document
            let new_json_value = if new_value.fract() == 0.0 && new_value >= i64::MIN as f64 && new_value <= i64::MAX as f64 {
                // Store as integer if it's a whole number
                JsonValue::from(new_value as i64)
            } else {
                JsonValue::from(new_value)
            };
            set_at_path(&mut doc.value, path, new_json_value)
                .map_err(|e| StrataError::invalid_input(format!("Path error: {}", e)))?;
            doc.touch();

            // Store updated document
            let serialized = Self::serialize_doc(&doc)?;
            txn.put(key.clone(), serialized)?;

            Ok((Version::counter(doc.version), new_value))
        })
    }

    /// Atomically pop a value from an array at path
    ///
    /// Removes and returns the last element from an array within a document.
    /// The operation is atomic.
    ///
    /// # Arguments
    ///
    /// * `run_id` - RunId for namespace isolation
    /// * `doc_id` - Document containing the array
    /// * `path` - Path to the array
    ///
    /// # Returns
    ///
    /// * `Ok((Version, Some(value)))` - New version and popped value
    /// * `Ok((Version, None))` - New version but array was empty
    ///
    /// # Errors
    ///
    /// * `InvalidOperation` - If document doesn't exist
    /// * `InvalidOperation` - If path doesn't point to an array
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Pop from array
    /// let (version, popped) = json.array_pop(&run_id, &doc_id, &"items".parse()?)?;
    /// match popped {
    ///     Some(val) => println!("Popped: {:?}", val),
    ///     None => println!("Array was empty"),
    /// }
    /// ```
    pub fn array_pop(
        &self,
        run_id: &RunId,
        doc_id: &str,
        path: &JsonPath,
    ) -> Result<(Version, Option<JsonValue>)> {
        // Validate path
        path.validate().map_err(limit_error_to_error)?;

        let key = self.key_for(run_id, doc_id);

        self.db.transaction(*run_id, |txn| {
            // Load existing document
            let stored = txn.get(&key)?.ok_or_else(|| {
                StrataError::invalid_input(format!("JSON document {} not found", doc_id))
            })?;
            let mut doc = Self::deserialize_doc(&stored)?;

            // Get the array at path
            let mut arr = match get_at_path(&doc.value, path) {
                Some(val) => match val.as_array() {
                    Some(arr) => arr.clone(),
                    None => {
                        return Err(StrataError::invalid_input(format!(
                            "Path '{}' does not point to an array",
                            path
                        )));
                    }
                },
                None => {
                    return Err(StrataError::invalid_input(format!(
                        "Path '{}' does not exist",
                        path
                    )));
                }
            };

            // Pop the last element
            let popped = arr.pop().map(JsonValue::from);

            // Update document
            let new_array = JsonValue::from(serde_json::Value::Array(arr));
            set_at_path(&mut doc.value, path, new_array)
                .map_err(|e| StrataError::invalid_input(format!("Path error: {}", e)))?;
            doc.touch();

            // Store updated document
            let serialized = Self::serialize_doc(&doc)?;
            txn.put(key.clone(), serialized)?;

            Ok((Version::counter(doc.version), popped))
        })
    }

    /// Query documents by exact field match
    ///
    /// Finds documents where the value at the specified path exactly matches
    /// the given value. Unlike search (fuzzy text matching), query performs
    /// exact equality comparison.
    ///
    /// # Arguments
    ///
    /// * `run_id` - RunId for namespace isolation
    /// * `path` - Path within documents to compare
    /// * `value` - Value to match exactly
    /// * `limit` - Maximum number of results to return
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<String>)` - Document IDs with matching value at path
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Find all documents where status == "active"
    /// let active_docs = json.query(
    ///     &run_id,
    ///     &"status".parse()?,
    ///     &JsonValue::from("active"),
    ///     100,
    /// )?;
    ///
    /// // Find all orders for a specific user
    /// let user_orders = json.query(
    ///     &run_id,
    ///     &"user_id".parse()?,
    ///     &JsonValue::from(user_id),
    ///     50,
    /// )?;
    /// ```
    pub fn query(
        &self,
        run_id: &RunId,
        path: &JsonPath,
        value: &JsonValue,
        limit: usize,
    ) -> Result<Vec<String>> {
        // Validate path limits
        path.validate().map_err(limit_error_to_error)?;

        let snapshot = self.db.storage().create_snapshot();
        let ns = self.namespace_for_run(run_id);
        let scan_prefix = Key::new_json_prefix(ns);

        let mut results = Vec::new();

        for (_key, versioned_value) in snapshot.scan_prefix(&scan_prefix)? {
            // Deserialize document
            let doc = match Self::deserialize_doc(&versioned_value.value) {
                Ok(d) => d,
                Err(_) => continue, // Skip invalid documents
            };

            // Check if value at path matches
            if let Some(doc_value) = get_at_path(&doc.value, path) {
                if doc_value == value {
                    results.push(doc.id);
                    if results.len() >= limit {
                        break;
                    }
                }
            }
        }

        Ok(results)
    }

    // ========================================================================
    // Search API
    // ========================================================================

    /// Search JSON documents
    ///
    /// Flattens JSON structure into searchable text and scores against query.
    /// Respects budget constraints (time and candidate limits).
    ///
    /// # Example
    ///
    /// ```ignore
    /// use strata_core::SearchRequest;
    ///
    /// let response = json.search(&SearchRequest::new(run_id, "Alice"))?;
    /// for hit in response.hits {
    ///     println!("Found doc {:?} with score {}", hit.doc_ref, hit.score);
    /// }
    /// ```
    pub fn search(
        &self,
        req: &strata_core::SearchRequest,
    ) -> strata_core::error::Result<strata_core::SearchResponse> {
        use crate::primitives::searchable::{build_search_response, SearchCandidate};
        use strata_core::search_types::EntityRef;
        use std::time::Instant;

        let start = Instant::now();
        let snapshot = self.db.storage().create_snapshot();
        let ns = self.namespace_for_run(&req.run_id);
        let scan_prefix = Key::new_json_prefix(ns);

        let mut candidates = Vec::new();
        let mut truncated = false;

        // Scan all JSON documents for this run
        for (_key, versioned_value) in snapshot.scan_prefix(&scan_prefix)? {
            // Check budget constraints
            if start.elapsed().as_micros() as u64 >= req.budget.max_wall_time_micros {
                truncated = true;
                break;
            }
            if candidates.len() >= req.budget.max_candidates_per_primitive {
                truncated = true;
                break;
            }

            // Deserialize document
            let doc = match Self::deserialize_doc(&versioned_value.value) {
                Ok(d) => d,
                Err(_) => continue, // Skip invalid documents
            };

            // Time range filter
            if let Some((start_ts, end_ts)) = req.time_range {
                let ts = doc.updated_at as u64;
                if ts < start_ts || ts > end_ts {
                    continue;
                }
            }

            // Extract searchable text by flattening JSON
            let text = self.flatten_json(&doc.value);

            candidates.push(SearchCandidate::new(
                EntityRef::Json {
                    run_id: req.run_id,
                    doc_id: doc.id,
                },
                text,
                Some(doc.updated_at as u64),
            ));
        }

        Ok(build_search_response(
            candidates,
            &req.query,
            req.k,
            truncated,
            start.elapsed().as_micros() as u64,
        ))
    }

    /// Flatten JSON into searchable text
    ///
    /// Recursively extracts all string values and creates "path: value" pairs
    /// for better search context.
    fn flatten_json(&self, value: &JsonValue) -> String {
        let mut parts = Vec::new();
        self.flatten_recursive(value.as_inner(), &mut parts, "");
        parts.join(" ")
    }

    /// Recursively flatten JSON value
    fn flatten_recursive(&self, value: &serde_json::Value, parts: &mut Vec<String>, path: &str) {
        use serde_json::Value as JV;

        match value {
            JV::String(s) => {
                parts.push(s.clone());
                if !path.is_empty() {
                    parts.push(format!("{}: {}", path, s));
                }
            }
            JV::Number(n) => {
                parts.push(format!("{}", n));
            }
            JV::Bool(b) => {
                parts.push(format!("{}", b));
            }
            JV::Array(arr) => {
                for (i, item) in arr.iter().enumerate() {
                    let child_path = if path.is_empty() {
                        format!("[{}]", i)
                    } else {
                        format!("{}[{}]", path, i)
                    };
                    self.flatten_recursive(item, parts, &child_path);
                }
            }
            JV::Object(obj) => {
                for (k, v) in obj.iter() {
                    let child_path = if path.is_empty() {
                        k.clone()
                    } else {
                        format!("{}.{}", path, k)
                    };
                    parts.push(k.clone()); // Include field names as searchable
                    self.flatten_recursive(v, parts, &child_path);
                }
            }
            JV::Null => {}
        }
    }
}

// ========== Searchable Trait Implementation ==========

impl crate::primitives::searchable::Searchable for JsonStore {
    fn search(
        &self,
        req: &strata_core::SearchRequest,
    ) -> strata_core::error::Result<strata_core::SearchResponse> {
        self.search(req)
    }

    fn primitive_kind(&self) -> strata_core::PrimitiveType {
        strata_core::PrimitiveType::Json
    }
}

// =============================================================================
// JsonStoreExt Implementation
// =============================================================================
//
// Extension trait implementation for cross-primitive transactions.
// Allows JSON operations within a TransactionContext.


impl JsonStoreExt for TransactionContext {
    fn json_get(&mut self, doc_id: &str, path: &JsonPath) -> Result<Option<JsonValue>> {
        // Validate path limits (Issue #440)
        path.validate().map_err(limit_error_to_error)?;

        let key = Key::new_json(Namespace::for_run(self.run_id), doc_id);

        // Read from transaction context (respects read-your-writes)
        match self.get(&key)? {
            Some(value) => {
                let doc = JsonStore::deserialize_doc(&value)?;
                Ok(get_at_path(&doc.value, path).cloned())
            }
            None => Ok(None),
        }
    }

    fn json_set(&mut self, doc_id: &str, path: &JsonPath, value: JsonValue) -> Result<Version> {
        // Validate path and value limits (Issue #440)
        path.validate().map_err(limit_error_to_error)?;
        value.validate().map_err(limit_error_to_error)?;

        let key = Key::new_json(Namespace::for_run(self.run_id), doc_id);

        // Load existing document from transaction context
        let stored = self.get(&key)?.ok_or_else(|| {
            StrataError::invalid_input(format!("JSON document {} not found", doc_id))
        })?;
        let mut doc = JsonStore::deserialize_doc(&stored)?;

        // Apply mutation
        set_at_path(&mut doc.value, path, value)
            .map_err(|e| StrataError::invalid_input(format!("Path error: {}", e)))?;
        doc.touch();

        // Store updated document in transaction write set
        let serialized = JsonStore::serialize_doc(&doc)?;
        self.put(key, serialized)?;

        Ok(Version::counter(doc.version))
    }

    fn json_create(&mut self, doc_id: &str, value: JsonValue) -> Result<Version> {
        // Validate document limits (Issue #440)
        value.validate().map_err(limit_error_to_error)?;

        let key = Key::new_json(Namespace::for_run(self.run_id), doc_id);
        let doc = JsonDoc::new(doc_id, value);

        // Check if document already exists
        if self.get(&key)?.is_some() {
            return Err(StrataError::invalid_input(format!(
                "JSON document {} already exists",
                doc_id
            )));
        }

        // Store new document
        let serialized = JsonStore::serialize_doc(&doc)?;
        self.put(key, serialized)?;

        Ok(Version::counter(doc.version))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jsonstore_is_stateless() {
        // JsonStore should have size of single Arc pointer
        assert_eq!(
            std::mem::size_of::<JsonStore>(),
            std::mem::size_of::<Arc<Database>>()
        );
    }

    #[test]
    fn test_jsonstore_is_clone() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store1 = JsonStore::new(db.clone());
        let store2 = store1.clone();
        assert!(Arc::ptr_eq(store1.database(), store2.database()));
    }

    #[test]
    fn test_jsonstore_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<JsonStore>();
    }

    #[test]
    fn test_key_for_run_isolation() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);

        let run1 = RunId::new();
        let run2 = RunId::new();
        let doc_id = "test-doc";

        let key1 = store.key_for(&run1, &doc_id);
        let key2 = store.key_for(&run2, &doc_id);

        // Keys for different runs should be different even for same doc_id
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_key_for_same_run() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);

        let run_id = RunId::new();
        let doc_id = "test-doc";

        let key1 = store.key_for(&run_id, &doc_id);
        let key2 = store.key_for(&run_id, &doc_id);

        // Same run and doc_id should produce same key
        assert_eq!(key1, key2);
    }

    // ========================================
    // JsonDoc Tests
    // ========================================

    #[test]
    fn test_json_doc_new() {
        let id = "test-doc";
        let value = JsonValue::from(42i64);
        let doc = JsonDoc::new(id, value.clone());

        assert_eq!(doc.id, id);
        assert_eq!(doc.value, value);
        assert_eq!(doc.version, 1);
        assert!(doc.created_at > 0);
        assert_eq!(doc.created_at, doc.updated_at);
    }

    #[test]
    fn test_json_doc_touch() {
        let id = "test-doc";
        let value = JsonValue::from(42i64);
        let mut doc = JsonDoc::new(id, value);

        let old_version = doc.version;
        let old_updated = doc.updated_at;

        // Sleep a tiny bit to ensure timestamp changes
        std::thread::sleep(std::time::Duration::from_millis(2));
        doc.touch();

        assert_eq!(doc.version, old_version + 1);
        assert!(doc.updated_at >= old_updated);
        // created_at should not change
        assert_eq!(doc.created_at, doc.created_at);
    }

    #[test]
    fn test_json_doc_touch_multiple() {
        let id = "test-doc";
        let value = JsonValue::object();
        let mut doc = JsonDoc::new(id, value);

        for i in 0..5 {
            doc.touch();
            assert_eq!(doc.version, 2 + i);
        }
        assert_eq!(doc.version, 6);
    }

    // ========================================
    // Serialization Tests
    // ========================================

    #[test]
    fn test_serialize_deserialize_roundtrip() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let _store = JsonStore::new(db);

        let doc = JsonDoc::new("test-doc", JsonValue::from("test value"));

        let serialized = JsonStore::serialize_doc(&doc).unwrap();
        let deserialized = JsonStore::deserialize_doc(&serialized).unwrap();

        assert_eq!(doc.id, deserialized.id);
        assert_eq!(doc.value, deserialized.value);
        assert_eq!(doc.version, deserialized.version);
        assert_eq!(doc.created_at, deserialized.created_at);
        assert_eq!(doc.updated_at, deserialized.updated_at);
    }

    #[test]
    fn test_serialize_complex_document() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let _store = JsonStore::new(db);

        let value: JsonValue = serde_json::json!({
            "string": "hello",
            "number": 42,
            "boolean": true,
            "null": null,
            "array": [1, 2, 3],
            "nested": {
                "foo": "bar"
            }
        })
        .into();

        let doc = JsonDoc::new("test-doc", value);

        let serialized = JsonStore::serialize_doc(&doc).unwrap();
        let deserialized = JsonStore::deserialize_doc(&serialized).unwrap();

        assert_eq!(doc.value, deserialized.value);
    }

    #[test]
    fn test_deserialize_invalid_type() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let _store = JsonStore::new(db);

        // Try to deserialize a non-bytes value
        let invalid = Value::Int(42);
        let result = JsonStore::deserialize_doc(&invalid);

        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_invalid_bytes() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let _store = JsonStore::new(db);

        // Try to deserialize garbage bytes
        let invalid = Value::Bytes(vec![0, 1, 2, 3, 4, 5]);
        let result = JsonStore::deserialize_doc(&invalid);

        assert!(result.is_err());
    }

    #[test]
    fn test_serialized_size_is_compact() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let _store = JsonStore::new(db);

        let doc = JsonDoc::new("test-doc", JsonValue::from(42i64));

        let serialized = JsonStore::serialize_doc(&doc).unwrap();

        match serialized {
            Value::Bytes(bytes) => {
                // MessagePack should produce reasonably compact output
                // UUID (16 bytes) + value + version + timestamps should be < 100 bytes
                assert!(bytes.len() < 100);
            }
            _ => panic!("Expected bytes"),
        }
    }

    // ========================================
    // Create Tests
    // ========================================

    #[test]
    fn test_create_document() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = "test-doc";

        let version = store
            .create(&run_id, &doc_id, JsonValue::from(42i64))
            .unwrap();
        assert_eq!(version, Version::counter(1));
    }

    #[test]
    fn test_create_object_document() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = "test-doc";

        let value: JsonValue = serde_json::json!({
            "name": "Alice",
            "age": 30
        })
        .into();

        let version = store.create(&run_id, &doc_id, value).unwrap();
        assert_eq!(version, Version::counter(1));
    }

    #[test]
    fn test_create_duplicate_fails() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = "test-doc";

        // First create succeeds
        store
            .create(&run_id, &doc_id, JsonValue::from(1i64))
            .unwrap();

        // Second create with same ID fails
        let result = store.create(&run_id, &doc_id, JsonValue::from(2i64));
        assert!(result.is_err());
    }

    #[test]
    fn test_create_different_docs() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();

        let doc1 = "doc-1";
        let doc2 = "doc-2";

        let v1 = store.create(&run_id, &doc1, JsonValue::from(1i64)).unwrap();
        let v2 = store.create(&run_id, &doc2, JsonValue::from(2i64)).unwrap();

        assert_eq!(v1, Version::counter(1));
        assert_eq!(v2, Version::counter(1));
    }

    #[test]
    fn test_create_run_isolation() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);

        let run1 = RunId::new();
        let run2 = RunId::new();
        let doc_id = "test-doc";

        // Same doc_id can be created in different runs
        let v1 = store.create(&run1, &doc_id, JsonValue::from(1i64)).unwrap();
        let v2 = store.create(&run2, &doc_id, JsonValue::from(2i64)).unwrap();

        assert_eq!(v1, Version::counter(1));
        assert_eq!(v2, Version::counter(1));
    }

    #[test]
    fn test_create_null_value() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = "test-doc";

        let version = store.create(&run_id, &doc_id, JsonValue::null()).unwrap();
        assert_eq!(version, Version::counter(1));
    }

    #[test]
    fn test_create_empty_object() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = "test-doc";

        let version = store.create(&run_id, &doc_id, JsonValue::object()).unwrap();
        assert_eq!(version, Version::counter(1));
    }

    #[test]
    fn test_create_empty_array() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = "test-doc";

        let version = store.create(&run_id, &doc_id, JsonValue::array()).unwrap();
        assert_eq!(version, Version::counter(1));
    }

    // ========================================
    // Get Tests
    // ========================================

    #[test]
    fn test_get_root() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = "test-doc";

        store
            .create(&run_id, &doc_id, JsonValue::from(42i64))
            .unwrap();

        let value = store.get(&run_id, &doc_id, &JsonPath::root()).unwrap();
        assert_eq!(value.map(|v| v.value).and_then(|v| v.as_i64()), Some(42));
    }

    #[test]
    fn test_get_at_path() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = "test-doc";

        let value: JsonValue = serde_json::json!({
            "name": "Alice",
            "age": 30
        })
        .into();

        store.create(&run_id, &doc_id, value).unwrap();

        let name = store
            .get(&run_id, &doc_id, &"name".parse().unwrap())
            .unwrap();
        assert_eq!(
            name.map(|v| v.value).and_then(|v| v.as_str().map(String::from)),
            Some("Alice".to_string())
        );

        let age = store
            .get(&run_id, &doc_id, &"age".parse().unwrap())
            .unwrap();
        assert_eq!(age.map(|v| v.value).and_then(|v| v.as_i64()), Some(30));
    }

    #[test]
    fn test_get_nested_path() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = "test-doc";

        let value: JsonValue = serde_json::json!({
            "user": {
                "profile": {
                    "name": "Bob"
                }
            }
        })
        .into();

        store.create(&run_id, &doc_id, value).unwrap();

        let name = store
            .get(&run_id, &doc_id, &"user.profile.name".parse().unwrap())
            .unwrap();
        assert_eq!(
            name.map(|v| v.value).and_then(|v| v.as_str().map(String::from)),
            Some("Bob".to_string())
        );
    }

    #[test]
    fn test_get_array_element() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = "test-doc";

        let value: JsonValue = serde_json::json!({
            "items": ["a", "b", "c"]
        })
        .into();

        store.create(&run_id, &doc_id, value).unwrap();

        let item = store
            .get(&run_id, &doc_id, &"items[1]".parse().unwrap())
            .unwrap();
        assert_eq!(
            item.map(|v| v.value).and_then(|v| v.as_str().map(String::from)),
            Some("b".to_string())
        );
    }

    #[test]
    fn test_get_missing_document() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = "test-doc";

        let result = store.get(&run_id, &doc_id, &JsonPath::root()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_missing_path() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = "test-doc";

        store.create(&run_id, &doc_id, JsonValue::object()).unwrap();

        let result = store
            .get(&run_id, &doc_id, &"nonexistent".parse().unwrap())
            .unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_doc() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = "test-doc";

        store
            .create(&run_id, &doc_id, JsonValue::from(42i64))
            .unwrap();

        let versioned_doc = store.get_doc(&run_id, &doc_id).unwrap().unwrap();
        let doc = versioned_doc.value;
        assert_eq!(doc.id, doc_id);
        assert_eq!(doc.version, 1);
        assert_eq!(doc.value, JsonValue::from(42i64));
    }

    #[test]
    fn test_get_doc_missing() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = "test-doc";

        let result = store.get_doc(&run_id, &doc_id).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_version() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = "test-doc";

        store
            .create(&run_id, &doc_id, JsonValue::from(42i64))
            .unwrap();

        let version = store.get_version(&run_id, &doc_id).unwrap();
        assert_eq!(version, Some(1));
    }

    #[test]
    fn test_get_version_missing() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = "test-doc";

        let version = store.get_version(&run_id, &doc_id).unwrap();
        assert!(version.is_none());
    }

    #[test]
    fn test_exists() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = "test-doc";

        assert!(!store.exists(&run_id, &doc_id).unwrap());

        store
            .create(&run_id, &doc_id, JsonValue::from(42i64))
            .unwrap();

        assert!(store.exists(&run_id, &doc_id).unwrap());
    }

    #[test]
    fn test_exists_run_isolation() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);

        let run1 = RunId::new();
        let run2 = RunId::new();
        let doc_id = "test-doc";

        store
            .create(&run1, &doc_id, JsonValue::from(42i64))
            .unwrap();

        // Document exists in run1 but not in run2
        assert!(store.exists(&run1, &doc_id).unwrap());
        assert!(!store.exists(&run2, &doc_id).unwrap());
    }

    // ========================================
    // Set Tests
    // ========================================

    #[test]
    fn test_set_at_root() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = "test-doc";

        store
            .create(&run_id, &doc_id, JsonValue::from(42i64))
            .unwrap();

        let v2 = store
            .set(&run_id, &doc_id, &JsonPath::root(), JsonValue::from(100i64))
            .unwrap();
        assert_eq!(v2, Version::counter(2));

        let value = store.get(&run_id, &doc_id, &JsonPath::root()).unwrap();
        assert_eq!(value.map(|v| v.value).and_then(|v| v.as_i64()), Some(100));
    }

    #[test]
    fn test_set_at_path() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = "test-doc";

        store.create(&run_id, &doc_id, JsonValue::object()).unwrap();

        let v2 = store
            .set(
                &run_id,
                &doc_id,
                &"name".parse().unwrap(),
                JsonValue::from("Alice"),
            )
            .unwrap();
        assert_eq!(v2, Version::counter(2));

        let name = store
            .get(&run_id, &doc_id, &"name".parse().unwrap())
            .unwrap();
        assert_eq!(
            name.map(|v| v.value).and_then(|v| v.as_str().map(String::from)),
            Some("Alice".to_string())
        );
    }

    #[test]
    fn test_set_nested_path() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = "test-doc";

        store.create(&run_id, &doc_id, JsonValue::object()).unwrap();

        // Creates intermediate objects automatically
        let v2 = store
            .set(
                &run_id,
                &doc_id,
                &"user.profile.name".parse().unwrap(),
                JsonValue::from("Bob"),
            )
            .unwrap();
        assert_eq!(v2, Version::counter(2));

        let name = store
            .get(&run_id, &doc_id, &"user.profile.name".parse().unwrap())
            .unwrap();
        assert_eq!(
            name.map(|v| v.value).and_then(|v| v.as_str().map(String::from)),
            Some("Bob".to_string())
        );
    }

    #[test]
    fn test_set_increments_version() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = "test-doc";

        store.create(&run_id, &doc_id, JsonValue::object()).unwrap();
        assert_eq!(store.get_version(&run_id, &doc_id).unwrap(), Some(1));

        store
            .set(
                &run_id,
                &doc_id,
                &"a".parse().unwrap(),
                JsonValue::from(1i64),
            )
            .unwrap();
        assert_eq!(store.get_version(&run_id, &doc_id).unwrap(), Some(2));

        store
            .set(
                &run_id,
                &doc_id,
                &"b".parse().unwrap(),
                JsonValue::from(2i64),
            )
            .unwrap();
        assert_eq!(store.get_version(&run_id, &doc_id).unwrap(), Some(3));
    }

    #[test]
    fn test_set_missing_document() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = "test-doc";

        let result = store.set(
            &run_id,
            &doc_id,
            &"name".parse().unwrap(),
            JsonValue::from("test"),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_set_overwrites_value() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = "test-doc";

        let value: JsonValue = serde_json::json!({ "name": "Alice" }).into();
        store.create(&run_id, &doc_id, value).unwrap();

        store
            .set(
                &run_id,
                &doc_id,
                &"name".parse().unwrap(),
                JsonValue::from("Bob"),
            )
            .unwrap();

        let name = store
            .get(&run_id, &doc_id, &"name".parse().unwrap())
            .unwrap();
        assert_eq!(
            name.map(|v| v.value).and_then(|v| v.as_str().map(String::from)),
            Some("Bob".to_string())
        );
    }

    #[test]
    fn test_set_array_element() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = "test-doc";

        let value: JsonValue = serde_json::json!({ "items": [1, 2, 3] }).into();
        store.create(&run_id, &doc_id, value).unwrap();

        store
            .set(
                &run_id,
                &doc_id,
                &"items[1]".parse().unwrap(),
                JsonValue::from(999i64),
            )
            .unwrap();

        let item = store
            .get(&run_id, &doc_id, &"items[1]".parse().unwrap())
            .unwrap();
        assert_eq!(item.map(|v| v.value).and_then(|v| v.as_i64()), Some(999));
    }

    // ========================================
    // Delete at Path Tests
    // ========================================

    #[test]
    fn test_delete_at_path() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = "test-doc";

        let value: JsonValue = serde_json::json!({
            "name": "Alice",
            "age": 30
        })
        .into();
        store.create(&run_id, &doc_id, value).unwrap();

        // Delete the "age" field
        let v2 = store
            .delete_at_path(&run_id, &doc_id, &"age".parse().unwrap())
            .unwrap();
        assert_eq!(v2, Version::counter(2));

        // Verify "age" is gone but "name" remains
        assert!(store
            .get(&run_id, &doc_id, &"age".parse().unwrap())
            .unwrap()
            .is_none());
        assert_eq!(
            store
                .get(&run_id, &doc_id, &"name".parse().unwrap())
                .unwrap()
                .map(|v| v.value)
                .and_then(|v| v.as_str().map(String::from)),
            Some("Alice".to_string())
        );
    }

    #[test]
    fn test_delete_at_nested_path() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = "test-doc";

        let value: JsonValue = serde_json::json!({
            "user": {
                "profile": {
                    "name": "Bob",
                    "temp": "to_delete"
                }
            }
        })
        .into();
        store.create(&run_id, &doc_id, value).unwrap();

        // Delete nested field
        let v2 = store
            .delete_at_path(&run_id, &doc_id, &"user.profile.temp".parse().unwrap())
            .unwrap();
        assert_eq!(v2, Version::counter(2));

        // Verify "temp" is gone
        assert!(store
            .get(&run_id, &doc_id, &"user.profile.temp".parse().unwrap())
            .unwrap()
            .is_none());

        // Verify "name" remains
        assert_eq!(
            store
                .get(&run_id, &doc_id, &"user.profile.name".parse().unwrap())
                .unwrap()
                .map(|v| v.value)
                .and_then(|v| v.as_str().map(String::from)),
            Some("Bob".to_string())
        );
    }

    #[test]
    fn test_delete_at_path_array_element() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = "test-doc";

        let value: JsonValue = serde_json::json!({
            "items": ["a", "b", "c"]
        })
        .into();
        store.create(&run_id, &doc_id, value).unwrap();

        // Delete middle element
        let v2 = store
            .delete_at_path(&run_id, &doc_id, &"items[1]".parse().unwrap())
            .unwrap();
        assert_eq!(v2, Version::counter(2));

        // Array should now be ["a", "c"]
        let items = store
            .get(&run_id, &doc_id, &"items".parse().unwrap())
            .unwrap()
            .unwrap()
            .value;
        let arr = items.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0].as_str(), Some("a"));
        assert_eq!(arr[1].as_str(), Some("c"));
    }

    #[test]
    fn test_delete_at_path_increments_version() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = "test-doc";

        let value: JsonValue = serde_json::json!({
            "a": 1,
            "b": 2,
            "c": 3
        })
        .into();
        store.create(&run_id, &doc_id, value).unwrap();
        assert_eq!(store.get_version(&run_id, &doc_id).unwrap(), Some(1));

        store
            .delete_at_path(&run_id, &doc_id, &"a".parse().unwrap())
            .unwrap();
        assert_eq!(store.get_version(&run_id, &doc_id).unwrap(), Some(2));

        store
            .delete_at_path(&run_id, &doc_id, &"b".parse().unwrap())
            .unwrap();
        assert_eq!(store.get_version(&run_id, &doc_id).unwrap(), Some(3));
    }

    #[test]
    fn test_delete_at_path_missing_document() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = "test-doc";

        let result = store.delete_at_path(&run_id, &doc_id, &"field".parse().unwrap());
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_at_path_missing_path() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = "test-doc";

        store.create(&run_id, &doc_id, JsonValue::object()).unwrap();

        // Deleting a nonexistent path is idempotent (succeeds, increments version)
        let v2 = store
            .delete_at_path(&run_id, &doc_id, &"nonexistent".parse().unwrap())
            .unwrap();
        assert_eq!(v2, Version::counter(2)); // Version still increments even though nothing was removed
    }

    // ========================================
    // Destroy Tests
    // ========================================

    #[test]
    fn test_destroy_existing_document() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = "test-doc";

        store
            .create(&run_id, &doc_id, JsonValue::from(42i64))
            .unwrap();
        assert!(store.exists(&run_id, &doc_id).unwrap());

        let existed = store.destroy(&run_id, &doc_id).unwrap();
        assert!(existed);
        assert!(!store.exists(&run_id, &doc_id).unwrap());
    }

    #[test]
    fn test_destroy_nonexistent_document() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = "test-doc";

        let existed = store.destroy(&run_id, &doc_id).unwrap();
        assert!(!existed);
    }

    #[test]
    fn test_destroy_run_isolation() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);

        let run1 = RunId::new();
        let run2 = RunId::new();
        let doc_id = "test-doc";

        // Create document in both runs
        store.create(&run1, &doc_id, JsonValue::from(1i64)).unwrap();
        store.create(&run2, &doc_id, JsonValue::from(2i64)).unwrap();

        // Destroy in run1
        store.destroy(&run1, &doc_id).unwrap();

        // Document should be gone from run1 but still exist in run2
        assert!(!store.exists(&run1, &doc_id).unwrap());
        assert!(store.exists(&run2, &doc_id).unwrap());
    }

    #[test]
    fn test_destroy_then_recreate() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = "test-doc";

        // Create, destroy, recreate
        store
            .create(&run_id, &doc_id, JsonValue::from(1i64))
            .unwrap();
        store.destroy(&run_id, &doc_id).unwrap();

        // Should be able to recreate with new value
        let version = store
            .create(&run_id, &doc_id, JsonValue::from(2i64))
            .unwrap();
        assert_eq!(version, Version::counter(1)); // Fresh document starts at version 1

        let value = store.get(&run_id, &doc_id, &JsonPath::root()).unwrap();
        assert_eq!(value.map(|v| v.value).and_then(|v| v.as_i64()), Some(2));
    }

    #[test]
    fn test_destroy_complex_document() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = "test-doc";

        let value: JsonValue = serde_json::json!({
            "user": {
                "name": "Alice",
                "items": [1, 2, 3],
                "nested": {
                    "deep": {
                        "value": true
                    }
                }
            }
        })
        .into();
        store.create(&run_id, &doc_id, value).unwrap();

        let existed = store.destroy(&run_id, &doc_id).unwrap();
        assert!(existed);
        assert!(!store.exists(&run_id, &doc_id).unwrap());
    }

    #[test]
    fn test_destroy_idempotent() {
        let db = Arc::new(Database::builder().in_memory().open_temp().unwrap());
        let store = JsonStore::new(db);
        let run_id = RunId::new();
        let doc_id = "test-doc";

        store
            .create(&run_id, &doc_id, JsonValue::from(42i64))
            .unwrap();

        // First destroy returns true
        assert!(store.destroy(&run_id, &doc_id).unwrap());

        // Subsequent destroys return false
        assert!(!store.destroy(&run_id, &doc_id).unwrap());
        assert!(!store.destroy(&run_id, &doc_id).unwrap());
    }
}
