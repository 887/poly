//! `impl MessagingBackend for LemmyClient` — typing/reply/search/pinned/emoji (H.4.a).
//!
//! Split out of `lib.rs` for Single Responsibility (B.1).

use async_trait::async_trait;
use poly_client::*;

use crate::LemmyClient;
use crate::api::map_comment_to_message;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::MessagingBackend for LemmyClient {
    async fn send_typing(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("Lemmy has no typing indicators".to_string()))
    }

    async fn send_reply_message(
        &self,
        channel_id: &str,
        reply_to_message_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message> {
        let text = match content {
            MessageContent::Text(t) => t,
            MessageContent::WithAttachments { text, .. } => text,
        };

        if let Some(post_id) = Self::parse_post_channel(channel_id) {
            // reply_to_message_id is `lemmy-comment-{id}`
            let parent_id = reply_to_message_id
                .strip_prefix("lemmy-comment-")
                .and_then(|s| s.parse::<i64>().ok());
            let view = self.http.create_comment(post_id, &text, parent_id).await?;
            return Ok(map_comment_to_message(&view));
        }

        Err(ClientError::NotSupported(
            "send_reply_message: channel must be a lemmy-post-{id} thread channel".to_string(),
        ))
    }

    async fn search_messages(
        &self,
        _query: MessageSearchQuery,
    ) -> ClientResult<Vec<MessageSearchHit>> {
        Err(ClientError::NotSupported("search_messages: Lemmy search not yet implemented".to_string()))
    }

    async fn get_pinned_messages(&self, _channel_id: &str) -> ClientResult<Vec<Message>> {
        Err(ClientError::NotSupported("get_pinned_messages: not supported by Lemmy".to_string()))
    }

    async fn set_message_pinned(
        &self,
        _channel_id: &str,
        _message_id: &str,
        _pinned: bool,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported("set_message_pinned: not supported by Lemmy".to_string()))
    }

    async fn get_channel_commands(&self, _channel_id: &str) -> ClientResult<Vec<ChatCommand>> {
        Ok(Vec::new())
    }

    async fn get_available_emojis(&self, _channel_id: &str) -> ClientResult<Vec<CustomEmoji>> {
        Ok(Vec::new())
    }

    async fn get_available_stickers(&self, _channel_id: &str) -> ClientResult<Vec<StickerItem>> {
        Ok(Vec::new())
    }
}

// ── WritableMessagingBackend (plan-trait-split-readable-vs-writable) ─────────

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::WritableMessagingBackend for LemmyClient {
    async fn send_message(
        &self,
        channel_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message> {
        let text = match content {
            MessageContent::Text(t) => t,
            MessageContent::WithAttachments { text, .. } => text,
        };

        if let Some(post_id) = Self::parse_post_channel(channel_id) {
            let view = self.http.create_comment(post_id, &text, None).await?;
            return Ok(map_comment_to_message(&view));
        }

        Err(ClientError::NotSupported(
            "send_message: channel must be a lemmy-post-{id} thread channel".to_string(),
        ))
    }
}
