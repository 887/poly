//! `impl ServerAdminBackend for StoatClient` — invite + channel-ack + stubs.
//!
//! Split out from `lib.rs` in SOLID-audit-stoat D.2 (C.4).
//!
//! Stoat only implements `invite_user_to_server` and `mark_channel_read`
//! from this trait. The remaining methods (`create_server`, `create_channel`,
//! `update_server_banner`, `respond_to_server_invite`) are intentional
//! `NotSupported` stubs — Stoat exposes these via its web UI rather than
//! the REST API used by Poly.

use crate::api::StoatSendMessageRequest;
use async_trait::async_trait;
use poly_client::{Channel, ChannelType, ClientError, ClientResult, MessageQuery, Server};
use poly_host_bridge::http::Method;

use super::StoatClient;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ServerAdminBackend for StoatClient {
    async fn create_server(&self, _name: &str) -> ClientResult<Server> {
        Err(ClientError::NotSupported(
            "stoat: create_server not exposed via the Revolt REST API used by Poly".to_string(),
        ))
    }

    async fn create_channel(
        &self,
        _server_id: &str,
        _name: &str,
        _channel_type: ChannelType,
    ) -> ClientResult<Channel> {
        Err(ClientError::NotSupported(
            "stoat: create_channel not exposed via the Revolt REST API used by Poly".to_string(),
        ))
    }

    async fn update_server_banner(
        &self,
        _server_id: &str,
        _banner_url: Option<&str>,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "stoat: update_server_banner requires an Autumn upload first; not yet implemented"
                .to_string(),
        ))
    }

    /// Mark the calling user's read position in a channel via `PUT /channels/{id}/ack/{message_id}`.
    ///
    /// If no messages are present the call is a safe no-op.
    async fn mark_channel_read(&self, channel_id: &str) -> ClientResult<()> {
        // Fetch the most recent message to get a valid ack target.
        let query = MessageQuery {
            before: None,
            after: None,
            around: None,
            limit: Some(1),
        };
        let messages = self.http.fetch_messages(channel_id, &query).await?;
        let (msgs, _, _) = messages.into_parts();
        let Some(latest) = msgs.into_iter().next() else {
            // No messages — nothing to ack.
            return Ok(());
        };
        let response = self
            .http
            .authenticated_request(
                Method::PUT,
                &format!("/channels/{channel_id}/ack/{}", latest.id),
            )?
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        if !(response.status().is_success() || response.status().as_u16() == 204) {
            tracing::debug!(channel_id, "mark_channel_read: ack returned non-success; ignoring");
        }
        Ok(())
    }

    async fn respond_to_server_invite(
        &self,
        _server_id: &str,
        _accept: bool,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "stoat: respond_to_server_invite not yet implemented".to_string(),
        ))
    }

    /// Invite a user to a server by creating an invite link on the first text
    /// channel and DMing it to the user.
    ///
    /// Flow:
    /// 1. Fetch the server to find a suitable channel.
    /// 2. `POST /channels/{channel_id}/invites` to create a link.
    /// 3. Open or reuse a DM channel with the target user.
    /// 4. Send the invite URL via DM.
    async fn invite_user_to_server(
        &self,
        server_id: &str,
        user_id: &str,
    ) -> ClientResult<()> {
        // Step 1: resolve a text channel on the server to anchor the invite.
        let server = self.http.fetch_server(server_id).await?;
        let channel_id = server.channels.into_iter().next().ok_or_else(|| {
            ClientError::NotSupported(
                "invite_user_to_server: server has no channels; cannot create invite".to_string(),
            )
        })?;

        // Step 2: create the invite (server-side defaults apply: no expiry, no use cap).
        let invite = self.http.create_channel_invite(&channel_id).await?;
        // Stoat/Revolt invite links use the app base URL with the invite code.
        let invite_url = format!("{}/invite/{}", self.http.base_url().trim_end_matches('/'), invite.code);

        // Step 3: open a DM channel with the target user (or reuse existing).
        let dm = self.http.open_direct_message_channel(user_id).await?;

        // Step 4: send the invite URL as a plain text message.
        let req = StoatSendMessageRequest::new(
            invite_url,
            Vec::new(),
            None,
            uuid::Uuid::new_v4().simple().to_string(),
        );
        self.http.send_message(&dm.id, &req).await?;

        Ok(())
    }
}
