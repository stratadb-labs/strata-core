//! API-based reranker using an OpenAI-compatible chat completions endpoint
//!
//! Sends a single batch prompt with query + document snippets, asks the model
//! to score each document 0-10, parses scores, and returns normalized results.

use super::{RerankError, RerankScore, Reranker};

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
    /// Sampling temperature (default: 0.0 for deterministic scoring)
    temperature: f32,
    /// Maximum response tokens (default: 200)
    max_tokens: u32,
}

/// Default rerank temperature — deterministic for consistent scoring.
const DEFAULT_RERANK_TEMPERATURE: f32 = 0.0;
/// Default max tokens for rerank responses.
const DEFAULT_RERANK_MAX_TOKENS: u32 = 200;

impl ApiReranker {
    /// Create a new ApiReranker.
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
            temperature: DEFAULT_RERANK_TEMPERATURE,
            max_tokens: DEFAULT_RERANK_MAX_TOKENS,
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
    #[cfg(feature = "rerank")]
    fn call_api(&self, query: &str, snippets: &[(usize, &str)]) -> Result<String, RerankError> {
        use super::prompt::build_rerank_messages;

        let body = serde_json::json!({
            "model": self.model,
            "messages": build_rerank_messages(query, snippets),
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

    /// Placeholder for when the `rerank` feature is not enabled.
    #[cfg(not(feature = "rerank"))]
    fn call_api(&self, _query: &str, _snippets: &[(usize, &str)]) -> Result<String, RerankError> {
        Err(RerankError::FeatureDisabled("rerank"))
    }
}

/// Parse the model's response text into rerank scores.
///
/// Expects lines like "1: 8" or "2: 5.5". Maps 1-based line numbers back
/// to the original snippet indices. Scores are normalized to [0.0, 1.0].
pub fn parse_rerank_response(text: &str, snippets: &[(usize, &str)]) -> Vec<RerankScore> {
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

            if let (Ok(line_num), Ok(raw_score)) =
                (num_part.parse::<usize>(), score_part.parse::<f32>())
            {
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
        // Capture snippets for the closure (need owned copies for lifetime)
        let snippets_owned: Vec<(usize, String)> =
            snippets.iter().map(|(i, s)| (*i, s.to_string())).collect();

        crate::llm_client::retry_once(
            || {
                let snippet_refs: Vec<(usize, &str)> = snippets_owned
                    .iter()
                    .map(|(i, s)| (*i, s.as_str()))
                    .collect();
                self.call_api(query, &snippet_refs)
            },
            |text| parse_rerank_response(text, snippets),
            |result| result.is_empty(),
            || {
                // Return empty vec on exhausted retries — caller uses RRF results unchanged
                // We signal this as a parse error
                RerankError::Parse("model returned no valid scores after retry".to_string())
            },
            "strata::rerank",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_reranker_url_construction() {
        let reranker = ApiReranker::new("http://localhost:11434/v1", "qwen3:1.7b", None, 5000);
        assert_eq!(reranker.url, "http://localhost:11434/v1/chat/completions");
    }

    #[test]
    fn test_api_reranker_strips_trailing_slash() {
        let reranker = ApiReranker::new("http://localhost:11434/v1/", "qwen3:1.7b", None, 5000);
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
        assert_eq!(scores[0].index, 3); // maps back to original index
        assert_eq!(scores[1].index, 7);
        assert_eq!(scores[2].index, 12);
    }
}
