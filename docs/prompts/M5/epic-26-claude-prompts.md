# Epic 26: Core Types Foundation - Implementation Prompts

**Epic Goal**: Define core JSON types that lock in semantics

**GitHub Issue**: [#256](https://github.com/anibjoshi/in-mem/issues/256)
**Status**: Ready to begin (after M4 complete)
**Dependencies**: M4 complete

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M5_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M5_ARCHITECTURE.md`
2. **Epic Spec**: `docs/milestones/M5/EPIC_26_CORE_TYPES.md`
3. **Prompt Header**: `docs/prompts/M5/M5_PROMPT_HEADER.md` for the 6 architectural rules

**The architecture spec is LAW.** Epic docs provide implementation details but MUST NOT contradict the architecture spec.

---

## Epic 26 Overview

### Scope
- JsonDocId unique identifier type
- JsonValue enum for all JSON types
- JsonPath and PathSegment types
- JsonPatch mutation types
- Document size limits and validation
- TypeTag::Json and Key::new_json() integration

### Success Criteria
- [ ] JsonDocId generates unique UUIDs and roundtrips through bytes
- [ ] TypeTag::Json = 0x11 added to existing enum
- [ ] Key::new_json() constructs keys correctly
- [ ] JsonValue represents all JSON types with IndexMap for objects
- [ ] JsonPath parses strings and supports overlap detection
- [ ] JsonPatch provides set/delete operations
- [ ] Validation enforces size limits

### Component Breakdown
- **Story #225 (GitHub #263)**: JsonDocId Type Definition - FOUNDATION
- **Story #226 (GitHub #264)**: JsonValue Type Definition
- **Story #227 (GitHub #265)**: JsonPath Type Definition
- **Story #228 (GitHub #266)**: JsonPatch Type Definition
- **Story #229 (GitHub #267)**: Document Size Limits

---

## Dependency Graph

```
Story #263 (JsonDocId + TypeTag) ──┬──> Story #264 (JsonValue)
                                   └──> Story #265 (JsonPath)
                                              │
                                              └──> Story #266 (JsonPatch)
                                                        │
                                                        └──> Story #267 (Size Limits)
```

---

## Parallelization Strategy

### Optimal Execution (3 Claudes)

| Phase | Duration | Claude 1 | Claude 2 | Claude 3 |
|-------|----------|----------|----------|----------|
| 1 | 2 hours | #263 JsonDocId | - | - |
| 2 | 3 hours | #264 JsonValue | #265 JsonPath | - |
| 3 | 2 hours | #266 JsonPatch | - | - |
| 4 | 2 hours | #267 Size Limits | - | - |

**Total Wall Time**: ~9 hours (vs. ~12 hours sequential)

---

## Story #263: JsonDocId Type Definition

**GitHub Issue**: [#263](https://github.com/anibjoshi/in-mem/issues/263)
**Estimated Time**: 2 hours
**Dependencies**: M4 complete
**Blocks**: Stories #264, #265

### Start Story

```bash
gh issue view 263
./scripts/start-story.sh 26 263 json-doc-id
```

### Implementation Steps

#### Step 1: Add TypeTag::Json to existing enum

Update `crates/core/src/types.rs`:

```rust
pub enum TypeTag {
    KV = 0x01,
    Event = 0x02,
    State = 0x03,
    Trace = 0x04,
    Run = 0x05,
    Vector = 0x10,
    Json = 0x11,  // NEW
}
```

#### Step 2: Create JsonDocId type

Add to `crates/core/src/types.rs`:

```rust
use uuid::Uuid;

/// Unique identifier for a JSON document within a run
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JsonDocId(Uuid);

impl JsonDocId {
    pub fn new() -> Self {
        JsonDocId(Uuid::new_v4())
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        JsonDocId(uuid)
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }

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
```

#### Step 3: Add Key::new_json()

```rust
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

### Tests

```rust
#[cfg(test)]
mod json_doc_id_tests {
    use super::*;

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
    fn test_json_doc_id_is_copy() {
        let id = JsonDocId::new();
        let id_copy = id;  // Copy
        assert_eq!(id, id_copy);
    }

    #[test]
    fn test_key_new_json() {
        let run_id = RunId::new();
        let doc_id = JsonDocId::new();
        let namespace = Namespace::for_run(run_id);
        let key = Key::new_json(namespace, &doc_id);
        assert_eq!(key.type_tag(), TypeTag::Json);
    }

    #[test]
    fn test_type_tag_json_value() {
        assert_eq!(TypeTag::Json as u8, 0x11);
    }
}
```

### Validation

```bash
~/.cargo/bin/cargo test -p in-mem-core -- json_doc_id
~/.cargo/bin/cargo clippy -p in-mem-core -- -D warnings
```

### Complete Story

```bash
./scripts/complete-story.sh 263
```

---

## Story #264: JsonValue Type Definition

**GitHub Issue**: [#264](https://github.com/anibjoshi/in-mem/issues/264)
**Estimated Time**: 3 hours
**Dependencies**: Story #263

### Start Story

```bash
gh issue view 264
./scripts/start-story.sh 26 264 json-value
```

### Implementation

Create `crates/core/src/json_types.rs`:

```rust
//! JSON value types for M5 JsonStore primitive

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use crate::value::Value;

/// JSON value types supported by JsonStore
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(JsonNumber),
    String(String),
    Array(Vec<JsonValue>),
    Object(IndexMap<String, JsonValue>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum JsonNumber {
    Int(i64),
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

// From implementations for common types
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
        // Implementation per spec
    }
}

impl From<JsonValue> for Value {
    fn from(v: JsonValue) -> Self {
        // Implementation per spec
    }
}
```

Update `crates/core/src/lib.rs`:

```rust
pub mod json_types;
pub use json_types::*;
```

Update `crates/core/Cargo.toml`:

```toml
[dependencies]
indexmap = { version = "2", features = ["serde"] }
```

### Tests

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

### Complete Story

```bash
./scripts/complete-story.sh 264
```

---

## Story #265: JsonPath Type Definition

**GitHub Issue**: [#265](https://github.com/anibjoshi/in-mem/issues/265)
**Estimated Time**: 3 hours
**Dependencies**: Story #263

### Start Story

```bash
gh issue view 265
./scripts/start-story.sh 26 265 json-path
```

### Implementation

Add to `crates/core/src/json_types.rs`:

```rust
/// A path into a JSON document
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JsonPath {
    segments: Vec<PathSegment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PathSegment {
    Key(String),
    Index(usize),
}

impl JsonPath {
    pub fn root() -> Self {
        JsonPath { segments: Vec::new() }
    }

    pub fn parse(s: &str) -> Result<Self, PathParseError> {
        // Implementation per spec
    }

    pub fn key(mut self, key: impl Into<String>) -> Self {
        self.segments.push(PathSegment::Key(key.into()));
        self
    }

    pub fn index(mut self, idx: usize) -> Self {
        self.segments.push(PathSegment::Index(idx));
        self
    }

    pub fn is_root(&self) -> bool { self.segments.is_empty() }
    pub fn segments(&self) -> &[PathSegment] { &self.segments }
    pub fn len(&self) -> usize { self.segments.len() }
    pub fn is_empty(&self) -> bool { self.segments.is_empty() }

    pub fn parent(&self) -> Option<JsonPath> {
        // Implementation per spec
    }

    pub fn is_ancestor_of(&self, other: &JsonPath) -> bool {
        // Implementation per spec
    }

    pub fn overlaps(&self, other: &JsonPath) -> bool {
        self.is_ancestor_of(other) || other.is_ancestor_of(self)
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

### Tests

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

    assert!(root.overlaps(&foo));
    assert!(foo.overlaps(&foo_bar));
    assert!(!foo.overlaps(&baz));
}

#[test]
fn test_path_display() {
    let path = JsonPath::root().key("foo").index(0).key("bar");
    assert_eq!(format!("{}", path), "$.foo[0].bar");
}
```

### Complete Story

```bash
./scripts/complete-story.sh 265
```

---

## Story #266: JsonPatch Type Definition

**GitHub Issue**: [#266](https://github.com/anibjoshi/in-mem/issues/266)
**Estimated Time**: 2 hours
**Dependencies**: Story #265

### Start Story

```bash
gh issue view 266
./scripts/start-story.sh 26 266 json-patch
```

### Implementation

Add to `crates/core/src/json_types.rs`:

```rust
/// A patch operation on a JSON document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JsonPatch {
    Set { path: JsonPath, value: JsonValue },
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

    pub fn conflicts_with(&self, other: &JsonPatch) -> bool {
        self.path().overlaps(other.path())
    }
}
```

### Tests

```rust
#[test]
fn test_patch_conflicts() {
    let patch1 = JsonPatch::set(JsonPath::parse("foo.bar").unwrap(), 42);
    let patch2 = JsonPatch::set(JsonPath::parse("foo.bar.baz").unwrap(), 43);
    let patch3 = JsonPatch::set(JsonPath::parse("other").unwrap(), 44);

    assert!(patch1.conflicts_with(&patch2));
    assert!(!patch1.conflicts_with(&patch3));
}
```

### Complete Story

```bash
./scripts/complete-story.sh 266
```

---

## Story #267: Document Size Limits

**GitHub Issue**: [#267](https://github.com/anibjoshi/in-mem/issues/267)
**Estimated Time**: 2 hours
**Dependencies**: Story #266

### Start Story

```bash
gh issue view 267
./scripts/start-story.sh 26 267 size-limits
```

### Implementation

Add to `crates/core/src/json_types.rs`:

```rust
pub const MAX_DOCUMENT_SIZE: usize = 16 * 1024 * 1024;
pub const MAX_NESTING_DEPTH: usize = 100;
pub const MAX_PATH_LENGTH: usize = 256;
pub const MAX_ARRAY_SIZE: usize = 1_000_000;

pub fn validate_json_value(value: &JsonValue) -> Result<(), JsonValidationError> {
    validate_depth(value, 0)?;
    validate_array_sizes(value)?;
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

### Tests

```rust
#[test]
fn test_depth_validation_passes() {
    let mut value = JsonValue::from(42);
    for _ in 0..50 {
        value = JsonValue::Array(vec![value]);
    }
    assert!(validate_json_value(&value).is_ok());
}

#[test]
fn test_depth_validation_fails() {
    let mut value = JsonValue::from(42);
    for _ in 0..101 {
        value = JsonValue::Array(vec![value]);
    }
    assert!(validate_json_value(&value).is_err());
}

#[test]
fn test_array_size_validation_fails() {
    let large_array: Vec<JsonValue> = (0..1_000_001).map(|i| JsonValue::from(i as i64)).collect();
    let value = JsonValue::Array(large_array);
    assert!(validate_json_value(&value).is_err());
}
```

### Complete Story

```bash
./scripts/complete-story.sh 267
```

---

## Epic 26 Completion Checklist

### 1. Final Validation

```bash
~/.cargo/bin/cargo test -p in-mem-core -- json
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### 2. Verify Deliverables

- [ ] TypeTag::Json = 0x11 exists in enum
- [ ] JsonDocId generates unique IDs
- [ ] Key::new_json() creates correct keys
- [ ] JsonValue supports all JSON types
- [ ] JsonPath parses and supports overlap detection
- [ ] JsonPatch set/delete operations work
- [ ] Validation enforces all limits

### 3. Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-26-core-types -m "Epic 26: Core Types Foundation complete

Delivered:
- TypeTag::Json and Key::new_json()
- JsonDocId unique identifier
- JsonValue with IndexMap for objects
- JsonPath with parsing and overlap detection
- JsonPatch set/delete operations
- Document size limit validation

Stories: #263, #264, #265, #266, #267
"
git push origin develop
gh issue close 256 --comment "Epic 26: Core Types Foundation - COMPLETE"
```

---

## Summary

Epic 26 establishes the foundational JSON types that all subsequent M5 epics build upon. These types define the semantic contracts for JSON documents, paths, and mutations.
