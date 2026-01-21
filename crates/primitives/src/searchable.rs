//! Searchable trait for primitives that support search
//!
//! This module defines the `Searchable` trait and search support structures
//! used by all primitives in M6 Retrieval Surfaces.

use strata_core::error::Result;
use strata_core::search_types::{
    DocRef, SearchHit, SearchRequest, SearchResponse, SearchStats,
};
use strata_core::PrimitiveType;

/// Trait for primitives that support search
///
/// Each primitive implements this trait to provide its own search functionality
/// with primitive-specific text extraction.
///
/// # Invariant
///
/// All search methods return SearchResponse. No primitive-specific result types.
/// This invariant must not change.
pub trait Searchable {
    /// Search within this primitive
    ///
    /// Returns results matching the query within budget constraints.
    /// Uses a snapshot for consistency.
    fn search(&self, req: &SearchRequest) -> Result<SearchResponse>;

    /// Get the primitive type
    fn primitive_kind(&self) -> PrimitiveType;
}

/// Internal candidate for scoring
///
/// Represents a document that matches the search criteria before final scoring.
#[derive(Debug, Clone)]
pub struct SearchCandidate {
    /// Back-pointer to source record
    pub doc_ref: DocRef,
    /// Extracted text for scoring
    pub text: String,
    /// Timestamp for time-based filtering/ordering
    pub timestamp: Option<u64>,
}

impl SearchCandidate {
    /// Create a new search candidate
    pub fn new(doc_ref: DocRef, text: String, timestamp: Option<u64>) -> Self {
        SearchCandidate {
            doc_ref,
            text,
            timestamp,
        }
    }
}

/// Simple keyword matcher/scorer for M6
///
/// BM25-lite implementation: scores based on term frequency and document length.
/// This is a simplified version - Epic 35 will implement the full Scorer trait.
pub struct SimpleScorer;

impl SimpleScorer {
    /// Score a candidate against a query
    ///
    /// Returns a score in [0.0, 1.0] based on token overlap.
    pub fn score(query: &str, text: &str) -> f32 {
        if query.is_empty() || text.is_empty() {
            return 0.0;
        }

        let query_lower = query.to_lowercase();
        let text_lower = text.to_lowercase();

        let query_tokens: Vec<String> = query_lower
            .split_whitespace()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();

        let text_token_count = text_lower.split_whitespace().count();

        if query_tokens.is_empty() || text_token_count == 0 {
            return 0.0;
        }

        // Count matching tokens
        let mut matches = 0;
        for qt in &query_tokens {
            // Check for substring match (more lenient than exact)
            if text_lower.contains(qt.as_str()) {
                matches += 1;
            }
        }

        if matches == 0 {
            return 0.0;
        }

        // BM25-lite: TF * IDF approximation
        let tf = matches as f32 / query_tokens.len() as f32;

        // Length normalization (shorter docs score higher for same match)
        let length_norm = 1.0 + (text_token_count as f32 / 100.0);
        let length_factor = 1.0 / length_norm;

        // Combine: term frequency * length factor
        (tf * length_factor).clamp(0.01, 1.0)
    }

    /// Score candidates and return top-k hits
    pub fn score_and_rank(
        candidates: Vec<SearchCandidate>,
        query: &str,
        k: usize,
    ) -> Vec<SearchHit> {
        // Score all candidates
        let mut scored: Vec<(SearchCandidate, f32)> = candidates
            .into_iter()
            .map(|c| {
                let score = Self::score(query, &c.text);
                (c, score)
            })
            .filter(|(_, score)| *score > 0.0)
            .collect();

        // Sort by score descending
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Take top-k and convert to SearchHit
        scored
            .into_iter()
            .take(k)
            .enumerate()
            .map(|(i, (candidate, score))| SearchHit {
                doc_ref: candidate.doc_ref,
                score,
                rank: (i + 1) as u32,
                snippet: Some(truncate_text(&candidate.text, 100)),
            })
            .collect()
    }
}

/// Truncate text to max length, adding "..." if truncated
fn truncate_text(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        text.to_string()
    } else {
        format!("{}...", &text[..max_len.saturating_sub(3)])
    }
}

/// Helper to build SearchResponse from scored candidates
pub fn build_search_response(
    candidates: Vec<SearchCandidate>,
    query: &str,
    k: usize,
    truncated: bool,
    elapsed_micros: u64,
) -> SearchResponse {
    let candidates_count = candidates.len();
    let hits = SimpleScorer::score_and_rank(candidates, query, k);

    SearchResponse {
        hits,
        truncated,
        stats: SearchStats::new(elapsed_micros, candidates_count),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strata_core::types::RunId;

    #[test]
    fn test_simple_scorer_basic() {
        let score = SimpleScorer::score("hello world", "hello world this is a test");
        assert!(score > 0.0);
    }

    #[test]
    fn test_simple_scorer_no_match() {
        let score = SimpleScorer::score("xyz", "hello world");
        assert!(score == 0.0);
    }

    #[test]
    fn test_simple_scorer_partial_match() {
        let score = SimpleScorer::score("hello test", "hello world");
        assert!(score > 0.0);
        assert!(score < 1.0);
    }

    #[test]
    fn test_simple_scorer_case_insensitive() {
        let score1 = SimpleScorer::score("Hello", "hello world");
        let score2 = SimpleScorer::score("hello", "HELLO WORLD");
        assert!(score1 > 0.0);
        assert!(score2 > 0.0);
    }

    #[test]
    fn test_score_and_rank() {
        let run_id = RunId::new();
        let candidates = vec![
            SearchCandidate::new(DocRef::Run { run_id }, "hello world".to_string(), None),
            SearchCandidate::new(
                DocRef::Run { run_id },
                "hello hello hello".to_string(),
                None,
            ),
            SearchCandidate::new(DocRef::Run { run_id }, "goodbye world".to_string(), None),
        ];

        let hits = SimpleScorer::score_and_rank(candidates, "hello", 10);
        assert!(!hits.is_empty());
        assert_eq!(hits[0].rank, 1);
        // First result should have higher score
        assert!(hits[0].score >= hits.last().map(|h| h.score).unwrap_or(0.0));
    }

    #[test]
    fn test_score_and_rank_respects_k() {
        let run_id = RunId::new();
        let candidates: Vec<_> = (0..100)
            .map(|i| {
                SearchCandidate::new(
                    DocRef::Run { run_id },
                    format!("hello document {}", i),
                    None,
                )
            })
            .collect();

        let hits = SimpleScorer::score_and_rank(candidates, "hello", 5);
        assert_eq!(hits.len(), 5);
    }

    #[test]
    fn test_truncate_text() {
        assert_eq!(truncate_text("short", 10), "short");
        assert_eq!(truncate_text("this is a longer string", 10), "this is...");
    }
}
