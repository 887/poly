//! Microsoft Graph change-notification webhook relay — Phase C of
//! `docs/plans/plan-teams-graph-subscriptions.md`.
//!
//! ## Endpoint contract
//!
//! Graph hits this endpoint two ways:
//!
//! 1. **Validation handshake** — synchronous `GET /<path>?validationToken=…`
//!    on subscription creation. Must respond `200 OK` with the **plain**
//!    `validationToken` value as the body (`Content-Type: text/plain`)
//!    within 10 seconds, or the subscription is rejected. See
//!    <https://learn.microsoft.com/graph/webhooks#notificationurl-validation>.
//!
//! 2. **Change notification** — `POST /<path>` with a JSON body shaped
//!    like:
//!
//!    ```json
//!    {
//!      "value": [
//!        {
//!          "subscriptionId": "abc",
//!          "clientState": "secret-from-create",
//!          "changeType": "created",
//!          "resource": "/teams/T/channels/C/messages/M",
//!          "tenantId": "…",
//!          "subscriptionExpirationDateTime": "2026-05-25T19:00:00Z",
//!          "resourceData": { … },        // optional rich payload
//!          "encryptedContent": { … }     // optional encrypted payload (Phase D)
//!        }
//!      ]
//!    }
//!    ```
//!
//! ## Security
//!
//! Every incoming `value[*]` is verified against the stored
//! `clientState` for `subscriptionId`. Mismatch → 202 Accepted (we
//! still ack so Graph stops retrying) but the notification is dropped.
//! Storage of per-subscription state is the caller's responsibility —
//! pass a `&dyn ClientStateStore`.
//!
//! ## Self-hosted only
//!
//! `notificationUrl` MUST be publicly addressable HTTPS. Local-dev
//! users do not have this; the long-poll path (`/test/events/poll`)
//! stays live for development (Phase E.1 fallback gate).

#![cfg(all(not(target_arch = "wasm32"), feature = "teams-webhook"))]

use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use serde::{Deserialize, Serialize};

/// Route prefix for the Teams notification relay.
pub const TEAMS_NOTIFICATIONS_PATH: &str = "/host/teams/notifications";

/// Sub-route for the per-account webhook endpoint.
///
/// Microsoft requires one `notificationUrl` per subscription; we split
/// by `{account_id}` so multiple accounts can share the same poly-host
/// without colliding `clientState` secrets.
pub const TEAMS_NOTIFICATIONS_ACCOUNT_PATH: &str = "/host/teams/notifications/{account_id}";

/// Wire envelope for a `POST` change-notification payload.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChangeNotificationEnvelope {
    /// One or more notifications batched into a single POST.
    pub value: Vec<ChangeNotification>,
    /// Optional validation tokens (when subscriptions request `latestSupportedTlsVersion`).
    #[serde(default, rename = "validationTokens", skip_serializing_if = "Vec::is_empty")]
    pub validation_tokens: Vec<String>,
}

/// One notification inside the [`ChangeNotificationEnvelope::value`] array.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChangeNotification {
    /// Subscription that this notification belongs to.
    #[serde(rename = "subscriptionId")]
    pub subscription_id: String,
    /// Echoed-back client state from `POST /subscriptions`. Used for
    /// HMAC verification (today: equality check against the stored
    /// secret; future: full HMAC-SHA256 if `latestSupportedTlsVersion`
    /// signed the secret).
    #[serde(rename = "clientState")]
    pub client_state: String,
    /// `"created"`, `"updated"`, `"deleted"`.
    #[serde(rename = "changeType")]
    pub change_type: String,
    /// Resource path that changed, e.g. `/teams/T/channels/C/messages/M`.
    pub resource: String,
    /// Tenant id (Entra directory).
    #[serde(default, rename = "tenantId")]
    pub tenant_id: String,
    /// Absolute expiry of the originating subscription.
    #[serde(default, rename = "subscriptionExpirationDateTime")]
    pub subscription_expiration_date_time: String,
    /// Optional rich resource data (only present if subscription
    /// requested it + Phase D encryption is in place).
    #[serde(default, rename = "resourceData")]
    pub resource_data: Option<serde_json::Value>,
    /// Optional encrypted payload — Phase D, currently passed through
    /// verbatim.
    #[serde(default, rename = "encryptedContent")]
    pub encrypted_content: Option<serde_json::Value>,
}

/// Validation-handshake query string.
#[derive(Debug, Clone, Deserialize)]
pub struct ValidationQuery {
    #[serde(rename = "validationToken")]
    pub validation_token: String,
}

/// Pluggable store mapping `subscription_id → expected client_state`.
///
/// Implementations live with the caller (poly-host daemon, the apps/web
/// fullstack server, …) — webhook code stays free of SQLite / storage
/// concerns.
pub trait ClientStateStore: Send + Sync {
    /// Return the stored `clientState` for `subscription_id`, or `None`
    /// if unknown (subscription was deleted / never created here).
    fn get(&self, subscription_id: &str) -> Option<String>;
}

/// Pluggable sink for verified notifications.
///
/// Caller hooks this to whatever event bus the WASM UI subscribes to —
/// the long-poll path already has a per-account channel; the webhook
/// relay just fans into the same channel.
pub trait NotificationSink: Send + Sync {
    /// Forward a verified notification. Implementations should not
    /// block — return immediately and queue work asynchronously.
    fn dispatch(&self, account_id: &str, notification: ChangeNotification);
}

/// Bundle of dependencies the webhook routes need.
#[derive(Clone)]
pub struct TeamsWebhookState {
    pub store: Arc<dyn ClientStateStore>,
    pub sink: Arc<dyn NotificationSink>,
}

impl TeamsWebhookState {
    /// Construct a new state bundle.
    #[must_use]
    pub fn new(store: Arc<dyn ClientStateStore>, sink: Arc<dyn NotificationSink>) -> Self {
        Self { store, sink }
    }
}

/// Construct the axum sub-router that mounts the validation + dispatch
/// handlers under [`TEAMS_NOTIFICATIONS_PATH`].
///
/// Mount via `Router::merge(teams_webhook::router(state))` from the
/// host process (see `apps/poly-host/src/lib.rs`).
#[must_use]
pub fn router(state: TeamsWebhookState) -> Router {
    Router::new()
        // Per-account routes — Graph hits `<base>/host/teams/notifications/<account_id>`.
        .route(
            TEAMS_NOTIFICATIONS_ACCOUNT_PATH,
            get(handle_validation).post(handle_notification),
        )
        // Generic catch-all without account_id (e.g. tests, single-account hosts).
        .route(
            TEAMS_NOTIFICATIONS_PATH,
            get(handle_validation_generic).post(handle_notification_generic),
        )
        .with_state(state)
}

// ── handlers ─────────────────────────────────────────────────────────────────

async fn handle_validation(
    Query(q): Query<ValidationQuery>,
) -> impl IntoResponse {
    validation_response(q.validation_token)
}

async fn handle_validation_generic(
    Query(q): Query<ValidationQuery>,
) -> impl IntoResponse {
    validation_response(q.validation_token)
}

/// Build the `200 OK text/plain` response Graph expects on the
/// validation handshake.
fn validation_response(token: String) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    (StatusCode::OK, headers, token)
}

async fn handle_notification(
    State(state): State<TeamsWebhookState>,
    axum::extract::Path(account_id): axum::extract::Path<String>,
    Json(envelope): Json<ChangeNotificationEnvelope>,
) -> StatusCode {
    dispatch_envelope(&state, &account_id, envelope);
    StatusCode::ACCEPTED
}

async fn handle_notification_generic(
    State(state): State<TeamsWebhookState>,
    Json(envelope): Json<ChangeNotificationEnvelope>,
) -> StatusCode {
    dispatch_envelope(&state, "", envelope);
    StatusCode::ACCEPTED
}

/// Verify + dispatch every notification in the envelope.
///
/// Always returns; failures (unknown subscription, mismatched
/// `clientState`) drop silently after a `tracing::warn!` so Graph isn't
/// served a 4xx that would trigger its lifecycle-event retry.
pub fn dispatch_envelope(
    state: &TeamsWebhookState,
    account_id: &str,
    envelope: ChangeNotificationEnvelope,
) {
    for notification in envelope.value {
        match state.store.get(&notification.subscription_id) {
            Some(expected) if constant_time_eq(&expected, &notification.client_state) => {
                state.sink.dispatch(account_id, notification);
            }
            Some(_) => {
                tracing::warn!(
                    subscription_id = %notification.subscription_id,
                    "teams webhook: clientState mismatch — dropping notification"
                );
            }
            None => {
                tracing::warn!(
                    subscription_id = %notification.subscription_id,
                    "teams webhook: unknown subscription_id — dropping notification"
                );
            }
        }
    }
}

/// Length-checked constant-time comparison — avoids leaking timing
/// signal about how many leading bytes of `clientState` matched.
fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.bytes().zip(b.bytes()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use std::sync::Mutex;

    struct MapStore(std::collections::HashMap<String, String>);
    impl ClientStateStore for MapStore {
        fn get(&self, sub_id: &str) -> Option<String> {
            self.0.get(sub_id).cloned()
        }
    }

    #[derive(Default)]
    struct CountSink {
        calls: Mutex<Vec<(String, String)>>,
    }
    impl NotificationSink for CountSink {
        fn dispatch(&self, account_id: &str, n: ChangeNotification) {
            self.calls
                .lock()
                .unwrap()
                .push((account_id.into(), n.subscription_id));
        }
    }

    fn make_state(
        sub_id: &str,
        client_state: &str,
    ) -> (TeamsWebhookState, Arc<CountSink>) {
        let mut m = std::collections::HashMap::new();
        m.insert(sub_id.into(), client_state.into());
        let store = Arc::new(MapStore(m));
        let sink = Arc::new(CountSink::default());
        let state = TeamsWebhookState::new(store, sink.clone());
        (state, sink)
    }

    fn notif(sub_id: &str, client_state: &str) -> ChangeNotification {
        ChangeNotification {
            subscription_id: sub_id.into(),
            client_state: client_state.into(),
            change_type: "created".into(),
            resource: "/teams/T/channels/C/messages/M".into(),
            tenant_id: "tenant".into(),
            subscription_expiration_date_time: "2026-05-25T19:00:00Z".into(),
            resource_data: None,
            encrypted_content: None,
        }
    }

    #[test]
    fn dispatch_valid_notification_reaches_sink() {
        let (state, sink) = make_state("sub-1", "secret");
        dispatch_envelope(
            &state,
            "acct-1",
            ChangeNotificationEnvelope {
                value: vec![notif("sub-1", "secret")],
                validation_tokens: vec![],
            },
        );
        let calls = sink.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "acct-1");
        assert_eq!(calls[0].1, "sub-1");
    }

    #[test]
    fn dispatch_mismatched_client_state_is_dropped() {
        let (state, sink) = make_state("sub-1", "secret");
        dispatch_envelope(
            &state,
            "acct-1",
            ChangeNotificationEnvelope {
                value: vec![notif("sub-1", "WRONG")],
                validation_tokens: vec![],
            },
        );
        assert!(sink.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn dispatch_unknown_subscription_is_dropped() {
        let (state, sink) = make_state("sub-1", "secret");
        dispatch_envelope(
            &state,
            "acct-1",
            ChangeNotificationEnvelope {
                value: vec![notif("sub-OTHER", "secret")],
                validation_tokens: vec![],
            },
        );
        assert!(sink.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn constant_time_eq_handles_unequal_lengths() {
        assert!(!constant_time_eq("a", "ab"));
        assert!(!constant_time_eq("ab", "a"));
        assert!(constant_time_eq("abc", "abc"));
        assert!(!constant_time_eq("abc", "abd"));
        assert!(constant_time_eq("", ""));
    }

    #[test]
    fn envelope_round_trip_preserves_optional_fields() {
        let raw = serde_json::json!({
            "value": [{
                "subscriptionId": "s1",
                "clientState": "cs",
                "changeType": "updated",
                "resource": "/x",
                "tenantId": "t",
                "subscriptionExpirationDateTime": "2026-05-25T19:00:00Z",
                "resourceData": {"@odata.type": "#microsoft.graph.chatMessage"}
            }]
        });
        let env: ChangeNotificationEnvelope = serde_json::from_value(raw).unwrap();
        assert_eq!(env.value.len(), 1);
        assert_eq!(env.value[0].change_type, "updated");
        assert!(env.value[0].resource_data.is_some());
        assert!(env.value[0].encrypted_content.is_none());
    }
}
