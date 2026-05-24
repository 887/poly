//! Authentication and session lifecycle endpoints.
//!
//! - `POST /auth/session/login`
//! - `POST /auth/session/logout`
//! - `GET  /users/@me` (used to resolve the current user after token-restore)
//!
//! Split out from the monolithic `http.rs` in SOLID-audit-stoat D.3.

use super::{STOAT_SESSION_TOKEN_HEADER, StoatHttpClient, StoatSessionState};
use crate::api::{
    StoatAuthenticatedSession, StoatLoginResponse, StoatPasswordLoginRequest, StoatUser,
};
use poly_client::{ClientError, ClientResult};
use poly_host_bridge::http::Method;

impl StoatHttpClient {
    /// Authenticate with email/password and populate session state.
    pub async fn login_with_password(
        &self,
        email: &str,
        password: &str,
        friendly_name: Option<&str>,
    ) -> ClientResult<StoatAuthenticatedSession> {
        let response = self
            .request(Method::POST, "/auth/session/login")
            .json(&StoatPasswordLoginRequest {
                email: email.to_string(),
                password: password.to_string(),
                friendly_name: friendly_name.map(std::string::ToString::to_string),
            })
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        let login = response
            .json::<StoatLoginResponse>()
            .await
            .map_err(|e| Self::network_error(&e))?
            .into_success()?;

        let (user, root_config) = tokio::try_join!(
            self.fetch_self_with_token(&login.token),
            self.fetch_server_config(),
        )?;
        self.set_ws_url(root_config.ws.clone());
        let authenticated = StoatAuthenticatedSession {
            session_id: login.session_id,
            user_id: login.user_id,
            token: login.token.clone(),
            user: user.into_poly_user_with_autumn(root_config.autumn_base_url()),
            session_name: login.session_name,
        };

        self.set_session(StoatSessionState {
            token: authenticated.token.clone(),
            session_id: Some(authenticated.session_id.clone()),
            user_id: Some(authenticated.user_id.clone()),
            user_display_name: Some(authenticated.user.display_name.clone()),
        })?;

        Ok(authenticated)
    }

    /// Restore an already-issued session token and resolve the current user.
    pub async fn authenticate_with_token(
        &self,
        token: String,
    ) -> ClientResult<StoatAuthenticatedSession> {
        let (user, root_config) = futures::future::try_join(
            self.fetch_self_with_token(&token),
            self.fetch_server_config(),
        )
        .await?;
        self.set_ws_url(root_config.ws.clone());
        let session = StoatAuthenticatedSession {
            // TODO(phase-3.1.2.2): fetch session inventory from Stoat when we
            // need an exact session identifier for token-restore flows.
            session_id: user.id.clone(),
            user_id: user.id.clone(),
            token: token.clone(),
            user: user.into_poly_user_with_autumn(root_config.autumn_base_url()),
            session_name: None,
        };

        self.set_session(StoatSessionState {
            token,
            session_id: Some(session.session_id.clone()),
            user_id: Some(session.user_id.clone()),
            user_display_name: Some(session.user.display_name.clone()),
        })?;

        Ok(session)
    }

    /// Fetch the authenticated user's full Stoat profile.
    pub async fn fetch_self(&self) -> ClientResult<StoatUser> {
        let token = self.session().map(|session| session.token).ok_or_else(|| {
            ClientError::AuthFailed("Stoat client is not authenticated".to_string())
        })?;

        self.fetch_self_with_token(&token).await
    }

    pub(super) async fn fetch_self_with_token(&self, token: &str) -> ClientResult<StoatUser> {
        let response = self
            .request(Method::GET, "/users/@me")
            .header(STOAT_SESSION_TOKEN_HEADER, token)
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !response.status().is_success() {
            return Err(Self::parse_error(response).await);
        }

        response.json().await.map_err(|e| Self::network_error(&e))
    }

    /// Log out the current Stoat session.
    pub async fn logout(&self) -> ClientResult<()> {
        if !self.is_authenticated() {
            return Ok(());
        }

        let response = self
            .authenticated_request(Method::POST, "/auth/session/logout")?
            .send()
            .await
            .map_err(|e| Self::network_error(&e))?;

        if !(response.status().is_success() || response.status().as_u16() == 204) {
            return Err(Self::parse_error(response).await);
        }

        self.clear_session()
    }
}
