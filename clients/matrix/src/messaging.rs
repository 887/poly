//! `impl MessagingBackend for MatrixClient` — typing, replies, search, pinned messages.

use async_trait::async_trait;
use poly_client::*;

use crate::api;
use crate::MatrixClient;

// ── H.4.a — MessagingBackend ─────────────────────────────────────────────────

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::MessagingBackend for MatrixClient {
    async fn send_typing(&self, channel_id: &str) -> ClientResult<()> {
        // SOLID-audit-matrix C.1: wire `PUT /_matrix/client/v3/rooms/{roomId}/typing/{userId}`
        // with a 4-second timeout.  Errors are best-effort and logged at debug level only —
        // typing indicators are non-critical and should never surface in the UI as errors.
        let user_id = match self.http.session().map(|s| s.user_id) {
            Some(id) => id,
            None => {
                tracing::debug!(channel_id, "matrix: send_typing skipped (not authenticated)");
                return Ok(());
            }
        };
        if let Err(err) = self.http.put_room_typing(channel_id, &user_id, 4000).await {
            tracing::debug!(channel_id, %err, "matrix: send_typing failed (best-effort)");
        }
        Ok(())
    }

    async fn send_reply_message(
        &self,
        channel_id: &str,
        reply_to_message_id: &str,
        content: MessageContent,
    ) -> ClientResult<Message> {
        let txn_id = uuid::Uuid::new_v4().to_string();
        let body = Self::extract_body(&content);

        let send_req = api::SendMessageRequest {
            msgtype: "m.text".to_string(),
            body: body.clone(),
            formatted_body: None,
            format: None,
            relates_to: Some(api::RelatesTo {
                in_reply_to: Some(api::InReplyTo {
                    event_id: reply_to_message_id.to_string(),
                }),
            }),
        };

        let result = self
            .http
            .send_message(channel_id, &txn_id, &send_req)
            .await?;

        self.build_message_from_send(result.event_id, body)
    }

    async fn search_messages(
        &self,
        query: MessageSearchQuery,
    ) -> ClientResult<Vec<MessageSearchHit>> {
        // SOLID-audit-matrix C.2: POST /_matrix/client/v3/search → room_events category.
        let room_id = query.channel_id.as_deref();
        let limit = query.limit;

        let resp = self
            .http
            .post_search(&query.text, room_id, limit)
            .await?;

        let results = resp.search_categories.room_events.results;
        let homeserver_url = self.homeserver_url().to_string();

        let hits: Vec<MessageSearchHit> = results
            .into_iter()
            .filter_map(|r| {
                let event = r.result;
                // Only surface m.room.message events; search may return state events.
                if event.event_type != "m.room.message" {
                    return None;
                }
                let room_id_str = event.event_id.as_deref()
                    .map(|_| room_id.map(str::to_string))
                    .unwrap_or(None)
                    .unwrap_or_default();

                // Extract the room ID the event belongs to from the event itself.
                // The Matrix search response attaches `room_id` directly on the event
                // but our `RoomEvent` struct doesn't decode it yet — use the filter
                // room_id when available, otherwise fall back to an empty string.
                // TODO: add `room_id` field to `RoomEvent` and remove this fallback.
                let _ = &homeserver_url; // used for future avatar hydration
                let msg = Self::room_event_to_message(&event)?;
                Some(MessageSearchHit {
                    channel_id: room_id_str,
                    channel_name: None,
                    server_id: query.server_id.clone(),
                    message: msg,
                })
            })
            .collect();

        Ok(hits)
    }

    async fn get_pinned_messages(&self, channel_id: &str) -> ClientResult<Vec<Message>> {
        // SOLID-audit-matrix C.3: GET m.room.pinned_events state event, then fetch each event.
        let event_ids = self.http.get_room_pinned_event_ids(channel_id).await?;

        if event_ids.is_empty() {
            return Ok(Vec::new());
        }

        // Fetch each pinned event individually and map to Message.
        // Errors on individual events are silently skipped (pin may have been
        // redacted or the event ID may be stale).
        let mut messages = Vec::with_capacity(event_ids.len());
        for event_id in &event_ids {
            match self.http.get_room_event(channel_id, event_id).await {
                Ok(ev) => {
                    if let Some(msg) = Self::room_event_to_message(&ev) {
                        messages.push(msg);
                    }
                }
                Err(err) => {
                    tracing::debug!(
                        channel_id,
                        event_id,
                        %err,
                        "matrix: get_pinned_messages — skipping stale/redacted event"
                    );
                }
            }
        }

        let messages = self.hydrate_message_authors(messages).await;
        Ok(messages)
    }

    async fn set_message_pinned(
        &self,
        channel_id: &str,
        message_id: &str,
        pinned: bool,
    ) -> ClientResult<()> {
        // SOLID-audit-matrix C.3: read-modify-write on m.room.pinned_events.
        // Requires the caller to have state=50 power level in the room.
        let mut ids = self.http.get_room_pinned_event_ids(channel_id).await?;

        if pinned {
            // Add if not already present.
            if !ids.contains(&message_id.to_string()) {
                ids.push(message_id.to_string());
            }
        } else {
            // Remove all occurrences.
            ids.retain(|id| id != message_id);
        }

        self.http.put_room_pinned_events(channel_id, ids).await
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
