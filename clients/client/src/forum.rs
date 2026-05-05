//! `ForumBackend` capability sub-trait (Phase H.2.b).
//!
//! Carved out of [`ClientBackend`] in Phase H.2.b.  Implemented by backends
//! that expose forum-style channels (`ChannelType::Forum`): currently
//! `poly-discord` (thread-based forum posts) and `poly-lemmy` (community posts
//! with comments).
//!
//! # Capability dispatch
//!
//! ```rust,ignore
//! if let Some(fb) = backend.as_forum() {
//!     let posts = fb.get_forum_posts(&channel_id, ForumSortOrder::LatestActivity, Some(50)).await?;
//!     // …
//! }
//! ```
//!
//! WIT interface: `poly:messenger/messenger-client` — `get-forum-posts`,
//! `create-forum-post` functions.  `get-recent-comments` is a Rust-only
//! extension (Lemmy-specific; not in WIT).
//!
//! [`ClientBackend`]: crate::ClientBackend

use async_trait::async_trait;

use crate::{ClientResult, ForumPost, ForumSortOrder, Message, MessageQuery};

/// Capability sub-trait for forum-channel operations.
///
/// Mirrors the forum-related functions from the
/// `poly:messenger/messenger-client` WIT interface, plus the Rust-only
/// `get_recent_comments` extension (Lemmy-specific).
///
/// No default impls: presence of `impl ForumBackend` is the opt-in signal.
/// Backends that do not support forum channels leave
/// [`IsBackend::as_forum`] returning `None` (the default).
///
/// [`IsBackend::as_forum`]: crate::IsBackend::as_forum
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait ForumBackend: Send + Sync {
    /// Get forum posts (threads) in a forum channel.
    ///
    /// Posts are sorted according to `sort`. `limit` caps the number returned;
    /// `None` uses the backend default.
    ///
    /// Backends that support threads but not post-listing should return
    /// `Err(ClientError::NotSupported(...))`.
    async fn get_forum_posts(
        &self,
        forum_channel_id: &str,
        sort: ForumSortOrder,
        limit: Option<u32>,
    ) -> ClientResult<Vec<ForumPost>>;

    /// Create a new forum post (thread) in a forum channel.
    ///
    /// `title` is the post/thread name, `body` is the starter message text,
    /// and `tags` is the list of tag IDs to apply.
    ///
    /// Backends that support reading but not creating posts should return
    /// `Err(ClientError::NotSupported(...))`.
    async fn create_forum_post(
        &self,
        forum_channel_id: &str,
        title: &str,
        body: &str,
        tags: Vec<String>,
    ) -> ClientResult<ForumPost>;

    /// Fetch recent comments across a community (Lemmy-specific).
    ///
    /// `channel_id` is the feed channel for the community (e.g. for Lemmy,
    /// `lemmy-feed-{community_id}`). Returns up to `query.limit` (default 50)
    /// of the most recently-posted comments across all posts in the community.
    ///
    /// Backends that do not support a comment-feed should return
    /// `Err(ClientError::NotSupported(...))`.
    async fn get_recent_comments(
        &self,
        channel_id: &str,
        query: MessageQuery,
    ) -> ClientResult<Vec<Message>>;
}
