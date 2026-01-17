//! Search infrastructure for M6 Retrieval Surfaces
//!
//! This crate provides:
//! - Scorer trait for pluggable scoring algorithms
//! - ScorerContext for corpus-level statistics
//! - BM25LiteScorer default implementation
//! - Basic tokenizer
//!
//! See `docs/architecture/M6_ARCHITECTURE.md` for authoritative specification.

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod scorer;
pub mod tokenizer;

// Re-export commonly used types
pub use scorer::{BM25LiteScorer, Scorer, ScorerContext, SearchDoc};
pub use tokenizer::{tokenize, tokenize_unique};
