//! `MessagingBackend` impl for [`super::RedditBackend`].

use async_trait::async_trait;
use poly_client::{
    ChatCommand, ClientError, ClientResult, CustomEmoji, Message, MessageContent,
    MessageSearchHit, MessageSearchQuery, PresenceStatus, StickerItem, User,
};

use super::error::{NS_PINNED_GET, NS_PINNED_SET, NS_SEARCH_MSG, NS_TYPING};
use super::RedditBackend;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::MessagingBackend for RedditBackend {
    async fn send_typing(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_TYPING.to_string()))
    }

    async fn send_reply_message(
        &self,
        _channel_id: &str,
        reply_to_message_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message> {
        let text = match &content {
            MessageContent::Text(s) => s.clone(),
            MessageContent::WithAttachments { text, .. } => text.clone(),
        };

        // reply_to_message_id is t3_<id> for posts, t1_<id> for comments,
        // or t4_<id> for DMs (Reddit's inbox-reply pattern).
        let accepted = reply_to_message_id.starts_with("t3_")
            || reply_to_message_id.starts_with("t1_")
            || reply_to_message_id.starts_with("t4_");

        if accepted {
            self.client
                .reply_comment(reply_to_message_id, &text)
                .await
                .map_err(ClientError::from)?;
        } else {
            return Err(ClientError::NotSupported(format!(
                "cannot reply to id: {reply_to_message_id}"
            )));
        }

        // Reddit's reply endpoint does not return the new comment ID.
        // Return a placeholder message so the host can show optimistic send.
        let now = chrono::Utc::now();
        let account_display = self.account_display_name().to_string();
        let bt = Self::backend_type();
        Ok(Message {
            id: format!("t1_pending-{}", now.timestamp_millis()),
            author: User {
                id: self
                    .session
                    .as_ref()
                    .map_or("u_me".to_string(), |s| s.user.id.clone()),
                display_name: account_display,
                avatar_url: None,
                presence: PresenceStatus::Offline,
                backend: bt,
            },
            content,
            timestamp: now,
            attachments: Vec::new(),
            reactions: Vec::new(),
            reply_to: None,
            edited: false,
            thread: None,
            preview_image_url: None,
        })
    }

    async fn search_messages(
        &self,
        _query: MessageSearchQuery,
    ) -> ClientResult<Vec<MessageSearchHit>> {
        Err(ClientError::NotSupported(NS_SEARCH_MSG.to_string()))
    }

    async fn get_pinned_messages(&self, _channel_id: &str) -> ClientResult<Vec<Message>> {
        Err(ClientError::NotSupported(NS_PINNED_GET.to_string()))
    }

    async fn set_message_pinned(
        &self,
        _channel_id: &str,
        _message_id: &str,
        _pinned: bool,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(NS_PINNED_SET.to_string()))
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
