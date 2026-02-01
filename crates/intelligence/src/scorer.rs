//! Scoring infrastructure — re-exported from strata_engine::search
//!
//! The scorer types have been moved to the engine crate so that
//! `build_search_response()` can use BM25 scoring directly.
//! This module re-exports for backward compatibility.

pub use strata_engine::search::{BM25LiteScorer, Scorer, ScorerContext, SearchDoc};
