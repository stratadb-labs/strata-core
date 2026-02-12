//! API-based query expander using an OpenAI-compatible endpoint
//!
//! Calls `{endpoint}/chat/completions` with the expansion prompt,
//! parses the response into typed queries.

use super::parser::parse_expansion_with_filter;
use super::{ExpandError, ExpandedQueries, QueryExpander};

/// Query expander that calls an OpenAI-compatible chat completions endpoint.
///
/// Works with Ollama, vLLM, llama.cpp server, OpenAI, and other compatible providers.
#[allow(dead_code)] // fields used behind #[cfg(feature = "expand")]
pub struct ApiExpander {
    /// Full URL to the chat completions endpoint
    url: String,
    /// Model name to request
    model: String,
    /// Optional bearer token
    api_key: Option<String>,
    /// Request timeout
    timeout: std::time::Duration,
    /// Sampling temperature (default: 0.7)
    temperature: f32,
    /// Maximum response tokens (default: 600)
    max_tokens: u32,
}

/// Default expansion temperature — moderate creativity for query variations.
const DEFAULT_EXPAND_TEMPERATURE: f32 = 0.7;
/// Default max tokens for expansion responses.
const DEFAULT_EXPAND_MAX_TOKENS: u32 = 600;

impl ApiExpander {
    /// Create a new ApiExpander.
    ///
    /// `endpoint` should be the base URL (e.g. "http://localhost:11434/v1").
    /// The `/chat/completions` path is appended automatically.
    pub fn new(endpoint: &str, model: &str, api_key: Option<&str>, timeout_ms: u64) -> Self {
        let base = endpoint.trim_end_matches('/');
        let url = format!("{}/chat/completions", base);
        Self {
            url,
            model: model.to_string(),
            api_key: api_key.map(|s| s.to_string()),
            timeout: std::time::Duration::from_millis(timeout_ms),
            temperature: DEFAULT_EXPAND_TEMPERATURE,
            max_tokens: DEFAULT_EXPAND_MAX_TOKENS,
        }
    }

    /// Override the sampling temperature.
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature;
        self
    }

    /// Override the maximum response tokens.
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Make the HTTP call and return the raw response text.
    #[cfg(feature = "expand")]
    fn call_api(&self, query: &str) -> Result<String, ExpandError> {
        use super::prompt::build_messages;

        let body = serde_json::json!({
            "model": self.model,
            "messages": build_messages(query),
            "temperature": self.temperature,
            "max_tokens": self.max_tokens,
        });

        crate::llm_client::call_chat_completions(
            &self.url,
            self.api_key.as_deref(),
            self.timeout,
            &body,
        )
    }

    /// Placeholder for when the `expand` feature is not enabled.
    #[cfg(not(feature = "expand"))]
    fn call_api(&self, _query: &str) -> Result<String, ExpandError> {
        Err(ExpandError::FeatureDisabled("expand"))
    }
}

impl QueryExpander for ApiExpander {
    fn expand(&self, query: &str) -> Result<ExpandedQueries, ExpandError> {
        crate::llm_client::retry_once(
            || self.call_api(query),
            |text| parse_expansion_with_filter(text, Some(query)),
            |result| result.queries.is_empty(),
            || {
                ExpandError::Parse(
                    "model returned no valid expansion lines after retry".to_string(),
                )
            },
            "strata::expand",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_expander_url_construction() {
        let expander = ApiExpander::new("http://localhost:11434/v1", "qwen3:1.7b", None, 5000);
        assert_eq!(expander.url, "http://localhost:11434/v1/chat/completions");
    }

    #[test]
    fn test_api_expander_strips_trailing_slash() {
        let expander = ApiExpander::new("http://localhost:11434/v1/", "qwen3:1.7b", None, 5000);
        assert_eq!(expander.url, "http://localhost:11434/v1/chat/completions");
    }
}
