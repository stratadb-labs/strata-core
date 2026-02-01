//! StateCell: CAS-based versioned cells for coordination
//!
//! ## Design Principles
//!
//! 1. **Versioned Updates**: Every update increments the version.
//! 2. **CAS Semantics**: Compare-and-swap ensures safe concurrent updates.
//!
//! ## API
//!
//! All operations go through `db.transaction()` for consistency:
//! - `init`, `read`, `set`, `cas`
//!
//! ## Key Design
//!
//! - TypeTag: State (0x03)
//! - Key format: `<namespace>:<TypeTag::State>:<cell_name>`

use crate::primitives::extensions::StateCellExt;
use strata_concurrency::TransactionContext;
use strata_core::contract::{Version, Versioned};
use strata_core::{StrataResult, VersionedHistory};
use strata_core::types::{Key, Namespace, BranchId};
use strata_core::value::Value;
use strata_core::Timestamp;
use crate::database::Database;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// Re-export State from core
pub use strata_core::primitives::State;

/// Serialize a struct to Value::String for storage
fn to_stored_value<T: Serialize>(v: &T) -> StrataResult<Value> {
    serde_json::to_string(v)
        .map(Value::String)
        .map_err(|e| strata_core::StrataError::serialization(e.to_string()))
}

/// Deserialize from Value::String storage
fn from_stored_value<T: for<'de> Deserialize<'de>>(
    v: &Value,
) -> std::result::Result<T, serde_json::Error> {
    match v {
        Value::String(s) => serde_json::from_str(s),
        _ => serde_json::from_str("null"), // Will fail with appropriate error
    }
}

/// CAS-based versioned cells for coordination
///
/// ## Design
///
/// Each cell has a value and monotonically increasing version.
/// Updates via CAS ensure safe concurrent access.
///
/// ## Example
///
/// ```rust,ignore
/// use strata_primitives::StateCell;
/// use strata_core::value::Value;
///
/// let sc = StateCell::new(db.clone());
/// let branch_id = BranchId::new();
///
/// // Initialize a counter
/// sc.init(&branch_id, "counter", Value::Int(0))?;
///
/// // Read current state
/// let state = sc.read(&branch_id, "counter")?.unwrap();
/// assert_eq!(state.value.version, Version::counter(1));
///
/// // CAS update (only succeeds if version matches)
/// sc.cas(&branch_id, "counter", Version::counter(1), Value::Int(1))?;
///
/// // Unconditional set
/// sc.set(&branch_id, "counter", Value::Int(10))?;
/// ```
#[derive(Clone)]
pub struct StateCell {
    db: Arc<Database>,
}

impl StateCell {
    /// Create new StateCell instance
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Build namespace for branch-scoped operations
    fn namespace_for_branch(&self, branch_id: &BranchId) -> Namespace {
        Namespace::for_branch(*branch_id)
    }

    /// Build key for state cell
    fn key_for(&self, branch_id: &BranchId, name: &str) -> Key {
        Key::new_state(self.namespace_for_branch(branch_id), name)
    }

    // ========== Read/Init Operations ==========

    /// Initialize a cell with a value (only if it doesn't exist)
    ///
    /// Returns `Versioned<Version>` containing the version with metadata.
    /// The version uses `Version::Counter` type.
    ///
    /// # StateCell Versioned Returns
    pub fn init(&self, branch_id: &BranchId, name: &str, value: Value) -> StrataResult<Versioned<Version>> {
        self.db.transaction(*branch_id, |txn| {
            let key = self.key_for(branch_id, name);

            // Idempotent: if cell already exists, return existing version
            if let Some(existing) = txn.get(&key)? {
                let state: State = from_stored_value(&existing)?;
                return Ok(Versioned::new(state.version, state.version));
            }

            // Create new state
            let state = State::new(value);
            txn.put(key, to_stored_value(&state)?)?;
            Ok(Versioned::new(state.version, state.version))
        })
    }

    /// Read current state value.
    ///
    /// Returns the user value, or `None` if the cell doesn't exist.
    /// Use `readv()` to access version metadata and history.
    pub fn read(&self, branch_id: &BranchId, name: &str) -> StrataResult<Option<Value>> {
        let key = self.key_for(branch_id, name);

        self.db.transaction(*branch_id, |txn| {
            match txn.get(&key)? {
                Some(v) => {
                    let state: State = from_stored_value(&v)
                        .map_err(|e| strata_core::StrataError::serialization(e.to_string()))?;
                    Ok(Some(state.value))
                }
                None => Ok(None),
            }
        })
    }

    /// Get full version history for a state cell.
    ///
    /// Returns `None` if the cell doesn't exist. Index with `[0]` = latest,
    /// `[1]` = previous, etc. Reads directly from storage (non-transactional).
    ///
    /// Returns `VersionedHistory<Value>` — the internal `State` wrapper is
    /// unwrapped so callers see the user value with storage-layer version/timestamp.
    pub fn readv(&self, branch_id: &BranchId, name: &str) -> StrataResult<Option<VersionedHistory<Value>>> {
        let key = self.key_for(branch_id, name);
        let history = self.db.get_history(&key, None, None)?;
        let versions: Vec<Versioned<Value>> = history
            .iter()
            .filter_map(|vv| {
                let state: State = from_stored_value(&vv.value).ok()?;
                Some(Versioned::with_timestamp(
                    state.value,
                    state.version,
                    Timestamp::from_micros(state.updated_at),
                ))
            })
            .collect();
        Ok(VersionedHistory::new(versions))
    }

    // ========== CAS & Set Operations ==========

    /// Compare-and-swap: Update only if version matches
    ///
    /// Returns `Versioned<Version>` containing new version on success.
    /// Uses `Version::Counter` type.
    ///
    /// # StateCell Versioned Returns
    pub fn cas(
        &self,
        branch_id: &BranchId,
        name: &str,
        expected_version: Version,
        new_value: Value,
    ) -> StrataResult<Versioned<Version>> {
        self.db.transaction(*branch_id, |txn| {
            let key = self.key_for(branch_id, name);

            let current: State = match txn.get(&key)? {
                Some(v) => from_stored_value(&v)
                    .map_err(|e| strata_core::StrataError::serialization(e.to_string()))?,
                None => {
                    return Err(strata_core::StrataError::invalid_input(format!(
                        "StateCell '{}' not found",
                        name
                    )))
                }
            };

            if current.version != expected_version {
                return Err(strata_core::StrataError::conflict(format!(
                    "Version mismatch: expected {:?}, got {:?}",
                    expected_version, current.version
                )));
            }

            let new_version = current.version.increment();
            let new_state = State {
                value: new_value,
                version: new_version,
                updated_at: State::now(),
            };

            txn.put(key, to_stored_value(&new_state)?)?;
            Ok(Versioned::new(new_state.version, new_state.version))
        })
    }

    /// Unconditional set (force write)
    ///
    /// Always succeeds, overwrites any existing value.
    /// Creates the cell if it doesn't exist.
    ///
    /// # StateCell Versioned Returns
    pub fn set(&self, branch_id: &BranchId, name: &str, value: Value) -> StrataResult<Versioned<Version>> {
        let value_for_index = value.clone();
        let result = self.db.transaction(*branch_id, |txn| {
            let key = self.key_for(branch_id, name);

            let new_version = match txn.get(&key)? {
                Some(v) => {
                    let current: State = from_stored_value(&v).map_err(|e| {
                        strata_core::StrataError::serialization(e.to_string())
                    })?;
                    current.version.increment()
                }
                None => Version::counter(1),
            };

            let new_state = State {
                value,
                version: new_version,
                updated_at: State::now(),
            };

            txn.put(key, to_stored_value(&new_state)?)?;
            Ok(Versioned::new(new_state.version, new_state.version))
        })?;

        // Update inverted index (zero overhead when disabled)
        let index = self.db.extension::<crate::search::InvertedIndex>();
        if index.is_enabled() {
            let text = format!("{} {}", name, serde_json::to_string(&value_for_index).unwrap_or_default());
            let entity_ref = crate::search::EntityRef::State {
                branch_id: *branch_id,
                name: name.to_string(),
            };
            index.index_document(&entity_ref, &text, None);
        }

        Ok(result)
    }

}

// ========== Searchable Trait Implementation ==========

impl crate::search::Searchable for StateCell {
    fn search(
        &self,
        _req: &crate::SearchRequest,
    ) -> strata_core::StrataResult<crate::SearchResponse> {
        // StateCell does not support search in MVP
        Ok(crate::SearchResponse::empty())
    }

    fn primitive_kind(&self) -> strata_core::PrimitiveType {
        strata_core::PrimitiveType::State
    }
}

// ========== StateCellExt Implementation ==========

impl StateCellExt for TransactionContext {
    fn state_read(&mut self, name: &str) -> StrataResult<Option<Value>> {
        let ns = Namespace::for_branch(self.branch_id);
        let key = Key::new_state(ns, name);

        match self.get(&key)? {
            Some(v) => {
                let state: State = from_stored_value(&v)
                    .map_err(|e| strata_core::StrataError::serialization(e.to_string()))?;
                Ok(Some(state.value))
            }
            None => Ok(None),
        }
    }

    fn state_cas(&mut self, name: &str, expected_version: Version, new_value: Value) -> StrataResult<Version> {
        let ns = Namespace::for_branch(self.branch_id);
        let key = Key::new_state(ns, name);

        let current: State = match self.get(&key)? {
            Some(v) => from_stored_value(&v)
                .map_err(|e| strata_core::StrataError::serialization(e.to_string()))?,
            None => {
                return Err(strata_core::StrataError::invalid_input(format!(
                    "StateCell '{}' not found",
                    name
                )))
            }
        };

        if current.version != expected_version {
            return Err(strata_core::StrataError::conflict(format!(
                "Version mismatch: expected {:?}, got {:?}",
                expected_version, current.version
            )));
        }

        let new_version = current.version.increment();
        let new_state = State {
            value: new_value,
            version: new_version,
            updated_at: State::now(),
        };

        self.put(key, to_stored_value(&new_state)?)?;
        Ok(new_version)
    }

    fn state_set(&mut self, name: &str, value: Value) -> StrataResult<Version> {
        let ns = Namespace::for_branch(self.branch_id);
        let key = Key::new_state(ns, name);

        let new_version = match self.get(&key)? {
            Some(v) => {
                let current: State = from_stored_value(&v)
                    .map_err(|e| strata_core::StrataError::serialization(e.to_string()))?;
                current.version.increment()
            }
            None => Version::counter(1),
        };

        let new_state = State {
            value,
            version: new_version,
            updated_at: State::now(),
        };

        self.put(key, to_stored_value(&new_state)?)?;
        Ok(new_version)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, Arc<Database>, StateCell) {
        let temp_dir = TempDir::new().unwrap();
        let db = Database::open(temp_dir.path()).unwrap();
        let sc = StateCell::new(db.clone());
        (temp_dir, db, sc)
    }

    // ========== Core & State Structure Tests ==========

    #[test]
    fn test_state_creation() {
        let state = State::new(Value::Int(42));
        assert_eq!(state.version, Version::counter(1));
        assert!(state.updated_at > 0);
        assert_eq!(state.value, Value::Int(42));
    }

    #[test]
    fn test_state_serialization() {
        let state = State::new(Value::String("test".into()));
        let json = serde_json::to_string(&state).unwrap();
        let restored: State = serde_json::from_str(&json).unwrap();
        assert_eq!(state.value, restored.value);
        assert_eq!(state.version, restored.version);
    }

    #[test]
    fn test_statecell_is_clone() {
        let (_temp, _db, sc) = setup();
        let _sc2 = sc.clone();
    }

    #[test]
    fn test_statecell_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<StateCell>();
    }

    // ========== Read/Init Tests ==========

    #[test]
    fn test_init_and_read() {
        let (_temp, _db, sc) = setup();
        let branch_id = BranchId::new();

        let versioned = sc.init(&branch_id, "counter", Value::Int(0)).unwrap();
        assert_eq!(versioned.value, Version::counter(1));
        assert!(versioned.version.is_counter());

        let value = sc.read(&branch_id, "counter").unwrap().unwrap();
        assert_eq!(value, Value::Int(0));
    }

    #[test]
    fn test_init_is_idempotent() {
        let (_temp, _db, sc) = setup();
        let branch_id = BranchId::new();

        let v1 = sc.init(&branch_id, "cell", Value::Int(42)).unwrap();
        // Second init with different value should succeed but return existing version
        let v2 = sc.init(&branch_id, "cell", Value::Int(99)).unwrap();
        assert_eq!(v1.version, v2.version, "Idempotent init should return same version");
    }

    #[test]
    fn test_read_nonexistent() {
        let (_temp, _db, sc) = setup();
        let branch_id = BranchId::new();

        let result = sc.read(&branch_id, "nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_branch_isolation() {
        let (_temp, _db, sc) = setup();
        let branch1 = BranchId::new();
        let branch2 = BranchId::new();

        sc.init(&branch1, "shared", Value::Int(1)).unwrap();
        sc.init(&branch2, "shared", Value::Int(2)).unwrap();

        let value1 = sc.read(&branch1, "shared").unwrap().unwrap();
        let value2 = sc.read(&branch2, "shared").unwrap().unwrap();

        assert_eq!(value1, Value::Int(1));
        assert_eq!(value2, Value::Int(2));
    }

    // ========== CAS & Set Tests ==========

    #[test]
    fn test_cas_success() {
        let (_temp, _db, sc) = setup();
        let branch_id = BranchId::new();

        sc.init(&branch_id, "counter", Value::Int(0)).unwrap();

        // CAS with correct version
        let new_versioned = sc.cas(&branch_id, "counter", Version::counter(1), Value::Int(1)).unwrap();
        assert_eq!(new_versioned.value, Version::counter(2));
        assert!(new_versioned.version.is_counter());

        let value = sc.read(&branch_id, "counter").unwrap().unwrap();
        assert_eq!(value, Value::Int(1));
    }

    #[test]
    fn test_cas_conflict() {
        let (_temp, _db, sc) = setup();
        let branch_id = BranchId::new();

        sc.init(&branch_id, "counter", Value::Int(0)).unwrap();

        // CAS with wrong version
        let result = sc.cas(&branch_id, "counter", Version::counter(999), Value::Int(1));
        assert!(matches!(
            result,
            Err(strata_core::StrataError::Conflict { .. })
        ));
    }

    #[test]
    fn test_cas_not_found() {
        let (_temp, _db, sc) = setup();
        let branch_id = BranchId::new();

        let result = sc.cas(&branch_id, "nonexistent", Version::counter(1), Value::Int(1));
        assert!(result.is_err());
    }

    #[test]
    fn test_set_creates_if_not_exists() {
        let (_temp, _db, sc) = setup();
        let branch_id = BranchId::new();

        let versioned = sc.set(&branch_id, "new-cell", Value::Int(42)).unwrap();
        assert_eq!(versioned.value, Version::counter(1));

        let value = sc.read(&branch_id, "new-cell").unwrap().unwrap();
        assert_eq!(value, Value::Int(42));
    }

    #[test]
    fn test_set_overwrites() {
        let (_temp, _db, sc) = setup();
        let branch_id = BranchId::new();

        sc.init(&branch_id, "cell", Value::Int(1)).unwrap();
        let versioned = sc.set(&branch_id, "cell", Value::Int(100)).unwrap();
        assert_eq!(versioned.value, Version::counter(2));

        let value = sc.read(&branch_id, "cell").unwrap().unwrap();
        assert_eq!(value, Value::Int(100));
    }

    #[test]
    fn test_version_monotonicity() {
        let (_temp, _db, sc) = setup();
        let branch_id = BranchId::new();

        sc.init(&branch_id, "cell", Value::Int(0)).unwrap();

        for i in 1..=10 {
            let v = sc.set(&branch_id, "cell", Value::Int(i)).unwrap();
            assert_eq!(v.value, Version::counter((i + 1) as u64));
        }

        let value = sc.read(&branch_id, "cell").unwrap().unwrap();
        assert_eq!(value, Value::Int(10));
    }

    // ========== StateCellExt Tests ==========

    #[test]
    fn test_statecell_ext_read() {
        let (_temp, db, sc) = setup();
        let branch_id = BranchId::new();

        sc.init(&branch_id, "cell", Value::String("hello".into()))
            .unwrap();

        let result = db
            .transaction(branch_id, |txn| {
                let value = txn.state_read("cell")?;
                Ok(value)
            })
            .unwrap();

        assert_eq!(result, Some(Value::String("hello".into())));
    }

    #[test]
    fn test_statecell_ext_read_not_found() {
        let (_temp, db, _sc) = setup();
        let branch_id = BranchId::new();

        let result = db
            .transaction(branch_id, |txn| {
                let value = txn.state_read("nonexistent")?;
                Ok(value)
            })
            .unwrap();

        assert!(result.is_none());
    }

    #[test]
    fn test_statecell_ext_cas() {
        let (_temp, db, sc) = setup();
        let branch_id = BranchId::new();

        sc.init(&branch_id, "cell", Value::Int(1)).unwrap();

        let new_version = db
            .transaction(branch_id, |txn| txn.state_cas("cell", Version::counter(1), Value::Int(2)))
            .unwrap();

        assert_eq!(new_version, Version::counter(2));

        let value = sc.read(&branch_id, "cell").unwrap().unwrap();
        assert_eq!(value, Value::Int(2));
    }

    #[test]
    fn test_statecell_ext_set() {
        let (_temp, db, sc) = setup();
        let branch_id = BranchId::new();

        let version = db
            .transaction(branch_id, |txn| txn.state_set("new-cell", Value::Int(42)))
            .unwrap();

        assert_eq!(version, Version::counter(1));

        let value = sc.read(&branch_id, "new-cell").unwrap().unwrap();
        assert_eq!(value, Value::Int(42));
    }

    #[test]
    fn test_cross_primitive_transaction() {
        use crate::primitives::extensions::KVStoreExt;

        let (_temp, db, sc) = setup();
        let branch_id = BranchId::new();

        sc.init(&branch_id, "counter", Value::Int(0)).unwrap();

        // Combine KV and StateCell in single transaction
        db.transaction(branch_id, |txn| {
            txn.kv_put("key", Value::String("value".into()))?;
            txn.state_set("counter", Value::Int(1))?;
            Ok(())
        })
        .unwrap();

        // Verify both were written
        let value = sc.read(&branch_id, "counter").unwrap().unwrap();
        assert_eq!(value, Value::Int(1));
    }

    // ========== Read Tests ==========

    #[test]
    fn test_read_returns_correct_value() {
        let (_temp, _db, sc) = setup();
        let branch_id = BranchId::new();

        sc.init(&branch_id, "cell", Value::Int(42)).unwrap();

        let value = sc.read(&branch_id, "cell").unwrap().unwrap();
        assert_eq!(value, Value::Int(42));
    }

    #[test]
    fn test_read_returns_none_for_missing() {
        let (_temp, _db, sc) = setup();
        let branch_id = BranchId::new();

        let value = sc.read(&branch_id, "nonexistent").unwrap();
        assert!(value.is_none());
    }

    #[test]
    fn test_read_branch_isolation() {
        let (_temp, _db, sc) = setup();
        let branch1 = BranchId::new();
        let branch2 = BranchId::new();

        sc.init(&branch1, "shared", Value::Int(1)).unwrap();
        sc.init(&branch2, "shared", Value::Int(2)).unwrap();

        let value1 = sc.read(&branch1, "shared").unwrap().unwrap();
        let value2 = sc.read(&branch2, "shared").unwrap().unwrap();

        assert_eq!(value1, Value::Int(1));
        assert_eq!(value2, Value::Int(2));
    }

    // ========== Versioned Returns Tests ==========

    #[test]
    fn test_versioned_init_has_counter_version() {
        let (_temp, _db, sc) = setup();
        let branch_id = BranchId::new();

        let versioned = sc.init(&branch_id, "cell", Value::Int(0)).unwrap();
        assert_eq!(versioned.value, Version::counter(1));
        assert!(versioned.version.is_counter());
        assert_eq!(versioned.version, Version::counter(1));
    }

    #[test]
    fn test_readv_has_counter_version() {
        let (_temp, _db, sc) = setup();
        let branch_id = BranchId::new();

        sc.init(&branch_id, "cell", Value::Int(42)).unwrap();
        let history = sc.readv(&branch_id, "cell").unwrap().unwrap();

        assert!(history.version().is_counter());
        assert_eq!(history.version(), Version::counter(1));
        assert!(history.timestamp().as_micros() > 0);
        assert_eq!(*history.value(), Value::Int(42));
    }

    #[test]
    fn test_versioned_cas_has_counter_version() {
        let (_temp, _db, sc) = setup();
        let branch_id = BranchId::new();

        sc.init(&branch_id, "cell", Value::Int(0)).unwrap();
        let versioned = sc.cas(&branch_id, "cell", Version::counter(1), Value::Int(1)).unwrap();

        assert!(versioned.version.is_counter());
        assert_eq!(versioned.version, Version::counter(2));
    }
}
