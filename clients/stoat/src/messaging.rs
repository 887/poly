//! `impl MessagingBackend for StoatClient` — typing, reply, search, pinned, emoji/sticker stubs.
//!
//! Split out from `lib.rs` in SOLID-audit-stoat D.2 (H.4.a).

use crate::api::{self, StoatSearchRequest};
use async_trait::async_trait;
use futures::future;
use poly_client::{
    ChatCommand, ClientError, ClientResult, CustomEmoji, Message, MessageContent,
    MessageSearchHit, MessageSearchQuery, StickerItem,
};
use std::collections::HashMap;

use super::StoatClient;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::MessagingBackend for StoatClient {
    async fn send_typing(&self, channel_id: &str) -> ClientResult<()> {
        // Stoat (Revolt) ships typing indicators only over the Bonfire WebSocket
        // (`ChannelStartTyping` / `ChannelStopTyping`) — there is no HTTP endpoint.
        // C.1: wire through the WS write-path callback stored by `event_stream`.
        // On WASM this path is inactive (ws_write_tx not compiled in) — WASM
        // typing is a best-effort no-op until the WASM WS write path is wired.
        #[cfg(not(target_arch = "wasm32"))]
        {
            let frame = serde_json::json!({
                "type": "BeginTyping",
                "channel": channel_id,
            });
            if let Ok(guard) = self.ws_write_tx.lock() {
                if let Some(ref send_fn) = *guard {
                    send_fn(frame.to_string());
                    tracing::debug!(channel_id, "stoat: send_typing — BeginTyping frame queued");
                } else {
                    // WS not yet connected — best-effort, not an error.
                    tracing::debug!(channel_id, "stoat: send_typing — WS not yet open; skipping");
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            tracing::debug!(channel_id, "stoat: send_typing no-op on WASM (WS write path not yet wired)");
        }
        Ok(())
    }

    async fn send_reply_message(
        &self,
        channel_id: &str,
        reply_to_message_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message> {
        self.send_message_internal(channel_id, content, Some(reply_to_message_id))
            .await
    }

    async fn search_messages(
        &self,
        query: MessageSearchQuery,
    ) -> ClientResult<Vec<MessageSearchHit>> {
        // Revolt exposes per-channel search only — no server-wide or global index.
        // A `channel_id` in the query is therefore required.
        let channel_id = query.channel_id.as_deref().ok_or_else(|| {
            ClientError::NotSupported(
                "search_messages: Stoat requires a channel_id — server-wide search is not supported".to_string(),
            )
        })?;

        let req = StoatSearchRequest {
            query: query.text.clone(),
            author: query.author_id.clone(),
            limit: query.limit,
            sort: Some("Latest".to_string()),
        };

        let (response, root_config) = future::try_join(
            self.http.search_messages_channel(channel_id, &req),
            self.http.fetch_server_config(),
        )
        .await?;

        let autumn_base_url = root_config.autumn_base_url();
        let user_index: HashMap<String, api::StoatUser> = response
            .users
            .into_iter()
            .map(|u| (u.id.clone(), u))
            .collect();
        let channel_id_owned = channel_id.to_string();

        let hits = response
            .messages
            .into_iter()
            .map(|msg| {
                let message = msg.into_poly_message(
                    &user_index,
                    &HashMap::new(),
                    self.current_user_id().as_deref(),
                    autumn_base_url,
                );
                MessageSearchHit {
                    channel_id: channel_id_owned.clone(),
                    channel_name: None,
                    server_id: query.server_id.clone(),
                    message,
                }
            })
            .collect();

        Ok(hits)
    }

    async fn get_pinned_messages(&self, _channel_id: &str) -> ClientResult<Vec<Message>> {
        Err(ClientError::NotSupported("get_pinned_messages: not yet implemented for Stoat".to_string()))
    }

    async fn set_message_pinned(
        &self,
        _channel_id: &str,
        _message_id: &str,
        _pinned: bool,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported("set_message_pinned: not yet implemented for Stoat".to_string()))
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
