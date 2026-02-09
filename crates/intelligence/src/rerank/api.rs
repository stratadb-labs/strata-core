//! API-based reranker using an OpenAI-compatible chat completions endpoint
//!
//! Sends a single batch prompt with query + document snippets, asks the model
//! to score each document 0-10, parses scores, and returns normalized results.

use super::{RerankError, RerankScore, Reranker};
use tracing::warn;

/// Reranker that calls an OpenAI-compatible chat completions endpoint.
///
/// Works with Ollama, vLLM, llama.cpp server, OpenAI, and other compatible providers.
#[allow(dead_code)] // fields used behind #[cfg(feature = "rerank")]
pub struct ApiReranker {
    /// Full URL to the chat completions endpoint
    url: String,
    /// Model name to request
    model: String,
    /// Optional bearer token
    api_key: Option<String>,
    /// Request timeout
    timeout: std::time::Duration,
}

impl ApiReranker {
    /// Create a new ApiReranker.
    ///
    /// `endpoint` should be the base URL (e.g. "http://localhost:11434/v1").
    /// The `/chat/completions` path is appended automatically.
    pub fn new(
        endpoint: &str,
        model: &str,
        api_key: Option<&str>,
        timeout_ms: u64,
    ) -> Self {
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
    #[cfg(feature = "rerank")]
    fn call_api(&self, query: &str, snippets: &[(usize, &str)]) -> Result<String, RerankError> {
        use super::prompt::build_rerank_messages;

        let body = serde_json::json!({
            "model": self.model,
            "messages": build_rerank_messages(query, snippets),
            "temperature": 0.0,
            "max_tokens": 200,
        });

        let body_bytes = serde_json::to_vec(&body)
            .map_err(|e| RerankError::Parse(format!("failed to serialize request: {}", e)))?;

        let config = ureq::Agent::config_builder()
            .timeout_global(Some(self.timeout))
            .build();
        let agent = ureq::Agent::new_with_config(config);

        let mut request = agent.post(&self.url)
            .header("Content-Type", "application/json");

        if let Some(ref key) = self.api_key {
            request = request.header("Authorization", &format!("Bearer {}", key));
        }

        let mut response = request
            .send(&body_bytes[..])
            .map_err(|e| {
                let msg = e.to_string();
                if msg.contains("timed out") || msg.contains("Timeout") {
                    RerankError::Timeout
                } else {
                    RerankError::Network(msg)
                }
            })?;

        let response_text = response
            .body_mut()
            .read_to_string()
            .map_err(|e| RerankError::Network(format!("failed to read response: {}", e)))?;

        // Parse the OpenAI-format response to extract the content
        let json: serde_json::Value = serde_json::from_str(&response_text)
            .map_err(|e| RerankError::Parse(format!("invalid JSON response: {}", e)))?;

        let content = json
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .ok_or_else(|| {
                RerankError::Parse(format!(
                    "unexpected response format: {}",
                    &response_text[..response_text.len().min(200)]
                ))
            })?;

        Ok(content.to_string())
    }

    /// Placeholder for when the `rerank` feature is not enabled.
    #[cfg(not(feature = "rerank"))]
    fn call_api(&self, _query: &str, _snippets: &[(usize, &str)]) -> Result<String, RerankError> {
        Err(RerankError::Network(
            "rerank feature not enabled".to_string(),
        ))
    }
}

/// Parse the model's response text into rerank scores.
///
/// Expects lines like "1: 8" or "2: 5.5". Maps 1-based line numbers back
/// to the original snippet indices. Scores are normalized to [0.0, 1.0].
pub fn parse_rerank_response(
    text: &str,
    snippets: &[(usize, &str)],
) -> Vec<RerankScore> {
    let mut scores = Vec::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Parse "N: score" format
        if let Some((num_part, score_part)) = line.split_once(':') {
            let num_part = num_part.trim();
            let score_part = score_part.trim();

            if let (Ok(line_num), Ok(raw_score)) = (
                num_part.parse::<usize>(),
                score_part.parse::<f32>(),
            ) {
                // line_num is 1-based, convert to 0-based index into snippets
                if line_num >= 1 && line_num <= snippets.len() {
                    let (orig_index, _) = snippets[line_num - 1];
                    let clamped = raw_score.clamp(0.0, 10.0);
                    scores.push(RerankScore {
                        index: orig_index,
                        relevance_score: clamped / 10.0,
                    });
                }
            }
        }
    }

    scores
}

impl Reranker for ApiReranker {
    fn rerank(
        &self,
        query: &str,
        snippets: &[(usize, &str)],
    ) -> Result<Vec<RerankScore>, RerankError> {
        // First attempt
        match self.call_api(query, snippets) {
            Ok(text) => {
                let result = parse_rerank_response(&text, snippets);
                if !result.is_empty() {
                    return Ok(result);
                }
                // Zero valid scores — retry once
                warn!(
                    target: "strata::rerank",
                    "First rerank returned no valid scores, retrying"
                );
            }
            Err(e) => {
                warn!(
                    target: "strata::rerank",
                    error = %e,
                    "First rerank call failed, retrying"
                );
            }
        }

        // Retry once
        match self.call_api(query, snippets) {
            Ok(text) => {
                let result = parse_rerank_response(&text, snippets);
                if result.is_empty() {
                    warn!(
                        target: "strata::rerank",
                        "Retry also returned no valid scores, falling back"
                    );
                    // Return empty — caller will use RRF results unchanged
                    Ok(vec![])
                } else {
                    Ok(result)
                }
            }
            Err(e) => {
                warn!(
                    target: "strata::rerank",
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
    fn test_api_reranker_url_construction() {
        let reranker = ApiReranker::new(
            "http://localhost:11434/v1",
            "qwen3:1.7b",
            None,
            5000,
        );
        assert_eq!(reranker.url, "http://localhost:11434/v1/chat/completions");
    }

    #[test]
    fn test_api_reranker_strips_trailing_slash() {
        let reranker = ApiReranker::new(
            "http://localhost:11434/v1/",
            "qwen3:1.7b",
            None,
            5000,
        );
        assert_eq!(reranker.url, "http://localhost:11434/v1/chat/completions");
    }

    #[test]
    fn test_parse_rerank_response_basic() {
        let snippets = vec![(0, "doc a"), (1, "doc b"), (2, "doc c")];
        let text = "1: 8\n2: 5\n3: 3\n";
        let scores = parse_rerank_response(text, &snippets);
        assert_eq!(scores.len(), 3);
        assert_eq!(scores[0].index, 0);
        assert!((scores[0].relevance_score - 0.8).abs() < f32::EPSILON);
        assert_eq!(scores[1].index, 1);
        assert!((scores[1].relevance_score - 0.5).abs() < f32::EPSILON);
        assert_eq!(scores[2].index, 2);
        assert!((scores[2].relevance_score - 0.3).abs() < f32::EPSILON);
    }

    #[test]
    fn test_parse_rerank_response_with_decimals() {
        let snippets = vec![(5, "doc a"), (10, "doc b")];
        let text = "1: 7.5\n2: 3.2\n";
        let scores = parse_rerank_response(text, &snippets);
        assert_eq!(scores.len(), 2);
        assert_eq!(scores[0].index, 5);
        assert!((scores[0].relevance_score - 0.75).abs() < f32::EPSILON);
        assert_eq!(scores[1].index, 10);
        assert!((scores[1].relevance_score - 0.32).abs() < 0.01);
    }

    #[test]
    fn test_parse_rerank_response_clamps_scores() {
        let snippets = vec![(0, "doc a")];
        let text = "1: 15\n";
        let scores = parse_rerank_response(text, &snippets);
        assert_eq!(scores.len(), 1);
        assert!((scores[0].relevance_score - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_parse_rerank_response_ignores_invalid_lines() {
        let snippets = vec![(0, "doc a"), (1, "doc b")];
        let text = "1: 8\nsome garbage\n2: five\n";
        let scores = parse_rerank_response(text, &snippets);
        // Only "1: 8" should parse successfully
        assert_eq!(scores.len(), 1);
        assert_eq!(scores[0].index, 0);
    }

    #[test]
    fn test_parse_rerank_response_empty() {
        let snippets = vec![(0, "doc a")];
        let text = "";
        let scores = parse_rerank_response(text, &snippets);
        assert!(scores.is_empty());
    }

    #[test]
    fn test_parse_rerank_response_out_of_range_number() {
        let snippets = vec![(0, "doc a")];
        // Line number 5 is out of range (only 1 snippet)
        let text = "5: 8\n";
        let scores = parse_rerank_response(text, &snippets);
        assert!(scores.is_empty());
    }

    #[test]
    fn test_parse_rerank_response_negative_scores() {
        let snippets = vec![(0, "doc a"), (1, "doc b")];
        let text = "1: -5\n2: 3\n";
        let scores = parse_rerank_response(text, &snippets);
        assert_eq!(scores.len(), 2);
        // Negative score should be clamped to 0.0
        assert!((scores[0].relevance_score - 0.0).abs() < f32::EPSILON);
        assert!((scores[1].relevance_score - 0.3).abs() < f32::EPSILON);
    }

    #[test]
    fn test_parse_rerank_response_zero_score() {
        let snippets = vec![(0, "doc a")];
        let text = "1: 0\n";
        let scores = parse_rerank_response(text, &snippets);
        assert_eq!(scores.len(), 1);
        assert!((scores[0].relevance_score - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_parse_rerank_response_preserves_original_indices() {
        // Original indices are not sequential
        let snippets = vec![(3, "doc d"), (7, "doc h"), (12, "doc m")];
        let text = "1: 9\n2: 4\n3: 7\n";
        let scores = parse_rerank_response(text, &snippets);
        assert_eq!(scores.len(), 3);
        assert_eq!(scores[0].index, 3);   // maps back to original index
        assert_eq!(scores[1].index, 7);
        assert_eq!(scores[2].index, 12);
    }
}
