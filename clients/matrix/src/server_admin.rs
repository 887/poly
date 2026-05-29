//! `ServerAdminBackend` + `WritableServerAdminBackend` for `MatrixClient`.
//!
//! Tier 2: `create_server`, `create_channel` (real) +
//! `update_server_banner` (stub) move into the writable trait. Reads
//! and the invite/mark-read methods stay on the read trait.

use async_trait::async_trait;
use poly_client::{ClientResult, ClientError, Server, BackendType, ChannelType, Channel};

use crate::api;
use crate::MatrixClient;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ServerAdminBackend for MatrixClient {
    async fn mark_channel_read(&self, channel_id: &str) -> ClientResult<()> {
        let from = self
            .http
            .session()
            .and_then(|s| s.sync_next_batch)
            .unwrap_or_default();

        let response = self
            .http
            .fetch_messages(channel_id, &from, "b", Some(1))
            .await;

        let event_id = match response {
            Ok(page) => page.chunk.into_iter().find_map(|ev| ev.event_id),
            Err(err) => {
                tracing::debug!(channel_id, %err, "matrix: mark_channel_read could not fetch latest event");
                return Ok(());
            }
        };

        let Some(event_id) = event_id else {
            tracing::debug!(channel_id, "matrix: mark_channel_read skipped (no events found)");
            return Ok(());
        };

        self.http.post_read_markers(channel_id, &event_id).await
    }

    async fn respond_to_server_invite(
        &self,
        _server_id: &str,
        _accept: bool,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "matrix: respond_to_server_invite not implemented".to_string(),
        ))
    }

    /// Matrix has no "server invite" concept equivalent to Discord. The closest
    /// mapping is inviting to the Space room directly.
    async fn invite_user_to_server(
        &self,
        server_id: &str,
        user_id: &str,
    ) -> ClientResult<()> {
        if server_id.starts_with('!') {
            self.http.invite_to_room(server_id, user_id).await
        } else {
            Err(ClientError::NotSupported(
                "invite_user_to_server: server_id is not a Matrix room ID; \
                 Matrix has no invite-link concept — pass the Space room ID instead"
                    .to_string(),
            ))
        }
    }

    fn as_writable_server_admin(
        &self,
    ) -> Option<&dyn poly_client::WritableServerAdminBackend> {
        Some(self)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::WritableServerAdminBackend for MatrixClient {
    async fn create_server(&self, name: &str) -> ClientResult<Server> {
        let req = api::CreateRoomRequest {
            preset: Some("public_chat".to_string()),
            name: Some(name.to_string()),
            room_type: Some("m.space".to_string()),
            ..Default::default()
        };
        let resp = self.http.create_room(&req).await?;
        let room_id = resp.room_id;

        let account_id = self.http.session().map(|s| s.user_id).unwrap_or_default();

        Ok(Server {
            id: room_id,
            name: name.to_string(),
            icon_url: None,
            banner_url: None,
            categories: Vec::new(),
            backend: BackendType::from(crate::SLUG),
            unread_count: 0,
            mention_count: 0,
            account_id: account_id.clone(),
            account_display_name: account_id,
            default_channel_id: None,
            description: None,
            star_count: None,
            language: None,
            forks_count: None,
            open_issues_count: None,
        })
    }

    async fn create_channel(
        &self,
        server_id: &str,
        name: &str,
        channel_type: ChannelType,
    ) -> ClientResult<Channel> {
        match channel_type {
            ChannelType::Text => {}
            ChannelType::Voice | ChannelType::Video | ChannelType::Forum | ChannelType::HackerNews | ChannelType::Code | ChannelType::Thread | ChannelType::Announcement => {
                return Err(ClientError::NotSupported(
                    "matrix: create_channel only supports Text channels; \
                     Matrix has no native Voice/Video room type"
                        .to_string(),
                ));
            }
        }

        let req = api::CreateRoomRequest {
            preset: Some("public_chat".to_string()),
            name: Some(name.to_string()),
            initial_state: vec![api::InitialStateEvent {
                event_type: "m.space.parent".to_string(),
                state_key: server_id.to_string(),
                content: serde_json::json!({
                    "via": [self.http.homeserver_url().trim_start_matches("https://").trim_start_matches("http://")],
                    "canonical": true
                }),
            }],
            ..Default::default()
        };
        let resp = self.http.create_room(&req).await?;
        let room_id = resp.room_id;

        if let Err(err) = self.http.put_space_child(server_id, &room_id).await {
            tracing::debug!(
                server_id,
                room_id,
                %err,
                "matrix: create_channel — m.space.child write failed (best-effort)"
            );
        }

        Ok(Channel {
            id: room_id,
            name: name.to_string(),
            channel_type: ChannelType::Text,
            server_id: server_id.to_string(),
            unread_count: 0,
            mention_count: 0,
            last_message_id: None,
            forum_tags: None,
            parent_channel_id: None,
            thread_metadata: None,
        })
    }

    async fn update_server_banner(
        &self,
        _server_id: &str,
        _banner_url: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "matrix: update_server_banner not implemented".to_string(),
        ))
    }
}
