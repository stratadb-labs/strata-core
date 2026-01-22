# Epic 38: Optional Indexing - Implementation Prompts

**Epic Goal**: Implement opt-in inverted index for faster search

**GitHub Issue**: [#300](https://github.com/anibjoshi/in-mem/issues/300)
**Status**: Ready after Epic 34
**Dependencies**: Epic 34 (Primitive Search)

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M6_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M6_ARCHITECTURE.md`
2. **Epic Spec**: `docs/milestones/M6/EPIC_38_INDEXING.md`
3. **Prompt Header**: `docs/prompts/M6/M6_PROMPT_HEADER.md` for the 6 architectural rules

**CRITICAL RULES**:
- Rule 1: No Data Movement (index stores DocRef only, not content)
- Rule 5: Zero Overhead When Disabled

---

## Epic 38 Overview

### Scope
- InvertedIndex structure with posting lists
- Enable/disable index per primitive
- Synchronous index updates on commit
- Index-accelerated search
- Version watermark for snapshot consistency

### Success Criteria
- [ ] InvertedIndex with DashMap for concurrency
- [ ] enable/disable per primitive works
- [ ] Index updated synchronously on commit
- [ ] Falls back to scan when index stale/disabled
- [ ] Zero overhead when disabled

### Component Breakdown
- **Story #284 (GitHub #329)**: InvertedIndex Structure - FOUNDATION
- **Story #285 (GitHub #330)**: Enable/Disable Index Per Primitive - HIGH
- **Story #286 (GitHub #331)**: Index Updates on Commit - HIGH
- **Story #287 (GitHub #332)**: Index-Accelerated Search - HIGH
- **Story #288 (GitHub #333)**: Index Version Watermark - HIGH

---

## Dependency Graph

```
Story #329 (InvertedIndex) ──┬──> Story #330 (Enable/Disable)
                             │
                             ├──> Story #331 (Index Updates)
                             │
                             ├──> Story #332 (Accelerated Search)
                             │
                             └──> Story #333 (Version Watermark)
```

---

## Parallelization Strategy

### Optimal Execution (2 Claudes)

| Phase | Duration | Claude 1 | Claude 2 |
|-------|----------|----------|----------|
| 1 | 3 hours | #329 InvertedIndex | - |
| 2 | 2 hours | #330 Enable/Disable | #331 Index Updates |
| 3 | 3 hours | #332 Accelerated Search | #333 Watermark |

**Total Wall Time**: ~8 hours (vs. ~14 hours sequential)

---

## Story #329: InvertedIndex Structure

**GitHub Issue**: [#329](https://github.com/anibjoshi/in-mem/issues/329)
**Estimated Time**: 3 hours
**Dependencies**: Epic 34 complete
**Blocks**: All other stories in this epic

### Start Story

```bash
gh issue view 329
./scripts/start-story.sh 38 329 inverted-index
```

### Implementation

Create `crates/search/src/index.rs`:

```rust
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use dashmap::DashMap;
use crate::search_types::DocRef;

/// Inverted index for fast keyword search
///
/// CRITICAL: This is OPTIONAL. Search works without it (via scan).
pub struct InvertedIndex {
    postings: DashMap<String, PostingList>,
    doc_freqs: DashMap<String, usize>,
    total_docs: AtomicUsize,
    enabled: AtomicBool,
    version: AtomicU64,
}

#[derive(Debug, Clone, Default)]
pub struct PostingList {
    entries: Vec<PostingEntry>,
}

#[derive(Debug, Clone)]
pub struct PostingEntry {
    pub doc_ref: DocRef,
    pub tf: u32,
    pub doc_len: u32,
    pub ts_micros: Option<u64>,
}

impl InvertedIndex {
    pub fn new() -> Self {
        InvertedIndex {
            postings: DashMap::new(),
            doc_freqs: DashMap::new(),
            total_docs: AtomicUsize::new(0),
            enabled: AtomicBool::new(false),
            version: AtomicU64::new(0),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }

    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Acquire)
    }

    pub fn total_docs(&self) -> usize {
        self.total_docs.load(Ordering::Relaxed)
    }

    pub fn doc_freq(&self, term: &str) -> usize {
        self.doc_freqs.get(term).map(|r| *r).unwrap_or(0)
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 329
```

---

## Story #330: Enable/Disable Index Per Primitive

**GitHub Issue**: [#330](https://github.com/anibjoshi/in-mem/issues/330)
**Estimated Time**: 2 hours
**Dependencies**: Story #329

### Start Story

```bash
gh issue view 330
./scripts/start-story.sh 38 330 enable-disable
```

### Implementation

Add to `crates/engine/src/database.rs`:

```rust
impl Database {
    pub fn enable_search_index(&self, primitive: PrimitiveKind) -> Result<()> {
        let index = self.get_index_mut(primitive)?;
        index.enable();
        Ok(())
    }

    pub fn disable_search_index(&self, primitive: PrimitiveKind) -> Result<()> {
        let index = self.get_index_mut(primitive)?;
        index.disable();
        Ok(())
    }

    pub fn is_search_index_enabled(&self, primitive: PrimitiveKind) -> bool {
        self.get_index(primitive)
            .map(|i| i.is_enabled())
            .unwrap_or(false)
    }

    pub fn rebuild_search_index(&self, primitive: PrimitiveKind) -> Result<()> {
        let index = self.get_index_mut(primitive)?;
        index.clear();
        // Scan and index all existing data
        match primitive {
            PrimitiveKind::Kv => self.rebuild_kv_index(index)?,
            PrimitiveKind::Json => self.rebuild_json_index(index)?,
            // ... other primitives ...
        }
        Ok(())
    }
}

impl InvertedIndex {
    pub fn enable(&self) {
        self.enabled.store(true, Ordering::Release);
    }

    pub fn disable(&self) {
        self.enabled.store(false, Ordering::Release);
    }

    pub fn clear(&self) {
        self.postings.clear();
        self.doc_freqs.clear();
        self.total_docs.store(0, Ordering::Relaxed);
        self.version.fetch_add(1, Ordering::Release);
    }
}
```

### Tests

```rust
#[test]
fn test_index_disabled_by_default() {
    let db = test_db();
    assert!(!db.is_search_index_enabled(PrimitiveKind::Kv));
}

#[test]
fn test_enable_disable_index() {
    let db = test_db();

    db.enable_search_index(PrimitiveKind::Kv)?;
    assert!(db.is_search_index_enabled(PrimitiveKind::Kv));

    db.disable_search_index(PrimitiveKind::Kv)?;
    assert!(!db.is_search_index_enabled(PrimitiveKind::Kv));
}
```

### Complete Story

```bash
./scripts/complete-story.sh 330
```

---

## Story #331: Index Updates on Commit

**GitHub Issue**: [#331](https://github.com/anibjoshi/in-mem/issues/331)
**Estimated Time**: 3 hours
**Dependencies**: Story #329

### Start Story

```bash
gh issue view 331
./scripts/start-story.sh 38 331 index-updates
```

### Implementation

Add to `crates/search/src/index.rs`:

```rust
impl InvertedIndex {
    /// Update index for a batch of writes
    /// NOOP if index is disabled.
    pub fn on_commit(&self, writes: &[WriteEntry]) {
        if !self.is_enabled() {
            return;  // Zero overhead when disabled
        }

        for write in writes {
            match &write.operation {
                WriteOp::Put { .. } => {
                    self.index_document(&write.doc_ref, &write.text, write.ts_micros);
                }
                WriteOp::Delete { .. } => {
                    self.remove_document(&write.doc_ref);
                }
            }
        }

        self.version.fetch_add(1, Ordering::Release);
    }

    fn index_document(&self, doc_ref: &DocRef, text: &str, ts_micros: Option<u64>) {
        let tokens = tokenize(text);
        let doc_len = tokens.len() as u32;

        let mut tf_map: HashMap<String, u32> = HashMap::new();
        for token in &tokens {
            *tf_map.entry(token.clone()).or_insert(0) += 1;
        }

        for (term, tf) in tf_map {
            self.postings
                .entry(term.clone())
                .or_insert_with(PostingList::default)
                .entries
                .push(PostingEntry {
                    doc_ref: doc_ref.clone(),
                    tf,
                    doc_len,
                    ts_micros,
                });

            self.doc_freqs
                .entry(term)
                .and_modify(|c| *c += 1)
                .or_insert(1);
        }

        self.total_docs.fetch_add(1, Ordering::Relaxed);
    }

    fn remove_document(&self, doc_ref: &DocRef) {
        for mut entry in self.postings.iter_mut() {
            let before_len = entry.entries.len();
            entry.entries.retain(|e| &e.doc_ref != doc_ref);

            if entry.entries.len() < before_len {
                let term = entry.key().clone();
                self.doc_freqs.entry(term).and_modify(|c| *c = c.saturating_sub(1));
            }
        }

        self.total_docs.fetch_sub(1, Ordering::Relaxed);
    }
}
```

### Tests

```rust
#[test]
fn test_index_updated_on_commit() {
    let db = test_db();
    db.enable_search_index(PrimitiveKind::Kv)?;

    db.kv.put(&run_id, "key1", "hello world")?;

    let index = db.get_kv_index();
    assert_eq!(index.total_docs(), 1);
    assert!(index.postings.contains_key("hello"));
}
```

### Complete Story

```bash
./scripts/complete-story.sh 331
```

---

## Story #332: Index-Accelerated Search

**GitHub Issue**: [#332](https://github.com/anibjoshi/in-mem/issues/332)
**Estimated Time**: 3 hours
**Dependencies**: Stories #329, #331

### Start Story

```bash
gh issue view 332
./scripts/start-story.sh 38 332 accelerated-search
```

### Implementation

Update `crates/primitives/src/kv.rs`:

```rust
impl KVStore {
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
        let index = self.db.get_kv_index();

        if index.is_enabled() && self.is_index_up_to_date(index) {
            self.search_with_index(req, index)
        } else {
            self.search_with_scan(req)
        }
    }

    fn search_with_index(
        &self,
        req: &SearchRequest,
        index: &InvertedIndex,
    ) -> Result<SearchResponse> {
        let start = Instant::now();
        let query_terms = tokenize(&req.query);

        let mut candidates: HashMap<DocRef, f32> = HashMap::new();

        for term in &query_terms {
            if let Some(postings) = index.postings.get(term) {
                let idf = index.compute_idf(term);

                for entry in &postings.entries {
                    if entry.doc_ref.run_id() != req.run_id {
                        continue;
                    }

                    let tf_norm = entry.tf as f32 / (entry.tf as f32 + 1.0);
                    *candidates.entry(entry.doc_ref.clone()).or_insert(0.0) += idf * tf_norm;
                }
            }
        }

        // Convert to hits and sort
        let mut hits: Vec<_> = candidates.into_iter()
            .map(|(doc_ref, score)| SearchHit::new(doc_ref, score, 0))
            .collect();

        hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal));

        let hits: Vec<_> = hits.into_iter()
            .take(req.k)
            .enumerate()
            .map(|(i, mut h)| {
                h.rank = (i + 1) as u32;
                h
            })
            .collect();

        Ok(SearchResponse {
            hits,
            truncated: false,
            stats: SearchStats {
                elapsed_micros: start.elapsed().as_micros() as u64,
                candidates_considered: candidates.len(),
                index_used: true,
                ..Default::default()
            },
        })
    }

    fn is_index_up_to_date(&self, index: &InvertedIndex) -> bool {
        index.version() >= self.db.storage_version()
    }
}
```

### Tests

```rust
#[test]
fn test_index_accelerated_search() {
    let db = test_db();
    db.enable_search_index(PrimitiveKind::Kv)?;

    for i in 0..1000 {
        db.kv.put(&run_id, &format!("key{}", i), &format!("value {}", i))?;
    }

    let req = SearchRequest::new(run_id, "value");
    let response = db.kv.search(&req)?;

    assert!(response.stats.index_used);
}

#[test]
fn test_search_without_index() {
    let db = test_db();
    // Index NOT enabled

    db.kv.put(&run_id, "key1", "hello world")?;

    let req = SearchRequest::new(run_id, "hello");
    let response = db.kv.search(&req)?;

    assert!(!response.stats.index_used);  // Used scan
    assert_eq!(response.hits.len(), 1);   // Still works
}
```

### Complete Story

```bash
./scripts/complete-story.sh 332
```

---

## Story #333: Index Version Watermark

**GitHub Issue**: [#333](https://github.com/anibjoshi/in-mem/issues/333)
**Estimated Time**: 2 hours
**Dependencies**: Story #329

### Start Story

```bash
gh issue view 333
./scripts/start-story.sh 38 333 watermark
```

### Implementation

Add to `crates/search/src/index.rs`:

```rust
impl InvertedIndex {
    pub fn is_consistent_with(&self, snapshot: &Snapshot) -> bool {
        self.version.load(Ordering::Acquire) >= snapshot.version()
    }

    pub fn wait_for_version(&self, version: u64, timeout: Duration) -> bool {
        let start = Instant::now();
        loop {
            if self.version.load(Ordering::Acquire) >= version {
                return true;
            }
            if start.elapsed() >= timeout {
                return false;
            }
            std::thread::yield_now();
        }
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 333
```

---

## Epic 38 Completion Checklist

### 1. Final Validation

```bash
~/.cargo/bin/cargo test -p search -- index
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### 2. Verify Deliverables

- [ ] InvertedIndex uses DashMap
- [ ] Enable/disable per primitive works
- [ ] Index updates on commit (NOOP when disabled)
- [ ] Index-accelerated search is faster
- [ ] Falls back to scan when stale

### 3. Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-38-indexing -m "Epic 38: Optional Indexing complete

Delivered:
- InvertedIndex with posting lists
- Enable/disable per primitive
- Synchronous index updates on commit
- Index-accelerated search
- Version watermark for consistency

Stories: #329-#333
"
git push origin develop
gh issue close 300 --comment "Epic 38: Optional Indexing - COMPLETE"
```
