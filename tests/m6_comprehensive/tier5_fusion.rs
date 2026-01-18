//! Tier 5: Fusion Correctness
//!
//! Tests for RRF and SimpleFuser correctness.

use in_mem_core::search_types::{DocRef, PrimitiveKind, SearchHit, SearchResponse, SearchStats};
use in_mem_core::types::{Key, Namespace, RunId};
use in_mem_search::{Fuser, RRFFuser, SimpleFuser};

// ============================================================================
// Test Helpers
// ============================================================================

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

fn make_kv_key(name: &str) -> Key {
    let run_id = RunId::new();
    let ns = Namespace::for_run(run_id);
    Key::new_kv(ns, name)
}

// ============================================================================
// SimpleFuser Tests
// ============================================================================

/// SimpleFuser handles empty input
#[test]
fn test_tier5_simple_fuser_empty() {
    let fuser = SimpleFuser::new();
    let result = fuser.fuse(vec![], 10);
    assert!(result.hits.is_empty());
    assert!(!result.truncated);
}

/// SimpleFuser merges single primitive
#[test]
fn test_tier5_simple_fuser_single() {
    let fuser = SimpleFuser::new();

    let key = make_kv_key("test");
    let hits = vec![
        make_hit(DocRef::Kv { key: key.clone() }, 0.8, 1),
        make_hit(DocRef::Kv { key: key.clone() }, 0.5, 2),
    ];
    let results = vec![(PrimitiveKind::Kv, make_response(hits))];

    let result = fuser.fuse(results, 10);
    assert_eq!(result.hits.len(), 2);
    assert_eq!(result.hits[0].rank, 1);
    assert_eq!(result.hits[1].rank, 2);
}

/// SimpleFuser sorts by score descending
#[test]
fn test_tier5_simple_fuser_sorts_by_score() {
    let fuser = SimpleFuser::new();

    let key_a = make_kv_key("a");
    let _key_b = make_kv_key("b");
    let run_id = RunId::new();

    let kv_hits = vec![make_hit(DocRef::Kv { key: key_a.clone() }, 0.7, 1)];
    let run_hits = vec![make_hit(DocRef::Run { run_id }, 0.9, 1)];

    let results = vec![
        (PrimitiveKind::Kv, make_response(kv_hits)),
        (PrimitiveKind::Run, make_response(run_hits)),
    ];

    let result = fuser.fuse(results, 10);
    assert_eq!(result.hits.len(), 2);
    // Higher score should be first
    assert!(result.hits[0].score > result.hits[1].score);
}

/// SimpleFuser respects k limit
#[test]
fn test_tier5_simple_fuser_respects_k() {
    let fuser = SimpleFuser::new();

    let key = make_kv_key("test");
    let hits: Vec<_> = (0..10)
        .map(|i| make_hit(DocRef::Kv { key: key.clone() }, 1.0 - i as f32 * 0.1, i + 1))
        .collect();

    let results = vec![(PrimitiveKind::Kv, make_response(hits))];

    let result = fuser.fuse(results, 3);
    assert_eq!(result.hits.len(), 3);
    assert!(result.truncated);
}

/// SimpleFuser has correct name
#[test]
fn test_tier5_simple_fuser_name() {
    let fuser = SimpleFuser::new();
    assert_eq!(fuser.name(), "simple");
}

// ============================================================================
// RRFFuser Tests
// ============================================================================

/// RRFFuser handles empty input
#[test]
fn test_tier5_rrf_fuser_empty() {
    let fuser = RRFFuser::default();
    let result = fuser.fuse(vec![], 10);
    assert!(result.hits.is_empty());
    assert!(!result.truncated);
}

/// RRFFuser handles single list
#[test]
fn test_tier5_rrf_fuser_single() {
    let fuser = RRFFuser::default();

    let key_a = make_kv_key("a");
    let key_b = make_kv_key("b");

    let hits = vec![
        make_hit(DocRef::Kv { key: key_a.clone() }, 0.9, 1),
        make_hit(DocRef::Kv { key: key_b.clone() }, 0.8, 2),
    ];
    let results = vec![(PrimitiveKind::Kv, make_response(hits))];

    let result = fuser.fuse(results, 10);
    assert_eq!(result.hits.len(), 2);
    // RRF scores: 1/(60+1)=0.0164, 1/(60+2)=0.0161
    assert!(result.hits[0].score > result.hits[1].score);
}

/// RRFFuser deduplicates across lists
#[test]
fn test_tier5_rrf_fuser_deduplication() {
    let fuser = RRFFuser::default();

    let key_a = make_kv_key("shared");

    // Same DocRef in both lists
    let list1_hits = vec![make_hit(DocRef::Kv { key: key_a.clone() }, 0.9, 1)];
    let list2_hits = vec![make_hit(DocRef::Kv { key: key_a.clone() }, 0.8, 1)];

    let results = vec![
        (PrimitiveKind::Kv, make_response(list1_hits)),
        (PrimitiveKind::Json, make_response(list2_hits)),
    ];

    let result = fuser.fuse(results, 10);

    // Should only have one hit (deduplicated)
    assert_eq!(result.hits.len(), 1);

    // RRF score should be sum: 1/(60+1) + 1/(60+1) = 2 * 0.0164 = 0.0328
    let expected_rrf = 2.0 / 61.0;
    assert!((result.hits[0].score - expected_rrf).abs() < 0.0001);
}

/// Documents in multiple lists rank higher
#[test]
fn test_tier5_rrf_multi_list_boost() {
    let fuser = RRFFuser::default();

    let key_shared = make_kv_key("shared");
    let key_only1 = make_kv_key("only1");
    let key_only2 = make_kv_key("only2");

    let list1 = vec![
        make_hit(
            DocRef::Kv {
                key: key_shared.clone(),
            },
            0.9,
            1,
        ),
        make_hit(
            DocRef::Kv {
                key: key_only1.clone(),
            },
            0.8,
            2,
        ),
    ];
    let list2 = vec![
        make_hit(
            DocRef::Kv {
                key: key_only2.clone(),
            },
            0.9,
            1,
        ),
        make_hit(
            DocRef::Kv {
                key: key_shared.clone(),
            },
            0.7,
            2,
        ),
    ];

    let results = vec![
        (PrimitiveKind::Kv, make_response(list1)),
        (PrimitiveKind::Json, make_response(list2)),
    ];

    let result = fuser.fuse(results, 10);

    // Shared doc should be first (appears in both lists)
    assert_eq!(
        result.hits[0].doc_ref,
        DocRef::Kv {
            key: key_shared.clone()
        }
    );
}

/// RRFFuser respects k limit
#[test]
fn test_tier5_rrf_fuser_respects_k() {
    let fuser = RRFFuser::default();

    let hits: Vec<_> = (0..10)
        .map(|i| {
            let key = make_kv_key(&format!("key{}", i));
            make_hit(DocRef::Kv { key }, 1.0 - i as f32 * 0.1, (i + 1) as u32)
        })
        .collect();

    let results = vec![(PrimitiveKind::Kv, make_response(hits))];

    let result = fuser.fuse(results, 3);
    assert_eq!(result.hits.len(), 3);
    assert!(result.truncated);
}

/// RRFFuser is deterministic
#[test]
fn test_tier5_rrf_fuser_deterministic() {
    let fuser = RRFFuser::default();

    let key_a = make_kv_key("det_a");
    let key_b = make_kv_key("det_b");
    let key_c = make_kv_key("det_c");

    let make_results = || {
        vec![
            (
                PrimitiveKind::Kv,
                make_response(vec![
                    make_hit(DocRef::Kv { key: key_a.clone() }, 0.9, 1),
                    make_hit(DocRef::Kv { key: key_b.clone() }, 0.8, 2),
                ]),
            ),
            (
                PrimitiveKind::Json,
                make_response(vec![
                    make_hit(DocRef::Kv { key: key_c.clone() }, 0.9, 1),
                    make_hit(DocRef::Kv { key: key_b.clone() }, 0.7, 2),
                ]),
            ),
        ]
    };

    let result1 = fuser.fuse(make_results(), 10);
    let result2 = fuser.fuse(make_results(), 10);

    assert_eq!(result1.hits.len(), result2.hits.len());
    for (h1, h2) in result1.hits.iter().zip(result2.hits.iter()) {
        assert_eq!(h1.doc_ref, h2.doc_ref);
        assert_eq!(h1.rank, h2.rank);
        assert!((h1.score - h2.score).abs() < 0.0001);
    }
}

/// RRFFuser custom k parameter works
#[test]
fn test_tier5_rrf_custom_k() {
    let fuser = RRFFuser::new(10);
    assert_eq!(fuser.k_rrf(), 10);

    let key = make_kv_key("custom");
    let hits = vec![make_hit(DocRef::Kv { key: key.clone() }, 0.9, 1)];
    let results = vec![(PrimitiveKind::Kv, make_response(hits))];

    let result = fuser.fuse(results, 10);

    // With k=10, score should be 1/(10+1) = 0.0909
    let expected = 1.0 / 11.0;
    assert!((result.hits[0].score - expected).abs() < 0.0001);
}

/// RRFFuser has correct name
#[test]
fn test_tier5_rrf_fuser_name() {
    let fuser = RRFFuser::default();
    assert_eq!(fuser.name(), "rrf");
}

// ============================================================================
// Fuser Trait Tests
// ============================================================================

/// Fusers are Send + Sync
#[test]
fn test_tier5_fusers_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<SimpleFuser>();
    assert_send_sync::<RRFFuser>();
}
