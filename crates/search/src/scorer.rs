//! Scoring infrastructure for M6 search
//!
//! This module provides:
//! - Scorer trait for pluggable scoring algorithms
//! - ScorerContext for corpus-level statistics
//! - SearchDoc internal document representation
//! - BM25LiteScorer default implementation
//!
//! See `docs/architecture/M6_ARCHITECTURE.md` for authoritative specification.

use crate::tokenizer::tokenize;
use std::collections::HashMap;

// ============================================================================
// SearchDoc
// ============================================================================

/// Internal representation of a document for scoring
///
/// This is an ephemeral view created during search, not stored.
/// Contains all information a scorer might need.
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
    /// Create a new SearchDoc with body text
    pub fn new(body: String) -> Self {
        SearchDoc {
            body,
            title: None,
            tags: vec![],
            ts_micros: None,
            byte_size: None,
        }
    }

    /// Builder: set title
    pub fn with_title(mut self, title: String) -> Self {
        self.title = Some(title);
        self
    }

    /// Builder: set timestamp
    pub fn with_timestamp(mut self, ts: u64) -> Self {
        self.ts_micros = Some(ts);
        self
    }

    /// Builder: set tags
    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    /// Builder: set byte size
    pub fn with_byte_size(mut self, size: u32) -> Self {
        self.byte_size = Some(size);
        self
    }
}

// ============================================================================
// ScorerContext
// ============================================================================

/// Context for scoring operations
///
/// Contains corpus-level statistics needed for algorithms like BM25.
/// Built during candidate enumeration.
///
/// # Warning
///
/// This struct is BM25-shaped for M6. Future scorers will need
/// additional signals (recency curves, salience, causality, trace centrality).
/// Use the `extensions` field for forward compatibility.
#[derive(Debug, Clone)]
pub struct ScorerContext {
    /// Total documents in corpus (for IDF calculation)
    pub total_docs: usize,

    /// Document frequency per term (for IDF calculation)
    pub doc_freqs: HashMap<String, usize>,

    /// Average document length in tokens (for length normalization)
    pub avg_doc_len: f32,

    /// Current timestamp for recency calculations (microseconds)
    pub now_micros: u64,

    /// Extension point for future scoring signals
    /// Current version: unused. Reserved for future scorer requirements.
    pub extensions: HashMap<String, serde_json::Value>,
}

impl ScorerContext {
    /// Create a new ScorerContext
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
    ///
    /// Uses standard IDF formula with smoothing:
    /// IDF(t) = ln((N - df + 0.5) / (df + 0.5) + 1)
    pub fn idf(&self, term: &str) -> f32 {
        let df = self.doc_freqs.get(term).copied().unwrap_or(0) as f32;
        let n = self.total_docs as f32;
        // Standard IDF formula with smoothing
        ((n - df + 0.5) / (df + 0.5) + 1.0).ln()
    }

    /// Add document frequency for a term
    pub fn add_doc_freq(&mut self, term: &str, count: usize) {
        self.doc_freqs.insert(term.to_string(), count);
    }

    /// Set average document length
    pub fn with_avg_doc_len(mut self, len: f32) -> Self {
        self.avg_doc_len = len;
        self
    }
}

impl Default for ScorerContext {
    fn default() -> Self {
        Self::new(0)
    }
}

// ============================================================================
// Scorer Trait
// ============================================================================

/// Pluggable scoring interface
///
/// Scorers take a document and query and return a relevance score.
/// Higher scores indicate more relevant documents.
///
/// # Thread Safety
///
/// Scorers must be Send + Sync for concurrent search operations.
///
/// # Implementation Notes
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

// ============================================================================
// BM25LiteScorer
// ============================================================================

/// BM25-Lite: Simple BM25-inspired scorer for M6
///
/// This is a "hello world" scorer to validate the interface.
/// It is NOT heavily optimized and may produce mediocre results.
/// Future milestones can swap in better scorers.
///
/// # BM25 Formula
///
/// For each query term t:
/// score += IDF(t) * (tf * (k1 + 1)) / (tf + k1 * (1 - b + b * dl/avgdl))
///
/// Where:
/// - tf = term frequency in document
/// - dl = document length
/// - avgdl = average document length
/// - k1 = term saturation parameter (default 1.2)
/// - b = length normalization parameter (default 0.75)
#[derive(Debug, Clone)]
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
            recency_boost: 0.1, // 10% max boost for recent docs
        }
    }
}

impl BM25LiteScorer {
    /// Create a new BM25LiteScorer with custom parameters
    pub fn new(k1: f32, b: f32) -> Self {
        BM25LiteScorer {
            k1,
            b,
            recency_boost: 0.0,
        }
    }

    /// Builder: set recency boost factor
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
            let tf = doc_term_counts
                .get(query_term.as_str())
                .copied()
                .unwrap_or(0) as f32;
            if tf == 0.0 {
                continue;
            }

            let idf = ctx.idf(query_term);

            // BM25 term score
            let avg_len = ctx.avg_doc_len.max(1.0);
            let tf_component = (tf * (self.k1 + 1.0))
                / (tf + self.k1 * (1.0 - self.b + self.b * doc_len / avg_len));

            score += idf * tf_component;
        }

        // Optional recency boost
        if self.recency_boost > 0.0 {
            if let Some(ts) = doc.ts_micros {
                let age_hours = (ctx.now_micros.saturating_sub(ts)) as f32 / 3_600_000_000.0;
                let recency_factor = 1.0 / (1.0 + age_hours / 24.0); // Decay over 24h
                score *= 1.0 + self.recency_boost * recency_factor;
            }
        }

        // Title match boost (if title contains query terms)
        if let Some(title) = &doc.title {
            let title_terms = tokenize(title);
            for query_term in &query_terms {
                if title_terms.contains(query_term) {
                    score *= 1.2; // 20% boost for title match
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================
    // SearchDoc Tests
    // ========================================

    #[test]
    fn test_search_doc_new() {
        let doc = SearchDoc::new("hello world".into());
        assert_eq!(doc.body, "hello world");
        assert!(doc.title.is_none());
        assert!(doc.tags.is_empty());
    }

    #[test]
    fn test_search_doc_builder() {
        let doc = SearchDoc::new("body".into())
            .with_title("title".into())
            .with_timestamp(12345)
            .with_tags(vec!["tag1".into()])
            .with_byte_size(100);

        assert_eq!(doc.body, "body");
        assert_eq!(doc.title, Some("title".into()));
        assert_eq!(doc.ts_micros, Some(12345));
        assert_eq!(doc.tags, vec!["tag1"]);
        assert_eq!(doc.byte_size, Some(100));
    }

    // ========================================
    // ScorerContext Tests
    // ========================================

    #[test]
    fn test_scorer_context_new() {
        let ctx = ScorerContext::new(100);
        assert_eq!(ctx.total_docs, 100);
        assert!(ctx.doc_freqs.is_empty());
        assert!(ctx.now_micros > 0);
    }

    #[test]
    fn test_scorer_context_idf() {
        let mut ctx = ScorerContext::new(100);
        ctx.add_doc_freq("common", 50);
        ctx.add_doc_freq("rare", 1);

        let common_idf = ctx.idf("common");
        let rare_idf = ctx.idf("rare");
        let missing_idf = ctx.idf("missing");

        // Rare terms should have higher IDF
        assert!(rare_idf > common_idf);
        // Missing terms should have highest IDF
        assert!(missing_idf > rare_idf);
    }

    #[test]
    fn test_scorer_context_default() {
        let ctx = ScorerContext::default();
        assert_eq!(ctx.total_docs, 0);
    }

    // ========================================
    // BM25LiteScorer Tests
    // ========================================

    #[test]
    fn test_bm25_basic_scoring() {
        let scorer = BM25LiteScorer::default();
        let doc = SearchDoc::new("the quick brown fox jumps over the lazy dog".into());
        let mut ctx = ScorerContext::new(100);
        ctx.add_doc_freq("quick", 10);
        ctx.add_doc_freq("fox", 5);
        ctx.avg_doc_len = 10.0;

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
    fn test_bm25_empty_query() {
        let scorer = BM25LiteScorer::default();
        let doc = SearchDoc::new("hello world".into());
        let ctx = ScorerContext::default();

        let score = scorer.score(&doc, "", &ctx);
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_bm25_empty_doc() {
        let scorer = BM25LiteScorer::default();
        let doc = SearchDoc::new("".into());
        let ctx = ScorerContext::default();

        let score = scorer.score(&doc, "hello", &ctx);
        assert_eq!(score, 0.0);
    }

    #[test]
    fn test_bm25_title_boost() {
        let scorer = BM25LiteScorer::default();

        // Both docs have "test" in body, but one also has it in title
        let doc_with_title =
            SearchDoc::new("test content here".into()).with_title("test document".into());
        let doc_without_title = SearchDoc::new("test content here".into());

        let mut ctx = ScorerContext::new(10);
        ctx.add_doc_freq("test", 2);
        ctx.avg_doc_len = 5.0;

        let score_with = scorer.score(&doc_with_title, "test", &ctx);
        let score_without = scorer.score(&doc_without_title, "test", &ctx);

        // Title match should boost score by ~20%
        assert!(score_with > score_without * 1.1);
    }

    #[test]
    fn test_bm25_recency_boost() {
        let scorer = BM25LiteScorer::default().with_recency_boost(0.5);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_micros() as u64;

        let recent_doc = SearchDoc::new("test content".into()).with_timestamp(now);
        let old_doc =
            SearchDoc::new("test content".into()).with_timestamp(now - 48 * 3600 * 1_000_000); // 48h ago

        let mut ctx = ScorerContext::new(10);
        ctx.add_doc_freq("test", 2);
        ctx.avg_doc_len = 5.0;
        ctx.now_micros = now;

        let score_recent = scorer.score(&recent_doc, "test", &ctx);
        let score_old = scorer.score(&old_doc, "test", &ctx);

        // Recent doc should score higher
        assert!(score_recent > score_old);
    }

    #[test]
    fn test_bm25_name() {
        let scorer = BM25LiteScorer::default();
        assert_eq!(scorer.name(), "bm25-lite");
    }

    #[test]
    fn test_bm25_custom_params() {
        let scorer = BM25LiteScorer::new(2.0, 0.5);
        assert!((scorer.k1 - 2.0).abs() < f32::EPSILON);
        assert!((scorer.b - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_bm25_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<BM25LiteScorer>();
    }
}
