//! Server, channel, group-DM, and invite endpoints.
//!
//! - `GET    /` (root config)
//! - `GET    /users/@me/servers`
//! - `GET    /servers/{id}`
//! - `GET    /servers/{id}/members`
//! - `GET    /channels/{id}`
//! - `GET    /users/dms`
//! - `GET    /users/{user_id}/dm`
//! - `GET    /channels/{id}/members`
//! - `PUT|DELETE /channels/{id}/recipients/{user_id}`
//! - `PATCH  /channels/{id}` (edit channel / group DM)
//! - `DELETE /channels/{id}` (close DM / leave group)
//! - `POST   /channels/{id}/invites`
//!
//! Split out from the monolithic `http.rs` in SOLID-audit-stoat D.3.

use super::StoatHttpClient;
use crate::api::{
    StoatAllMemberResponse, StoatChannel, StoatChannelEdit, StoatGroupEdit, StoatRootConfig,
    StoatServer, StoatUser,
};
use poly_client::{ClientError, ClientResult};
use poly_host_bridge::http::Method;

impl StoatHttpClient {
    /// Fetch root instance configuration.
    pub async fn fetch_server_config(&self) -> ClientResult<StoatRootConfig> {
        let response = self
            .request(Method::GET, "/")
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(|e| Self::network_error(&e))
    }

    /// Fetch all servers the authenticated user belongs to.
    ///
    /// Uses `GET /users/@me/servers` — a non-standard extension supported by
    /// Poly test servers. Falls back to `NotSupported` if the endpoint is
    /// not available.
    pub async fn fetch_my_servers(&self) -> ClientResult<Vec<StoatServer>> {
        let response = self
            .authenticated_request(Method::GET, "/users/@me/servers")?
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !response.status().is_success() {
            return Err(ClientError::NotSupported(
                "Server listing endpoint not available".to_string(),
            ));
        }

        response.json().await.map_err(|e| Self::network_error(&e))
    }

    /// Fetch a Stoat server by ID.
    pub async fn fetch_server(&self, server_id: &str) -> ClientResult<StoatServer> {
        let response = self
            .authenticated_request(Method::GET, &format!("/servers/{server_id}"))?
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(|e| Self::network_error(&e))
    }

    /// Fetch all members for a Stoat server.
    pub async fn fetch_server_members(
        &self,
        server_id: &str,
    ) -> ClientResult<StoatAllMemberResponse> {
        let response = self
            .authenticated_request(Method::GET, &format!("/servers/{server_id}/members"))?
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(|e| Self::network_error(&e))
    }

    /// Fetch a Stoat channel by ID.
    pub async fn fetch_channel(&self, channel_id: &str) -> ClientResult<StoatChannel> {
        let response = self
            .authenticated_request(Method::GET, &format!("/channels/{channel_id}"))?
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(|e| Self::network_error(&e))
    }

    /// Fetch the authenticated account's DM and group channels.
    pub async fn fetch_direct_message_channels(&self) -> ClientResult<Vec<StoatChannel>> {
        let response = self
            .authenticated_request(Method::GET, "/users/dms")?
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(|e| Self::network_error(&e))
    }

    /// Open or create a direct-message-like channel with the target user.
    ///
    /// Stoat returns a normal one-to-one DM for another user, and returns the
    /// personal Saved Messages channel when the target is the authenticated
    /// user themself.
    pub async fn open_direct_message_channel(&self, user_id: &str) -> ClientResult<StoatChannel> {
        let response = self
            .authenticated_request(Method::GET, &format!("/users/{user_id}/dm"))?
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(|e| Self::network_error(&e))
    }

    /// Fetch all members in a Stoat group DM.
    pub async fn fetch_group_members(&self, channel_id: &str) -> ClientResult<Vec<StoatUser>> {
        let response = self
            .authenticated_request(Method::GET, &format!("/channels/{channel_id}/members"))?
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(|e| Self::network_error(&e))
    }

    /// Add a member to a Stoat group DM.
    pub async fn add_group_member(&self, group_id: &str, member_id: &str) -> ClientResult<()> {
        let response = self
            .authenticated_request(
                Method::PUT,
                &format!("/channels/{group_id}/recipients/{member_id}"),
            )?
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        Ok(())
    }

    /// Remove a member from a Stoat group DM.
    pub async fn remove_group_member(&self, group_id: &str, member_id: &str) -> ClientResult<()> {
        let response = self
            .authenticated_request(
                Method::DELETE,
                &format!("/channels/{group_id}/recipients/{member_id}"),
            )?
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        Ok(())
    }

    /// Update channel settings (`PATCH /channels/{channel_id}`).
    pub async fn edit_channel(
        &self,
        channel_id: &str,
        edit: &StoatChannelEdit,
    ) -> ClientResult<()> {
        let response = self
            .authenticated_request(Method::PATCH, &format!("/channels/{channel_id}"))?
            .json(edit)
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !(response.status().is_success() || response.status().as_u16() == 204) {
            return Err(Self::parse_error(response).await);
        }
        Ok(())
    }

    /// Close a DM channel or leave a group DM (`DELETE /channels/{channel_id}`).
    ///
    /// For 1-on-1 DMs this hides the conversation; the channel reopens when a
    /// new message arrives.  For Group channels the caller leaves the group.
    pub async fn close_or_leave_channel(&self, channel_id: &str) -> ClientResult<()> {
        let response = self
            .authenticated_request(
                Method::DELETE,
                &format!("/channels/{channel_id}?leave_silent=true"),
            )?
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !(response.status().is_success() || response.status().as_u16() == 204) {
            return Err(Self::parse_error(response).await);
        }
        Ok(())
    }

    /// Edit a Group DM's metadata (`PATCH /channels/{channel_id}`).
    pub async fn edit_group_dm(
        &self,
        channel_id: &str,
        edit: &StoatGroupEdit,
    ) -> ClientResult<()> {
        let response = self
            .authenticated_request(Method::PATCH, &format!("/channels/{channel_id}"))?
            .json(edit)
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !(response.status().is_success() || response.status().as_u16() == 204) {
            return Err(Self::parse_error(response).await);
        }
        Ok(())
    }

    /// Create a server invite via the first available channel
    /// (`POST /channels/{channel_id}/invites`).
    ///
    /// Revolt's invite model is channel-scoped (not server-scoped), so the
    /// caller must resolve a suitable channel ID first.
    pub async fn create_channel_invite(
        &self,
        channel_id: &str,
    ) -> ClientResult<crate::api::StoatCreateInviteResponse> {
        let response = self
            .authenticated_request(
                Method::POST,
                &format!("/channels/{channel_id}/invites"),
            )?
            .json(&serde_json::json!({}))
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(|e| Self::network_error(&e))
    }
}
