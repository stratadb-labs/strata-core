# Epic 27: Path Operations - Implementation Prompts

**Epic Goal**: Implement operations on JSON documents using paths

**GitHub Issue**: [#257](https://github.com/anibjoshi/in-mem/issues/257)
**Status**: Ready after Epic 26
**Dependencies**: Epic 26 complete

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M5_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M5_ARCHITECTURE.md`
2. **Epic Spec**: `docs/milestones/M5/EPIC_27_PATH_OPERATIONS.md`
3. **Prompt Header**: `docs/prompts/M5/M5_PROMPT_HEADER.md` for the 6 architectural rules

**The architecture spec is LAW.** Epic docs provide implementation details but MUST NOT contradict the architecture spec.

---

## Epic 27 Overview

### Scope
- Path traversal (get_at_path)
- Path mutation (set_at_path)
- Path deletion (delete_at_path)
- Patch application (apply_patches)

### Success Criteria
- [ ] get_at_path() returns value at arbitrary depth
- [ ] set_at_path() creates intermediate objects/arrays as needed
- [ ] delete_at_path() removes values and shifts arrays
- [ ] apply_patches() applies multiple operations atomically
- [ ] All operations handle edge cases (missing paths, type mismatches)

### Component Breakdown
- **Story #230 (GitHub #268)**: Path Traversal (get_at_path)
- **Story #231 (GitHub #269)**: Path Mutation (set_at_path)
- **Story #232 (GitHub #270)**: Path Deletion (delete_at_path)
- **Story #233 (GitHub #271)**: Patch Application (apply_patches)

---

## Dependency Graph

```
Story #268 (get_at_path) ──┬──> Story #269 (set_at_path)
                          └──> Story #270 (delete_at_path)
                                      │
                                      └──> Story #271 (apply_patches)
```

---

## Story #268: Path Traversal (get_at_path)

**GitHub Issue**: [#268](https://github.com/anibjoshi/in-mem/issues/268)
**Estimated Time**: 2 hours
**Dependencies**: Epic 26 complete

### Start Story

```bash
gh issue view 268
./scripts/start-story.sh 27 268 get-at-path
```

### Implementation

Add to `crates/core/src/json_types.rs`:

```rust
/// Get value at path in a JSON document
pub fn get_at_path<'a>(value: &'a JsonValue, path: &JsonPath) -> Option<&'a JsonValue> {
    let mut current = value;
    for segment in path.segments() {
        match (segment, current) {
            (PathSegment::Key(key), JsonValue::Object(obj)) => {
                current = obj.get(key)?;
            }
            (PathSegment::Index(idx), JsonValue::Array(arr)) => {
                current = arr.get(*idx)?;
            }
            _ => return None,
        }
    }
    Some(current)
}

/// Get mutable reference to value at path
pub fn get_at_path_mut<'a>(value: &'a mut JsonValue, path: &JsonPath) -> Option<&'a mut JsonValue> {
    let mut current = value;
    for segment in path.segments() {
        match (segment, current) {
            (PathSegment::Key(key), JsonValue::Object(obj)) => {
                current = obj.get_mut(key)?;
            }
            (PathSegment::Index(idx), JsonValue::Array(arr)) => {
                current = arr.get_mut(*idx)?;
            }
            _ => return None,
        }
    }
    Some(current)
}
```

### Tests

```rust
#[test]
fn test_get_at_root() {
    let value = JsonValue::from(42);
    assert_eq!(get_at_path(&value, &JsonPath::root()), Some(&value));
}

#[test]
fn test_get_at_nested_path() {
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
fn test_get_missing_path() {
    let value = JsonValue::Object(IndexMap::new());
    let path = JsonPath::parse("missing").unwrap();
    assert!(get_at_path(&value, &path).is_none());
}
```

### Complete Story

```bash
./scripts/complete-story.sh 268
```

---

## Story #269: Path Mutation (set_at_path)

**GitHub Issue**: [#269](https://github.com/anibjoshi/in-mem/issues/269)
**Estimated Time**: 3 hours
**Dependencies**: Story #268

### Start Story

```bash
gh issue view 269
./scripts/start-story.sh 27 269 set-at-path
```

### Implementation

```rust
/// Set value at path, creating intermediate containers as needed
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

    // Navigate to parent, creating containers as needed
    let mut current = root;
    for (i, segment) in parent_segments.iter().enumerate() {
        current = ensure_container(current, segment, &segments[i + 1])?;
    }

    // Set the value
    match (&last_segment[0], current) {
        (PathSegment::Key(key), JsonValue::Object(obj)) => {
            obj.insert(key.clone(), value);
            Ok(())
        }
        (PathSegment::Index(idx), JsonValue::Array(arr)) => {
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
        _ => Err(JsonPathError::TypeMismatch),
    }
}

fn ensure_container<'a>(
    current: &'a mut JsonValue,
    segment: &PathSegment,
    next_segment: &PathSegment,
) -> Result<&'a mut JsonValue, JsonPathError> {
    match (segment, current) {
        (PathSegment::Key(key), JsonValue::Object(obj)) => {
            if !obj.contains_key(key) {
                let new_value = match next_segment {
                    PathSegment::Key(_) => JsonValue::Object(IndexMap::new()),
                    PathSegment::Index(_) => JsonValue::Array(Vec::new()),
                };
                obj.insert(key.clone(), new_value);
            }
            obj.get_mut(key).ok_or(JsonPathError::TypeMismatch)
        }
        (PathSegment::Index(idx), JsonValue::Array(arr)) => {
            if *idx >= arr.len() {
                return Err(JsonPathError::IndexOutOfBounds { index: *idx, len: arr.len() });
            }
            Ok(&mut arr[*idx])
        }
        _ => Err(JsonPathError::TypeMismatch),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum JsonPathError {
    #[error("type mismatch: expected object or array")]
    TypeMismatch,
    #[error("index out of bounds: {index} >= {len}")]
    IndexOutOfBounds { index: usize, len: usize },
}
```

### Tests

```rust
#[test]
fn test_set_at_root() {
    let mut value = JsonValue::from(1);
    set_at_path(&mut value, &JsonPath::root(), JsonValue::from(2)).unwrap();
    assert_eq!(value.as_i64(), Some(2));
}

#[test]
fn test_set_creates_intermediate() {
    let mut value = JsonValue::Object(IndexMap::new());
    let path = JsonPath::parse("a.b.c").unwrap();
    set_at_path(&mut value, &path, JsonValue::from(42)).unwrap();

    let result = get_at_path(&value, &path);
    assert_eq!(result.and_then(|v| v.as_i64()), Some(42));
}
```

### Complete Story

```bash
./scripts/complete-story.sh 269
```

---

## Story #270: Path Deletion (delete_at_path)

**GitHub Issue**: [#270](https://github.com/anibjoshi/in-mem/issues/270)
**Estimated Time**: 2 hours
**Dependencies**: Story #268

### Start Story

```bash
gh issue view 270
./scripts/start-story.sh 27 270 delete-at-path
```

### Implementation

```rust
/// Delete value at path, returning the deleted value
pub fn delete_at_path(
    root: &mut JsonValue,
    path: &JsonPath,
) -> Result<Option<JsonValue>, JsonPathError> {
    if path.is_root() {
        let old = std::mem::replace(root, JsonValue::Null);
        return Ok(Some(old));
    }

    let parent_path = path.parent().ok_or(JsonPathError::TypeMismatch)?;
    let parent = get_at_path_mut(root, &parent_path).ok_or(JsonPathError::TypeMismatch)?;

    match (path.segments().last().unwrap(), parent) {
        (PathSegment::Key(key), JsonValue::Object(obj)) => {
            Ok(obj.shift_remove(key))
        }
        (PathSegment::Index(idx), JsonValue::Array(arr)) => {
            if *idx < arr.len() {
                Ok(Some(arr.remove(*idx)))
            } else {
                Ok(None)
            }
        }
        _ => Err(JsonPathError::TypeMismatch),
    }
}
```

### Tests

```rust
#[test]
fn test_delete_object_key() {
    let mut obj = IndexMap::new();
    obj.insert("a".to_string(), JsonValue::from(1));
    obj.insert("b".to_string(), JsonValue::from(2));
    let mut value = JsonValue::Object(obj);

    let deleted = delete_at_path(&mut value, &JsonPath::parse("a").unwrap()).unwrap();
    assert_eq!(deleted.and_then(|v| v.as_i64()), Some(1));
    assert!(get_at_path(&value, &JsonPath::parse("a").unwrap()).is_none());
}

#[test]
fn test_delete_array_element_shifts() {
    let mut value = JsonValue::Array(vec![
        JsonValue::from(1),
        JsonValue::from(2),
        JsonValue::from(3),
    ]);

    delete_at_path(&mut value, &JsonPath::parse("[1]").unwrap()).unwrap();

    let arr = value.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[1].as_i64(), Some(3));  // Shifted
}
```

### Complete Story

```bash
./scripts/complete-story.sh 270
```

---

## Story #271: Patch Application (apply_patches)

**GitHub Issue**: [#271](https://github.com/anibjoshi/in-mem/issues/271)
**Estimated Time**: 2 hours
**Dependencies**: Stories #269, #270

### Start Story

```bash
gh issue view 271
./scripts/start-story.sh 27 271 apply-patches
```

### Implementation

```rust
/// Apply multiple patches to a document
pub fn apply_patches(
    root: &mut JsonValue,
    patches: &[JsonPatch],
) -> Result<(), JsonPathError> {
    for patch in patches {
        match patch {
            JsonPatch::Set { path, value } => {
                set_at_path(root, path, value.clone())?;
            }
            JsonPatch::Delete { path } => {
                delete_at_path(root, path)?;
            }
        }
    }
    Ok(())
}
```

### Tests

```rust
#[test]
fn test_apply_multiple_patches() {
    let mut value = JsonValue::Object(IndexMap::new());

    let patches = vec![
        JsonPatch::set(JsonPath::parse("a").unwrap(), 1),
        JsonPatch::set(JsonPath::parse("b").unwrap(), 2),
        JsonPatch::set(JsonPath::parse("c").unwrap(), 3),
    ];

    apply_patches(&mut value, &patches).unwrap();

    assert_eq!(get_at_path(&value, &JsonPath::parse("a").unwrap()).and_then(|v| v.as_i64()), Some(1));
    assert_eq!(get_at_path(&value, &JsonPath::parse("b").unwrap()).and_then(|v| v.as_i64()), Some(2));
    assert_eq!(get_at_path(&value, &JsonPath::parse("c").unwrap()).and_then(|v| v.as_i64()), Some(3));
}
```

### Complete Story

```bash
./scripts/complete-story.sh 271
```

---

## Epic 27 Completion Checklist

### Final Validation

```bash
~/.cargo/bin/cargo test -p in-mem-core -- json
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace -- -D warnings
```

### Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-27-path-operations -m "Epic 27: Path Operations complete"
git push origin develop
gh issue close 257 --comment "Epic 27: Path Operations - COMPLETE"
```
