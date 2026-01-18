# M5 Epics and User Stories: JSON Primitive

**Milestone**: M5 - JSON Primitive
**Goal**: Implement a native JSON primitive as a first-class citizen in the agent database
**Estimated Duration**: 8-9 days
**Architecture Spec**: v1.1

---

## Critical Implementation Invariants

Before implementing any M5 story, understand these invariants:

### 1. Path Semantics Are Positional, Not Identity-Based
> **Paths like `$.items[0]` refer to positions, not stable identities.**

Insertions and removals change what a path refers to. Paths are views, not references.

### 2. JSON Mutations Are Path-Based, Not Value-Based
> **All JSON writes are defined as mutations to paths, not replacements of opaque values.**

WAL records patches (path + operation), never full documents.

### 3. Conflict Detection Is Region-Based
> **Two JSON operations conflict if and only if their paths overlap.**

Overlap means: ancestor, descendant, or equal paths.

### 4. Required Dependencies
```toml
indexmap = "2"  # Ordered map for JSON objects (preserves insertion order)
# rmp-serde already present for MessagePack serialization
```

### 5. Architectural Integration Rules (NON-NEGOTIABLE)

These rules ensure M5 integrates properly with the M1-M4 architecture:

**Rule 1: JSON Lives Inside ShardedStore**
- Documents stored as: `Key { namespace, TypeTag::Json, doc_id_bytes } -> VersionedValue`
- NO separate DashMap storage
- Uses existing sharding, versioning, snapshots, WAL, recovery

**Rule 2: JsonStore Is a Stateless Facade**
- `pub struct JsonStore { db: Arc<Database> }` - ONLY this
- No internal maps, locks, or state
- Exactly like KVStore, EventLog, StateCell, Trace, RunIndex

**Rule 3: JSON Extends TransactionContext, Not Replaces It**
- Uses JsonStoreExt trait on TransactionContext
- No separate JsonTransactionState
- Hooks into existing commit pipeline

**Rule 4: Path-Level Semantics Live in Validation, Not Storage**
- Storage sees: `Key::new_json(namespace, doc_id) -> VersionedValue`
- Validation sees: `(Key, JsonPath) -> version`
- Path-level conflict detection during validation, not storage

**Rule 5: WAL Remains Unified**
- New entry variants (0x20-0x23) added to existing WALEntry enum
- Replay, ordering, durability remain unified

**Rule 6: JSON API Must Feel Like Every Other Primitive**
- `json.get(&run_id, &doc_id, &path)?`
- `json.transaction(&run_id, |txn| { txn.json_set(...) })?`

---

## Epic 26: Core Types Foundation

**Goal**: Define core JSON types that lock in semantics

### Scope
- JsonDocId unique identifier type
- JsonValue enum for all JSON types
- JsonPath and PathSegment types
- JsonPatch mutation types
- Document size limits and validation

### Success Criteria
- [ ] JsonDocId generates unique, hashable identifiers
- [ ] JsonValue represents all JSON types with IndexMap for objects
- [ ] JsonPath supports parsing, display, and overlap detection
- [ ] JsonPatch defines Set and Delete operations
- [ ] Size limits enforced: 16MB doc, 100 depth, 256 path segments

### Dependencies
- M4 complete

### Estimated Effort
1 day with 2 Claudes in parallel

### User Stories
- **#225**: JsonDocId Type Definition (2 hours) 🔴 FOUNDATION
- **#226**: JsonValue Type Definition (3 hours)
- **#227**: JsonPath Type Definition (4 hours)
- **#228**: JsonPatch Type Definition (2 hours)
- **#229**: Document Size Limits (2 hours)

### Parallelization
After #225, stories #226-229 can run in parallel (4 Claudes)

---

### Story #225: JsonDocId Type Definition

**File**: `crates/core/src/types.rs`

**Deliverable**: JsonDocId unique identifier type

**Implementation**:
```rust
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
```

**Acceptance Criteria**:
- [ ] JsonDocId::new() generates unique IDs
- [ ] JsonDocId is Copy, Clone, Hash, Eq
- [ ] JsonDocId serializes/deserializes correctly
- [ ] Display shows UUID string

---

### Story #226: JsonValue Type Definition

**File**: `crates/core/src/json.rs` (NEW)

**Deliverable**: JsonValue enum representing all JSON types

**Implementation**:
```rust
//! JSON value types for M5 JsonStore primitive

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

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
```

**Acceptance Criteria**:
- [ ] All JSON types represented
- [ ] IndexMap preserves insertion order
- [ ] Type checking methods work
- [ ] Accessor methods return correct values
- [ ] From implementations work for common types

---

### Story #227: JsonPath Type Definition

**File**: `crates/core/src/json.rs`

**Deliverable**: JsonPath and PathSegment types with overlap detection

**Implementation**:
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

        // Skip leading dot if present
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

**Acceptance Criteria**:
- [ ] Root path is empty segments
- [ ] parse() handles all valid syntax
- [ ] is_ancestor_of() works correctly
- [ ] overlaps() is symmetric
- [ ] Display shows JSONPath notation

---

### Story #228: JsonPatch Type Definition

**File**: `crates/core/src/json.rs`

**Deliverable**: JsonPatch enum for mutation operations

**Implementation**:
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

**Acceptance Criteria**:
- [ ] Set and Delete variants work
- [ ] path() returns correct path
- [ ] conflicts_with() uses overlap detection

---

### Story #229: Document Size Limits

**File**: `crates/core/src/json.rs`

**Deliverable**: Constants and validation for document limits

**Implementation**:
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
                return Err(JsonValidationError::ArrayTooLarge { size: arr.len(), max: MAX_ARRAY_SIZE });
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
        return Err(JsonValidationError::PathTooLong { length: path.len(), max: MAX_PATH_LENGTH });
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
    DocumentTooLarge { size: u64, max: u64 },
}
```

**Acceptance Criteria**:
- [ ] Limits match specification
- [ ] validate_json_value() catches deep nesting
- [ ] validate_json_value() catches large arrays
- [ ] validate_path() catches long paths

---

## Epic 27: Path Operations

**Goal**: Implement path traversal and manipulation

### Scope
- Path traversal (get value at path)
- Path mutation (set value at path)
- Path deletion
- Intermediate path creation

### Success Criteria
- [ ] get_at_path() navigates objects and arrays correctly
- [ ] set_at_path() creates intermediate structures as needed
- [ ] delete_at_path() removes values and cleans up empty containers
- [ ] Type mismatches return appropriate errors

### Dependencies
- Epic 26 complete

### Estimated Effort
1 day with 2 Claudes in parallel

### User Stories
- **#230**: Path Traversal (Get) (3 hours)
- **#231**: Path Mutation (Set) (4 hours) 🔴 CRITICAL
- **#232**: Path Deletion (3 hours)
- **#233**: Intermediate Path Creation (3 hours)

### Parallelization
After #230, stories #231-233 can run in parallel

---

### Story #230: Path Traversal (Get)

**File**: `crates/core/src/json.rs`

**Deliverable**: Function to get value at path

**Implementation**:
```rust
/// Get value at path within a JSON value
///
/// Returns None if path does not exist.
pub fn get_at_path(root: &JsonValue, path: &JsonPath) -> Option<&JsonValue> {
    let mut current = root;

    for segment in path.segments() {
        match (current, segment) {
            (JsonValue::Object(obj), PathSegment::Key(key)) => {
                current = obj.get(key)?;
            }
            (JsonValue::Array(arr), PathSegment::Index(idx)) => {
                current = arr.get(*idx)?;
            }
            _ => return None, // Type mismatch
        }
    }

    Some(current)
}

/// Get mutable value at path
pub fn get_at_path_mut(root: &mut JsonValue, path: &JsonPath) -> Option<&mut JsonValue> {
    let mut current = root;

    for segment in path.segments() {
        current = match (current, segment) {
            (JsonValue::Object(obj), PathSegment::Key(key)) => obj.get_mut(key)?,
            (JsonValue::Array(arr), PathSegment::Index(idx)) => arr.get_mut(*idx)?,
            _ => return None,
        };
    }

    Some(current)
}
```

**Acceptance Criteria**:
- [ ] get_at_path returns root for empty path
- [ ] get_at_path navigates object keys
- [ ] get_at_path navigates array indices
- [ ] get_at_path returns None for missing paths
- [ ] get_at_path returns None for type mismatches

---

### Story #231: Path Mutation (Set)

**File**: `crates/core/src/json.rs`

**Deliverable**: Function to set value at path

**Implementation**:
```rust
/// Set value at path within a JSON value
///
/// Creates intermediate objects/arrays as needed.
pub fn set_at_path(
    root: &mut JsonValue,
    path: &JsonPath,
    value: JsonValue,
) -> Result<(), JsonPathError> {
    if path.is_root() {
        *root = value;
        return Ok(());
    }

    let segments = path.segments();
    let (parent_segments, last_segment) = segments.split_at(segments.len() - 1);

    // Navigate to parent, creating intermediates as needed
    let mut current = root;
    for (i, segment) in parent_segments.iter().enumerate() {
        current = ensure_and_navigate(current, segment, &segments[i + 1])?;
    }

    // Set the value at the last segment
    match (&mut *current, &last_segment[0]) {
        (JsonValue::Object(obj), PathSegment::Key(key)) => {
            obj.insert(key.clone(), value);
            Ok(())
        }
        (JsonValue::Array(arr), PathSegment::Index(idx)) => {
            if *idx < arr.len() {
                arr[*idx] = value;
                Ok(())
            } else if *idx == arr.len() {
                arr.push(value);
                Ok(())
            } else {
                Err(JsonPathError::IndexOutOfBounds { index: *idx, len: arr.len() })
            }
        }
        (JsonValue::Object(_), PathSegment::Index(_)) => {
            Err(JsonPathError::TypeMismatch { expected: "array", found: "object" })
        }
        (JsonValue::Array(_), PathSegment::Key(_)) => {
            Err(JsonPathError::TypeMismatch { expected: "object", found: "array" })
        }
        _ => Err(JsonPathError::TypeMismatch { expected: "container", found: "scalar" }),
    }
}

fn ensure_and_navigate<'a>(
    current: &'a mut JsonValue,
    segment: &PathSegment,
    next_segment: &PathSegment,
) -> Result<&'a mut JsonValue, JsonPathError> {
    match (current, segment) {
        (JsonValue::Object(obj), PathSegment::Key(key)) => {
            if !obj.contains_key(key) {
                let new_value = match next_segment {
                    PathSegment::Key(_) => JsonValue::Object(IndexMap::new()),
                    PathSegment::Index(_) => JsonValue::Array(Vec::new()),
                };
                obj.insert(key.clone(), new_value);
            }
            Ok(obj.get_mut(key).unwrap())
        }
        (JsonValue::Array(arr), PathSegment::Index(idx)) => {
            if *idx < arr.len() {
                Ok(&mut arr[*idx])
            } else {
                Err(JsonPathError::IndexOutOfBounds { index: *idx, len: arr.len() })
            }
        }
        _ => Err(JsonPathError::TypeMismatch { expected: "container", found: "scalar" }),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum JsonPathError {
    #[error("index out of bounds: {index} (len: {len})")]
    IndexOutOfBounds { index: usize, len: usize },
    #[error("type mismatch: expected {expected}, found {found}")]
    TypeMismatch { expected: &'static str, found: &'static str },
    #[error("path not found")]
    NotFound,
}
```

**Acceptance Criteria**:
- [ ] set_at_path replaces root for empty path
- [ ] set_at_path creates intermediate objects
- [ ] set_at_path creates intermediate arrays (for push only)
- [ ] set_at_path fails for out-of-bounds array indices
- [ ] set_at_path fails for type mismatches

---

### Story #232: Path Deletion

**File**: `crates/core/src/json.rs`

**Deliverable**: Function to delete value at path

**Implementation**:
```rust
/// Delete value at path within a JSON value
///
/// Returns the deleted value if it existed.
pub fn delete_at_path(
    root: &mut JsonValue,
    path: &JsonPath,
) -> Result<Option<JsonValue>, JsonPathError> {
    if path.is_root() {
        let old = std::mem::replace(root, JsonValue::Null);
        return Ok(Some(old));
    }

    let segments = path.segments();
    let (parent_segments, last_segment) = segments.split_at(segments.len() - 1);

    // Navigate to parent
    let mut current = root;
    for segment in parent_segments {
        current = match (current, segment) {
            (JsonValue::Object(obj), PathSegment::Key(key)) => {
                obj.get_mut(key).ok_or(JsonPathError::NotFound)?
            }
            (JsonValue::Array(arr), PathSegment::Index(idx)) => {
                arr.get_mut(*idx).ok_or(JsonPathError::NotFound)?
            }
            _ => return Err(JsonPathError::NotFound),
        };
    }

    // Delete at the last segment
    match (&mut *current, &last_segment[0]) {
        (JsonValue::Object(obj), PathSegment::Key(key)) => {
            Ok(obj.swap_remove(key))
        }
        (JsonValue::Array(arr), PathSegment::Index(idx)) => {
            if *idx < arr.len() {
                Ok(Some(arr.remove(*idx)))
            } else {
                Ok(None)
            }
        }
        _ => Ok(None),
    }
}
```

**Acceptance Criteria**:
- [ ] delete_at_path replaces root with Null
- [ ] delete_at_path removes object keys
- [ ] delete_at_path removes array elements (shifts indices)
- [ ] delete_at_path returns deleted value
- [ ] delete_at_path returns None for non-existent paths

---

### Story #233: Intermediate Path Creation

**File**: `crates/core/src/json.rs`

**Deliverable**: Helper functions for path creation

**Implementation**:
```rust
/// Ensure all intermediate paths exist, creating empty containers as needed
pub fn ensure_path(root: &mut JsonValue, path: &JsonPath) -> Result<(), JsonPathError> {
    if path.is_root() { return Ok(()); }

    let segments = path.segments();
    let mut current = root;

    for (i, segment) in segments.iter().enumerate() {
        let is_last = i == segments.len() - 1;
        let next_is_index = if is_last {
            false
        } else {
            matches!(segments[i + 1], PathSegment::Index(_))
        };

        match (current, segment) {
            (JsonValue::Object(obj), PathSegment::Key(key)) => {
                if !obj.contains_key(key) {
                    let new_value = if next_is_index {
                        JsonValue::Array(Vec::new())
                    } else {
                        JsonValue::Object(IndexMap::new())
                    };
                    obj.insert(key.clone(), new_value);
                }
                current = obj.get_mut(key).unwrap();
            }
            (JsonValue::Array(arr), PathSegment::Index(idx)) => {
                if *idx >= arr.len() {
                    return Err(JsonPathError::IndexOutOfBounds { index: *idx, len: arr.len() });
                }
                current = &mut arr[*idx];
            }
            _ => return Err(JsonPathError::TypeMismatch { expected: "container", found: "scalar" }),
        }
    }

    Ok(())
}
```

**Acceptance Criteria**:
- [ ] ensure_path creates nested objects
- [ ] ensure_path creates nested arrays where appropriate
- [ ] ensure_path fails for out-of-bounds array access
- [ ] ensure_path is idempotent

---

## Epic 28: JsonStore Core

**Goal**: Implement the JsonStore facade with full API

### Scope
- JsonDoc internal structure
- JsonStore struct definition
- Document create/delete operations
- Path-based get/set/delete operations
- Exists/list operations
- MessagePack serialization

### Success Criteria
- [ ] JsonDoc stores value, version, created_at, updated_at
- [ ] JsonStore provides thread-safe document storage
- [ ] CRUD operations work correctly
- [ ] Version increments on every mutation
- [ ] MessagePack serialization roundtrips correctly

### Dependencies
- Epic 27 complete

### Estimated Effort
1.5 days with 3 Claudes in parallel

### User Stories
- **#234**: JsonDoc Internal Structure (2 hours) 🔴 FOUNDATION
- **#235**: JsonStore Struct Definition (3 hours) 🔴 FOUNDATION
- **#236**: Document Create/Delete (3 hours)
- **#237**: Document Get/Set/Delete at Path (4 hours)
- **#238**: Document Exists/List (2 hours)
- **#239**: MessagePack Serialization (3 hours)

### Parallelization
After #234-235, stories #236-239 can run in parallel

---

### Story #234: JsonDoc Internal Structure

**File**: `crates/core/src/json.rs`

**Deliverable**: JsonDoc struct with version tracking

**Implementation**:
```rust
use std::time::SystemTime;

/// Internal document representation
///
/// DESIGN: Document-level versioning (single version for entire doc)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonDoc {
    /// Document unique identifier
    pub id: JsonDocId,
    /// The JSON value (root of document)
    pub value: JsonValue,
    /// Document version (increments on any change)
    pub version: u64,
    /// Creation timestamp
    pub created_at: SystemTime,
    /// Last modification timestamp
    pub updated_at: SystemTime,
}

impl JsonDoc {
    /// Create a new document with initial value
    pub fn new(id: JsonDocId, value: JsonValue) -> Self {
        let now = SystemTime::now();
        JsonDoc {
            id,
            value,
            version: 1,
            created_at: now,
            updated_at: now,
        }
    }

    /// Increment version and update timestamp
    pub fn touch(&mut self) {
        self.version += 1;
        self.updated_at = SystemTime::now();
    }
}
```

**Acceptance Criteria**:
- [ ] JsonDoc stores all required fields
- [ ] new() initializes version to 1
- [ ] touch() increments version
- [ ] touch() updates updated_at

---

### Story #235: JsonStore Struct Definition

**File**: `crates/primitives/src/json_store.rs` (NEW)

**Deliverable**: JsonStore as a STATELESS FACADE (like all other primitives)

**CRITICAL**: JsonStore must follow the same pattern as KVStore, EventLog, StateCell, Trace, RunIndex.

**Implementation**:
```rust
use in_mem_core::error::Result;
use in_mem_core::types::{Key, Namespace, RunId};
use in_mem_core::value::Value;
use in_mem_engine::Database;
use std::sync::Arc;

/// JSON document storage primitive
///
/// STATELESS FACADE over Database - all state lives in unified ShardedStore.
/// Multiple JsonStore instances on same Database are safe.
///
/// # Design
///
/// JsonStore does NOT own storage. It is a facade that:
/// - Uses Arc<Database> for all operations
/// - Stores documents via Key::new_json() in ShardedStore
/// - Uses SnapshotView for fast path reads
/// - Participates in cross-primitive transactions
///
/// # Example
///
/// ```ignore
/// let db = Arc::new(Database::open("/path/to/data")?);
/// let json = JsonStore::new(db);
/// let run_id = RunId::new();
/// let doc_id = JsonDocId::new();
///
/// // Simple operations
/// json.create(&run_id, &doc_id, JsonValue::Object(IndexMap::new()))?;
/// let value = json.get(&run_id, &doc_id, &JsonPath::root())?;
/// json.set(&run_id, &doc_id, &JsonPath::parse("foo")?, JsonValue::from(42))?;
/// ```
#[derive(Clone)]
pub struct JsonStore {
    db: Arc<Database>,  // ONLY state: reference to database
}

impl JsonStore {
    /// Create new JsonStore instance
    pub fn new(db: Arc<Database>) -> Self {
        Self { db }
    }

    /// Get the underlying database reference
    pub fn database(&self) -> &Arc<Database> {
        &self.db
    }

    /// Build namespace for run-scoped operations
    fn namespace_for_run(&self, run_id: &RunId) -> Namespace {
        Namespace::for_run(*run_id)
    }

    /// Build key for JSON document
    fn key_for(&self, run_id: &RunId, doc_id: &JsonDocId) -> Key {
        Key::new_json(self.namespace_for_run(run_id), doc_id)
    }

    // ========== Fast Path (Implicit Transactions) ==========

    /// Get value at path (FAST PATH)
    ///
    /// Bypasses full transaction overhead:
    /// - Direct snapshot read
    /// - No transaction object allocation
    /// - No read-set recording
    pub fn get(&self, run_id: &RunId, doc_id: &JsonDocId, path: &JsonPath) -> Result<Option<JsonValue>> {
        use in_mem_core::traits::SnapshotView;

        let snapshot = self.db.storage().create_snapshot();
        let key = self.key_for(run_id, doc_id);

        match snapshot.get(&key)? {
            Some(vv) => {
                let doc: JsonDoc = deserialize_doc(&vv.value)?;
                Ok(get_at_path(&doc.value, path).cloned())
            }
            None => Ok(None),
        }
    }

    /// Check if document exists (FAST PATH)
    pub fn exists(&self, run_id: &RunId, doc_id: &JsonDocId) -> Result<bool> {
        use in_mem_core::traits::SnapshotView;

        let snapshot = self.db.storage().create_snapshot();
        let key = self.key_for(run_id, doc_id);
        Ok(snapshot.get(&key)?.is_some())
    }

    // ========== Single Operations (Implicit Transactions) ==========

    /// Create a new document
    pub fn create(&self, run_id: &RunId, doc_id: &JsonDocId, value: JsonValue) -> Result<()> {
        validate_json_value(&value)?;

        self.db.transaction(*run_id, |txn| {
            let key = self.key_for(run_id, doc_id);

            // Check doesn't already exist
            if txn.get(&key)?.is_some() {
                return Err(Error::AlreadyExists(format!("Document {}", doc_id)));
            }

            let doc = JsonDoc::new(*doc_id, value);
            let serialized = serialize_doc(&doc)?;
            txn.put(key, Value::String(serialized))
        })
    }

    /// Set value at path
    pub fn set(&self, run_id: &RunId, doc_id: &JsonDocId, path: &JsonPath, value: JsonValue) -> Result<()> {
        self.db.transaction(*run_id, |txn| {
            let key = self.key_for(run_id, doc_id);

            // Get existing doc
            let mut doc = match txn.get(&key)? {
                Some(v) => deserialize_doc(&v)?,
                None => return Err(Error::NotFound(format!("Document {}", doc_id))),
            };

            // Apply patch
            set_at_path(&mut doc.value, path, value)?;
            doc.touch();

            // Store updated
            let serialized = serialize_doc(&doc)?;
            txn.put(key, Value::String(serialized))
        })
    }

    // ========== Explicit Transaction ==========

    /// Execute multiple JSON operations atomically
    pub fn transaction<F, T>(&self, run_id: &RunId, f: F) -> Result<T>
    where
        F: FnOnce(&mut JsonTransaction<'_>) -> Result<T>,
    {
        self.db.transaction(*run_id, |txn| {
            let mut json_txn = JsonTransaction {
                txn,
                run_id: *run_id,
            };
            f(&mut json_txn)
        })
    }
}

/// Transaction handle for multi-operation JSON work
pub struct JsonTransaction<'a> {
    txn: &'a mut TransactionContext,
    run_id: RunId,
}
```

**Acceptance Criteria**:
- [ ] JsonStore holds ONLY `Arc<Database>` (stateless facade)
- [ ] Uses Key::new_json() for storage keys
- [ ] Fast path reads use SnapshotView
- [ ] Single operations use db.transaction()
- [ ] Explicit transaction API matches other primitives
- [ ] No DashMap, no internal storage

---

### Story #236: Document Create/Delete

**File**: `crates/primitives/src/json_store.rs`

**Deliverable**: Document creation and deletion operations (using unified storage)

**Implementation**:
```rust
impl JsonStore {
    /// Create a new document
    ///
    /// Uses implicit transaction for atomic create.
    pub fn create(&self, run_id: &RunId, doc_id: &JsonDocId, value: JsonValue) -> Result<()> {
        validate_json_value(&value)?;

        self.db.transaction(*run_id, |txn| {
            let key = self.key_for(run_id, doc_id);

            // Check doesn't already exist
            if txn.get(&key)?.is_some() {
                return Err(Error::AlreadyExists(format!("Document {}", doc_id)));
            }

            let doc = JsonDoc::new(*doc_id, value);
            let serialized = serialize_doc(&doc)?;
            txn.put(key, Value::String(serialized))
        })
    }

    /// Delete a document entirely
    ///
    /// Uses implicit transaction for atomic delete.
    pub fn delete_doc(&self, run_id: &RunId, doc_id: &JsonDocId) -> Result<bool> {
        self.db.transaction(*run_id, |txn| {
            let key = self.key_for(run_id, doc_id);

            if txn.get(&key)?.is_some() {
                txn.delete(key)?;
                Ok(true)
            } else {
                Ok(false)
            }
        })
    }

    /// List all document IDs in a run
    pub fn list(&self, run_id: &RunId) -> Result<Vec<JsonDocId>> {
        use in_mem_core::traits::SnapshotView;

        let snapshot = self.db.storage().create_snapshot();
        let prefix = Key::new_json_prefix(self.namespace_for_run(run_id));

        let entries = snapshot.scan_prefix(&prefix)?;
        let doc_ids = entries
            .into_iter()
            .filter_map(|(key, _)| {
                // Extract doc_id from key user_key bytes
                JsonDocId::try_from_bytes(key.user_key())
            })
            .collect();

        Ok(doc_ids)
    }
}

// Helper functions
fn serialize_doc(doc: &JsonDoc) -> Result<String> {
    serde_json::to_string(doc)
        .map_err(|e| Error::Serialization(e.to_string()))
}

fn deserialize_doc(value: &Value) -> Result<JsonDoc> {
    match value {
        Value::String(s) => serde_json::from_str(s)
            .map_err(|e| Error::Deserialization(e.to_string())),
        _ => Err(Error::InvalidType("expected serialized JsonDoc".into())),
    }
}
```

**Acceptance Criteria**:
- [ ] create() validates document before storing
- [ ] create() uses unified Key::new_json()
- [ ] create() fails if document exists (via transaction)
- [ ] delete_doc() uses unified storage delete
- [ ] delete_doc() returns bool (true if deleted)
- [ ] list() uses SnapshotView and prefix scan

---

### Story #237: Document Get/Set/Delete at Path

**File**: `crates/primitives/src/json_store.rs`

**Deliverable**: Path-based document operations (using unified storage)

**Implementation**:
```rust
impl JsonStore {
    /// Get value at path in document (FAST PATH)
    ///
    /// Uses SnapshotView directly for read-only access.
    pub fn get(&self, run_id: &RunId, doc_id: &JsonDocId, path: &JsonPath) -> Result<Option<JsonValue>> {
        use in_mem_core::traits::SnapshotView;

        let snapshot = self.db.storage().create_snapshot();
        let key = self.key_for(run_id, doc_id);

        match snapshot.get(&key)? {
            Some(vv) => {
                let doc: JsonDoc = deserialize_doc(&vv.value)?;
                Ok(get_at_path(&doc.value, path).cloned())
            }
            None => Ok(None),
        }
    }

    /// Get document version (FAST PATH)
    pub fn get_version(&self, run_id: &RunId, doc_id: &JsonDocId) -> Result<Option<u64>> {
        use in_mem_core::traits::SnapshotView;

        let snapshot = self.db.storage().create_snapshot();
        let key = self.key_for(run_id, doc_id);

        match snapshot.get(&key)? {
            Some(vv) => {
                let doc: JsonDoc = deserialize_doc(&vv.value)?;
                Ok(Some(doc.version))
            }
            None => Ok(None),
        }
    }

    /// Set value at path in document
    ///
    /// Uses implicit transaction for atomic update.
    pub fn set(&self, run_id: &RunId, doc_id: &JsonDocId, path: &JsonPath, value: JsonValue) -> Result<u64> {
        validate_json_value(&value)?;
        validate_path(path)?;

        self.db.transaction(*run_id, |txn| {
            let key = self.key_for(run_id, doc_id);

            // Get existing doc
            let mut doc = match txn.get(&key)? {
                Some(v) => deserialize_doc(&v)?,
                None => return Err(Error::NotFound(format!("Document {}", doc_id))),
            };

            // Apply patch
            set_at_path(&mut doc.value, path, value)?;
            doc.touch();

            let new_version = doc.version;
            let serialized = serialize_doc(&doc)?;
            txn.put(key, Value::String(serialized))?;
            Ok(new_version)
        })
    }

    /// Delete value at path in document
    ///
    /// Uses implicit transaction for atomic update.
    pub fn delete_at_path(&self, run_id: &RunId, doc_id: &JsonDocId, path: &JsonPath) -> Result<Option<JsonValue>> {
        self.db.transaction(*run_id, |txn| {
            let key = self.key_for(run_id, doc_id);

            // Get existing doc
            let mut doc = match txn.get(&key)? {
                Some(v) => deserialize_doc(&v)?,
                None => return Err(Error::NotFound(format!("Document {}", doc_id))),
            };

            // Apply delete
            let deleted = delete_at_path(&mut doc.value, path)?;
            doc.touch();

            let serialized = serialize_doc(&doc)?;
            txn.put(key, Value::String(serialized))?;
            Ok(deleted)
    }
}
```

**Acceptance Criteria**:
- [ ] get() returns cloned value at path
- [ ] get() returns None for non-existent paths
- [ ] set() validates value and path
- [ ] set() increments version
- [ ] delete_at_path() returns deleted value

---

### Story #238: Document Exists/List

**File**: `crates/core/src/json_store.rs`

**Deliverable**: Document existence and listing operations

**Implementation**:
```rust
impl JsonStore {
    /// List all document IDs for a run
    pub fn list_docs(&self, run_id: RunId) -> Vec<JsonDocId> {
        self.docs.iter()
            .filter(|entry| entry.key().0 == run_id)
            .map(|entry| entry.key().1)
            .collect()
    }

    /// Get full document (for serialization/snapshot)
    pub fn get_doc(
        &self,
        run_id: RunId,
        doc_id: JsonDocId,
    ) -> Result<Arc<JsonDoc>, JsonStoreError> {
        let key = (run_id, doc_id);
        self.docs.get(&key)
            .map(|r| r.value().clone())
            .ok_or(JsonStoreError::DocumentNotFound { doc_id })
    }
}
```

**Acceptance Criteria**:
- [ ] list_docs() returns all doc IDs for run
- [ ] list_docs() filters by run_id
- [ ] get_doc() returns Arc to document

---

### Story #239: MessagePack Serialization

**File**: `crates/core/src/json_store.rs`

**Deliverable**: MessagePack serialization for documents

**Implementation**:
```rust
impl JsonStore {
    /// Serialize document to bytes
    pub fn serialize_doc(doc: &JsonDoc) -> Result<Vec<u8>, JsonStoreError> {
        rmp_serde::to_vec(doc)
            .map_err(|e| JsonStoreError::Serialization(e.to_string()))
    }

    /// Deserialize document from bytes
    pub fn deserialize_doc(bytes: &[u8]) -> Result<JsonDoc, JsonStoreError> {
        rmp_serde::from_slice(bytes)
            .map_err(|e| JsonStoreError::Deserialization(e.to_string()))
    }

    /// Get serialized size of document
    pub fn doc_size(doc: &JsonDoc) -> Result<usize, JsonStoreError> {
        let bytes = Self::serialize_doc(doc)?;
        Ok(bytes.len())
    }

    /// Validate document size limit
    pub fn validate_doc_size(doc: &JsonDoc) -> Result<(), JsonStoreError> {
        let size = Self::doc_size(doc)?;
        if size > MAX_DOCUMENT_SIZE {
            return Err(JsonStoreError::Validation(
                JsonValidationError::DocumentTooLarge {
                    size: size as u64,
                    max: MAX_DOCUMENT_SIZE as u64,
                }
            ));
        }
        Ok(())
    }
}

// Add to JsonStoreError:
// #[error("serialization error: {0}")]
// Serialization(String),
// #[error("deserialization error: {0}")]
// Deserialization(String),
```

**Acceptance Criteria**:
- [ ] serialize_doc() produces MessagePack bytes
- [ ] deserialize_doc() reconstructs document
- [ ] Round-trip preserves all fields
- [ ] validate_doc_size() enforces 16MB limit

---

## Epic 29: WAL Integration

**Goal**: Integrate JSON operations with write-ahead logging

### Scope
- JSON WAL entry types
- WAL write for JSON operations
- WAL replay for JSON
- Idempotent replay logic

### Success Criteria
- [ ] WAL entry types 0x20-0x23 defined
- [ ] All JSON mutations write WAL entries
- [ ] WAL replay reconstructs state
- [ ] Replay is idempotent

### Dependencies
- Epic 28 complete

### Estimated Effort
1 day with 2 Claudes in parallel

### User Stories
- **#240**: JSON WAL Entry Types (3 hours) 🔴 FOUNDATION
- **#241**: WAL Write for JSON Operations (4 hours)
- **#242**: WAL Replay for JSON (4 hours)
- **#243**: Idempotent Replay Logic (3 hours)

### Parallelization
After #240, stories #241-242 can run in parallel. #243 runs last.

---

### Story #240: JSON WAL Entry Types

**File**: `crates/core/src/wal.rs`

**Deliverable**: WAL entry types for JSON operations

**Implementation**:
```rust
/// WAL entry type constants for JSON operations
pub const WAL_JSON_CREATE: u8 = 0x20;
pub const WAL_JSON_SET: u8 = 0x21;
pub const WAL_JSON_DELETE: u8 = 0x22;
pub const WAL_JSON_DELETE_DOC: u8 = 0x23;

/// JSON WAL entry payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JsonWalEntry {
    /// Create new document
    Create {
        run_id: RunId,
        doc_id: JsonDocId,
        value: JsonValue,
        version: u64,
    },
    /// Set value at path
    Set {
        run_id: RunId,
        doc_id: JsonDocId,
        path: JsonPath,
        value: JsonValue,
        version: u64,
    },
    /// Delete value at path
    Delete {
        run_id: RunId,
        doc_id: JsonDocId,
        path: JsonPath,
        version: u64,
    },
    /// Delete entire document
    DeleteDoc {
        run_id: RunId,
        doc_id: JsonDocId,
    },
}

impl JsonWalEntry {
    pub fn entry_type(&self) -> u8 {
        match self {
            JsonWalEntry::Create { .. } => WAL_JSON_CREATE,
            JsonWalEntry::Set { .. } => WAL_JSON_SET,
            JsonWalEntry::Delete { .. } => WAL_JSON_DELETE,
            JsonWalEntry::DeleteDoc { .. } => WAL_JSON_DELETE_DOC,
        }
    }

    pub fn run_id(&self) -> RunId {
        match self {
            JsonWalEntry::Create { run_id, .. } => *run_id,
            JsonWalEntry::Set { run_id, .. } => *run_id,
            JsonWalEntry::Delete { run_id, .. } => *run_id,
            JsonWalEntry::DeleteDoc { run_id, .. } => *run_id,
        }
    }

    pub fn doc_id(&self) -> JsonDocId {
        match self {
            JsonWalEntry::Create { doc_id, .. } => *doc_id,
            JsonWalEntry::Set { doc_id, .. } => *doc_id,
            JsonWalEntry::Delete { doc_id, .. } => *doc_id,
            JsonWalEntry::DeleteDoc { doc_id, .. } => *doc_id,
        }
    }
}
```

**Acceptance Criteria**:
- [ ] All four entry types defined
- [ ] Entry type constants match spec (0x20-0x23)
- [ ] Each entry contains required fields
- [ ] Entries are serializable

---

### Story #241: WAL Write for JSON Operations

**File**: `crates/core/src/json_store.rs`

**Deliverable**: WAL writes for all JSON mutations

**Implementation**:
```rust
impl JsonStore {
    /// Create document with WAL
    pub fn create_with_wal(
        &self,
        wal: &impl WalWriter,
        run_id: RunId,
        doc_id: JsonDocId,
        value: JsonValue,
    ) -> Result<(), JsonStoreError> {
        validate_json_value(&value)?;

        let entry = JsonWalEntry::Create {
            run_id,
            doc_id,
            value: value.clone(),
            version: 1,
        };

        // Write WAL first (durability)
        wal.append(&entry)?;

        // Then update in-memory state
        self.create(run_id, doc_id, value)
    }

    /// Set at path with WAL
    pub fn set_with_wal(
        &self,
        wal: &impl WalWriter,
        run_id: RunId,
        doc_id: JsonDocId,
        path: &JsonPath,
        value: JsonValue,
    ) -> Result<u64, JsonStoreError> {
        validate_json_value(&value)?;
        validate_path(path)?;

        // Get current version first
        let current_version = self.get_version(run_id, doc_id)?;
        let new_version = current_version + 1;

        let entry = JsonWalEntry::Set {
            run_id,
            doc_id,
            path: path.clone(),
            value: value.clone(),
            version: new_version,
        };

        // Write WAL first
        wal.append(&entry)?;

        // Then update in-memory state
        self.set(run_id, doc_id, path, value)
    }

    /// Delete at path with WAL
    pub fn delete_at_path_with_wal(
        &self,
        wal: &impl WalWriter,
        run_id: RunId,
        doc_id: JsonDocId,
        path: &JsonPath,
    ) -> Result<Option<JsonValue>, JsonStoreError> {
        let current_version = self.get_version(run_id, doc_id)?;
        let new_version = current_version + 1;

        let entry = JsonWalEntry::Delete {
            run_id,
            doc_id,
            path: path.clone(),
            version: new_version,
        };

        wal.append(&entry)?;
        self.delete_at_path(run_id, doc_id, path)
    }

    /// Delete document with WAL
    pub fn delete_doc_with_wal(
        &self,
        wal: &impl WalWriter,
        run_id: RunId,
        doc_id: JsonDocId,
    ) -> Result<Option<Arc<JsonDoc>>, JsonStoreError> {
        let entry = JsonWalEntry::DeleteDoc { run_id, doc_id };
        wal.append(&entry)?;
        self.delete_doc(run_id, doc_id)
    }
}
```

**Acceptance Criteria**:
- [ ] All mutations write WAL before state update
- [ ] WAL entries contain correct versions
- [ ] Failures rollback correctly

---

### Story #242: WAL Replay for JSON

**File**: `crates/core/src/json_store.rs`

**Deliverable**: WAL replay to reconstruct JSON state

**Implementation**:
```rust
impl JsonStore {
    /// Replay a single WAL entry
    pub fn replay_entry(&self, entry: &JsonWalEntry) -> Result<(), JsonStoreError> {
        match entry {
            JsonWalEntry::Create { run_id, doc_id, value, version } => {
                let mut doc = JsonDoc::new(*doc_id, value.clone());
                doc.version = *version;
                let key = (*run_id, *doc_id);
                self.docs.insert(key, Arc::new(doc));
                Ok(())
            }
            JsonWalEntry::Set { run_id, doc_id, path, value, version } => {
                self.replay_set(*run_id, *doc_id, path, value.clone(), *version)
            }
            JsonWalEntry::Delete { run_id, doc_id, path, version } => {
                self.replay_delete(*run_id, *doc_id, path, *version)
            }
            JsonWalEntry::DeleteDoc { run_id, doc_id } => {
                self.delete_doc(*run_id, *doc_id)?;
                Ok(())
            }
        }
    }

    fn replay_set(
        &self,
        run_id: RunId,
        doc_id: JsonDocId,
        path: &JsonPath,
        value: JsonValue,
        version: u64,
    ) -> Result<(), JsonStoreError> {
        let key = (run_id, doc_id);
        let mut entry = self.docs.get_mut(&key)
            .ok_or(JsonStoreError::DocumentNotFound { doc_id })?;

        // Skip if already applied (idempotent)
        if entry.version >= version {
            return Ok(());
        }

        let mut new_doc = (*entry.value()).clone();
        set_at_path(&mut new_doc.value, path, value)?;
        new_doc.version = version;
        new_doc.updated_at = SystemTime::now();

        *entry = Arc::new(new_doc);
        Ok(())
    }

    fn replay_delete(
        &self,
        run_id: RunId,
        doc_id: JsonDocId,
        path: &JsonPath,
        version: u64,
    ) -> Result<(), JsonStoreError> {
        let key = (run_id, doc_id);
        let mut entry = self.docs.get_mut(&key)
            .ok_or(JsonStoreError::DocumentNotFound { doc_id })?;

        // Skip if already applied
        if entry.version >= version {
            return Ok(());
        }

        let mut new_doc = (*entry.value()).clone();
        delete_at_path(&mut new_doc.value, path)?;
        new_doc.version = version;
        new_doc.updated_at = SystemTime::now();

        *entry = Arc::new(new_doc);
        Ok(())
    }
}
```

**Acceptance Criteria**:
- [ ] replay_entry() handles all entry types
- [ ] State matches after replay
- [ ] Order is preserved

---

### Story #243: Idempotent Replay Logic

**File**: `crates/core/src/json_store.rs`

**Deliverable**: Version-based idempotent replay

**Implementation**:
```rust
impl JsonStore {
    /// Check if entry has already been applied
    pub fn is_entry_applied(&self, entry: &JsonWalEntry) -> bool {
        let key = (entry.run_id(), entry.doc_id());

        match entry {
            JsonWalEntry::Create { .. } => {
                self.docs.contains_key(&key)
            }
            JsonWalEntry::Set { version, .. } |
            JsonWalEntry::Delete { version, .. } => {
                self.docs.get(&key)
                    .map(|doc| doc.version >= *version)
                    .unwrap_or(false)
            }
            JsonWalEntry::DeleteDoc { .. } => {
                !self.docs.contains_key(&key)
            }
        }
    }

    /// Replay entry only if not already applied
    pub fn replay_entry_idempotent(&self, entry: &JsonWalEntry) -> Result<bool, JsonStoreError> {
        if self.is_entry_applied(entry) {
            return Ok(false); // Already applied
        }
        self.replay_entry(entry)?;
        Ok(true) // Applied
    }
}
```

**Acceptance Criteria**:
- [ ] is_entry_applied() uses version comparison
- [ ] Duplicate replays are no-ops
- [ ] Returns whether entry was applied

---

## Epic 30: Transaction Integration

**Goal**: Integrate JSON operations with transaction system

### Scope
- JSON read/write set types
- Lazy set initialization
- TransactionContext extension
- Snapshot version capture
- Cross-primitive transactions

### Success Criteria
- [ ] JsonReadEntry and JsonWriteEntry types defined
- [ ] Read/write sets are Option<Vec<...>> for lazy allocation
- [ ] TransactionContext tracks JSON operations
- [ ] Snapshot captures document versions
- [ ] Cross-primitive atomicity works

### Dependencies
- Epic 28 complete

### Estimated Effort
1.5 days with 2 Claudes in parallel

### User Stories
- **#244**: JSON Read/Write Set Types (2 hours) 🔴 FOUNDATION
- **#245**: Lazy Set Initialization (2 hours)
- **#246**: TransactionContext Extension (4 hours)
- **#247**: Snapshot Version Capture (3 hours)
- **#248**: Cross-Primitive Transactions (4 hours)

### Parallelization
After #244-245, stories #246-248 can run in parallel

---

### Story #244: JSON Read/Write Set Types

**File**: `crates/core/src/transaction.rs`

**Deliverable**: Types for tracking JSON reads and writes

**Implementation**:
```rust
/// Entry in the JSON read set
#[derive(Debug, Clone)]
pub struct JsonReadEntry {
    pub run_id: RunId,
    pub doc_id: JsonDocId,
    pub path: JsonPath,
    pub version_at_read: u64,
}

/// Entry in the JSON write set
#[derive(Debug, Clone)]
pub struct JsonWriteEntry {
    pub run_id: RunId,
    pub doc_id: JsonDocId,
    pub patch: JsonPatch,
    pub resulting_version: u64,
}

impl JsonReadEntry {
    pub fn new(run_id: RunId, doc_id: JsonDocId, path: JsonPath, version: u64) -> Self {
        JsonReadEntry {
            run_id,
            doc_id,
            path,
            version_at_read: version,
        }
    }
}

impl JsonWriteEntry {
    pub fn new(run_id: RunId, doc_id: JsonDocId, patch: JsonPatch, version: u64) -> Self {
        JsonWriteEntry {
            run_id,
            doc_id,
            patch,
            resulting_version: version,
        }
    }
}
```

**Acceptance Criteria**:
- [ ] JsonReadEntry captures read context
- [ ] JsonWriteEntry captures write context
- [ ] Both include version information

---

### Story #245: Lazy Set Initialization

**File**: `crates/core/src/transaction.rs`

**Deliverable**: Lazy allocation of JSON tracking sets

**Implementation**:
```rust
/// Extension to TransactionContext for JSON tracking
pub struct JsonTransactionState {
    /// Lazily allocated read set
    read_set: Option<Vec<JsonReadEntry>>,
    /// Lazily allocated write set
    write_set: Option<Vec<JsonWriteEntry>>,
}

impl JsonTransactionState {
    pub fn new() -> Self {
        JsonTransactionState {
            read_set: None,
            write_set: None,
        }
    }

    /// Get or create read set
    pub fn read_set_mut(&mut self) -> &mut Vec<JsonReadEntry> {
        self.read_set.get_or_insert_with(Vec::new)
    }

    /// Get or create write set
    pub fn write_set_mut(&mut self) -> &mut Vec<JsonWriteEntry> {
        self.write_set.get_or_insert_with(Vec::new)
    }

    /// Check if any JSON operations occurred
    pub fn has_json_ops(&self) -> bool {
        self.read_set.is_some() || self.write_set.is_some()
    }

    /// Get read set if allocated
    pub fn read_set(&self) -> Option<&Vec<JsonReadEntry>> {
        self.read_set.as_ref()
    }

    /// Get write set if allocated
    pub fn write_set(&self) -> Option<&Vec<JsonWriteEntry>> {
        self.write_set.as_ref()
    }

    /// Reset for pooling
    pub fn reset(&mut self) {
        if let Some(ref mut rs) = self.read_set {
            rs.clear();
        }
        if let Some(ref mut ws) = self.write_set {
            ws.clear();
        }
    }
}
```

**Acceptance Criteria**:
- [ ] Sets are None initially
- [ ] First access allocates
- [ ] has_json_ops() returns correct status
- [ ] reset() clears without deallocating

---

### Story #246: TransactionContext Extension

**File**: `crates/core/src/transaction.rs`

**Deliverable**: Extend TransactionContext with JSON state

**Implementation**:
```rust
// Add to TransactionContext:
pub struct TransactionContext {
    // ... existing fields ...

    /// JSON transaction state (lazy)
    json_state: JsonTransactionState,
}

impl TransactionContext {
    /// Record a JSON read
    pub fn record_json_read(
        &mut self,
        run_id: RunId,
        doc_id: JsonDocId,
        path: JsonPath,
        version: u64,
    ) {
        self.json_state.read_set_mut().push(
            JsonReadEntry::new(run_id, doc_id, path, version)
        );
    }

    /// Record a JSON write
    pub fn record_json_write(
        &mut self,
        run_id: RunId,
        doc_id: JsonDocId,
        patch: JsonPatch,
        version: u64,
    ) {
        self.json_state.write_set_mut().push(
            JsonWriteEntry::new(run_id, doc_id, patch, version)
        );
    }

    /// Get JSON reads
    pub fn json_reads(&self) -> Option<&Vec<JsonReadEntry>> {
        self.json_state.read_set()
    }

    /// Get JSON writes
    pub fn json_writes(&self) -> Option<&Vec<JsonWriteEntry>> {
        self.json_state.write_set()
    }

    /// Check if transaction has JSON operations
    pub fn has_json_ops(&self) -> bool {
        self.json_state.has_json_ops()
    }
}
```

**Acceptance Criteria**:
- [ ] record_json_read() tracks reads
- [ ] record_json_write() tracks writes
- [ ] Accessors return correct data
- [ ] No overhead for non-JSON transactions

---

### Story #247: Snapshot Version Capture

**File**: `crates/core/src/transaction.rs`

**Deliverable**: Capture document versions at snapshot time

**Implementation**:
```rust
/// Snapshot of document versions at transaction start
#[derive(Debug, Clone, Default)]
pub struct JsonSnapshot {
    /// Document versions at snapshot time
    versions: HashMap<(RunId, JsonDocId), u64>,
}

impl JsonSnapshot {
    pub fn new() -> Self {
        JsonSnapshot { versions: HashMap::new() }
    }

    /// Record document version at snapshot
    pub fn capture(&mut self, run_id: RunId, doc_id: JsonDocId, version: u64) {
        self.versions.insert((run_id, doc_id), version);
    }

    /// Get captured version
    pub fn get_version(&self, run_id: RunId, doc_id: JsonDocId) -> Option<u64> {
        self.versions.get(&(run_id, doc_id)).copied()
    }

    /// Check if document version matches snapshot
    pub fn check_version(&self, run_id: RunId, doc_id: JsonDocId, current: u64) -> bool {
        match self.get_version(run_id, doc_id) {
            Some(snapshot_version) => snapshot_version == current,
            None => true, // Not in snapshot, no conflict
        }
    }
}

// Add to TransactionContext:
impl TransactionContext {
    /// Capture document version for snapshot
    pub fn capture_json_version(
        &mut self,
        run_id: RunId,
        doc_id: JsonDocId,
        version: u64,
    ) {
        // Initialize snapshot if needed
        // self.json_snapshot.capture(run_id, doc_id, version);
    }
}
```

**Acceptance Criteria**:
- [ ] Snapshot captures versions lazily
- [ ] check_version() detects changes
- [ ] Non-captured documents don't conflict

---

### Story #248: Cross-Primitive Transactions

**File**: `crates/core/src/database.rs`

**Deliverable**: Atomic transactions spanning JSON and other primitives

**Implementation**:
```rust
impl Database {
    /// Execute a transaction that may include JSON and other primitives
    pub fn transaction<F, T>(&self, run_id: RunId, f: F) -> Result<T, TransactionError>
    where
        F: FnOnce(&mut TransactionContext) -> Result<T, TransactionError>,
    {
        let mut ctx = self.begin_transaction(run_id)?;

        let result = f(&mut ctx)?;

        // Validate JSON conflicts if JSON ops occurred
        if ctx.has_json_ops() {
            self.validate_json_conflicts(&ctx)?;
        }

        // Commit all primitives atomically
        self.commit_transaction(ctx)?;

        Ok(result)
    }

    fn validate_json_conflicts(&self, ctx: &TransactionContext) -> Result<(), TransactionError> {
        // Check read-write conflicts
        if let Some(reads) = ctx.json_reads() {
            for read in reads {
                let current_version = self.json_store.get_version(read.run_id, read.doc_id)?;
                if current_version != read.version_at_read {
                    return Err(TransactionError::JsonConflict {
                        doc_id: read.doc_id,
                        path: read.path.clone(),
                        reason: "document modified since read",
                    });
                }
            }
        }

        // Check write-write conflicts
        if let Some(writes) = ctx.json_writes() {
            for (i, w1) in writes.iter().enumerate() {
                for w2 in writes.iter().skip(i + 1) {
                    if w1.doc_id == w2.doc_id && w1.patch.conflicts_with(&w2.patch) {
                        return Err(TransactionError::JsonConflict {
                            doc_id: w1.doc_id,
                            path: w1.patch.path().clone(),
                            reason: "conflicting writes to overlapping paths",
                        });
                    }
                }
            }
        }

        Ok(())
    }
}
```

**Acceptance Criteria**:
- [ ] JSON + KV in same transaction works
- [ ] Conflicts detected before commit
- [ ] Rollback on conflict includes all primitives

---

## Epic 31: Conflict Detection

**Goal**: Implement region-based conflict detection for JSON

### Scope
- Path overlap detection
- Read-write conflict check
- Write-write conflict check
- Integration with commit

### Success Criteria
- [ ] overlaps() correctly identifies all cases
- [ ] Read-write conflicts detected
- [ ] Write-write conflicts detected
- [ ] Integration with commit flow

### Dependencies
- Epic 30 complete

### Estimated Effort
1 day with 2 Claudes in parallel

### User Stories
- **#249**: Path Overlap Detection (2 hours)
- **#250**: Read-Write Conflict Check (3 hours)
- **#251**: Write-Write Conflict Check (3 hours)
- **#252**: Conflict Integration with Commit (4 hours)

### Parallelization
Stories #249-251 can run in parallel. #252 runs last.

---

### Story #249: Path Overlap Detection

**File**: `crates/core/src/json.rs`

**Deliverable**: Comprehensive path overlap detection

**Implementation**:
```rust
impl JsonPath {
    /// Check if two paths overlap
    ///
    /// Paths overlap if:
    /// - They are equal
    /// - One is an ancestor of the other
    /// - One is a descendant of the other
    ///
    /// Examples:
    /// - $.foo overlaps with $.foo (equal)
    /// - $.foo overlaps with $.foo.bar (ancestor)
    /// - $.foo.bar overlaps with $.foo (descendant)
    /// - $.foo does NOT overlap with $.bar (disjoint)
    /// - $.items[0] does NOT overlap with $.items[1] (disjoint)
    pub fn overlaps(&self, other: &JsonPath) -> bool {
        let min_len = self.segments.len().min(other.segments.len());

        // Check if common prefix matches
        for i in 0..min_len {
            if self.segments[i] != other.segments[i] {
                return false; // Disjoint paths
            }
        }

        // If we get here, one path is a prefix of the other (or equal)
        true
    }

    /// Get the common ancestor path
    pub fn common_ancestor(&self, other: &JsonPath) -> JsonPath {
        let mut common = Vec::new();

        for (a, b) in self.segments.iter().zip(other.segments.iter()) {
            if a == b {
                common.push(a.clone());
            } else {
                break;
            }
        }

        JsonPath { segments: common }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_overlap() {
        let root = JsonPath::root();
        let foo = JsonPath::parse("foo").unwrap();
        let foo_bar = JsonPath::parse("foo.bar").unwrap();
        let baz = JsonPath::parse("baz").unwrap();
        let items_0 = JsonPath::parse("items[0]").unwrap();
        let items_1 = JsonPath::parse("items[1]").unwrap();

        // Root overlaps with everything
        assert!(root.overlaps(&foo));
        assert!(root.overlaps(&foo_bar));

        // Equal paths overlap
        assert!(foo.overlaps(&foo));

        // Ancestor/descendant overlap
        assert!(foo.overlaps(&foo_bar));
        assert!(foo_bar.overlaps(&foo));

        // Disjoint paths don't overlap
        assert!(!foo.overlaps(&baz));
        assert!(!items_0.overlaps(&items_1));
    }
}
```

**Acceptance Criteria**:
- [ ] Equal paths overlap
- [ ] Ancestor/descendant overlap
- [ ] Disjoint paths don't overlap
- [ ] Different array indices don't overlap
- [ ] Root overlaps with everything

---

### Story #250: Read-Write Conflict Check

**File**: `crates/core/src/transaction.rs`

**Deliverable**: Detect read-write conflicts

**Implementation**:
```rust
/// Check for read-write conflicts in a transaction
pub fn check_read_write_conflicts(
    reads: &[JsonReadEntry],
    writes: &[JsonWriteEntry],
) -> Result<(), JsonConflictError> {
    for read in reads {
        for write in writes {
            // Same document?
            if read.run_id != write.run_id || read.doc_id != write.doc_id {
                continue;
            }

            // Paths overlap?
            if read.path.overlaps(write.patch.path()) {
                return Err(JsonConflictError::ReadWriteConflict {
                    doc_id: read.doc_id,
                    read_path: read.path.clone(),
                    write_path: write.patch.path().clone(),
                });
            }
        }
    }

    Ok(())
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum JsonConflictError {
    #[error("read-write conflict on {doc_id}: read at {read_path}, write at {write_path}")]
    ReadWriteConflict {
        doc_id: JsonDocId,
        read_path: JsonPath,
        write_path: JsonPath,
    },
    #[error("write-write conflict on {doc_id}: writes at {path1} and {path2}")]
    WriteWriteConflict {
        doc_id: JsonDocId,
        path1: JsonPath,
        path2: JsonPath,
    },
    #[error("version mismatch on {doc_id}: expected {expected}, found {found}")]
    VersionMismatch {
        doc_id: JsonDocId,
        expected: u64,
        found: u64,
    },
}
```

**Acceptance Criteria**:
- [ ] Detects overlapping read and write paths
- [ ] Only same document conflicts
- [ ] Returns detailed error info

---

### Story #251: Write-Write Conflict Check

**File**: `crates/core/src/transaction.rs`

**Deliverable**: Detect write-write conflicts

**Implementation**:
```rust
/// Check for write-write conflicts in a transaction
pub fn check_write_write_conflicts(
    writes: &[JsonWriteEntry],
) -> Result<(), JsonConflictError> {
    for (i, w1) in writes.iter().enumerate() {
        for w2 in writes.iter().skip(i + 1) {
            // Same document?
            if w1.run_id != w2.run_id || w1.doc_id != w2.doc_id {
                continue;
            }

            // Paths overlap?
            if w1.patch.path().overlaps(w2.patch.path()) {
                return Err(JsonConflictError::WriteWriteConflict {
                    doc_id: w1.doc_id,
                    path1: w1.patch.path().clone(),
                    path2: w2.patch.path().clone(),
                });
            }
        }
    }

    Ok(())
}

/// Check for version conflicts (stale reads)
pub fn check_version_conflicts(
    json_store: &JsonStore,
    reads: &[JsonReadEntry],
) -> Result<(), JsonConflictError> {
    for read in reads {
        let current = json_store.get_version(read.run_id, read.doc_id)
            .map_err(|_| JsonConflictError::VersionMismatch {
                doc_id: read.doc_id,
                expected: read.version_at_read,
                found: 0,
            })?;

        if current != read.version_at_read {
            return Err(JsonConflictError::VersionMismatch {
                doc_id: read.doc_id,
                expected: read.version_at_read,
                found: current,
            });
        }
    }

    Ok(())
}
```

**Acceptance Criteria**:
- [ ] Detects overlapping write paths
- [ ] Detects version mismatches
- [ ] Handles document deletion

---

### Story #252: Conflict Integration with Commit

**File**: `crates/core/src/database.rs`

**Deliverable**: Integrate conflict detection into commit flow

**Implementation**:
```rust
impl Database {
    /// Validate all JSON conflicts before commit
    pub fn validate_json_transaction(
        &self,
        ctx: &TransactionContext,
    ) -> Result<(), TransactionError> {
        if !ctx.has_json_ops() {
            return Ok(());
        }

        // 1. Check version conflicts (stale reads)
        if let Some(reads) = ctx.json_reads() {
            check_version_conflicts(&self.json_store, reads)
                .map_err(|e| TransactionError::JsonConflict(e))?;
        }

        // 2. Check read-write conflicts within transaction
        if let (Some(reads), Some(writes)) = (ctx.json_reads(), ctx.json_writes()) {
            check_read_write_conflicts(reads, writes)
                .map_err(|e| TransactionError::JsonConflict(e))?;
        }

        // 3. Check write-write conflicts within transaction
        if let Some(writes) = ctx.json_writes() {
            check_write_write_conflicts(writes)
                .map_err(|e| TransactionError::JsonConflict(e))?;
        }

        Ok(())
    }

    /// Commit transaction with JSON support
    pub fn commit_with_json(&self, ctx: TransactionContext) -> Result<(), TransactionError> {
        // Validate JSON conflicts
        self.validate_json_transaction(&ctx)?;

        // Commit other primitives
        self.commit_kv(&ctx)?;
        self.commit_events(&ctx)?;
        self.commit_state(&ctx)?;
        self.commit_trace(&ctx)?;

        // Commit JSON (already in store, just flush WAL)
        self.commit_json(&ctx)?;

        Ok(())
    }
}
```

**Acceptance Criteria**:
- [ ] All conflict types checked before commit
- [ ] Conflicts abort entire transaction
- [ ] JSON commits with other primitives

---

## Epic 32: Validation & Non-Regression

**Goal**: Ensure correctness and maintain M4 performance

### Scope
- JSON unit tests
- JSON integration tests
- Non-regression benchmark suite
- Performance baseline documentation

### Success Criteria
- [ ] Unit tests cover all path operations
- [ ] Integration tests verify WAL and transactions
- [ ] M4 performance targets maintained
- [ ] Documentation complete

### Dependencies
- All other epics complete

### Estimated Effort
1 day with 2 Claudes in parallel

### User Stories
- **#253**: JSON Unit Tests (4 hours)
- **#254**: JSON Integration Tests (4 hours)
- **#255**: Non-Regression Benchmark Suite (3 hours)
- **#256**: Performance Baseline Documentation (2 hours)

### Parallelization
Stories #253-254 can run in parallel. #255-256 run after.

---

### Story #253: JSON Unit Tests

**File**: `crates/core/src/json_tests.rs` (NEW)

**Deliverable**: Comprehensive unit tests for JSON types and operations

**Test Coverage**:
```rust
#[cfg(test)]
mod tests {
    // JsonValue tests
    #[test] fn test_json_value_types() { ... }
    #[test] fn test_json_value_accessors() { ... }
    #[test] fn test_json_value_from_impls() { ... }

    // JsonPath tests
    #[test] fn test_path_parse_simple() { ... }
    #[test] fn test_path_parse_nested() { ... }
    #[test] fn test_path_parse_array() { ... }
    #[test] fn test_path_parse_mixed() { ... }
    #[test] fn test_path_parse_errors() { ... }
    #[test] fn test_path_overlap() { ... }
    #[test] fn test_path_ancestor() { ... }
    #[test] fn test_path_display() { ... }

    // Path operations tests
    #[test] fn test_get_at_path_root() { ... }
    #[test] fn test_get_at_path_object() { ... }
    #[test] fn test_get_at_path_array() { ... }
    #[test] fn test_get_at_path_nested() { ... }
    #[test] fn test_get_at_path_missing() { ... }
    #[test] fn test_set_at_path_root() { ... }
    #[test] fn test_set_at_path_create_intermediate() { ... }
    #[test] fn test_set_at_path_overwrite() { ... }
    #[test] fn test_delete_at_path() { ... }

    // Validation tests
    #[test] fn test_validate_nesting_depth() { ... }
    #[test] fn test_validate_array_size() { ... }
    #[test] fn test_validate_path_length() { ... }

    // Serialization tests
    #[test] fn test_messagepack_roundtrip() { ... }
    #[test] fn test_large_document_serialization() { ... }
}
```

**Acceptance Criteria**:
- [ ] All JsonValue types tested
- [ ] All path parsing cases covered
- [ ] All path operations tested
- [ ] Edge cases covered
- [ ] Error conditions tested

---

### Story #254: JSON Integration Tests

**File**: `crates/core/tests/json_integration.rs` (NEW)

**Deliverable**: Integration tests for JSON with WAL and transactions

**Test Coverage**:
```rust
#[test]
fn test_json_create_and_read() { ... }

#[test]
fn test_json_set_at_path() { ... }

#[test]
fn test_json_wal_replay() { ... }

#[test]
fn test_json_transaction_commit() { ... }

#[test]
fn test_json_transaction_conflict() { ... }

#[test]
fn test_json_cross_primitive_transaction() { ... }

#[test]
fn test_json_concurrent_access() { ... }

#[test]
fn test_json_version_conflict_detection() { ... }

#[test]
fn test_json_path_conflict_detection() { ... }

#[test]
fn test_json_durability_modes() { ... }
```

**Acceptance Criteria**:
- [ ] WAL replay reconstructs state
- [ ] Transaction isolation works
- [ ] Conflicts detected correctly
- [ ] Cross-primitive atomicity works
- [ ] Concurrent access is safe

---

### Story #255: Non-Regression Benchmark Suite

**File**: `benches/m5_performance.rs` (NEW)

**Deliverable**: Benchmarks ensuring M4 targets maintained

**Benchmark Coverage**:
```rust
// JSON operation benchmarks
#[bench] fn bench_json_create_1kb() { ... }
#[bench] fn bench_json_get_at_path() { ... }
#[bench] fn bench_json_set_at_path() { ... }
#[bench] fn bench_json_delete_at_path() { ... }
#[bench] fn bench_json_deep_path_access() { ... }

// Non-regression benchmarks (M4 targets)
#[bench] fn bench_kv_put_inmemory() { ... }  // < 3µs
#[bench] fn bench_kv_put_buffered() { ... }  // < 30µs
#[bench] fn bench_kv_get_fast_path() { ... } // < 5µs
#[bench] fn bench_event_append() { ... }
#[bench] fn bench_state_read() { ... }
#[bench] fn bench_trace_append() { ... }

// Mixed workload
#[bench] fn bench_mixed_json_kv() { ... }
#[bench] fn bench_cross_primitive_transaction() { ... }
```

**Acceptance Criteria**:
- [ ] JSON create < 1ms for 1KB
- [ ] JSON get < 100µs for 1KB
- [ ] JSON set < 1ms for 1KB
- [ ] KV put InMemory < 3µs (M4 target)
- [ ] KV put Buffered < 30µs (M4 target)
- [ ] No regression > 10%

---

### Story #256: Performance Baseline Documentation

**File**: `docs/performance/M5_BASELINES.md` (NEW)

**Deliverable**: Document performance baselines and testing methodology

**Content**:
```markdown
# M5 Performance Baselines

## Test Environment
- Hardware: [specification]
- OS: [version]
- Rust: [version]
- Commit: [hash]

## JSON Operation Baselines

| Operation | Size | Target | Measured | Status |
|-----------|------|--------|----------|--------|
| create | 1KB | < 1ms | | |
| get_at_path | 1KB | < 100µs | | |
| set_at_path | 1KB | < 1ms | | |
| delete_at_path | 1KB | < 500µs | | |

## Non-Regression Verification

| Operation | M4 Target | M5 Measured | Delta | Status |
|-----------|-----------|-------------|-------|--------|
| KV put (InMemory) | < 3µs | | | |
| KV put (Buffered) | < 30µs | | | |
| KV get (fast path) | < 5µs | | | |
| Event append | | | | |
| State read | | | | |

## Methodology
- Each benchmark runs 1000 iterations
- Warmup: 100 iterations discarded
- Statistics: p50, p95, p99
```

**Acceptance Criteria**:
- [ ] All baselines documented
- [ ] Methodology described
- [ ] Test environment recorded
- [ ] Comparison with M4 included

---

## Summary

This document provides detailed implementation specifications for all 32 stories across 7 epics of M5. Each story includes:

- File location
- Implementation code
- Acceptance criteria

**Key Files Created**:
- `crates/core/src/json.rs` - Core JSON types and operations
- `crates/core/src/json_store.rs` - JsonStore facade
- `crates/core/src/json_tests.rs` - Unit tests
- `crates/core/tests/json_integration.rs` - Integration tests
- `benches/m5_performance.rs` - Performance benchmarks
- `docs/performance/M5_BASELINES.md` - Baseline documentation

**Dependencies**:
```toml
indexmap = "2"
```

Refer to [M5_IMPLEMENTATION_PLAN.md](./M5_IMPLEMENTATION_PLAN.md) for the high-level plan and [M5_ARCHITECTURE.md](../../architecture/M5_ARCHITECTURE.md) for architectural details.
