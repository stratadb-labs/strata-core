//! Intelligence layer for Strata
//!
//! Derived operations over the six primitives: search, indexing, fusion.
//! Search is the first prototype; future work includes graph traversal,
//! vector indexing, and other composite intelligence.
//!
//! This crate provides:
//! - Fuser trait for result fusion (RRFFuser)
//! - HybridSearch for composite search orchestration
//! - DatabaseSearchExt extension trait for db.hybrid() accessor
//! - Query expansion and re-ranking via external LLM endpoints
//!
//! # Usage
//!
//! ```text
//! use strata_intelligence::DatabaseSearchExt;
//!
//! let response = db.hybrid().search(&request)?;
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod expand;
pub mod fuser;
pub mod hybrid;
pub mod llm_client;
pub mod rerank;

#[cfg(feature = "embed")]
pub mod runtime;

#[cfg(feature = "embed")]
pub mod embed;

use std::sync::Arc;
use strata_engine::Database;

// Re-export commonly used types
pub use fuser::{weighted_rrf_fuse, FusedResult, Fuser, RRFFuser};
pub use hybrid::HybridSearch;

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
/// ```text
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
