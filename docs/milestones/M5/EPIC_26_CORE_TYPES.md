# Epic 26: Core Types Foundation

**Goal**: Define core JSON types that lock in semantics

**Dependencies**: M4 complete

**GitHub Issue**: #256

---

## Scope

- JsonDocId unique identifier type
- JsonValue enum for all JSON types
- JsonPath and PathSegment types
- JsonPatch mutation types
- Document size limits and validation
- TypeTag::Json and Key::new_json() integration

---

## User Stories

| Story | Description | Priority | GitHub Issue |
|-------|-------------|----------|--------------|
| #225 | JsonDocId Type Definition | FOUNDATION | #263 |
| #226 | JsonValue Type Definition | FOUNDATION | #264 |
| #227 | JsonPath Type Definition | FOUNDATION | #265 |
| #228 | JsonPatch Type Definition | HIGH | #266 |
| #229 | Document Size Limits | HIGH | #267 |

---

## Story #225: JsonDocId Type Definition

**File**: `crates/core/src/types.rs`

**Deliverable**: JsonDocId unique identifier type with TypeTag integration

### Implementation

```rust
use uuid::Uuid;

/// Unique identifier for a JSON document within a run
///
/// Each document has a unique ID that persists for its lifetime.
/// IDs are UUIDs to ensure global uniqueness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JsonDocId(Uuid);

impl JsonDocId {
    /// Create a new unique document ID
    pub fn new() -> Self {
        JsonDocId(Uuid::new_v4())
    }

    /// Create from existing UUID (for deserialization/recovery)
    pub fn from_uuid(uuid: Uuid) -> Self {
        JsonDocId(uuid)
    }

    /// Get the underlying UUID
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }

    /// Get bytes for key encoding
    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

    /// Try to parse from bytes (for key decoding)
    pub fn try_from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() == 16 {
            Uuid::from_slice(bytes).ok().map(JsonDocId)
        } else {
            None
        }
    }
}

impl Default for JsonDocId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for JsonDocId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// Add TypeTag for JSON
pub enum TypeTag {
    KV = 0x01,
    Event = 0x02,
    State = 0x03,
    Trace = 0x04,
    Run = 0x05,
    Vector = 0x10,
    Json = 0x11,  // NEW
}

impl Key {
    /// Create key for JSON document storage
    pub fn new_json(namespace: Namespace, doc_id: &JsonDocId) -> Self {
        Key::new(namespace, TypeTag::Json, doc_id.as_bytes().to_vec())
    }

    /// Create prefix for scanning all JSON docs in namespace
    pub fn new_json_prefix(namespace: Namespace) -> Self {
        Key::new(namespace, TypeTag::Json, vec![])
    }
}
```

### Acceptance Criteria

- [ ] JsonDocId::new() generates unique IDs
- [ ] JsonDocId is Copy, Clone, Hash, Eq
- [ ] JsonDocId serializes/deserializes correctly
- [ ] as_bytes()/try_from_bytes() roundtrip correctly
- [ ] Display shows UUID string
- [ ] TypeTag::Json = 0x11 added
- [ ] Key::new_json() implemented

### Testing

```rust
#[test]
fn test_json_doc_id_unique() {
    let id1 = JsonDocId::new();
    let id2 = JsonDocId::new();
    assert_ne!(id1, id2);
}

#[test]
fn test_json_doc_id_bytes_roundtrip() {
    let id = JsonDocId::new();
    let bytes = id.as_bytes();
    let recovered = JsonDocId::try_from_bytes(bytes).unwrap();
    assert_eq!(id, recovered);
}

#[test]
fn test_key_new_json() {
    let run_id = RunId::new();
    let doc_id = JsonDocId::new();
    let namespace = Namespace::for_run(run_id);
    let key = Key::new_json(namespace, &doc_id);
    assert_eq!(key.type_tag(), TypeTag::Json);
}
```

---

## Story #226: JsonValue Type Definition

**File**: `crates/core/src/json_types.rs` (NEW)

**Deliverable**: JsonValue enum representing all JSON types

### Implementation

```rust
//! JSON value types for M5 JsonStore primitive

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use crate::value::Value;

/// JSON value types supported by JsonStore
///
/// Represents all valid JSON values as defined by RFC 8259.
/// Uses IndexMap for objects to preserve key insertion order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum JsonValue {
    /// JSON null
    Null,
    /// JSON boolean
    Bool(bool),
    /// JSON number (integer or floating point)
    Number(JsonNumber),
    /// JSON string
    String(String),
    /// JSON array
    Array(Vec<JsonValue>),
    /// JSON object (preserves insertion order)
    Object(IndexMap<String, JsonValue>),
}

/// JSON number representation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum JsonNumber {
    /// 64-bit signed integer
    Int(i64),
    /// 64-bit floating point
    Float(f64),
}

impl JsonValue {
    pub fn is_null(&self) -> bool { matches!(self, JsonValue::Null) }
    pub fn is_bool(&self) -> bool { matches!(self, JsonValue::Bool(_)) }
    pub fn is_number(&self) -> bool { matches!(self, JsonValue::Number(_)) }
    pub fn is_string(&self) -> bool { matches!(self, JsonValue::String(_)) }
    pub fn is_array(&self) -> bool { matches!(self, JsonValue::Array(_)) }
    pub fn is_object(&self) -> bool { matches!(self, JsonValue::Object(_)) }

    pub fn as_bool(&self) -> Option<bool> {
        match self { JsonValue::Bool(b) => Some(*b), _ => None }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self { JsonValue::Number(JsonNumber::Int(n)) => Some(*n), _ => None }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            JsonValue::Number(JsonNumber::Float(n)) => Some(*n),
            JsonValue::Number(JsonNumber::Int(n)) => Some(*n as f64),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self { JsonValue::String(s) => Some(s), _ => None }
    }

    pub fn as_array(&self) -> Option<&Vec<JsonValue>> {
        match self { JsonValue::Array(arr) => Some(arr), _ => None }
    }

    pub fn as_array_mut(&mut self) -> Option<&mut Vec<JsonValue>> {
        match self { JsonValue::Array(arr) => Some(arr), _ => None }
    }

    pub fn as_object(&self) -> Option<&IndexMap<String, JsonValue>> {
        match self { JsonValue::Object(obj) => Some(obj), _ => None }
    }

    pub fn as_object_mut(&mut self) -> Option<&mut IndexMap<String, JsonValue>> {
        match self { JsonValue::Object(obj) => Some(obj), _ => None }
    }
}

// Convenience constructors
impl From<bool> for JsonValue {
    fn from(b: bool) -> Self { JsonValue::Bool(b) }
}

impl From<i64> for JsonValue {
    fn from(n: i64) -> Self { JsonValue::Number(JsonNumber::Int(n)) }
}

impl From<i32> for JsonValue {
    fn from(n: i32) -> Self { JsonValue::Number(JsonNumber::Int(n as i64)) }
}

impl From<f64> for JsonValue {
    fn from(n: f64) -> Self { JsonValue::Number(JsonNumber::Float(n)) }
}

impl From<String> for JsonValue {
    fn from(s: String) -> Self { JsonValue::String(s) }
}

impl From<&str> for JsonValue {
    fn from(s: &str) -> Self { JsonValue::String(s.to_string()) }
}

// Conversion to/from existing Value type
impl From<Value> for JsonValue {
    fn from(v: Value) -> Self {
        match v {
            Value::Null => JsonValue::Null,
            Value::Bool(b) => JsonValue::Bool(b),
            Value::I64(n) => JsonValue::Number(JsonNumber::Int(n)),
            Value::F64(n) => JsonValue::Number(JsonNumber::Float(n)),
            Value::String(s) => JsonValue::String(s),
            Value::Bytes(b) => JsonValue::String(base64::encode(b)),
            Value::Array(a) => JsonValue::Array(a.into_iter().map(Into::into).collect()),
            Value::Map(m) => JsonValue::Object(m.into_iter().map(|(k, v)| (k, v.into())).collect()),
        }
    }
}

impl From<JsonValue> for Value {
    fn from(v: JsonValue) -> Self {
        match v {
            JsonValue::Null => Value::Null,
            JsonValue::Bool(b) => Value::Bool(b),
            JsonValue::Number(JsonNumber::Int(n)) => Value::I64(n),
            JsonValue::Number(JsonNumber::Float(n)) => Value::F64(n),
            JsonValue::String(s) => Value::String(s),
            JsonValue::Array(a) => Value::Array(a.into_iter().map(Into::into).collect()),
            JsonValue::Object(m) => Value::Map(m.into_iter().map(|(k, v)| (k, v.into())).collect()),
        }
    }
}
```

### Acceptance Criteria

- [ ] All JSON types represented
- [ ] IndexMap preserves insertion order
- [ ] Type checking methods work
- [ ] Accessor methods return correct values
- [ ] From implementations work for common types
- [ ] From<Value>/Into<Value> conversions work

### Testing

```rust
#[test]
fn test_json_value_types() {
    assert!(JsonValue::Null.is_null());
    assert!(JsonValue::Bool(true).is_bool());
    assert!(JsonValue::from(42i64).is_number());
    assert!(JsonValue::from("hello").is_string());
}

#[test]
fn test_object_preserves_order() {
    let mut obj = IndexMap::new();
    obj.insert("z".to_string(), JsonValue::from(1));
    obj.insert("a".to_string(), JsonValue::from(2));
    obj.insert("m".to_string(), JsonValue::from(3));

    let json = JsonValue::Object(obj);
    let keys: Vec<_> = json.as_object().unwrap().keys().collect();
    assert_eq!(keys, vec!["z", "a", "m"]);
}

#[test]
fn test_value_conversion_roundtrip() {
    let json = JsonValue::from(42i64);
    let value: Value = json.clone().into();
    let back: JsonValue = value.into();
    assert_eq!(json, back);
}
```

---

## Story #227: JsonPath Type Definition

**File**: `crates/core/src/json_types.rs`

**Deliverable**: JsonPath and PathSegment types with overlap detection

### Implementation

```rust
/// A path into a JSON document
///
/// CRITICAL SEMANTIC: Paths are POSITIONAL, not identity-based.
/// $.items[0] refers to "the element at index 0", not "a stable object".
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JsonPath {
    segments: Vec<PathSegment>,
}

/// A single segment in a JSON path
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PathSegment {
    /// Object key: `.foo`
    Key(String),
    /// Array index: `[0]`
    Index(usize),
}

impl JsonPath {
    /// Root path (empty - refers to entire document)
    pub fn root() -> Self {
        JsonPath { segments: Vec::new() }
    }

    /// Create path from segments
    pub fn from_segments(segments: Vec<PathSegment>) -> Self {
        JsonPath { segments }
    }

    /// Parse path from string: "foo.bar[0].baz"
    pub fn parse(s: &str) -> Result<Self, PathParseError> {
        if s.is_empty() { return Ok(Self::root()); }

        let mut segments = Vec::new();
        let mut chars = s.chars().peekable();

        // Skip leading dot or $ if present
        if chars.peek() == Some(&'$') { chars.next(); }
        if chars.peek() == Some(&'.') { chars.next(); }

        while chars.peek().is_some() {
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                let mut num_str = String::new();
                while let Some(&c) = chars.peek() {
                    if c == ']' { break; }
                    if !c.is_ascii_digit() {
                        return Err(PathParseError::InvalidIndex(num_str));
                    }
                    num_str.push(c);
                    chars.next();
                }
                if chars.next() != Some(']') {
                    return Err(PathParseError::UnclosedBracket);
                }
                let index: usize = num_str.parse()
                    .map_err(|_| PathParseError::InvalidIndex(num_str))?;
                segments.push(PathSegment::Index(index));
            } else if chars.peek() == Some(&'.') {
                chars.next();
            } else {
                let mut key = String::new();
                while let Some(&c) = chars.peek() {
                    if c == '.' || c == '[' { break; }
                    key.push(c);
                    chars.next();
                }
                if key.is_empty() { return Err(PathParseError::EmptyKey); }
                segments.push(PathSegment::Key(key));
            }
        }

        Ok(JsonPath { segments })
    }

    /// Append a key segment
    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.segments.push(PathSegment::Key(key.into()));
        self
    }

    /// Append an index segment
    pub fn index(mut self, idx: usize) -> Self {
        self.segments.push(PathSegment::Index(idx));
        self
    }

    pub fn is_root(&self) -> bool { self.segments.is_empty() }
    pub fn segments(&self) -> &[PathSegment] { &self.segments }
    pub fn len(&self) -> usize { self.segments.len() }
    pub fn is_empty(&self) -> bool { self.segments.is_empty() }

    /// Get parent path (None if root)
    pub fn parent(&self) -> Option<JsonPath> {
        if self.segments.is_empty() {
            None
        } else {
            Some(JsonPath {
                segments: self.segments[..self.segments.len() - 1].to_vec(),
            })
        }
    }

    /// Check if this path is an ancestor of another (or equal)
    pub fn is_ancestor_of(&self, other: &JsonPath) -> bool {
        if self.segments.len() > other.segments.len() { return false; }
        self.segments.iter().zip(other.segments.iter()).all(|(a, b)| a == b)
    }

    /// Check if this path is a descendant of another (or equal)
    pub fn is_descendant_of(&self, other: &JsonPath) -> bool {
        other.is_ancestor_of(self)
    }

    /// Check if two paths overlap (one is ancestor/descendant of other, or equal)
    ///
    /// CRITICAL: This is the core conflict detection rule.
    pub fn overlaps(&self, other: &JsonPath) -> bool {
        self.is_ancestor_of(other) || self.is_descendant_of(other)
    }
}

impl std::fmt::Display for JsonPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "$")?;
        for seg in &self.segments {
            match seg {
                PathSegment::Key(k) => write!(f, ".{}", k)?,
                PathSegment::Index(i) => write!(f, "[{}]", i)?,
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PathParseError {
    #[error("invalid array index: {0}")]
    InvalidIndex(String),
    #[error("unclosed bracket")]
    UnclosedBracket,
    #[error("empty key")]
    EmptyKey,
}
```

### Acceptance Criteria

- [ ] Root path is empty segments
- [ ] parse() handles all valid syntax including $ prefix
- [ ] is_ancestor_of() works correctly
- [ ] overlaps() is symmetric
- [ ] Display shows JSONPath notation

### Testing

```rust
#[test]
fn test_path_parse() {
    assert!(JsonPath::parse("").unwrap().is_root());
    assert_eq!(JsonPath::parse("foo").unwrap().len(), 1);
    assert_eq!(JsonPath::parse("foo.bar").unwrap().len(), 2);
    assert_eq!(JsonPath::parse("foo[0]").unwrap().len(), 2);
    assert_eq!(JsonPath::parse("$.foo.bar[0].baz").unwrap().len(), 4);
}

#[test]
fn test_path_overlaps() {
    let root = JsonPath::root();
    let foo = JsonPath::parse("foo").unwrap();
    let foo_bar = JsonPath::parse("foo.bar").unwrap();
    let baz = JsonPath::parse("baz").unwrap();

    assert!(root.overlaps(&foo));      // root is ancestor of foo
    assert!(foo.overlaps(&root));      // foo is descendant of root
    assert!(foo.overlaps(&foo_bar));   // foo is ancestor of foo.bar
    assert!(!foo.overlaps(&baz));      // foo and baz don't overlap
}

#[test]
fn test_path_display() {
    let path = JsonPath::root().key("foo").index(0).key("bar");
    assert_eq!(format!("{}", path), "$.foo[0].bar");
}
```

---

## Story #228: JsonPatch Type Definition

**File**: `crates/core/src/json_types.rs`

**Deliverable**: JsonPatch enum for mutation operations

### Implementation

```rust
/// A patch operation on a JSON document
///
/// Patches are the atomic unit of mutation.
/// WAL entries record patches, not full documents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JsonPatch {
    /// Set value at path
    Set { path: JsonPath, value: JsonValue },
    /// Delete value at path
    Delete { path: JsonPath },
}

impl JsonPatch {
    pub fn set(path: JsonPath, value: impl Into<JsonValue>) -> Self {
        JsonPatch::Set { path, value: value.into() }
    }

    pub fn delete(path: JsonPath) -> Self {
        JsonPatch::Delete { path }
    }

    pub fn path(&self) -> &JsonPath {
        match self {
            JsonPatch::Set { path, .. } => path,
            JsonPatch::Delete { path } => path,
        }
    }

    /// Check if this patch conflicts with another
    pub fn conflicts_with(&self, other: &JsonPatch) -> bool {
        self.path().overlaps(other.path())
    }
}
```

### Acceptance Criteria

- [ ] Set and Delete variants work
- [ ] path() returns correct path
- [ ] conflicts_with() uses overlap detection

### Testing

```rust
#[test]
fn test_patch_conflicts() {
    let patch1 = JsonPatch::set(JsonPath::parse("foo.bar").unwrap(), 42);
    let patch2 = JsonPatch::set(JsonPath::parse("foo.bar.baz").unwrap(), 43);
    let patch3 = JsonPatch::set(JsonPath::parse("other").unwrap(), 44);

    assert!(patch1.conflicts_with(&patch2));  // ancestor/descendant
    assert!(!patch1.conflicts_with(&patch3)); // no overlap
}
```

---

## Story #229: Document Size Limits

**File**: `crates/core/src/json_types.rs`

**Deliverable**: Constants and validation for document limits

### Implementation

```rust
/// Maximum document size in bytes (16 MB)
pub const MAX_DOCUMENT_SIZE: usize = 16 * 1024 * 1024;

/// Maximum nesting depth (100 levels)
pub const MAX_NESTING_DEPTH: usize = 100;

/// Maximum path length (256 segments)
pub const MAX_PATH_LENGTH: usize = 256;

/// Maximum array size (1 million elements)
pub const MAX_ARRAY_SIZE: usize = 1_000_000;

/// Validate a JSON value against size limits
pub fn validate_json_value(value: &JsonValue) -> Result<(), JsonValidationError> {
    validate_depth(value, 0)?;
    validate_array_sizes(value)?;
    Ok(())
}

fn validate_depth(value: &JsonValue, depth: usize) -> Result<(), JsonValidationError> {
    if depth > MAX_NESTING_DEPTH {
        return Err(JsonValidationError::NestingTooDeep { depth, max: MAX_NESTING_DEPTH });
    }
    match value {
        JsonValue::Array(arr) => {
            for item in arr { validate_depth(item, depth + 1)?; }
        }
        JsonValue::Object(obj) => {
            for (_, v) in obj { validate_depth(v, depth + 1)?; }
        }
        _ => {}
    }
    Ok(())
}

fn validate_array_sizes(value: &JsonValue) -> Result<(), JsonValidationError> {
    match value {
        JsonValue::Array(arr) => {
            if arr.len() > MAX_ARRAY_SIZE {
                return Err(JsonValidationError::ArrayTooLarge {
                    size: arr.len(),
                    max: MAX_ARRAY_SIZE
                });
            }
            for item in arr { validate_array_sizes(item)?; }
        }
        JsonValue::Object(obj) => {
            for (_, v) in obj { validate_array_sizes(v)?; }
        }
        _ => {}
    }
    Ok(())
}

pub fn validate_path(path: &JsonPath) -> Result<(), JsonValidationError> {
    if path.len() > MAX_PATH_LENGTH {
        return Err(JsonValidationError::PathTooLong {
            length: path.len(),
            max: MAX_PATH_LENGTH
        });
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum JsonValidationError {
    #[error("nesting too deep: {depth} levels (max {max})")]
    NestingTooDeep { depth: usize, max: usize },
    #[error("array too large: {size} elements (max {max})")]
    ArrayTooLarge { size: usize, max: usize },
    #[error("path too long: {length} segments (max {max})")]
    PathTooLong { length: usize, max: usize },
    #[error("document too large: {size} bytes (max {max})")]
    DocumentTooLarge { size: usize, max: usize },
}
```

### Acceptance Criteria

- [ ] Limits match specification
- [ ] validate_json_value() catches deep nesting
- [ ] validate_json_value() catches large arrays
- [ ] validate_path() catches long paths

### Testing

```rust
#[test]
fn test_depth_validation() {
    // Create deeply nested structure
    let mut value = JsonValue::from(42);
    for _ in 0..101 {
        value = JsonValue::Array(vec![value]);
    }
    assert!(validate_json_value(&value).is_err());
}

#[test]
fn test_array_size_validation() {
    let large_array: Vec<JsonValue> = (0..1_000_001).map(|i| JsonValue::from(i as i64)).collect();
    let value = JsonValue::Array(large_array);
    assert!(validate_json_value(&value).is_err());
}
```

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/core/src/types.rs` | MODIFY - Add TypeTag::Json, Key::new_json(), JsonDocId |
| `crates/core/src/json_types.rs` | CREATE - JsonValue, JsonPath, JsonPatch, validation |
| `crates/core/src/lib.rs` | MODIFY - Export json_types module |
| `crates/core/Cargo.toml` | MODIFY - Add indexmap dependency |
