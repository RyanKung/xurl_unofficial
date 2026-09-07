//! Pure JSON projections from GraphQL payloads.

use serde_json::Value;

use crate::error::Error;
use crate::model::{DirectMessage, Media, Tweet, User};

/// Read `data.user.result` from UserByScreenName.
pub fn user_from_screen_name_payload(data: &Value, screen_name: &str) -> Result<User, Error> {
    let user = data
        .get("data")
        .and_then(|d| d.get("user"))
        .and_then(|u| u.get("result"))
        .ok_or_else(|| Error::UserNotFound(screen_name.to_string()))?;
    if typename_is(user, "UserUnavailable") {
        return Err(Error::UserNotFound(screen_name.to_string()));
    }
    user_from_result(user).ok_or_else(|| Error::UserNotFound(screen_name.to_string()))
}

/// Read `data.viewer.user_results.result` from Viewer.
pub fn user_from_viewer_payload(data: &Value) -> Result<User, Error> {
    let user = data
        .get("data")
        .and_then(|d| d.get("viewer"))
        .and_then(|v| v.get("user_results"))
        .and_then(|u| u.get("result"))
        .ok_or_else(|| Error::UserNotFound("me".to_string()))?;
    if typename_is(user, "UserUnavailable") {
        return Err(Error::UserNotFound("me".to_string()));
    }
    user_from_result(user).ok_or_else(|| Error::UserNotFound("me".to_string()))
}

/// Read `screen_name` from account/settings.json.
pub fn screen_name_from_settings(data: &Value) -> Result<String, Error> {
    string_field(data, "screen_name").ok_or_else(|| Error::UserNotFound("me".to_string()))
}

/// Read account/verify_credentials.json.
pub fn user_from_verify_credentials(data: &Value) -> Result<User, Error> {
    let id = string_field(data, "id_str").or_else(|| string_field(data, "id"));
    let name = string_field(data, "name");
    let username = string_field(data, "screen_name");
    match (id, name, username) {
        (Some(id), Some(name), Some(username)) => Ok(User { id, name, username }),
        _ => Err(Error::UserNotFound("me".to_string())),
    }
}

/// Read CreateTweet `data.create_tweet.tweet_results.result`.
pub fn tweet_from_create_payload(data: &Value) -> Result<Tweet, Error> {
    let result = data
        .get("data")
        .and_then(|d| d.get("create_tweet"))
        .and_then(|c| c.get("tweet_results"))
        .and_then(|t| t.get("result"))
        .ok_or_else(|| Error::TweetNotFound("create".to_string()))?;
    tweet_from_result(result).ok_or_else(|| Error::TweetNotFound("create".to_string()))
}

/// Find a tweet whose rest_id matches `post_id`.
pub fn tweet_from_detail_payload(data: &Value, post_id: &str) -> Result<Tweet, Error> {
    let mut found = Vec::new();
    collect_tweets(data, &mut found);
    found
        .into_iter()
        .find(|tweet| tweet.id == post_id)
        .ok_or_else(|| Error::TweetNotFound(post_id.to_string()))
}

/// Collect tweets from a timeline or search payload, capped at `limit`.
pub fn tweets_from_timeline_payload(data: &Value, limit: usize) -> Vec<Tweet> {
    let mut found = Vec::new();
    collect_tweets(data, &mut found);
    found.into_iter().take(limit).collect()
}

/// Tweets plus the Bottom cursor (`next_token`).
pub fn timeline_page(data: &Value, limit: usize) -> (Vec<Tweet>, Option<String>) {
    (
        tweets_from_timeline_payload(data, limit),
        bottom_cursor(data),
    )
}

/// Users plus the Bottom cursor (`next_token`).
pub fn user_page(data: &Value, limit: usize) -> (Vec<User>, Option<String>) {
    let mut found = Vec::new();
    collect_users(data, &mut found);
    (found.into_iter().take(limit).collect(), bottom_cursor(data))
}

fn bottom_cursor(value: &Value) -> Option<String> {
    let mut found = None;
    walk_bottom_cursor(value, &mut found);
    found
}

fn walk_bottom_cursor(value: &Value, found: &mut Option<String>) {
    if found.is_some() {
        return;
    }
    match value {
        Value::Object(map) => {
            let is_bottom = map
                .get("cursorType")
                .and_then(Value::as_str)
                .map(|kind| kind == "Bottom")
                .unwrap_or(false);
            if is_bottom {
                if let Some(cursor) = map.get("value").and_then(Value::as_str) {
                    if !cursor.is_empty() {
                        *found = Some(cursor.to_string());
                        return;
                    }
                }
            }
            for child in map.values() {
                walk_bottom_cursor(child, found);
            }
        }
        Value::Array(items) => {
            for child in items {
                walk_bottom_cursor(child, found);
            }
        }
        _ => {}
    }
}

fn typename_is(value: &Value, expected: &str) -> bool {
    value
        .get("__typename")
        .and_then(Value::as_str)
        .map(|name| name == expected)
        .unwrap_or(false)
}

fn user_from_result(user: &Value) -> Option<User> {
    let id = string_field(user, "rest_id")?;
    let profile = user.get("legacy").or_else(|| user.get("core"))?;
    let name = string_field(profile, "name")?;
    let username = string_field(profile, "screen_name")?;
    Some(User { id, name, username })
}

fn tweet_from_result(result: &Value) -> Option<Tweet> {
    let core = result.get("tweet").or(Some(result));
    let node = core?;
    let id = string_field(node, "rest_id").or_else(|| {
        node.get("legacy")
            .and_then(|legacy| string_field(legacy, "id_str"))
    })?;
    let legacy = node.get("legacy")?;
    let text = string_field(legacy, "full_text")?;
    let created_at = string_field(legacy, "created_at");
    Some(Tweet {
        id,
        text,
        created_at,
    })
}

fn collect_tweets(data: &Value, out: &mut Vec<Tweet>) {
    if let Some(tweet) = tweet_from_result(data) {
        if out.iter().all(|existing| existing.id != tweet.id) {
            out.push(tweet);
        }
        return;
    }
    match data {
        Value::Object(map) => {
            for child in map.values() {
                collect_tweets(child, out);
            }
        }
        Value::Array(items) => {
            for child in items {
                collect_tweets(child, out);
            }
        }
        _ => {}
    }
}

fn collect_users(data: &Value, out: &mut Vec<User>) {
    if let Some(user) = user_from_result(data) {
        if out.iter().all(|existing| existing.id != user.id) {
            out.push(user);
        }
        return;
    }
    match data {
        Value::Object(map) => {
            for child in map.values() {
                collect_users(child, out);
            }
        }
        Value::Array(items) => {
            for child in items {
                collect_users(child, out);
            }
        }
        _ => {}
    }
}

/// INIT/FINALIZE/STATUS `media_id_string`.
pub fn media_from_upload(data: &Value) -> Result<Media, Error> {
    let media_id = string_field(data, "media_id_string")
        .or_else(|| data.get("media_id").and_then(json_id))
        .ok_or(Error::InvalidMedia)?;
    let state = data
        .get("processing_info")
        .and_then(|info| string_field(info, "state"));
    Ok(Media { media_id, state })
}

/// Collect DM entries from inbox_initial_state.
pub fn dms_from_inbox(data: &Value, limit: usize) -> Vec<DirectMessage> {
    let mut found = Vec::new();
    collect_dms(data, &mut found);
    found.into_iter().take(limit).collect()
}

fn collect_dms(data: &Value, out: &mut Vec<DirectMessage>) {
    if let Some(message) = dm_from_value(data) {
        if out.iter().all(|existing| existing.id != message.id) {
            out.push(message);
        }
        return;
    }
    match data {
        Value::Object(map) => {
            for child in map.values() {
                collect_dms(child, out);
            }
        }
        Value::Array(items) => {
            for child in items {
                collect_dms(child, out);
            }
        }
        _ => {}
    }
}

fn dm_from_value(value: &Value) -> Option<DirectMessage> {
    let data = value.get("message_data").unwrap_or(value);
    let id = string_field(data, "id").or_else(|| string_field(value, "id"))?;
    let text = string_field(data, "text")?;
    if text.is_empty() {
        return None;
    }
    Some(DirectMessage {
        id,
        text,
        sender_id: string_field(data, "sender_id"),
        conversation_id: string_field(data, "conversation_id")
            .or_else(|| string_field(value, "conversation_id")),
    })
}

fn json_id(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return Some(text.to_string());
    }
    value.as_u64().map(|n| n.to_string())
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::{
        dms_from_inbox, media_from_upload, screen_name_from_settings, timeline_page,
        tweet_from_create_payload, tweets_from_timeline_payload, user_from_screen_name_payload,
        user_from_verify_credentials, user_from_viewer_payload, user_page,
    };
    use serde_json::json;

    #[test]
    fn user_payload_reads_rest_id_and_handle() {
        let payload = json!({
            "data": {
                "user": {
                    "result": {
                        "rest_id": "1",
                        "legacy": { "name": "Ryan", "screen_name": "alice" }
                    }
                }
            }
        });
        let user = user_from_screen_name_payload(&payload, "alice");
        assert!(user.is_ok());
        if let Ok(user) = user {
            assert_eq!(user.id, "1");
            assert_eq!(user.username, "alice");
        }
    }

    #[test]
    fn create_payload_reads_full_text() {
        let payload = json!({
            "data": {
                "create_tweet": {
                    "tweet_results": {
                        "result": {
                            "rest_id": "42",
                            "legacy": { "full_text": "hello", "created_at": "now" }
                        }
                    }
                }
            }
        });
        let tweet = tweet_from_create_payload(&payload);
        assert!(tweet.is_ok());
        if let Ok(tweet) = tweet {
            assert_eq!(tweet.id, "42");
            assert_eq!(tweet.text, "hello");
        }
    }

    #[test]
    fn verify_credentials_reads_id_str_and_screen_name() {
        let payload = json!({
            "id_str": "1",
            "name": "Ryan",
            "screen_name": "alice"
        });
        let user = user_from_verify_credentials(&payload);
        assert!(user.is_ok());
        if let Ok(user) = user {
            assert_eq!(user.id, "1");
            assert_eq!(user.username, "alice");
        }
    }

    #[test]
    fn settings_payload_reads_screen_name() {
        let payload = json!({ "screen_name": "alice" });
        let name = screen_name_from_settings(&payload);
        assert!(name.is_ok());
        if let Ok(name) = name {
            assert_eq!(name, "alice");
        }
    }

    #[test]
    fn viewer_payload_reads_core_profile() {
        let payload = json!({
            "data": {
                "viewer": {
                    "user_results": {
                        "result": {
                            "rest_id": "1",
                            "core": { "name": "Ryan", "screen_name": "alice" }
                        }
                    }
                }
            }
        });
        let user = user_from_viewer_payload(&payload);
        assert!(user.is_ok());
        if let Ok(user) = user {
            assert_eq!(user.id, "1");
            assert_eq!(user.username, "alice");
        }
    }

    #[test]
    fn timeline_payload_collects_nested_tweets() {
        let payload = json!({
            "data": {
                "search_by_raw_query": {
                    "search_timeline": {
                        "timeline": {
                            "instructions": [{
                                "entries": [{
                                    "content": {
                                        "itemContent": {
                                            "tweet_results": {
                                                "result": {
                                                    "rest_id": "99",
                                                    "legacy": { "full_text": "hit", "created_at": "now" }
                                                }
                                            }
                                        }
                                    }
                                }, {
                                    "content": {
                                        "cursorType": "Bottom",
                                        "value": "bottom-cursor"
                                    }
                                }]
                            }]
                        }
                    }
                }
            }
        });
        let tweets = tweets_from_timeline_payload(&payload, 10);
        assert_eq!(tweets.len(), 1);
        if let Some(tweet) = tweets.first() {
            assert_eq!(tweet.id, "99");
            assert_eq!(tweet.text, "hit");
        }
        let (_tweets, next) = timeline_page(&payload, 10);
        assert_eq!(next.as_deref(), Some("bottom-cursor"));
    }

    #[test]
    fn user_page_reads_following_entries() {
        let payload = json!({
            "data": {
                "user": {
                    "result": {
                        "timeline": {
                            "timeline": {
                                "instructions": [{
                                    "entries": [{
                                        "content": {
                                            "itemContent": {
                                                "user_results": {
                                                    "result": {
                                                        "rest_id": "7",
                                                        "core": { "name": "Ada", "screen_name": "ada" }
                                                    }
                                                }
                                            }
                                        }
                                    }]
                                }]
                            }
                        }
                    }
                }
            }
        });
        let (users, _) = user_page(&payload, 10);
        assert_eq!(users.len(), 1);
        if let Some(user) = users.first() {
            assert_eq!(user.username, "ada");
        }
    }

    #[test]
    fn media_from_upload_reads_string_id() {
        let payload = json!({
            "media_id": 99,
            "media_id_string": "99",
            "processing_info": { "state": "succeeded" }
        });
        let media = media_from_upload(&payload);
        assert!(media.is_ok());
        if let Ok(media) = media {
            assert_eq!(media.media_id, "99");
            assert_eq!(media.state.as_deref(), Some("succeeded"));
        }
    }

    #[test]
    fn dms_from_inbox_reads_message_data() {
        let payload = json!({
            "inbox_initial_state": {
                "entries": [{
                    "message": {
                        "message_data": {
                            "id": "8",
                            "text": "hi",
                            "sender_id": "1",
                            "conversation_id": "1-2"
                        }
                    }
                }]
            }
        });
        let dms = dms_from_inbox(&payload, 10);
        assert_eq!(dms.len(), 1);
        if let Some(dm) = dms.first() {
            assert_eq!(dm.text, "hi");
        }
    }
}
