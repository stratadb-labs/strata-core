//! Search command handler.
//!
//! Handles cross-primitive search via the intelligence layer's HybridSearch.
//! When a model is configured, transparently expands queries for better recall.

use std::sync::Arc;

use strata_engine::database::ModelConfigState;
use strata_engine::search::PrimitiveType;
use strata_engine::{SearchBudget, SearchMode, SearchRequest};
use strata_intelligence::HybridSearch;
use tracing::debug;

use crate::bridge::{to_core_branch_id, Primitives};
use crate::types::{BranchId, SearchResultHit};
use crate::{Output, Result};

/// Strong signal threshold: if top BM25 score >= this, skip expansion.
const STRONG_SIGNAL_SCORE: f32 = 0.85;
/// Strong signal gap: top score must exceed #2 by at least this much.
const STRONG_SIGNAL_GAP: f32 = 0.15;

/// Handle Search command: cross-primitive search
pub fn search(
    p: &Arc<Primitives>,
    branch: BranchId,
    _space: String,
    query: String,
    k: Option<u64>,
    primitives: Option<Vec<String>>,
) -> Result<Output> {
    let core_branch_id = to_core_branch_id(&branch)?;

    // Build primitive filter from string names
    let primitive_filter = primitives.map(|names| {
        names
            .iter()
            .filter_map(|name| match name.to_lowercase().as_str() {
                "kv" => Some(PrimitiveType::Kv),
                "json" => Some(PrimitiveType::Json),
                "event" => Some(PrimitiveType::Event),
                "state" => Some(PrimitiveType::State),
                "branch" => Some(PrimitiveType::Branch),
                "vector" => Some(PrimitiveType::Vector),
                _ => None,
            })
            .collect::<Vec<_>>()
    });

    let mut req = SearchRequest::new(core_branch_id, &query);
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

    // Check if a model is configured for query expansion
    let has_model = has_model_configured(&p.db);

    let response = if has_model {
        // Strong signal detection: cheap BM25 probe BEFORE calling LLM
        let probe_req = req.clone().with_mode(SearchMode::Keyword);
        let probe = hybrid.search(&probe_req).map_err(crate::Error::from)?;

        if has_strong_signal(&probe) {
            debug!(
                target: "strata::search",
                query = %query,
                top_score = probe.hits.first().map(|h| h.score).unwrap_or(0.0),
                "Strong BM25 signal, skipping expansion"
            );
            // Strong signal: return full hybrid search (skip LLM entirely)
            hybrid.search(&req).map_err(crate::Error::from)?
        } else if let Some(expansions) = try_expand(&p.db, &query) {
            debug!(
                target: "strata::search",
                query = %query,
                expansion_count = expansions.len(),
                "Using query expansion"
            );
            hybrid
                .search_expanded(&req, &expansions, 2.0)
                .map_err(crate::Error::from)?
        } else {
            // Expansion failed — fall back to normal search
            hybrid.search(&req).map_err(crate::Error::from)?
        }
    } else {
        // No model configured — existing search path
        hybrid.search(&req).map_err(crate::Error::from)?
    };

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

/// Check if a model is configured (cheap — no LLM call).
fn has_model_configured(db: &Arc<strata_engine::Database>) -> bool {
    db.extension::<ModelConfigState>()
        .ok()
        .and_then(|state| {
            let guard = state.config.read();
            if guard.is_some() { Some(()) } else { None }
        })
        .is_some()
}

/// Try to expand a query using the configured model.
///
/// Returns `Some(expansions)` if a model is configured and expansion succeeds.
/// Returns `None` if no model is configured or expansion fails (graceful degradation).
fn try_expand(
    db: &Arc<strata_engine::Database>,
    query: &str,
) -> Option<Vec<strata_intelligence::expand::ExpandedQuery>> {
    let state = db.extension::<ModelConfigState>().ok()?;
    let config_guard = state.config.read();
    let config = config_guard.as_ref()?;

    let expander = strata_intelligence::expand::ApiExpander::new(
        &config.endpoint,
        &config.model,
        config.api_key.as_deref(),
        config.timeout_ms,
    );

    match strata_intelligence::expand::QueryExpander::expand(&expander, query) {
        Ok(expanded) if !expanded.queries.is_empty() => Some(expanded.queries),
        Ok(_) => {
            debug!(target: "strata::search", "Expansion returned empty, falling back");
            None
        }
        Err(e) => {
            debug!(target: "strata::search", error = %e, "Expansion failed, falling back");
            None
        }
    }
}

/// Check if BM25 probe results have a strong enough signal to skip expansion.
fn has_strong_signal(response: &strata_engine::search::SearchResponse) -> bool {
    if response.hits.is_empty() {
        return false;
    }
    let top_score = response.hits[0].score;
    let second_score = response.hits.get(1).map(|h| h.score).unwrap_or(0.0);
    top_score >= STRONG_SIGNAL_SCORE && (top_score - second_score) >= STRONG_SIGNAL_GAP
}

/// Format an EntityRef into (entity_string, primitive_string) for display
fn format_entity_ref(doc_ref: &strata_engine::search::EntityRef) -> (String, String) {
    match doc_ref {
        strata_engine::search::EntityRef::Kv { key, .. } => (key.clone(), "kv".to_string()),
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
            (uuid.to_string(), "branch".to_string())
        }
        strata_engine::search::EntityRef::Vector { key, .. } => {
            (key.clone(), "vector".to_string())
        }
    }
}
