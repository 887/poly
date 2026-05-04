//! Composer helper functions and types for the chat view.
//!
//! Contains slash-command filtering, built-in command transforms,
//! reply-preview snippet builder, attachment preview building and
//! appending, and the contextual compose placeholder logic.

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use dioxus::html::HasFileData;
use dioxus::prelude::*;
use poly_client::{Attachment, ChatCommand, CommandScope, MessageContent};

// ── Built-in slash commands ───────────────────────────────────────────────────

/// Built-in slash commands always available in every channel.
///
/// Tuple fields: (name, description, usage_hint)
pub(super) const BUILTIN_COMMANDS: &[(&str, &str, &str)] = &[
    ("shrug", r"Append ¯\_(ツ)_/¯ to your message", ""),
    ("tableflip", "Append (╯°□°）╯︵ ┻━┻ to your message", ""),
    ("unflip", "Append ┬─┬ ノ( ゜-゜ノ) to your message", ""),
    ("me", "Display your action text in italics", "<action>"),
    ("spoiler", "Send text hidden as a spoiler", "<text>"),
    ("tts", "Send a text-to-speech message", "<text>"),
    ("nick", "Change your server nickname", "<new nickname>"),
    (
        "msg",
        "Send a private message to a user",
        "<@user> <message>",
    ),
];

/// Return all slash commands (built-in + backend) matching the given query string.
///
/// `query` is the text the user typed after the leading `/`.
pub(super) fn filtered_slash_commands(query: &str, backend_cmds: &[ChatCommand]) -> Vec<ChatCommand> {
    let q = query.to_lowercase();
    let builtin = BUILTIN_COMMANDS
        .iter()
        .filter(|(name, desc, _)| {
            q.is_empty() || name.starts_with(q.as_str()) || desc.to_lowercase().contains(q.as_str())
        })
        .map(|(name, desc, usage)| ChatCommand {
            name: (*name).to_string(),
            description: (*desc).to_string(),
            provider: "Built-in".to_string(),
            is_builtin: true,
            usage: if usage.is_empty() {
                None
            } else {
                Some((*usage).to_string())
            },
            scope: CommandScope::Global,
        });
    let backend = backend_cmds
        .iter()
        .filter(|c| {
            q.is_empty()
                || c.name.starts_with(q.as_str())
                || c.description.to_lowercase().contains(q.as_str())
        })
        .cloned();
    builtin.chain(backend).collect()
}

/// Apply built-in slash command transforms to message text before sending.
///
/// Replaces known commands like `/shrug` with their text equivalents.
/// Returns `None` if the text is not a recognized transformable built-in.
pub(super) fn apply_builtin_command(text: &str) -> Option<String> {
    if text == "/shrug" {
        return Some(r"¯\_(ツ)_/¯".to_string());
    }
    if text == "/tableflip" {
        return Some("(╯°□°）╯︵ ┻━┻".to_string());
    }
    if text == "/unflip" {
        return Some("┬─┬ ノ( ゜-゜ノ)".to_string());
    }
    if let Some(action) = text.strip_prefix("/me ") {
        return Some(format!("*{action}*"));
    }
    if let Some(spoiled) = text.strip_prefix("/spoiler ") {
        return Some(format!("||{spoiled}||"));
    }
    None
}

/// Build a short snippet suitable for reply previews.
pub(super) fn reply_preview_snippet(content: &MessageContent) -> String {
    let raw = match content {
        MessageContent::Text(text) | MessageContent::WithAttachments { text, .. } => text.clone(),
    };
    raw.chars().take(80).collect()
}

/// Extract the slash-command query from a composer text string.
///
/// Returns the text after the leading `/` and before the first space,
/// or `""` if the text is not a slash command.
pub(super) fn slash_command_query(text: &str) -> &str {
    text.trim_start()
        .strip_prefix('/')
        .unwrap_or("")
        .split(' ')
        .next()
        .unwrap_or("")
}

/// Return the contextual compose placeholder for the given channel state.
pub(super) fn contextual_compose_placeholder(
    current_channel: Option<&poly_client::Channel>,
    is_dm_channel: bool,
    is_group_channel: bool,
) -> String {
    use crate::i18n::t_args;
    if is_dm_channel {
        return t_args(
            "chat-type-message-user",
            &[(
                "user",
                current_channel.map_or("", |channel| channel.name.as_str()),
            )],
        );
    }
    if is_group_channel {
        return t_args(
            "chat-type-message-group",
            &[(
                "group",
                current_channel.map_or("", |channel| channel.name.as_str()),
            )],
        );
    }
    t_args(
        "chat-type-message-channel",
        &[(
            "channel",
            current_channel.map_or("", |channel| channel.name.as_str()),
        )],
    )
}

// ── Attachment preview types and builders ────────────────────────────────────

/// In-memory representation of a file attachment the user has selected but
/// not yet sent.  Holds the raw bytes so they can be uploaded when the message
/// is sent, and a base64 preview URL for images so a thumbnail can be shown
/// in the composer attachment strip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PendingAttachmentPreview {
    pub(super) id: String,
    pub(super) filename: String,
    pub(super) content_type: String,
    pub(super) size: u64,
    pub(super) preview_url: Option<String>,
    pub(super) upload_bytes: Vec<u8>,
}

/// Convert a `PendingAttachmentPreview` into the `Attachment` type expected by
/// the send-message API.  The upload bytes are carried through so the backend
/// can perform the actual upload.
pub(super) fn pending_attachment_to_attachment(preview: &PendingAttachmentPreview) -> Attachment {
    let mut att = Attachment::remote(
        preview.id.clone(),
        preview.filename.clone(),
        preview.content_type.clone(),
        preview.preview_url.clone().unwrap_or_default(),
        preview.size,
    );
    att.upload_bytes = Some(preview.upload_bytes.clone());
    att
}

/// Read a list of `FileData` objects (from a file-picker or drag-drop event)
/// and build `PendingAttachmentPreview` entries for each file, including
/// a base64 data URL for image files ≤ 5 MiB.
pub(super) async fn build_attachment_previews(
    files: Vec<dioxus::html::FileData>,
) -> Vec<PendingAttachmentPreview> {
    let mut previews = Vec::new();

    for (index, file) in files.into_iter().enumerate() {
        let filename = file.name();
        let content_type = file
            .content_type()
            .unwrap_or_else(|| "application/octet-stream".to_string());
        let size = file.size();
        let upload_bytes = match file.read_bytes().await {
            Ok(bytes) => bytes.to_vec(),
            Err(err) => {
                tracing::warn!("failed to read attachment bytes: {err}");
                continue;
            }
        };

        let preview_url = if content_type.starts_with("image/") && size <= 5_000_000 {
            Some(format!(
                "data:{content_type};base64,{}",
                BASE64_STANDARD.encode(&upload_bytes)
            ))
        } else {
            None
        };

        previews.push(PendingAttachmentPreview {
            id: format!("pending-{}-{}-{}", file.last_modified(), index, filename),
            filename,
            content_type,
            size,
            preview_url,
            upload_bytes,
        });
    }

    previews
}

/// Append new attachment previews (built from the given files) to an existing
/// `Signal<Vec<PendingAttachmentPreview>>`.  Does nothing if `files` is empty.
pub(super) async fn append_attachment_previews(
    mut pending_attachments: Signal<Vec<PendingAttachmentPreview>>,
    files: Vec<dioxus::html::FileData>,
) {
    if files.is_empty() {
        return;
    }

    let mut next = pending_attachments.read().clone();
    next.extend(build_attachment_previews(files).await);
    pending_attachments.set(next);
}
