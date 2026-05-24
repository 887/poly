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
    pub fn max_lifetime(self) -> Duration {
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
            max - margin
        } else {
            // Pathological — fall back to half the lifetime.
            Duration::from_secs(max.as_secs() / 2)
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
    let expiry = now + chrono::Duration::seconds(secs);
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
        };
        let s = serde_json::to_value(&req).unwrap();
        assert_eq!(s["changeType"], "created,updated");
        assert_eq!(s["notificationUrl"], "https://relay/webhook");
        assert_eq!(s["resource"], "/teams/T/channels/C/messages");
        assert_eq!(s["expirationDateTime"], "2026-05-25T18:30:00Z");
        assert_eq!(s["clientState"], "secret-abc");
        assert_eq!(s["latestSupportedTlsVersion"], "v1_2");
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
