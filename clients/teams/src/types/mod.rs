//! Microsoft Graph API types used by the Teams client.
//!
//! We model a subset of the Graph v1.0 schema — just the endpoints this
//! client actually calls. Field names follow Graph's `camelCase` convention
//! via `#[serde(rename_all = "camelCase")]`; where Rust naming diverges,
//! individual fields use `#[serde(rename = "…")]`.
//!
//! Every Graph list endpoint wraps its payload in an OData envelope
//! (`{ "value": [...], "@odata.nextLink": "…?skiptoken=…" }`); see
//! [`ODataResponse`]. Error payloads (4xx/5xx from Graph) deserialize to
//! [`GraphError`], which the HTTP layer maps to [`poly_client::ClientError`].

mod chat;
mod channel;
mod message;
mod team;
mod user;

pub use chat::{GraphChat, GraphChatMember};
pub use channel::GraphChannel;
pub use message::{GraphMessage, GraphMessageBody, GraphMessageFrom, GraphFromUser};
pub use team::GraphTeam;
pub use user::GraphUser;

/// A member entry returned by `GET /v1.0/teams/{id}/members`.
///
/// The `id` field here is the **membership ID** (base64-encoded composite),
/// not the user's OID. Use this `id` for `DELETE /teams/{t}/members/{id}`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphMember {
    /// Membership ID — use as the path segment for DELETE /members/{id}.
    pub id: String,
    /// AAD user object ID (OID).
    pub user_id: Option<String>,
    pub display_name: Option<String>,
    /// `["owner"]` for owners, empty for regular members.
    #[serde(default)]
    pub roles: Vec<String>,
}

use poly_client::ClientError;
use serde::Deserialize;

/// OData list envelope — every `GET /v1.0/.../list` endpoint returns this.
#[derive(Debug, Clone, Deserialize)]
pub struct ODataResponse<T> {
    pub value: Vec<T>,
    #[serde(rename = "@odata.nextLink")]
    pub next_link: Option<String>,
}

/// Legacy alias — older call sites used `GraphCollection`. New code should
/// prefer [`ODataResponse`].
pub type GraphCollection<T> = ODataResponse<T>;

/// Graph API error payload shape: `{ "error": { "code": "...", "message": "..." } }`.
#[derive(Debug, Clone, Deserialize)]
pub struct GraphError {
    pub error: GraphErrorBody,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GraphErrorBody {
    pub code: String,
    pub message: String,
}

impl GraphError {
    /// Map a Graph error + HTTP status to a `ClientError`.
    ///
    /// - 401 → `AuthFailed` (reauth-worthy)
    /// - 404 → `NotFound`
    /// - 429 / 5xx → `Network` (transient)
    /// - other 4xx → `Internal`
    pub fn into_client_error(self, status: u16) -> ClientError {
        let msg = format!("Graph {status} {}: {}", self.error.code, self.error.message);
        match status {
            401 | 403 => ClientError::AuthFailed(msg),
            404 => ClientError::NotFound(msg),
            429 | 500..=599 => ClientError::Network(msg),
            _ => ClientError::Internal(msg),
        }
    }
}
