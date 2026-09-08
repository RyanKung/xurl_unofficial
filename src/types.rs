//! Validated identifiers used by X GraphQL operations.

use crate::error::Error;

const STANDARD_TWEET_MAX_WEIGHTED_LENGTH: usize = 280;
const URL_WEIGHTED_LENGTH: usize = 23;

/// X handle without a leading `@`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenName(String);

impl ScreenName {
    /// Parse a handle, stripping a single leading `@`.
    pub fn parse(raw: &str) -> Result<Self, Error> {
        let trimmed = raw.trim();
        let without_at = trimmed.strip_prefix('@').unwrap_or(trimmed);
        if !is_valid_screen_name(without_at) {
            return Err(Error::InvalidScreenName);
        }
        Ok(Self(without_at.to_string()))
    }

    /// Borrow the canonical handle.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn is_valid_screen_name(name: &str) -> bool {
    let len = name.len();
    (1..=15).contains(&len) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Digit-only post identifier, optionally extracted from an x.com status URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostId(String);

impl PostId {
    /// Parse a numeric id or a status URL.
    pub fn parse(raw: &str) -> Result<Self, Error> {
        let trimmed = raw.trim();
        let parsed = status_from_url(trimmed);
        let candidate = parsed.as_ref().map(|status| status.id).unwrap_or(trimmed);
        if !is_valid_post_id(candidate) {
            return Err(Error::InvalidPostId);
        }
        Ok(Self(candidate.to_string()))
    }

    /// Borrow the digit string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// URL suitable for GraphQL `attachment_url` when quote-posting.
    pub fn attachment_url(&self) -> String {
        format!("https://x.com/i/status/{}", self.0)
    }
}

struct ParsedStatus<'a> {
    id: &'a str,
}

fn status_from_url(raw: &str) -> Option<ParsedStatus<'_>> {
    let scheme_end = raw.find("://")?;
    let after_scheme = raw.get((scheme_end + 3)..)?;
    let mut parts = after_scheme.split('/');
    let host = parts.next()?;
    if !matches!(
        host,
        "x.com" | "twitter.com" | "www.x.com" | "www.twitter.com"
    ) {
        return None;
    }
    let handle = parts.next()?;
    let marker = parts.next()?;
    if marker != "status" || !is_valid_screen_name(handle) {
        return None;
    }
    let after_status = raw.split("/status/").nth(1)?;
    let id = after_status
        .split(|c: char| !c.is_ascii_digit())
        .next()
        .unwrap_or("");
    if id.is_empty() {
        None
    } else {
        Some(ParsedStatus { id })
    }
}

fn is_valid_post_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_digit())
}

/// Non-empty post body. Upper bound is Premium-sized, not Free-tier 280.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostText(String);

impl PostText {
    /// Reject empty or oversized text.
    pub fn parse(raw: &str) -> Result<Self, Error> {
        let trimmed = raw.trim();
        if !is_valid_post_text(trimmed) {
            return Err(Error::InvalidPostText);
        }
        Ok(Self(trimmed.to_string()))
    }

    /// Borrow the text.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether Web CreateTweet must use the long-form NoteTweet payload.
    pub fn requires_note_tweet(&self) -> bool {
        weighted_tweet_length(&self.0) > STANDARD_TWEET_MAX_WEIGHTED_LENGTH
    }
}

/// Approximate X's tweet length weighting: URLs count as t.co links.
pub fn weighted_tweet_length(text: &str) -> usize {
    let mut total = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((index, _ch)) = chars.next() {
        if starts_url_at(text, index) {
            total += URL_WEIGHTED_LENGTH;
            consume_url_tail(&mut chars);
        } else {
            total += 1;
        }
    }
    total
}

fn starts_url_at(text: &str, index: usize) -> bool {
    text.get(index..)
        .map(|tail| tail.starts_with("https://") || tail.starts_with("http://"))
        .unwrap_or(false)
}

fn consume_url_tail(chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>) {
    while let Some((_index, ch)) = chars.peek() {
        if ch.is_whitespace() {
            break;
        }
        let _ = chars.next();
    }
}

fn is_valid_post_text(text: &str) -> bool {
    let chars = text.chars().count();
    (1..=25_000).contains(&chars)
}

/// Result count for search and timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageSize(u32);

impl PageSize {
    /// Accept 1 through 100 inclusive.
    pub fn parse(raw: u32) -> Result<Self, Error> {
        if is_valid_page_size(raw) {
            Ok(Self(raw))
        } else {
            Err(Error::InvalidPageSize)
        }
    }

    /// Borrow the count for GraphQL `count`.
    pub fn get(self) -> u32 {
        self.0
    }
}

fn is_valid_page_size(n: u32) -> bool {
    (1..=100).contains(&n)
}

/// Non-empty search string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery(String);

impl SearchQuery {
    /// Reject blank queries.
    pub fn parse(raw: &str) -> Result<Self, Error> {
        let trimmed = raw.trim();
        if !is_valid_search_query(trimmed) {
            return Err(Error::InvalidSearchQuery);
        }
        Ok(Self(trimmed.to_string()))
    }

    /// Borrow the query.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn is_valid_search_query(query: &str) -> bool {
    let chars = query.chars().count();
    (1..=512).contains(&chars)
}

/// Digit-only media identifier from INIT.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaId(String);

impl MediaId {
    /// Parse a numeric media id.
    pub fn parse(raw: &str) -> Result<Self, Error> {
        let trimmed = raw.trim();
        if !is_valid_post_id(trimmed) {
            return Err(Error::InvalidMediaId);
        }
        Ok(Self(trimmed.to_string()))
    }

    /// Borrow the digit string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// At most four media ids for CreateTweet.
pub fn parse_media_ids(raw: &[String]) -> Result<Vec<MediaId>, Error> {
    if raw.len() > 4 {
        return Err(Error::TooManyMedia);
    }
    raw.iter().map(|item| MediaId::parse(item)).collect()
}

#[cfg(test)]
mod tests {
    use super::{
        parse_media_ids, weighted_tweet_length, MediaId, PageSize, PostId, PostText, ScreenName,
        SearchQuery,
    };

    #[test]
    fn screen_name_strips_at_and_rejects_empty() {
        assert_eq!(
            ScreenName::parse("@alice")
                .ok()
                .map(|n| n.as_str().to_string()),
            Some("alice".to_string())
        );
        assert!(ScreenName::parse("").is_err());
        assert!(ScreenName::parse("not a handle").is_err());
    }

    #[test]
    fn post_id_extracts_from_url() {
        let url = "https://x.com/user/status/1234567890?s=20";
        let parsed = PostId::parse(url);
        assert!(parsed.is_ok());
        if let Ok(id) = parsed {
            assert_eq!(id.as_str(), "1234567890");
            assert_eq!(id.attachment_url(), "https://x.com/i/status/1234567890");
        }
        assert!(PostId::parse("abc").is_err());
    }

    #[test]
    fn post_text_rejects_blank() {
        assert!(PostText::parse("  ").is_err());
        assert_eq!(
            PostText::parse("hi")
                .ok()
                .map(|text| text.as_str().to_string()),
            Some("hi".to_string())
        );
    }

    #[test]
    fn post_text_marks_longform() {
        let short = PostText::parse("x".repeat(280).as_str());
        assert!(short.is_ok());
        if let Ok(short) = short {
            assert!(!short.requires_note_tweet());
        }
        let long = PostText::parse("x".repeat(281).as_str());
        assert!(long.is_ok());
        if let Ok(long) = long {
            assert!(long.requires_note_tweet());
        }
    }

    #[test]
    fn weighted_tweet_length_counts_urls_as_tco_links() {
        let text = format!("{} https://example.com/{}", "b".repeat(250), "x".repeat(80));
        assert_eq!(weighted_tweet_length(&text), 274);
        let post = PostText::parse(&text);
        assert!(post.is_ok());
        if let Ok(post) = post {
            assert!(!post.requires_note_tweet());
        }
    }

    #[test]
    fn page_size_rejects_zero() {
        assert!(PageSize::parse(0).is_err());
        assert_eq!(PageSize::parse(10).ok().map(PageSize::get), Some(10));
    }

    #[test]
    fn search_query_rejects_blank() {
        assert!(SearchQuery::parse("  ").is_err());
        assert_eq!(
            SearchQuery::parse("from:a")
                .ok()
                .map(|q| q.as_str().to_string()),
            Some("from:a".to_string())
        );
    }

    #[test]
    fn media_id_rejects_non_digits() {
        assert!(MediaId::parse("abc").is_err());
        assert_eq!(
            MediaId::parse("99").ok().map(|id| id.as_str().to_string()),
            Some("99".to_string())
        );
        assert!(
            parse_media_ids(&["1".into(), "2".into(), "3".into(), "4".into(), "5".into()]).is_err()
        );
    }
}
