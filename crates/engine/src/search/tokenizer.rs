//! Text tokenizer for search operations
//!
//! Pipeline: UAX#29 word boundaries → strip possessives → remove non-alpha
//!           → lowercase → filter short tokens → remove stopwords → Porter stem

use super::stemmer;
use unicode_segmentation::UnicodeSegmentation;

/// Standard English stopwords (Lucene's default set).
///
/// These high-frequency words carry little discriminative value for BM25
/// and are filtered out during tokenization.
const STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "but", "by", "for", "if", "in", "into", "is", "it",
    "no", "not", "of", "on", "or", "such", "that", "the", "their", "then", "there", "these",
    "they", "this", "to", "was", "will", "with",
];

/// Check if a token is a stopword.
#[inline]
fn is_stopword(token: &str) -> bool {
    // Linear scan is fast for ~33 entries (all < cache line).
    STOPWORDS.contains(&token)
}

/// Strip English possessive suffix (`'s` / `\u{2019}s`).
#[inline]
fn strip_possessive(word: &str) -> &str {
    word.strip_suffix("'s")
        .or_else(|| word.strip_suffix("\u{2019}s"))
        .unwrap_or(word)
}

/// Tokenize text into searchable terms.
///
/// Pipeline (matches Lucene's StandardTokenizer + analysis chain):
/// 1. UAX#29 word boundaries (`unicode_words`)
/// 2. Strip English possessives (`'s`)
/// 3. Remove non-alphanumeric characters (e.g. internal apostrophes)
/// 4. Lowercase
/// 5. Filter tokens shorter than 2 characters
/// 6. Remove stopwords
/// 7. Porter-stem each token
///
/// # Example
///
/// ```
/// use strata_engine::search::tokenizer::tokenize;
///
/// let tokens = tokenize("The Quick Brown Foxes");
/// assert_eq!(tokens, vec!["quick", "brown", "fox"]);
/// ```
pub fn tokenize(text: &str) -> Vec<String> {
    text.unicode_words()
        .map(strip_possessive)
        .map(|w| {
            w.chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
        })
        .map(|w| w.to_lowercase())
        .filter(|s| s.len() >= 2)
        .filter(|s| !is_stopword(s))
        .map(|s| stemmer::stem(&s))
        .collect()
}

/// Tokenize and deduplicate for query processing.
///
/// # Example
///
/// ```
/// use strata_engine::search::tokenizer::tokenize_unique;
///
/// let tokens = tokenize_unique("testing tests TESTS");
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
        // "I" and "a" filtered (< 2 chars); "a" is also a stopword
        let tokens = tokenize("I am a test");
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
        // "test", "test", "test" all stem to "test" → deduplicated
        let tokens = tokenize_unique("test test TEST");
        assert_eq!(tokens, vec!["test"]);
    }

    #[test]
    fn test_tokenize_unique_preserves_order() {
        let tokens = tokenize_unique("apple banana apple cherry");
        assert_eq!(tokens, vec!["appl", "banana", "cherri"]);
    }

    // ------------------------------------------------------------------
    // Stopword tests
    // ------------------------------------------------------------------

    #[test]
    fn test_stopwords_removed() {
        let tokens = tokenize("the quick and the dead");
        // "the" (x2) and "and" are stopwords
        assert_eq!(tokens, vec!["quick", "dead"]);
    }

    #[test]
    fn test_all_stopwords() {
        let tokens = tokenize("the a an is are was");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_stopwords_case_insensitive() {
        let tokens = tokenize("The AND Not");
        // All are stopwords after lowercasing
        assert!(tokens.is_empty());
    }

    // ------------------------------------------------------------------
    // Stemming integration tests
    // ------------------------------------------------------------------

    #[test]
    fn test_stemming_applied() {
        let tokens = tokenize("running quickly");
        assert_eq!(tokens, vec!["run", "quickli"]);
    }

    #[test]
    fn test_stemming_morphological_variants() {
        // "treatments" and "treatment" should produce the same stem
        let t1 = tokenize("treatments");
        let t2 = tokenize("treatment");
        assert_eq!(t1, t2);
    }

    #[test]
    fn test_stemming_plurals() {
        let tokens = tokenize("infections diseases patients");
        assert_eq!(tokens, vec!["infect", "diseas", "patient"]);
    }

    #[test]
    fn test_full_pipeline() {
        // Combines stopword removal + stemming
        let tokens = tokenize("The treatment of bacterial infections in patients");
        // "the", "of", "in" are stopwords
        assert_eq!(tokens, vec!["treatment", "bacteri", "infect", "patient"]);
    }

    #[test]
    fn test_unique_after_stemming() {
        // "run", "running", "runs" all stem to "run"
        let tokens = tokenize_unique("run running runs");
        assert_eq!(tokens, vec!["run"]);
    }

    // ------------------------------------------------------------------
    // UAX#29 word boundary tests
    // ------------------------------------------------------------------

    #[test]
    fn test_contractions() {
        // UAX#29 keeps "don't" as one word; apostrophe stripped → "dont"
        let tokens = tokenize("don't stop");
        assert_eq!(tokens, vec!["dont", "stop"]);
    }

    #[test]
    fn test_possessives() {
        // Possessive filter strips 's before further processing
        let tokens = tokenize("John's book");
        assert_eq!(tokens, vec!["john", "book"]);
    }

    #[test]
    fn test_curly_possessive() {
        let tokens = tokenize("John\u{2019}s book");
        assert_eq!(tokens, vec!["john", "book"]);
    }

    #[test]
    fn test_decimal_numbers() {
        // UAX#29 keeps "3.14" as one word
        let tokens = tokenize("3.14 seconds");
        assert_eq!(tokens, vec!["314", "second"]);
    }

    #[test]
    fn test_abbreviation() {
        // UAX#29 keeps "U.S.A." together; dots stripped → "usa"
        let tokens = tokenize("U.S.A. policy");
        assert_eq!(tokens, vec!["usa", "polici"]);
    }

    #[test]
    fn test_hyphens() {
        // UAX#29 splits on hyphens
        let tokens = tokenize("state-of-the-art");
        // "of" and "the" are stopwords
        assert_eq!(tokens, vec!["state", "art"]);
    }

    #[test]
    fn test_email_like() {
        // UAX#29 keeps "example.com" as one word; dots stripped → "examplecom"
        let tokens = tokenize("user@example.com");
        assert!(tokens.contains(&"user".to_string()));
        assert!(tokens.contains(&"examplecom".to_string()));
    }
}
