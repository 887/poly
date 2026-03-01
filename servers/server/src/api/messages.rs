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

    let raw: Vec<serde_json::Value> = match &q.before {
        Some(cursor) => state
            .db
            .query(
                "SELECT * FROM message \
                     WHERE channel = type::thing($ch) AND id < type::thing($cursor) \
                     ORDER BY id DESC LIMIT $lim",
            )
            .bind(("ch", channel_id.clone()))
            .bind(("cursor", cursor.clone()))
            .bind(("lim", limit))
            .await?
            .take(0)
            .map_err(AppError::Db)?,
        None => state
            .db
            .query(
                "SELECT * FROM message WHERE channel = type::thing($ch) \
                     ORDER BY id DESC LIMIT $lim",
            )
            .bind(("ch", channel_id.clone()))
            .bind(("lim", limit))
            .await?
            .take(0)
            .map_err(AppError::Db)?,
    };

    let messages = from_values::<Message>(raw)?;
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
    if req.content.trim().is_empty() && req.attachments.as_ref().is_none_or(|a| a.is_empty()) {
        return Err(AppError::BadRequest(
            "message must have content or attachment".into(),
        ));
    }

    let now = Utc::now();
    let raw: Vec<serde_json::Value> = state
        .db
        .query(
            "CREATE message CONTENT { \
              channel: type::thing($ch), \
              author: type::thing($author), \
              content: $content, \
              reply_to: $reply_to, \
              edited_at: NONE, \
              deleted: false, \
              created_at: $now \
            } RETURN *",
        )
        .bind(("ch", channel_id.clone()))
        .bind(("author", auth.user_id.clone()))
        .bind(("content", req.content.trim().to_owned()))
        .bind((
            "reply_to",
            req.reply_to.as_ref().map(|r| format!("message:{r}")),
        ))
        .bind(("now", now))
        .await?
        .take(0)
        .map_err(AppError::Db)?;

    let msg: Message = raw
        .into_iter()
        .next()
        .map(|v| {
            serde_json::from_value::<Message>(v).map_err(|e| AppError::Internal(e.to_string()))
        })
        .transpose()?
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
                .query("UPDATE type::thing($id) SET message = type::thing($mid)")
                .bind(("id", att_id.clone()))
                .bind(("mid", msg_id.clone()))
                .await?
                .check()
                .map_err(AppError::Db)?;
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
    let now = Utc::now();
    let raw: Option<serde_json::Value> = state
        .db
        .query("UPDATE type::thing($id) SET content = $c, edited_at = $now RETURN *")
        .bind(("id", msg_id.clone()))
        .bind(("c", req.content.trim().to_owned()))
        .bind(("now", now))
        .await?
        .take(0)
        .map_err(AppError::Db)?;
    let updated: Message = raw
        .map(|v| {
            serde_json::from_value::<Message>(v).map_err(|e| AppError::Internal(e.to_string()))
        })
        .transpose()?
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
    state
        .db
        .query("UPDATE type::thing($id) SET deleted = true, content = '[deleted]'")
        .bind(("id", msg_id.clone()))
        .await?
        .check()
        .map_err(AppError::Db)?;
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
        .query(
            "IF (SELECT count() FROM reaction WHERE message = type::thing($mid) \
                AND user = type::thing($uid) AND emoji = $em GROUP ALL)[0].count == 0 { \
              CREATE reaction CONTENT { \
                message: type::thing($mid), user: type::thing($uid), emoji: $em \
              } \
            }",
        )
        .bind(("mid", msg_id.clone()))
        .bind(("uid", auth.user_id.clone()))
        .bind(("em", emoji.clone()))
        .await?
        .check()
        .map_err(AppError::Db)?;
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
        .query(
            "DELETE reaction WHERE message = type::thing($mid) \
             AND user = type::thing($uid) AND emoji = $em",
        )
        .bind(("mid", msg_id.clone()))
        .bind(("uid", auth.user_id.clone()))
        .bind(("em", emoji.clone()))
        .await?
        .check()
        .map_err(AppError::Db)?;
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
    let reactions: Vec<serde_json::Value> = state
        .db
        .query("SELECT * FROM reaction WHERE message = type::thing($mid)")
        .bind(("mid", msg_id))
        .await?
        .take(0)
        .map_err(AppError::Db)?;
    Ok(Json(reactions))
}

// ── Helpers ────────────────────────────────────────────────────────────────────

/// Verify the requesting user may read this channel.
async fn require_readable_channel(state: &AppState, channel_id: &str, user_id: &str) -> Result<()> {
    // Check participant record first (covers DMs, groups and server channels with explicit participants).
    let part: Option<serde_json::Value> = state
        .db
        .query(
            "SELECT * FROM participant WHERE \
             channel = type::thing($ch) AND user = type::thing($uid) LIMIT 1",
        )
        .bind(("ch", channel_id.to_owned()))
        .bind(("uid", user_id.to_owned()))
        .await?
        .take(0)
        .map_err(AppError::Db)?;
    if part.is_some() {
        return Ok(());
    }
    // Server membership check.
    let ch_raw: Option<serde_json::Value> = state
        .db
        .query("SELECT server FROM type::thing($ch) LIMIT 1")
        .bind(("ch", channel_id.to_owned()))
        .await?
        .take(0)
        .map_err(AppError::Db)?;
    if let Some(server_id) = ch_raw
        .as_ref()
        .and_then(|ch_val| ch_val.get("server"))
        .and_then(|v| v.as_str())
    {
        let member: Option<serde_json::Value> = state
            .db
            .query(
                "SELECT * FROM membership WHERE \
                 server = type::thing($sid) AND user = type::thing($uid) LIMIT 1",
            )
            .bind(("sid", server_id.to_owned()))
            .bind(("uid", user_id.to_owned()))
            .await?
            .take(0)
            .map_err(AppError::Db)?;
        if member.is_some() {
            return Ok(());
        }
    }
    Err(AppError::Forbidden)
}

async fn is_server_owner_for_channel(
    state: &AppState,
    channel_id: &str,
    user_id: &str,
) -> Result<bool> {
    let ch_raw: Option<serde_json::Value> = state
        .db
        .query("SELECT server FROM type::thing($ch) LIMIT 1")
        .bind(("ch", channel_id.to_owned()))
        .await?
        .take(0)
        .map_err(AppError::Db)?;
    let Some(server_id) = ch_raw
        .as_ref()
        .and_then(|v| v.get("server"))
        .and_then(|v| v.as_str())
        .map(str::to_owned)
    else {
        return Ok(false);
    };
    let owner: Option<serde_json::Value> = state
        .db
        .query("SELECT * FROM type::thing($sid) WHERE owner = type::thing($uid) LIMIT 1")
        .bind(("sid", server_id))
        .bind(("uid", user_id.to_owned()))
        .await?
        .take(0)
        .map_err(AppError::Db)?;
    Ok(owner.is_some())
}

async fn get_message_or_404(state: &AppState, msg_id: &str) -> Result<Message> {
    let raw: Option<serde_json::Value> = state
        .db
        .query("SELECT * FROM type::thing($id) LIMIT 1")
        .bind(("id", msg_id.to_owned()))
        .await?
        .take(0)
        .map_err(AppError::Db)?;
    raw.map(|v| serde_json::from_value::<Message>(v).map_err(|e| AppError::Internal(e.to_string())))
        .transpose()?
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
    let att_raw: Vec<serde_json::Value> = state
        .db
        .query("SELECT * FROM attachment WHERE message = type::thing($mid)")
        .bind(("mid", id.clone()))
        .await?
        .take(0)
        .map_err(AppError::Db)?;
    let attachments: Vec<Attachment> = from_values(att_raw)?;
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
    // Collect member user IDs from server memberships and participants.
    let server_members: Vec<String> = state
        .db
        .query(
            "SELECT user FROM membership WHERE server = \
             (SELECT server FROM type::thing($ch) LIMIT 1)[0].server",
        )
        .bind(("ch", channel_id.to_owned()))
        .await
        .ok()
        .and_then(|mut r| r.take::<Vec<String>>("user").ok())
        .unwrap_or_default();

    let participants: Vec<String> = state
        .db
        .query("SELECT user FROM participant WHERE channel = type::thing($ch)")
        .bind(("ch", channel_id.to_owned()))
        .await
        .ok()
        .and_then(|mut r| r.take::<Vec<String>>("user").ok())
        .unwrap_or_default();

    let user_ids: Vec<String> = {
        let mut set = std::collections::HashSet::new();
        for id in server_members.into_iter().chain(participants) {
            set.insert(id);
        }
        set.into_iter().collect()
    };

    state.ws.broadcast_to_users(&user_ids, event).await;
}

/// Deserialise a `Vec<serde_json::Value>` into `Vec<T>`.
fn from_values<T: serde::de::DeserializeOwned>(raw: Vec<serde_json::Value>) -> Result<Vec<T>> {
    raw.into_iter()
        .map(|v| serde_json::from_value::<T>(v).map_err(|e| AppError::Internal(e.to_string())))
        .collect()
}

// Ensure Reaction is used (avoids unused-import lints).
const _: fn() = || {
    let _ = std::mem::size_of::<Reaction>();
};
