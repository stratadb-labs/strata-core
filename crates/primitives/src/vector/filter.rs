//! Metadata filtering for vector search
//!
//! Supports only equality filtering on top-level scalar fields.
//! Complex filters (ranges, nested paths, arrays) are deferred to future versions.

use serde_json::Value as JsonValue;
use std::collections::HashMap;

/// Metadata filter for search (equality only)
///
/// Supports only top-level field equality filtering.
/// Complex filters (ranges, nested paths, arrays) are deferred to future versions.
#[derive(Debug, Clone, Default)]
pub struct MetadataFilter {
    /// Top-level field equality (scalar values only)
    /// All conditions must match (AND semantics)
    pub equals: HashMap<String, JsonScalar>,
}

impl MetadataFilter {
    /// Create an empty filter (matches all)
    pub fn new() -> Self {
        MetadataFilter {
            equals: HashMap::new(),
        }
    }

    /// Add an equality condition
    pub fn eq(mut self, field: impl Into<String>, value: impl Into<JsonScalar>) -> Self {
        self.equals.insert(field.into(), value.into());
        self
    }

    /// Check if metadata matches this filter
    ///
    /// Returns true if all conditions match.
    /// Returns false if metadata is None and filter is non-empty.
    pub fn matches(&self, metadata: &Option<JsonValue>) -> bool {
        if self.equals.is_empty() {
            return true;
        }

        let Some(meta) = metadata else {
            return false;
        };

        let Some(obj) = meta.as_object() else {
            return false;
        };

        for (key, expected) in &self.equals {
            let Some(actual) = obj.get(key) else {
                return false;
            };
            if !expected.matches_json(actual) {
                return false;
            }
        }

        true
    }

    /// Check if filter is empty (matches all)
    pub fn is_empty(&self) -> bool {
        self.equals.is_empty()
    }

    /// Get the number of conditions in the filter
    pub fn len(&self) -> usize {
        self.equals.len()
    }
}

/// JSON scalar value for filtering
///
/// Only scalar values can be used in equality filters.
/// Complex types (arrays, objects) are not supported.
#[derive(Debug, Clone, PartialEq)]
pub enum JsonScalar {
    /// Null value
    Null,
    /// Boolean value
    Bool(bool),
    /// Numeric value (stored as f64)
    Number(f64),
    /// String value
    String(String),
}

impl JsonScalar {
    /// Check if this scalar matches a JSON value
    pub fn matches_json(&self, value: &JsonValue) -> bool {
        match (self, value) {
            (JsonScalar::Null, JsonValue::Null) => true,
            (JsonScalar::Bool(a), JsonValue::Bool(b)) => a == b,
            (JsonScalar::Number(a), JsonValue::Number(b)) => {
                b.as_f64().is_some_and(|n| (a - n).abs() < f64::EPSILON)
            }
            (JsonScalar::String(a), JsonValue::String(b)) => a == b,
            _ => false,
        }
    }
}

// Convenience conversions
impl From<bool> for JsonScalar {
    fn from(b: bool) -> Self {
        JsonScalar::Bool(b)
    }
}

impl From<i32> for JsonScalar {
    fn from(n: i32) -> Self {
        JsonScalar::Number(n as f64)
    }
}

impl From<i64> for JsonScalar {
    fn from(n: i64) -> Self {
        JsonScalar::Number(n as f64)
    }
}

impl From<f64> for JsonScalar {
    fn from(n: f64) -> Self {
        JsonScalar::Number(n)
    }
}

impl From<f32> for JsonScalar {
    fn from(n: f32) -> Self {
        JsonScalar::Number(n as f64)
    }
}

impl From<String> for JsonScalar {
    fn from(s: String) -> Self {
        JsonScalar::String(s)
    }
}

impl From<&str> for JsonScalar {
    fn from(s: &str) -> Self {
        JsonScalar::String(s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_empty_filter_matches_all() {
        let filter = MetadataFilter::new();
        assert!(filter.matches(&None));
        assert!(filter.matches(&Some(json!({"foo": "bar"}))));
        assert!(filter.is_empty());
        assert_eq!(filter.len(), 0);
    }

    #[test]
    fn test_filter_matches_exact() {
        let filter = MetadataFilter::new()
            .eq("category", "document")
            .eq("year", 2024);

        // Matching metadata
        let meta = json!({
            "category": "document",
            "year": 2024,
            "extra": "ignored"
        });
        assert!(filter.matches(&Some(meta)));
        assert!(!filter.is_empty());
        assert_eq!(filter.len(), 2);
    }

    #[test]
    fn test_filter_missing_field() {
        let filter = MetadataFilter::new()
            .eq("category", "document")
            .eq("year", 2024);

        let meta = json!({ "category": "document" });
        assert!(!filter.matches(&Some(meta)));
    }

    #[test]
    fn test_filter_wrong_value() {
        let filter = MetadataFilter::new().eq("category", "document");

        let meta = json!({ "category": "image" });
        assert!(!filter.matches(&Some(meta)));
    }

    #[test]
    fn test_filter_none_metadata() {
        let filter = MetadataFilter::new().eq("category", "document");
        assert!(!filter.matches(&None));
    }

    #[test]
    fn test_filter_non_object_metadata() {
        let filter = MetadataFilter::new().eq("category", "document");
        assert!(!filter.matches(&Some(json!("not an object"))));
        assert!(!filter.matches(&Some(json!([1, 2, 3]))));
    }

    #[test]
    fn test_filter_bool_value() {
        let filter = MetadataFilter::new().eq("active", true);

        assert!(filter.matches(&Some(json!({ "active": true }))));
        assert!(!filter.matches(&Some(json!({ "active": false }))));
    }

    #[test]
    fn test_filter_null_value() {
        let filter = MetadataFilter::new().eq("deleted", JsonScalar::Null);

        assert!(filter.matches(&Some(json!({ "deleted": null }))));
        assert!(!filter.matches(&Some(json!({ "deleted": false }))));
    }

    #[test]
    fn test_filter_number_value() {
        let filter = MetadataFilter::new().eq("count", 42);

        assert!(filter.matches(&Some(json!({ "count": 42 }))));
        assert!(!filter.matches(&Some(json!({ "count": 43 }))));
    }

    #[test]
    fn test_filter_float_value() {
        let filter = MetadataFilter::new().eq("score", 0.95f64);

        assert!(filter.matches(&Some(json!({ "score": 0.95 }))));
        assert!(!filter.matches(&Some(json!({ "score": 0.96 }))));
    }

    #[test]
    fn test_json_scalar_equality() {
        assert_eq!(JsonScalar::Null, JsonScalar::Null);
        assert_eq!(JsonScalar::Bool(true), JsonScalar::Bool(true));
        assert_ne!(JsonScalar::Bool(true), JsonScalar::Bool(false));
        assert_eq!(JsonScalar::Number(42.0), JsonScalar::Number(42.0));
        assert_eq!(
            JsonScalar::String("test".to_string()),
            JsonScalar::String("test".to_string())
        );
    }

    #[test]
    fn test_json_scalar_from_conversions() {
        let _: JsonScalar = true.into();
        let _: JsonScalar = 42i32.into();
        let _: JsonScalar = 42i64.into();
        let _: JsonScalar = 42.0f64.into();
        let _: JsonScalar = 42.0f32.into();
        let _: JsonScalar = "test".into();
        let _: JsonScalar = String::from("test").into();
    }

    #[test]
    fn test_json_scalar_matches_json() {
        assert!(JsonScalar::Null.matches_json(&JsonValue::Null));
        assert!(JsonScalar::Bool(true).matches_json(&json!(true)));
        assert!(JsonScalar::Number(42.0).matches_json(&json!(42)));
        assert!(JsonScalar::String("test".to_string()).matches_json(&json!("test")));

        // Type mismatches
        assert!(!JsonScalar::Bool(true).matches_json(&json!(1)));
        assert!(!JsonScalar::Number(42.0).matches_json(&json!("42")));
        assert!(!JsonScalar::String("42".to_string()).matches_json(&json!(42)));
    }

    #[test]
    fn test_filter_chaining() {
        let filter = MetadataFilter::new().eq("a", "1").eq("b", "2").eq("c", "3");

        assert_eq!(filter.len(), 3);

        let meta = json!({
            "a": "1",
            "b": "2",
            "c": "3"
        });
        assert!(filter.matches(&Some(meta)));

        let partial = json!({
            "a": "1",
            "b": "2"
        });
        assert!(!filter.matches(&Some(partial)));
    }
}
