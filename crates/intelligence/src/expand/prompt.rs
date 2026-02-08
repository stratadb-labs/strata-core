//! Prompt template for query expansion

/// System prompt for query expansion via an LLM.
///
/// Instructs the model to output `lex:`, `vec:`, and `hyde:` prefixed lines.
pub const SYSTEM_PROMPT: &str = "\
You are a search query expander for a multi-primitive database that stores \
key-value pairs, JSON documents, events, state cells, and vector embeddings.

Given a user's search query, generate alternative search variants to improve recall.

Output format (one per line, no other text):
lex: <keyword query for BM25 text search>
vec: <natural language phrase for semantic vector similarity>
hyde: <hypothetical document passage that would match the query>

Rules:
- Generate 1-3 lex lines (keyword reformulations, abbreviations, technical terms)
- Generate 1 vec line (natural language rephrasing for semantic similarity)
- Generate 1 hyde line (50-200 chars, what the ideal matching document looks like)
- Do NOT repeat the original query verbatim
- Do NOT include any explanation, numbering, or markdown
- Output ONLY lines starting with lex:, vec:, or hyde:";

/// Build the messages array for an OpenAI-compatible chat completions request.
pub fn build_messages(query: &str) -> serde_json::Value {
    serde_json::json!([
        {"role": "system", "content": SYSTEM_PROMPT},
        {"role": "user", "content": query}
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_prompt_not_empty() {
        assert!(!SYSTEM_PROMPT.is_empty());
        assert!(SYSTEM_PROMPT.contains("lex:"));
        assert!(SYSTEM_PROMPT.contains("vec:"));
        assert!(SYSTEM_PROMPT.contains("hyde:"));
    }

    #[test]
    fn test_build_messages_structure() {
        let messages = build_messages("test query");
        let arr = messages.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["role"], "system");
        assert_eq!(arr[1]["role"], "user");
        assert_eq!(arr[1]["content"], "test query");
    }
}
