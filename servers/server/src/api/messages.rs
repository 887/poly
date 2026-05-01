use axum::{
    Json, Router,
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    routing::{get, patch, post},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::{
    AppState,
    auth::AuthUser,
    error::{AppError, Result},
    models::{Attachment, Message, Reaction},
    ws::events::{MessagePayload, ServerEvent},
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/channels/{id}/messages",
            get(list_messages).post(create_message),
        )
        .route("/messages/{id}", patch(edit_message).delete(delete_message))
        .route(
            "/messages/{id}/reactions/{emoji}",
            post(add_reaction).delete(remove_reaction),
        )
        .route("/messages/{id}/reactions", get(list_reactions))
}

// ── Request/response types ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ListMessagesQuery {
    /// Cursor — return messages *before* this message ID (exclusive).
    before: Option<String>,
    limit: Option<u8>,
}

#[derive(Debug, Deserialize)]
struct CreateMessageRequest {
    content: String,
    reply_to: Option<String>,
    /// Attachment IDs previously uploaded via `POST /attachments`.
    attachments: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct EditMessageRequest {
    content: String,
}

/// Wire representation of a message (hides deleted content, embeds author).
#[derive(Debug, Serialize)]
pub struct MessageResponse {
    pub id: String,
    pub channel_id: String,
    pub author_id: String,
    pub content: String,
    pub reply_to_id: Option<String>,
    pub edited_at: Option<chrono::DateTime<Utc>>,
    pub deleted: bool,
    pub attachments: Vec<AttachmentRef>,
    pub created_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Serialize, Default)]
pub struct AttachmentRef {
    pub id: String,
    pub filename: String,
    pub mime_type: String,
    pub size_bytes: u64,
}

// ── Handlers ──────────────────────────────────────────────────────────────────

async fn list_messages(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(channel_id): Path<String>,
    Query(q): Query<ListMessagesQuery>,
) -> Result<Json<Vec<MessageResponse>>> {
    require_readable_channel(&state, &channel_id, &auth.user_id).await?;
    let limit: u8 = q.limit.unwrap_or(50).min(100);

    let messages = state
        .db
        .list_messages(&channel_id, q.before.as_deref(), limit)
        .await?;

    let mut responses = Vec::with_capacity(messages.len());
    for msg in messages {
        responses.push(message_to_response(&state, msg).await?);
    }
    Ok(Json(responses))
}

async fn create_message(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(channel_id): Path<String>,
    Json(req): Json<CreateMessageRequest>,
) -> Result<(StatusCode, Json<MessageResponse>)> {
    require_readable_channel(&state, &channel_id, &auth.user_id).await?;
    if req.content.trim().is_empty() && req.attachments.as_ref().is_none_or(Vec::is_empty) {
        return Err(AppError::BadRequest(
            "message must have content or attachment".into(),
        ));
    }

    let msg: Message = state
        .db
        .create_message(
            &channel_id,
            &auth.user_id,
            req.content.trim(),
            req.reply_to.as_deref(),
        )
        .await?
        .ok_or_else(|| AppError::Internal("no record".into()))?;
    let msg_id = msg
        .id
        .clone()
        .ok_or_else(|| AppError::Internal("no id".into()))?;

    // Attach any uploaded files.
    if let Some(attachment_ids) = &req.attachments {
        for att_id in attachment_ids {
            state
                .db
                .link_attachment_to_message(att_id, &msg_id)
                .await?;
        }
    }

    let resp = message_to_response(&state, msg).await?;
    let payload = MessagePayload {
        id: msg_id,
        channel_id: channel_id.clone(),
        author_id: auth.user_id.clone(),
        content: resp.content.clone(),
        reply_to_id: resp.reply_to_id.clone(),
        edited_at: None,
        deleted: false,
        attachments: resp.attachments.iter().map(|a| a.id.clone()).collect(),
        created_at: resp.created_at,
    };
    broadcast_to_channel(&state, &channel_id, ServerEvent::MessageCreated(payload)).await;
    Ok((StatusCode::CREATED, Json(resp)))
}

async fn edit_message(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(msg_id): Path<String>,
    Json(req): Json<EditMessageRequest>,
) -> Result<Json<MessageResponse>> {
    let msg = get_message_or_404(&state, &msg_id).await?;
    if msg.author != auth.user_id {
        return Err(AppError::Forbidden);
    }
    if msg.deleted {
        return Err(AppError::BadRequest("cannot edit a deleted message".into()));
    }
    let channel_id = msg.channel.clone();
    let updated: Message = state
        .db
        .edit_message(&msg_id, req.content.trim())
        .await?
        .ok_or(AppError::NotFound)?;
    let resp = message_to_response(&state, updated).await?;
    let payload = MessagePayload {
        id: msg_id,
        channel_id: channel_id.clone(),
        author_id: auth.user_id.clone(),
        content: resp.content.clone(),
        reply_to_id: resp.reply_to_id.clone(),
        edited_at: resp.edited_at,
        deleted: false,
        attachments: vec![],
        created_at: resp.created_at,
    };
    broadcast_to_channel(&state, &channel_id, ServerEvent::MessageEdited(payload)).await;
    Ok(Json(resp))
}

async fn delete_message(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(msg_id): Path<String>,
) -> Result<Json<serde_json::Value>> {
    let msg = get_message_or_404(&state, &msg_id).await?;
    let is_author = msg.author == auth.user_id;
    let channel_id = msg.channel.clone();
    if !is_author {
        let is_owner = is_server_owner_for_channel(&state, &channel_id, &auth.user_id).await?;
        if !is_owner {
            return Err(AppError::Forbidden);
        }
    }
    state.db.soft_delete_message(&msg_id).await?;
    broadcast_to_channel(
        &state,
        &channel_id,
        ServerEvent::MessageDeleted {
            channel_id: channel_id.clone(),
            message_id: msg_id,
        },
    )
    .await;
    Ok(Json(serde_json::json!({ "ok": true })))
}

// ── Reactions ─────────────────────────────────────────────────────────────────

async fn add_reaction(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((msg_id, emoji)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>> {
    let msg = get_message_or_404(&state, &msg_id).await?;
    let channel_id = msg.channel.clone();
    require_readable_channel(&state, &channel_id, &auth.user_id).await?;
    state
        .db
        .add_reaction(&msg_id, &auth.user_id, &emoji)
        .await?;
    broadcast_to_channel(
        &state,
        &channel_id,
        ServerEvent::ReactionAdded {
            message_id: msg_id,
            channel_id: channel_id.clone(),
            user_id: auth.user_id.clone(),
            emoji: emoji.clone(),
        },
    )
    .await;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn remove_reaction(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path((msg_id, emoji)): Path<(String, String)>,
) -> Result<Json<serde_json::Value>> {
    let msg = get_message_or_404(&state, &msg_id).await?;
    let channel_id = msg.channel.clone();
    state
        .db
        .remove_reaction(&msg_id, &auth.user_id, &emoji)
        .await?;
    broadcast_to_channel(
        &state,
        &channel_id,
        ServerEvent::ReactionRemoved {
            message_id: msg_id,
            channel_id: channel_id.clone(),
            user_id: auth.user_id.clone(),
            emoji,
        },
    )
    .await;
    Ok(Json(serde_json::json!({ "ok": true })))
}

async fn list_reactions(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthUser>,
    Path(msg_id): Path<String>,
) -> Result<Json<Vec<serde_json::Value>>> {
    let msg = get_message_or_404(&state, &msg_id).await?;
    let channel_id = msg.channel.clone();
    require_readable_channel(&state, &channel_id, &auth.user_id).await?;
    let reactions = state.db.list_reactions(&msg_id).await?;
    Ok(Json(reactions))
}

// ── Helpers ────────────────────────────────────────────────────────────────────

/// Verify the requesting user may read this channel.
async fn require_readable_channel(state: &AppState, channel_id: &str, user_id: &str) -> Result<()> {
    // Check participant record first (covers DMs, groups and server channels with explicit participants).
    if state.db.is_participant(user_id, channel_id).await? {
        return Ok(());
    }
    // Server membership check.
    if let Some(server_id) = state.db.get_channel_server_id(channel_id).await?
        && state.db.get_membership(user_id, &server_id).await?.is_some()
    {
        return Ok(());
    }
    Err(AppError::Forbidden)
}

async fn is_server_owner_for_channel(
    state: &AppState,
    channel_id: &str,
    user_id: &str,
) -> Result<bool> {
    let Some(server_id) = state.db.get_channel_server_id(channel_id).await? else {
        return Ok(false);
    };
    state.db.is_server_owner(&server_id, user_id).await
}

async fn get_message_or_404(state: &AppState, msg_id: &str) -> Result<Message> {
    state
        .db
        .get_message(msg_id)
        .await?
        .ok_or(AppError::NotFound)
}

async fn message_to_response(state: &AppState, msg: Message) -> Result<MessageResponse> {
    let id = msg.id.clone().unwrap_or_default();
    let content = if msg.deleted {
        "[deleted]".to_owned()
    } else {
        msg.content.clone()
    };

    // Fetch attachments for this message.
    let attachments = state.db.list_attachments_for_message(&id).await?;
    let attachment_refs = attachments
        .into_iter()
        .map(|a| AttachmentRef {
            id: a.id.clone().unwrap_or_default(),
            filename: a.filename,
            mime_type: a.mime_type,
            size_bytes: a.size_bytes,
        })
        .collect();

    Ok(MessageResponse {
        id,
        channel_id: msg.channel.clone(),
        author_id: msg.author.clone(),
        content,
        reply_to_id: msg.reply_to.clone(),
        edited_at: msg.edited_at,
        deleted: msg.deleted,
        attachments: attachment_refs,
        created_at: msg.created_at,
    })
}

/// Gather all channel members and push a WS event to each.
async fn broadcast_to_channel(state: &AppState, channel_id: &str, event: ServerEvent) {
    let user_ids = state.db.get_channel_member_ids(channel_id).await;
    state.ws.broadcast_to_users(&user_ids, event).await;
}

// Ensure Reaction and Attachment are used (avoids unused-import lints).
const _ASSERT_REACTION_REFERENCED: std::marker::PhantomData<Reaction> = std::marker::PhantomData;
const _ASSERT_ATTACHMENT_REFERENCED: std::marker::PhantomData<Attachment> =
    std::marker::PhantomData;
