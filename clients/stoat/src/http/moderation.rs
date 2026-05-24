//! Moderation endpoints (kick / ban / timeout / member edit).
//!
//! - `GET    /servers/{id}/members/@me` (permission resolution)
//! - `DELETE /servers/{id}/members/{member_id}` (kick)
//! - `PUT    /servers/{id}/bans/{user_id}`     (ban)
//! - `DELETE /servers/{id}/bans/{user_id}`     (unban)
//! - `GET    /servers/{id}/bans`               (list bans)
//! - `PATCH  /servers/{id}/members/{member_id}` (timeout / role edit)
//!
//! Split out from the monolithic `http.rs` in SOLID-audit-stoat D.3.

use super::StoatHttpClient;
use crate::api::{StoatBanCreate, StoatBansResponse, StoatMemberEdit};
use poly_client::ClientResult;
use poly_host_bridge::http::Method;

impl StoatHttpClient {
    // ── Moderation (B-ST) ────────────────────────────────────────────────────

    /// Fetch the calling member's own record for a server.
    ///
    /// Used to compute `MemberPermissions` by merging each assigned role's
    /// permission bits (with the server owner getting all bits set).
    pub async fn fetch_my_member(
        &self,
        server_id: &str,
    ) -> ClientResult<crate::api::StoatServerMemberMe> {
        let response = self
            .authenticated_request(
                Method::GET,
                &format!("/servers/{server_id}/members/@me"),
            )?
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(|e| Self::network_error(&e))
    }

    /// Kick a member from a server (`DELETE /servers/{server_id}/members/{member_id}`).
    pub async fn kick_member(&self, server_id: &str, member_id: &str) -> ClientResult<()> {
        let response = self
            .authenticated_request(
                Method::DELETE,
                &format!("/servers/{server_id}/members/{member_id}"),
            )?
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !(response.status().is_success() || response.status().as_u16() == 204) {
            return Err(Self::parse_error(response).await);
        }
        Ok(())
    }

    /// Permanently ban a user from a server (`PUT /servers/{server_id}/bans/{user_id}`).
    pub async fn ban_member(
        &self,
        server_id: &str,
        user_id: &str,
        ban: &StoatBanCreate,
    ) -> ClientResult<()> {
        let response = self
            .authenticated_request(
                Method::PUT,
                &format!("/servers/{server_id}/bans/{user_id}"),
            )?
            .json(ban)
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !(response.status().is_success() || response.status().as_u16() == 204) {
            return Err(Self::parse_error(response).await);
        }
        Ok(())
    }

    /// Lift a ban from a user (`DELETE /servers/{server_id}/bans/{user_id}`).
    pub async fn unban_member(&self, server_id: &str, user_id: &str) -> ClientResult<()> {
        let response = self
            .authenticated_request(
                Method::DELETE,
                &format!("/servers/{server_id}/bans/{user_id}"),
            )?
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !(response.status().is_success() || response.status().as_u16() == 204) {
            return Err(Self::parse_error(response).await);
        }
        Ok(())
    }

    /// Get the list of banned users for a server (`GET /servers/{server_id}/bans`).
    pub async fn get_bans(&self, server_id: &str) -> ClientResult<StoatBansResponse> {
        let response = self
            .authenticated_request(Method::GET, &format!("/servers/{server_id}/bans"))?
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(|e| Self::network_error(&e))
    }

    /// Edit a server member's record (`PATCH /servers/{server_id}/members/{member_id}`).
    ///
    /// Used for both timeout (`{timeout: ISO8601}`) and untimeout (`{remove: ["Timeout"]}`).
    pub async fn edit_member(
        &self,
        server_id: &str,
        member_id: &str,
        edit: &StoatMemberEdit,
    ) -> ClientResult<()> {
        let response = self
            .authenticated_request(
                Method::PATCH,
                &format!("/servers/{server_id}/members/{member_id}"),
            )?
            .json(edit)
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !(response.status().is_success() || response.status().as_u16() == 204) {
            return Err(Self::parse_error(response).await);
        }
        Ok(())
    }
}
