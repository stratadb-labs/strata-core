//! Secondary indices for efficient query patterns
//!
//! This module provides secondary indices that enable efficient queries
//! without scanning the entire BTreeMap:
//! - RunIndex: Maps RunId → Set<Key> for fast run-scoped queries (critical for replay)
//! - TypeIndex: Maps TypeTag → Set<Key> for primitive-specific queries

use strata_core::{Key, RunId, TypeTag};
use std::collections::{HashMap, HashSet};

/// Secondary index: RunId → Keys
///
/// Enables efficient scan_by_run queries for replay by maintaining
/// a mapping from each RunId to all keys belonging to that run.
/// This changes scan_by_run from O(total data) to O(run size).
#[derive(Debug, Default)]
pub struct RunIndex {
    index: HashMap<RunId, HashSet<Key>>,
}

impl RunIndex {
    /// Create a new empty RunIndex
    pub fn new() -> Self {
        Self {
            index: HashMap::new(),
        }
    }

    /// Add key to run's index
    ///
    /// Inserts the key into the set of keys for the given run_id.
    /// If the run_id doesn't exist yet, creates a new entry.
    pub fn insert(&mut self, run_id: RunId, key: Key) {
        self.index.entry(run_id).or_default().insert(key);
    }

    /// Remove key from run's index
    ///
    /// Removes the key from the set for the given run_id.
    /// If the set becomes empty, removes the run_id entry entirely
    /// to avoid accumulating empty sets.
    pub fn remove(&mut self, run_id: RunId, key: &Key) {
        if let Some(keys) = self.index.get_mut(&run_id) {
            keys.remove(key);
            if keys.is_empty() {
                self.index.remove(&run_id);
            }
        }
    }

    /// Get all keys for a run
    ///
    /// Returns a reference to the set of keys for the given run_id,
    /// or None if no keys exist for that run.
    pub fn get(&self, run_id: &RunId) -> Option<&HashSet<Key>> {
        self.index.get(run_id)
    }

    /// Remove all keys for a run (for cleanup)
    ///
    /// Removes the entire entry for a run_id, useful for
    /// cleaning up after a run is complete.
    pub fn remove_run(&mut self, run_id: &RunId) {
        self.index.remove(run_id);
    }

    /// Check if the index is empty
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// Get the number of runs in the index
    pub fn len(&self) -> usize {
        self.index.len()
    }
}

/// Secondary index: TypeTag → Keys
///
/// Enables efficient queries by primitive type by maintaining
/// a mapping from each TypeTag to all keys of that type.
/// Useful for queries like "all events" or "all KV entries".
#[derive(Debug, Default)]
pub struct TypeIndex {
    index: HashMap<TypeTag, HashSet<Key>>,
}

impl TypeIndex {
    /// Create a new empty TypeIndex
    pub fn new() -> Self {
        Self {
            index: HashMap::new(),
        }
    }

    /// Add key to type's index
    ///
    /// Inserts the key into the set of keys for the given type_tag.
    /// If the type_tag doesn't exist yet, creates a new entry.
    pub fn insert(&mut self, type_tag: TypeTag, key: Key) {
        self.index.entry(type_tag).or_default().insert(key);
    }

    /// Remove key from type's index
    ///
    /// Removes the key from the set for the given type_tag.
    /// If the set becomes empty, removes the type_tag entry entirely.
    pub fn remove(&mut self, type_tag: TypeTag, key: &Key) {
        if let Some(keys) = self.index.get_mut(&type_tag) {
            keys.remove(key);
            if keys.is_empty() {
                self.index.remove(&type_tag);
            }
        }
    }

    /// Get all keys for a type
    ///
    /// Returns a reference to the set of keys for the given type_tag,
    /// or None if no keys exist for that type.
    pub fn get(&self, type_tag: &TypeTag) -> Option<&HashSet<Key>> {
        self.index.get(type_tag)
    }

    /// Check if the index is empty
    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    /// Get the number of types in the index
    pub fn len(&self) -> usize {
        self.index.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strata_core::Namespace;

    /// Helper to create a test namespace
    fn test_namespace(run_id: RunId) -> Namespace {
        Namespace::new(
            "tenant".to_string(),
            "app".to_string(),
            "agent".to_string(),
            run_id,
        )
    }

    // ========================================
    // RunIndex Tests
    // ========================================

    #[test]
    fn test_run_index_insert_and_get() {
        let mut index = RunIndex::new();
        let run_id = RunId::new();
        let ns = test_namespace(run_id);
        let key1 = Key::new_kv(ns.clone(), "key1");
        let key2 = Key::new_kv(ns.clone(), "key2");

        // Insert two keys for the same run
        index.insert(run_id, key1.clone());
        index.insert(run_id, key2.clone());

        // Verify both keys are in the index
        let keys = index.get(&run_id).unwrap();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&key1));
        assert!(keys.contains(&key2));
    }

    #[test]
    fn test_run_index_remove() {
        let mut index = RunIndex::new();
        let run_id = RunId::new();
        let ns = test_namespace(run_id);
        let key1 = Key::new_kv(ns.clone(), "key1");
        let key2 = Key::new_kv(ns.clone(), "key2");

        // Insert two keys
        index.insert(run_id, key1.clone());
        index.insert(run_id, key2.clone());

        // Remove one key
        index.remove(run_id, &key1);

        // Verify only key2 remains
        let keys = index.get(&run_id).unwrap();
        assert_eq!(keys.len(), 1);
        assert!(!keys.contains(&key1));
        assert!(keys.contains(&key2));

        // Remove the last key - set should be cleaned up
        index.remove(run_id, &key2);
        assert!(index.get(&run_id).is_none());
        assert!(index.is_empty());
    }

    #[test]
    fn test_run_index_multiple_runs() {
        let mut index = RunIndex::new();
        let run1 = RunId::new();
        let run2 = RunId::new();
        let ns1 = test_namespace(run1);
        let ns2 = test_namespace(run2);

        let key1 = Key::new_kv(ns1.clone(), "key1");
        let key2 = Key::new_kv(ns2.clone(), "key2");

        index.insert(run1, key1.clone());
        index.insert(run2, key2.clone());

        // Verify each run has its own key
        assert_eq!(index.get(&run1).unwrap().len(), 1);
        assert_eq!(index.get(&run2).unwrap().len(), 1);
        assert!(index.get(&run1).unwrap().contains(&key1));
        assert!(index.get(&run2).unwrap().contains(&key2));
        assert_eq!(index.len(), 2);
    }

    #[test]
    fn test_run_index_remove_run() {
        let mut index = RunIndex::new();
        let run_id = RunId::new();
        let ns = test_namespace(run_id);

        index.insert(run_id, Key::new_kv(ns.clone(), "key1"));
        index.insert(run_id, Key::new_kv(ns.clone(), "key2"));

        // Remove entire run
        index.remove_run(&run_id);

        assert!(index.get(&run_id).is_none());
        assert!(index.is_empty());
    }

    #[test]
    fn test_run_index_default() {
        let index = RunIndex::default();
        assert!(index.is_empty());
        assert_eq!(index.len(), 0);
    }

    // ========================================
    // TypeIndex Tests
    // ========================================

    #[test]
    fn test_type_index_insert_and_get() {
        let mut index = TypeIndex::new();
        let run_id = RunId::new();
        let ns = test_namespace(run_id);
        let key1 = Key::new_kv(ns.clone(), "key1");
        let key2 = Key::new_kv(ns.clone(), "key2");

        // Insert two KV keys
        index.insert(TypeTag::KV, key1.clone());
        index.insert(TypeTag::KV, key2.clone());

        // Verify both keys are in the index
        let keys = index.get(&TypeTag::KV).unwrap();
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&key1));
        assert!(keys.contains(&key2));
    }

    #[test]
    fn test_type_index_remove() {
        let mut index = TypeIndex::new();
        let run_id = RunId::new();
        let ns = test_namespace(run_id);
        let key1 = Key::new_kv(ns.clone(), "key1");
        let key2 = Key::new_kv(ns.clone(), "key2");

        // Insert two keys
        index.insert(TypeTag::KV, key1.clone());
        index.insert(TypeTag::KV, key2.clone());

        // Remove one key
        index.remove(TypeTag::KV, &key1);

        // Verify only key2 remains
        let keys = index.get(&TypeTag::KV).unwrap();
        assert_eq!(keys.len(), 1);
        assert!(!keys.contains(&key1));
        assert!(keys.contains(&key2));

        // Remove the last key - set should be cleaned up
        index.remove(TypeTag::KV, &key2);
        assert!(index.get(&TypeTag::KV).is_none());
        assert!(index.is_empty());
    }

    #[test]
    fn test_type_index_multiple_types() {
        let mut index = TypeIndex::new();
        let run_id = RunId::new();
        let ns = test_namespace(run_id);

        let kv_key = Key::new_kv(ns.clone(), "data");
        let event_key = Key::new_event(ns.clone(), 1);
        let trace_key = Key::new_trace(ns.clone(), 1);

        index.insert(TypeTag::KV, kv_key.clone());
        index.insert(TypeTag::Event, event_key.clone());
        index.insert(TypeTag::Trace, trace_key.clone());

        // Verify each type has its own key
        assert_eq!(index.get(&TypeTag::KV).unwrap().len(), 1);
        assert_eq!(index.get(&TypeTag::Event).unwrap().len(), 1);
        assert_eq!(index.get(&TypeTag::Trace).unwrap().len(), 1);
        assert!(index.get(&TypeTag::KV).unwrap().contains(&kv_key));
        assert!(index.get(&TypeTag::Event).unwrap().contains(&event_key));
        assert!(index.get(&TypeTag::Trace).unwrap().contains(&trace_key));
        assert_eq!(index.len(), 3);
    }

    #[test]
    fn test_type_index_default() {
        let index = TypeIndex::default();
        assert!(index.is_empty());
        assert_eq!(index.len(), 0);
    }
}
