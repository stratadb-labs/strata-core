//! JSON types for M5 JSON primitive
//!
//! This module defines types for the JSON document storage system:
//! - JsonValue: Newtype wrapper around serde_json::Value
//! - JsonPath: Path into a JSON document (e.g., `user.name` or `items[0]`)
//! - PathSegment: Individual path component (Key or Index)
//! - JsonPatch: Patch operation (Set or Delete)
//!
//! # Document Size Limits
//!
//! M5 enforces the following limits to prevent memory issues:
//!
//! | Limit | Value | Constant |
//! |-------|-------|----------|
//! | Max document size | 16 MB | [`MAX_DOCUMENT_SIZE`] |
//! | Max nesting depth | 100 levels | [`MAX_NESTING_DEPTH`] |
//! | Max path length | 256 segments | [`MAX_PATH_LENGTH`] |
//! | Max array size | 1M elements | [`MAX_ARRAY_SIZE`] |

use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Deref, DerefMut};
use std::str::FromStr;
use thiserror::Error;

// =============================================================================
// Document Size Limits
// =============================================================================

/// Maximum document size in bytes (16 MB)
///
/// Documents larger than this will be rejected to prevent memory issues.
/// This limit is checked on create and update operations.
pub const MAX_DOCUMENT_SIZE: usize = 16 * 1024 * 1024; // 16 MB

/// Maximum nesting depth in a JSON document (100 levels)
///
/// Prevents stack overflow during recursive operations like serialization,
/// deserialization, and path traversal.
pub const MAX_NESTING_DEPTH: usize = 100;

/// Maximum path length in segments (256 segments)
///
/// Limits the depth of paths like "a.b.c.d..." to prevent extremely deep
/// nesting and potential performance issues.
pub const MAX_PATH_LENGTH: usize = 256;

/// Maximum array size in elements (1 million elements)
///
/// Prevents creation of extremely large arrays that could cause memory issues.
pub const MAX_ARRAY_SIZE: usize = 1_000_000;

/// Error type for document limit violations
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum LimitError {
    /// Document exceeds maximum size
    #[error("document size {size} exceeds maximum of {max} bytes")]
    DocumentTooLarge {
        /// Actual document size
        size: usize,
        /// Maximum allowed size
        max: usize,
    },

    /// Document nesting exceeds maximum depth
    #[error("document nesting depth {depth} exceeds maximum of {max} levels")]
    NestingTooDeep {
        /// Actual nesting depth
        depth: usize,
        /// Maximum allowed depth
        max: usize,
    },

    /// Path exceeds maximum length
    #[error("path length {length} exceeds maximum of {max} segments")]
    PathTooLong {
        /// Actual path length
        length: usize,
        /// Maximum allowed length
        max: usize,
    },

    /// Array exceeds maximum size
    #[error("array size {size} exceeds maximum of {max} elements")]
    ArrayTooLarge {
        /// Actual array size
        size: usize,
        /// Maximum allowed size
        max: usize,
    },
}

/// JSON value wrapper
///
/// Newtype around serde_json::Value providing:
/// - Direct access to underlying serde_json::Value via Deref/DerefMut
/// - Easy construction from common types
/// - Serialization/deserialization support
///
/// # Examples
///
/// ```
/// use in_mem_core::JsonValue;
///
/// // From JSON literals
/// let obj = JsonValue::object();
/// let arr = JsonValue::array();
/// let null = JsonValue::null();
///
/// // From common types
/// let s = JsonValue::from("hello");
/// let n = JsonValue::from(42i64);
/// let b = JsonValue::from(true);
///
/// // Access underlying value
/// assert!(obj.is_object());
/// assert!(arr.is_array());
/// assert!(null.is_null());
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct JsonValue(serde_json::Value);

impl JsonValue {
    /// Create a null JSON value
    pub fn null() -> Self {
        JsonValue(serde_json::Value::Null)
    }

    /// Create an empty JSON object
    pub fn object() -> Self {
        JsonValue(serde_json::Value::Object(serde_json::Map::new()))
    }

    /// Create an empty JSON array
    pub fn array() -> Self {
        JsonValue(serde_json::Value::Array(Vec::new()))
    }

    /// Create from a serde_json::Value
    pub fn from_value(value: serde_json::Value) -> Self {
        JsonValue(value)
    }

    /// Get the underlying serde_json::Value
    pub fn into_inner(self) -> serde_json::Value {
        self.0
    }

    /// Get a reference to the underlying serde_json::Value
    pub fn as_inner(&self) -> &serde_json::Value {
        &self.0
    }

    /// Get a mutable reference to the underlying serde_json::Value
    pub fn as_inner_mut(&mut self) -> &mut serde_json::Value {
        &mut self.0
    }

    /// Serialize to compact JSON string
    pub fn to_json_string(&self) -> String {
        self.0.to_string()
    }

    /// Serialize to pretty JSON string
    pub fn to_json_string_pretty(&self) -> String {
        serde_json::to_string_pretty(&self.0).unwrap_or_else(|_| self.to_json_string())
    }

    /// Calculate approximate size in bytes (for limit checking)
    ///
    /// This is an estimate based on the JSON string representation.
    /// Actual in-memory size may differ.
    pub fn size_bytes(&self) -> usize {
        self.to_json_string().len()
    }

    /// Calculate the maximum nesting depth of this JSON value
    ///
    /// Returns 0 for primitives (null, bool, number, string),
    /// and counts nested objects/arrays.
    pub fn nesting_depth(&self) -> usize {
        fn depth_of(value: &serde_json::Value) -> usize {
            match value {
                serde_json::Value::Null
                | serde_json::Value::Bool(_)
                | serde_json::Value::Number(_)
                | serde_json::Value::String(_) => 0,
                serde_json::Value::Array(arr) => 1 + arr.iter().map(depth_of).max().unwrap_or(0),
                serde_json::Value::Object(obj) => 1 + obj.values().map(depth_of).max().unwrap_or(0),
            }
        }
        depth_of(&self.0)
    }

    /// Find the maximum array size in this JSON value (including nested arrays)
    pub fn max_array_size(&self) -> usize {
        fn max_arr_size(value: &serde_json::Value) -> usize {
            match value {
                serde_json::Value::Null
                | serde_json::Value::Bool(_)
                | serde_json::Value::Number(_)
                | serde_json::Value::String(_) => 0,
                serde_json::Value::Array(arr) => {
                    let nested_max = arr.iter().map(max_arr_size).max().unwrap_or(0);
                    arr.len().max(nested_max)
                }
                serde_json::Value::Object(obj) => obj.values().map(max_arr_size).max().unwrap_or(0),
            }
        }
        max_arr_size(&self.0)
    }

    /// Validate document size limit
    ///
    /// Returns an error if the document exceeds [`MAX_DOCUMENT_SIZE`].
    pub fn validate_size(&self) -> Result<(), LimitError> {
        let size = self.size_bytes();
        if size > MAX_DOCUMENT_SIZE {
            Err(LimitError::DocumentTooLarge {
                size,
                max: MAX_DOCUMENT_SIZE,
            })
        } else {
            Ok(())
        }
    }

    /// Validate document nesting depth limit
    ///
    /// Returns an error if the document exceeds [`MAX_NESTING_DEPTH`].
    pub fn validate_depth(&self) -> Result<(), LimitError> {
        let depth = self.nesting_depth();
        if depth > MAX_NESTING_DEPTH {
            Err(LimitError::NestingTooDeep {
                depth,
                max: MAX_NESTING_DEPTH,
            })
        } else {
            Ok(())
        }
    }

    /// Validate array size limits
    ///
    /// Returns an error if any array in the document exceeds [`MAX_ARRAY_SIZE`].
    pub fn validate_array_size(&self) -> Result<(), LimitError> {
        let size = self.max_array_size();
        if size > MAX_ARRAY_SIZE {
            Err(LimitError::ArrayTooLarge {
                size,
                max: MAX_ARRAY_SIZE,
            })
        } else {
            Ok(())
        }
    }

    /// Validate all document limits
    ///
    /// Checks size, nesting depth, and array sizes.
    /// Returns the first error encountered, if any.
    pub fn validate(&self) -> Result<(), LimitError> {
        self.validate_size()?;
        self.validate_depth()?;
        self.validate_array_size()?;
        Ok(())
    }
}

// Implement FromStr for parsing from strings
impl FromStr for JsonValue {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(s).map(JsonValue)
    }
}

// Deref to access serde_json::Value methods directly
impl Deref for JsonValue {
    type Target = serde_json::Value;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for JsonValue {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

// Display for easy printing
impl fmt::Display for JsonValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// Default is null
impl Default for JsonValue {
    fn default() -> Self {
        Self::null()
    }
}

// From implementations for common types
impl From<serde_json::Value> for JsonValue {
    fn from(v: serde_json::Value) -> Self {
        JsonValue(v)
    }
}

impl From<JsonValue> for serde_json::Value {
    fn from(v: JsonValue) -> Self {
        v.0
    }
}

impl From<bool> for JsonValue {
    fn from(v: bool) -> Self {
        JsonValue(serde_json::Value::Bool(v))
    }
}

impl From<i64> for JsonValue {
    fn from(v: i64) -> Self {
        JsonValue(serde_json::Value::Number(v.into()))
    }
}

impl From<i32> for JsonValue {
    fn from(v: i32) -> Self {
        JsonValue(serde_json::Value::Number(v.into()))
    }
}

impl From<u64> for JsonValue {
    fn from(v: u64) -> Self {
        JsonValue(serde_json::Value::Number(v.into()))
    }
}

impl From<u32> for JsonValue {
    fn from(v: u32) -> Self {
        JsonValue(serde_json::Value::Number(v.into()))
    }
}

impl From<f64> for JsonValue {
    fn from(v: f64) -> Self {
        JsonValue(
            serde_json::Number::from_f64(v)
                .map_or(serde_json::Value::Null, serde_json::Value::Number),
        )
    }
}

impl From<&str> for JsonValue {
    fn from(v: &str) -> Self {
        JsonValue(serde_json::Value::String(v.to_string()))
    }
}

impl From<String> for JsonValue {
    fn from(v: String) -> Self {
        JsonValue(serde_json::Value::String(v))
    }
}

impl<T: Into<JsonValue>> From<Vec<T>> for JsonValue {
    fn from(v: Vec<T>) -> Self {
        JsonValue(serde_json::Value::Array(
            v.into_iter().map(|x| x.into().0).collect(),
        ))
    }
}

impl<T: Into<JsonValue>> From<Option<T>> for JsonValue {
    fn from(v: Option<T>) -> Self {
        match v {
            Some(v) => v.into(),
            None => JsonValue::null(),
        }
    }
}

// =============================================================================
// JsonPath and PathSegment
// =============================================================================

/// Error type for JSON path parsing
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PathParseError {
    /// Empty key in path
    #[error("empty key in path at position {0}")]
    EmptyKey(usize),
    /// Unclosed bracket
    #[error("unclosed bracket starting at position {0}")]
    UnclosedBracket(usize),
    /// Invalid array index
    #[error("invalid array index at position {0}: {1}")]
    InvalidIndex(usize, String),
    /// Unexpected character
    #[error("unexpected character '{0}' at position {1}")]
    UnexpectedChar(char, usize),
}

/// A segment in a JSON path
///
/// Paths are composed of key segments (object property access)
/// and index segments (array element access).
///
/// # Examples
///
/// ```
/// use in_mem_core::json::PathSegment;
///
/// let key = PathSegment::Key("name".to_string());
/// let idx = PathSegment::Index(0);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PathSegment {
    /// Object key: `.foo`
    Key(String),
    /// Array index: `[0]`
    Index(usize),
}

impl fmt::Display for PathSegment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PathSegment::Key(k) => write!(f, ".{}", k),
            PathSegment::Index(i) => write!(f, "[{}]", i),
        }
    }
}

/// A path into a JSON document
///
/// JsonPath represents a location within a JSON document using a sequence
/// of key and index segments. Paths support:
///
/// - Object property access: `.foo`
/// - Array index access: `[0]`
/// - Nested paths: `.user.address.city` or `.items[0].name`
///
/// # Path Syntax (M5 Subset)
///
/// | Syntax | Meaning | Example |
/// |--------|---------|---------|
/// | `.key` | Object property | `.user` |
/// | `[n]` | Array index | `[0]` |
/// | `.key1.key2` | Nested property | `.user.name` |
/// | `.key[n]` | Property then index | `.items[0]` |
/// | (empty) | Root | `` |
///
/// # Examples
///
/// ```
/// use in_mem_core::json::JsonPath;
///
/// // Create paths
/// let root = JsonPath::root();
/// let user_name = JsonPath::root().key("user").key("name");
/// let first_item = JsonPath::root().key("items").index(0);
///
/// // Parse from string
/// let path: JsonPath = "user.name".parse().unwrap();
/// assert_eq!(path, user_name);
///
/// // Check relationships
/// let user = JsonPath::root().key("user");
/// assert!(user.is_ancestor_of(&user_name));
/// assert!(user_name.is_descendant_of(&user));
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct JsonPath {
    segments: Vec<PathSegment>,
}

impl JsonPath {
    /// Create the root path (empty path)
    pub fn root() -> Self {
        JsonPath {
            segments: Vec::new(),
        }
    }

    /// Create a path from a vector of segments
    pub fn from_segments(segments: Vec<PathSegment>) -> Self {
        JsonPath { segments }
    }

    /// Get the path segments
    pub fn segments(&self) -> &[PathSegment] {
        &self.segments
    }

    /// Get the number of segments in the path
    pub fn len(&self) -> usize {
        self.segments.len()
    }

    /// Check if this is the root path (empty)
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Check if this is the root path
    pub fn is_root(&self) -> bool {
        self.segments.is_empty()
    }

    /// Append a key segment (builder pattern)
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.segments.push(PathSegment::Key(key.into()));
        self
    }

    /// Append an index segment (builder pattern)
    pub fn index(mut self, idx: usize) -> Self {
        self.segments.push(PathSegment::Index(idx));
        self
    }

    /// Push a key segment (mutating)
    pub fn push_key(&mut self, key: impl Into<String>) {
        self.segments.push(PathSegment::Key(key.into()));
    }

    /// Push an index segment (mutating)
    pub fn push_index(&mut self, idx: usize) {
        self.segments.push(PathSegment::Index(idx));
    }

    /// Get the parent path (None if root)
    pub fn parent(&self) -> Option<JsonPath> {
        if self.segments.is_empty() {
            None
        } else {
            let mut parent = self.clone();
            parent.segments.pop();
            Some(parent)
        }
    }

    /// Get the last segment (None if root)
    pub fn last_segment(&self) -> Option<&PathSegment> {
        self.segments.last()
    }

    /// Check if this path is an ancestor of another (or equal)
    ///
    /// A path is an ancestor if it is a prefix of the other path.
    /// The root path is an ancestor of all paths.
    /// A path is considered an ancestor of itself.
    pub fn is_ancestor_of(&self, other: &JsonPath) -> bool {
        if self.segments.len() > other.segments.len() {
            return false;
        }
        self.segments
            .iter()
            .zip(other.segments.iter())
            .all(|(a, b)| a == b)
    }

    /// Check if this path is a descendant of another (or equal)
    ///
    /// A path is a descendant if the other path is a prefix of this path.
    /// All paths are descendants of the root path.
    /// A path is considered a descendant of itself.
    pub fn is_descendant_of(&self, other: &JsonPath) -> bool {
        other.is_ancestor_of(self)
    }

    /// Check if two paths overlap (one is ancestor/descendant of the other)
    ///
    /// Used for conflict detection: if two paths overlap and both are
    /// accessed in a transaction (one read, one write), there's a potential conflict.
    pub fn overlaps(&self, other: &JsonPath) -> bool {
        self.is_ancestor_of(other) || self.is_descendant_of(other)
    }

    /// Validate path length limit
    ///
    /// Returns an error if the path exceeds [`MAX_PATH_LENGTH`].
    pub fn validate(&self) -> Result<(), LimitError> {
        let length = self.segments.len();
        if length > MAX_PATH_LENGTH {
            Err(LimitError::PathTooLong {
                length,
                max: MAX_PATH_LENGTH,
            })
        } else {
            Ok(())
        }
    }

    /// Convert to a string representation
    pub fn to_path_string(&self) -> String {
        if self.segments.is_empty() {
            return String::new();
        }
        let mut result = String::new();
        for seg in &self.segments {
            match seg {
                PathSegment::Key(k) => {
                    if !result.is_empty() || result.is_empty() {
                        result.push('.');
                    }
                    result.push_str(k);
                }
                PathSegment::Index(i) => {
                    result.push('[');
                    result.push_str(&i.to_string());
                    result.push(']');
                }
            }
        }
        // Remove leading dot if it starts with one
        if result.starts_with('.') {
            result.remove(0);
        }
        result
    }
}

impl FromStr for JsonPath {
    type Err = PathParseError;

    /// Parse a path from a string
    ///
    /// Supported syntax:
    /// - `foo` or `.foo` - object key
    /// - `[0]` - array index
    /// - `foo.bar` - nested keys
    /// - `foo[0]` - key then index
    /// - `foo[0].bar` - mixed
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Ok(JsonPath::root());
        }

        let mut segments = Vec::new();
        let chars: Vec<char> = s.chars().collect();
        let mut i = 0;

        // Skip leading dot if present
        if i < chars.len() && chars[i] == '.' {
            i += 1;
        }

        while i < chars.len() {
            let c = chars[i];

            if c == '.' {
                // Start of a key segment
                i += 1;
                if i >= chars.len() {
                    return Err(PathParseError::EmptyKey(i));
                }
            }

            if chars[i] == '[' {
                // Array index segment
                let start = i;
                i += 1;
                let idx_start = i;

                // Find closing bracket
                while i < chars.len() && chars[i] != ']' {
                    i += 1;
                }

                if i >= chars.len() {
                    return Err(PathParseError::UnclosedBracket(start));
                }

                let idx_str: String = chars[idx_start..i].iter().collect();
                let idx = idx_str
                    .parse::<usize>()
                    .map_err(|_| PathParseError::InvalidIndex(idx_start, idx_str))?;

                segments.push(PathSegment::Index(idx));
                i += 1; // Skip closing bracket
            } else if chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '-' {
                // Key segment
                let key_start = i;
                while i < chars.len()
                    && (chars[i].is_alphanumeric() || chars[i] == '_' || chars[i] == '-')
                {
                    i += 1;
                }
                let key: String = chars[key_start..i].iter().collect();
                segments.push(PathSegment::Key(key));
            } else {
                return Err(PathParseError::UnexpectedChar(chars[i], i));
            }
        }

        Ok(JsonPath { segments })
    }
}

impl fmt::Display for JsonPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_path_string())
    }
}

// =============================================================================
// JsonPatch
// =============================================================================

/// A patch operation on a JSON document
///
/// JsonPatch represents an atomic mutation to a JSON document.
/// Patches are used for:
/// - WAL recording (patch-based persistence)
/// - Transaction tracking
/// - Conflict detection
///
/// # Supported Operations
///
/// - `Set`: Set a value at a path (creates intermediate objects/arrays if needed)
/// - `Delete`: Remove a value at a path
///
/// # Conflict Detection
///
/// Two patches conflict if their paths overlap (one is ancestor/descendant of the other).
/// This is used for region-based conflict detection in transactions.
///
/// # Examples
///
/// ```
/// use in_mem_core::json::{JsonPatch, JsonPath, JsonValue};
///
/// // Create patches
/// let set = JsonPatch::set("user.name", JsonValue::from("Alice"));
/// let delete = JsonPatch::delete("user.email");
///
/// // Check conflict
/// let patch1 = JsonPatch::set("user", JsonValue::object());
/// let patch2 = JsonPatch::set("user.name", JsonValue::from("Bob"));
/// assert!(patch1.conflicts_with(&patch2)); // user is ancestor of user.name
///
/// // Non-conflicting patches
/// let patch3 = JsonPatch::set("user.name", JsonValue::from("Alice"));
/// let patch4 = JsonPatch::set("user.email", JsonValue::from("alice@example.com"));
/// assert!(!patch3.conflicts_with(&patch4)); // Different paths
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum JsonPatch {
    /// Set value at path
    Set {
        /// The path to set
        path: JsonPath,
        /// The value to set
        value: JsonValue,
    },
    /// Delete value at path
    Delete {
        /// The path to delete
        path: JsonPath,
    },
}

impl JsonPatch {
    /// Create a Set patch
    ///
    /// Convenience constructor that parses the path from a string.
    ///
    /// # Panics
    ///
    /// Panics if the path string is invalid.
    pub fn set(path: impl AsRef<str>, value: JsonValue) -> Self {
        JsonPatch::Set {
            path: path
                .as_ref()
                .parse()
                .expect("Invalid path in JsonPatch::set"),
            value,
        }
    }

    /// Create a Set patch with a pre-parsed path
    pub fn set_at(path: JsonPath, value: JsonValue) -> Self {
        JsonPatch::Set { path, value }
    }

    /// Create a Delete patch
    ///
    /// Convenience constructor that parses the path from a string.
    ///
    /// # Panics
    ///
    /// Panics if the path string is invalid.
    pub fn delete(path: impl AsRef<str>) -> Self {
        JsonPatch::Delete {
            path: path
                .as_ref()
                .parse()
                .expect("Invalid path in JsonPatch::delete"),
        }
    }

    /// Create a Delete patch with a pre-parsed path
    pub fn delete_at(path: JsonPath) -> Self {
        JsonPatch::Delete { path }
    }

    /// Get the path affected by this patch
    pub fn path(&self) -> &JsonPath {
        match self {
            JsonPatch::Set { path, .. } => path,
            JsonPatch::Delete { path } => path,
        }
    }

    /// Check if this patch conflicts with another
    ///
    /// Two patches conflict if their paths overlap (one is ancestor/descendant of the other).
    /// This is used for region-based conflict detection.
    ///
    /// Note: Two Set patches to the same path also conflict, but can sometimes be
    /// resolved by last-writer-wins semantics depending on the use case.
    pub fn conflicts_with(&self, other: &JsonPatch) -> bool {
        self.path().overlaps(other.path())
    }

    /// Check if this is a Set operation
    pub fn is_set(&self) -> bool {
        matches!(self, JsonPatch::Set { .. })
    }

    /// Check if this is a Delete operation
    pub fn is_delete(&self) -> bool {
        matches!(self, JsonPatch::Delete { .. })
    }

    /// Get the value if this is a Set patch
    pub fn value(&self) -> Option<&JsonValue> {
        match self {
            JsonPatch::Set { value, .. } => Some(value),
            JsonPatch::Delete { .. } => None,
        }
    }
}

impl fmt::Display for JsonPatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            JsonPatch::Set { path, value } => write!(f, "SET {} = {}", path, value),
            JsonPatch::Delete { path } => write!(f, "DELETE {}", path),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_value_null() {
        let v = JsonValue::null();
        assert!(v.is_null());
    }

    #[test]
    fn test_json_value_object() {
        let v = JsonValue::object();
        assert!(v.is_object());
        assert_eq!(v.as_object().unwrap().len(), 0);
    }

    #[test]
    fn test_json_value_array() {
        let v = JsonValue::array();
        assert!(v.is_array());
        assert_eq!(v.as_array().unwrap().len(), 0);
    }

    #[test]
    fn test_json_value_from_bool() {
        let t = JsonValue::from(true);
        let f = JsonValue::from(false);
        assert_eq!(t.as_bool(), Some(true));
        assert_eq!(f.as_bool(), Some(false));
    }

    #[test]
    fn test_json_value_from_i64() {
        let v = JsonValue::from(42i64);
        assert_eq!(v.as_i64(), Some(42));
    }

    #[test]
    fn test_json_value_from_i32() {
        let v = JsonValue::from(42i32);
        assert_eq!(v.as_i64(), Some(42));
    }

    #[test]
    fn test_json_value_from_u64() {
        let v = JsonValue::from(42u64);
        assert_eq!(v.as_u64(), Some(42));
    }

    #[test]
    fn test_json_value_from_f64() {
        let v = JsonValue::from(3.14f64);
        assert!((v.as_f64().unwrap() - 3.14).abs() < f64::EPSILON);
    }

    #[test]
    fn test_json_value_from_str_ref() {
        let v = JsonValue::from("hello");
        assert_eq!(v.as_str(), Some("hello"));
    }

    #[test]
    fn test_json_value_from_string() {
        let v = JsonValue::from("world".to_string());
        assert_eq!(v.as_str(), Some("world"));
    }

    #[test]
    fn test_json_value_from_vec() {
        let v: JsonValue = vec![1i64, 2, 3].into();
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0].as_i64(), Some(1));
    }

    #[test]
    fn test_json_value_from_option_some() {
        let v: JsonValue = Some(42i64).into();
        assert_eq!(v.as_i64(), Some(42));
    }

    #[test]
    fn test_json_value_from_option_none() {
        let v: JsonValue = Option::<i64>::None.into();
        assert!(v.is_null());
    }

    #[test]
    fn test_json_value_deref() {
        let v = JsonValue::from(42i64);
        // Access serde_json::Value methods via Deref
        assert!(v.is_number());
        assert!(!v.is_string());
    }

    #[test]
    fn test_json_value_deref_mut() {
        let mut v = JsonValue::object();
        // Mutate via DerefMut
        v.as_object_mut()
            .unwrap()
            .insert("key".to_string(), serde_json::json!(123));
        assert_eq!(v["key"].as_i64(), Some(123));
    }

    #[test]
    fn test_json_value_parse() {
        let v: JsonValue = r#"{"name": "test", "value": 42}"#.parse().unwrap();
        assert!(v.is_object());
        assert_eq!(v["name"].as_str(), Some("test"));
        assert_eq!(v["value"].as_i64(), Some(42));
    }

    #[test]
    fn test_json_value_parse_invalid() {
        let result: Result<JsonValue, _> = "not valid json {".parse();
        assert!(result.is_err());
    }

    #[test]
    fn test_json_value_to_json_string() {
        let v: JsonValue = r#"{"a":1}"#.parse().unwrap();
        let s = v.to_json_string();
        assert!(s.contains("\"a\""));
        assert!(s.contains("1"));
    }

    #[test]
    fn test_json_value_display() {
        let v = JsonValue::from(42i64);
        let s = format!("{}", v);
        assert_eq!(s, "42");
    }

    #[test]
    fn test_json_value_default() {
        let v = JsonValue::default();
        assert!(v.is_null());
    }

    #[test]
    fn test_json_value_clone() {
        let v1 = JsonValue::from("test");
        let v2 = v1.clone();
        assert_eq!(v1, v2);
    }

    #[test]
    fn test_json_value_equality() {
        let v1 = JsonValue::from(42i64);
        let v2 = JsonValue::from(42i64);
        let v3 = JsonValue::from(43i64);
        assert_eq!(v1, v2);
        assert_ne!(v1, v3);
    }

    #[test]
    fn test_json_value_serialization() {
        let v: JsonValue = r#"{"key": "value"}"#.parse().unwrap();
        let json = serde_json::to_string(&v).unwrap();
        let v2: JsonValue = serde_json::from_str(&json).unwrap();
        assert_eq!(v, v2);
    }

    #[test]
    fn test_json_value_into_inner() {
        let v = JsonValue::from(42i64);
        let inner: serde_json::Value = v.into_inner();
        assert_eq!(inner.as_i64(), Some(42));
    }

    #[test]
    fn test_json_value_as_inner() {
        let v = JsonValue::from(42i64);
        let inner: &serde_json::Value = v.as_inner();
        assert_eq!(inner.as_i64(), Some(42));
    }

    #[test]
    fn test_json_value_size_bytes() {
        let v: JsonValue = r#"{"key": "value"}"#.parse().unwrap();
        let size = v.size_bytes();
        // Should be at least the length of the JSON string
        assert!(size > 0);
        assert!(size <= 20); // Reasonable upper bound for this small object
    }

    #[test]
    fn test_json_value_from_serde_json_value() {
        let serde_val = serde_json::json!({"nested": {"deep": true}});
        let v = JsonValue::from(serde_val);
        assert!(v.is_object());
        assert!(v["nested"]["deep"].as_bool().unwrap());
    }

    #[test]
    fn test_json_value_into_serde_json_value() {
        let v = JsonValue::from(42i64);
        let serde_val: serde_json::Value = v.into();
        assert_eq!(serde_val.as_i64(), Some(42));
    }

    #[test]
    fn test_json_value_f64_nan() {
        // NaN/Infinity cannot be represented in JSON, should become null
        let v = JsonValue::from(f64::NAN);
        assert!(v.is_null());
    }

    #[test]
    fn test_json_value_f64_infinity() {
        // Infinity cannot be represented in JSON, should become null
        let v = JsonValue::from(f64::INFINITY);
        assert!(v.is_null());
    }

    #[test]
    fn test_json_value_nested_modification() {
        let mut v: JsonValue = r#"{"user": {"name": "Alice"}}"#.parse().unwrap();
        v["user"]["name"] = serde_json::json!("Bob");
        assert_eq!(v["user"]["name"].as_str(), Some("Bob"));
    }

    #[test]
    fn test_json_value_to_json_string_pretty() {
        let v: JsonValue = r#"{"a":1,"b":2}"#.parse().unwrap();
        let pretty = v.to_json_string_pretty();
        // Pretty output should have newlines
        assert!(pretty.contains('\n'));
    }

    #[test]
    fn test_json_value_from_value() {
        let serde_val = serde_json::json!([1, 2, 3]);
        let v = JsonValue::from_value(serde_val);
        assert!(v.is_array());
        assert_eq!(v.as_array().unwrap().len(), 3);
    }

    // ========================================
    // JsonPath Tests (M5)
    // ========================================

    #[test]
    fn test_path_root() {
        let root = JsonPath::root();
        assert!(root.is_root());
        assert!(root.is_empty());
        assert_eq!(root.len(), 0);
    }

    #[test]
    fn test_path_key_builder() {
        let path = JsonPath::root().key("user").key("name");
        assert_eq!(path.len(), 2);
        assert!(!path.is_root());
        assert_eq!(
            path.segments(),
            &[
                PathSegment::Key("user".to_string()),
                PathSegment::Key("name".to_string())
            ]
        );
    }

    #[test]
    fn test_path_index_builder() {
        let path = JsonPath::root().key("items").index(0);
        assert_eq!(path.len(), 2);
        assert_eq!(
            path.segments(),
            &[PathSegment::Key("items".to_string()), PathSegment::Index(0)]
        );
    }

    #[test]
    fn test_path_push_methods() {
        let mut path = JsonPath::root();
        path.push_key("user");
        path.push_index(0);
        path.push_key("name");
        assert_eq!(path.len(), 3);
    }

    #[test]
    fn test_path_parse_simple_key() {
        let path: JsonPath = "user".parse().unwrap();
        assert_eq!(path.len(), 1);
        assert_eq!(path.segments(), &[PathSegment::Key("user".to_string())]);
    }

    #[test]
    fn test_path_parse_dotted_keys() {
        let path: JsonPath = "user.name".parse().unwrap();
        assert_eq!(path.len(), 2);
        assert_eq!(
            path.segments(),
            &[
                PathSegment::Key("user".to_string()),
                PathSegment::Key("name".to_string())
            ]
        );
    }

    #[test]
    fn test_path_parse_leading_dot() {
        let path: JsonPath = ".user.name".parse().unwrap();
        assert_eq!(path.len(), 2);
        assert_eq!(
            path.segments(),
            &[
                PathSegment::Key("user".to_string()),
                PathSegment::Key("name".to_string())
            ]
        );
    }

    #[test]
    fn test_path_parse_array_index() {
        let path: JsonPath = "[0]".parse().unwrap();
        assert_eq!(path.len(), 1);
        assert_eq!(path.segments(), &[PathSegment::Index(0)]);
    }

    #[test]
    fn test_path_parse_key_then_index() {
        let path: JsonPath = "items[0]".parse().unwrap();
        assert_eq!(path.len(), 2);
        assert_eq!(
            path.segments(),
            &[PathSegment::Key("items".to_string()), PathSegment::Index(0)]
        );
    }

    #[test]
    fn test_path_parse_complex() {
        let path: JsonPath = "users[0].profile.settings[2].value".parse().unwrap();
        assert_eq!(path.len(), 6);
        assert_eq!(
            path.segments(),
            &[
                PathSegment::Key("users".to_string()),
                PathSegment::Index(0),
                PathSegment::Key("profile".to_string()),
                PathSegment::Key("settings".to_string()),
                PathSegment::Index(2),
                PathSegment::Key("value".to_string()),
            ]
        );
    }

    #[test]
    fn test_path_parse_empty() {
        let path: JsonPath = "".parse().unwrap();
        assert!(path.is_root());
    }

    #[test]
    fn test_path_parse_with_underscore() {
        let path: JsonPath = "user_name".parse().unwrap();
        assert_eq!(
            path.segments(),
            &[PathSegment::Key("user_name".to_string())]
        );
    }

    #[test]
    fn test_path_parse_with_hyphen() {
        let path: JsonPath = "content-type".parse().unwrap();
        assert_eq!(
            path.segments(),
            &[PathSegment::Key("content-type".to_string())]
        );
    }

    #[test]
    fn test_path_parse_error_unclosed_bracket() {
        let result: Result<JsonPath, _> = "items[0".parse();
        assert!(matches!(result, Err(PathParseError::UnclosedBracket(_))));
    }

    #[test]
    fn test_path_parse_error_invalid_index() {
        let result: Result<JsonPath, _> = "items[abc]".parse();
        assert!(matches!(result, Err(PathParseError::InvalidIndex(_, _))));
    }

    #[test]
    fn test_path_parse_error_empty_key() {
        let result: Result<JsonPath, _> = "user.".parse();
        assert!(matches!(result, Err(PathParseError::EmptyKey(_))));
    }

    #[test]
    fn test_path_parent() {
        let path = JsonPath::root().key("user").key("name");
        let parent = path.parent().unwrap();
        assert_eq!(parent.len(), 1);
        assert_eq!(parent.segments(), &[PathSegment::Key("user".to_string())]);

        let grandparent = parent.parent().unwrap();
        assert!(grandparent.is_root());

        assert!(grandparent.parent().is_none());
    }

    #[test]
    fn test_path_last_segment() {
        let path = JsonPath::root().key("user").index(0);
        assert_eq!(path.last_segment(), Some(&PathSegment::Index(0)));

        let root = JsonPath::root();
        assert_eq!(root.last_segment(), None);
    }

    #[test]
    fn test_path_is_ancestor_of() {
        let root = JsonPath::root();
        let user = JsonPath::root().key("user");
        let user_name = JsonPath::root().key("user").key("name");
        let items = JsonPath::root().key("items");

        // Root is ancestor of all
        assert!(root.is_ancestor_of(&root));
        assert!(root.is_ancestor_of(&user));
        assert!(root.is_ancestor_of(&user_name));

        // Paths are ancestors of themselves
        assert!(user.is_ancestor_of(&user));
        assert!(user_name.is_ancestor_of(&user_name));

        // Parent is ancestor of child
        assert!(user.is_ancestor_of(&user_name));

        // Child is not ancestor of parent
        assert!(!user_name.is_ancestor_of(&user));

        // Unrelated paths are not ancestors
        assert!(!user.is_ancestor_of(&items));
        assert!(!items.is_ancestor_of(&user));
    }

    #[test]
    fn test_path_is_descendant_of() {
        let root = JsonPath::root();
        let user = JsonPath::root().key("user");
        let user_name = JsonPath::root().key("user").key("name");

        // All paths are descendants of root
        assert!(root.is_descendant_of(&root));
        assert!(user.is_descendant_of(&root));
        assert!(user_name.is_descendant_of(&root));

        // Paths are descendants of themselves
        assert!(user.is_descendant_of(&user));

        // Child is descendant of parent
        assert!(user_name.is_descendant_of(&user));

        // Parent is not descendant of child
        assert!(!user.is_descendant_of(&user_name));
    }

    #[test]
    fn test_path_overlaps() {
        let user = JsonPath::root().key("user");
        let user_name = JsonPath::root().key("user").key("name");
        let items = JsonPath::root().key("items");

        // Ancestor/descendant paths overlap
        assert!(user.overlaps(&user_name));
        assert!(user_name.overlaps(&user));

        // Paths overlap with themselves
        assert!(user.overlaps(&user));

        // Unrelated paths don't overlap
        assert!(!user.overlaps(&items));
        assert!(!items.overlaps(&user_name));
    }

    #[test]
    fn test_path_to_string() {
        assert_eq!(JsonPath::root().to_path_string(), "");
        assert_eq!(JsonPath::root().key("user").to_path_string(), "user");
        assert_eq!(
            JsonPath::root().key("user").key("name").to_path_string(),
            "user.name"
        );
        assert_eq!(
            JsonPath::root().key("items").index(0).to_path_string(),
            "items[0]"
        );
        assert_eq!(
            JsonPath::root()
                .key("items")
                .index(0)
                .key("name")
                .to_path_string(),
            "items[0].name"
        );
    }

    #[test]
    fn test_path_display() {
        let path = JsonPath::root().key("user").key("name");
        assert_eq!(format!("{}", path), "user.name");
    }

    #[test]
    fn test_path_default() {
        let path = JsonPath::default();
        assert!(path.is_root());
    }

    #[test]
    fn test_path_clone() {
        let path1 = JsonPath::root().key("user");
        let path2 = path1.clone();
        assert_eq!(path1, path2);
    }

    #[test]
    fn test_path_equality() {
        let path1 = JsonPath::root().key("user").key("name");
        let path2: JsonPath = "user.name".parse().unwrap();
        let path3 = JsonPath::root().key("user").key("email");

        assert_eq!(path1, path2);
        assert_ne!(path1, path3);
    }

    #[test]
    fn test_path_hash() {
        use std::collections::HashSet;

        let path1 = JsonPath::root().key("user");
        let path2: JsonPath = "user".parse().unwrap();
        let path3 = JsonPath::root().key("items");

        let mut set = HashSet::new();
        set.insert(path1.clone());
        set.insert(path2); // Same as path1
        set.insert(path3);

        assert_eq!(set.len(), 2);
        assert!(set.contains(&path1));
    }

    #[test]
    fn test_path_serialization() {
        let path = JsonPath::root().key("user").index(0).key("name");
        let json = serde_json::to_string(&path).unwrap();
        let path2: JsonPath = serde_json::from_str(&json).unwrap();
        assert_eq!(path, path2);
    }

    #[test]
    fn test_path_segment_display() {
        assert_eq!(format!("{}", PathSegment::Key("foo".to_string())), ".foo");
        assert_eq!(format!("{}", PathSegment::Index(42)), "[42]");
    }

    #[test]
    fn test_path_from_segments() {
        let segments = vec![PathSegment::Key("user".to_string()), PathSegment::Index(0)];
        let path = JsonPath::from_segments(segments.clone());
        assert_eq!(path.segments(), &segments);
    }

    // ========================================
    // JsonPatch Tests (M5)
    // ========================================

    #[test]
    fn test_patch_set() {
        let patch = JsonPatch::set("user.name", JsonValue::from("Alice"));
        assert!(patch.is_set());
        assert!(!patch.is_delete());
        assert_eq!(patch.path().to_path_string(), "user.name");
        assert_eq!(patch.value().unwrap().as_str(), Some("Alice"));
    }

    #[test]
    fn test_patch_set_at() {
        let path = JsonPath::root().key("user").key("name");
        let patch = JsonPatch::set_at(path.clone(), JsonValue::from("Bob"));
        assert!(patch.is_set());
        assert_eq!(patch.path(), &path);
    }

    #[test]
    fn test_patch_delete() {
        let patch = JsonPatch::delete("user.email");
        assert!(!patch.is_set());
        assert!(patch.is_delete());
        assert_eq!(patch.path().to_path_string(), "user.email");
        assert!(patch.value().is_none());
    }

    #[test]
    fn test_patch_delete_at() {
        let path = JsonPath::root().key("user").key("email");
        let patch = JsonPatch::delete_at(path.clone());
        assert!(patch.is_delete());
        assert_eq!(patch.path(), &path);
    }

    #[test]
    fn test_patch_conflicts_with_overlapping() {
        let patch1 = JsonPatch::set("user", JsonValue::object());
        let patch2 = JsonPatch::set("user.name", JsonValue::from("Alice"));

        // Parent/child paths conflict
        assert!(patch1.conflicts_with(&patch2));
        assert!(patch2.conflicts_with(&patch1));
    }

    #[test]
    fn test_patch_conflicts_with_same_path() {
        let patch1 = JsonPatch::set("user.name", JsonValue::from("Alice"));
        let patch2 = JsonPatch::set("user.name", JsonValue::from("Bob"));

        // Same path conflicts
        assert!(patch1.conflicts_with(&patch2));
    }

    #[test]
    fn test_patch_no_conflict_different_paths() {
        let patch1 = JsonPatch::set("user.name", JsonValue::from("Alice"));
        let patch2 = JsonPatch::set("user.email", JsonValue::from("alice@example.com"));

        // Sibling paths don't conflict
        assert!(!patch1.conflicts_with(&patch2));
        assert!(!patch2.conflicts_with(&patch1));
    }

    #[test]
    fn test_patch_delete_conflicts() {
        let set_patch = JsonPatch::set("user.profile", JsonValue::object());
        let delete_patch = JsonPatch::delete("user");

        // Delete of ancestor conflicts with set of descendant
        assert!(set_patch.conflicts_with(&delete_patch));
        assert!(delete_patch.conflicts_with(&set_patch));
    }

    #[test]
    fn test_patch_display_set() {
        let patch = JsonPatch::set("user.name", JsonValue::from("Alice"));
        let s = format!("{}", patch);
        assert!(s.starts_with("SET"));
        assert!(s.contains("user.name"));
        assert!(s.contains("Alice"));
    }

    #[test]
    fn test_patch_display_delete() {
        let patch = JsonPatch::delete("user.email");
        let s = format!("{}", patch);
        assert!(s.starts_with("DELETE"));
        assert!(s.contains("user.email"));
    }

    #[test]
    fn test_patch_clone() {
        let patch1 = JsonPatch::set("user.name", JsonValue::from("Alice"));
        let patch2 = patch1.clone();
        assert_eq!(patch1, patch2);
    }

    #[test]
    fn test_patch_equality() {
        let patch1 = JsonPatch::set("user.name", JsonValue::from("Alice"));
        let patch2 = JsonPatch::set("user.name", JsonValue::from("Alice"));
        let patch3 = JsonPatch::set("user.name", JsonValue::from("Bob"));
        let patch4 = JsonPatch::delete("user.name");

        assert_eq!(patch1, patch2);
        assert_ne!(patch1, patch3);
        assert_ne!(patch1, patch4);
    }

    #[test]
    fn test_patch_serialization() {
        let patch = JsonPatch::set("user.name", JsonValue::from("Alice"));
        let json = serde_json::to_string(&patch).unwrap();
        let patch2: JsonPatch = serde_json::from_str(&json).unwrap();
        assert_eq!(patch, patch2);
    }

    #[test]
    fn test_patch_delete_serialization() {
        let patch = JsonPatch::delete("user.email");
        let json = serde_json::to_string(&patch).unwrap();
        let patch2: JsonPatch = serde_json::from_str(&json).unwrap();
        assert_eq!(patch, patch2);
    }

    #[test]
    fn test_patch_root_conflicts_with_all() {
        let root_patch = JsonPatch::set("", JsonValue::object());
        let other_patch = JsonPatch::set("user.name", JsonValue::from("Alice"));

        // Root path conflicts with all paths
        assert!(root_patch.conflicts_with(&other_patch));
        assert!(other_patch.conflicts_with(&root_patch));
    }

    #[test]
    fn test_patch_with_array_index() {
        let patch1 = JsonPatch::set("items[0].name", JsonValue::from("First"));
        let patch2 = JsonPatch::set("items[1].name", JsonValue::from("Second"));

        // Different array indices don't conflict
        assert!(!patch1.conflicts_with(&patch2));

        let patch3 = JsonPatch::set("items", JsonValue::array());
        // Parent of array conflicts with child path
        assert!(patch3.conflicts_with(&patch1));
    }

    // ========================================
    // Document Size Limits Tests (M5)
    // ========================================

    #[test]
    fn test_limit_constants() {
        // Verify limit constants are defined with expected values
        assert_eq!(MAX_DOCUMENT_SIZE, 16 * 1024 * 1024); // 16 MB
        assert_eq!(MAX_NESTING_DEPTH, 100);
        assert_eq!(MAX_PATH_LENGTH, 256);
        assert_eq!(MAX_ARRAY_SIZE, 1_000_000);
    }

    #[test]
    fn test_nesting_depth_primitive() {
        assert_eq!(JsonValue::null().nesting_depth(), 0);
        assert_eq!(JsonValue::from(true).nesting_depth(), 0);
        assert_eq!(JsonValue::from(42i64).nesting_depth(), 0);
        assert_eq!(JsonValue::from("hello").nesting_depth(), 0);
    }

    #[test]
    fn test_nesting_depth_simple_object() {
        let v = JsonValue::object();
        assert_eq!(v.nesting_depth(), 1);
    }

    #[test]
    fn test_nesting_depth_simple_array() {
        let v = JsonValue::array();
        assert_eq!(v.nesting_depth(), 1);
    }

    #[test]
    fn test_nesting_depth_nested() {
        let v: JsonValue = r#"{"a": {"b": {"c": 1}}}"#.parse().unwrap();
        assert_eq!(v.nesting_depth(), 3);
    }

    #[test]
    fn test_nesting_depth_mixed() {
        let v: JsonValue = r#"{"arr": [{"nested": [1, 2]}]}"#.parse().unwrap();
        assert_eq!(v.nesting_depth(), 4);
    }

    #[test]
    fn test_max_array_size_empty() {
        let v = JsonValue::object();
        assert_eq!(v.max_array_size(), 0);
    }

    #[test]
    fn test_max_array_size_simple() {
        let v: JsonValue = r#"[1, 2, 3, 4, 5]"#.parse().unwrap();
        assert_eq!(v.max_array_size(), 5);
    }

    #[test]
    fn test_max_array_size_nested() {
        let v: JsonValue = r#"{"a": [1, 2], "b": [1, 2, 3, 4, 5, 6, 7]}"#.parse().unwrap();
        assert_eq!(v.max_array_size(), 7);
    }

    #[test]
    fn test_validate_size_ok() {
        let v = JsonValue::from("small document");
        assert!(v.validate_size().is_ok());
    }

    #[test]
    fn test_validate_depth_ok() {
        let v: JsonValue = r#"{"a": {"b": {"c": 1}}}"#.parse().unwrap();
        assert!(v.validate_depth().is_ok());
    }

    #[test]
    fn test_validate_array_size_ok() {
        let v: JsonValue = r#"[1, 2, 3]"#.parse().unwrap();
        assert!(v.validate_array_size().is_ok());
    }

    #[test]
    fn test_validate_all_ok() {
        let v: JsonValue = r#"{"user": {"name": "Alice", "tags": [1, 2, 3]}}"#.parse().unwrap();
        assert!(v.validate().is_ok());
    }

    #[test]
    fn test_path_validate_ok() {
        let path = JsonPath::root().key("user").key("name");
        assert!(path.validate().is_ok());
    }

    #[test]
    fn test_path_validate_long_path() {
        let mut path = JsonPath::root();
        for i in 0..300 {
            path.push_key(format!("key{}", i));
        }
        let result = path.validate();
        assert!(matches!(result, Err(LimitError::PathTooLong { .. })));
    }

    #[test]
    fn test_limit_error_display() {
        let err = LimitError::DocumentTooLarge {
            size: 20_000_000,
            max: MAX_DOCUMENT_SIZE,
        };
        let s = format!("{}", err);
        assert!(s.contains("20000000"));
        assert!(s.contains("16777216"));
    }

    #[test]
    fn test_limit_error_nesting_display() {
        let err = LimitError::NestingTooDeep {
            depth: 150,
            max: MAX_NESTING_DEPTH,
        };
        let s = format!("{}", err);
        assert!(s.contains("150"));
        assert!(s.contains("100"));
    }

    #[test]
    fn test_limit_error_path_display() {
        let err = LimitError::PathTooLong {
            length: 300,
            max: MAX_PATH_LENGTH,
        };
        let s = format!("{}", err);
        assert!(s.contains("300"));
        assert!(s.contains("256"));
    }

    #[test]
    fn test_limit_error_array_display() {
        let err = LimitError::ArrayTooLarge {
            size: 2_000_000,
            max: MAX_ARRAY_SIZE,
        };
        let s = format!("{}", err);
        assert!(s.contains("2000000"));
        assert!(s.contains("1000000"));
    }

    #[test]
    fn test_limit_error_equality() {
        let err1 = LimitError::DocumentTooLarge { size: 100, max: 50 };
        let err2 = LimitError::DocumentTooLarge { size: 100, max: 50 };
        let err3 = LimitError::DocumentTooLarge { size: 200, max: 50 };
        assert_eq!(err1, err2);
        assert_ne!(err1, err3);
    }

    #[test]
    fn test_limit_error_clone() {
        let err = LimitError::PathTooLong {
            length: 300,
            max: 256,
        };
        let err2 = err.clone();
        assert_eq!(err, err2);
    }
}
