//! JSON Facade - Simplified JSON document operations
//!
//! This module provides Redis-like JSON operations (similar to RedisJSON).
//!
//! ## Desugaring
//!
//! | Facade | Substrate |
//! |--------|-----------|
//! | `json_get(key, path)` | `json_get(default_run, key, path).map(\|v\| v.value)` |
//! | `json_set(key, path, val)` | `json_set(default_run, key, path, val)` |
//! | `json_del(key, path)` | `json_delete(default_run, key, path)` |

use strata_core::{StrataResult, Value};

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

    /// Get array length at path
    fn json_arrlen(&self, key: &str, path: &str) -> StrataResult<Option<usize>>;

    /// Get object keys at path
    fn json_objkeys(&self, key: &str, path: &str) -> StrataResult<Option<Vec<String>>>;

    /// Get object key count at path
    fn json_objlen(&self, key: &str, path: &str) -> StrataResult<Option<usize>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trait_is_object_safe() {
        fn _assert_object_safe(_: &dyn JsonFacade) {}
    }
}
