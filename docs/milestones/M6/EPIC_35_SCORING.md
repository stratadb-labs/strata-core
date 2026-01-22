# Epic 35: Scoring Infrastructure

**Goal**: Implement pluggable scoring with BM25-lite default

**Dependencies**: Epic 33 (Core Search Types)

---

## Scope

- Scorer trait for pluggable scoring algorithms
- ScorerContext for scoring metadata
- BM25LiteScorer default implementation
- Basic tokenizer

---

## User Stories

| Story | Description | Priority |
|-------|-------------|----------|
| #271 | Scorer Trait Definition | FOUNDATION |
| #272 | ScorerContext Type | FOUNDATION |
| #273 | BM25LiteScorer Implementation | CRITICAL |
| #274 | Tokenizer (Basic) | HIGH |

---

## Story #271: Scorer Trait Definition

**File**: `crates/search/src/scorer.rs` (NEW)

**Deliverable**: Trait for pluggable scoring algorithms

### Implementation

```rust
use crate::search_types::SearchDoc;

/// Pluggable scoring interface
///
/// Scorers take a document and query and return a relevance score.
/// Higher scores indicate more relevant documents.
///
/// M6 ships with BM25LiteScorer. Future milestones can swap in
/// vector similarity, learned scorers, etc.
pub trait Scorer: Send + Sync {
    /// Score a document against a query
    ///
    /// Returns a score where higher = more relevant.
    /// Scores are not normalized; fusion handles cross-scorer comparisons.
    fn score(&self, doc: &SearchDoc, query: &str, ctx: &ScorerContext) -> f32;

    /// Name for debugging and logging
    fn name(&self) -> &str;
}

/// Internal representation of a document for scoring
///
/// This is an ephemeral view created during search, not stored.
#[derive(Debug, Clone)]
pub struct SearchDoc {
    /// Primary searchable text
    pub body: String,

    /// Optional title (e.g., key name)
    pub title: Option<String>,

    /// Tags for filtering
    pub tags: Vec<String>,

    /// Timestamp in microseconds
    pub ts_micros: Option<u64>,

    /// Document size in bytes
    pub byte_size: Option<u32>,
}

impl SearchDoc {
    pub fn new(body: String) -> Self {
        SearchDoc {
            body,
            title: None,
            tags: vec![],
            ts_micros: None,
            byte_size: None,
        }
    }

    pub fn with_title(mut self, title: String) -> Self {
        self.title = Some(title);
        self
    }

    pub fn with_timestamp(mut self, ts: u64) -> Self {
        self.ts_micros = Some(ts);
        self
    }
}
```

### Acceptance Criteria

- [ ] Scorer trait defined with score() method
- [ ] name() for debugging
- [ ] Send + Sync for thread safety
- [ ] SearchDoc internal type defined

---

## Story #272: ScorerContext Type

**File**: `crates/search/src/scorer.rs`

**Deliverable**: Context type for scoring metadata

### Implementation

```rust
use std::collections::HashMap;

/// Context for scoring operations
///
/// Contains corpus-level statistics needed for algorithms like BM25.
/// Built during candidate enumeration.
///
/// WARNING: This struct is BM25-shaped for M6. Future scorers will need
/// additional signals (recency curves, salience, causality, trace centrality).
/// Use the `extensions` field for forward compatibility.
pub struct ScorerContext {
    /// Total documents in corpus (for IDF calculation)
    pub total_docs: usize,

    /// Document frequency per term (for IDF calculation)
    pub doc_freqs: HashMap<String, usize>,

    /// Average document length in tokens (for length normalization)
    pub avg_doc_len: f32,

    /// Current timestamp for recency calculations
    pub now_micros: u64,

    /// Extension point for future scoring signals
    /// M6: unused. Reserved for future scorer requirements.
    pub extensions: HashMap<String, serde_json::Value>,
}

impl ScorerContext {
    pub fn new(total_docs: usize) -> Self {
        ScorerContext {
            total_docs,
            doc_freqs: HashMap::new(),
            avg_doc_len: 0.0,
            now_micros: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_micros() as u64,
            extensions: HashMap::new(),
        }
    }

    /// Compute IDF for a term
    pub fn idf(&self, term: &str) -> f32 {
        let df = self.doc_freqs.get(term).copied().unwrap_or(0) as f32;
        let n = self.total_docs as f32;
        // Standard IDF formula with smoothing
        ((n - df + 0.5) / (df + 0.5) + 1.0).ln()
    }
}

impl Default for ScorerContext {
    fn default() -> Self {
        Self::new(0)
    }
}
```

### Acceptance Criteria

- [ ] Contains total_docs, doc_freqs, avg_doc_len, now_micros
- [ ] Contains extensions HashMap for future signals
- [ ] idf() helper for IDF calculation
- [ ] Default implementation

---

## Story #273: BM25LiteScorer Implementation

**File**: `crates/search/src/scorer.rs`

**Deliverable**: BM25-inspired scorer as M6 default

### Implementation

```rust
use super::tokenizer::tokenize;

/// BM25-Lite: Simple BM25-inspired scorer for M6
///
/// This is a "hello world" scorer to validate the interface.
/// It is NOT optimized and may produce mediocre results.
/// Future milestones can swap in better scorers.
pub struct BM25LiteScorer {
    /// k1 parameter: term frequency saturation (default 1.2)
    k1: f32,
    /// b parameter: length normalization (default 0.75)
    b: f32,
    /// Optional recency boost factor (0.0 = disabled)
    recency_boost: f32,
}

impl Default for BM25LiteScorer {
    fn default() -> Self {
        BM25LiteScorer {
            k1: 1.2,
            b: 0.75,
            recency_boost: 0.1,  // 10% max boost for recent docs
        }
    }
}

impl BM25LiteScorer {
    pub fn new(k1: f32, b: f32) -> Self {
        BM25LiteScorer { k1, b, recency_boost: 0.0 }
    }

    pub fn with_recency_boost(mut self, factor: f32) -> Self {
        self.recency_boost = factor;
        self
    }
}

impl Scorer for BM25LiteScorer {
    fn score(&self, doc: &SearchDoc, query: &str, ctx: &ScorerContext) -> f32 {
        let query_terms = tokenize(query);
        let doc_terms = tokenize(&doc.body);
        let doc_len = doc_terms.len() as f32;

        if query_terms.is_empty() || doc_terms.is_empty() {
            return 0.0;
        }

        let mut score = 0.0;

        // Count term frequencies in document
        let mut doc_term_counts: HashMap<&str, usize> = HashMap::new();
        for term in &doc_terms {
            *doc_term_counts.entry(term.as_str()).or_insert(0) += 1;
        }

        // BM25 scoring
        for query_term in &query_terms {
            let tf = doc_term_counts.get(query_term.as_str()).copied().unwrap_or(0) as f32;
            if tf == 0.0 {
                continue;
            }

            let idf = ctx.idf(query_term);

            // BM25 term score
            let tf_component = (tf * (self.k1 + 1.0)) /
                (tf + self.k1 * (1.0 - self.b + self.b * doc_len / ctx.avg_doc_len.max(1.0)));

            score += idf * tf_component;
        }

        // Optional recency boost
        if self.recency_boost > 0.0 {
            if let Some(ts) = doc.ts_micros {
                let age_hours = (ctx.now_micros.saturating_sub(ts)) as f32 / 3_600_000_000.0;
                let recency_factor = 1.0 / (1.0 + age_hours / 24.0);  // Decay over 24h
                score *= 1.0 + self.recency_boost * recency_factor;
            }
        }

        // Title match boost (if title contains query terms)
        if let Some(title) = &doc.title {
            let title_terms = tokenize(title);
            for query_term in &query_terms {
                if title_terms.contains(query_term) {
                    score *= 1.2;  // 20% boost for title match
                    break;
                }
            }
        }

        score
    }

    fn name(&self) -> &str {
        "bm25-lite"
    }
}
```

### Acceptance Criteria

- [ ] BM25 formula implemented (TF * IDF with length normalization)
- [ ] k1 and b parameters configurable
- [ ] Optional recency boost
- [ ] Title match boost
- [ ] Returns 0.0 for empty queries/docs

---

## Story #274: Tokenizer (Basic)

**File**: `crates/search/src/tokenizer.rs` (NEW)

**Deliverable**: Basic tokenizer for text processing

### Implementation

```rust
/// Tokenize text into searchable terms
///
/// This is a simple tokenizer for M6:
/// - Lowercase
/// - Split on non-alphanumeric characters
/// - Filter tokens shorter than 2 characters
///
/// Future milestones can add stemming, stopwords, etc.
pub fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| s.len() >= 2)
        .map(String::from)
        .collect()
}

/// Tokenize and deduplicate for query processing
pub fn tokenize_unique(text: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    tokenize(text)
        .into_iter()
        .filter(|t| seen.insert(t.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_basic() {
        let tokens = tokenize("Hello, World!");
        assert_eq!(tokens, vec!["hello", "world"]);
    }

    #[test]
    fn test_tokenize_filters_short() {
        let tokens = tokenize("I am a test");
        assert_eq!(tokens, vec!["am", "test"]);  // "I" and "a" filtered
    }

    #[test]
    fn test_tokenize_unique() {
        let tokens = tokenize_unique("test test TEST");
        assert_eq!(tokens, vec!["test"]);  // deduplicated
    }
}
```

### Acceptance Criteria

- [ ] Lowercases text
- [ ] Splits on non-alphanumeric
- [ ] Filters tokens < 2 chars
- [ ] tokenize_unique() deduplicates

---

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bm25_basic_scoring() {
        let scorer = BM25LiteScorer::default();

        let doc = SearchDoc::new("the quick brown fox jumps over the lazy dog".into());
        let ctx = ScorerContext {
            total_docs: 100,
            doc_freqs: [("quick".into(), 10), ("fox".into(), 5)].into_iter().collect(),
            avg_doc_len: 10.0,
            now_micros: 0,
        };

        let score = scorer.score(&doc, "quick fox", &ctx);
        assert!(score > 0.0);
    }

    #[test]
    fn test_bm25_no_match() {
        let scorer = BM25LiteScorer::default();
        let doc = SearchDoc::new("hello world".into());
        let ctx = ScorerContext::default();

        let score = scorer.score(&doc, "banana", &ctx);
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_bm25_title_boost() {
        let scorer = BM25LiteScorer::default();

        let doc_with_title = SearchDoc::new("some content".into())
            .with_title("test document".into());
        let doc_without_title = SearchDoc::new("test content".into());

        let ctx = ScorerContext {
            total_docs: 10,
            doc_freqs: [("test".into(), 2)].into_iter().collect(),
            avg_doc_len: 5.0,
            now_micros: 0,
        };

        let score_with = scorer.score(&doc_with_title, "test", &ctx);
        let score_without = scorer.score(&doc_without_title, "test", &ctx);

        // Title match should boost score
        assert!(score_with > score_without * 1.1);
    }
}
```

---

## Files Modified/Created

| File | Action |
|------|--------|
| `crates/search/Cargo.toml` | CREATE - New search crate |
| `crates/search/src/lib.rs` | CREATE - Crate root |
| `crates/search/src/scorer.rs` | CREATE - Scorer trait, BM25LiteScorer |
| `crates/search/src/tokenizer.rs` | CREATE - Basic tokenizer |
| `Cargo.toml` (workspace) | MODIFY - Add search crate |
