//! File upload / attachment serving.
//!
//! Flow:
//!  1. Client uploads file via `POST /attachments` (multipart/form-data).
//!  2. Server stores the file on disk under `Config::uploads_dir`, records metadata
//!     in the `attachment` table, and returns the attachment ID.
//!  3. Client includes attachment IDs in `POST /channels/:id/messages`.
//!  4. Client fetches files via `GET /attachments/:id` — the server enforces that
//!     the requesting user has read-access to the channel the attachment belongs to.

use axum::{
    Router,
    body::Body,
    extract::{Extension, Multipart, Path, State},
    http::{StatusCode, header},
    response::Response,
    routing::{get, post},
};
use serde::Serialize;
use tokio::io::AsyncWriteExt as _;
use uuid::Uuid;

use crate::{
    AppState,
    auth::AuthUser,
    error::{AppError, Result},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/attachments", post(upload))
        .route("/attachments/{id}", get(serve))
}

/// Max upload size: 50 MiB.
const MAX_UPLOAD_BYTES: usize = 50 * 1024 * 1024;

#[derive(Debug, Serialize)]
struct UploadResponse {
    id: String,
    filename: String,
    mime_type: String,
    size_bytes: u64,
}

// ── Upload handler ────────────────────────────────────────────────────────────

async fn upload(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    mut multipart: Multipart,
) -> Result<(StatusCode, axum::Json<UploadResponse>)> {
    let uploads_dir = &state.config.uploads_dir;
    tokio::fs::create_dir_all(uploads_dir)
        .await
        .map_err(|e| AppError::Internal(format!("cannot create uploads dir: {e}")))?;

    let field = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("multipart error: {e}")))?
        .ok_or_else(|| AppError::BadRequest("no field in upload".into()))?;

    let filename = field
        .file_name()
        .map_or_else(|| "upload".to_owned(), str::to_owned);
    let content_type = field
        .content_type()
        .map_or_else(|| "application/octet-stream".to_owned(), str::to_owned);

    let data = field
        .bytes()
        .await
        .map_err(|e| AppError::BadRequest(format!("read error: {e}")))?;

    if data.len() > MAX_UPLOAD_BYTES {
        return Err(AppError::BadRequest(format!(
            "file too large ({} MiB max)",
            MAX_UPLOAD_BYTES.wrapping_div(1024).wrapping_div(1024)
        )));
    }
    let size_bytes = u64::try_from(data.len()).unwrap_or(u64::MAX);

    // Sanitise filename — keep extension, replace everything else.
    let ext = std::path::Path::new(&filename)
        .extension()
        .and_then(|s| s.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    let storage_name = format!("{}{}", Uuid::new_v4(), ext);
    let disk_path = std::path::Path::new(uploads_dir).join(&storage_name);

    let mut f = tokio::fs::File::create(&disk_path)
        .await
        .map_err(|e| AppError::Internal(format!("create file: {e}")))?;
    f.write_all(&data)
        .await
        .map_err(|e| AppError::Internal(format!("write file: {e}")))?;

    // Record in DB — `message` is NONE until the message is sent.
    let att = state
        .db
        .create_attachment(&auth.user_id, &filename, &storage_name, &content_type, size_bytes)
        .await?
        .ok_or_else(|| AppError::Internal("no record".into()))?;
    let id = att.id.unwrap_or_default();

    Ok((
        StatusCode::CREATED,
        axum::Json(UploadResponse {
            id,
            filename,
            mime_type: content_type,
            size_bytes,
        }),
    ))
}

// ── Serve handler ─────────────────────────────────────────────────────────────

async fn serve(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(att_id): Path<String>,
) -> Result<Response<Body>> {
    let att = state
        .db
        .get_attachment(&att_id)
        .await?
        .ok_or(AppError::NotFound)?;

    // Access control: either the uploader, or a user who can read the linked channel.
    let can_access = att.uploaded_by == auth.user_id
        || match &att.message {
            Some(msg_id) => {
                if let Some(ch_id) = state.db.get_message_channel_id(msg_id).await? {
                    can_read_channel(&state, &ch_id, &auth.user_id).await?
                } else {
                    false
                }
            }
            None => {
                // Orphan attachment (not yet linked to a message) — only uploader.
                false
            }
        };

    if !can_access {
        return Err(AppError::Forbidden);
    }

    let disk_path = std::path::Path::new(&state.config.uploads_dir).join(&att.storage_name);
    let file_bytes = tokio::fs::read(&disk_path)
        .await
        .map_err(|e| AppError::Internal(format!("read file: {e}")))?;

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, att.mime_type)
        .header(
            header::CONTENT_DISPOSITION,
            format!("inline; filename=\"{}\"", att.filename),
        )
        .body(Body::from(file_bytes))
        .map_err(|e| AppError::Internal(format!("build response: {e}")))?;

    Ok(response)
}

// ── Helpers ────────────────────────────────────────────────────────────────────

async fn can_read_channel(state: &AppState, channel_id: &str, user_id: &str) -> Result<bool> {
    if state.db.is_participant(user_id, channel_id).await? {
        return Ok(true);
    }
    if let Some(server_id) = state.db.get_channel_server_id(channel_id).await?
        && state.db.get_membership(user_id, &server_id).await?.is_some()
    {
        return Ok(true);
    }
    Ok(false)
}
