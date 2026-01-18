# Epic 38: Optional Indexing

**Goal**: Implement opt-in inverted index for faster search

**Dependencies**: Epic 34 (Primitive Search)

---

## Scope

- InvertedIndex structure with posting lists
- Enable/disable index per primitive
- Synchronous index updates on commit
- Index-accelerated search
- Version watermark for snapshot consistency

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #284 | InvertedIndex Structure | FOUNDATION |
| #285 | Enable/Disable Index Per Primitive | HIGH |
| #286 | Index Updates on Commit | HIGH |
| #287 | Index-Accelerated Search | HIGH |
| #288 | Index Version Watermark | HIGH |

---

## Story #284: InvertedIndex Structure

**File**: `crates/search/src/index.rs` (NEW)

**Deliverable**: Inverted index with posting lists

### Implementation

```rust
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use dashmap::DashMap;
use crate::search_types::DocRef;

/// Inverted index for fast keyword search
///
/// Maps tokens to lists of documents containing that token.
/// Used to accelerate search when enabled for a primitive.
///
/// CRITICAL: This is OPTIONAL. Search works without it (via scan).
pub struct InvertedIndex {
    /// Token -> posting list
    postings: DashMap<String, PostingList>,

    /// Document frequency per term (for IDF calculation)
    doc_freqs: DashMap<String, usize>,

    /// Total documents indexed
    total_docs: AtomicUsize,

    /// Whether indexing is enabled
    enabled: AtomicBool,

    /// Version watermark (for snapshot consistency)
    version: AtomicU64,
}

/// List of documents containing a term
#[derive(Debug, Clone, Default)]
pub struct PostingList {
    entries: Vec<PostingEntry>,
}

/// Entry in a posting list
#[derive(Debug, Clone)]
pub struct PostingEntry {
    /// Reference to the document
    pub doc_ref: DocRef,

    /// Term frequency in this document
    pub tf: u32,

    /// Document length in tokens
    pub doc_len: u32,

    /// Document timestamp (for recency)
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

### Acceptance Criteria

- [ ] DashMap for concurrent access
- [ ] PostingList with DocRef, TF, doc_len
- [ ] Atomic enabled flag
- [ ] Atomic version for watermark

---

## Story #285: Enable/Disable Index Per Primitive

**File**: `crates/engine/src/database.rs`

**Deliverable**: API to enable/disable indexing per primitive

### Implementation

```rust
impl Database {
    /// Enable search indexing for a primitive
    ///
    /// When enabled, the index is updated on every commit.
    /// Search uses the index for faster candidate retrieval.
    ///
    /// NOTE: Enabling does NOT backfill existing data.
    /// New data is indexed; existing data requires rebuild.
    pub fn enable_search_index(&self, primitive: PrimitiveKind) -> Result<()> {
        let index = self.get_index_mut(primitive)?;
        index.enable();
        Ok(())
    }

    /// Disable search indexing for a primitive
    ///
    /// Stops updating the index. Does NOT clear existing index.
    /// Search falls back to scan mode.
    pub fn disable_search_index(&self, primitive: PrimitiveKind) -> Result<()> {
        let index = self.get_index_mut(primitive)?;
        index.disable();
        Ok(())
    }

    /// Check if indexing is enabled for a primitive
    pub fn is_search_index_enabled(&self, primitive: PrimitiveKind) -> bool {
        self.get_index(primitive)
            .map(|i| i.is_enabled())
            .unwrap_or(false)
    }

    /// Rebuild index from existing data
    ///
    /// Scans all data for the primitive and builds index.
    /// This can be slow for large datasets.
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

### Acceptance Criteria

- [ ] enable_search_index(primitive) works
- [ ] disable_search_index(primitive) works
- [ ] is_search_index_enabled(primitive) returns correct state
- [ ] rebuild_search_index() scans and indexes existing data

---

## Story #286: Index Updates on Commit

**File**: `crates/search/src/index.rs`

**Deliverable**: Update index when data is committed

### Implementation

```rust
impl InvertedIndex {
    /// Update index for a batch of writes
    ///
    /// Called synchronously during commit.
    /// NOOP if index is disabled.
    pub fn on_commit(&self, writes: &[WriteEntry]) {
        if !self.is_enabled() {
            return;
        }

        for write in writes {
            match &write.operation {
                WriteOp::Put { key, value } => {
                    self.index_document(&write.doc_ref, &write.text, write.ts_micros);
                }
                WriteOp::Delete { key } => {
                    self.remove_document(&write.doc_ref);
                }
            }
        }

        // Bump version watermark
        self.version.fetch_add(1, Ordering::Release);
    }

    /// Add a document to the index
    fn index_document(&self, doc_ref: &DocRef, text: &str, ts_micros: Option<u64>) {
        let tokens = tokenize(text);
        let doc_len = tokens.len() as u32;

        // Count term frequencies
        let mut tf_map: HashMap<String, u32> = HashMap::new();
        for token in &tokens {
            *tf_map.entry(token.clone()).or_insert(0) += 1;
        }

        // Update postings
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

            // Update document frequency
            self.doc_freqs
                .entry(term)
                .and_modify(|c| *c += 1)
                .or_insert(1);
        }

        self.total_docs.fetch_add(1, Ordering::Relaxed);
    }

    /// Remove a document from the index
    fn remove_document(&self, doc_ref: &DocRef) {
        // Remove from all posting lists
        for mut entry in self.postings.iter_mut() {
            let before_len = entry.entries.len();
            entry.entries.retain(|e| &e.doc_ref != doc_ref);

            if entry.entries.len() < before_len {
                // Update doc frequency
                let term = entry.key().clone();
                self.doc_freqs.entry(term).and_modify(|c| *c = c.saturating_sub(1));
            }
        }

        self.total_docs.fetch_sub(1, Ordering::Relaxed);
    }
}
```

### Acceptance Criteria

- [ ] on_commit() updates index synchronously
- [ ] Put adds document to postings
- [ ] Delete removes document from postings
- [ ] Version watermark bumped on update
- [ ] NOOP when index disabled

---

## Story #287: Index-Accelerated Search

**File**: `crates/primitives/src/kv.rs` (and other primitives)

**Deliverable**: Use index for faster candidate retrieval

### Implementation

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

        // Collect candidates from posting lists
        let mut candidates: HashMap<DocRef, f32> = HashMap::new();

        for term in &query_terms {
            if let Some(postings) = index.postings.get(term) {
                let idf = index.compute_idf(term);

                for entry in &postings.entries {
                    // Quick filter: check run_id
                    if entry.doc_ref.run_id() != req.run_id {
                        continue;
                    }

                    // BM25-ish scoring
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

        // Assign ranks and take top-k
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
        // Index is up-to-date if version >= storage version
        index.version() >= self.db.storage_version()
    }
}
```

### Acceptance Criteria

- [ ] Uses index when enabled and up-to-date
- [ ] Falls back to scan when index stale
- [ ] stats.index_used reflects which path taken
- [ ] Index search is faster than scan

---

## Story #288: Index Version Watermark

**File**: `crates/search/src/index.rs`

**Deliverable**: Track index freshness for consistency

### Implementation

```rust
impl InvertedIndex {
    /// Check if index is consistent with a snapshot
    ///
    /// Returns true if all data visible in the snapshot
    /// has been indexed.
    pub fn is_consistent_with(&self, snapshot: &Snapshot) -> bool {
        self.version.load(Ordering::Acquire) >= snapshot.version()
    }

    /// Wait for index to catch up to a version
    ///
    /// Used when search needs to ensure index is up-to-date.
    /// In M6, index updates are synchronous so this is immediate.
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

impl KVStore {
    fn search_with_index(
        &self,
        req: &SearchRequest,
        index: &InvertedIndex,
        snapshot: &Snapshot,
    ) -> Result<SearchResponse> {
        // Check index is consistent with snapshot
        if !index.is_consistent_with(snapshot) {
            // Index is stale - fall back to scan
            return self.search_with_scan(req, snapshot);
        }

        // ... rest of index search ...
    }
}
```

### Acceptance Criteria

- [ ] Version watermark tracks index state
- [ ] is_consistent_with() checks snapshot compatibility
- [ ] Falls back to scan if index is stale
- [ ] No stale reads from index

---

## Zero Overhead When Disabled

The indexing system has **zero overhead** when disabled:

1. **No allocations**: InvertedIndex is lazily initialized
2. **No write amplification**: on_commit() checks enabled flag first
3. **No background work**: M6 index updates are synchronous on commit
4. **No search overhead**: search() checks enabled flag before using index

```rust
impl InvertedIndex {
    pub fn on_commit(&self, writes: &[WriteEntry]) {
        // Early exit if disabled - O(1) check
        if !self.is_enabled() {
            return;
        }
        // ... rest of indexing ...
    }
}
```

---

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn test_index_updated_on_commit() {
        let db = test_db();
        db.enable_search_index(PrimitiveKind::Kv)?;

        db.kv.put(&run_id, "key1", "hello world")?;

        let index = db.get_kv_index();
        assert_eq!(index.total_docs(), 1);
        assert!(index.postings.contains_key("hello"));
    }

    #[test]
    fn test_index_accelerated_search() {
        let db = test_db();
        db.enable_search_index(PrimitiveKind::Kv)?;

        // Add test data
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
}
```

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/search/src/index.rs` | CREATE - InvertedIndex |
| `crates/search/src/lib.rs` | MODIFY - Export index module |
| `crates/engine/src/database.rs` | MODIFY - Add index management APIs |
| `crates/primitives/src/*.rs` | MODIFY - Use index in search |
