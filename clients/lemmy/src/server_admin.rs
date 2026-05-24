//! `impl ServerAdminBackend for LemmyClient` — server/channel/invite ops (H.4.b).
//!
//! Almost all are unsupported; `update_server_banner` wires through to the
//! Lemmy `PUT /api/v3/community` endpoint.
//! Split out of `lib.rs` for Single Responsibility (B.1).

use async_trait::async_trait;
use poly_client::*;

use crate::LemmyClient;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ServerAdminBackend for LemmyClient {
    async fn create_server(&self, _name: &str) -> ClientResult<Server> {
        Err(ClientError::NotSupported("lemmy: create_server not implemented".to_string()))
    }

    async fn create_channel(
        &self,
        _server_id: &str,
        _name: &str,
        _channel_type: ChannelType,
    ) -> ClientResult<Channel> {
        Err(ClientError::NotSupported("lemmy: create_channel not implemented".to_string()))
    }

    async fn update_server_banner(
        &self,
        server_id: &str,
        banner_url: Option<&str>,
    ) -> ClientResult<()> {
        let community_id = LemmyClient::parse_community_id(server_id)?;
        self.http
            .put_community(community_id, banner_url)
            .await
            .map(|_| ())
    }

    async fn mark_channel_read(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("lemmy: mark_channel_read not implemented".to_string()))
    }

    async fn respond_to_server_invite(&self, _server_id: &str, _accept: bool) -> ClientResult<()> {
        Err(ClientError::NotSupported("lemmy: respond_to_server_invite not implemented".to_string()))
    }

    async fn invite_user_to_server(&self, _server_id: &str, _user_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("lemmy: invite_user_to_server not implemented".to_string()))
    }
}
