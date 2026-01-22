# Epic 34: Primitive Search Surface

**Goal**: Implement `.search()` on each primitive

**Dependencies**: Epic 33 (Core Search Types)

---

## Scope

- Searchable trait definition
- search() method on each of 6 primitives
- Text extraction strategies per primitive
- Budget enforcement in each primitive
- Snapshot-consistent search

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #263 | Searchable Trait Definition | FOUNDATION |
| #264 | KVStore.search() Implementation | CRITICAL |
| #265 | JsonStore.search() Implementation | CRITICAL |
| #266 | EventLog.search() Implementation | CRITICAL |
| #267 | StateCell.search() Implementation | CRITICAL |
| #268 | TraceStore.search() Implementation | CRITICAL |
| #269 | RunIndex.search() Implementation | CRITICAL |
| #270 | Text Extraction Per Primitive | HIGH |

---

## Story #263: Searchable Trait Definition

**File**: `crates/primitives/src/searchable.rs` (NEW)

**Deliverable**: Trait that all searchable primitives implement

### Implementation

```rust
use crate::search_types::{SearchRequest, SearchResponse, PrimitiveKind};
use crate::error::Result;

/// Trait for primitives that support search
///
/// Each primitive implements this trait to provide its own
/// search functionality with primitive-specific text extraction.
pub trait Searchable {
    /// Search within this primitive
    ///
    /// Returns results matching the query within budget constraints.
    /// Uses a snapshot for consistency.
    fn search(&self, req: &SearchRequest) -> Result<SearchResponse>;

    /// Get the primitive kind
    fn primitive_kind(&self) -> PrimitiveKind;
}
```

### Acceptance Criteria

- [ ] Trait defined with search() method
- [ ] primitive_kind() returns correct kind
- [ ] Result type for error handling

---

## Story #264: KVStore.search() Implementation

**File**: `crates/primitives/src/kv.rs`

**Deliverable**: Search method on KVStore

### Implementation

```rust
impl KVStore {
    /// Search KV entries
    ///
    /// Searches key names and string values.
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        let start = Instant::now();
        let snapshot = self.db.snapshot();
        let mut candidates = Vec::new();
        let mut truncated = false;

        // Scan KV entries in the run
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

            // Extract searchable text
            let text = self.extract_text(&key, &value);

            // Apply time range filter if present
            if let Some((start_ts, end_ts)) = req.time_range {
                if let Some(ts) = value.timestamp() {
                    if ts < start_ts || ts > end_ts {
                        continue;
                    }
                }
            }

            candidates.push(SearchCandidate {
                doc_ref: DocRef::Kv { key: key.clone() },
                text,
                timestamp: value.timestamp(),
            });
        }

        // Score and rank candidates
        let hits = self.score_and_rank(candidates, &req.query, req.k)?;

        let stats = SearchStats {
            elapsed_micros: start.elapsed().as_micros() as u64,
            candidates_considered: candidates.len(),
            index_used: false,
            ..Default::default()
        };

        Ok(SearchResponse { hits, truncated, stats })
    }

    fn extract_text(&self, key: &Key, value: &VersionedValue) -> String {
        let mut parts = Vec::new();

        // Include key name
        parts.push(key.user_key_str().to_string());

        // Include value based on type
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

### Acceptance Criteria

- [ ] search() returns SearchResponse
- [ ] Uses snapshot for consistency
- [ ] Budget enforcement (time and candidates)
- [ ] Extracts text from keys and values
- [ ] Time range filter works

---

## Story #265: JsonStore.search() Implementation

**File**: `crates/primitives/src/json_store.rs`

**Deliverable**: Search method on JsonStore

### Implementation

```rust
impl JsonStore {
    /// Search JSON documents
    ///
    /// Flattens JSON structure into searchable text.
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

    /// Flatten JSON into searchable text
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
            JsonValue::Number(n) => {
                parts.push(format!("{}", n));
            }
            JsonValue::Bool(b) => {
                parts.push(format!("{}", b));
            }
            JsonValue::Array(arr) => {
                for (i, item) in arr.iter().enumerate() {
                    let child_path = format!("{}[{}]", path, i);
                    self.flatten_recursive(item, parts, &child_path);
                }
            }
            JsonValue::Object(obj) => {
                for (k, v) in obj.iter() {
                    let child_path = if path.is_empty() {
                        k.clone()
                    } else {
                        format!("{}.{}", path, k)
                    };
                    self.flatten_recursive(v, parts, &child_path);
                }
            }
            JsonValue::Null => {}
        }
    }
}
```

### Acceptance Criteria

- [ ] Flattens JSON into searchable text
- [ ] Includes field names and values
- [ ] Budget enforcement
- [ ] Returns JsonDocId in DocRef

---

## Story #266: EventLog.search() Implementation

**File**: `crates/primitives/src/event.rs`

**Deliverable**: Search method on EventLog

### Implementation

```rust
impl EventLog {
    /// Search events
    ///
    /// Searches event type and payload.
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        let start = Instant::now();
        let snapshot = self.db.snapshot();
        let mut candidates = Vec::new();
        let mut truncated = false;

        for (log_key, events) in snapshot.scan_event_logs(&req.run_id)? {
            for (seq, event) in events.iter().enumerate() {
                if start.elapsed().as_micros() as u64 >= req.budget.max_wall_time_micros {
                    truncated = true;
                    break;
                }
                if candidates.len() >= req.budget.max_candidates_per_primitive {
                    truncated = true;
                    break;
                }

                // Time range filter
                if let Some((start_ts, end_ts)) = req.time_range {
                    if event.timestamp < start_ts || event.timestamp > end_ts {
                        continue;
                    }
                }

                let text = self.extract_event_text(event);

                candidates.push(SearchCandidate {
                    doc_ref: DocRef::Event { log_key: log_key.clone(), seq: seq as u64 },
                    text,
                    timestamp: Some(event.timestamp),
                });
            }
        }

        let hits = self.score_and_rank(candidates, &req.query, req.k)?;

        Ok(SearchResponse {
            hits,
            truncated,
            stats: SearchStats::new(start.elapsed().as_micros() as u64, candidates.len()),
        })
    }

    fn extract_event_text(&self, event: &Event) -> String {
        let mut parts = vec![event.event_type.clone()];
        if let Ok(s) = serde_json::to_string(&event.payload) {
            parts.push(s);
        }
        parts.join(" ")
    }
}
```

### Acceptance Criteria

- [ ] Searches event type and payload
- [ ] Time range filter works
- [ ] Returns log_key and seq in DocRef

---

## Story #267: StateCell.search() Implementation

**File**: `crates/primitives/src/state.rs`

**Deliverable**: Search method on StateCell

### Implementation

```rust
impl StateCell {
    /// Search state cells
    ///
    /// Searches state names and current values.
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        let start = Instant::now();
        let snapshot = self.db.snapshot();
        let mut candidates = Vec::new();
        let mut truncated = false;

        for (key, state) in snapshot.scan_states(&req.run_id)? {
            if start.elapsed().as_micros() as u64 >= req.budget.max_wall_time_micros {
                truncated = true;
                break;
            }
            if candidates.len() >= req.budget.max_candidates_per_primitive {
                truncated = true;
                break;
            }

            let text = self.extract_state_text(&key, &state);

            candidates.push(SearchCandidate {
                doc_ref: DocRef::State { key: key.clone() },
                text,
                timestamp: state.updated_at,
            });
        }

        let hits = self.score_and_rank(candidates, &req.query, req.k)?;

        Ok(SearchResponse {
            hits,
            truncated,
            stats: SearchStats::new(start.elapsed().as_micros() as u64, candidates.len()),
        })
    }

    fn extract_state_text(&self, key: &Key, state: &StateCellData) -> String {
        let mut parts = vec![key.user_key_str().to_string()];
        parts.push(state.name.clone());
        if let Ok(s) = serde_json::to_string(&state.value) {
            parts.push(s);
        }
        parts.join(" ")
    }
}
```

### Acceptance Criteria

- [ ] Searches state names and values
- [ ] Budget enforcement
- [ ] Returns key in DocRef

---

## Story #268: TraceStore.search() Implementation

**File**: `crates/primitives/src/trace.rs`

**Deliverable**: Search method on TraceStore

### Implementation

```rust
impl TraceStore {
    /// Search trace spans
    ///
    /// Searches span names and attributes.
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        let start = Instant::now();
        let snapshot = self.db.snapshot();
        let mut candidates = Vec::new();
        let mut truncated = false;

        for (key, spans) in snapshot.scan_traces(&req.run_id)? {
            for span in spans {
                if start.elapsed().as_micros() as u64 >= req.budget.max_wall_time_micros {
                    truncated = true;
                    break;
                }
                if candidates.len() >= req.budget.max_candidates_per_primitive {
                    truncated = true;
                    break;
                }

                let text = self.extract_span_text(&span);

                candidates.push(SearchCandidate {
                    doc_ref: DocRef::Trace { key: key.clone(), span_id: span.span_id },
                    text,
                    timestamp: Some(span.start_time),
                });
            }
        }

        let hits = self.score_and_rank(candidates, &req.query, req.k)?;

        Ok(SearchResponse {
            hits,
            truncated,
            stats: SearchStats::new(start.elapsed().as_micros() as u64, candidates.len()),
        })
    }

    fn extract_span_text(&self, span: &Span) -> String {
        let mut parts = vec![span.name.clone()];
        for (k, v) in &span.attributes {
            parts.push(format!("{}: {}", k, v));
        }
        parts.join(" ")
    }
}
```

### Acceptance Criteria

- [ ] Searches span names and attributes
- [ ] Returns span_id in DocRef
- [ ] Budget enforcement

---

## Story #269: RunIndex.search() Implementation

**File**: `crates/primitives/src/run_index.rs`

**Deliverable**: Search method on RunIndex

### Implementation

```rust
impl RunIndex {
    /// Search runs
    ///
    /// Searches run metadata.
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        let start = Instant::now();
        let mut candidates = Vec::new();
        let mut truncated = false;

        for (run_id, run_meta) in self.list_runs()? {
            if start.elapsed().as_micros() as u64 >= req.budget.max_wall_time_micros {
                truncated = true;
                break;
            }
            if candidates.len() >= req.budget.max_candidates_per_primitive {
                truncated = true;
                break;
            }

            let text = self.extract_run_text(&run_id, &run_meta);

            candidates.push(SearchCandidate {
                doc_ref: DocRef::Run { run_id },
                text,
                timestamp: Some(run_meta.created_at),
            });
        }

        let hits = self.score_and_rank(candidates, &req.query, req.k)?;

        Ok(SearchResponse {
            hits,
            truncated,
            stats: SearchStats::new(start.elapsed().as_micros() as u64, candidates.len()),
        })
    }

    fn extract_run_text(&self, run_id: &RunId, meta: &RunMetadata) -> String {
        let mut parts = vec![run_id.to_string()];
        parts.push(meta.status.to_string());
        if let Some(name) = &meta.name {
            parts.push(name.clone());
        }
        parts.join(" ")
    }
}
```

### Acceptance Criteria

- [ ] Searches run ID and metadata
- [ ] Returns run_id in DocRef
- [ ] Budget enforcement

---

## Story #270: Text Extraction Per Primitive

**Deliverable**: Text extraction strategies documented and implemented

### Text Extraction Strategies

| Primitive | Extraction Strategy |
|-----------|---------------------|
| **KV** | Key name + string values + JSON-stringified complex values |
| **JSON** | Recursive flatten: all string values + "path: value" pairs |
| **Event** | Event type + JSON-stringified payload |
| **State** | Key name + state name + JSON-stringified value |
| **Trace** | Span name + "attribute: value" pairs |
| **Run** | Run ID + status + name metadata |

### Acceptance Criteria

- [ ] Each primitive has appropriate text extraction
- [ ] Complex values are JSON-stringified
- [ ] Key/field names are included for context

---

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn test_json_search_flattens() {
        let db = test_db();
        let doc_id = db.json.create(&run_id, json!({"user": {"name": "Alice"}}))?;

        let req = SearchRequest::new(run_id, "Alice");
        let response = db.json.search(&req)?;

        assert_eq!(response.hits.len(), 1);
    }

    #[test]
    fn test_search_respects_budget() {
        let db = test_db_with_many_entries();

        let req = SearchRequest::new(run_id, "common")
            .with_budget(SearchBudget::default().with_candidates(10));

        let response = db.kv.search(&req)?;

        assert!(response.truncated);
        assert!(response.stats.candidates_considered <= 10);
    }
}
```

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/primitives/src/searchable.rs` | CREATE - Searchable trait |
| `crates/primitives/src/kv.rs` | MODIFY - Add search() |
| `crates/primitives/src/json_store.rs` | MODIFY - Add search() |
| `crates/primitives/src/event.rs` | MODIFY - Add search() |
| `crates/primitives/src/state.rs` | MODIFY - Add search() |
| `crates/primitives/src/trace.rs` | MODIFY - Add search() |
| `crates/primitives/src/run_index.rs` | MODIFY - Add search() |
