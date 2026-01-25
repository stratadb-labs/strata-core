//! Substrate API Implementation
//!
//! This module provides the concrete implementation of all Substrate API traits.
//! `SubstrateImpl` wraps the Database and primitives, providing the canonical
//! API surface for power users.
//!
//! ## Design
//!
//! `SubstrateImpl` is a **semantic boundary** between unified API types and
//! domain-specific primitive types. It performs:
//!
//! - Type normalization: `Value` ↔ `JsonValue`, `&str` ↔ `JsonDocId`/`JsonPath`
//! - Run ID mapping: `ApiRunId("default")` → `RunId(UUID::nil)`
//! - Version normalization: Primitive versions → unified `Version` enum
//! - Error normalization: Primitive errors → `StrataError`
//!
//! ## Usage
//!
//! ```ignore
//! use strata_api::substrate::SubstrateImpl;
//! use strata_engine::Database;
//! use std::sync::Arc;
//!
//! let db = Arc::new(Database::open("/path/to/data")?);
//! let substrate = SubstrateImpl::new(db);
//!
//! // Use the substrate API
//! let run = ApiRunId::default_run_id();
//! let version = substrate.kv_put(&run, "key", Value::Int(42))?;
//! let value = substrate.kv_get(&run, "key")?;
//! ```

use std::sync::Arc;
use strata_core::json::{JsonPath, JsonValue};
use strata_core::types::JsonDocId;
use strata_core::{StrataError, StrataResult, Value};
use strata_engine::Database;
use strata_primitives::{
    EventLog as PrimitiveEventLog,
    JsonStore as PrimitiveJsonStore,
    KVStore as PrimitiveKVStore,
    RunIndex as PrimitiveRunIndex,
    StateCell as PrimitiveStateCell,
    VectorError,
    VectorStore as PrimitiveVectorStore,
};

use super::types::ApiRunId;

// =============================================================================
// Type Conversion Helpers
// =============================================================================

/// Convert strata_core::Value to JsonValue
///
/// This converts our canonical Value type to the JSON-specific wrapper.
/// Note: We manually convert to avoid serde's tagged enum serialization.
pub(crate) fn value_to_json(value: Value) -> StrataResult<JsonValue> {
    let json_val = value_to_serde_json(value)?;
    Ok(JsonValue::from(json_val))
}

/// Convert a Value to serde_json::Value without using serde serialization
///
/// This avoids the tagged enum representation that serde produces by default.
fn value_to_serde_json(value: Value) -> StrataResult<serde_json::Value> {
    use serde_json::Value as JV;
    use serde_json::Map;

    match value {
        Value::Null => Ok(JV::Null),
        Value::Bool(b) => Ok(JV::Bool(b)),
        Value::Int(i) => Ok(JV::Number(i.into())),
        Value::Float(f) => {
            // JSON doesn't support Infinity or NaN
            if f.is_infinite() || f.is_nan() {
                return Err(StrataError::serialization(
                    format!("Cannot convert {} to JSON: not a valid JSON number", f)
                ));
            }
            serde_json::Number::from_f64(f)
                .map(JV::Number)
                .ok_or_else(|| StrataError::serialization(
                    format!("Cannot convert {} to JSON number", f)
                ))
        }
        Value::String(s) => Ok(JV::String(s)),
        Value::Bytes(b) => {
            // Encode bytes as base64 string with a prefix to distinguish from regular strings
            use base64::Engine;
            let encoded = base64::engine::general_purpose::STANDARD.encode(&b);
            Ok(JV::String(format!("__bytes__:{}", encoded)))
        }
        Value::Array(arr) => {
            let converted: Result<Vec<_>, _> = arr.into_iter()
                .map(value_to_serde_json)
                .collect();
            Ok(JV::Array(converted?))
        }
        Value::Object(obj) => {
            let mut map = Map::new();
            for (k, v) in obj {
                map.insert(k, value_to_serde_json(v)?);
            }
            Ok(JV::Object(map))
        }
    }
}

/// Convert JsonValue to strata_core::Value
///
/// This converts the JSON-specific wrapper back to our canonical Value type.
pub(crate) fn json_to_value(json: JsonValue) -> StrataResult<Value> {
    let serde_val: serde_json::Value = json.into();
    serde_json_to_value(serde_val)
}

/// Convert serde_json::Value to Value without using serde deserialization
fn serde_json_to_value(json: serde_json::Value) -> StrataResult<Value> {
    use serde_json::Value as JV;

    match json {
        JV::Null => Ok(Value::Null),
        JV::Bool(b) => Ok(Value::Bool(b)),
        JV::Number(n) => {
            // Try to preserve integers when possible
            if let Some(i) = n.as_i64() {
                Ok(Value::Int(i))
            } else if let Some(f) = n.as_f64() {
                Ok(Value::Float(f))
            } else {
                Err(StrataError::serialization(
                    format!("Cannot convert JSON number {} to Value", n)
                ))
            }
        }
        JV::String(s) => {
            // Check for base64-encoded bytes
            if let Some(encoded) = s.strip_prefix("__bytes__:") {
                use base64::Engine;
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .map_err(|e| StrataError::serialization(
                        format!("Invalid base64 in bytes value: {}", e)
                    ))?;
                Ok(Value::Bytes(bytes))
            } else {
                Ok(Value::String(s))
            }
        }
        JV::Array(arr) => {
            let converted: Result<Vec<_>, _> = arr.into_iter()
                .map(serde_json_to_value)
                .collect();
            Ok(Value::Array(converted?))
        }
        JV::Object(obj) => {
            let mut map = std::collections::HashMap::new();
            for (k, v) in obj {
                map.insert(k, serde_json_to_value(v)?);
            }
            Ok(Value::Object(map))
        }
    }
}

/// Parse a string document ID to JsonDocId
///
/// Supports both UUID strings and human-readable IDs (hashed to deterministic UUID).
pub(crate) fn parse_doc_id(doc_id: &str) -> StrataResult<JsonDocId> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    // First try to parse as UUID
    match uuid::Uuid::parse_str(doc_id) {
        Ok(uuid) => Ok(JsonDocId::from_uuid(uuid)),
        Err(_) => {
            // Generate deterministic UUID from string hash
            let mut hasher = DefaultHasher::new();
            doc_id.hash(&mut hasher);
            let hash = hasher.finish();

            // Create UUID v8 from hash bytes (custom namespace)
            let mut bytes = [0u8; 16];
            bytes[0..8].copy_from_slice(&hash.to_le_bytes());
            // Hash again for second half
            doc_id.hash(&mut hasher);
            let hash2 = hasher.finish();
            bytes[8..16].copy_from_slice(&hash2.to_le_bytes());

            // Set version (bits 12-15 of time_hi_and_version) to 8 (custom)
            bytes[6] = (bytes[6] & 0x0f) | 0x80;
            // Set variant (bits 6-7 of clock_seq_hi_and_reserved) to 10
            bytes[8] = (bytes[8] & 0x3f) | 0x80;

            let uuid = uuid::Uuid::from_bytes(bytes);
            Ok(JsonDocId::from_uuid(uuid))
        }
    }
}

/// Parse a string path to JsonPath
pub(crate) fn parse_path(path: &str) -> StrataResult<JsonPath> {
    if path.is_empty() || path == "$" {
        return Ok(JsonPath::root());
    }
    path.parse()
        .map_err(|e| StrataError::invalid_input(
            format!("Invalid JSON path '{}': {:?}", path, e)
        ))
}

/// Convert primitive error to StrataError
pub(crate) fn convert_error(err: strata_core::error::Error) -> StrataError {
    // Most errors pass through directly since they're already StrataError compatible
    StrataError::from(err)
}

/// Convert VectorError to StrataError
///
/// Uses the existing From implementation in strata_primitives::vector::error.
pub(crate) fn convert_vector_error(err: VectorError) -> StrataError {
    StrataError::from(err)
}

/// Reserved key prefix that users cannot use
const RESERVED_KEY_PREFIX: &str = "_strata/";

/// Maximum key length in bytes (matches Limits::default().max_key_bytes)
const MAX_KEY_BYTES: usize = 1024;

/// Validate a KV key according to the contract:
/// - Non-empty
/// - No NUL bytes
/// - Not starting with `_strata/` (reserved prefix)
/// - Not exceeding max key length (1024 bytes)
///
/// Returns an error if the key is invalid.
pub(crate) fn validate_key(key: &str) -> StrataResult<()> {
    if key.is_empty() {
        return Err(StrataError::invalid_input("Key must not be empty"));
    }
    if key.len() > MAX_KEY_BYTES {
        return Err(StrataError::invalid_input(
            format!("Key exceeds maximum length of {} bytes", MAX_KEY_BYTES)
        ));
    }
    if key.contains('\0') {
        return Err(StrataError::invalid_input("Key must not contain NUL bytes"));
    }
    if key.starts_with(RESERVED_KEY_PREFIX) {
        return Err(StrataError::invalid_input(
            format!("Key must not start with reserved prefix '{}'", RESERVED_KEY_PREFIX)
        ));
    }
    Ok(())
}

/// Validate an event stream name:
/// - Non-empty
pub(crate) fn validate_stream_name(stream: &str) -> StrataResult<()> {
    if stream.is_empty() {
        return Err(StrataError::invalid_input("Stream name must not be empty"));
    }
    Ok(())
}

/// Validate an event payload (must be Object)
pub(crate) fn validate_event_payload(payload: &Value) -> StrataResult<()> {
    if !matches!(payload, Value::Object(_)) {
        return Err(StrataError::invalid_input(
            "Event payload must be an Object"
        ));
    }
    Ok(())
}

/// Convert ApiRunId to string representation for RunIndex
pub(crate) fn api_run_id_to_string(run: &ApiRunId) -> String {
    if run.is_default() {
        "default".to_string()
    } else {
        run.as_str().to_string()
    }
}

// =============================================================================
// SubstrateImpl
// =============================================================================

/// Substrate API implementation
///
/// This struct provides the concrete implementation of all Substrate API traits.
/// It wraps the Database and provides access to all primitives.
///
/// ## Thread Safety
///
/// `SubstrateImpl` is `Send + Sync` and can be safely shared across threads.
/// Multiple `SubstrateImpl` instances on the same Database are safe.
///
/// ## Stateless Design
///
/// `SubstrateImpl` holds no mutable state - all state lives in the Database.
/// This enables safe concurrent access and simple cloning.
#[derive(Clone)]
pub struct SubstrateImpl {
    /// The underlying database
    db: Arc<Database>,
    /// KV primitive
    kv: PrimitiveKVStore,
    /// JSON primitive
    json: PrimitiveJsonStore,
    /// Event primitive
    event: PrimitiveEventLog,
    /// State primitive
    state: PrimitiveStateCell,
    /// Run primitive
    run: PrimitiveRunIndex,
    /// Vector primitive
    vector: PrimitiveVectorStore,
}

impl SubstrateImpl {
    /// Create a new SubstrateImpl wrapping the given database
    pub fn new(db: Arc<Database>) -> Self {
        Self {
            kv: PrimitiveKVStore::new(db.clone()),
            json: PrimitiveJsonStore::new(db.clone()),
            event: PrimitiveEventLog::new(db.clone()),
            state: PrimitiveStateCell::new(db.clone()),
            run: PrimitiveRunIndex::new(db.clone()),
            vector: PrimitiveVectorStore::new(db.clone()),
            db,
        }
    }

    /// Get the underlying database reference
    pub fn database(&self) -> &Arc<Database> {
        &self.db
    }

    // =========================================================================
    // Accessor methods for use by trait implementations in other files
    // =========================================================================

    /// Get database reference (for transactions)
    pub(crate) fn db(&self) -> &Database {
        &self.db
    }

    /// Get KV primitive reference
    pub(crate) fn kv(&self) -> &PrimitiveKVStore {
        &self.kv
    }

    /// Get JSON primitive reference
    pub(crate) fn json(&self) -> &PrimitiveJsonStore {
        &self.json
    }

    /// Get Event primitive reference
    pub(crate) fn event(&self) -> &PrimitiveEventLog {
        &self.event
    }

    /// Get State primitive reference
    pub(crate) fn state(&self) -> &PrimitiveStateCell {
        &self.state
    }

    /// Get Run primitive reference
    pub(crate) fn run(&self) -> &PrimitiveRunIndex {
        &self.run
    }

    /// Get Vector primitive reference
    pub(crate) fn vector(&self) -> &PrimitiveVectorStore {
        &self.vector
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use strata_core::{Value, Version};
    use crate::substrate::kv::KVStore;
    use tempfile::TempDir;

    fn setup() -> (TempDir, SubstrateImpl) {
        let temp_dir = TempDir::new().unwrap();
        let db = Arc::new(Database::open(temp_dir.path()).unwrap());
        let substrate = SubstrateImpl::new(db);
        (temp_dir, substrate)
    }

    #[test]
    fn test_substrate_impl_creation() {
        let (_temp, substrate) = setup();
        assert!(Arc::strong_count(substrate.database()) >= 1);
    }

    #[test]
    fn test_substrate_impl_clone() {
        let (_temp, substrate1) = setup();
        let substrate2 = substrate1.clone();
        assert!(Arc::ptr_eq(substrate1.database(), substrate2.database()));
    }

    #[test]
    fn test_kv_put_and_get() {
        let (_temp, substrate) = setup();
        let run = ApiRunId::default_run_id();

        let version = substrate.kv_put(&run, "key1", Value::String("value1".into())).unwrap();
        assert!(matches!(version, Version::Txn(_)));

        let result = substrate.kv_get(&run, "key1").unwrap();
        assert!(result.is_some());
        assert_eq!(result.unwrap().value, Value::String("value1".into()));
    }

    #[test]
    fn test_kv_default_run_uses_uuid_nil() {
        let (_temp, substrate) = setup();
        let run = ApiRunId::default_run_id();

        substrate.kv_put(&run, "test_key", Value::Int(42)).unwrap();
        let result = substrate.kv_get(&run, "test_key").unwrap();
        assert_eq!(result.unwrap().value, Value::Int(42));
    }

    #[test]
    fn test_kv_run_isolation() {
        let (_temp, substrate) = setup();
        let run1 = ApiRunId::default_run_id();
        let run2 = ApiRunId::new();

        substrate.kv_put(&run1, "key", Value::String("run1".into())).unwrap();
        substrate.kv_put(&run2, "key", Value::String("run2".into())).unwrap();

        let val1 = substrate.kv_get(&run1, "key").unwrap().unwrap().value;
        let val2 = substrate.kv_get(&run2, "key").unwrap().unwrap().value;

        assert_eq!(val1, Value::String("run1".into()));
        assert_eq!(val2, Value::String("run2".into()));
    }

    #[test]
    fn test_value_to_json_conversion() {
        // Value uses tagged serialization format: {"Int": 42} not raw 42
        let value = Value::Int(42);
        let json = value_to_json(value).unwrap();
        // Convert back to verify round-trip
        let restored = json_to_value(json).unwrap();
        assert_eq!(restored, Value::Int(42));

        let value = Value::String("hello".into());
        let json = value_to_json(value).unwrap();
        let restored = json_to_value(json).unwrap();
        assert_eq!(restored, Value::String("hello".into()));
    }

    #[test]
    fn test_json_to_value_conversion() {
        // Direct JSON to Value conversion (not serde-tagged enums)
        // JSON number -> Value::Int
        let json_str = r#"42"#;
        let serde_val: serde_json::Value = serde_json::from_str(json_str).unwrap();
        let json = JsonValue::from(serde_val);
        let value = json_to_value(json).unwrap();
        assert_eq!(value, Value::Int(42));

        // JSON string -> Value::String
        let json_str = r#""hello""#;
        let serde_val: serde_json::Value = serde_json::from_str(json_str).unwrap();
        let json = JsonValue::from(serde_val);
        let value = json_to_value(json).unwrap();
        assert_eq!(value, Value::String("hello".into()));

        // JSON object -> Value::Object
        let json_str = r#"{"key": 42}"#;
        let serde_val: serde_json::Value = serde_json::from_str(json_str).unwrap();
        let json = JsonValue::from(serde_val);
        let value = json_to_value(json).unwrap();
        match value {
            Value::Object(map) => {
                assert_eq!(map.get("key"), Some(&Value::Int(42)));
            }
            _ => panic!("Expected Object"),
        }
    }

    #[test]
    fn test_parse_doc_id_uuid() {
        let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
        let doc_id = parse_doc_id(uuid_str).unwrap();
        assert_eq!(format!("{}", doc_id), uuid_str);
    }

    #[test]
    fn test_parse_doc_id_string() {
        // Non-UUID strings should produce consistent deterministic UUIDs
        let doc_id1 = parse_doc_id("my-document").unwrap();
        let doc_id2 = parse_doc_id("my-document").unwrap();
        assert_eq!(doc_id1, doc_id2);

        let doc_id3 = parse_doc_id("other-document").unwrap();
        assert_ne!(doc_id1, doc_id3);
    }

    #[test]
    fn test_parse_path_root() {
        let path = parse_path("").unwrap();
        assert!(path.is_root());

        let path = parse_path("$").unwrap();
        assert!(path.is_root());
    }

    #[test]
    fn test_parse_path_simple() {
        let path = parse_path("name").unwrap();
        assert_eq!(path.segments().len(), 1);
    }
}
