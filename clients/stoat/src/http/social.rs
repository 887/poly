//! Social graph endpoints (friends, blocks, user fetch).
//!
//! - `GET    /users/{id}` (user fetch)
//! - `POST   /users/friend` (send-by-username)
//! - `PUT    /users/{id}/friend` (accept / add-by-id)
//! - `DELETE /users/{id}/friend` (deny / remove)
//! - `PUT    /users/{id}/block`
//! - `DELETE /users/{id}/block`
//!
//! Split out from the monolithic `http.rs` in SOLID-audit-stoat D.3.

use super::StoatHttpClient;
use crate::api::{StoatSendFriendRequest, StoatUser};
use poly_client::ClientResult;
use poly_host_bridge::http::Method;

impl StoatHttpClient {
    /// Fetch a Stoat user by ID.
    pub async fn fetch_user(&self, user_id: &str) -> ClientResult<StoatUser> {
        let response = self
            .authenticated_request(Method::GET, &format!("/users/{user_id}"))?
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(|e| Self::network_error(&e))
    }

    /// Send a Stoat friend request by username/discriminator.
    pub async fn send_friend_request(&self, username: &str) -> ClientResult<StoatUser> {
        let response = self
            .authenticated_request(Method::POST, "/users/friend")?
            .json(&StoatSendFriendRequest {
                username: username.to_string(),
            })
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(|e| Self::network_error(&e))
    }

    /// Accept a pending Stoat friend request.
    pub async fn accept_friend_request(&self, user_id: &str) -> ClientResult<StoatUser> {
        let response = self
            .authenticated_request(Method::PUT, &format!("/users/{user_id}/friend"))?
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(|e| Self::network_error(&e))
    }

    /// Deny a pending Stoat friend request or remove an existing friend.
    pub async fn remove_friend(&self, user_id: &str) -> ClientResult<StoatUser> {
        let response = self
            .authenticated_request(Method::DELETE, &format!("/users/{user_id}/friend"))?
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(|e| Self::network_error(&e))
    }

    /// Block a user (`PUT /users/{user_id}/block`).
    pub async fn block_user(&self, user_id: &str) -> ClientResult<()> {
        let response = self
            .authenticated_request(Method::PUT, &format!("/users/{user_id}/block"))?
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !(response.status().is_success() || response.status().as_u16() == 204) {
            return Err(Self::parse_error(response).await);
        }
        Ok(())
    }

    /// Unblock a user (`DELETE /users/{user_id}/block`).
    pub async fn unblock_user(&self, user_id: &str) -> ClientResult<()> {
        let response = self
            .authenticated_request(Method::DELETE, &format!("/users/{user_id}/block"))?
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !(response.status().is_success() || response.status().as_u16() == 204) {
            return Err(Self::parse_error(response).await);
        }
        Ok(())
    }

    /// Send a friend request by user ID (`PUT /users/{user_id}/friend`).
    ///
    /// Stoat reuses this endpoint for both sending and accepting a request;
    /// the server resolves the correct transition.
    pub async fn add_friend(&self, user_id: &str) -> ClientResult<()> {
        let response = self
            .authenticated_request(Method::PUT, &format!("/users/{user_id}/friend"))?
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !(response.status().is_success() || response.status().as_u16() == 204) {
            return Err(Self::parse_error(response).await);
        }
        Ok(())
    }

    /// Remove a friend or cancel a pending request by user ID
    /// (`DELETE /users/{user_id}/friend`).
    ///
    /// Distinct from `remove_friend` which discards the user object; this
    /// variant simply performs the HTTP call and returns `()`.
    pub async fn remove_friend_by_id(&self, user_id: &str) -> ClientResult<()> {
        let response = self
            .authenticated_request(Method::DELETE, &format!("/users/{user_id}/friend"))?
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !(response.status().is_success() || response.status().as_u16() == 204) {
            return Err(Self::parse_error(response).await);
        }
        Ok(())
    }
}
