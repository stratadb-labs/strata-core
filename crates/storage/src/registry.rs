//! Primitive registry for dynamic primitive handling
//!
//! The registry allows new primitives to be registered without modifying
//! the recovery engine or snapshot format.
//!
//! ## Usage
//!
//! ```rust,ignore
//! let mut registry = PrimitiveRegistry::new();
//!
//! // Register primitives
//! registry.register(Arc::new(KvStorageExt::new(kv)));
//! registry.register(Arc::new(JsonStorageExt::new(json)));
//!
//! // Look up by type ID
//! let prim = registry.get(1);  // KV
//!
//! // Look up by WAL entry type
//! let prim = registry.get_for_wal_type(0x10);  // KvPut -> KV
//!
//! // Check if entry type is known
//! if !registry.knows_entry_type(0xFF) {
//!     warn!("Unknown entry type");
//! }
//! ```

use crate::primitive_ext::PrimitiveStorageExt;
use std::collections::HashMap;
use std::sync::Arc;

/// Registry of primitives for recovery/snapshot
///
/// Maintains mappings from:
/// - Primitive type ID -> Primitive instance
/// - WAL entry type -> Primitive type ID
///
/// This allows the recovery engine to route WAL entries to the
/// correct primitive without hardcoding the primitive types.
pub struct PrimitiveRegistry {
    /// Primitives by type ID
    primitives: HashMap<u8, Arc<dyn PrimitiveStorageExt>>,
    /// WAL entry type -> Primitive type ID mapping
    wal_type_to_primitive: HashMap<u8, u8>,
}

impl PrimitiveRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        PrimitiveRegistry {
            primitives: HashMap::new(),
            wal_type_to_primitive: HashMap::new(),
        }
    }

    /// Register a primitive
    ///
    /// Maps the primitive's type ID and all its WAL entry types.
    pub fn register(&mut self, primitive: Arc<dyn PrimitiveStorageExt>) {
        let type_id = primitive.primitive_type_id();

        // Map WAL entry types to this primitive
        for &wal_type in primitive.wal_entry_types() {
            self.wal_type_to_primitive.insert(wal_type, type_id);
        }

        self.primitives.insert(type_id, primitive);
    }

    /// Get primitive by type ID
    pub fn get(&self, type_id: u8) -> Option<Arc<dyn PrimitiveStorageExt>> {
        self.primitives.get(&type_id).cloned()
    }

    /// Get primitive for a WAL entry type
    ///
    /// Returns the primitive that handles this entry type, or None
    /// if the entry type is not registered.
    pub fn get_for_wal_type(&self, wal_type: u8) -> Option<Arc<dyn PrimitiveStorageExt>> {
        self.wal_type_to_primitive
            .get(&wal_type)
            .and_then(|&type_id| self.primitives.get(&type_id))
            .cloned()
    }

    /// Check if an entry type is known
    ///
    /// Returns true if a primitive is registered that handles this entry type.
    pub fn knows_entry_type(&self, wal_type: u8) -> bool {
        self.wal_type_to_primitive.contains_key(&wal_type)
    }

    /// Get primitive type ID for a WAL entry type
    ///
    /// Returns the type ID of the primitive that handles this entry type.
    pub fn type_id_for_wal_type(&self, wal_type: u8) -> Option<u8> {
        self.wal_type_to_primitive.get(&wal_type).copied()
    }

    /// List all registered primitives
    pub fn list(&self) -> Vec<Arc<dyn PrimitiveStorageExt>> {
        self.primitives.values().cloned().collect()
    }

    /// Get all registered type IDs
    pub fn type_ids(&self) -> Vec<u8> {
        let mut ids: Vec<u8> = self.primitives.keys().copied().collect();
        ids.sort();
        ids
    }

    /// Get all registered WAL entry types
    pub fn wal_types(&self) -> Vec<u8> {
        let mut types: Vec<u8> = self.wal_type_to_primitive.keys().copied().collect();
        types.sort();
        types
    }

    /// Check if a primitive type is registered
    pub fn is_registered(&self, type_id: u8) -> bool {
        self.primitives.contains_key(&type_id)
    }

    /// Get the number of registered primitives
    pub fn len(&self) -> usize {
        self.primitives.len()
    }

    /// Check if the registry is empty
    pub fn is_empty(&self) -> bool {
        self.primitives.is_empty()
    }

    /// Unregister a primitive by type ID
    ///
    /// Removes the primitive and all its WAL entry type mappings.
    pub fn unregister(&mut self, type_id: u8) -> Option<Arc<dyn PrimitiveStorageExt>> {
        if let Some(primitive) = self.primitives.remove(&type_id) {
            // Remove WAL entry type mappings
            for &wal_type in primitive.wal_entry_types() {
                self.wal_type_to_primitive.remove(&wal_type);
            }
            Some(primitive)
        } else {
            None
        }
    }

    /// Clear all registered primitives
    pub fn clear(&mut self) {
        self.primitives.clear();
        self.wal_type_to_primitive.clear();
    }
}

impl Default for PrimitiveRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for PrimitiveRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrimitiveRegistry")
            .field("primitive_count", &self.primitives.len())
            .field("type_ids", &self.type_ids())
            .field("wal_type_count", &self.wal_type_to_primitive.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitive_ext::PrimitiveExtError;

    /// Mock primitive for testing
    struct MockPrimitive {
        type_id: u8,
        name: &'static str,
        wal_types: &'static [u8],
    }

    impl PrimitiveStorageExt for MockPrimitive {
        fn primitive_type_id(&self) -> u8 {
            self.type_id
        }

        fn wal_entry_types(&self) -> &'static [u8] {
            self.wal_types
        }

        fn snapshot_serialize(&self) -> Result<Vec<u8>, PrimitiveExtError> {
            Ok(vec![])
        }

        fn snapshot_deserialize(&mut self, _data: &[u8]) -> Result<(), PrimitiveExtError> {
            Ok(())
        }

        fn apply_wal_entry(
            &mut self,
            _entry_type: u8,
            _payload: &[u8],
        ) -> Result<(), PrimitiveExtError> {
            Ok(())
        }

        fn primitive_name(&self) -> &'static str {
            self.name
        }
    }

    #[test]
    fn test_registry_new() {
        let registry = PrimitiveRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_registry_register_and_get() {
        let mut registry = PrimitiveRegistry::new();

        let mock = Arc::new(MockPrimitive {
            type_id: 1,
            name: "mock",
            wal_types: &[0x10, 0x11],
        });

        registry.register(mock.clone());

        assert!(registry.is_registered(1));
        assert!(!registry.is_registered(2));
        assert_eq!(registry.len(), 1);

        let retrieved = registry.get(1).unwrap();
        assert_eq!(retrieved.primitive_name(), "mock");
    }

    #[test]
    fn test_registry_get_for_wal_type() {
        let mut registry = PrimitiveRegistry::new();

        registry.register(Arc::new(MockPrimitive {
            type_id: 1,
            name: "kv",
            wal_types: &[0x10, 0x11],
        }));

        registry.register(Arc::new(MockPrimitive {
            type_id: 2,
            name: "json",
            wal_types: &[0x20, 0x21, 0x22],
        }));

        // KV types
        assert_eq!(
            registry.get_for_wal_type(0x10).unwrap().primitive_name(),
            "kv"
        );
        assert_eq!(
            registry.get_for_wal_type(0x11).unwrap().primitive_name(),
            "kv"
        );

        // JSON types
        assert_eq!(
            registry.get_for_wal_type(0x20).unwrap().primitive_name(),
            "json"
        );
        assert_eq!(
            registry.get_for_wal_type(0x21).unwrap().primitive_name(),
            "json"
        );
        assert_eq!(
            registry.get_for_wal_type(0x22).unwrap().primitive_name(),
            "json"
        );

        // Unknown type
        assert!(registry.get_for_wal_type(0xFF).is_none());
    }

    #[test]
    fn test_registry_knows_entry_type() {
        let mut registry = PrimitiveRegistry::new();

        registry.register(Arc::new(MockPrimitive {
            type_id: 1,
            name: "mock",
            wal_types: &[0x10, 0x11],
        }));

        assert!(registry.knows_entry_type(0x10));
        assert!(registry.knows_entry_type(0x11));
        assert!(!registry.knows_entry_type(0x12));
        assert!(!registry.knows_entry_type(0xFF));
    }

    #[test]
    fn test_registry_type_ids() {
        let mut registry = PrimitiveRegistry::new();

        registry.register(Arc::new(MockPrimitive {
            type_id: 3,
            name: "three",
            wal_types: &[0x30],
        }));

        registry.register(Arc::new(MockPrimitive {
            type_id: 1,
            name: "one",
            wal_types: &[0x10],
        }));

        registry.register(Arc::new(MockPrimitive {
            type_id: 2,
            name: "two",
            wal_types: &[0x20],
        }));

        // Should be sorted
        assert_eq!(registry.type_ids(), vec![1, 2, 3]);
    }

    #[test]
    fn test_registry_wal_types() {
        let mut registry = PrimitiveRegistry::new();

        registry.register(Arc::new(MockPrimitive {
            type_id: 1,
            name: "mock",
            wal_types: &[0x30, 0x10, 0x20],
        }));

        let wal_types = registry.wal_types();
        assert_eq!(wal_types, vec![0x10, 0x20, 0x30]); // Sorted
    }

    #[test]
    fn test_registry_list() {
        let mut registry = PrimitiveRegistry::new();

        registry.register(Arc::new(MockPrimitive {
            type_id: 1,
            name: "one",
            wal_types: &[0x10],
        }));

        registry.register(Arc::new(MockPrimitive {
            type_id: 2,
            name: "two",
            wal_types: &[0x20],
        }));

        let list = registry.list();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_registry_unregister() {
        let mut registry = PrimitiveRegistry::new();

        registry.register(Arc::new(MockPrimitive {
            type_id: 1,
            name: "mock",
            wal_types: &[0x10, 0x11],
        }));

        assert!(registry.is_registered(1));
        assert!(registry.knows_entry_type(0x10));

        let removed = registry.unregister(1);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().primitive_name(), "mock");

        assert!(!registry.is_registered(1));
        assert!(!registry.knows_entry_type(0x10));
        assert!(!registry.knows_entry_type(0x11));
    }

    #[test]
    fn test_registry_unregister_nonexistent() {
        let mut registry = PrimitiveRegistry::new();
        assert!(registry.unregister(99).is_none());
    }

    #[test]
    fn test_registry_clear() {
        let mut registry = PrimitiveRegistry::new();

        registry.register(Arc::new(MockPrimitive {
            type_id: 1,
            name: "one",
            wal_types: &[0x10],
        }));

        registry.register(Arc::new(MockPrimitive {
            type_id: 2,
            name: "two",
            wal_types: &[0x20],
        }));

        assert_eq!(registry.len(), 2);

        registry.clear();

        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
        assert!(!registry.knows_entry_type(0x10));
        assert!(!registry.knows_entry_type(0x20));
    }

    #[test]
    fn test_registry_debug() {
        let mut registry = PrimitiveRegistry::new();

        registry.register(Arc::new(MockPrimitive {
            type_id: 1,
            name: "mock",
            wal_types: &[0x10, 0x11],
        }));

        let debug = format!("{:?}", registry);
        assert!(debug.contains("PrimitiveRegistry"));
        assert!(debug.contains("primitive_count"));
    }

    #[test]
    fn test_registry_type_id_for_wal_type() {
        let mut registry = PrimitiveRegistry::new();

        registry.register(Arc::new(MockPrimitive {
            type_id: 5,
            name: "mock",
            wal_types: &[0x50, 0x51],
        }));

        assert_eq!(registry.type_id_for_wal_type(0x50), Some(5));
        assert_eq!(registry.type_id_for_wal_type(0x51), Some(5));
        assert_eq!(registry.type_id_for_wal_type(0x52), None);
    }
}
