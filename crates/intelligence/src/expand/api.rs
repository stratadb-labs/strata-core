//! API-based query expander using an OpenAI-compatible endpoint
//!
//! Calls `{endpoint}/chat/completions` with the expansion prompt,
//! parses the response into typed queries.

use super::parser::parse_expansion_with_filter;
use super::prompt::build_messages;
use super::{ExpandError, ExpandedQueries, QueryExpander};
use tracing::warn;

/// Query expander that calls an OpenAI-compatible chat completions endpoint.
///
/// Works with Ollama, vLLM, llama.cpp server, OpenAI, and other compatible providers.
pub struct ApiExpander {
    /// Full URL to the chat completions endpoint
    url: String,
    /// Model name to request
    model: String,
    /// Optional bearer token
    api_key: Option<String>,
    /// Request timeout
    timeout: std::time::Duration,
}

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
        }
    }

    /// Make the HTTP call and return the raw response text.
    #[cfg(feature = "expand")]
    fn call_api(&self, query: &str) -> Result<String, ExpandError> {
        let body = serde_json::json!({
            "model": self.model,
            "messages": build_messages(query),
            "temperature": 0.7,
            "max_tokens": 600,
        });

        let body_bytes = serde_json::to_vec(&body)
            .map_err(|e| ExpandError::Parse(format!("failed to serialize request: {}", e)))?;

        let config = ureq::Agent::config_builder()
            .timeout_global(Some(self.timeout))
            .build();
        let agent = ureq::Agent::new_with_config(config);

        let mut request = agent
            .post(&self.url)
            .header("Content-Type", "application/json");

        if let Some(ref key) = self.api_key {
            request = request.header("Authorization", &format!("Bearer {}", key));
        }

        let mut response = request.send(&body_bytes[..]).map_err(|e| {
            let msg = e.to_string();
            if msg.contains("timed out") || msg.contains("Timeout") {
                ExpandError::Timeout
            } else {
                ExpandError::Network(msg)
            }
        })?;

        let response_text = response
            .body_mut()
            .read_to_string()
            .map_err(|e| ExpandError::Network(format!("failed to read response: {}", e)))?;

        // Parse the OpenAI-format response to extract the content
        let json: serde_json::Value = serde_json::from_str(&response_text)
            .map_err(|e| ExpandError::Parse(format!("invalid JSON response: {}", e)))?;

        let content = json
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .ok_or_else(|| {
                ExpandError::Parse(format!(
                    "unexpected response format: {}",
                    &response_text[..response_text.len().min(200)]
                ))
            })?;

        Ok(content.to_string())
    }

    /// Placeholder for when the `expand` feature is not enabled.
    #[cfg(not(feature = "expand"))]
    fn call_api(&self, _query: &str) -> Result<String, ExpandError> {
        Err(ExpandError::Network(
            "expand feature not enabled".to_string(),
        ))
    }
}

impl QueryExpander for ApiExpander {
    fn expand(&self, query: &str) -> Result<ExpandedQueries, ExpandError> {
        // First attempt
        match self.call_api(query) {
            Ok(text) => {
                let result = parse_expansion_with_filter(&text, Some(query));
                if !result.queries.is_empty() {
                    return Ok(result);
                }
                // Zero valid lines — retry once
                warn!(
                    target: "strata::expand",
                    "First expansion returned no valid lines, retrying"
                );
            }
            Err(e) => {
                warn!(
                    target: "strata::expand",
                    error = %e,
                    "First expansion call failed, retrying"
                );
            }
        }

        // Retry once
        match self.call_api(query) {
            Ok(text) => {
                let result = parse_expansion_with_filter(&text, Some(query));
                if result.queries.is_empty() {
                    warn!(
                        target: "strata::expand",
                        "Retry also returned no valid lines, falling back"
                    );
                    Err(ExpandError::Parse(
                        "model returned no valid expansion lines after retry".to_string(),
                    ))
                } else {
                    Ok(result)
                }
            }
            Err(e) => {
                warn!(
                    target: "strata::expand",
                    error = %e,
                    "Retry also failed, falling back"
                );
                Err(e)
            }
        }
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
