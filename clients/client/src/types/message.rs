//! Message, attachment, reaction, emoji, sticker, and search types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::server::ThreadInfo;
use super::user::User;

/// Content that can be sent in a message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageContent {
    /// Plain text message.
    Text(String),
    /// Message with text and attachments.
    WithAttachments {
        text: String,
        attachments: Vec<Attachment>,
    },
}

/// A file attachment in a message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    /// Attachment ID.
    pub id: String,
    /// Original filename.
    pub filename: String,
    /// MIME content type.
    pub content_type: String,
    /// URL to download the attachment.
    pub url: String,
    /// File size in bytes.
    pub size: u64,
    /// Native-only raw file bytes for outbound upload flows.
    ///
    /// This is populated by host-side composers before a backend send so
    /// native backends can upload files to their remote media services.
    /// Persisted / inbound attachments leave this as `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upload_bytes: Option<Vec<u8>>,
}

impl Attachment {
    /// Construct an attachment that already exists on a remote backend.
    #[must_use]
    pub const fn remote(
        id: String,
        filename: String,
        content_type: String,
        url: String,
        size: u64,
    ) -> Self {
        Self {
            id,
            filename,
            content_type,
            url,
            size,
            upload_bytes: None,
        }
    }
}

/// Lightweight preview metadata for a replied-to message.
///
/// Loaded from the backend with each message so the UI can render a Discord-like
/// reply header without fetching the original message separately.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageReplyPreview {
    /// Backend-specific ID of the original message.
    pub message_id: String,
    /// Author ID of the original message.
    pub author_id: String,
    /// Display name of the original message author.
    pub author_display_name: String,
    /// Optional avatar URL of the original message author.
    pub author_avatar_url: Option<String>,
    /// Short text snippet shown in the reply preview line.
    pub snippet: String,
}

/// A chat message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    /// Backend-specific message ID.
    pub id: String,
    /// Author of the message.
    pub author: User,
    /// Message content.
    pub content: MessageContent,
    /// When the message was sent.
    pub timestamp: DateTime<Utc>,
    /// Attached files/images.
    pub attachments: Vec<Attachment>,
    /// Reactions on this message.
    pub reactions: Vec<Reaction>,
    /// Preview of the replied-to message, if this message is a reply.
    #[serde(default)]
    pub reply_to: Option<MessageReplyPreview>,
    /// Whether the message has been edited.
    pub edited: bool,
    /// If this message has spawned a thread, lightweight info about it.
    #[serde(default)]
    pub thread: Option<ThreadInfo>,
    /// Optional preview thumbnail URL for forum posts (Lemmy `thumbnail_url`).
    /// Populated when the post has an associated Open Graph image. Absent for
    /// non-forum messages and when no preview was fetched. Gated by the
    /// per-backend `render-previews` mechanism.
    #[serde(default)]
    pub preview_image_url: Option<String>,
}

/// A reaction on a message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reaction {
    /// Emoji or custom reaction identifier.
    pub emoji: String,
    /// Number of users who reacted with this.
    pub count: u32,
    /// Whether the authenticated user has reacted.
    pub me: bool,
}

/// A custom emoji available to the current channel/account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomEmoji {
    /// Backend-specific emoji ID.
    pub id: String,
    /// Shortcode without surrounding colons (e.g. `"party_parrot"`).
    pub shortcode: String,
    /// Optional image URL for custom emoji.
    pub image_url: Option<String>,
    /// Optional Unicode fallback glyph when available.
    pub unicode_fallback: Option<String>,
    /// Whether the emoji is animated.
    pub animated: bool,
    /// Optional server/community that owns this emoji.
    pub server_id: Option<String>,
    /// Human-readable source label shown in search results.
    pub source_name: Option<String>,
}

/// A sticker available to the current channel/account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StickerItem {
    /// Backend-specific sticker ID.
    pub id: String,
    /// Sticker display name.
    pub name: String,
    /// URL to the sticker preview/full asset.
    pub image_url: String,
    /// Optional pack or collection name.
    pub pack_name: Option<String>,
    /// Optional descriptive text.
    pub description: Option<String>,
    /// Optional server/community that owns this sticker.
    pub server_id: Option<String>,
    /// Human-readable source label shown in search results.
    pub source_name: Option<String>,
    /// Asset format (e.g. `"png"`, `"apng"`, `"json"`, `"lottie"`).
    pub format: String,
}

/// Query options for fetching messages.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageQuery {
    /// Fetch messages before this message ID.
    pub before: Option<String>,
    /// Fetch messages after this message ID.
    pub after: Option<String>,
    /// Fetch a window of messages centered around this message ID.
    ///
    /// Used for jump-to-message flows (search results, pinned messages,
    /// notifications) where the UI needs surrounding history even if the
    /// target message is far outside the currently loaded window.
    pub around: Option<String>,
    /// Maximum number of messages to return.
    pub limit: Option<u32>,
}

/// Query options for backend-native message search.
///
/// Models Discord-like search primitives while remaining generic enough for
/// backends that expose different server-side search APIs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageSearchQuery {
    /// Free-text search string.
    pub text: String,
    /// Restrict search to a specific channel, if supported.
    pub channel_id: Option<String>,
    /// Restrict search to a specific server/community, if supported.
    pub server_id: Option<String>,
    /// Restrict search to a specific author, if supported.
    pub author_id: Option<String>,
    /// Restrict search to messages containing a link.
    pub has_link: bool,
    /// Restrict search to messages mentioning a specific user.
    pub mentions_user_id: Option<String>,
    /// Maximum number of hits to return.
    pub limit: Option<u32>,
}

/// A backend search result hit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageSearchHit {
    /// Channel containing the hit.
    pub channel_id: String,
    /// Optional display name for the channel containing the hit.
    pub channel_name: Option<String>,
    /// Optional server/community containing the hit.
    pub server_id: Option<String>,
    /// The matched message.
    pub message: Message,
}
