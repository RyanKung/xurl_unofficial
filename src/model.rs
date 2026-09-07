//! Domain records returned to the CLI.

use serde::Serialize;

/// Public user fields from `UserByScreenName` / verify_credentials.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct User {
    /// Numeric rest id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Handle without `@`.
    pub username: String,
}

/// Post fields used by xurl-shaped JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Tweet {
    /// Numeric rest id.
    pub id: String,
    /// Full text.
    pub text: String,
    /// Optional Twitter timestamp string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

/// Uploaded media identifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Media {
    /// `media_id_string` from INIT/FINALIZE/STATUS.
    pub media_id: String,
    /// Optional processing state (`pending`, `succeeded`, `failed`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
}

/// Direct message summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DirectMessage {
    /// Message id.
    pub id: String,
    /// Message text.
    pub text: String,
    /// Sender rest id, if present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender_id: Option<String>,
    /// Conversation id, if present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
}
