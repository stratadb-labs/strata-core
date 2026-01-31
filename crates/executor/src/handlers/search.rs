//! Search command handler.
//!
//! Handles cross-primitive search via the intelligence layer's HybridSearch.

use std::sync::Arc;

use strata_engine::{SearchBudget, SearchRequest};
use strata_engine::search::PrimitiveType;
use strata_intelligence::HybridSearch;

use crate::bridge::{to_core_branch_id, Primitives};
use crate::types::{BranchId, SearchResultHit};
use crate::{Output, Result};

/// Handle Search command: cross-primitive search
pub fn search(
    p: &Arc<Primitives>,
    run: BranchId,
    query: String,
    k: Option<u64>,
    primitives: Option<Vec<String>>,
) -> Result<Output> {
    let core_run_id = to_core_branch_id(&run)?;

    // Build primitive filter from string names
    let primitive_filter = primitives.map(|names| {
        names
            .iter()
            .filter_map(|name| match name.to_lowercase().as_str() {
                "kv" => Some(PrimitiveType::Kv),
                "json" => Some(PrimitiveType::Json),
                "event" => Some(PrimitiveType::Event),
                "state" => Some(PrimitiveType::State),
                "run" => Some(PrimitiveType::Branch),
                "vector" => Some(PrimitiveType::Vector),
                _ => None,
            })
            .collect::<Vec<_>>()
    });

    let mut req = SearchRequest::new(core_run_id, &query);
    if let Some(top_k) = k {
        req = req.with_k(top_k as usize);
    }
    req.budget = SearchBudget::default();
    if let Some(filter) = primitive_filter {
        if !filter.is_empty() {
            req = req.with_primitive_filter(filter);
        }
    }

    let hybrid = HybridSearch::new(p.db.clone());
    let response = hybrid.search(&req).map_err(|e| crate::Error::Internal {
        reason: e.to_string(),
    })?;

    // Convert SearchResponse hits to SearchResultHit
    let results: Vec<SearchResultHit> = response
        .hits
        .into_iter()
        .map(|hit| {
            let (entity, primitive) = format_entity_ref(&hit.doc_ref);
            SearchResultHit {
                entity,
                primitive,
                score: hit.score,
                rank: hit.rank,
                snippet: hit.snippet,
            }
        })
        .collect();

    Ok(Output::SearchResults(results))
}

/// Format an EntityRef into (entity_string, primitive_string) for display
fn format_entity_ref(doc_ref: &strata_engine::search::EntityRef) -> (String, String) {
    match doc_ref {
        strata_engine::search::EntityRef::Kv { key, .. } => {
            (key.clone(), "kv".to_string())
        }
        strata_engine::search::EntityRef::Json { doc_id, .. } => {
            (doc_id.clone(), "json".to_string())
        }
        strata_engine::search::EntityRef::Event { sequence, .. } => {
            (format!("seq:{}", sequence), "event".to_string())
        }
        strata_engine::search::EntityRef::State { name, .. } => {
            (name.clone(), "state".to_string())
        }
        strata_engine::search::EntityRef::Branch { branch_id } => {
            let uuid = uuid::Uuid::from_bytes(*branch_id.as_bytes());
            (uuid.to_string(), "run".to_string())
        }
        strata_engine::search::EntityRef::Vector { key, .. } => {
            (key.clone(), "vector".to_string())
        }
    }
}
