# Epic 35: Scoring Infrastructure - Implementation Prompts

**Epic Goal**: Implement pluggable scoring with BM25-lite default

**GitHub Issue**: [#297](https://github.com/anibjoshi/in-mem/issues/297)
**Status**: Ready after Epic 33
**Dependencies**: Epic 33 (Core Search Types)

---

## AUTHORITATIVE SPECIFICATIONS - READ THESE FIRST

**`docs/architecture/M6_ARCHITECTURE.md` is THE AUTHORITATIVE SPEC.**

Before starting ANY story in this epic, read:
1. **Architecture Spec (AUTHORITATIVE)**: `docs/architecture/M6_ARCHITECTURE.md`
2. **Epic Spec**: `docs/milestones/M6/EPIC_35_SCORING.md`
3. **Prompt Header**: `docs/prompts/M6/M6_PROMPT_HEADER.md` for the 6 architectural rules

**CRITICAL**: Rule 6 - Algorithm Swappable. Scorer is a TRAIT, not hardcoded.

---

## Epic 35 Overview

### Scope
- Scorer trait for pluggable scoring algorithms
- ScorerContext for scoring metadata
- BM25LiteScorer default implementation
- Basic tokenizer

### Success Criteria
- [ ] Scorer trait defined with score() method
- [ ] ScorerContext provides IDF and corpus stats
- [ ] BM25LiteScorer implements Scorer trait
- [ ] Tokenizer lowercases and splits on non-alphanumeric
- [ ] Extensions field reserved for future signals

### Component Breakdown
- **Story #271 (GitHub #316)**: Scorer Trait Definition - FOUNDATION
- **Story #272 (GitHub #317)**: ScorerContext Type - FOUNDATION
- **Story #273 (GitHub #318)**: BM25LiteScorer Implementation - CRITICAL
- **Story #274 (GitHub #319)**: Tokenizer (Basic) - HIGH

---

## Dependency Graph

```
Story #316 (Scorer Trait) ──┬──> Story #318 (BM25LiteScorer)
                            │
Story #317 (ScorerContext) ─┘
                                       │
Story #319 (Tokenizer) ────────────────┘
```

---

## Parallelization Strategy

### Optimal Execution (2 Claudes)

| Phase | Duration | Claude 1 | Claude 2 |
|-------|----------|----------|----------|
| 1 | 2 hours | #316 Scorer Trait | #317 ScorerContext |
| 2 | 2 hours | #319 Tokenizer | - |
| 3 | 3 hours | #318 BM25LiteScorer | - |

**Total Wall Time**: ~7 hours (vs. ~10 hours sequential)

---

## Story #316: Scorer Trait Definition

**GitHub Issue**: [#316](https://github.com/anibjoshi/in-mem/issues/316)
**Estimated Time**: 2 hours
**Dependencies**: Epic 33 complete
**Blocks**: Story #318

### Start Story

```bash
gh issue view 316
./scripts/start-story.sh 35 316 scorer-trait
```

### Implementation

Create `crates/search/src/scorer.rs`:

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
    fn score(&self, doc: &SearchDoc, query: &str, ctx: &ScorerContext) -> f32;

    /// Name for debugging and logging
    fn name(&self) -> &str;
}

/// Internal representation of a document for scoring
#[derive(Debug, Clone)]
pub struct SearchDoc {
    pub body: String,
    pub title: Option<String>,
    pub tags: Vec<String>,
    pub ts_micros: Option<u64>,
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

### Complete Story

```bash
./scripts/complete-story.sh 316
```

---

## Story #317: ScorerContext Type

**GitHub Issue**: [#317](https://github.com/anibjoshi/in-mem/issues/317)
**Estimated Time**: 2 hours
**Dependencies**: None
**Blocks**: Story #318

### Start Story

```bash
gh issue view 317
./scripts/start-story.sh 35 317 scorer-context
```

### Implementation

Add to `crates/search/src/scorer.rs`:

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
    pub total_docs: usize,
    pub doc_freqs: HashMap<String, usize>,
    pub avg_doc_len: f32,
    pub now_micros: u64,
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

    pub fn idf(&self, term: &str) -> f32 {
        let df = self.doc_freqs.get(term).copied().unwrap_or(0) as f32;
        let n = self.total_docs as f32;
        ((n - df + 0.5) / (df + 0.5) + 1.0).ln()
    }
}

impl Default for ScorerContext {
    fn default() -> Self {
        Self::new(0)
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 317
```

---

## Story #318: BM25LiteScorer Implementation

**GitHub Issue**: [#318](https://github.com/anibjoshi/in-mem/issues/318)
**Estimated Time**: 3 hours
**Dependencies**: Stories #316, #317, #319

### Start Story

```bash
gh issue view 318
./scripts/start-story.sh 35 318 bm25-scorer
```

### Implementation

Add to `crates/search/src/scorer.rs`:

```rust
use super::tokenizer::tokenize;
use std::collections::HashMap;

/// BM25-Lite: Simple BM25-inspired scorer for M6
pub struct BM25LiteScorer {
    k1: f32,
    b: f32,
    recency_boost: f32,
}

impl Default for BM25LiteScorer {
    fn default() -> Self {
        BM25LiteScorer {
            k1: 1.2,
            b: 0.75,
            recency_boost: 0.1,
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

        // Count term frequencies
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
            let tf_component = (tf * (self.k1 + 1.0)) /
                (tf + self.k1 * (1.0 - self.b + self.b * doc_len / ctx.avg_doc_len.max(1.0)));

            score += idf * tf_component;
        }

        // Recency boost
        if self.recency_boost > 0.0 {
            if let Some(ts) = doc.ts_micros {
                let age_hours = (ctx.now_micros.saturating_sub(ts)) as f32 / 3_600_000_000.0;
                let recency_factor = 1.0 / (1.0 + age_hours / 24.0);
                score *= 1.0 + self.recency_boost * recency_factor;
            }
        }

        // Title match boost
        if let Some(title) = &doc.title {
            let title_terms = tokenize(title);
            for query_term in &query_terms {
                if title_terms.contains(query_term) {
                    score *= 1.2;
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

### Tests

```rust
#[test]
fn test_bm25_basic_scoring() {
    let scorer = BM25LiteScorer::default();
    let doc = SearchDoc::new("the quick brown fox jumps over the lazy dog".into());
    let ctx = ScorerContext {
        total_docs: 100,
        doc_freqs: [("quick".into(), 10), ("fox".into(), 5)].into_iter().collect(),
        avg_doc_len: 10.0,
        now_micros: 0,
        extensions: HashMap::new(),
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
```

### Complete Story

```bash
./scripts/complete-story.sh 318
```

---

## Story #319: Tokenizer (Basic)

**GitHub Issue**: [#319](https://github.com/anibjoshi/in-mem/issues/319)
**Estimated Time**: 2 hours
**Dependencies**: None

### Start Story

```bash
gh issue view 319
./scripts/start-story.sh 35 319 tokenizer
```

### Implementation

Create `crates/search/src/tokenizer.rs`:

```rust
/// Tokenize text into searchable terms
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
        assert_eq!(tokens, vec!["am", "test"]);
    }

    #[test]
    fn test_tokenize_unique() {
        let tokens = tokenize_unique("test test TEST");
        assert_eq!(tokens, vec!["test"]);
    }
}
```

### Complete Story

```bash
./scripts/complete-story.sh 319
```

---

## Epic 35 Completion Checklist

### 1. Final Validation

```bash
~/.cargo/bin/cargo test -p search -- scorer
~/.cargo/bin/cargo test -p search -- tokenizer
~/.cargo/bin/cargo test --workspace
~/.cargo/bin/cargo clippy --workspace -- -D warnings
~/.cargo/bin/cargo fmt --check
```

### 2. Verify Deliverables

- [ ] Scorer trait is Send + Sync
- [ ] ScorerContext has extensions field
- [ ] BM25LiteScorer implements Scorer
- [ ] Tokenizer lowercases and splits correctly
- [ ] IDF helper works

### 3. Merge to Develop

```bash
git checkout develop
git merge --no-ff epic-35-scoring -m "Epic 35: Scoring Infrastructure complete

Delivered:
- Scorer trait for pluggable scoring
- ScorerContext with IDF and extensions
- BM25LiteScorer default implementation
- Basic tokenizer

Stories: #316-#319
"
git push origin develop
gh issue close 297 --comment "Epic 35: Scoring Infrastructure - COMPLETE"
```
