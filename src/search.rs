//! Map official Search API knobs onto web SearchTimeline.

use crate::error::Error;
use crate::types::SearchQuery;

/// Official `sort_order` for `/2/tweets/search/recent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    /// Newest first (`product=Latest`).
    Recency,
    /// Ranked / Top (`product=Top`).
    Relevancy,
}

impl SortOrder {
    /// Parse official enum values.
    pub fn parse(raw: &str) -> Result<Self, Error> {
        match raw {
            "recency" => Ok(Self::Recency),
            "relevancy" => Ok(Self::Relevancy),
            _ => Err(Error::InvalidSortOrder),
        }
    }

    /// GraphQL SearchTimeline `product`.
    pub fn product(self) -> &'static str {
        match self {
            Self::Recency => "Latest",
            Self::Relevancy => "Top",
        }
    }
}

/// Rewrite official query operators and optional time bounds for web search.
pub fn compile_query(
    raw: &str,
    start_time: Option<&str>,
    end_time: Option<&str>,
) -> Result<SearchQuery, Error> {
    let mut query = rewrite_api_operators(raw.trim());
    if let Some(start) = start_time {
        append_operator(&mut query, "since:", &ymd_prefix(start)?);
    }
    if let Some(end) = end_time {
        append_operator(&mut query, "until:", &ymd_prefix(end)?);
    }
    SearchQuery::parse(&query)
}

/// Official `next_token` / `pagination_token` (mutually exclusive).
pub fn pick_pagination_token(
    next_token: Option<&str>,
    pagination_token: Option<&str>,
) -> Result<Option<String>, Error> {
    match (next_token, pagination_token) {
        (Some(_), Some(_)) => Err(Error::InvalidPaginationToken),
        (Some(token), None) | (None, Some(token)) => {
            let trimmed = token.trim();
            if trimmed.is_empty() {
                Err(Error::InvalidPaginationToken)
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
        (None, None) => Ok(None),
    }
}

fn rewrite_api_operators(raw: &str) -> String {
    let mut query = raw.to_string();
    for (from, to) in [
        ("min_likes:", "min_faves:"),
        ("min_reposts:", "min_retweets:"),
        ("-is:retweet", "-filter:nativeretweets"),
        ("is:retweet", "filter:nativeretweets"),
        ("-is:reply", "-filter:replies"),
        ("is:reply", "filter:replies"),
    ] {
        query = query.replace(from, to);
    }
    query
}

fn append_operator(query: &mut String, operator: &str, value: &str) {
    if query.contains(operator) {
        return;
    }
    if !query.is_empty() {
        query.push(' ');
    }
    query.push_str(operator);
    query.push_str(value);
}

fn ymd_prefix(raw: &str) -> Result<String, Error> {
    let date = raw.get(..10).ok_or(Error::InvalidSearchTime)?;
    if is_ymd(date) {
        Ok(date.to_string())
    } else {
        Err(Error::InvalidSearchTime)
    }
}

fn is_ymd(value: &str) -> bool {
    let bytes = value.as_bytes();
    matches!(bytes, [y0, y1, y2, y3, b'-', m0, m1, b'-', d0, d1]
        if y0.is_ascii_digit()
            && y1.is_ascii_digit()
            && y2.is_ascii_digit()
            && y3.is_ascii_digit()
            && m0.is_ascii_digit()
            && m1.is_ascii_digit()
            && d0.is_ascii_digit()
            && d1.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::{compile_query, pick_pagination_token, SortOrder};

    #[test]
    fn sort_order_maps_to_graphql_product() {
        assert_eq!(
            SortOrder::parse("recency").ok().map(SortOrder::product),
            Some("Latest")
        );
        assert_eq!(
            SortOrder::parse("relevancy").ok().map(SortOrder::product),
            Some("Top")
        );
        assert!(SortOrder::parse("hot").is_err());
    }

    #[test]
    fn compile_rewrites_official_operators_and_dates() {
        let query = compile_query(
            "crypto lang:en -is:retweet min_likes:100",
            Some("2026-09-01T00:00:00Z"),
            Some("2026-09-07"),
        );
        assert_eq!(
            query.ok().map(|q| q.as_str().to_string()),
            Some(
                "crypto lang:en -filter:nativeretweets min_faves:100 since:2026-09-01 until:2026-09-07"
                    .to_string()
            )
        );
    }

    #[test]
    fn compile_keeps_existing_since() {
        let query = compile_query("btc since:2026-09-06", Some("2026-01-01"), None);
        assert_eq!(
            query.ok().map(|q| q.as_str().to_string()),
            Some("btc since:2026-09-06".to_string())
        );
    }

    #[test]
    fn pagination_tokens_are_exclusive() {
        assert!(pick_pagination_token(Some("a"), Some("b")).is_err());
        assert_eq!(
            pick_pagination_token(Some(" abc "), None).ok().flatten(),
            Some("abc".to_string())
        );
        assert_eq!(pick_pagination_token(None, None).ok(), Some(None));
    }
}
