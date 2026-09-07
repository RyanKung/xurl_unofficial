//! Algebraic errors for cookie GraphQL access.

use std::fmt;
use std::io;
use std::path::PathBuf;

/// Recoverable failure of the unofficial X client.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A required session cookie was not found.
    #[error("missing session field {0}")]
    MissingAuth(AuthField),
    /// Cookie file existed but could not be parsed as toml.
    #[error("invalid cookies file {path}: {source}")]
    CookiesFile {
        /// Path that failed to parse.
        path: PathBuf,
        /// Underlying toml error.
        #[source]
        source: toml::de::Error,
    },
    /// Filesystem failure at the auth boundary.
    #[error("io error at {path:?}: {source}")]
    Io {
        /// Path involved in the IO, if known.
        path: Option<PathBuf>,
        /// Underlying IO error.
        #[source]
        source: io::Error,
    },
    /// Screen name rejected by the constructor.
    #[error("invalid screen name")]
    InvalidScreenName,
    /// Post id rejected by the constructor.
    #[error("invalid post id")]
    InvalidPostId,
    /// Post text rejected by the constructor.
    #[error("invalid post text")]
    InvalidPostText,
    /// Bundled GraphQL catalog JSON is not usable.
    #[error("graphql catalog is corrupt: {0}")]
    CatalogCorrupt(&'static str),
    /// Named GraphQL operation is missing from the catalog.
    #[error("unknown graphql operation {0}")]
    UnknownOperation(&'static str),
    /// HTTP transport failure.
    #[error("http transport: {0}")]
    Transport(#[from] wreq::Error),
    /// `x-client-transaction-id` could not be generated from the live homepage.
    #[error("client transaction: {0}")]
    Transaction(String),
    /// Live JS bundles did not yield GraphQL query IDs.
    #[error("could not refresh graphql query ids")]
    BundleRefresh,
    /// HTTP header value could not be encoded.
    #[error("invalid http header")]
    InvalidHeader,
    /// X returned a non-success status.
    #[error("graphql status {status}: {body}")]
    GraphQlStatus {
        /// HTTP status code.
        status: u16,
        /// Truncated response body.
        body: String,
    },
    /// Response body was not JSON.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    /// GraphQL payload did not contain the expected user.
    #[error("user not found: {0}")]
    UserNotFound(String),
    /// GraphQL payload did not contain the expected tweet.
    #[error("tweet not found: {0}")]
    TweetNotFound(String),
    /// Search query was empty or too long.
    #[error("invalid search query")]
    InvalidSearchQuery,
    /// Page size was outside 1..=100.
    #[error("invalid page size")]
    InvalidPageSize,
    /// `sort_order` was not `recency` or `relevancy`.
    #[error("invalid sort order")]
    InvalidSortOrder,
    /// `start_time` / `end_time` was not YYYY-MM-DD or RFC3339.
    #[error("invalid search time")]
    InvalidSearchTime,
    /// `next_token` / `pagination_token` was empty or both were set.
    #[error("invalid pagination token")]
    InvalidPaginationToken,
    /// Media id was not a digit string.
    #[error("invalid media id")]
    InvalidMediaId,
    /// Upload path is missing, empty, too large, or an unsupported type.
    #[error("invalid media file")]
    InvalidMedia,
    /// More than four `--media-id` values.
    #[error("too many media ids")]
    TooManyMedia,
    /// HOME is unset so the default cookies path cannot be formed.
    #[error("HOME is unset")]
    MissingHome,
    /// Cookie file could not be serialized.
    #[error("cannot write cookies file: {0}")]
    CookiesSerialize(#[from] toml::ser::Error),
    /// Stdin is not a terminal, so interactive auth cannot run.
    #[error("auth requires an interactive terminal")]
    AuthNotInteractive,
    /// Default browser could not be opened.
    #[error("cannot open browser: {0}")]
    BrowserOpen(String),
    /// Chrome cookie import failed. The message must not contain cookie values.
    #[error("cannot import Chrome cookies: {0}")]
    ChromeImport(String),
    /// Operator declined to overwrite an existing cookies file.
    #[error("auth cancelled; existing cookies were not overwritten")]
    AuthCancelled,
}

/// Session cookie field required for GraphQL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthField {
    /// `auth_token` cookie.
    AuthToken,
    /// `ct0` CSRF cookie.
    Ct0,
}

impl fmt::Display for AuthField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AuthToken => write!(f, "auth_token"),
            Self::Ct0 => write!(f, "ct0"),
        }
    }
}

/// Truncate a response body on a UTF-8 boundary.
pub fn body_preview(body: &str, max_chars: usize) -> String {
    match body.char_indices().nth(max_chars) {
        None => body.to_string(),
        Some((byte_index, _)) => match body.get(..byte_index) {
            Some(head) => format!("{head}... (truncated)"),
            None => body.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::body_preview;

    #[test]
    fn body_preview_keeps_short_text() {
        assert_eq!(body_preview("abc", 10), "abc");
    }

    #[test]
    fn body_preview_truncates_on_char_boundary() {
        let preview = body_preview("你好世界", 2);
        assert!(preview.ends_with("... (truncated)"));
        assert!(preview.starts_with("你好"));
    }
}
