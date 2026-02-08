//! Mock query expander for deterministic testing

use super::{ExpandError, ExpandedQueries, ExpandedQuery, QueryExpander, QueryType};

/// Mock expander that returns hand-crafted expansions.
///
/// Generates deterministic lex/vec/hyde expansions from any query,
/// enabling testing of the multi-query pipeline without a real model.
pub struct MockExpander;

impl QueryExpander for MockExpander {
    fn expand(&self, query: &str) -> Result<ExpandedQueries, ExpandError> {
        Ok(ExpandedQueries {
            queries: vec![
                ExpandedQuery {
                    query_type: QueryType::Lex,
                    text: query.to_string(),
                },
                ExpandedQuery {
                    query_type: QueryType::Vec,
                    text: format!("information about {}", query),
                },
                ExpandedQuery {
                    query_type: QueryType::Hyde,
                    text: format!(
                        "This document contains detailed information about {}. \
                         It covers the key aspects and provides relevant context.",
                        query
                    ),
                },
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_expander_returns_all_types() {
        let expander = MockExpander;
        let result = expander.expand("test query").unwrap();

        assert_eq!(result.queries.len(), 3);
        assert_eq!(result.queries[0].query_type, QueryType::Lex);
        assert_eq!(result.queries[1].query_type, QueryType::Vec);
        assert_eq!(result.queries[2].query_type, QueryType::Hyde);
    }

    #[test]
    fn test_mock_expander_includes_original_query() {
        let expander = MockExpander;
        let result = expander.expand("user authentication").unwrap();

        assert_eq!(result.queries[0].text, "user authentication");
        assert!(result.queries[1].text.contains("user authentication"));
        assert!(result.queries[2].text.contains("user authentication"));
    }
}
