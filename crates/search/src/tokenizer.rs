//! Basic tokenizer for M6 search
//!
//! This module provides simple text tokenization for search operations.
//! Future milestones can add stemming, stopwords, etc.

/// Tokenize text into searchable terms
///
/// This is a simple tokenizer for M6:
/// - Lowercase
/// - Split on non-alphanumeric characters
/// - Filter tokens shorter than 2 characters
///
/// # Example
///
/// ```
/// use in_mem_search::tokenizer::tokenize;
///
/// let tokens = tokenize("Hello, World!");
/// assert_eq!(tokens, vec!["hello", "world"]);
/// ```
pub fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| s.len() >= 2)
        .map(String::from)
        .collect()
}

/// Tokenize and deduplicate for query processing
///
/// # Example
///
/// ```
/// use in_mem_search::tokenizer::tokenize_unique;
///
/// let tokens = tokenize_unique("test test TEST");
/// assert_eq!(tokens, vec!["test"]);
/// ```
pub fn tokenize_unique(text: &str) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    tokenize(text)
        .into_iter()
        .filter(|t| seen.insert(t.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_basic() {
        let tokens = tokenize("Hello, World!");
        assert_eq!(tokens, vec!["hello", "world"]);
    }

    #[test]
    fn test_tokenize_filters_short() {
        let tokens = tokenize("I am a test");
        // "I" and "a" filtered (< 2 chars)
        assert_eq!(tokens, vec!["am", "test"]);
    }

    #[test]
    fn test_tokenize_numbers() {
        let tokens = tokenize("test123 foo456bar");
        assert_eq!(tokens, vec!["test123", "foo456bar"]);
    }

    #[test]
    fn test_tokenize_empty() {
        let tokens = tokenize("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_tokenize_only_punctuation() {
        let tokens = tokenize("...---...");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_tokenize_unique() {
        let tokens = tokenize_unique("test test TEST");
        assert_eq!(tokens, vec!["test"]);
    }

    #[test]
    fn test_tokenize_unique_preserves_order() {
        let tokens = tokenize_unique("apple banana apple cherry");
        assert_eq!(tokens, vec!["apple", "banana", "cherry"]);
    }
}
