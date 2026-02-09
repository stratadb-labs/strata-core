//! Position-aware score blending for re-ranking
//!
//! Blends RRF scores with reranker relevance scores using position-aware weights:
//! - Ranks 1-3: 75% RRF + 25% reranker (trust established ranking)
//! - Ranks 4-10: 60% RRF + 40% reranker (moderate reranker influence)
//! - Ranks 11+: 40% RRF + 60% reranker (reranker dominates tail)

use super::RerankScore;
use strata_engine::search::SearchHit;

/// Blend RRF scores with reranker scores using position-aware weights.
///
/// Hits without a matching reranker score keep their normalized RRF score.
/// Results are re-sorted by blended score (descending) and ranks reassigned.
pub fn blend_scores(mut hits: Vec<SearchHit>, scores: &[RerankScore]) -> Vec<SearchHit> {
    if hits.is_empty() || scores.is_empty() {
        return hits;
    }

    // Normalize RRF scores to [0, 1]
    let max_rrf = hits
        .iter()
        .map(|h| h.score)
        .fold(f32::NEG_INFINITY, f32::max);
    let min_rrf = hits.iter().map(|h| h.score).fold(f32::INFINITY, f32::min);
    let rrf_range = max_rrf - min_rrf;

    for (pos, hit) in hits.iter_mut().enumerate() {
        let norm_rrf = if rrf_range > 0.0 {
            (hit.score - min_rrf) / rrf_range
        } else {
            1.0 // all same score → treat as 1.0
        };

        // Find matching reranker score by original index
        if let Some(rerank) = scores.iter().find(|s| s.index == pos) {
            let (w_rrf, w_rerank) = position_weights(pos);
            hit.score = w_rrf * norm_rrf + w_rerank * rerank.relevance_score;
        } else {
            hit.score = norm_rrf;
        }
    }

    // Re-sort by blended score (descending)
    hits.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Reassign ranks (1-indexed)
    for (i, hit) in hits.iter_mut().enumerate() {
        hit.rank = (i + 1) as u32;
    }

    hits
}

/// Position-aware weights: (rrf_weight, reranker_weight).
///
/// Lower-ranked results give more influence to the reranker.
fn position_weights(position: usize) -> (f32, f32) {
    match position {
        0..=2 => (0.75, 0.25), // ranks 1-3
        3..=9 => (0.60, 0.40), // ranks 4-10
        _ => (0.40, 0.60),     // ranks 11+
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strata_core::types::BranchId;
    use strata_engine::search::EntityRef;

    fn make_hit(score: f32, rank: u32) -> SearchHit {
        SearchHit {
            doc_ref: EntityRef::Kv {
                branch_id: BranchId::new(),
                key: format!("key{}", rank),
            },
            score,
            rank,
            snippet: Some(format!("snippet {}", rank)),
        }
    }

    #[test]
    fn test_blend_empty_hits() {
        let result = blend_scores(vec![], &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_blend_empty_scores() {
        let hits = vec![make_hit(1.0, 1), make_hit(0.5, 2)];
        let result = blend_scores(hits.clone(), &[]);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_blend_reorders_in_tail_tier() {
        // In the tail tier (positions 10+), reranker gets 60% weight.
        // Create enough hits so positions 10 and 11 land in the tail tier.
        let hits: Vec<SearchHit> = (0..12)
            .map(|i| make_hit(1.0 - i as f32 * 0.05, (i + 1) as u32))
            .collect();
        // Hits 10 and 11 (0-indexed) are in the tail tier
        // hit10: RRF ~0.50, hit11: RRF ~0.45
        let scores = vec![
            RerankScore {
                index: 10,
                relevance_score: 0.1,
            }, // low reranker
            RerankScore {
                index: 11,
                relevance_score: 1.0,
            }, // high reranker
        ];
        let result = blend_scores(hits, &scores);
        // hit11 should outrank hit10 due to high reranker score in tail tier
        let pos_hit11 = result
            .iter()
            .position(|h| h.snippet.as_deref() == Some("snippet 12"))
            .unwrap();
        let pos_hit10 = result
            .iter()
            .position(|h| h.snippet.as_deref() == Some("snippet 11"))
            .unwrap();
        assert!(
            pos_hit11 < pos_hit10,
            "High reranker score should boost hit11 above hit10"
        );
    }

    #[test]
    fn test_blend_all_same_rrf_score() {
        let hits = vec![make_hit(0.5, 1), make_hit(0.5, 2), make_hit(0.5, 3)];
        let scores = vec![
            RerankScore {
                index: 0,
                relevance_score: 0.3,
            },
            RerankScore {
                index: 1,
                relevance_score: 0.9,
            },
            RerankScore {
                index: 2,
                relevance_score: 0.1,
            },
        ];
        let result = blend_scores(hits, &scores);
        // With same RRF, reranker scores dominate
        assert_eq!(result[0].snippet.as_deref(), Some("snippet 2"));
        assert_eq!(result[1].snippet.as_deref(), Some("snippet 1"));
        assert_eq!(result[2].snippet.as_deref(), Some("snippet 3"));
    }

    #[test]
    fn test_blend_single_hit() {
        let hits = vec![make_hit(1.0, 1)];
        let scores = vec![RerankScore {
            index: 0,
            relevance_score: 0.5,
        }];
        let result = blend_scores(hits, &scores);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].rank, 1);
    }

    #[test]
    fn test_blend_partial_scores() {
        // Only some hits have reranker scores
        let hits = vec![make_hit(1.0, 1), make_hit(0.8, 2), make_hit(0.6, 3)];
        let scores = vec![
            RerankScore {
                index: 0,
                relevance_score: 0.2,
            },
            // index 1 has no reranker score
            RerankScore {
                index: 2,
                relevance_score: 0.9,
            },
        ];
        let result = blend_scores(hits, &scores);
        assert_eq!(result.len(), 3);
        // All ranks should be assigned
        let ranks: Vec<u32> = result.iter().map(|h| h.rank).collect();
        assert_eq!(ranks, vec![1, 2, 3]);
    }

    #[test]
    fn test_blend_deterministic_sort_on_ties() {
        // When blended scores are equal, stable sort preserves original order
        let hits = vec![make_hit(0.8, 1), make_hit(0.6, 2), make_hit(0.4, 3)];
        // Give all three the same reranker score → blended scores differ
        // only by RRF, so ordering should match original RRF order
        let scores = vec![
            RerankScore {
                index: 0,
                relevance_score: 0.5,
            },
            RerankScore {
                index: 1,
                relevance_score: 0.5,
            },
            RerankScore {
                index: 2,
                relevance_score: 0.5,
            },
        ];
        let result = blend_scores(hits, &scores);
        assert_eq!(result[0].snippet.as_deref(), Some("snippet 1"));
        assert_eq!(result[1].snippet.as_deref(), Some("snippet 2"));
        assert_eq!(result[2].snippet.as_deref(), Some("snippet 3"));
    }

    #[test]
    fn test_position_weights_tiers() {
        // Tier 1: ranks 1-3 (positions 0-2)
        assert_eq!(position_weights(0), (0.75, 0.25));
        assert_eq!(position_weights(2), (0.75, 0.25));

        // Tier 2: ranks 4-10 (positions 3-9)
        assert_eq!(position_weights(3), (0.60, 0.40));
        assert_eq!(position_weights(9), (0.60, 0.40));

        // Tier 3: ranks 11+ (positions 10+)
        assert_eq!(position_weights(10), (0.40, 0.60));
        assert_eq!(position_weights(100), (0.40, 0.60));
    }
}
