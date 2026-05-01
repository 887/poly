//! `GET /api/info` — public server information endpoint.
//!
//! Returns publicly visible metadata about this backup server instance.
//! No authentication is required — clients call this during the setup wizard
//! before they have a session token.

use axum::{Json, extract::State};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{AppState, error::Result};

/// Public server metadata returned by `GET /api/info`.
#[derive(Debug, Serialize, ToSchema)]
pub struct ServerInfoResponse {
    /// Human-readable server name (set via `POLY_SERVER_NAME` env var).
    pub name: String,
    /// Whether a passphrase is required for client authentication.
    ///
    /// If `false`, clients should send an empty `passphrase` field and the
    /// password step in the setup wizard can be skipped.
    pub password_required: bool,
    /// Whether new user registrations are currently accepted.
    ///
    /// `false` means the server has reached its configured `POLY_MAX_ACCOUNTS`
    /// limit and cannot accept new users.
    pub registrations_open: bool,
    /// Server software version (`CARGO_PKG_VERSION`).
    pub version: &'static str,
}

/// `GET /api/info` — public server metadata.
///
/// Returns the server name, whether a passphrase is required for clients to
/// register and authenticate, whether new registrations are accepted, and the
/// software version. This endpoint requires **no** authentication — clients
/// use it during the setup wizard before they have a session token.
///
/// Clients should call this endpoint to:
/// - Display a human-readable server name before the user decides to connect.
/// - Determine whether to show the password input in the setup wizard.
/// - Show a clear error if the server is at capacity and cannot accept new users.
#[utoipa::path(
    get,
    path = "/api/info",
    responses(
        (status = 200, description = "Server info", body = ServerInfoResponse),
    ),
    tag = "info"
)]
pub async fn server_info(State(state): State<AppState>) -> Result<Json<ServerInfoResponse>> {
    // Determine whether registrations are open.
    let registrations_open = if state.config.max_accounts == 0 {
        // Unlimited accounts — always open.
        true
    } else {
        let count_result: Option<serde_json::Value> = state
            .db
            .query("SELECT count() AS cnt FROM account GROUP ALL")
            .await?
            .take(0)
            .map_err(crate::error::AppError::from)?;

        let current = count_result
            .as_ref()
            .and_then(|v| v.get("cnt"))
            .and_then(serde_json::Value::as_i64)
            .and_then(|v| usize::try_from(v).ok())
            .unwrap_or(0);

        current < state.config.max_accounts
    };

    Ok(Json(ServerInfoResponse {
        name: state.config.server_name.clone(),
        password_required: !state.config.passphrase.is_empty(),
        registrations_open,
        version: env!("CARGO_PKG_VERSION"),
    }))
}
