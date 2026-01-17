//! Fusion infrastructure for combining search results
//!
//! This module provides:
//! - Fuser trait for pluggable fusion algorithms
//! - SimpleFuser: basic concatenation + sort (M6 default)
//! - RRFFuser: Reciprocal Rank Fusion (Epic 37)
//!
//! See `docs/architecture/M6_ARCHITECTURE.md` for authoritative specification.

use in_mem_core::search_types::{PrimitiveKind, SearchHit, SearchResponse};

// ============================================================================
// FusedResult
// ============================================================================

/// Result of fusing multiple primitive search results
#[derive(Debug, Clone)]
pub struct FusedResult {
    /// Final ranked list of hits
    pub hits: Vec<SearchHit>,
    /// Whether results were truncated
    pub truncated: bool,
}

impl FusedResult {
    /// Create a new FusedResult
    pub fn new(hits: Vec<SearchHit>, truncated: bool) -> Self {
        FusedResult { hits, truncated }
    }
}

// ============================================================================
// Fuser Trait
// ============================================================================

/// Pluggable fusion interface
///
/// Fusers combine search results from multiple primitives into a single
/// ranked list. Different algorithms can prioritize different factors.
///
/// # Thread Safety
///
/// Fusers must be Send + Sync for concurrent search operations.
///
/// # M6 Implementation
///
/// M6 ships with SimpleFuser (sort by score). Epic 37 adds RRFFuser
/// which uses Reciprocal Rank Fusion.
pub trait Fuser: Send + Sync {
    /// Fuse results from multiple primitives
    ///
    /// Takes a list of (primitive, results) pairs and returns a combined
    /// ranked list truncated to k items.
    fn fuse(&self, results: Vec<(PrimitiveKind, SearchResponse)>, k: usize) -> FusedResult;

    /// Name for debugging and logging
    fn name(&self) -> &str;
}

// ============================================================================
// SimpleFuser (M6 Default)
// ============================================================================

/// Simple fusion: concatenate and sort by score
///
/// This is the M6 default fuser. It simply combines all hits from
/// all primitives, sorts by score descending, and takes top-k.
///
/// No rank normalization or primitive weighting.
#[derive(Debug, Clone, Default)]
pub struct SimpleFuser;

impl SimpleFuser {
    /// Create a new SimpleFuser
    pub fn new() -> Self {
        SimpleFuser
    }
}

impl Fuser for SimpleFuser {
    fn fuse(&self, results: Vec<(PrimitiveKind, SearchResponse)>, k: usize) -> FusedResult {
        // Collect all hits
        let mut all_hits: Vec<SearchHit> = results
            .into_iter()
            .flat_map(|(_, response)| response.hits)
            .collect();

        // Sort by score descending
        all_hits.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Take top-k and update ranks
        let truncated = all_hits.len() > k;
        let hits: Vec<SearchHit> = all_hits
            .into_iter()
            .take(k)
            .enumerate()
            .map(|(i, mut hit)| {
                hit.rank = (i + 1) as u32;
                hit
            })
            .collect();

        FusedResult::new(hits, truncated)
    }

    fn name(&self) -> &str {
        "simple"
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use in_mem_core::search_types::{DocRef, SearchStats};
    use in_mem_core::types::{Key, Namespace, RunId};

    fn make_hit(doc_ref: DocRef, score: f32, rank: u32) -> SearchHit {
        SearchHit {
            doc_ref,
            score,
            rank,
            snippet: None,
        }
    }

    fn make_response(hits: Vec<SearchHit>) -> SearchResponse {
        SearchResponse {
            hits,
            truncated: false,
            stats: SearchStats::new(0, 0),
        }
    }

    fn test_key() -> Key {
        let run_id = RunId::new();
        let ns = Namespace::for_run(run_id);
        Key::new_kv(ns, "test")
    }

    #[test]
    fn test_simple_fuser_empty() {
        let fuser = SimpleFuser::new();
        let result = fuser.fuse(vec![], 10);
        assert!(result.hits.is_empty());
        assert!(!result.truncated);
    }

    #[test]
    fn test_simple_fuser_single_primitive() {
        let fuser = SimpleFuser::new();

        let key = test_key();
        let hits = vec![
            make_hit(DocRef::Kv { key: key.clone() }, 0.8, 1),
            make_hit(DocRef::Kv { key: key.clone() }, 0.5, 2),
        ];
        let results = vec![(PrimitiveKind::Kv, make_response(hits))];

        let result = fuser.fuse(results, 10);
        assert_eq!(result.hits.len(), 2);
        assert_eq!(result.hits[0].rank, 1);
        assert_eq!(result.hits[1].rank, 2);
        assert!(result.hits[0].score >= result.hits[1].score);
    }

    #[test]
    fn test_simple_fuser_multiple_primitives() {
        let fuser = SimpleFuser::new();

        let key = test_key();
        let run_id = RunId::new();

        let kv_hits = vec![make_hit(DocRef::Kv { key: key.clone() }, 0.7, 1)];
        let run_hits = vec![make_hit(DocRef::Run { run_id }, 0.9, 1)];

        let results = vec![
            (PrimitiveKind::Kv, make_response(kv_hits)),
            (PrimitiveKind::Run, make_response(run_hits)),
        ];

        let result = fuser.fuse(results, 10);
        assert_eq!(result.hits.len(), 2);
        // Higher score should be first
        assert!(result.hits[0].score > result.hits[1].score);
        // Ranks should be updated
        assert_eq!(result.hits[0].rank, 1);
        assert_eq!(result.hits[1].rank, 2);
    }

    #[test]
    fn test_simple_fuser_respects_k() {
        let fuser = SimpleFuser::new();

        let key = test_key();
        let hits: Vec<_> = (0..10)
            .map(|i| make_hit(DocRef::Kv { key: key.clone() }, 1.0 - i as f32 * 0.1, i + 1))
            .collect();

        let results = vec![(PrimitiveKind::Kv, make_response(hits))];

        let result = fuser.fuse(results, 3);
        assert_eq!(result.hits.len(), 3);
        assert!(result.truncated);
    }

    #[test]
    fn test_simple_fuser_name() {
        let fuser = SimpleFuser::new();
        assert_eq!(fuser.name(), "simple");
    }

    #[test]
    fn test_fuser_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SimpleFuser>();
    }
}
