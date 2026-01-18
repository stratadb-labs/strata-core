# Epic 34: Primitive Search Surface - Implementation Prompts

**Epic Goal**: Implement `.search()` on each primitive

**GitHub Issue**: [#296](https://github.com/anibjoshi/in-mem/issues/296)
**Status**: Ready after Epic 33
**Dependencies**: Epic 33 (Core Search Types)

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M6_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M6_ARCHITECTURE.md`
2. **Epic Spec**: `docs/milestones/M6/EPIC_34_PRIMITIVE_SEARCH.md`
3. **Prompt Header**: `docs/prompts/M6/M6_PROMPT_HEADER.md` for the 6 architectural rules

---

## Epic 34 Overview

### Scope
- Searchable trait definition
- search() method on each of 6 primitives
- Text extraction strategies per primitive
- Budget enforcement in each primitive
- Snapshot-consistent search

### Success Criteria
- [ ] Searchable trait defined
- [ ] All 6 primitives implement search()
- [ ] Budget enforcement works (time and candidates)
- [ ] Text extraction appropriate per primitive
- [ ] Snapshot consistency maintained

### Component Breakdown
- **Story #263 (GitHub #308)**: Searchable Trait Definition - FOUNDATION
- **Story #264 (GitHub #309)**: KVStore.search() Implementation - CRITICAL
- **Story #265 (GitHub #310)**: JsonStore.search() Implementation - CRITICAL
- **Story #266 (GitHub #311)**: EventLog.search() Implementation - CRITICAL
- **Story #267 (GitHub #312)**: StateCell.search() Implementation - CRITICAL
- **Story #268 (GitHub #313)**: TraceStore.search() Implementation - CRITICAL
- **Story #269 (GitHub #314)**: RunIndex.search() Implementation - CRITICAL
- **Story #270 (GitHub #315)**: Text Extraction Per Primitive - HIGH

---

## Dependency Graph

```
Story #308 (Searchable Trait) ──┬──> Story #309 (KV Search)
                                ├──> Story #310 (JSON Search)
                                ├──> Story #311 (Event Search)
                                ├──> Story #312 (State Search)
                                ├──> Story #313 (Trace Search)
                                └──> Story #314 (Run Search)
                                           │
                                           └──> Story #315 (Text Extraction)
```

---

## Parallelization Strategy

### Optimal Execution (3 Claudes)

| Phase | Duration | Claude 1 | Claude 2 | Claude 3 |
|-------|----------|----------|----------|----------|
| 1 | 2 hours | #308 Searchable Trait | - | - |
| 2 | 3 hours | #309 KV Search | #310 JSON Search | #311 Event Search |
| 3 | 3 hours | #312 State Search | #313 Trace Search | #314 Run Search |
| 4 | 2 hours | #315 Text Extraction | - | - |

**Total Wall Time**: ~10 hours (vs. ~18 hours sequential)

---

## Story #308: Searchable Trait Definition

**GitHub Issue**: [#308](https://github.com/anibjoshi/in-mem/issues/308)
**Estimated Time**: 2 hours
**Dependencies**: Epic 33 complete
**Blocks**: All primitive search stories

### Start Story

```bash
gh issue view 308
./scripts/start-story.sh 34 308 searchable-trait
```

### Implementation

Create `crates/primitives/src/searchable.rs`:

```rust
use crate::search_types::{SearchRequest, SearchResponse, PrimitiveKind};
use crate::error::Result;

/// Trait for primitives that support search
pub trait Searchable {
    /// Search within this primitive
    fn search(&self, req: &SearchRequest) -> Result<SearchResponse>;

    /// Get the primitive kind
    fn primitive_kind(&self) -> PrimitiveKind;
}
```

### Complete Story

```bash
./scripts/complete-story.sh 308
```

---

## Story #309: KVStore.search() Implementation

**GitHub Issue**: [#309](https://github.com/anibjoshi/in-mem/issues/309)
**Estimated Time**: 3 hours
**Dependencies**: Story #308

### Start Story

```bash
gh issue view 309
./scripts/start-story.sh 34 309 kv-search
```

### Implementation

Add to `crates/primitives/src/kv.rs`:

```rust
impl KVStore {
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        let start = Instant::now();
        let snapshot = self.db.snapshot();
        let mut candidates = Vec::new();
        let mut truncated = false;

        for (key, value) in snapshot.scan_kv_prefix(&req.run_id)? {
            // Check budget
            if start.elapsed().as_micros() as u64 >= req.budget.max_wall_time_micros {
                truncated = true;
                break;
            }
            if candidates.len() >= req.budget.max_candidates_per_primitive {
                truncated = true;
                break;
            }

            let text = self.extract_text(&key, &value);

            candidates.push(SearchCandidate {
                doc_ref: DocRef::Kv { key: key.clone() },
                text,
                timestamp: value.timestamp(),
            });
        }

        let hits = self.score_and_rank(candidates, &req.query, req.k)?;

        Ok(SearchResponse {
            hits,
            truncated,
            stats: SearchStats::new(start.elapsed().as_micros() as u64, candidates.len()),
        })
    }

    fn extract_text(&self, key: &Key, value: &VersionedValue) -> String {
        let mut parts = vec![key.user_key_str().to_string()];
        match &value.value {
            Value::String(s) => parts.push(s.clone()),
            Value::Bytes(b) => {
                if let Ok(s) = std::str::from_utf8(b) {
                    parts.push(s.to_string());
                }
            }
            other => {
                if let Ok(s) = serde_json::to_string(other) {
                    parts.push(s);
                }
            }
        }
        parts.join(" ")
    }
}

impl Searchable for KVStore {
    fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        self.search(req)
    }

    fn primitive_kind(&self) -> PrimitiveKind {
        PrimitiveKind::Kv
    }
}
```

### Tests

```rust
#[test]
fn test_kv_search() {
    let db = test_db();
    db.kv.put(&run_id, "greeting", "hello world")?;
    db.kv.put(&run_id, "farewell", "goodbye world")?;

    let req = SearchRequest::new(run_id, "hello");
    let response = db.kv.search(&req)?;

    assert_eq!(response.hits.len(), 1);
    assert!(matches!(response.hits[0].doc_ref, DocRef::Kv { .. }));
}
```

### Complete Story

```bash
./scripts/complete-story.sh 309
```

---

## Story #310: JsonStore.search() Implementation

**GitHub Issue**: [#310](https://github.com/anibjoshi/in-mem/issues/310)
**Estimated Time**: 3 hours
**Dependencies**: Story #308

### Start Story

```bash
gh issue view 310
./scripts/start-story.sh 34 310 json-search
```

### Implementation

Add to `crates/primitives/src/json_store.rs`:

```rust
impl JsonStore {
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        let start = Instant::now();
        let snapshot = self.db.snapshot();
        let mut candidates = Vec::new();
        let mut truncated = false;

        for (key, doc) in snapshot.scan_json_prefix(&req.run_id)? {
            if start.elapsed().as_micros() as u64 >= req.budget.max_wall_time_micros {
                truncated = true;
                break;
            }
            if candidates.len() >= req.budget.max_candidates_per_primitive {
                truncated = true;
                break;
            }

            let text = self.flatten_json(&doc.value);

            candidates.push(SearchCandidate {
                doc_ref: DocRef::Json { key: key.clone(), doc_id: doc.doc_id },
                text,
                timestamp: Some(doc.updated_at),
            });
        }

        let hits = self.score_and_rank(candidates, &req.query, req.k)?;

        Ok(SearchResponse {
            hits,
            truncated,
            stats: SearchStats::new(start.elapsed().as_micros() as u64, candidates.len()),
        })
    }

    fn flatten_json(&self, value: &JsonValue) -> String {
        let mut parts = Vec::new();
        self.flatten_recursive(value, &mut parts, "");
        parts.join(" ")
    }

    fn flatten_recursive(&self, value: &JsonValue, parts: &mut Vec<String>, path: &str) {
        match value {
            JsonValue::String(s) => {
                parts.push(s.clone());
                if !path.is_empty() {
                    parts.push(format!("{}: {}", path, s));
                }
            }
            JsonValue::Array(arr) => {
                for (i, item) in arr.iter().enumerate() {
                    let child_path = format!("{}[{}]", path, i);
                    self.flatten_recursive(item, parts, &child_path);
                }
            }
            JsonValue::Object(obj) => {
                for (k, v) in obj.iter() {
                    let child_path = if path.is_empty() { k.clone() } else { format!("{}.{}", path, k) };
                    self.flatten_recursive(v, parts, &child_path);
                }
            }
            _ => {}
        }
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 310
```

---

## Stories #311-#314: Other Primitive Search Implementations

Follow the same pattern as KVStore and JsonStore for:

- **#311**: EventLog.search() - Search event type and payload
- **#312**: StateCell.search() - Search state names and values
- **#313**: TraceStore.search() - Search span names and attributes
- **#314**: RunIndex.search() - Search run ID and metadata

Each implementation should:
1. Take a snapshot
2. Scan entries for the run
3. Check budget (time and candidates)
4. Extract searchable text
5. Score and rank
6. Return SearchResponse

---

## Story #315: Text Extraction Per Primitive

**GitHub Issue**: [#315](https://github.com/anibjoshi/in-mem/issues/315)
**Estimated Time**: 2 hours
**Dependencies**: Stories #309-#314

### Text Extraction Strategies

| Primitive | Extraction Strategy |
|-----------|---------------------|
| **KV** | Key name + string values + JSON-stringified complex values |
| **JSON** | Recursive flatten: all string values + "path: value" pairs |
| **Event** | Event type + JSON-stringified payload |
| **State** | Key name + state name + JSON-stringified value |
| **Trace** | Span name + "attribute: value" pairs |
| **Run** | Run ID + status + name metadata |

### Complete Story

```bash
./scripts/complete-story.sh 315
```

---

## Epic 34 Completion Checklist

### 1. Final Validation

```bash
~/.cargo/bin/cargo test -p primitives -- search
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### 2. Verify Deliverables

- [ ] Searchable trait exists
- [ ] All 6 primitives implement search()
- [ ] Budget enforcement works
- [ ] Text extraction is appropriate per primitive
- [ ] Snapshot consistency maintained

### 3. Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-34-primitive-search -m "Epic 34: Primitive Search Surface complete

Delivered:
- Searchable trait definition
- KVStore.search() implementation
- JsonStore.search() implementation
- EventLog.search() implementation
- StateCell.search() implementation
- TraceStore.search() implementation
- RunIndex.search() implementation
- Text extraction strategies per primitive

Stories: #308-#315
"
git push origin develop
gh issue close 296 --comment "Epic 34: Primitive Search Surface - COMPLETE"
```
