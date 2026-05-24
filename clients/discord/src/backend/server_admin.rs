//! Extracted from lib.rs as part of SOLID B.1 split.
//!
//! Pure structural move — no behaviour change.

use super::super::*;
use async_trait::async_trait;
use poly_client::*;

#[cfg(feature = "native")]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl poly_client::ServerAdminBackend for DiscordClient {
    async fn create_server(&self, _name: &str) -> ClientResult<Server> {
        Err(ClientError::NotSupported("discord: create_server not implemented".to_string()))
    }

    async fn create_channel(
        &self,
        _server_id: &str,
        _name: &str,
        _channel_type: ChannelType,
    ) -> ClientResult<Channel> {
        Err(ClientError::NotSupported("discord: create_channel not implemented".to_string()))
    }

    async fn update_server_banner(
        &self,
        server_id: &str,
        banner_url: Option<&str>,
    ) -> ClientResult<()> {
        let body = serde_json::json!({ "banner": banner_url });
        self.http
            .patch_guild(server_id, body)
            .await
            .map(|_| ())
    }

    async fn mark_channel_read(&self, _channel_id: &str) -> ClientResult<()> {
        Err(ClientError::NotSupported("discord: mark_channel_read not implemented".to_string()))
    }

    async fn respond_to_server_invite(&self, _server_id: &str, _accept: bool) -> ClientResult<()> {
        Err(ClientError::NotSupported("discord: respond_to_server_invite not implemented".to_string()))
    }

    async fn invite_user_to_server(
        &self,
        server_id: &str,
        user_id: &str,
    ) -> ClientResult<()> {
        // Step 1: resolve system channel.
        let guild = self.http.get_guild(server_id).await?;
        let system_channel_id = guild.system_channel_id.ok_or_else(|| {
            ClientError::NotSupported(
                "invite_user_to_server: server has no system channel; cannot create invite".to_string(),
            )
        })?;

        // Step 2: create invite (1 day, 1 use).
        let invite_code = self
            .http
            .create_invite(&system_channel_id, 86400, 1)
            .await?;
        let invite_url = format!("https://discord.gg/{invite_code}");

        // Step 3: open DM and send the invite URL.
        let dm_channel_id = self.http.open_dm(user_id).await?;
        self.http.send_message(&dm_channel_id, &invite_url).await?;
        Ok(())
    }
}
