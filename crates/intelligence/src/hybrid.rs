//! Composite search orchestrator for M6
//!
//! This module provides:
//! - HybridSearch struct that orchestrates searches across primitives
//! - Primitive selection based on filters
//! - Budget allocation across primitives
//! - Search orchestration with consistent snapshot
//!
//! See `docs/architecture/M6_ARCHITECTURE.md` for authoritative specification.
//!
//! # Architectural Rules
//!
//! - Rule 3: Composite Orchestrates, Doesn't Replace
//! - Rule 4: Snapshot-Consistent Search
//!
//! HybridSearch is STATELESS. It holds only references to Database and primitives.

use crate::fuser::{Fuser, RRFFuser, SimpleFuser};
use std::sync::Arc;
use std::time::Instant;
use strata_core::PrimitiveType;
use strata_core::StrataResult;
use strata_engine::search::{SearchBudget, SearchMode, SearchRequest, SearchResponse, SearchStats};
#[cfg(feature = "embed")]
use strata_engine::search::SearchHit;
use strata_engine::Database;
use strata_engine::{BranchIndex, EventLog, JsonStore, KVStore, StateCell, VectorStore};

// Shadow vector collection names (duplicated from executor::embed_hook to avoid
// cross-crate dependency from intelligence → executor).
#[cfg(feature = "embed")]
const SHADOW_KV: &str = "_system_embed_kv";
#[cfg(feature = "embed")]
const SHADOW_JSON: &str = "_system_embed_json";
#[cfg(feature = "embed")]
const SHADOW_EVENT: &str = "_system_embed_event";
#[cfg(feature = "embed")]
const SHADOW_STATE: &str = "_system_embed_state";

// ============================================================================
// HybridSearch
// ============================================================================

/// Composite search orchestrator
///
/// HybridSearch coordinates searches across multiple primitives
/// and fuses results into a single ranked list.
///
/// # Architecture
///
/// ```text
/// SearchRequest
///      │
///      ▼
/// ┌─────────────────────────────────────────┐
/// │            HybridSearch                  │
/// │  ┌────────────┐  ┌────────────────────┐ │
/// │  │select_prims│──│allocate_budgets    │ │
/// │  └────────────┘  └────────────────────┘ │
/// │                                          │
/// │  ┌────────────────────────────────────┐ │
/// │  │     Search Each Primitive          │ │
/// │  │  ┌───┐ ┌────┐ ┌─────┐ ┌─────┐     │ │
/// │  │  │KV │ │JSON│ │Event│ │State│ ... │ │
/// │  │  └─┬─┘ └──┬─┘ └──┬──┘ └──┬──┘     │ │
/// │  └────┼──────┼──────┼───────┼────────┘ │
/// │       └──────┴──────┴───────┘          │
/// │                │                        │
/// │         ┌──────┴──────┐                 │
/// │         │   Fuser     │                 │
/// │         └──────┬──────┘                 │
/// └────────────────┼────────────────────────┘
///                  │
///                  ▼
///           SearchResponse
/// ```
///
/// # Stateless Design
///
/// CRITICAL: HybridSearch is STATELESS. It holds only Arc references.
/// All search state is ephemeral per-request.
#[derive(Clone)]
pub struct HybridSearch {
    /// Database reference (kept for future snapshot consistency)
    #[allow(dead_code)]
    db: Arc<Database>,
    /// Fuser for combining results
    fuser: Arc<dyn Fuser>,
    /// All primitive facades
    kv: KVStore,
    json: JsonStore,
    event: EventLog,
    state: StateCell,
    branch_index: BranchIndex,
    vector: VectorStore,
}

impl HybridSearch {
    /// Create a new HybridSearch orchestrator
    ///
    /// Creates all primitive facades internally.
    /// Uses SimpleFuser by default.
    pub fn new(db: Arc<Database>) -> Self {
        HybridSearch {
            kv: KVStore::new(db.clone()),
            json: JsonStore::new(db.clone()),
            event: EventLog::new(db.clone()),
            state: StateCell::new(db.clone()),
            branch_index: BranchIndex::new(db.clone()),
            vector: VectorStore::new(db.clone()),
            db,
            fuser: Arc::new(SimpleFuser),
        }
    }

    /// Builder: set custom fuser
    pub fn with_fuser(mut self, fuser: Arc<dyn Fuser>) -> Self {
        self.fuser = fuser;
        self
    }

    // ========================================================================
    // Search Orchestration
    // ========================================================================

    /// Search across all (or filtered) primitives
    ///
    /// # Flow
    ///
    /// 1. Select primitives based on filter
    /// 2. Allocate budget across primitives
    /// 3. Execute searches (respecting budget)
    /// 4. Fuse results
    /// 5. Return combined response
    ///
    /// # Snapshot Consistency
    ///
    /// Per Rule 4: Each primitive's search() uses its own snapshot.
    /// For true cross-primitive consistency, primitives would need
    /// search_with_snapshot() methods. This is acceptable for M6.
    pub fn search(&self, req: &SearchRequest) -> StrataResult<SearchResponse> {
        let start = Instant::now();

        // 1. Select primitives
        let primitives = self.select_primitives(req);

        if primitives.is_empty() {
            return Ok(SearchResponse {
                hits: vec![],
                truncated: false,
                stats: SearchStats::new(start.elapsed().as_micros() as u64, 0),
            });
        }

        // 2. Allocate budgets
        let budgets = self.allocate_budgets(req, primitives.len());

        // 3. Execute searches
        let mut primitive_results = Vec::new();
        let mut total_candidates = 0;
        let mut any_truncated = false;

        for (primitive, budget) in primitives.iter().zip(budgets.iter()) {
            // In Hybrid mode, skip the Vector primitive in the BM25 loop —
            // vector search is handled separately in step 4 via shadow collections.
            if req.mode == SearchMode::Hybrid && *primitive == PrimitiveType::Vector {
                continue;
            }

            // Check overall time budget
            if start.elapsed().as_micros() as u64 >= req.budget.max_wall_time_micros {
                any_truncated = true;
                break;
            }

            // Create sub-request with allocated budget
            let sub_req = req.clone().with_budget(*budget);

            // Execute search on this primitive
            let result = self.search_primitive(*primitive, &sub_req)?;

            total_candidates += result.stats.candidates_considered;
            if result.truncated {
                any_truncated = true;
            }

            primitive_results.push((*primitive, result));
        }

        // 4. Vector search for Hybrid mode
        #[cfg(feature = "embed")]
        if req.mode == SearchMode::Hybrid {
            if let Some(query_embedding) =
                crate::embed::embed_query(&self.db, &req.query)
            {
                let shadow_collections = [SHADOW_KV, SHADOW_JSON, SHADOW_EVENT, SHADOW_STATE];
                let mut vector_hits: Vec<SearchHit> = Vec::new();

                for collection in &shadow_collections {
                    let matches = if let Some((start, end)) = req.time_range {
                        self.vector.system_search_with_sources_in_range(
                            req.branch_id,
                            collection,
                            &query_embedding,
                            req.k,
                            start,
                            end,
                        )
                    } else {
                        self.vector.system_search_with_sources(
                            req.branch_id,
                            collection,
                            &query_embedding,
                            req.k,
                        )
                    };

                    if let Ok(results) = matches {
                        for m in results {
                            if let Some(source_ref) = m.source_ref {
                                vector_hits.push(SearchHit::new(
                                    source_ref,
                                    m.score,
                                    0, // placeholder — re-assigned after global sort
                                ));
                            }
                        }
                    }
                    // Silently skip collections that don't exist yet
                }

                if !vector_hits.is_empty() {
                    // Sort by score descending so RRF ranks reflect global relevance,
                    // not the arbitrary shadow-collection iteration order.
                    vector_hits.sort_by(|a, b| {
                        b.score
                            .partial_cmp(&a.score)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                    for (i, hit) in vector_hits.iter_mut().enumerate() {
                        hit.rank = (i + 1) as u32;
                    }

                    total_candidates += vector_hits.len();
                    let vector_response = SearchResponse::new(
                        vector_hits,
                        false,
                        SearchStats::new(0, 0),
                    );
                    primitive_results.push((PrimitiveType::Vector, vector_response));
                }
            }
        }

        // 5. Fuse results — use RRF for Hybrid mode to properly merge keyword + vector
        let fuser: &dyn Fuser = if req.mode == SearchMode::Hybrid {
            &RRFFuser::default()
        } else {
            self.fuser.as_ref()
        };
        let fused = fuser.fuse(primitive_results, req.k);

        // 6. Build stats
        let stats = SearchStats::new(start.elapsed().as_micros() as u64, total_candidates);

        Ok(SearchResponse {
            hits: fused.hits,
            truncated: any_truncated || fused.truncated,
            stats,
        })
    }

    // ========================================================================
    // Primitive Selection
    // ========================================================================

    /// Select which primitives to search based on request filters
    fn select_primitives(&self, req: &SearchRequest) -> Vec<PrimitiveType> {
        match &req.primitive_filter {
            Some(filter) => filter.clone(),
            None => PrimitiveType::all().to_vec(),
        }
    }

    // ========================================================================
    // Budget Allocation
    // ========================================================================

    /// Allocate budget across primitives
    ///
    /// Simple proportional allocation: divide time evenly.
    /// Future: could weight by primitive "importance" or size.
    fn allocate_budgets(&self, req: &SearchRequest, num_primitives: usize) -> Vec<SearchBudget> {
        if num_primitives == 0 {
            return vec![];
        }

        let per_primitive_time = req.budget.max_wall_time_micros / num_primitives as u64;
        let per_primitive_candidates = req.budget.max_candidates_per_primitive;

        vec![
            SearchBudget {
                max_wall_time_micros: per_primitive_time,
                max_candidates: per_primitive_candidates,
                max_candidates_per_primitive: per_primitive_candidates,
            };
            num_primitives
        ]
    }

    // ========================================================================
    // Per-Primitive Search
    // ========================================================================

    /// Execute search on a single primitive
    fn search_primitive(
        &self,
        primitive: PrimitiveType,
        req: &SearchRequest,
    ) -> StrataResult<SearchResponse> {
        use strata_engine::Searchable;

        match primitive {
            PrimitiveType::Kv => self.kv.search(req),
            PrimitiveType::Json => self.json.search(req),
            PrimitiveType::Event => self.event.search(req),
            PrimitiveType::State => self.state.search(req),
            PrimitiveType::Branch => self.branch_index.search(req),
            // Vector primitive now implements Searchable.
            // Per M8_ARCHITECTURE.md Section 12.3:
            // - Keyword search returns empty (by design)
            // - For vector/hybrid search with embeddings, the orchestrator
            //   should call vector.search_response() directly with the embedding
            PrimitiveType::Vector => Searchable::search(&self.vector, req),
        }
    }

    /// Search with query expansion: run multiple queries and fuse with weighted RRF.
    ///
    /// Takes the original query plus expanded variants. Each expanded query runs
    /// through the full HybridSearch pipeline. Results are fused with weighted RRF
    /// where the original query gets `original_weight` and expansions get 1.0.
    ///
    /// # Search mode by query type
    ///
    /// - `Lex` expansions: Keyword mode (BM25 only)
    /// - `Vec` expansions: Hybrid mode (BM25 + vector)
    /// - `Hyde` expansions: Hybrid mode (embed the hypothetical text)
    pub fn search_expanded(
        &self,
        req: &SearchRequest,
        expansions: &[crate::expand::ExpandedQuery],
        original_weight: f32,
    ) -> StrataResult<SearchResponse> {
        use crate::expand::QueryType;
        use crate::fuser::weighted_rrf_fuse;

        let start = Instant::now();
        let mut result_lists: Vec<(SearchResponse, f32)> = Vec::new();

        // Pass 0: original query with Hybrid mode and original_weight
        let original_req = req.clone().with_mode(SearchMode::Hybrid);
        let original_response = self.search(&original_req)?;
        result_lists.push((original_response, original_weight));

        // Expansion passes
        for expansion in expansions {
            let mode = match expansion.query_type {
                QueryType::Lex => SearchMode::Keyword,
                QueryType::Vec | QueryType::Hyde => SearchMode::Hybrid,
            };

            let mut exp_req = SearchRequest::new(req.branch_id, &expansion.text)
                .with_k(req.k)
                .with_mode(mode)
                .with_budget(req.budget.clone());

            if let Some(ref filter) = req.primitive_filter {
                exp_req = exp_req.with_primitive_filter(filter.clone());
            }
            if let Some((start, end)) = req.time_range {
                exp_req = exp_req.with_time_range(start, end);
            }

            match self.search(&exp_req) {
                Ok(response) => result_lists.push((response, 1.0)),
                Err(e) => {
                    tracing::warn!(
                        target: "strata::search",
                        error = %e,
                        expansion = %expansion.text,
                        "Expansion search failed, skipping"
                    );
                }
            }
        }

        // Fuse all results with weighted RRF
        let fused = weighted_rrf_fuse(result_lists, 60, req.k);

        let stats = strata_engine::search::SearchStats::new(
            start.elapsed().as_micros() as u64,
            0, // total candidates not tracked across passes
        );

        Ok(SearchResponse {
            hits: fused.hits,
            truncated: fused.truncated,
            stats,
        })
    }

    /// Get a reference to the VectorStore for direct semantic search
    ///
    /// Use this when you have an embedding vector and want to perform
    /// semantic search. The Searchable::search() method returns empty
    /// for keyword queries because Vector requires explicit embeddings.
    pub fn vector(&self) -> &VectorStore {
        &self.vector
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use strata_core::types::BranchId;
    use strata_core::value::Value;

    fn test_db() -> Arc<Database> {
        Database::cache().expect("Failed to create test database")
    }

    #[test]
    fn test_hybrid_search_new() {
        let db = test_db();
        let _hybrid = HybridSearch::new(db);
        // Should compile and not panic - primitives share the same database
    }

    #[test]
    fn test_hybrid_search_empty() {
        let db = test_db();
        let hybrid = HybridSearch::new(db);
        let branch_id = BranchId::new();

        let req = SearchRequest::new(branch_id, "test");
        let response = hybrid.search(&req).unwrap();

        assert!(response.hits.is_empty());
        assert!(!response.truncated);
    }

    #[test]
    fn test_hybrid_search_kv_only() {
        let db = test_db();
        let kv = KVStore::new(db.clone());
        let branch_id = BranchId::new();

        // Add test data
        kv.put(
            &branch_id,
            "default",
            "hello",
            Value::String("world test data".into()),
        )
        .unwrap();
        kv.put(
            &branch_id,
            "default",
            "test",
            Value::String("this is a test".into()),
        )
        .unwrap();

        let hybrid = HybridSearch::new(db);
        let req =
            SearchRequest::new(branch_id, "test").with_primitive_filter(vec![PrimitiveType::Kv]);
        let response = hybrid.search(&req).unwrap();

        // Note: MVP simplification - primitives' Searchable implementations return empty results.
        // Full-text search should use InvertedIndex directly. This test verifies the orchestration
        // works correctly even when primitives return empty results.
        assert!(response.hits.is_empty());
        assert!(!response.truncated);
    }

    #[test]
    fn test_hybrid_search_primitive_filter() {
        let db = test_db();
        let hybrid = HybridSearch::new(db);
        let branch_id = BranchId::new();

        // Test with filter
        let req_filtered = SearchRequest::new(branch_id, "test")
            .with_primitive_filter(vec![PrimitiveType::Kv, PrimitiveType::Json]);

        let primitives = hybrid.select_primitives(&req_filtered);
        assert_eq!(primitives.len(), 2);
        assert!(primitives.contains(&PrimitiveType::Kv));
        assert!(primitives.contains(&PrimitiveType::Json));

        // Test without filter (all primitives)
        let req_all = SearchRequest::new(branch_id, "test");
        let all_primitives = hybrid.select_primitives(&req_all);
        assert_eq!(all_primitives.len(), 6); // Kv, Event, State, Branch, Json, Vector
    }

    #[test]
    fn test_hybrid_search_budget_allocation() {
        let db = test_db();
        let hybrid = HybridSearch::new(db);
        let branch_id = BranchId::new();

        let req = SearchRequest::new(branch_id, "test");

        // Allocate for 3 primitives
        let budgets = hybrid.allocate_budgets(&req, 3);
        assert_eq!(budgets.len(), 3);

        // Each should get ~1/3 of time budget
        let expected_time = req.budget.max_wall_time_micros / 3;
        for budget in &budgets {
            assert_eq!(budget.max_wall_time_micros, expected_time);
        }

        // Edge case: 0 primitives
        let empty_budgets = hybrid.allocate_budgets(&req, 0);
        assert!(empty_budgets.is_empty());
    }

    #[test]
    fn test_hybrid_search_with_custom_fuser() {
        let db = test_db();
        let hybrid = HybridSearch::new(db).with_fuser(Arc::new(SimpleFuser::new()));
        // Should compile and not panic
        let _ = hybrid;
    }

    #[test]
    fn test_hybrid_search_is_send_sync() {
        // HybridSearch should be Send + Sync for concurrent use
        // Note: Currently may not be due to Arc<dyn Fuser>
        // fn assert_send_sync<T: Send + Sync>() {}
        // assert_send_sync::<HybridSearch>();
    }

    #[test]
    fn test_hybrid_search_multiple_primitives() {
        let db = test_db();
        let kv = KVStore::new(db.clone());
        let branch_index = BranchIndex::new(db.clone());
        let branch_id = BranchId::new();

        // Create the branch in branch_index
        branch_index.create_branch(&branch_id.to_string()).unwrap();

        // Add data to KV primitive
        kv.put(
            &branch_id,
            "default",
            "hello",
            Value::String("hello world data".into()),
        )
        .unwrap();

        let hybrid = HybridSearch::new(db);
        let req = SearchRequest::new(branch_id, "hello");
        let response = hybrid.search(&req).unwrap();

        // Note: MVP simplification - primitives' Searchable implementations return empty results.
        // Full-text search should use InvertedIndex directly. This test verifies orchestration.
        assert!(response.hits.is_empty());
        assert!(!response.truncated);
    }
}
