//! Facade API Implementation
//!
//! This module provides concrete implementations of all Facade API traits.
//! `FacadeImpl` wraps `SubstrateImpl` and provides Redis-like convenience.
//!
//! ## Design
//!
//! The Facade is syntactic sugar over the Substrate:
//! - Implicit default run targeting
//! - Auto-commit for each operation
//! - Simple return types (strips version info by default)
//!
//! ## Desugaring
//!
//! Every facade call desugars to exactly one substrate call pattern.
//! No magic, no hidden semantics.

use std::sync::Arc;
use strata_core::{Value, Version};

use super::types::FacadeConfig;

use crate::substrate::{
    ApiRunId, SubstrateImpl,
};

// =============================================================================
// FacadeImpl
// =============================================================================

/// Facade API Implementation
///
/// Wraps `SubstrateImpl` to provide Redis-like convenience.
/// All operations target the default run and auto-commit.
pub struct FacadeImpl {
    /// The underlying substrate implementation
    substrate: Arc<SubstrateImpl>,

    /// Facade configuration
    #[allow(dead_code)]
    config: FacadeConfig,

    /// Default run ID for all operations
    default_run: ApiRunId,
}

impl FacadeImpl {
    /// Create a new facade implementation
    pub fn new(substrate: Arc<SubstrateImpl>) -> Self {
        FacadeImpl {
            substrate,
            config: FacadeConfig::default(),
            default_run: ApiRunId::default(),
        }
    }

    /// Create a new facade with custom configuration
    pub fn with_config(substrate: Arc<SubstrateImpl>, config: FacadeConfig) -> Self {
        FacadeImpl {
            substrate,
            config,
            default_run: ApiRunId::default(),
        }
    }

    /// Create a facade scoped to a specific run
    pub fn with_run(substrate: Arc<SubstrateImpl>, run_id: ApiRunId) -> Self {
        FacadeImpl {
            substrate,
            config: FacadeConfig::default(),
            default_run: run_id,
        }
    }

    /// Get a reference to the underlying substrate
    pub fn substrate(&self) -> &SubstrateImpl {
        &self.substrate
    }

    /// Get the default run ID
    pub(crate) fn default_run(&self) -> &ApiRunId {
        &self.default_run
    }

    /// Get the substrate arc (for scoped facades)
    pub(crate) fn substrate_arc(&self) -> Arc<SubstrateImpl> {
        Arc::clone(&self.substrate)
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Convert Version to u64
pub(crate) fn version_to_u64(version: &Version) -> u64 {
    match version {
        Version::Txn(txn) => *txn,
        Version::Sequence(seq) => *seq,
        Version::Counter(cnt) => *cnt,
    }
}

/// Get the type name of a Value
pub(crate) fn value_type_name(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(_) => "boolean".to_string(),
        Value::Int(_) => "integer".to_string(),
        Value::Float(_) => "number".to_string(),
        Value::String(_) => "string".to_string(),
        Value::Bytes(_) => "bytes".to_string(),
        Value::Array(_) => "array".to_string(),
        Value::Object(_) => "object".to_string(),
    }
}

/// Merge two Values using JSON Merge Patch semantics (RFC 7396)
pub(crate) fn merge_values(base: Value, patch: Value) -> Value {
    match (base, patch) {
        (Value::Object(mut base_map), Value::Object(patch_map)) => {
            for (key, patch_value) in patch_map {
                if matches!(patch_value, Value::Null) {
                    base_map.remove(&key);
                } else if let Some(base_value) = base_map.remove(&key) {
                    base_map.insert(key, merge_values(base_value, patch_value));
                } else {
                    base_map.insert(key, patch_value);
                }
            }
            Value::Object(base_map)
        }
        (_, patch) => patch, // Non-object patches replace entirely
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_to_u64() {
        assert_eq!(version_to_u64(&Version::Txn(42)), 42);
    }

    #[test]
    fn test_merge_values_objects() {
        use std::collections::HashMap;

        let base = Value::Object(HashMap::from([
            ("a".to_string(), Value::Int(1)),
            ("b".to_string(), Value::Int(2)),
        ]));

        let patch = Value::Object(HashMap::from([
            ("b".to_string(), Value::Int(3)),
            ("c".to_string(), Value::Int(4)),
        ]));

        let result = merge_values(base, patch);

        if let Value::Object(map) = result {
            assert_eq!(map.get("a"), Some(&Value::Int(1)));
            assert_eq!(map.get("b"), Some(&Value::Int(3)));
            assert_eq!(map.get("c"), Some(&Value::Int(4)));
        } else {
            panic!("Expected object");
        }
    }

    #[test]
    fn test_merge_values_null_deletes() {
        use std::collections::HashMap;

        let base = Value::Object(HashMap::from([
            ("a".to_string(), Value::Int(1)),
            ("b".to_string(), Value::Int(2)),
        ]));

        let patch = Value::Object(HashMap::from([
            ("b".to_string(), Value::Null),
        ]));

        let result = merge_values(base, patch);

        if let Value::Object(map) = result {
            assert_eq!(map.get("a"), Some(&Value::Int(1)));
            assert!(!map.contains_key("b"));
        } else {
            panic!("Expected object");
        }
    }

}
