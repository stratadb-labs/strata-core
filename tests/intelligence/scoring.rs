//! Tier 4: Scoring Accuracy
//!
//! Tests for BM25-lite scorer correctness.

use strata_intelligence::{tokenize, tokenize_unique, BM25LiteScorer, Scorer, ScorerContext, SearchDoc};
use std::collections::HashMap;

// ============================================================================
// Tokenizer Tests
// ============================================================================

/// Tokenizer lowercases text
#[test]
fn test_tier4_tokenizer_lowercases() {
    let tokens = tokenize("Hello World");
    assert!(tokens.contains(&"hello".to_string()));
    assert!(tokens.contains(&"world".to_string()));
}

/// Tokenizer splits on non-alphanumeric
#[test]
fn test_tier4_tokenizer_splits_punctuation() {
    let tokens = tokenize("hello, world!");
    assert_eq!(tokens, vec!["hello", "world"]);
}

/// Tokenizer filters short tokens
#[test]
fn test_tier4_tokenizer_filters_short() {
    let tokens = tokenize("I am a test");
    // "I", "a" should be filtered (less than 2 chars)
    assert!(!tokens.contains(&"i".to_string()));
    assert!(!tokens.contains(&"a".to_string()));
    assert!(tokens.contains(&"am".to_string()));
    assert!(tokens.contains(&"test".to_string()));
}

/// Tokenize unique removes duplicates
#[test]
fn test_tier4_tokenize_unique() {
    let tokens = tokenize_unique("test test TEST");
    assert_eq!(tokens, vec!["test"]);
}

/// Tokenize unique preserves order
#[test]
fn test_tier4_tokenize_unique_order() {
    let tokens = tokenize_unique("alpha beta alpha gamma");
    assert_eq!(tokens, vec!["alpha", "beta", "gamma"]);
}

// ============================================================================
// BM25 Scorer Tests
// ============================================================================

/// BM25 scorer returns positive score for matching documents
#[test]
fn test_tier4_bm25_positive_for_match() {
    let scorer = BM25LiteScorer::default();
    let doc = SearchDoc::new("the quick brown fox".into());
    let ctx = ScorerContext {
        total_docs: 100,
        doc_freqs: [("quick".into(), 10)].into_iter().collect(),
        avg_doc_len: 10.0,
        now_micros: 0,
        extensions: HashMap::new(),
    };

    let score = scorer.score(&doc, "quick", &ctx);
    assert!(score > 0.0, "Matching document should have positive score");
}

/// BM25 scorer returns zero for non-matching documents
#[test]
fn test_tier4_bm25_zero_for_no_match() {
    let scorer = BM25LiteScorer::default();
    let doc = SearchDoc::new("hello world".into());
    let ctx = ScorerContext::default();

    let score = scorer.score(&doc, "banana", &ctx);
    assert_eq!(score, 0.0, "Non-matching document should have zero score");
}

/// BM25 scorer returns zero for empty query
#[test]
fn test_tier4_bm25_zero_for_empty_query() {
    let scorer = BM25LiteScorer::default();
    let doc = SearchDoc::new("hello world".into());
    let ctx = ScorerContext::default();

    let score = scorer.score(&doc, "", &ctx);
    assert_eq!(score, 0.0, "Empty query should have zero score");
}

/// BM25 scorer returns zero for empty document
#[test]
fn test_tier4_bm25_zero_for_empty_doc() {
    let scorer = BM25LiteScorer::default();
    let doc = SearchDoc::new("".into());
    let ctx = ScorerContext::default();

    let score = scorer.score(&doc, "test", &ctx);
    assert_eq!(score, 0.0, "Empty document should have zero score");
}

/// Higher TF gives higher score
#[test]
fn test_tier4_bm25_higher_tf_higher_score() {
    let scorer = BM25LiteScorer::default();
    let doc_1x = SearchDoc::new("test".into());
    let doc_5x = SearchDoc::new("test test test test test".into());
    let ctx = ScorerContext {
        total_docs: 100,
        doc_freqs: [("test".into(), 50)].into_iter().collect(),
        avg_doc_len: 10.0,
        now_micros: 0,
        extensions: HashMap::new(),
    };

    let score_1x = scorer.score(&doc_1x, "test", &ctx);
    let score_5x = scorer.score(&doc_5x, "test", &ctx);

    assert!(score_5x > score_1x, "Higher TF should give higher score");
}

/// Rarer terms get higher IDF
#[test]
fn test_tier4_idf_rare_terms_higher() {
    let ctx = ScorerContext {
        total_docs: 100,
        doc_freqs: [("common".into(), 80), ("rare".into(), 5)]
            .into_iter()
            .collect(),
        avg_doc_len: 10.0,
        now_micros: 0,
        extensions: HashMap::new(),
    };

    let idf_common = ctx.idf("common");
    let idf_rare = ctx.idf("rare");

    assert!(idf_rare > idf_common, "Rare terms should have higher IDF");
}

/// IDF for unknown term is maximal
#[test]
fn test_tier4_idf_unknown_term() {
    let ctx = ScorerContext {
        total_docs: 100,
        doc_freqs: [("known".into(), 50)].into_iter().collect(),
        avg_doc_len: 10.0,
        now_micros: 0,
        extensions: HashMap::new(),
    };

    let idf_unknown = ctx.idf("unknown");
    let idf_known = ctx.idf("known");

    assert!(
        idf_unknown > idf_known,
        "Unknown terms should have higher IDF"
    );
}

// ============================================================================
// Scorer Configuration Tests
// ============================================================================

/// BM25 scorer has correct name
#[test]
fn test_tier4_bm25_name() {
    let scorer = BM25LiteScorer::default();
    assert_eq!(scorer.name(), "bm25-lite");
}

/// BM25 custom parameters work
#[test]
fn test_tier4_bm25_custom_params() {
    let scorer = BM25LiteScorer::new(1.5, 0.5);
    let doc = SearchDoc::new("test document".into());
    let ctx = ScorerContext {
        total_docs: 100,
        doc_freqs: [("test".into(), 10)].into_iter().collect(),
        avg_doc_len: 10.0,
        now_micros: 0,
        extensions: HashMap::new(),
    };

    let score = scorer.score(&doc, "test", &ctx);
    assert!(score > 0.0);
}

/// Scorer is Send + Sync
#[test]
fn test_tier4_scorer_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<BM25LiteScorer>();
}

// ============================================================================
// SearchDoc Tests
// ============================================================================

/// SearchDoc builder works
#[test]
fn test_tier4_search_doc_builder() {
    let doc = SearchDoc::new("body text".into())
        .with_title("Title".into())
        .with_timestamp(12345);

    assert_eq!(doc.body, "body text");
    assert_eq!(doc.title, Some("Title".into()));
    assert_eq!(doc.ts_micros, Some(12345));
}

/// Title match boosts score
#[test]
fn test_tier4_title_match_boost() {
    let scorer = BM25LiteScorer::default();
    // Both have same body content containing "test"
    let doc_no_title = SearchDoc::new("test document body".into());
    let doc_with_title =
        SearchDoc::new("test document body".into()).with_title("test title".into());

    let ctx = ScorerContext {
        total_docs: 100,
        doc_freqs: [("test".into(), 10)].into_iter().collect(),
        avg_doc_len: 10.0,
        now_micros: 0,
        extensions: HashMap::new(),
    };

    let score_no_title = scorer.score(&doc_no_title, "test", &ctx);
    let score_with_title = scorer.score(&doc_with_title, "test", &ctx);

    // Title match should boost score (1.2x boost per implementation)
    assert!(
        score_with_title > score_no_title,
        "Title match should provide boost: with_title={} > no_title={}",
        score_with_title,
        score_no_title
    );
}
