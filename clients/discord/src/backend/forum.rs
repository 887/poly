//! Extracted from lib.rs as part of SOLID B.1 split.
//!
//! Pure structural move — no behaviour change.

use super::super::*;
use async_trait::async_trait;
use poly_client::*;

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ForumBackend for DiscordClient {
    async fn get_forum_posts(
        &self,
        forum_channel_id: &str,
        sort: ForumSortOrder,
        limit: Option<u32>,
    ) -> ClientResult<Vec<ForumPost>> {
        // Fetch the forum channel to get the guild ID.
        let forum_ch = self.http.get_channel(forum_channel_id).await?;
        let guild_id = forum_ch
            .guild_id
            .map(|id| id.to_string())
            .ok_or_else(|| ClientError::Internal("forum channel missing guild_id".into()))?;

        let cap = usize::try_from(limit.unwrap_or(50).min(100)).unwrap_or(usize::MAX);

        // Fetch all active threads in the guild, filter to this forum.
        let active = self.http.get_active_threads(&guild_id).await?;
        let mut threads: Vec<api::DiscordChannel> = active
            .threads
            .into_iter()
            .filter(|t| {
                t.parent_id
                    .is_some_and(|pid| pid.to_string() == forum_channel_id)
            })
            .collect();

        // Sort per the requested order.
        match sort {
            ForumSortOrder::LatestActivity => {
                // last_message_id is a snowflake — lexicographic sort is chronological.
                // Since we don't have last_message_id on the thread object yet, we fall
                // back to insertion order (Discord returns newest-activity first anyway).
            }
            ForumSortOrder::CreationDate => {
                // Sort by thread creation timestamp, newest first.
                threads.sort_by(|a, b| {
                    let ts_a = a.thread_metadata.as_ref().and_then(|m| m.create_timestamp.as_deref())
                        .unwrap_or("");
                    let ts_b = b.thread_metadata.as_ref().and_then(|m| m.create_timestamp.as_deref())
                        .unwrap_or("");
                    ts_b.cmp(ts_a) // descending
                });
            }
        }

        threads.truncate(cap);

        let mut posts = Vec::with_capacity(threads.len());
        for t in threads {
            let thread_id = t.id.to_string();
            // Fetch the starter message (oldest message) for each thread.
            // Discord returns messages in reverse-chronological order; `after=0`
            // returns the first message ever posted (after snowflake 0).
            let starter_message_id = self
                .http
                .get_thread_messages(&thread_id, Some(1), Some("0"))
                .await
                .ok()
                .and_then(|msgs| msgs.into_iter().next())
                .map(|m| m.id.to_string());
            let applied_tags = t
                .applied_tags
                .as_ref()
                .map(|tags| tags.iter().map(std::string::ToString::to_string).collect())
                .unwrap_or_default();
            posts.push(ForumPost {
                thread: Self::discord_thread_to_thread_info(&t),
                applied_tags,
                starter_message_id,
            });
        }

        Ok(posts)
    }

    async fn create_forum_post(
        &self,
        _forum_channel_id: &str,
        _title: &str,
        _body: &str,
        _tags: Vec<String>,
    ) -> ClientResult<ForumPost> {
        Err(ClientError::NotSupported("create_forum_post".to_string()))
    }

    async fn get_recent_comments(
        &self,
        _channel_id: &str,
        _query: MessageQuery,
    ) -> ClientResult<Vec<Message>> {
        Err(ClientError::NotSupported("get_recent_comments".to_string()))
    }
}
