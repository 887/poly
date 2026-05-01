//! Microsoft Graph OAuth2 — Device Code Flow + PKCE helpers.
//!
//! Targets `login.microsoftonline.com/common/oauth2/v2.0/*`. Uses the
//! public `ttyms` Azure AD client ID by default (a community Teams CLI) so
//! a Poly-owned registration isn't required to get started.
//!
//! Two flows live here:
//!
//! 1. **Device Code** — headless / terminal-friendly. Ideal when a browser
//!    is unavailable. User visits a URL and types a short code.
//! 2. **PKCE (Authorization Code + PKCE)** — desktop browser flow. The host
//!    shell opens a system browser to the auth URL, a loopback `http://127.0.0.1:<port>`
//!    one-shot listener catches the redirect, and we exchange the code for tokens.
//!
//! ## Scopes
//! Minimal delegated set for read/send across teams, channels, chats, and
//! presence, plus `offline_access` for refresh tokens.
//!
//! ## Refresh
//! `refresh_access_token` trades a refresh token for a new access token —
//! call it when a 401 comes back and only surface the reauth prompt when the
//! refresh itself fails.

use std::time::Duration;

use poly_client::ClientError;
use poly_host_bridge::http::{HttpClient, RequestBuilder, Response};
use serde::{Deserialize, Serialize};

const MAX_AUTH_ATTEMPTS: u32 = 3;
const DEFAULT_RETRY_AFTER_SECS: u64 = 1;
const MAX_BACKOFF_SECS: u64 = 30;

/// Run a `login.microsoftonline.com` POST through up to 3 attempts. Honors
/// `Retry-After` on 429; exponential backoff on 5xx. 4xx (other than 429)
/// returns immediately so the caller can inspect the OAuth error body.
async fn send_oauth_retry<F>(make_req: F) -> Result<Response, ClientError>
where
    F: Fn() -> RequestBuilder,
{
    let mut attempt: u32 = 0;
    loop {
        attempt = attempt.saturating_add(1);
        let resp = make_req()
            .send()
            .await
            .map_err(|e| ClientError::Network(e.to_string()))?;
        let status = resp.status().as_u16();
        let retryable = status == 429 || (500..600).contains(&status);
        if !retryable || attempt >= MAX_AUTH_ATTEMPTS {
            return Ok(resp);
        }
        let delay = if status == 429 {
            resp.headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(DEFAULT_RETRY_AFTER_SECS)
        } else {
            1u64 << attempt.saturating_sub(1)
        }
        .min(MAX_BACKOFF_SECS);
        tokio::time::sleep(Duration::from_secs(delay)).await;
    }
}

/// Community-maintained Azure AD client ID shipped with `ttyms`.
/// Covers delegated scopes without admin consent. Replace with a Poly-owned
/// registration once one exists.
pub const DEFAULT_CLIENT_ID: &str = "04b07795-8ddb-461a-bbee-02f9e1bf7b46";

/// Default tenant — `common` lets any Microsoft account sign in (work, school, personal).
pub const DEFAULT_TENANT: &str = "common";

/// Minimal delegated scopes for channel + chat read/write + presence + refresh.
pub const DEFAULT_SCOPES: &[&str] = &[
    "User.Read",
    "Team.ReadBasic.All",
    "Channel.ReadBasic.All",
    "ChannelMessage.Read.All",
    "ChannelMessage.Send",
    "Chat.Read",
    "Chat.ReadWrite",
    "Presence.Read",
    "offline_access",
];

fn authority_base(tenant: &str) -> String {
    format!("https://login.microsoftonline.com/{tenant}/oauth2/v2.0")
}

/// Response from `/devicecode` — show `user_code` + `verification_uri` to the user,
/// then poll `/token` with `device_code` until they complete sign-in.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
    pub message: Option<String>,
}

/// Token bundle returned on successful exchange. `refresh_token` is only
/// present when `offline_access` is in the requested scopes.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OAuthError {
    error: String,
}

/// Kick off Device Code flow. The returned `user_code` should be shown to the
/// user along with `verification_uri`; then repeatedly call
/// [`poll_device_code_token`] with `device_code` until sign-in completes.
pub async fn start_device_code(
    tenant: &str,
    client_id: &str,
    scopes: &[&str],
) -> Result<DeviceCodeResponse, ClientError> {
    let http = HttpClient::new();
    let url = format!("https://login.microsoftonline.com/{tenant}/oauth2/v2.0/devicecode");
    let body = format!(
        "client_id={}&scope={}",
        urlencoding::encode(client_id),
        urlencoding::encode(&scopes.join(" "))
    );
    let resp = send_oauth_retry(|| {
        http.post(url.clone())
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body.clone())
    })
    .await?;
    if !resp.status().is_success() {
        return Err(ClientError::Network(format!(
            "devicecode HTTP {}",
            resp.status().as_u16()
        )));
    }
    resp.json::<DeviceCodeResponse>()
        .await
        .map_err(|e| ClientError::Internal(e.to_string()))
}

/// Poll `/token` with `device_code=...`. Callers should respect `interval`
/// from the device-code response between polls.
///
/// Returns:
/// - `Ok(Some(tokens))` on success
/// - `Ok(None)` when the user hasn't completed sign-in yet (`authorization_pending`)
/// - `Err(AuthFailed)` on `authorization_declined`, `expired_token`, `bad_verification_code`
/// - `Err(Network)` on other transport/HTTP failures
pub async fn poll_device_code_token(
    tenant: &str,
    client_id: &str,
    device_code: &str,
) -> Result<Option<TokenResponse>, ClientError> {
    let http = HttpClient::new();
    let url = format!("{}/token", authority_base(tenant));
    let body = format!(
        "grant_type=urn:ietf:params:oauth:grant-type:device_code&client_id={}&device_code={}",
        urlencoding::encode(client_id),
        urlencoding::encode(device_code),
    );
    let resp = send_oauth_retry(|| {
        http.post(url.clone())
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body.clone())
    })
    .await?;
    let status = resp.status();
    if status.is_success() {
        return resp
            .json::<TokenResponse>()
            .await
            .map(Some)
            .map_err(|e| ClientError::Internal(e.to_string()));
    }
    let err: OAuthError = resp
        .json()
        .await
        .unwrap_or(OAuthError {
            error: format!("http_{}", status.as_u16()),
        });
    match err.error.as_str() {
        "authorization_pending" | "slow_down" => Ok(None),
        "authorization_declined" => Err(ClientError::AuthFailed("Sign-in declined".into())),
        "expired_token" | "bad_verification_code" => {
            Err(ClientError::AuthFailed(format!("Device code invalid: {}", err.error)))
        }
        other => Err(ClientError::Network(format!("devicecode poll: {other}"))),
    }
}

/// Trade a refresh token for a fresh access token. Silent reauth — call on 401.
/// Microsoft may rotate the refresh token; `refresh_token` in the response (if present)
/// replaces the old one.
pub async fn refresh_access_token(
    tenant: &str,
    client_id: &str,
    refresh_token: &str,
    scopes: &[&str],
) -> Result<TokenResponse, ClientError> {
    let http = HttpClient::new();
    let url = format!("{}/token", authority_base(tenant));
    let body = format!(
        "grant_type=refresh_token&client_id={}&refresh_token={}&scope={}",
        urlencoding::encode(client_id),
        urlencoding::encode(refresh_token),
        urlencoding::encode(&scopes.join(" ")),
    );
    let resp = send_oauth_retry(|| {
        http.post(url.clone())
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body.clone())
    })
    .await?;
    if !resp.status().is_success() {
        return Err(ClientError::AuthFailed(format!(
            "refresh failed: HTTP {}",
            resp.status().as_u16()
        )));
    }
    resp.json::<TokenResponse>()
        .await
        .map_err(|e| ClientError::Internal(e.to_string()))
}

/// Build the Authorization Code + PKCE authorize URL. The shell opens this
/// in a browser; the redirect lands on `http://127.0.0.1:<port>` with `code=`.
///
/// `code_verifier` — caller-generated 43-128 char random string. Hash it with
/// SHA-256 + base64url (no padding) to get the `code_challenge`.
#[must_use] 
pub fn build_pkce_authorize_url(
    tenant: &str,
    client_id: &str,
    redirect_uri: &str,
    scopes: &[&str],
    code_challenge: &str,
    state: &str,
) -> String {
    format!(
        "https://login.microsoftonline.com/{tenant}/oauth2/v2.0/authorize?\
         client_id={}&response_type=code&redirect_uri={}&scope={}\
         &code_challenge={}&code_challenge_method=S256&state={}",
        urlencoding::encode(client_id),
        urlencoding::encode(redirect_uri),
        urlencoding::encode(&scopes.join(" ")),
        urlencoding::encode(code_challenge),
        urlencoding::encode(state),
    )
}

/// Exchange an Authorization Code for tokens (PKCE leg 2).
pub async fn exchange_pkce_code(
    tenant: &str,
    client_id: &str,
    code: &str,
    redirect_uri: &str,
    code_verifier: &str,
) -> Result<TokenResponse, ClientError> {
    let http = HttpClient::new();
    let url = format!("{}/token", authority_base(tenant));
    let body = format!(
        "grant_type=authorization_code&client_id={}&code={}&redirect_uri={}&code_verifier={}",
        urlencoding::encode(client_id),
        urlencoding::encode(code),
        urlencoding::encode(redirect_uri),
        urlencoding::encode(code_verifier),
    );
    let resp = send_oauth_retry(|| {
        http.post(url.clone())
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body.clone())
    })
    .await?;
    if !resp.status().is_success() {
        return Err(ClientError::AuthFailed(format!(
            "code exchange failed: HTTP {}",
            resp.status().as_u16()
        )));
    }
    resp.json::<TokenResponse>()
        .await
        .map_err(|e| ClientError::Internal(e.to_string()))
}
