//! Validated identifiers used by X GraphQL operations.

use crate::error::Error;

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
        let candidate = status_id_from_url(trimmed).unwrap_or(trimmed);
        if !is_valid_post_id(candidate) {
            return Err(Error::InvalidPostId);
        }
        Ok(Self(candidate.to_string()))
    }

    /// Borrow the digit string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn status_id_from_url(raw: &str) -> Option<&str> {
    let after_status = raw.split("/status/").nth(1)?;
    let id = after_status
        .split(|c: char| !c.is_ascii_digit())
        .next()
        .unwrap_or("");
    if id.is_empty() {
        None
    } else {
        Some(id)
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
    use super::{parse_media_ids, MediaId, PageSize, PostId, PostText, ScreenName, SearchQuery};

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
        assert_eq!(
            PostId::parse(url).ok().map(|id| id.as_str().to_string()),
            Some("1234567890".to_string())
        );
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
