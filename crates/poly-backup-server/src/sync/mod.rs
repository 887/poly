//! Sync endpoints — push/pull encrypted settings blobs.

use axum::{
    Json,
    extract::{Query, State},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::AppState;

/// Push request body.
#[derive(Debug, Deserialize)]
pub struct PushRequest {
    pub data: Vec<u8>,
}

/// Push response.
#[derive(Debug, Serialize)]
pub struct PushResponse {
    pub sequence: u64,
}

/// Pull query parameters.
#[derive(Debug, Deserialize)]
pub struct PullQuery {
    pub since: Option<u64>,
}

/// A sync blob entry.
#[derive(Debug, Serialize)]
pub struct SyncEntry {
    pub sequence: u64,
    pub data: Vec<u8>,
    pub timestamp: String,
}

/// Push encrypted settings to the server.
pub async fn push(
    State(_state): State<Arc<RwLock<AppState>>>,
    Json(body): Json<PushRequest>,
) -> Json<PushResponse> {
    // TODO(phase-2.8.5): Store encrypted blob in SurrealDB
    tracing::info!("Received push: {} bytes", body.data.len());

    Json(PushResponse {
        sequence: 1, // TODO: Monotonic sequence
    })
}

/// Pull settings changes since a sequence number.
pub async fn pull(
    State(_state): State<Arc<RwLock<AppState>>>,
    Query(params): Query<PullQuery>,
) -> Json<Vec<SyncEntry>> {
    let since = params.since.unwrap_or(0);
    tracing::info!("Pull request since sequence: {since}");

    // TODO(phase-2.8.6): Query SurrealDB for entries after sequence
    Json(vec![])
}
