//! Microsoft Graph change-notification subscriptions — Phase B + part
//! of C of `docs/plans/plan-teams-graph-subscriptions.md`.
//!
//! Production Graph does NOT offer a long-polling event stream like the
//! test server's `/test/events/poll`. The production replacement is
//! the **change-notifications** API:
//!
//! 1. Client `POST /v1.0/subscriptions` with `notificationUrl`,
//!    `resource`, `expirationDateTime`, `clientState`,
//!    optionally `encryptionCertificate`.
//! 2. Microsoft synchronously validates the URL (`GET ?validationToken=…`
//!    → expects 200 + echoed token).
//! 3. Microsoft POSTs change notifications to the URL on resource
//!    change.
//! 4. Subscriptions expire (max 1h for chat messages, ~3 days for most
//!    other resources); client `PATCH /subscriptions/{id}` to renew
//!    before expiry.
//!
//! This module implements the **client side** of the lifecycle:
//! create / renew / delete plus a renewal-time helper.
//! The webhook handler that receives Microsoft's POSTs lives in
//! `crates/host-bridge/src/teams_webhook.rs` (Phase C).
//!
//! ## Encryption (rich notifications)
//!
//! Phase D of the plan — generate a per-tenant RSA keypair, encode the
//! public cert in the subscription request, decrypt incoming payloads
//! via AES-256-CBC + RSA-OAEP-SHA256 hybrid. **Deferred** — the
//! "resource-light" subscription path (notifications without resource
//! data) covers the common case and ships first.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Opaque subscription identifier returned by `POST /v1.0/subscriptions`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SubscriptionId(pub String);

impl From<String> for SubscriptionId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for SubscriptionId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl std::fmt::Display for SubscriptionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Request body for `POST /v1.0/subscriptions`.
///
/// Per the [Graph subscription docs](https://learn.microsoft.com/graph/api/subscription-post-subscriptions).
#[derive(Debug, Clone, Serialize)]
pub struct CreateSubscriptionRequest {
    /// Change type — `"created"`, `"updated"`, `"deleted"`, or a
    /// comma-joined list.
    #[serde(rename = "changeType")]
    pub change_type: String,
    /// Publicly accessible HTTPS URL Microsoft will POST notifications
    /// to.
    #[serde(rename = "notificationUrl")]
    pub notification_url: String,
    /// Resource path, e.g.
    /// `"/teams/{team-id}/channels/{channel-id}/messages"`.
    pub resource: String,
    /// Absolute expiration time (RFC3339). Must not exceed the
    /// resource's max lifetime.
    #[serde(rename = "expirationDateTime")]
    pub expiration_date_time: String,
    /// Opaque secret echoed in every notification — used for HMAC
    /// verification on the webhook side.
    #[serde(rename = "clientState")]
    pub client_state: String,
    /// Optional latest-supported TLS version for the relay (string like
    /// `"v1_2"`).
    #[serde(rename = "latestSupportedTlsVersion", skip_serializing_if = "Option::is_none")]
    pub latest_supported_tls_version: Option<String>,
    /// Whether Microsoft should embed the changed resource's data in
    /// the notification (`encryptedContent`). Required for messaging
    /// resources to receive message bodies without a follow-up fetch.
    /// When `true`, [`Self::encryption_certificate`] +
    /// [`Self::encryption_certificate_id`] MUST be set.
    #[serde(
        rename = "includeResourceData",
        skip_serializing_if = "Option::is_none"
    )]
    pub include_resource_data: Option<bool>,
    /// Base64-encoded PKCS#1 DER of the per-tenant RSA public key —
    /// produced by `poly_host_bridge::teams_encryption::TeamsKeyStore::
    /// public_certificate_b64()`. Phase D wire field.
    #[serde(
        rename = "encryptionCertificate",
        skip_serializing_if = "Option::is_none"
    )]
    pub encryption_certificate: Option<String>,
    /// Stable identifier echoed back by Microsoft in every
    /// `encryptedContent.encryptionCertificateId` field, so the
    /// receiver can pick the correct private key when multiple
    /// tenants / rotated keys are multiplexed through a single relay.
    #[serde(
        rename = "encryptionCertificateId",
        skip_serializing_if = "Option::is_none"
    )]
    pub encryption_certificate_id: Option<String>,
}

/// Response body for `POST /v1.0/subscriptions` (and `PATCH`).
#[derive(Debug, Clone, Deserialize)]
pub struct SubscriptionResponse {
    /// Subscription id (opaque).
    pub id: String,
    /// New / current absolute expiry (RFC3339).
    #[serde(rename = "expirationDateTime")]
    pub expiration_date_time: String,
    /// Resource path echoed back.
    #[serde(default)]
    pub resource: String,
    /// Notification URL echoed back.
    #[serde(default, rename = "notificationUrl")]
    pub notification_url: String,
}

/// Request body for `PATCH /v1.0/subscriptions/{id}` (renewal).
#[derive(Debug, Clone, Serialize)]
pub struct RenewSubscriptionRequest {
    /// New absolute expiry (RFC3339).
    #[serde(rename = "expirationDateTime")]
    pub expiration_date_time: String,
}

/// Resource lifetime caps published by Microsoft Graph.
///
/// Picked from the [resource-types table](https://learn.microsoft.com/graph/api/resources/subscription).
/// We store conservative values; the renewal scheduler uses these to
/// pick the renewal interval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    /// Channel message — `/teams/{team-id}/channels/{channel-id}/messages`.
    /// Max lifetime: ~60 minutes.
    ChannelMessage,
    /// Chat message — `/chats/{chat-id}/messages`. Max lifetime: ~60
    /// minutes.
    ChatMessage,
    /// User presence — `/communications/presences/{user-id}`. Max
    /// lifetime: ~60 minutes.
    UserPresence,
    /// User callRecord, mailbox, etc. Max lifetime: ~4230 minutes
    /// (~3 days).
    Generic,
}

impl ResourceKind {
    /// Maximum lifetime allowed by Graph for this resource kind.
    #[must_use]
    pub const fn max_lifetime(self) -> Duration {
        match self {
            Self::ChannelMessage | Self::ChatMessage | Self::UserPresence => {
                Duration::from_secs(60 * 60)
            }
            Self::Generic => Duration::from_secs(4230 * 60),
        }
    }

    /// Recommended renewal interval — `max_lifetime - safety_margin`.
    /// Safety margin is 5 minutes (matches plan B.4).
    #[must_use]
    pub fn renewal_interval(self) -> Duration {
        let max = self.max_lifetime();
        let margin = Duration::from_secs(5 * 60);
        if max > margin {
            max.checked_sub(margin).unwrap_or(Duration::from_secs(0))
        } else {
            // Pathological — fall back to half the lifetime.
            Duration::from_secs(max.as_secs().wrapping_div(2))
        }
    }
}

/// Compute the absolute RFC3339 `expirationDateTime` for a subscription
/// request given the current time and the resource kind.
///
/// Saturating: if `lifetime` overflows the max for the kind, we use the
/// max.
#[must_use]
pub fn compute_expiration_iso(
    now: chrono::DateTime<chrono::Utc>,
    kind: ResourceKind,
) -> String {
    let lifetime = kind.max_lifetime();
    let secs = i64::try_from(lifetime.as_secs()).unwrap_or(i64::MAX);
    let expiry = now
        .checked_add_signed(chrono::Duration::seconds(secs))
        .unwrap_or(now);
    expiry.to_rfc3339()
}

/// Generate an opaque `clientState` secret suitable for HMAC
/// verification on the webhook side.
///
/// Lightweight UUID-v4 — sufficient entropy (122 bits) without pulling
/// in a CSPRNG. Caller stores the value alongside the subscription id
/// so the webhook handler can compare on receipt.
#[must_use]
pub fn generate_client_state() -> String {
    uuid::Uuid::new_v4().to_string()
}

// ── Phase E.1 — long-poll fallback gate ──────────────────────────────────────

/// Decide whether to register Graph change-notification subscriptions
/// (webhooks) vs. long-poll the test server.
///
/// **Rule:** if `base_url` contains `"/test/"` it's the in-tree test
/// server (`servers/test-teams`) — it offers `/test/events/poll` and
/// does NOT speak the Graph subscriptions API. Real Graph deployments
/// (`https://graph.microsoft.com`) use webhooks. Pre-production
/// deployments that DO speak webhooks but still want to disable them
/// (operator hasn't stood up the relay yet) can layer the
/// [`use_webhooks_kv_key`] flag on top — see Phase E.2.
#[must_use]
pub fn should_use_webhooks(base_url: &str) -> bool {
    !base_url.contains("/test/")
}

// ── Phase E.2 — migration KV flag ────────────────────────────────────────────

/// KV key that controls whether the teams client tries to register Graph
/// subscriptions for an account. Defaults to `false` when absent — flip
/// to `true` after a successful `create_subscription` call, flip back
/// to `false` on subscription-setup failure so the long-poll path stays
/// the canonical event stream during operator rollout.
///
/// Lives in the `client.config.<backend_id>.*` namespace owned by
/// [`poly_host_bridge::client_config::ClientConfigStore`]; we use a
/// sub-key under `teams` rather than a per-account top-level key so the
/// existing `ClientConfigStore::list_overrides` snapshot mechanism
/// surfaces it for free.
#[must_use]
pub fn use_webhooks_kv_key(account_id: &str) -> String {
    format!("client.config.teams.use_webhooks.{account_id}")
}

#[cfg(feature = "native")]
pub use webhook_flag::{get_use_webhooks, set_use_webhooks};

#[cfg(feature = "native")]
mod webhook_flag {
    //! Read/write the `use_webhooks` KV flag through the host bridge.
    //!
    //! Both helpers fail-open: a missing key / unreachable bridge
    //! returns `Ok(false)` so the long-poll fallback stays the safe
    //! default. Errors only propagate when KV is genuinely broken
    //! (poisoned lock, JSON corruption) — those callers want to know.

    use super::use_webhooks_kv_key;
    use poly_host_bridge::{BridgeError, Client};

    /// Returns `true` when the per-account flag is set to `true`,
    /// `false` otherwise (absent key, non-bool value, or `false`).
    pub async fn get_use_webhooks(
        client: &Client,
        account_id: &str,
    ) -> Result<bool, BridgeError> {
        let key = use_webhooks_kv_key(account_id);
        match client.kv_get(&key).await? {
            Some(serde_json::Value::Bool(b)) => Ok(b),
            _ => Ok(false),
        }
    }

    /// Persist the flag. Flip to `true` after a successful
    /// `create_subscription`; flip back to `false` on rejection so the
    /// long-poll path keeps running.
    pub async fn set_use_webhooks(
        client: &Client,
        account_id: &str,
        enabled: bool,
    ) -> Result<(), BridgeError> {
        let key = use_webhooks_kv_key(account_id);
        client
            .kv_set(&key, serde_json::Value::Bool(enabled))
            .await
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn channel_message_max_lifetime_is_60min() {
        assert_eq!(
            ResourceKind::ChannelMessage.max_lifetime(),
            Duration::from_secs(3600)
        );
    }

    #[test]
    fn renewal_interval_subtracts_safety_margin() {
        assert_eq!(
            ResourceKind::ChannelMessage.renewal_interval(),
            Duration::from_secs(3600 - 300)
        );
    }

    #[test]
    fn generic_max_lifetime_is_3days() {
        assert_eq!(
            ResourceKind::Generic.max_lifetime(),
            Duration::from_secs(4230 * 60)
        );
    }

    #[test]
    fn compute_expiration_iso_in_future() {
        let now = chrono::Utc::now();
        let s = compute_expiration_iso(now, ResourceKind::ChannelMessage);
        let parsed = chrono::DateTime::parse_from_rfc3339(&s).unwrap();
        let delta = (parsed.with_timezone(&chrono::Utc) - now).num_seconds();
        assert!(
            (3590..=3610).contains(&delta),
            "expected ~3600s from now, got {delta}s"
        );
    }

    #[test]
    fn generate_client_state_is_uuid_shaped() {
        let s = generate_client_state();
        // 8-4-4-4-12 = 36 chars.
        assert_eq!(s.len(), 36, "got {s}");
        assert_eq!(s.matches('-').count(), 4);
    }

    #[test]
    fn subscription_id_round_trips_through_display() {
        let id = SubscriptionId::from("abc");
        assert_eq!(id.to_string(), "abc");
    }

    #[test]
    fn create_subscription_request_serializes_with_graph_field_names() {
        let req = CreateSubscriptionRequest {
            change_type: "created,updated".into(),
            notification_url: "https://relay/webhook".into(),
            resource: "/teams/T/channels/C/messages".into(),
            expiration_date_time: "2026-05-25T18:30:00Z".into(),
            client_state: "secret-abc".into(),
            latest_supported_tls_version: Some("v1_2".into()),
            include_resource_data: None,
            encryption_certificate: None,
            encryption_certificate_id: None,
        };
        let s = serde_json::to_value(&req).unwrap();
        assert_eq!(s["changeType"], "created,updated");
        assert_eq!(s["notificationUrl"], "https://relay/webhook");
        assert_eq!(s["resource"], "/teams/T/channels/C/messages");
        assert_eq!(s["expirationDateTime"], "2026-05-25T18:30:00Z");
        assert_eq!(s["clientState"], "secret-abc");
        assert_eq!(s["latestSupportedTlsVersion"], "v1_2");
        // Optional encryption fields omitted when None.
        assert!(s.get("includeResourceData").is_none());
        assert!(s.get("encryptionCertificate").is_none());
        assert!(s.get("encryptionCertificateId").is_none());
    }

    #[test]
    fn create_subscription_request_serializes_encryption_fields_when_set() {
        let req = CreateSubscriptionRequest {
            change_type: "created".into(),
            notification_url: "https://relay/webhook".into(),
            resource: "/chats/C/messages".into(),
            expiration_date_time: "2026-05-25T18:30:00Z".into(),
            client_state: "s".into(),
            latest_supported_tls_version: None,
            include_resource_data: Some(true),
            encryption_certificate: Some("BASE64DERCERT".into()),
            encryption_certificate_id: Some("cert-7".into()),
        };
        let s = serde_json::to_value(&req).unwrap();
        assert_eq!(s["includeResourceData"], true);
        assert_eq!(s["encryptionCertificate"], "BASE64DERCERT");
        assert_eq!(s["encryptionCertificateId"], "cert-7");
    }

    #[test]
    fn should_use_webhooks_skips_test_server() {
        assert!(!should_use_webhooks("http://localhost:9103/test/graph"));
        assert!(!should_use_webhooks("https://example.com/test/whatever"));
    }

    #[test]
    fn should_use_webhooks_picks_production_graph() {
        assert!(should_use_webhooks("https://graph.microsoft.com"));
        assert!(should_use_webhooks("https://graph.microsoft.com/v1.0"));
    }

    #[test]
    fn use_webhooks_kv_key_is_per_account_under_teams_namespace() {
        assert_eq!(
            use_webhooks_kv_key("acct-1"),
            "client.config.teams.use_webhooks.acct-1"
        );
        // Two different accounts get distinct keys.
        assert_ne!(
            use_webhooks_kv_key("acct-1"),
            use_webhooks_kv_key("acct-2")
        );
    }

    #[test]
    fn subscription_response_deserializes_minimal_graph_payload() {
        let raw = serde_json::json!({
            "id": "sub-1",
            "expirationDateTime": "2026-05-25T19:00:00Z",
        });
        let r: SubscriptionResponse = serde_json::from_value(raw).unwrap();
        assert_eq!(r.id, "sub-1");
        assert_eq!(r.expiration_date_time, "2026-05-25T19:00:00Z");
        assert!(r.resource.is_empty());
        assert!(r.notification_url.is_empty());
    }
}
