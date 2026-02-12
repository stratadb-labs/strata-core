//! Error types for query expansion — type alias for shared LlmClientError

/// Errors that can occur during query expansion
pub type ExpandError = crate::llm_client::LlmClientError;
