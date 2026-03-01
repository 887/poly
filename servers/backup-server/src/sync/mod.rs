//! Sync endpoints — push and pull encrypted settings blobs.
//!
//! All routes require a valid Bearer token (enforced by the `AuthUser` extractor).
//! The server stores only opaque encrypted blobs — it has no knowledge of the plaintext.
//!
//! ## Sequence numbers
//! Each account has a monotonically increasing per-account sequence counter.
//! Clients record their `last_sequence` locally and pass it to `/api/sync/pull?since=N`
//! to fetch only new blobs.

use axum::{
    Json,
    extract::{Query, State},
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{
    AppState,
    auth::AuthUser,
    error::{AppError, Result},
};

// ── Request / response types ─────────────────────────────────────────────────

/// Request body for `POST /api/sync/push`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct PushRequest {
    /// Base64-encoded encrypted settings blob (opaque to the server).
    pub encrypted_blob: String,
    /// Hints at the client's current sequence number; used for diagnostics only.
    pub sequence_hint: Option<i64>,
}

/// Response from `POST /api/sync/push`.
#[derive(Debug, Serialize, ToSchema)]
pub struct PushResponse {
    /// The sequence number assigned to this blob.
    pub sequence: i64,
}

/// Query parameters for `GET /api/sync/pull`.
#[derive(Debug, Deserialize, ToSchema)]
pub struct PullQuery {
    /// Return only blobs with sequence strictly greater than this value.
    /// Omit (or pass 0) to fetch all blobs.
    pub since: Option<i64>,
}

/// A single sync blob entry returned in a pull response.
#[derive(Debug, Serialize, ToSchema)]
pub struct BlobEntry {
    /// Monotonically increasing per-account sequence number.
    pub sequence: i64,
    /// Base64-encoded encrypted blob.
    pub encrypted_blob: String,
    /// ISO-8601 UTC timestamp when this blob was pushed.
    pub pushed_at: String,
}

/// Response from `GET /api/sync/pull`.
#[derive(Debug, Serialize, ToSchema)]
pub struct PullResponse {
    /// Blobs with sequence > `since`, ordered by sequence ascending.
    pub blobs: Vec<BlobEntry>,
    /// The highest sequence number currently stored for this account.
    pub latest_sequence: i64,
}

/// Response from `GET /api/sync/status`.
#[derive(Debug, Serialize, ToSchema)]
pub struct SyncStatusResponse {
    /// Authenticated account's public key.
    pub public_key: String,
    /// ISO-8601 UTC timestamp of account registration.
    pub registered_at: String,
    /// Highest sequence stored for this account.
    pub latest_sequence: i64,
    /// Total number of blobs stored.
    pub blob_count: i64,
}

// ── Handlers ─────────────────────────────────────────────────────────────────

/// `POST /api/sync/push` — store a new encrypted settings blob.
#[utoipa::path(
    post,
    path = "/api/sync/push",
    request_body = PushRequest,
    responses(
        (status = 200, description = "Blob stored; sequence number returned", body = PushResponse),
        (status = 400, description = "Empty blob"),
        (status = 401, description = "Invalid or expired token"),
    ),
    security(("BearerAuth" = [])),
    tag = "sync"
)]
pub async fn push(
    State(state): State<AppState>,
    user: AuthUser,
    Json(body): Json<PushRequest>,
) -> Result<Json<PushResponse>> {
    if body.encrypted_blob.is_empty() {
        return Err(AppError::BadRequest(
            "encrypted_blob must not be empty".into(),
        ));
    }

    tracing::debug!(
        "Push from pk={}… hint={:?}",
        &user.public_key[..8],
        body.sequence_hint
    );

    // Compute next sequence = MAX(current) + 1 for this account.
    let max_seq: Option<serde_json::Value> = state
        .db
        .query(
            "SELECT math::max(sequence) AS max_seq FROM sync_blob \
             WHERE public_key = $pk GROUP ALL",
        )
        .bind(("pk", user.public_key.clone()))
        .await?
        .take(0)
        .map_err(AppError::from)?;

    let next_seq = max_seq
        .as_ref()
        .and_then(|v| v.get("max_seq"))
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0)
        + 1;

    let now_str = chrono::Utc::now().to_rfc3339();
    state
        .db
        .query(
            "CREATE sync_blob CONTENT { \
               public_key: $pk, sequence: $seq, \
               encrypted_blob: $blob, pushed_at: $now \
             }",
        )
        .bind(("pk", user.public_key))
        .bind(("seq", next_seq))
        .bind(("blob", body.encrypted_blob))
        .bind(("now", now_str))
        .await?
        .check()
        .map_err(AppError::from)?;

    Ok(Json(PushResponse { sequence: next_seq }))
}

/// `GET /api/sync/pull?since=N` — fetch encrypted blobs since a sequence number.
#[utoipa::path(
    get,
    path = "/api/sync/pull",
    params(
        ("since" = Option<i64>, Query, description = "Return blobs with sequence > this value"),
    ),
    responses(
        (status = 200, description = "Blob delta returned", body = PullResponse),
        (status = 401, description = "Invalid or expired token"),
    ),
    security(("BearerAuth" = [])),
    tag = "sync"
)]
pub async fn pull(
    State(state): State<AppState>,
    user: AuthUser,
    Query(params): Query<PullQuery>,
) -> Result<Json<PullResponse>> {
    let since = params.since.unwrap_or(0);

    tracing::debug!("Pull from pk={}… since={}", &user.public_key[..8], since);

    let records: Vec<serde_json::Value> = state
        .db
        .query(
            "SELECT sequence, encrypted_blob, pushed_at FROM sync_blob \
             WHERE public_key = $pk AND sequence > $since \
             ORDER BY sequence ASC",
        )
        .bind(("pk", user.public_key.clone()))
        .bind(("since", since))
        .await?
        .take(0)
        .map_err(AppError::from)?;

    let blobs = records
        .into_iter()
        .map(|r| {
            let sequence = r
                .get("sequence")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0);
            let encrypted_blob = r
                .get("encrypted_blob")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let pushed_at = r
                .get("pushed_at")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned();
            BlobEntry {
                sequence,
                encrypted_blob,
                pushed_at,
            }
        })
        .collect::<Vec<_>>();

    let latest_sequence = blobs.last().map(|b| b.sequence).unwrap_or(since);

    Ok(Json(PullResponse {
        blobs,
        latest_sequence,
    }))
}

/// `GET /api/sync/status` — return account info and token metadata for the authenticated client.
#[utoipa::path(
    get,
    path = "/api/sync/status",
    responses(
        (status = 200, description = "Account status", body = SyncStatusResponse),
        (status = 401, description = "Invalid or expired token"),
    ),
    security(("BearerAuth" = [])),
    tag = "sync"
)]
pub async fn status(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<SyncStatusResponse>> {
    let account: Option<serde_json::Value> = state
        .db
        .query("SELECT public_key, registered_at FROM account WHERE public_key = $pk LIMIT 1")
        .bind(("pk", user.public_key.clone()))
        .await?
        .take(0)
        .map_err(AppError::from)?;

    let account = account.ok_or(AppError::NotFound)?;

    let stats: Option<serde_json::Value> = state
        .db
        .query(
            "SELECT count() AS blob_count, math::max(sequence) AS latest_seq \
             FROM sync_blob WHERE public_key = $pk GROUP ALL",
        )
        .bind(("pk", user.public_key.clone()))
        .await?
        .take(0)
        .map_err(AppError::from)?;

    let blob_count = stats
        .as_ref()
        .and_then(|v| v.get("blob_count"))
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0);
    let latest_sequence = stats
        .as_ref()
        .and_then(|v| v.get("latest_seq"))
        .and_then(serde_json::Value::as_i64)
        .unwrap_or(0);

    Ok(Json(SyncStatusResponse {
        public_key: user.public_key,
        registered_at: account
            .get("registered_at")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_owned(),
        latest_sequence,
        blob_count,
    }))
}
