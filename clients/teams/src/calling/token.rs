//! ACS access-token acquisition — Phase B of
//! `docs/plans/plan-teams-calling.md`.
//!
//! The Microsoft Graph endpoint
//! `POST /v1.0/users/{id}/teamwork/sendActivityNotification` is **not** a
//! calling-token endpoint despite older docs implying otherwise. Calling
//! tokens come from the ACS Identity REST API:
//!
//! ```text
//! POST {acsEndpoint}/identities/{acsUserId}/access-tokens?api-version=2023-10-01
//! Authorization: Bearer <ACS resource access key signed JWT>
//! Content-Type: application/json
//!
//! { "scopes": ["voip"] }
//! ```
//!
//! Response:
//!
//! ```json
//! { "token": "<jwt>", "expiresOn": "2026-05-25T18:30:00Z" }
//! ```
//!
//! The Bearer used here is **not** the user's Graph token — it's an
//! HMAC-signed JWT minted from the ACS resource's access key, or in the
//! Entra-managed-identity case a federated token. Generating that signed
//! JWT is a server-side responsibility (the access key must never reach
//! the client). This module ships the request scaffolding; the
//! signing-bearer is acquired through an injected provider so an
//! eventual server-side relay (analogous to the Phase A.1 webhook relay
//! in `plan-teams-graph-subscriptions.md`) can plug in without further
//! changes here.
//!
//! ## Refresh
//!
//! Tokens live ~24h; refresh at 22h with jitter (Phase B.2). The
//! [`AcsTokenAcquirer::seconds_until_refresh`] helper centralises this
//! so the scheduler logic stays uniform with the OAuth refresh path in
//! [`crate::auth`].

use poly_client::ClientError;
use poly_host_bridge::http::HttpClient;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use super::types::{AcsAccessToken, CallingError};

/// Configuration for the ACS Identity token endpoint.
#[derive(Debug, Clone)]
pub struct TokenAcquisitionConfig {
    /// Base URL of the ACS resource, e.g.
    /// `https://contoso.communication.azure.com`.
    pub acs_endpoint: String,
    /// ACS user identity (`8:acs:<resource-guid>_<user-guid>`).
    pub acs_user_id: String,
    /// API version segment for the ACS Identity REST API.
    pub api_version: &'static str,
    /// Scopes to request. Today only `["voip"]` is meaningful for
    /// calling; chat tokens use a different scope set.
    pub scopes: Vec<String>,
}

impl TokenAcquisitionConfig {
    /// Build a config with the default API version and `voip` scope.
    #[must_use]
    pub fn voip(acs_endpoint: impl Into<String>, acs_user_id: impl Into<String>) -> Self {
        Self {
            acs_endpoint: acs_endpoint.into(),
            acs_user_id: acs_user_id.into(),
            api_version: "2023-10-01",
            scopes: vec!["voip".into()],
        }
    }
}

/// Source of the privileged Bearer that authorises the ACS Identity
/// REST call. In production this is an HMAC-signed JWT minted from the
/// ACS resource access key (server-side) — never the user's Graph
/// token. Inject so the calling subsystem stays free of secret material.
pub trait AcsAdminBearer: Send + Sync {
    /// Return a Bearer token (without the `Bearer ` prefix) valid for
    /// `POST {acsEndpoint}/identities/.../access-tokens`.
    fn admin_bearer(&self) -> Result<String, CallingError>;
}

/// Thin wrapper that drives `POST .../access-tokens` and parses the
/// response.
///
/// Stateless — caller decides where to cache tokens and when to refresh
/// (use [`Self::seconds_until_refresh`]).
pub struct AcsTokenAcquirer {
    http: HttpClient,
    config: TokenAcquisitionConfig,
}

impl AcsTokenAcquirer {
    /// Construct a new acquirer over a shared `HttpClient`.
    #[must_use]
    pub fn new(http: HttpClient, config: TokenAcquisitionConfig) -> Self {
        Self { http, config }
    }

    /// Hit `POST {acsEndpoint}/identities/{acsUserId}/access-tokens` and
    /// return the parsed token bundle.
    ///
    /// # Errors
    ///
    /// - [`CallingError::Network`] on transport failure.
    /// - [`CallingError::TokenAcquisition`] on non-2xx ACS response or
    ///   JSON-decode failure.
    pub async fn acquire(
        &self,
        bearer_provider: &dyn AcsAdminBearer,
    ) -> Result<AcsAccessToken, CallingError> {
        let url = format!(
            "{}/identities/{}/access-tokens?api-version={}",
            self.config.acs_endpoint.trim_end_matches('/'),
            self.config.acs_user_id,
            self.config.api_version,
        );
        let bearer = bearer_provider.admin_bearer()?;
        let body = serde_json::json!({ "scopes": self.config.scopes });
        let body_bytes = serde_json::to_vec(&body)
            .map_err(|e| CallingError::Internal(format!("serialize token request: {e}")))?;

        let resp = self
            .http
            .post(url.clone())
            .header("Authorization", format!("Bearer {bearer}"))
            .header("Content-Type", "application/json")
            .body(body_bytes)
            .send()
            .await
            .map_err(|e| CallingError::Network(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(CallingError::TokenAcquisition(format!(
                "ACS Identity returned HTTP {}",
                resp.status().as_u16()
            )));
        }
        let parsed: AcsTokenResponse = resp
            .json()
            .await
            .map_err(|e| CallingError::TokenAcquisition(format!("parse response: {e}")))?;
        Ok(AcsAccessToken {
            token: parsed.token,
            expires_on: parsed.expires_on,
        })
    }

    /// Compute the refresh delay relative to the token's `expires_on`.
    ///
    /// Returns roughly `lifetime - 2h ± jitter`. Falls back to
    /// `Duration::from_secs(0)` when `expires_on` is missing or
    /// unparseable so the caller refreshes immediately.
    ///
    /// `jitter_seconds_provider` is injected so tests can pin it; in
    /// production wire to a small RNG.
    #[must_use]
    pub fn seconds_until_refresh(
        token: &AcsAccessToken,
        now: chrono::DateTime<chrono::Utc>,
        jitter_seconds_provider: &dyn Fn() -> i64,
    ) -> Duration {
        let Some(ref expires) = token.expires_on else {
            return Duration::from_secs(0);
        };
        let Ok(expiry) = chrono::DateTime::parse_from_rfc3339(expires) else {
            return Duration::from_secs(0);
        };
        let expiry_utc = expiry.with_timezone(&chrono::Utc);
        // Two-hour safety window before expiry.
        let target = expiry_utc - chrono::Duration::hours(2);
        let jitter = jitter_seconds_provider();
        let target_with_jitter = target + chrono::Duration::seconds(jitter);
        let delta = (target_with_jitter - now).num_seconds();
        if delta <= 0 {
            Duration::from_secs(0)
        } else {
            Duration::from_secs(delta.unsigned_abs())
        }
    }
}

/// Wire shape of the ACS Identity response.
#[derive(Debug, Serialize, Deserialize)]
struct AcsTokenResponse {
    token: String,
    #[serde(rename = "expiresOn")]
    expires_on: Option<String>,
}

/// Convenience: hoist a [`CallingError`] to [`ClientError`] for the
/// places that already speak `Result<_, ClientError>`.
#[must_use]
pub fn into_client_error(e: CallingError) -> ClientError {
    e.into()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    struct StaticBearer(&'static str);
    impl AcsAdminBearer for StaticBearer {
        fn admin_bearer(&self) -> Result<String, CallingError> {
            Ok(self.0.into())
        }
    }

    struct FailingBearer;
    impl AcsAdminBearer for FailingBearer {
        fn admin_bearer(&self) -> Result<String, CallingError> {
            Err(CallingError::AcsNotProvisioned("no admin key".into()))
        }
    }

    #[test]
    fn voip_config_defaults() {
        let c = TokenAcquisitionConfig::voip("https://x/", "8:acs:y");
        assert_eq!(c.acs_endpoint, "https://x/");
        assert_eq!(c.acs_user_id, "8:acs:y");
        assert_eq!(c.api_version, "2023-10-01");
        assert_eq!(c.scopes, vec!["voip".to_string()]);
    }

    #[test]
    fn seconds_until_refresh_missing_expiry_returns_zero() {
        let tok = AcsAccessToken {
            token: "t".into(),
            expires_on: None,
        };
        let d = AcsTokenAcquirer::seconds_until_refresh(
            &tok,
            chrono::Utc::now(),
            &|| 0,
        );
        assert_eq!(d, Duration::from_secs(0));
    }

    #[test]
    fn seconds_until_refresh_two_hour_window() {
        // expiry = now + 24h, expect refresh in ~22h (-2h safety).
        let now = chrono::Utc::now();
        let expiry = now + chrono::Duration::hours(24);
        let tok = AcsAccessToken {
            token: "t".into(),
            expires_on: Some(expiry.to_rfc3339()),
        };
        let d =
            AcsTokenAcquirer::seconds_until_refresh(&tok, now, &|| 0);
        let secs = d.as_secs() as i64;
        // 22h ± 5s slack for clock drift in test runtime.
        assert!(
            (22 * 3600 - 5..=22 * 3600 + 5).contains(&secs),
            "expected ~22h, got {secs}s"
        );
    }

    #[test]
    fn seconds_until_refresh_past_expiry_returns_zero() {
        let now = chrono::Utc::now();
        let expiry = now - chrono::Duration::hours(1);
        let tok = AcsAccessToken {
            token: "t".into(),
            expires_on: Some(expiry.to_rfc3339()),
        };
        let d =
            AcsTokenAcquirer::seconds_until_refresh(&tok, now, &|| 0);
        assert_eq!(d, Duration::from_secs(0));
    }

    #[test]
    fn into_client_error_round_trip() {
        let ce = into_client_error(CallingError::Network("dns".into()));
        assert!(matches!(ce, ClientError::Network(_)));
    }

    #[test]
    fn failing_bearer_propagates() {
        let b = FailingBearer;
        assert!(matches!(
            b.admin_bearer().unwrap_err(),
            CallingError::AcsNotProvisioned(_)
        ));
    }

    #[test]
    fn static_bearer_returns_value() {
        let b = StaticBearer("abc");
        assert_eq!(b.admin_bearer().unwrap(), "abc");
    }
}
