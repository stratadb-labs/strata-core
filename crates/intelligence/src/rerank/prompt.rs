//! Prompt template for re-ranking via chat completions

/// System prompt for relevance scoring via an LLM.
///
/// Instructs the model to output `N: score` lines for each numbered document.
pub const SYSTEM_PROMPT: &str = "\
You are a search relevance scorer. Given a query and numbered documents, \
score each document's relevance to the query from 0 to 10.

Output format (one per line, no other text):
1: <score>
2: <score>
...

Rules:
- Score 0 = completely irrelevant, 10 = perfect match
- Output ONLY numbered score lines
- Score every document listed";

/// Build the messages array for a reranking chat completions request.
///
/// The user message contains the query and numbered document snippets.
pub fn build_rerank_messages(query: &str, snippets: &[(usize, &str)]) -> serde_json::Value {
    let mut user_content = format!("Query: {}\n\nDocuments:", query);
    for (i, (_orig_idx, text)) in snippets.iter().enumerate() {
        user_content.push_str(&format!("\n{}. {}", i + 1, text));
    }

    serde_json::json!([
        {"role": "system", "content": SYSTEM_PROMPT},
        {"role": "user", "content": user_content}
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_prompt_not_empty() {
        assert!(!SYSTEM_PROMPT.is_empty());
        assert!(SYSTEM_PROMPT.contains("score"));
        assert!(SYSTEM_PROMPT.contains("0 to 10"));
    }

    #[test]
    fn test_build_rerank_messages_structure() {
        let snippets = vec![(0, "first doc"), (1, "second doc")];
        let messages = build_rerank_messages("test query", &snippets);
        let arr = messages.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["role"], "system");
        assert_eq!(arr[1]["role"], "user");

        let content = arr[1]["content"].as_str().unwrap();
        assert!(content.contains("Query: test query"));
        assert!(content.contains("1. first doc"));
        assert!(content.contains("2. second doc"));
    }

    #[test]
    fn test_build_rerank_messages_numbering() {
        let snippets = vec![(5, "alpha"), (10, "beta"), (15, "gamma")];
        let messages = build_rerank_messages("q", &snippets);
        let content = messages[1]["content"].as_str().unwrap();
        // Numbering is sequential 1-based regardless of original indices
        assert!(content.contains("1. alpha"));
        assert!(content.contains("2. beta"));
        assert!(content.contains("3. gamma"));
    }
}
