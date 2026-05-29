//! `ServerAdminBackend` + `WritableServerAdminBackend` for `LemmyClient`.
//!
//! Lemmy's only real server-admin write is `update_server_banner` (wires
//! through `PUT /api/v3/community`). The other writes
//! (`create_server`, `create_channel`) are unsupported and fall through
//! to the read-trait shim's `NotSupported` default.

use async_trait::async_trait;
use poly_client::{ClientResult, ClientError, Server, ChannelType, Channel};

use crate::LemmyClient;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ServerAdminBackend for LemmyClient {
    async fn mark_channel_read(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "lemmy: mark_channel_read not implemented".to_string(),
        ))
    }

    async fn respond_to_server_invite(
        &self,
        _server_id: &str,
        _accept: bool,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "lemmy: respond_to_server_invite not implemented".to_string(),
        ))
    }

    async fn invite_user_to_server(
        &self,
        _server_id: &str,
        _user_id: &str,
    ) -> ClientResult<()> {
        Err(ClientError::NotSupported(
            "lemmy: invite_user_to_server not implemented".to_string(),
        ))
    }

    fn as_writable_server_admin(
        &self,
    ) -> Option<&dyn poly_client::WritableServerAdminBackend> {
        Some(self)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::WritableServerAdminBackend for LemmyClient {
    async fn create_server(&self, _name: &str) -> ClientResult<Server> {
        Err(ClientError::NotSupported(
            "lemmy: create_server not implemented".to_string(),
        ))
    }

    async fn create_channel(
        &self,
        _server_id: &str,
        _name: &str,
        _channel_type: ChannelType,
    ) -> ClientResult<Channel> {
        Err(ClientError::NotSupported(
            "lemmy: create_channel not implemented".to_string(),
        ))
    }

    async fn update_server_banner(
        &self,
        server_id: &str,
        banner_url: Option<&str>,
    ) -> ClientResult<()> {
        let community_id = Self::parse_community_id(server_id)?;
        self.http
            .put_community(community_id, banner_url)
            .await
            .map(|_| ())
    }
}
