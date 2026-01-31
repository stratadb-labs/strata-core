//! Search module for keyword and retrieval operations
//!
//! This module contains:
//! - `types`: Core search types (SearchRequest, SearchResponse, SearchHit, etc.)
//! - `searchable`: Searchable trait and scoring infrastructure
//! - `index`: Optional inverted index for fast keyword search
//! - `tokenizer`: Basic text tokenization

mod types;
mod searchable;
mod index;
pub mod tokenizer;

pub use types::{
    EntityRef, PrimitiveType,
    SearchBudget, SearchMode, SearchRequest, SearchResponse, SearchHit, SearchStats,
};
pub use searchable::{
    Searchable, SearchCandidate, SearchDoc, Scorer, BM25LiteScorer, SimpleScorer, ScorerContext,
    build_search_response, build_search_response_with_index,
};
pub use index::{InvertedIndex, PostingEntry, PostingList};
pub use tokenizer::{tokenize, tokenize_unique};
