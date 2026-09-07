//! Named X operations composed from HTTP + parsers.

use serde_json::json;

use crate::auth::SessionCookies;
use crate::error::Error;
use crate::http::Http;
use crate::media::{kind_for_path, validate_bytes, SEGMENT_BYTES};
use crate::model::{DirectMessage, Media, Tweet, User};
use crate::parse::{
    dms_from_inbox, media_from_upload, timeline_page, tweet_from_create_payload,
    tweet_from_detail_payload, user_from_screen_name_payload, user_from_viewer_payload, user_page,
};
use crate::search::SortOrder;
use crate::types::{MediaId, PageSize, PostId, PostText, ScreenName, SearchQuery};
use std::fs;
use std::path::Path;

/// High-level client. Effects stay in [`Http`].
pub struct XClient {
    http: Http,
}

impl XClient {
    /// Construct from session cookies.
    pub fn new(session: SessionCookies) -> Result<Self, Error> {
        Ok(Self {
            http: Http::new(session)?,
        })
    }

    /// Current account via GraphQL Viewer.
    pub async fn whoami(&self) -> Result<User, Error> {
        let variables = json!({ "withCommunitiesMemberships": true });
        let payload = self.http.graphql("Viewer", variables).await?;
        user_from_viewer_payload(&payload)
    }

    /// Look up a user by handle.
    pub async fn user(&self, name: &ScreenName) -> Result<User, Error> {
        let variables = json!({
            "screen_name": name.as_str(),
            "withSafetyModeUserFields": true,
        });
        let payload = self.http.graphql("UserByScreenName", variables).await?;
        user_from_screen_name_payload(&payload, name.as_str())
    }

    /// Read a post by rest id.
    pub async fn read(&self, post_id: &PostId) -> Result<Tweet, Error> {
        let variables = json!({
            "focalTweetId": post_id.as_str(),
            "referrer": "tweet",
            "with_rux_injections": false,
            "rankingMode": "Relevance",
            "includePromotedContent": true,
            "withCommunity": true,
            "withQuickPromoteEligibilityTweetFields": true,
            "withBirdwatchNotes": true,
            "withVoice": true,
        });
        let payload = self.http.graphql("TweetDetail", variables).await?;
        tweet_from_detail_payload(&payload, post_id.as_str())
    }

    /// Search posts. `sort` maps official `sort_order` onto SearchTimeline product.
    pub async fn search(
        &self,
        query: &SearchQuery,
        limit: PageSize,
        sort: SortOrder,
        cursor: Option<&str>,
    ) -> Result<(Vec<Tweet>, Option<String>), Error> {
        let mut variables = json!({
            "rawQuery": query.as_str(),
            "count": limit.get(),
            "querySource": "typed_query",
            "product": sort.product(),
        });
        insert_cursor(&mut variables, cursor);
        let payload = self.http.graphql("SearchTimeline", variables).await?;
        Ok(timeline_page(&payload, limit.get() as usize))
    }

    /// Home latest timeline for the session.
    pub async fn timeline(
        &self,
        limit: PageSize,
        cursor: Option<&str>,
    ) -> Result<(Vec<Tweet>, Option<String>), Error> {
        let mut variables = json!({
            "count": limit.get(),
            "includePromotedContent": true,
            "latestControlAvailable": true,
            "requestContext": "launch",
        });
        insert_cursor(&mut variables, cursor);
        let payload = self.http.graphql("HomeLatestTimeline", variables).await?;
        Ok(timeline_page(&payload, limit.get() as usize))
    }

    /// Create a root post.
    pub async fn post(&self, text: &PostText, media: &[MediaId]) -> Result<Tweet, Error> {
        self.create_tweet(text, None, None, media).await
    }

    /// Reply to an existing post.
    pub async fn reply(
        &self,
        post_id: &PostId,
        text: &PostText,
        media: &[MediaId],
    ) -> Result<Tweet, Error> {
        self.create_tweet(text, Some(post_id), None, media).await
    }

    /// Quote an existing post.
    pub async fn quote(
        &self,
        post_id: &PostId,
        text: &PostText,
        media: &[MediaId],
    ) -> Result<Tweet, Error> {
        self.create_tweet(text, None, Some(post_id), media).await
    }

    /// Delete a post owned by the session.
    pub async fn delete(&self, post_id: &PostId) -> Result<(), Error> {
        let variables = json!({
            "tweet_id": post_id.as_str(),
            "dark_request": false,
        });
        let _payload = self.http.graphql("DeleteTweet", variables).await?;
        Ok(())
    }

    /// Favorite a post.
    pub async fn like(&self, post_id: &PostId) -> Result<(), Error> {
        self.tweet_id_mutation("FavoriteTweet", post_id).await
    }

    /// Remove a favorite.
    pub async fn unlike(&self, post_id: &PostId) -> Result<(), Error> {
        self.tweet_id_mutation("UnfavoriteTweet", post_id).await
    }

    /// Repost a post.
    pub async fn repost(&self, post_id: &PostId) -> Result<(), Error> {
        self.tweet_id_mutation("CreateRetweet", post_id).await
    }

    /// Undo a repost.
    pub async fn unrepost(&self, post_id: &PostId) -> Result<(), Error> {
        let variables = json!({
            "source_tweet_id": post_id.as_str(),
            "dark_request": false,
        });
        let _payload = self.http.graphql("DeleteRetweet", variables).await?;
        Ok(())
    }

    /// Follow a user by handle.
    pub async fn follow(&self, name: &ScreenName) -> Result<User, Error> {
        let user = self.user(name).await?;
        let body = friendship_body(&user.id, name.as_str());
        let _payload = self
            .http
            .form_post("/i/api/1.1/friendships/create.json", &body)
            .await?;
        Ok(user)
    }

    /// Unfollow a user by handle.
    pub async fn unfollow(&self, name: &ScreenName) -> Result<User, Error> {
        let user = self.user(name).await?;
        let body = friendship_body(&user.id, name.as_str());
        let _payload = self
            .http
            .form_post("/i/api/1.1/friendships/destroy.json", &body)
            .await?;
        Ok(user)
    }

    /// Mentions timeline for the session.
    pub async fn mentions(
        &self,
        limit: PageSize,
        cursor: Option<&str>,
    ) -> Result<(Vec<Tweet>, Option<String>), Error> {
        let mut variables = json!({
            "timeline_type": "Mentions",
            "count": limit.get(),
        });
        insert_cursor(&mut variables, cursor);
        let payload = self
            .http
            .graphql("NotificationsTimeline", variables)
            .await?;
        Ok(timeline_page(&payload, limit.get() as usize))
    }

    /// Bookmarked posts.
    pub async fn bookmarks(
        &self,
        limit: PageSize,
        cursor: Option<&str>,
    ) -> Result<(Vec<Tweet>, Option<String>), Error> {
        let mut variables = json!({
            "count": limit.get(),
            "includePromotedContent": false,
        });
        insert_cursor(&mut variables, cursor);
        let payload = self.http.graphql("Bookmarks", variables).await?;
        Ok(timeline_page(&payload, limit.get() as usize))
    }

    /// Liked posts for this session (or `--of` user).
    pub async fn likes(
        &self,
        of: Option<&ScreenName>,
        limit: PageSize,
        cursor: Option<&str>,
    ) -> Result<(Vec<Tweet>, Option<String>), Error> {
        let user_id = self.target_user_id(of).await?;
        self.user_tweet_list("Likes", &user_id, limit, cursor).await
    }

    /// Accounts this session (or `--of`) follows.
    pub async fn following(
        &self,
        of: Option<&ScreenName>,
        limit: PageSize,
        cursor: Option<&str>,
    ) -> Result<(Vec<User>, Option<String>), Error> {
        let user_id = self.target_user_id(of).await?;
        self.user_list("Following", &user_id, limit, cursor).await
    }

    /// Followers of this session (or `--of`).
    pub async fn followers(
        &self,
        of: Option<&ScreenName>,
        limit: PageSize,
        cursor: Option<&str>,
    ) -> Result<(Vec<User>, Option<String>), Error> {
        let user_id = self.target_user_id(of).await?;
        self.user_list("Followers", &user_id, limit, cursor).await
    }

    /// Bookmark a post.
    pub async fn bookmark(&self, post_id: &PostId) -> Result<(), Error> {
        self.tweet_id_mutation("CreateBookmark", post_id).await
    }

    /// Remove a bookmark.
    pub async fn unbookmark(&self, post_id: &PostId) -> Result<(), Error> {
        self.tweet_id_mutation("DeleteBookmark", post_id).await
    }

    /// Block a user.
    pub async fn block(&self, name: &ScreenName) -> Result<User, Error> {
        self.form_user("/i/api/1.1/blocks/create.json", name).await
    }

    /// Unblock a user.
    pub async fn unblock(&self, name: &ScreenName) -> Result<User, Error> {
        self.form_user("/i/api/1.1/blocks/destroy.json", name).await
    }

    /// Mute a user.
    pub async fn mute(&self, name: &ScreenName) -> Result<User, Error> {
        self.form_user("/i/api/1.1/mutes/users/create.json", name)
            .await
    }

    /// Unmute a user.
    pub async fn unmute(&self, name: &ScreenName) -> Result<User, Error> {
        self.form_user("/i/api/1.1/mutes/users/destroy.json", name)
            .await
    }

    /// Upload local media (INIT/APPEND/FINALIZE). Does not create a post.
    pub async fn upload_media(&self, path: &Path) -> Result<Media, Error> {
        let (media_type, category) = kind_for_path(path)?;
        let bytes = fs::read(path).map_err(|source| Error::Io {
            path: Some(path.to_path_buf()),
            source,
        })?;
        validate_bytes(&bytes)?;
        let total = u64::try_from(bytes.len()).map_err(|_| Error::InvalidMedia)?;
        let init = self.http.media_init(total, media_type, category).await?;
        let media = media_from_upload(&init)?;
        for (index, chunk) in bytes.chunks(SEGMENT_BYTES).enumerate() {
            let segment = u32::try_from(index).map_err(|_| Error::InvalidMedia)?;
            self.http
                .media_append(&media.media_id, segment, chunk.to_vec())
                .await?;
        }
        let finalized = self.http.media_finalize(&media.media_id).await?;
        media_from_upload(&finalized)
    }

    /// Poll uploaded media processing.
    pub async fn media_status(&self, media_id: &MediaId) -> Result<Media, Error> {
        let payload = self.http.media_status(media_id.as_str()).await?;
        media_from_upload(&payload)
    }

    /// Inbox messages for the session.
    pub async fn dms(&self, limit: PageSize) -> Result<Vec<DirectMessage>, Error> {
        let payload = self
            .http
            .get_path("/i/api/1.1/dm/inbox_initial_state.json")
            .await?;
        Ok(dms_from_inbox(&payload, limit.get() as usize))
    }

    /// Send a DM to a handle. Wired; do not call unless asked.
    pub async fn dm(
        &self,
        name: &ScreenName,
        text: &PostText,
    ) -> Result<Vec<DirectMessage>, Error> {
        let user = self.user(name).await?;
        let payload = self
            .http
            .form_pairs(
                "/i/api/1.1/dm/new.json",
                &[
                    ("text", text.as_str().to_string()),
                    ("recipient_ids", user.id),
                ],
            )
            .await?;
        Ok(dms_from_inbox(&payload, 10))
    }

    async fn tweet_id_mutation(&self, op: &'static str, post_id: &PostId) -> Result<(), Error> {
        let variables = json!({ "tweet_id": post_id.as_str() });
        let _payload = self.http.graphql(op, variables).await?;
        Ok(())
    }

    async fn create_tweet(
        &self,
        text: &PostText,
        reply_to: Option<&PostId>,
        quote_of: Option<&PostId>,
        media: &[MediaId],
    ) -> Result<Tweet, Error> {
        let entities: Vec<serde_json::Value> = media
            .iter()
            .map(|id| json!({ "media_id": id.as_str(), "tagged_users": [] }))
            .collect();
        let mut variables = json!({
            "tweet_text": text.as_str(),
            "dark_request": false,
            "media": {
                "media_entities": entities,
                "possibly_sensitive": false
            },
            "semantic_annotation_ids": []
        });
        if let Some(object) = variables.as_object_mut() {
            if let Some(post_id) = reply_to {
                object.insert(
                    "reply".to_string(),
                    json!({
                        "in_reply_to_tweet_id": post_id.as_str(),
                        "exclude_reply_user_ids": []
                    }),
                );
            }
            if let Some(post_id) = quote_of {
                object.insert(
                    "attachment_url".to_string(),
                    json!(format!("https://x.com/i/web/status/{}", post_id.as_str())),
                );
            }
        }
        let payload = self.http.graphql("CreateTweet", variables).await?;
        tweet_from_create_payload(&payload)
    }

    async fn target_user_id(&self, of: Option<&ScreenName>) -> Result<String, Error> {
        match of {
            Some(name) => Ok(self.user(name).await?.id),
            None => Ok(self.whoami().await?.id),
        }
    }

    async fn user_tweet_list(
        &self,
        op: &'static str,
        user_id: &str,
        limit: PageSize,
        cursor: Option<&str>,
    ) -> Result<(Vec<Tweet>, Option<String>), Error> {
        let mut variables = json!({
            "userId": user_id,
            "count": limit.get(),
            "includePromotedContent": false,
        });
        insert_cursor(&mut variables, cursor);
        let payload = self.http.graphql(op, variables).await?;
        Ok(timeline_page(&payload, limit.get() as usize))
    }

    async fn user_list(
        &self,
        op: &'static str,
        user_id: &str,
        limit: PageSize,
        cursor: Option<&str>,
    ) -> Result<(Vec<User>, Option<String>), Error> {
        let mut variables = json!({
            "userId": user_id,
            "count": limit.get(),
            "includePromotedContent": false,
        });
        insert_cursor(&mut variables, cursor);
        let payload = self.http.graphql(op, variables).await?;
        Ok(user_page(&payload, limit.get() as usize))
    }

    async fn form_user(&self, path: &'static str, name: &ScreenName) -> Result<User, Error> {
        let user = self.user(name).await?;
        let body = friendship_body(&user.id, name.as_str());
        let _payload = self.http.form_post(path, &body).await?;
        Ok(user)
    }
}

fn friendship_body(user_id: &str, screen_name: &str) -> String {
    format!("user_id={user_id}&screen_name={screen_name}")
}

fn insert_cursor(variables: &mut serde_json::Value, cursor: Option<&str>) {
    if let Some(token) = cursor {
        if let Some(object) = variables.as_object_mut() {
            object.insert("cursor".to_string(), json!(token));
        }
    }
}
