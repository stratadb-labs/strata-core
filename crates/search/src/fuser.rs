//! Fusion infrastructure for combining search results
//!
//! This module provides:
//! - Fuser trait for pluggable fusion algorithms
//! - SimpleFuser: basic concatenation + sort (M6 default)
//! - RRFFuser: Reciprocal Rank Fusion (Epic 37)
//!
//! See `docs/architecture/M6_ARCHITECTURE.md` for authoritative specification.

use strata_core::search_types::{DocRef, SearchHit, SearchResponse};
use strata_core::PrimitiveType;

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
    fn fuse(&self, results: Vec<(PrimitiveType, SearchResponse)>, k: usize) -> FusedResult;

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
    fn fuse(&self, results: Vec<(PrimitiveType, SearchResponse)>, k: usize) -> FusedResult {
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
// RRFFuser (Epic 37)
// ============================================================================

/// Reciprocal Rank Fusion (RRF)
///
/// RRF Score = sum(1 / (k + rank)) across all lists
/// Where k is a smoothing constant (default 60).
///
/// This fuser is better than SimpleFuser when combining results from
/// different ranking algorithms (e.g., keyword + vector search).
///
/// # Algorithm
///
/// For each document appearing in any list:
/// - Calculate RRF contribution: 1 / (k_rrf + rank)
/// - Sum contributions across all lists
/// - Higher RRF score = higher final rank
///
/// # Example
///
/// ```text
/// Given:
///   - List A: [doc1@rank1, doc2@rank2, doc3@rank3]
///   - List B: [doc2@rank1, doc4@rank2, doc1@rank3]
///   - k_rrf = 60
///
/// RRF scores:
///   doc1: 1/(60+1) + 1/(60+3) = 0.0164 + 0.0159 = 0.0323
///   doc2: 1/(60+2) + 1/(60+1) = 0.0161 + 0.0164 = 0.0325  <- highest
///   doc3: 1/(60+3) = 0.0159
///   doc4: 1/(60+2) = 0.0161
///
/// Final ranking: [doc2, doc1, doc4, doc3]
/// ```
#[derive(Debug, Clone)]
pub struct RRFFuser {
    /// Smoothing constant (default 60)
    k_rrf: u32,
}

impl Default for RRFFuser {
    fn default() -> Self {
        RRFFuser { k_rrf: 60 }
    }
}

impl RRFFuser {
    /// Create a new RRFFuser with custom k value
    pub fn new(k_rrf: u32) -> Self {
        RRFFuser { k_rrf }
    }

    /// Get the k parameter
    pub fn k_rrf(&self) -> u32 {
        self.k_rrf
    }
}

impl Fuser for RRFFuser {
    fn fuse(&self, results: Vec<(PrimitiveType, SearchResponse)>, k: usize) -> FusedResult {
        use std::collections::hash_map::DefaultHasher;
        use std::collections::HashMap;
        use std::hash::{Hash, Hasher};

        let mut rrf_scores: HashMap<DocRef, f32> = HashMap::new();
        let mut hit_data: HashMap<DocRef, SearchHit> = HashMap::new();

        for (_primitive, response) in results {
            for hit in response.hits {
                // RRF contribution: 1 / (k + rank)
                let rrf_contribution = 1.0 / (self.k_rrf as f32 + hit.rank as f32);
                *rrf_scores.entry(hit.doc_ref.clone()).or_insert(0.0) += rrf_contribution;

                // Keep first occurrence of hit data (for snippet, etc.)
                hit_data.entry(hit.doc_ref.clone()).or_insert(hit);
            }
        }

        // Sort by RRF score with deterministic tie-breaking
        let mut scored: Vec<_> = rrf_scores.into_iter().collect();
        scored.sort_by(|a, b| {
            // Primary: RRF score (descending)
            match b.1.partial_cmp(&a.1) {
                Some(std::cmp::Ordering::Equal) | None => {
                    // Tie-breaker 1: original score from first occurrence (descending)
                    let orig_a = hit_data.get(&a.0).map(|h| h.score).unwrap_or(0.0);
                    let orig_b = hit_data.get(&b.0).map(|h| h.score).unwrap_or(0.0);
                    match orig_b.partial_cmp(&orig_a) {
                        Some(std::cmp::Ordering::Equal) | None => {
                            // Tie-breaker 2: DocRef hash (stable ordering)
                            let hash_a = {
                                let mut hasher = DefaultHasher::new();
                                a.0.hash(&mut hasher);
                                hasher.finish()
                            };
                            let hash_b = {
                                let mut hasher = DefaultHasher::new();
                                b.0.hash(&mut hasher);
                                hasher.finish()
                            };
                            hash_a.cmp(&hash_b)
                        }
                        Some(ord) => ord,
                    }
                }
                Some(ord) => ord,
            }
        });

        // Build final ranked list
        let truncated = scored.len() > k;
        let hits: Vec<SearchHit> = scored
            .into_iter()
            .take(k)
            .enumerate()
            .map(|(i, (doc_ref, rrf_score))| {
                let mut hit = hit_data.remove(&doc_ref).unwrap();
                hit.score = rrf_score;
                hit.rank = (i + 1) as u32;
                hit
            })
            .collect();

        FusedResult::new(hits, truncated)
    }

    fn name(&self) -> &str {
        "rrf"
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use strata_core::search_types::{DocRef, SearchStats};
    use strata_core::types::RunId;

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

    /// Helper to create a KV DocRef
    fn make_kv_doc_ref(run_id: &RunId, key: &str) -> DocRef {
        DocRef::Kv {
            run_id: run_id.clone(),
            key: key.to_string(),
        }
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

        let run_id = RunId::new();
        let doc_ref = make_kv_doc_ref(&run_id, "test");
        let hits = vec![
            make_hit(doc_ref.clone(), 0.8, 1),
            make_hit(doc_ref.clone(), 0.5, 2),
        ];
        let results = vec![(PrimitiveType::Kv, make_response(hits))];

        let result = fuser.fuse(results, 10);
        assert_eq!(result.hits.len(), 2);
        assert_eq!(result.hits[0].rank, 1);
        assert_eq!(result.hits[1].rank, 2);
        assert!(result.hits[0].score >= result.hits[1].score);
    }

    #[test]
    fn test_simple_fuser_multiple_primitives() {
        let fuser = SimpleFuser::new();

        let run_id = RunId::new();

        let kv_ref = make_kv_doc_ref(&run_id, "test");
        let run_ref = DocRef::Run { run_id: run_id.clone() };

        let kv_hits = vec![make_hit(kv_ref, 0.7, 1)];
        let run_hits = vec![make_hit(run_ref, 0.9, 1)];

        let results = vec![
            (PrimitiveType::Kv, make_response(kv_hits)),
            (PrimitiveType::Run, make_response(run_hits)),
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

        let run_id = RunId::new();
        let doc_ref = make_kv_doc_ref(&run_id, "test");
        let hits: Vec<_> = (0..10)
            .map(|i| make_hit(doc_ref.clone(), 1.0 - i as f32 * 0.1, i + 1))
            .collect();

        let results = vec![(PrimitiveType::Kv, make_response(hits))];

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
        assert_send_sync::<RRFFuser>();
    }

    // ========================================
    // RRFFuser Tests
    // ========================================

    #[test]
    fn test_rrf_fuser_empty() {
        let fuser = RRFFuser::default();
        let result = fuser.fuse(vec![], 10);
        assert!(result.hits.is_empty());
        assert!(!result.truncated);
    }

    #[test]
    fn test_rrf_fuser_single_list() {
        let fuser = RRFFuser::default();

        let run_id = RunId::new();
        let doc_ref_a = make_kv_doc_ref(&run_id, "a");
        let doc_ref_b = make_kv_doc_ref(&run_id, "b");

        let hits = vec![
            make_hit(doc_ref_a, 0.9, 1),
            make_hit(doc_ref_b, 0.8, 2),
        ];
        let results = vec![(PrimitiveType::Kv, make_response(hits))];

        let result = fuser.fuse(results, 10);
        assert_eq!(result.hits.len(), 2);
        // RRF scores: 1/(60+1)=0.0164, 1/(60+2)=0.0161
        assert!(result.hits[0].score > result.hits[1].score);
    }

    #[test]
    fn test_rrf_fuser_deduplication() {
        let fuser = RRFFuser::default();

        let run_id = RunId::new();
        let doc_ref_shared = make_kv_doc_ref(&run_id, "shared");

        // Same DocRef in both lists
        let list1_hits = vec![make_hit(doc_ref_shared.clone(), 0.9, 1)];
        let list2_hits = vec![make_hit(doc_ref_shared.clone(), 0.8, 1)];

        let results = vec![
            (PrimitiveType::Kv, make_response(list1_hits)),
            (PrimitiveType::Json, make_response(list2_hits)),
        ];

        let result = fuser.fuse(results, 10);

        // Should only have one hit (deduplicated)
        assert_eq!(result.hits.len(), 1);

        // RRF score should be sum: 1/(60+1) + 1/(60+1) = 2 * 0.0164 = 0.0328
        let expected_rrf = 2.0 / 61.0;
        assert!((result.hits[0].score - expected_rrf).abs() < 0.0001);
    }

    #[test]
    fn test_rrf_fuser_documents_in_both_lists_rank_higher() {
        let fuser = RRFFuser::default();

        let run_id = RunId::new();
        let doc_ref_a = make_kv_doc_ref(&run_id, "in_both");
        let doc_ref_b = make_kv_doc_ref(&run_id, "only_list1");
        let doc_ref_c = make_kv_doc_ref(&run_id, "only_list2");

        let list1_hits = vec![
            make_hit(doc_ref_a.clone(), 0.9, 1),
            make_hit(doc_ref_b, 0.8, 2),
        ];
        let list2_hits = vec![
            make_hit(doc_ref_c, 0.9, 1),
            make_hit(doc_ref_a.clone(), 0.7, 2),
        ];

        let results = vec![
            (PrimitiveType::Kv, make_response(list1_hits)),
            (PrimitiveType::Json, make_response(list2_hits)),
        ];

        let result = fuser.fuse(results, 10);

        // key_a appears in both lists, so it should have highest RRF score
        // key_a: 1/(60+1) + 1/(60+2) = 0.0164 + 0.0161 = 0.0325
        // key_b: 1/(60+2) = 0.0161
        // key_c: 1/(60+1) = 0.0164
        assert_eq!(result.hits.len(), 3);
        assert_eq!(result.hits[0].doc_ref, doc_ref_a);
    }

    #[test]
    fn test_rrf_fuser_respects_k() {
        let fuser = RRFFuser::default();

        let run_id = RunId::new();
        let hits: Vec<_> = (0..10)
            .map(|i| {
                let doc_ref = make_kv_doc_ref(&run_id, &format!("key{}", i));
                make_hit(doc_ref, 1.0 - i as f32 * 0.1, (i + 1) as u32)
            })
            .collect();

        let results = vec![(PrimitiveType::Kv, make_response(hits))];

        let result = fuser.fuse(results, 3);
        assert_eq!(result.hits.len(), 3);
        assert!(result.truncated);
    }

    #[test]
    fn test_rrf_fuser_determinism() {
        let fuser = RRFFuser::default();

        let run_id = RunId::new();
        let doc_ref_a = make_kv_doc_ref(&run_id, "det_a");
        let doc_ref_b = make_kv_doc_ref(&run_id, "det_b");
        let doc_ref_c = make_kv_doc_ref(&run_id, "det_c");

        let make_results = || {
            vec![
                (
                    PrimitiveType::Kv,
                    make_response(vec![
                        make_hit(doc_ref_a.clone(), 0.9, 1),
                        make_hit(doc_ref_b.clone(), 0.8, 2),
                    ]),
                ),
                (
                    PrimitiveType::Json,
                    make_response(vec![
                        make_hit(doc_ref_c.clone(), 0.9, 1),
                        make_hit(doc_ref_b.clone(), 0.7, 2),
                    ]),
                ),
            ]
        };

        let result1 = fuser.fuse(make_results(), 10);
        let result2 = fuser.fuse(make_results(), 10);

        // Same inputs should produce same output order
        assert_eq!(result1.hits.len(), result2.hits.len());
        for (h1, h2) in result1.hits.iter().zip(result2.hits.iter()) {
            assert_eq!(h1.doc_ref, h2.doc_ref);
            assert_eq!(h1.rank, h2.rank);
            assert!((h1.score - h2.score).abs() < 0.0001);
        }
    }

    #[test]
    fn test_rrf_fuser_custom_k() {
        let fuser = RRFFuser::new(10);
        assert_eq!(fuser.k_rrf(), 10);

        let run_id = RunId::new();
        let doc_ref = make_kv_doc_ref(&run_id, "custom_k");
        let hits = vec![make_hit(doc_ref, 0.9, 1)];
        let results = vec![(PrimitiveType::Kv, make_response(hits))];

        let result = fuser.fuse(results, 10);

        // With k=10, score should be 1/(10+1) = 0.0909
        let expected = 1.0 / 11.0;
        assert!((result.hits[0].score - expected).abs() < 0.0001);
    }

    #[test]
    fn test_rrf_fuser_name() {
        let fuser = RRFFuser::default();
        assert_eq!(fuser.name(), "rrf");
    }
}
