//! Stable wire types for Teams calling — Phase A of
//! `docs/plans/plan-teams-calling.md`.

use std::fmt;

use poly_client::ClientError;

/// Identifier for an in-flight Teams / ACS call.
///
/// Wraps the ACS `callId` string returned by `CallAgent.startCall(…)` in
/// the JS SDK. Opaque to Rust callers — never parse it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CallId(pub String);

impl fmt::Display for CallId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for CallId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for CallId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Lifecycle state of an ACS call.
///
/// Mirrors the ACS JS SDK's `CallState` enum (Connecting, Ringing,
/// Connected, …) — see
/// <https://learn.microsoft.com/javascript/api/azure-communication-services/@azure/communication-calling/call?view=azure-communication-services-js#@azure-communication-calling-call-state>.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallState {
    /// Call object created but no signaling sent yet.
    None,
    /// Outbound: dialing. Inbound: incoming, awaiting accept.
    Connecting,
    /// Outbound only — remote endpoint is ringing.
    Ringing,
    /// Bidirectional media flowing.
    Connected,
    /// Local user placed call on hold or remote put us on hold.
    LocalHold,
    RemoteHold,
    /// Call is being torn down.
    Disconnecting,
    /// Call is fully torn down.
    Disconnected,
    /// Inbound call: early-media phase before accept.
    EarlyMedia,
    /// Inbound call: ringing locally, awaiting user action.
    IncomingCall,
}

/// An ACS access token bundle.
///
/// Returned by `POST {acsEndpoint}/identities/{acsUserId}/access-tokens`
/// with scopes `["voip"]`. Lifetime is normally 24h; refresh at 22h with
/// jitter (Phase B.2).
#[derive(Debug, Clone)]
pub struct AcsAccessToken {
    /// JWT bearer suitable for `CallClient.createCallAgent(credential)`.
    pub token: String,
    /// Absolute expiry in RFC3339 / ISO-8601 UTC. None if the server
    /// didn't supply one (treat as "expires soon").
    pub expires_on: Option<String>,
}

/// Mapping between an AAD user identity (the user's Teams principal) and
/// the ACS Communication Services User identity used for calling.
///
/// Persisted in `teams.config.<account>.acs_identity` KV (Phase B.3). One
/// ACS identity is created per AAD user on first calling-bootstrap.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AcsIdentity {
    /// AAD user object id (the principal that owns the Teams account).
    pub aad_user_id: String,
    /// ACS Communication Services user identity, e.g.
    /// `8:acs:11111111-2222-3333-4444-555555555555_22222222-…`.
    pub acs_user_id: String,
    /// ACS endpoint base URL, e.g. `https://contoso.communication.azure.com`.
    pub acs_endpoint: String,
}

/// Errors surfaced from the calling subsystem.
///
/// Designed to bridge cleanly to [`ClientError`] via `From<CallingError>`
/// so the existing voice-stub call-sites don't need new error arms.
#[derive(Debug, thiserror::Error)]
pub enum CallingError {
    /// The platform / shell does not yet support calling. Shipped on
    /// every code path until the JS bridge lands (Phase C).
    #[error("calling not supported: {0}")]
    NotSupported(String),

    /// The bridge is wired but the JS-side `@azure/communication-calling`
    /// integration has not been written yet. Distinct from
    /// [`Self::NotSupported`] — `NotSupported` means "this platform will
    /// never have calling"; `NotImplemented` means "this code path is
    /// scaffolded and will work once the JS bridge ships in Phase C of
    /// `docs/plans/plan-teams-calling.md`."
    #[error("calling not implemented: {0}")]
    NotImplemented(String),

    /// Tenant has no ACS resource provisioned, or the user has no ACS
    /// identity mapping yet. Phase B.3 setup-screen prompt.
    #[error("ACS identity not provisioned: {0}")]
    AcsNotProvisioned(String),

    /// Token acquisition failed (network, 4xx from ACS Identity REST).
    #[error("ACS token acquisition failed: {0}")]
    TokenAcquisition(String),

    /// Network error talking to ACS or the JS bridge.
    #[error("network error: {0}")]
    Network(String),

    /// Tenant policy blocked the call (admin disabled external federated
    /// calls / guest tenant calling / anonymous join / lobby).
    #[error("tenant policy: {0}")]
    PolicyDenied(String),

    /// Catch-all for ACS-side failures with no better mapping.
    #[error("ACS error: {0}")]
    Internal(String),
}

impl From<CallingError> for ClientError {
    fn from(e: CallingError) -> Self {
        match e {
            CallingError::NotSupported(msg) | CallingError::NotImplemented(msg) => Self::NotSupported(msg),
            CallingError::AcsNotProvisioned(msg) | CallingError::TokenAcquisition(msg) => Self::AuthFailed(msg),
            CallingError::Network(msg) => Self::Network(msg),
            CallingError::PolicyDenied(msg) => Self::PermissionDenied(msg),
            CallingError::Internal(msg) => Self::Internal(msg),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    #[test]
    fn call_id_display_round_trips() {
        let id: CallId = "abc-123".into();
        assert_eq!(id.to_string(), "abc-123");
        assert_eq!(CallId::from("abc-123".to_string()).to_string(), "abc-123");
    }

    #[test]
    fn calling_error_maps_to_client_error_not_supported() {
        let e = CallingError::NotSupported("nope".into());
        let ce: ClientError = e.into();
        assert!(matches!(ce, ClientError::NotSupported(_)));
    }

    #[test]
    fn calling_error_not_implemented_maps_to_not_supported() {
        let e = CallingError::NotImplemented("phase C".into());
        let ce: ClientError = e.into();
        assert!(matches!(ce, ClientError::NotSupported(_)));
    }

    #[test]
    fn calling_error_maps_to_client_error_network() {
        let e = CallingError::Network("dns".into());
        let ce: ClientError = e.into();
        assert!(matches!(ce, ClientError::Network(_)));
    }

    #[test]
    fn calling_error_maps_to_client_error_auth_failed() {
        for e in [
            CallingError::AcsNotProvisioned("x".into()),
            CallingError::TokenAcquisition("y".into()),
        ] {
            let ce: ClientError = e.into();
            assert!(matches!(ce, ClientError::AuthFailed(_)));
        }
    }

    #[test]
    fn calling_error_policy_denied_maps_to_permission_denied() {
        let e = CallingError::PolicyDenied("no federated calls".into());
        let ce: ClientError = e.into();
        assert!(matches!(ce, ClientError::PermissionDenied(_)));
    }

    #[test]
    fn acs_identity_serializes_round_trip() {
        let id = AcsIdentity {
            aad_user_id: "aad-1".into(),
            acs_user_id: "8:acs:abc".into(),
            acs_endpoint: "https://contoso.communication.azure.com".into(),
        };
        let s = serde_json::to_string(&id).unwrap();
        let back: AcsIdentity = serde_json::from_str(&s).unwrap();
        assert_eq!(back.aad_user_id, "aad-1");
        assert_eq!(back.acs_user_id, "8:acs:abc");
        assert_eq!(back.acs_endpoint, "https://contoso.communication.azure.com");
    }

    #[test]
    fn call_state_variants_are_distinct() {
        // Compile-time check that all variants survive equality.
        assert_ne!(CallState::Connecting, CallState::Connected);
        assert_ne!(CallState::IncomingCall, CallState::Ringing);
    }
}
