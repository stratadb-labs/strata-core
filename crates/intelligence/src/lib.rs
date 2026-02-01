//! Intelligence layer for Strata
//!
//! Derived operations over the six primitives: search, indexing, fusion.
//! Search is the first prototype; future work includes graph traversal,
//! vector indexing, and other composite intelligence.
//!
//! This crate provides:
//! - Scorer trait for pluggable scoring algorithms
//! - ScorerContext for corpus-level statistics
//! - BM25LiteScorer default implementation
//! - Basic tokenizer
//! - Fuser trait for result fusion
//! - HybridSearch for composite search orchestration
//! - DatabaseSearchExt extension trait for db.hybrid() accessor
//!
//! # Usage
//!
//! ```ignore
//! use strata_intelligence::DatabaseSearchExt;
//!
//! let response = db.hybrid().search(&request)?;
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod fuser;
pub mod hybrid;
pub mod index;
pub mod scorer;
pub mod tokenizer;

use std::sync::Arc;
use strata_engine::Database;

// Re-export commonly used types
pub use fuser::{FusedResult, Fuser, RRFFuser, SimpleFuser};
pub use hybrid::HybridSearch;
pub use index::{InvertedIndex, PostingEntry, PostingList};
pub use scorer::{BM25LiteScorer, Scorer, ScorerContext, SearchDoc};
pub use tokenizer::{tokenize, tokenize_unique};

// ============================================================================
// Database Extension
// ============================================================================

/// Extension trait for Database to provide search functionality
///
/// This trait adds the `.hybrid()` method to Arc<Database> for accessing
/// the composite search orchestrator.
///
/// # Example
///
/// ```ignore
/// use strata_intelligence::DatabaseSearchExt;
/// use std::sync::Arc;
///
/// let db = Database::cache()?;
/// let hybrid = db.hybrid();
/// let response = hybrid.search(&request)?;
/// ```
pub trait DatabaseSearchExt {
    /// Get the hybrid search interface
    ///
    /// Returns an orchestrator for searching across multiple primitives.
    fn hybrid(&self) -> HybridSearch;
}

impl DatabaseSearchExt for Arc<Database> {
    fn hybrid(&self) -> HybridSearch {
        HybridSearch::new(Arc::clone(self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strata_core::types::BranchId;
    use strata_engine::search::SearchRequest;

    #[test]
    fn test_database_search_ext() {
        let db = Database::cache().expect("Failed to create test database");

        let hybrid = db.hybrid();
        let branch_id = BranchId::new();
        let req = SearchRequest::new(branch_id, "test");

        // Should be able to search (even if no results)
        let response = hybrid.search(&req).unwrap();
        assert!(response.hits.is_empty());
    }
}
