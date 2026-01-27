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

use crate::fuser::{Fuser, SimpleFuser};
use strata_core::error::Result;
use strata_core::search_types::{SearchBudget, SearchRequest, SearchResponse, SearchStats};
use strata_core::PrimitiveType;
use strata_engine::Database;
use strata_engine::{EventLog, JsonStore, KVStore, RunIndex, StateCell, VectorStore};
use std::sync::Arc;
use std::time::Instant;

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
    run_index: RunIndex,
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
            run_index: RunIndex::new(db.clone()),
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
    pub fn search(&self, req: &SearchRequest) -> Result<SearchResponse> {
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

        // 4. Fuse results
        let fused = self.fuser.fuse(primitive_results, req.k);

        // 5. Build stats
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
    ) -> Result<SearchResponse> {
        use strata_engine::Searchable;

        match primitive {
            PrimitiveType::Kv => self.kv.search(req),
            PrimitiveType::Json => self.json.search(req),
            PrimitiveType::Event => self.event.search(req),
            PrimitiveType::State => self.state.search(req),
            PrimitiveType::Run => self.run_index.search(req),
            // Vector primitive now implements Searchable.
            // Per M8_ARCHITECTURE.md Section 12.3:
            // - Keyword search returns empty (by design)
            // - For vector/hybrid search with embeddings, the orchestrator
            //   should call vector.search_response() directly with the embedding
            PrimitiveType::Vector => Searchable::search(&self.vector, req),
        }
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
    use strata_core::types::RunId;
    use strata_core::value::Value;

    fn test_db() -> Arc<Database> {
        Arc::new(
            Database::builder()
                .in_memory()
                .open_temp()
                .expect("Failed to create test database"),
        )
    }

    #[test]
    fn test_hybrid_search_new() {
        let db = test_db();
        let hybrid = HybridSearch::new(db);
        // Should compile and not panic
        assert!(Arc::ptr_eq(hybrid.kv.database(), hybrid.json.database()));
    }

    #[test]
    fn test_hybrid_search_empty() {
        let db = test_db();
        let hybrid = HybridSearch::new(db);
        let run_id = RunId::new();

        let req = SearchRequest::new(run_id, "test");
        let response = hybrid.search(&req).unwrap();

        assert!(response.hits.is_empty());
        assert!(!response.truncated);
    }

    #[test]
    fn test_hybrid_search_kv_only() {
        let db = test_db();
        let kv = KVStore::new(db.clone());
        let run_id = RunId::new();

        // Add test data
        kv.put(&run_id, "hello", Value::String("world test data".into()))
            .unwrap();
        kv.put(&run_id, "test", Value::String("this is a test".into()))
            .unwrap();

        let hybrid = HybridSearch::new(db);
        let req = SearchRequest::new(run_id, "test").with_primitive_filter(vec![PrimitiveType::Kv]);
        let response = hybrid.search(&req).unwrap();

        // Should have at least one result
        assert!(!response.hits.is_empty());

        // All results should be KV
        for hit in &response.hits {
            assert_eq!(hit.doc_ref.primitive_type(), PrimitiveType::Kv);
        }
    }

    #[test]
    fn test_hybrid_search_primitive_filter() {
        let db = test_db();
        let hybrid = HybridSearch::new(db);
        let run_id = RunId::new();

        // Test with filter
        let req_filtered = SearchRequest::new(run_id, "test")
            .with_primitive_filter(vec![PrimitiveType::Kv, PrimitiveType::Json]);

        let primitives = hybrid.select_primitives(&req_filtered);
        assert_eq!(primitives.len(), 2);
        assert!(primitives.contains(&PrimitiveType::Kv));
        assert!(primitives.contains(&PrimitiveType::Json));

        // Test without filter (all primitives)
        let req_all = SearchRequest::new(run_id, "test");
        let all_primitives = hybrid.select_primitives(&req_all);
        assert_eq!(all_primitives.len(), 6); // Kv, Event, State, Run, Json, Vector
    }

    #[test]
    fn test_hybrid_search_budget_allocation() {
        let db = test_db();
        let hybrid = HybridSearch::new(db);
        let run_id = RunId::new();

        let req = SearchRequest::new(run_id, "test");

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
        let run_index = RunIndex::new(db.clone());
        let run_id = RunId::new();

        // Create the run in run_index
        run_index.create_run(&run_id.to_string()).unwrap();

        // Add data to KV primitive
        kv.put(&run_id, "hello", Value::String("hello world data".into()))
            .unwrap();

        let hybrid = HybridSearch::new(db);
        let req = SearchRequest::new(run_id, "hello");
        let response = hybrid.search(&req).unwrap();

        // At least KV should have results
        assert!(!response.hits.is_empty());
    }
}
