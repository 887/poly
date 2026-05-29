//! `impl ForumBackend for LemmyClient` — forum posts + comment feed (H.2.b).
//!
//! Split out of `lib.rs` for Single Responsibility (B.1).

use async_trait::async_trait;
use poly_client::{ForumSortOrder, ClientResult, ForumPost, ClientError, MessageQuery, Message};

use crate::LemmyClient;
use crate::api::map_comment_to_message;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ForumBackend for LemmyClient {
    async fn get_forum_posts(
        &self,
        forum_channel_id: &str,
        sort: ForumSortOrder,
        limit: Option<u32>,
    ) -> ClientResult<Vec<ForumPost>> {
        let community_id = Self::parse_feed_channel(forum_channel_id).ok_or_else(|| {
            ClientError::NotFound(format!(
                "get_forum_posts: expected lemmy-feed-<id>, got: {forum_channel_id}"
            ))
        })?;

        // Map Poly's ForumSortOrder to a Lemmy sort string.
        let sort_str = match sort {
            ForumSortOrder::LatestActivity => "Active",
            ForumSortOrder::CreationDate => "New",
        };

        let cap = limit.unwrap_or(20).min(50);
        let resp = self
            .http
            .fetch_posts_paged(community_id, sort_str, 1, cap)
            .await?;

        let posts = resp
            .posts
            .iter()
            .map(|view| ForumPost {
                thread: poly_client::ThreadInfo {
                    thread_id: format!("lemmy-post-{}", view.post.id),
                    parent_channel_id: forum_channel_id.to_string(),
                    message_count: u32::try_from(view.counts.comments.max(0))
                        .unwrap_or(u32::MAX),
                    member_count: 0,
                },
                applied_tags: vec![],
                starter_message_id: Some(format!("lemmy-post-{}", view.post.id)),
            })
            .collect();

        Ok(posts)
    }

    /// C.7 — wire `create_forum_post` for Lemmy via `POST /api/v3/post`.
    ///
    /// `forum_channel_id` must be `lemmy-feed-{community_id}`.  Tags are
    /// ignored (Lemmy's tag system requires community-specific tag IDs that
    /// the UI doesn't yet expose).
    async fn create_forum_post(
        &self,
        forum_channel_id: &str,
        title: &str,
        body: &str,
        _tags: Vec<String>,
    ) -> ClientResult<ForumPost> {
        let community_id = Self::parse_feed_channel(forum_channel_id).ok_or_else(|| {
            ClientError::NotFound(format!(
                "create_forum_post: expected lemmy-feed-<id>, got: {forum_channel_id}"
            ))
        })?;

        let post_view = self
            .http
            .create_post(community_id, title, Some(body), None)
            .await?;

        Ok(ForumPost {
            thread: poly_client::ThreadInfo {
                thread_id: format!("lemmy-post-{}", post_view.post.id),
                parent_channel_id: forum_channel_id.to_string(),
                message_count: 0,
                member_count: 0,
            },
            applied_tags: vec![],
            starter_message_id: None,
        })
    }

    /// Return recent comments across a Lemmy community (Phase D).
    ///
    /// `channel_id` must be a `lemmy-feed-{community_id}` channel. Returns up
    /// to `query.limit` (default 50) comments sorted by newest first, each
    /// mapped to a `Message` via `map_comment_to_message`.
    async fn get_recent_comments(
        &self,
        channel_id: &str,
        query: MessageQuery,
    ) -> ClientResult<Vec<Message>> {
        let community_id = Self::parse_feed_channel(channel_id).ok_or_else(|| {
            ClientError::NotFound(format!(
                "get_recent_comments: expected lemmy-feed-<id>, got: {channel_id}"
            ))
        })?;

        let limit = query.limit.unwrap_or(50).min(200);
        let resp = self.http.fetch_community_comments(community_id, limit).await?;

        let messages: Vec<Message> = resp
            .comments
            .iter()
            .map(map_comment_to_message)
            .collect();

        Ok(messages)
    }
}
