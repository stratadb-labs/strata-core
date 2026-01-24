//! JSON Facade - Simplified JSON document operations
//!
//! This module provides Redis-like JSON operations (similar to RedisJSON).
//!
//! ## Desugaring
//!
//! | Facade | Substrate |
//! |--------|-----------|
//! | `json_get(key, path)` | `json_get(default_run, key, path).map(\|v\| v.value)` |
//! | `json_getv(key, path)` | `json_get(default_run, key, path)` (document-level version) |
//! | `json_set(key, path, val)` | `json_set(default_run, key, path, val)` |
//! | `json_del(key, path)` | `json_delete(default_run, key, path)` |

use strata_core::{StrataResult, Value};
use super::kv::Versioned;

/// JSON Facade - simplified document operations
///
/// Mirrors RedisJSON-style operations with implicit default run.
///
/// ## Path Syntax
///
/// - `$` - Root document
/// - `$.field` - Object field
/// - `$.array[0]` - Array index
/// - `$.array[-]` - Array append (set only)
pub trait JsonFacade {
    /// Get value at path
    ///
    /// Returns `None` if key or path doesn't exist.
    ///
    /// ## Example
    /// ```ignore
    /// // Get entire document
    /// let doc = facade.json_get("user:1", "$")?;
    ///
    /// // Get nested field
    /// let name = facade.json_get("user:1", "$.profile.name")?;
    /// ```
    fn json_get(&self, key: &str, path: &str) -> StrataResult<Option<Value>>;

    /// Get versioned value at path
    ///
    /// Escape hatch to access version information.
    ///
    /// **Important**: Returns **document-level** version, not subpath version.
    /// Modifying any part of the document updates its version.
    ///
    /// ## Desugars to
    /// ```text
    /// json_get(default_run, key, path)
    /// ```
    ///
    /// ## Example
    /// ```ignore
    /// let versioned = facade.json_getv("user:1", "$.name")?;
    /// if let Some(v) = versioned {
    ///     println!("Value: {:?}, version: {}", v.value, v.version);
    /// }
    /// ```
    fn json_getv(&self, key: &str, path: &str) -> StrataResult<Option<Versioned<Value>>>;

    /// Set value at path
    ///
    /// Creates intermediate objects/arrays as needed.
    ///
    /// ## Example
    /// ```ignore
    /// // Create document
    /// facade.json_set("user:1", "$", json!({"name": "Alice"}))?;
    ///
    /// // Update field
    /// facade.json_set("user:1", "$.age", Value::Int(30))?;
    ///
    /// // Append to array
    /// facade.json_set("user:1", "$.tags[-]", Value::String("vip".into()))?;
    /// ```
    fn json_set(&self, key: &str, path: &str, value: Value) -> StrataResult<()>;

    /// Delete value at path
    ///
    /// Returns count of elements deleted.
    ///
    /// ## Note
    /// Cannot delete root (`$`) - use `del(key)` from KVFacade instead.
    fn json_del(&self, key: &str, path: &str) -> StrataResult<u64>;

    /// Merge patch at path (RFC 7396)
    ///
    /// Applies JSON Merge Patch semantics:
    /// - `null` deletes fields
    /// - Objects merge recursively
    /// - Arrays replace entirely
    fn json_merge(&self, key: &str, path: &str, patch: Value) -> StrataResult<()>;

    /// Get type of value at path
    ///
    /// Returns type name: "object", "array", "string", "integer", "number", "boolean", "null"
    fn json_type(&self, key: &str, path: &str) -> StrataResult<Option<String>>;

    /// Increment numeric value at path
    ///
    /// Returns the new value.
    ///
    /// ## Errors
    /// - `WrongType` if value is not a number
    fn json_numincrby(&self, key: &str, path: &str, delta: f64) -> StrataResult<f64>;

    /// Append to string at path
    ///
    /// Returns new string length.
    fn json_strappend(&self, key: &str, path: &str, suffix: &str) -> StrataResult<usize>;

    /// Append to array at path
    ///
    /// Returns new array length.
    fn json_arrappend(&self, key: &str, path: &str, values: Vec<Value>) -> StrataResult<usize>;

    /// Pop from array at path
    ///
    /// Removes and returns the last element. Returns None if array is empty.
    fn json_arrpop(&self, key: &str, path: &str) -> StrataResult<Option<Value>>;

    /// Get array length at path
    fn json_arrlen(&self, key: &str, path: &str) -> StrataResult<Option<usize>>;

    /// Get object keys at path
    fn json_objkeys(&self, key: &str, path: &str) -> StrataResult<Option<Vec<String>>>;

    /// Get object key count at path
    fn json_objlen(&self, key: &str, path: &str) -> StrataResult<Option<usize>>;
}

// =============================================================================
// Implementation
// =============================================================================

use strata_core::StrataError;
use super::impl_::{FacadeImpl, version_to_u64, value_type_name};
use crate::substrate::JsonStore as SubstrateJsonStore;

impl JsonFacade for FacadeImpl {
    fn json_get(&self, key: &str, path: &str) -> StrataResult<Option<Value>> {
        let result = self.substrate().json_get(self.default_run(), key, path)?;
        Ok(result.map(|v| v.value))
    }

    fn json_getv(&self, key: &str, path: &str) -> StrataResult<Option<Versioned<Value>>> {
        let result = self.substrate().json_get(self.default_run(), key, path)?;
        Ok(result.map(|v| Versioned {
            value: v.value,
            version: version_to_u64(&v.version),
            timestamp: v.timestamp.as_micros(),
        }))
    }

    fn json_set(&self, key: &str, path: &str, value: Value) -> StrataResult<()> {
        let _version = self.substrate().json_set(self.default_run(), key, path, value)?;
        Ok(())
    }

    fn json_del(&self, key: &str, path: &str) -> StrataResult<u64> {
        self.substrate().json_delete(self.default_run(), key, path)
    }

    fn json_merge(&self, key: &str, path: &str, patch: Value) -> StrataResult<()> {
        // Delegate to substrate's atomic merge implementation
        self.substrate().json_merge(self.default_run(), key, path, patch)?;
        Ok(())
    }

    fn json_type(&self, key: &str, path: &str) -> StrataResult<Option<String>> {
        let result = self.substrate().json_get(self.default_run(), key, path)?;
        Ok(result.map(|v| value_type_name(&v.value)))
    }

    fn json_numincrby(&self, key: &str, path: &str, delta: f64) -> StrataResult<f64> {
        // Delegate to substrate's atomic increment
        self.substrate().json_increment(self.default_run(), key, path, delta)
    }

    fn json_strappend(&self, key: &str, path: &str, suffix: &str) -> StrataResult<usize> {
        let current = self.substrate().json_get(self.default_run(), key, path)?;
        let current_str = match current {
            Some(v) => match v.value {
                Value::String(s) => s,
                _ => return Err(StrataError::invalid_operation(
                    strata_core::EntityRef::kv(self.default_run().to_run_id(), key),
                    &format!("Expected string, got {:?}", value_type_name(&v.value)),
                )),
            },
            None => String::new(),
        };
        let new_value = format!("{}{}", current_str, suffix);
        let len = new_value.len();
        self.substrate().json_set(self.default_run(), key, path, Value::String(new_value))?;
        Ok(len)
    }

    fn json_arrappend(&self, key: &str, path: &str, values: Vec<Value>) -> StrataResult<usize> {
        // Delegate to substrate's atomic array push
        self.substrate().json_array_push(self.default_run(), key, path, values)
    }

    fn json_arrpop(&self, key: &str, path: &str) -> StrataResult<Option<Value>> {
        // Delegate to substrate's atomic array pop
        self.substrate().json_array_pop(self.default_run(), key, path)
    }

    fn json_arrlen(&self, key: &str, path: &str) -> StrataResult<Option<usize>> {
        let result = self.substrate().json_get(self.default_run(), key, path)?;
        Ok(result.and_then(|v| match v.value {
            Value::Array(a) => Some(a.len()),
            _ => None,
        }))
    }

    fn json_objkeys(&self, key: &str, path: &str) -> StrataResult<Option<Vec<String>>> {
        let result = self.substrate().json_get(self.default_run(), key, path)?;
        Ok(result.and_then(|v| match v.value {
            Value::Object(map) => Some(map.keys().cloned().collect()),
            _ => None,
        }))
    }

    fn json_objlen(&self, key: &str, path: &str) -> StrataResult<Option<usize>> {
        let result = self.substrate().json_get(self.default_run(), key, path)?;
        Ok(result.and_then(|v| match v.value {
            Value::Object(map) => Some(map.len()),
            _ => None,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trait_is_object_safe() {
        fn _assert_object_safe(_: &dyn JsonFacade) {}
    }
}
