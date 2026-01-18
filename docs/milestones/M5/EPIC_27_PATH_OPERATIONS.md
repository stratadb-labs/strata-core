# Epic 27: Path Operations

**Goal**: Implement path traversal and manipulation

**Dependencies**: Epic 26 complete

**GitHub Issue**: #257

---

## Scope

- Path traversal (get value at path)
- Path mutation (set value at path)
- Path deletion
- Intermediate path creation

---

## User Stories

| Story | Description | Priority | GitHub Issue |
|-------|-------------|----------|--------------|
| #230 | Path Traversal (Get) | CRITICAL | #268 |
| #231 | Path Mutation (Set) | CRITICAL | #269 |
| #232 | Path Deletion | CRITICAL | #270 |
| #233 | Intermediate Path Creation | HIGH | #271 |

---

## Story #230: Path Traversal (Get)

**File**: `crates/core/src/json_types.rs`

**Deliverable**: Function to get value at path

### Implementation

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

### Acceptance Criteria

- [ ] get_at_path returns root for empty path
- [ ] get_at_path navigates object keys
- [ ] get_at_path navigates array indices
- [ ] get_at_path returns None for missing paths
- [ ] get_at_path returns None for type mismatches

### Testing

```rust
#[test]
fn test_get_at_path_root() {
    let value = JsonValue::from(42);
    let result = get_at_path(&value, &JsonPath::root());
    assert_eq!(result, Some(&value));
}

#[test]
fn test_get_at_path_object() {
    let mut obj = IndexMap::new();
    obj.insert("foo".to_string(), JsonValue::from(42));
    let value = JsonValue::Object(obj);

    let path = JsonPath::parse("foo").unwrap();
    let result = get_at_path(&value, &path);
    assert_eq!(result.and_then(|v| v.as_i64()), Some(42));
}

#[test]
fn test_get_at_path_array() {
    let arr = vec![JsonValue::from(1), JsonValue::from(2), JsonValue::from(3)];
    let value = JsonValue::Array(arr);

    let path = JsonPath::parse("[1]").unwrap();
    let result = get_at_path(&value, &path);
    assert_eq!(result.and_then(|v| v.as_i64()), Some(2));
}

#[test]
fn test_get_at_path_nested() {
    let mut inner = IndexMap::new();
    inner.insert("bar".to_string(), JsonValue::from(42));

    let mut outer = IndexMap::new();
    outer.insert("foo".to_string(), JsonValue::Object(inner));

    let value = JsonValue::Object(outer);
    let path = JsonPath::parse("foo.bar").unwrap();
    let result = get_at_path(&value, &path);
    assert_eq!(result.and_then(|v| v.as_i64()), Some(42));
}

#[test]
fn test_get_at_path_missing() {
    let mut obj = IndexMap::new();
    obj.insert("foo".to_string(), JsonValue::from(42));
    let value = JsonValue::Object(obj);

    let path = JsonPath::parse("bar").unwrap();
    let result = get_at_path(&value, &path);
    assert!(result.is_none());
}
```

---

## Story #231: Path Mutation (Set)

**File**: `crates/core/src/json_types.rs`

**Deliverable**: Function to set value at path

### Implementation

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

### Acceptance Criteria

- [ ] set_at_path replaces root for empty path
- [ ] set_at_path creates intermediate objects
- [ ] set_at_path creates intermediate arrays (for push only)
- [ ] set_at_path fails for out-of-bounds array indices
- [ ] set_at_path fails for type mismatches

### Testing

```rust
#[test]
fn test_set_at_path_root() {
    let mut value = JsonValue::from(1);
    set_at_path(&mut value, &JsonPath::root(), JsonValue::from(42)).unwrap();
    assert_eq!(value.as_i64(), Some(42));
}

#[test]
fn test_set_at_path_create_intermediate() {
    let mut value = JsonValue::Object(IndexMap::new());
    let path = JsonPath::parse("foo.bar").unwrap();
    set_at_path(&mut value, &path, JsonValue::from(42)).unwrap();

    let result = get_at_path(&value, &path);
    assert_eq!(result.and_then(|v| v.as_i64()), Some(42));
}

#[test]
fn test_set_at_path_overwrite() {
    let mut obj = IndexMap::new();
    obj.insert("foo".to_string(), JsonValue::from(1));
    let mut value = JsonValue::Object(obj);

    let path = JsonPath::parse("foo").unwrap();
    set_at_path(&mut value, &path, JsonValue::from(42)).unwrap();

    let result = get_at_path(&value, &path);
    assert_eq!(result.and_then(|v| v.as_i64()), Some(42));
}

#[test]
fn test_set_at_path_array_push() {
    let arr = vec![JsonValue::from(1), JsonValue::from(2)];
    let mut value = JsonValue::Array(arr);

    let path = JsonPath::parse("[2]").unwrap();
    set_at_path(&mut value, &path, JsonValue::from(3)).unwrap();

    assert_eq!(value.as_array().unwrap().len(), 3);
}

#[test]
fn test_set_at_path_out_of_bounds() {
    let arr = vec![JsonValue::from(1)];
    let mut value = JsonValue::Array(arr);

    let path = JsonPath::parse("[5]").unwrap();
    let result = set_at_path(&mut value, &path, JsonValue::from(42));
    assert!(result.is_err());
}
```

---

## Story #232: Path Deletion

**File**: `crates/core/src/json_types.rs`

**Deliverable**: Function to delete value at path

### Implementation

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

### Acceptance Criteria

- [ ] delete_at_path replaces root with Null
- [ ] delete_at_path removes object keys
- [ ] delete_at_path removes array elements (shifts indices)
- [ ] delete_at_path returns deleted value
- [ ] delete_at_path returns None for non-existent paths

### Testing

```rust
#[test]
fn test_delete_at_path_root() {
    let mut value = JsonValue::from(42);
    let deleted = delete_at_path(&mut value, &JsonPath::root()).unwrap();
    assert_eq!(deleted.and_then(|v| v.as_i64()), Some(42));
    assert!(value.is_null());
}

#[test]
fn test_delete_at_path_object_key() {
    let mut obj = IndexMap::new();
    obj.insert("foo".to_string(), JsonValue::from(42));
    obj.insert("bar".to_string(), JsonValue::from(43));
    let mut value = JsonValue::Object(obj);

    let path = JsonPath::parse("foo").unwrap();
    let deleted = delete_at_path(&mut value, &path).unwrap();

    assert_eq!(deleted.and_then(|v| v.as_i64()), Some(42));
    assert!(get_at_path(&value, &path).is_none());
    assert!(get_at_path(&value, &JsonPath::parse("bar").unwrap()).is_some());
}

#[test]
fn test_delete_at_path_array_element() {
    let arr = vec![JsonValue::from(1), JsonValue::from(2), JsonValue::from(3)];
    let mut value = JsonValue::Array(arr);

    let path = JsonPath::parse("[1]").unwrap();
    let deleted = delete_at_path(&mut value, &path).unwrap();

    assert_eq!(deleted.and_then(|v| v.as_i64()), Some(2));
    assert_eq!(value.as_array().unwrap().len(), 2);
    // Verify indices shifted
    assert_eq!(value.as_array().unwrap()[1].as_i64(), Some(3));
}

#[test]
fn test_delete_at_path_missing() {
    let mut obj = IndexMap::new();
    obj.insert("foo".to_string(), JsonValue::from(42));
    let mut value = JsonValue::Object(obj);

    let path = JsonPath::parse("bar").unwrap();
    let deleted = delete_at_path(&mut value, &path).unwrap();
    assert!(deleted.is_none());
}
```

---

## Story #233: Intermediate Path Creation

**File**: `crates/core/src/json_types.rs`

**Deliverable**: Helper functions for path creation

### Implementation

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

/// Check if a path exists in a JSON value
pub fn path_exists(root: &JsonValue, path: &JsonPath) -> bool {
    get_at_path(root, path).is_some()
}

/// Get the type of value at a path
pub fn type_at_path(root: &JsonValue, path: &JsonPath) -> Option<&'static str> {
    get_at_path(root, path).map(|v| match v {
        JsonValue::Null => "null",
        JsonValue::Bool(_) => "boolean",
        JsonValue::Number(_) => "number",
        JsonValue::String(_) => "string",
        JsonValue::Array(_) => "array",
        JsonValue::Object(_) => "object",
    })
}
```

### Acceptance Criteria

- [ ] ensure_path creates nested objects
- [ ] ensure_path creates nested arrays where appropriate
- [ ] ensure_path fails for out-of-bounds array access
- [ ] ensure_path is idempotent
- [ ] path_exists correctly reports existence
- [ ] type_at_path returns correct type name

### Testing

```rust
#[test]
fn test_ensure_path_creates_objects() {
    let mut value = JsonValue::Object(IndexMap::new());
    let path = JsonPath::parse("foo.bar.baz").unwrap();

    ensure_path(&mut value, &path).unwrap();

    assert!(get_at_path(&value, &JsonPath::parse("foo").unwrap()).is_some());
    assert!(get_at_path(&value, &JsonPath::parse("foo.bar").unwrap()).is_some());
    assert!(get_at_path(&value, &JsonPath::parse("foo.bar.baz").unwrap()).is_some());
}

#[test]
fn test_ensure_path_creates_array() {
    let mut value = JsonValue::Object(IndexMap::new());
    let path = JsonPath::parse("items[0]").unwrap();

    // First create the items array
    let items_path = JsonPath::parse("items").unwrap();
    set_at_path(&mut value, &items_path, JsonValue::Array(vec![JsonValue::Object(IndexMap::new())])).unwrap();

    // Now ensure_path should work
    ensure_path(&mut value, &path).unwrap();
    assert!(get_at_path(&value, &path).is_some());
}

#[test]
fn test_ensure_path_idempotent() {
    let mut value = JsonValue::Object(IndexMap::new());
    let path = JsonPath::parse("foo.bar").unwrap();

    ensure_path(&mut value, &path).unwrap();
    ensure_path(&mut value, &path).unwrap(); // Should not error

    assert!(get_at_path(&value, &path).is_some());
}

#[test]
fn test_path_exists() {
    let mut obj = IndexMap::new();
    obj.insert("foo".to_string(), JsonValue::from(42));
    let value = JsonValue::Object(obj);

    assert!(path_exists(&value, &JsonPath::parse("foo").unwrap()));
    assert!(!path_exists(&value, &JsonPath::parse("bar").unwrap()));
}

#[test]
fn test_type_at_path() {
    let mut obj = IndexMap::new();
    obj.insert("num".to_string(), JsonValue::from(42));
    obj.insert("str".to_string(), JsonValue::from("hello"));
    obj.insert("arr".to_string(), JsonValue::Array(vec![]));
    let value = JsonValue::Object(obj);

    assert_eq!(type_at_path(&value, &JsonPath::parse("num").unwrap()), Some("number"));
    assert_eq!(type_at_path(&value, &JsonPath::parse("str").unwrap()), Some("string"));
    assert_eq!(type_at_path(&value, &JsonPath::parse("arr").unwrap()), Some("array"));
    assert_eq!(type_at_path(&value, &JsonPath::root()), Some("object"));
}
```

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/core/src/json_types.rs` | MODIFY - Add path operations |

---

## Success Criteria

- [ ] get_at_path() navigates objects and arrays correctly
- [ ] set_at_path() creates intermediate structures as needed
- [ ] delete_at_path() removes values and cleans up empty containers
- [ ] Type mismatches return appropriate errors
- [ ] Root path operations work correctly
