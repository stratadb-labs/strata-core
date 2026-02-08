//! Error types for query expansion

use std::fmt;

/// Errors that can occur during query expansion
#[derive(Debug)]
pub enum ExpandError {
    /// HTTP request failed (network unreachable, connection refused, etc.)
    Network(String),
    /// Failed to parse model response into valid expansions
    Parse(String),
    /// Model request timed out
    Timeout,
    /// Model returned an error response
    Model(String),
}

impl fmt::Display for ExpandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExpandError::Network(msg) => write!(f, "network error: {}", msg),
            ExpandError::Parse(msg) => write!(f, "parse error: {}", msg),
            ExpandError::Timeout => write!(f, "model request timed out"),
            ExpandError::Model(msg) => write!(f, "model error: {}", msg),
        }
    }
}

impl std::error::Error for ExpandError {}
