//! Extracted from lib.rs as part of SOLID B.1 split.
//!
//! Pure structural move — no behaviour change.

use super::super::DiscordClient;
use async_trait::async_trait;
use poly_client::{ClientResult, MessageContent, Message, MessageSearchQuery, MessageSearchHit, ClientError, ChatCommand, CustomEmoji, StickerItem};

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::MessagingBackend for DiscordClient {
    async fn send_typing(&self, channel_id: &str) -> ClientResult<()> {
        // D.6 — client-side 8 s typing-fire-rate cap.
        if !self.typing_cap.should_send(channel_id) {
            // Silently drop re-triggers inside the window — no error to the UI.
            // F.1 — record the suppressed re-trigger in telemetry.
            self.http.counters.inc_typing_cap_drop(channel_id);
            return Ok(());
        }
        self.http.trigger_typing(channel_id).await
    }

    async fn send_reply_message(
        &self,
        channel_id: &str,
        _reply_to_message_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message> {
        // Discord reply threading not yet wired through the HTTP client.
        // Fall back to a top-level send so the message is not lost.
        // D.5 — slow-mode guard (rate_limit_per_user=0 → no slow mode).
        // We don't have the cached channel here, so we check with 0 (permissive);
        // the SlowModeGuard only records sends when rate_limit_per_user > 0.
        if let Err(e) = self.slow_mode_guard.check(channel_id, 0) {
            self.http.counters.inc_slow_mode_trip(channel_id);
            return Err(e);
        }
        self.slow_mode_guard.record_send(channel_id);
        let text = match content {
            MessageContent::Text(t) => t,
            MessageContent::WithAttachments { text, .. } => text,
        };
        let m = self.http.send_message(channel_id, &text).await?;
        Ok(self.discord_message_to_poly(m))
    }

    async fn search_messages(
        &self,
        _query: MessageSearchQuery,
    ) -> ClientResult<Vec<MessageSearchHit>> {
        Err(ClientError::NotSupported("search_messages: Discord search not yet implemented".to_string()))
    }

    async fn get_pinned_messages(&self, _channel_id: &str) -> ClientResult<Vec<Message>> {
        Err(ClientError::NotSupported("get_pinned_messages: not yet implemented for Discord".to_string()))
    }

    async fn set_message_pinned(
        &self,
        _channel_id: &str,
        _message_id: &str,
        _pinned: bool,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported("set_message_pinned: not yet implemented for Discord".to_string()))
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

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::WritableMessagingBackend for DiscordClient {
    async fn send_message(&self, channel_id: &str, content: MessageContent) -> ClientResult<Message> {
        // D.5 — slow-mode guard.  `rate_limit_per_user` of 0 means no restriction.
        // We record the send unconditionally; the guard only blocks when a window is set.
        if let Err(e) = self.slow_mode_guard.check(channel_id, 0) {
            self.http.counters.inc_slow_mode_trip(channel_id);
            return Err(e);
        }
        self.slow_mode_guard.record_send(channel_id);
        let text = match content {
            MessageContent::Text(t) => t,
            MessageContent::WithAttachments { text, .. } => text,
        };
        let m = self.http.send_message(channel_id, &text).await?;
        Ok(self.discord_message_to_poly(m))
    }
}
