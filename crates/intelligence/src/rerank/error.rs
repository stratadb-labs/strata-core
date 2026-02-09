//! Error types for re-ranking

use std::fmt;

/// Errors that can occur during re-ranking
#[derive(Debug)]
pub enum RerankError {
    /// HTTP request failed (network unreachable, connection refused, etc.)
    Network(String),
    /// Failed to parse model response into valid scores
    Parse(String),
    /// Model request timed out
    Timeout,
}

impl fmt::Display for RerankError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RerankError::Network(msg) => write!(f, "network error: {}", msg),
            RerankError::Parse(msg) => write!(f, "parse error: {}", msg),
            RerankError::Timeout => write!(f, "rerank request timed out"),
        }
    }
}

impl std::error::Error for RerankError {}
