//! Typed Stoat REST API models used by the native client implementation.
//!
//! These types are intentionally kept internal to `poly-stoat` so external app
//! crates stay isolated from Stoat/Revolt-specific protocol details.

use chrono::{DateTime, Utc};
use poly_client::{
    Attachment, BackendType, Category, Channel, ChannelType, ClientError, ClientResult, Message,
    MessageContent, MessageReplyPreview, PresenceStatus, Reaction, Server, User,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Minimal Autumn upload response for newly uploaded files.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StoatAutumnUploadResponse {
    /// Uploaded file ID used in later message-send payloads.
    #[serde(rename = "id")]
    pub file_id: String,
}

/// Root server configuration returned by `GET /`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct StoatRootConfig {
    /// Revolt/Stoat API version string.
    pub revolt: String,
    /// Optional service feature configuration.
    #[serde(default)]
    pub features: StoatRootFeatures,
    /// Server-provided websocket URL.
    pub ws: String,
}

impl StoatRootConfig {
    /// File-service base URL when the instance exposes Autumn.
    #[must_use]
    pub fn autumn_base_url(&self) -> Option<&str> {
        self.features
            .autumn
            .as_ref()
            .filter(|feature| feature.enabled)
            .map(|feature| feature.url.as_str())
    }
}

/// Feature configuration block returned by `GET /`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct StoatRootFeatures {
    /// Autumn file-service configuration.
    #[serde(default)]
    pub autumn: Option<StoatServiceFeature>,
}

/// Generic service feature descriptor.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StoatServiceFeature {
    /// Whether the service is enabled.
    pub enabled: bool,
    /// Service base URL (absent when the service is disabled).
    #[serde(default)]
    pub url: String,
}

/// Email/password login payload for `POST /auth/session/login`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoatPasswordLoginRequest {
    /// Account email address.
    pub email: String,
    /// Account password.
    pub password: String,
    /// Friendly client name shown in Stoat session management.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub friendly_name: Option<String>,
}

/// Friend-request payload for `POST /users/friend`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoatSendFriendRequest {
    /// Username and discriminator, e.g. `alice#1234`.
    pub username: String,
}

/// Reply target metadata for `POST /channels/{target}/messages`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoatReplyIntent {
    /// Referenced message ID.
    pub id: String,
    /// Whether sending this reply should mention the original author.
    pub mention: bool,
    /// Whether the request should fail when the referenced message is missing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fail_if_not_exists: Option<bool>,
}

/// Minimal message-send payload for Stoat text and reply sends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoatSendMessageRequest {
    /// Optional client-provided nonce/idempotency hint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
    /// Plain text message content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Pre-uploaded attachment IDs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<String>>,
    /// Optional reply target list.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replies: Option<Vec<StoatReplyIntent>>,
}

impl StoatSendMessageRequest {
    /// Build a Stoat send request from already-uploaded attachment IDs.
    #[must_use]
    pub fn new(
        text: String,
        attachment_ids: Vec<String>,
        reply_to_message_id: Option<String>,
        nonce: String,
    ) -> Self {
        Self {
            nonce: Some(nonce),
            content: Some(text),
            attachments: (!attachment_ids.is_empty()).then_some(attachment_ids),
            replies: reply_to_message_id.map(|id| {
                vec![StoatReplyIntent {
                    id,
                    mention: false,
                    fail_if_not_exists: Some(true),
                }]
            }),
        }
    }
}

/// Session login response returned by Stoat.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "result")]
pub enum StoatLoginResponse {
    /// Login succeeded and a new session token was issued.
    Success {
        /// Session ID.
        #[serde(rename = "_id")]
        id: String,
        /// Authenticated user ID.
        user_id: String,
        /// Session token used for `X-Session-Token`.
        token: String,
        /// Session friendly name.
        name: String,
        /// Last seen timestamp.
        last_seen: String,
        /// Optional session origin.
        origin: Option<String>,
    },
    /// Login requires MFA resolution before a session can be created.
    #[serde(rename = "MFA")]
    Mfa {
        /// MFA ticket returned by Stoat.
        ticket: String,
        /// Allowed MFA methods for this account.
        allowed_methods: Vec<String>,
    },
    /// Account exists but is disabled.
    Disabled {
        /// User ID of the disabled account.
        user_id: String,
    },
}

/// Successful Stoat login metadata before the current user profile is resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoatLoginSuccess {
    /// Session ID.
    pub session_id: String,
    /// Authenticated user ID.
    pub user_id: String,
    /// Session token.
    pub token: String,
    /// Optional friendly session name.
    pub session_name: Option<String>,
}

/// Simplified authenticated session data used internally by `StoatClient`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoatAuthenticatedSession {
    /// Session ID or fallback synthetic ID for token-restore flows.
    pub session_id: String,
    /// Authenticated user ID.
    pub user_id: String,
    /// Session token.
    pub token: String,
    /// Resolved current user profile.
    pub user: User,
    /// Optional friendly session name.
    pub session_name: Option<String>,
}

impl StoatLoginResponse {
    /// Convert the Stoat login response into a success payload or a typed
    /// authentication error.
    pub fn into_success(self) -> ClientResult<StoatLoginSuccess> {
        match self {
            Self::Success {
                id,
                user_id,
                token,
                name,
                last_seen: _,
                origin: _,
            } => Ok(StoatLoginSuccess {
                session_id: id,
                user_id,
                token,
                session_name: Some(name),
            }),
            Self::Mfa {
                ticket: _,
                allowed_methods,
            } => Err(ClientError::AuthFailed(format!(
                "Stoat requires MFA before login can continue (allowed methods: {})",
                allowed_methods.join(", ")
            ))),
            Self::Disabled { user_id } => Err(ClientError::AuthFailed(format!(
                "Stoat account is disabled for user {user_id}"
            ))),
        }
    }
}

/// Current-user / user-profile payload.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StoatUser {
    /// User ID.
    #[serde(rename = "_id")]
    pub id: String,
    /// Username.
    pub username: String,
    /// Username discriminator.
    pub discriminator: String,
    /// Optional display name.
    #[serde(default)]
    pub display_name: Option<String>,
    /// Optional avatar attachment.
    #[serde(default)]
    pub avatar: Option<StoatFile>,
    /// All known relationships from the authenticated user to other users.
    #[serde(default)]
    pub relations: Vec<StoatRelationship>,
    /// Relationship between the authenticated account and this user.
    #[serde(default)]
    pub relationship: Option<StoatRelationshipStatus>,
    /// Optional active status.
    #[serde(default)]
    pub status: Option<StoatUserStatus>,
    /// Whether the user is currently online.
    pub online: bool,
}

/// A role definition returned inside a Stoat server payload.
///
/// Revolt stores roles as an object keyed by role ID.  We flatten that
/// into a `Vec` with the key promoted to `StoatRole::id` so we can
/// iterate normally.  See `StoatServer::into_poly_roles`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StoatRole {
    /// Role display name.
    pub name: String,
    /// Optional hex colour (e.g. `"#5865F2"`).  Revolt stores colours as
    /// integers; the Stoat fork may expose them as strings.
    #[serde(default)]
    pub colour: Option<String>,
    /// Permission bitmask (Revolt permission model).
    #[serde(default)]
    pub permissions: Option<serde_json::Value>,
    /// Sort rank — lower number means lower in the hierarchy.
    #[serde(default)]
    pub rank: Option<u32>,
}

/// Server/community payload returned by Stoat.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StoatServer {
    /// Server ID.
    #[serde(rename = "_id")]
    pub id: String,
    /// Owner user ID.
    pub owner: String,
    /// Server display name.
    pub name: String,
    /// Optional description.
    #[serde(default)]
    pub description: Option<String>,
    /// Channel IDs belonging to this server.
    pub channels: Vec<String>,
    /// Optional server categories.
    #[serde(default)]
    pub categories: Option<Vec<StoatCategory>>,
    /// Optional server icon.
    #[serde(default)]
    pub icon: Option<StoatFile>,
    /// Optional server banner.
    #[serde(default)]
    pub banner: Option<StoatFile>,
    /// Role definitions keyed by role ID.  Most Stoat instances populate this;
    /// absent on servers with no custom roles.
    #[serde(default)]
    pub roles: Option<std::collections::HashMap<String, StoatRole>>,
}

impl StoatServer {
    /// Convert the Stoat server model into Poly's backend-agnostic server
    /// shape.
    #[must_use]
    pub fn into_poly_server(
        self,
        account_id: String,
        account_display_name: String,
        unread_count: u32,
        mention_count: u32,
        autumn_base_url: Option<&str>,
    ) -> Server {
        Server {
            id: self.id,
            name: self.name,
            icon_url: self
                .icon
                .and_then(|icon| icon.download_url(autumn_base_url)),
            banner_url: self
                .banner
                .and_then(|banner| banner.download_url(autumn_base_url)),
            categories: self
                .categories
                .unwrap_or_default()
                .into_iter()
                .map(StoatCategory::into_poly_category)
                .collect(),
            backend: BackendType::from(crate::SLUG),
            unread_count,
            mention_count,
            account_id,
            account_display_name,
            default_channel_id: None,
            description: None,
            star_count: None,
            language: None,
            forks_count: None,
            open_issues_count: None,
        }
    }

    /// Convert the server's role map into Poly's [`poly_client::Role`] vector.
    ///
    /// Roles are sorted by `rank` ascending (lower rank = lower in the hierarchy).
    /// Roles with no rank are placed after those that have one.
    ///
    /// Note: Revolt's permission bitmask is not mapped to `MemberPermissions` at
    /// this stage — that requires understanding the calling member's effective grants.
    /// All permission fields default to `false`; callers that need grant-level detail
    /// should consult `StoatServerMemberMe.roles` and the individual role permissions.
    #[must_use]
    pub fn into_poly_roles(self) -> Vec<poly_client::Role> {
        let Some(roles_map) = self.roles else {
            return Vec::new();
        };

        let mut roles: Vec<poly_client::Role> = roles_map
            .into_iter()
            .map(|(id, role)| poly_client::Role {
                id,
                name: role.name,
                color: role.colour,
                permissions: poly_client::MemberPermissions {
                    manage_server: false,
                    manage_channels: false,
                    manage_roles: false,
                    kick_members: false,
                    ban_members: false,
                    manage_messages: false,
                    timeout_members: false,
                    display_role: String::new(),
                    power_level: None,
                },
                position: role.rank.unwrap_or(u32::MAX),
            })
            .collect();

        roles.sort_by_key(|r| r.position);
        roles
    }
}

/// Stoat server category.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StoatCategory {
    /// Category ID.
    pub id: String,
    /// Display title.
    pub title: String,
    /// Channel IDs in this category.
    pub channels: Vec<String>,
}

impl StoatCategory {
    /// Convert the Stoat category model into Poly's category shape.
    #[must_use]
    pub fn into_poly_category(self) -> Category {
        Category {
            id: self.id,
            name: self.title,
            channel_ids: self.channels,
        }
    }
}

/// Simplified channel payload used by the current native Stoat slice.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StoatChannel {
    /// Channel kind discriminator.
    pub channel_type: String,
    /// Channel ID.
    #[serde(rename = "_id")]
    pub id: String,
    /// Server ID for server channels.
    #[serde(default)]
    pub server: Option<String>,
    /// Saved-messages owner when this is a personal notes channel.
    #[serde(default)]
    pub user: Option<String>,
    /// Whether a DM is currently open on both sides.
    #[serde(default)]
    pub active: Option<bool>,
    /// DM/group recipient user IDs.
    #[serde(default)]
    pub recipients: Option<Vec<String>>,
    /// Owner user ID for group chats.
    #[serde(default)]
    pub owner: Option<String>,
    /// Channel display name.
    #[serde(default)]
    pub name: Option<String>,
    /// Optional description.
    #[serde(default)]
    pub description: Option<String>,
    /// Optional group icon.
    #[serde(default)]
    pub icon: Option<StoatFile>,
    /// Last message ID when present.
    #[serde(default)]
    pub last_message_id: Option<String>,
    /// Voice metadata for server voice channels.
    #[serde(default)]
    pub voice: Option<StoatVoiceInformation>,
}

impl StoatChannel {
    /// Whether this channel is a one-to-one DM.
    #[must_use]
    pub fn is_direct_message(&self) -> bool {
        self.channel_type == "DirectMessage"
    }

    /// Whether this channel is a multi-user group DM.
    #[must_use]
    pub fn is_group(&self) -> bool {
        self.channel_type == "Group"
    }

    /// Whether this channel is the user's personal saved-messages room.
    #[must_use]
    pub fn is_saved_messages(&self) -> bool {
        self.channel_type == "SavedMessages"
    }

    /// Convert the Stoat channel model into a Poly server-channel.
    pub fn into_poly_server_channel(
        self,
        unread_count: u32,
        mention_count: u32,
    ) -> ClientResult<Channel> {
        let Some(server_id) = self.server else {
            return Err(ClientError::NotSupported(format!(
                "Stoat channel {} is not a server channel",
                self.id
            )));
        };

        let Some(name) = self.name else {
            return Err(ClientError::Internal(format!(
                "Stoat server channel {} is missing a name",
                self.id
            )));
        };

        let channel_type = if self.channel_type == "VoiceChannel" || self.voice.is_some() {
            ChannelType::Voice
        } else {
            ChannelType::Text
        };

        Ok(Channel {
            id: self.id,
            name,
            channel_type,
            server_id,
            unread_count,
            mention_count,
            last_message_id: self.last_message_id,
            forum_tags: None,
            parent_channel_id: None,
            thread_metadata: None,
        })
    }
}

/// Relationship entry exposed on `GET /users/@me` and other user payloads.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StoatRelationship {
    /// Other user's ID.
    #[serde(rename = "_id")]
    pub user_id: String,
    /// Current relationship status.
    pub status: StoatRelationshipStatus,
}

/// Stoat/Revolt relationship status values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum StoatRelationshipStatus {
    /// No relationship with the user.
    None,
    /// The referenced user is the authenticated user.
    User,
    /// The users are friends.
    Friend,
    /// Outgoing pending friend request.
    Outgoing,
    /// Incoming pending friend request.
    Incoming,
    /// Authenticated user blocked the other user.
    Blocked,
    /// Other user blocked the authenticated user.
    BlockedOther,
}

/// Voice metadata for voice-enabled server channels.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StoatVoiceInformation {
    /// Maximum concurrent users when configured.
    #[serde(default)]
    pub max_users: Option<u32>,
}

/// Unread-state payload returned by `GET /sync/unreads`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StoatChannelUnread {
    /// Composite channel/user key.
    #[serde(rename = "_id")]
    pub key: StoatChannelCompositeKey,
    /// Last read message ID if known.
    #[serde(default)]
    pub last_id: Option<String>,
    /// Mentioned message IDs.
    #[serde(default)]
    pub mentions: Option<Vec<String>>,
}

/// All-members response returned by `GET /servers/{target}/members`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StoatAllMemberResponse {
    /// Server member records.
    pub members: Vec<StoatMember>,
    /// Matching user records for those members.
    pub users: Vec<StoatUser>,
}

impl StoatChannelUnread {
    /// Mention count derived from the unread payload.
    #[must_use]
    pub fn mention_count(&self) -> u32 {
        self.mentions
            .as_ref()
            .map_or(0, |mentions| u32::try_from(mentions.len()).unwrap_or(u32::MAX))
    }

    /// Conservative unread estimate used until full message sync lands.
    #[must_use]
    pub fn approximate_unread_count(&self) -> u32 {
        let mention_count = self.mention_count();
        if mention_count > 0 {
            mention_count
        } else if self.last_id.is_some() {
            1
        } else {
            0
        }
    }
}

/// Composite key for a channel unread entry.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StoatChannelCompositeKey {
    /// Channel ID.
    pub channel: String,
    /// User ID.
    pub user: String,
}

/// Response shape for Stoat bulk message fetches.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum StoatBulkMessageResponse {
    /// Simple array response.
    Messages(Vec<StoatMessage>),
    /// Expanded response with bundled users and members.
    Expanded(StoatBulkMessageEnvelope),
}

impl StoatBulkMessageResponse {
    /// Split the response into messages plus bundled lookup maps.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        Vec<StoatMessage>,
        HashMap<String, StoatUser>,
        HashMap<String, StoatMember>,
    ) {
        match self {
            Self::Messages(messages) => (messages, HashMap::new(), HashMap::new()),
            Self::Expanded(envelope) => (
                envelope.messages,
                envelope
                    .users
                    .into_iter()
                    .map(|user| (user.id.clone(), user))
                    .collect(),
                envelope
                    .members
                    .unwrap_or_default()
                    .into_iter()
                    .map(|member| (member.key.user.clone(), member))
                    .collect(),
            ),
        }
    }
}

/// Expanded bulk message envelope.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StoatBulkMessageEnvelope {
    /// Returned messages.
    pub messages: Vec<StoatMessage>,
    /// Bundled users.
    pub users: Vec<StoatUser>,
    /// Bundled members.
    #[serde(default)]
    pub members: Option<Vec<StoatMember>>,
}

/// Stoat message payload.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StoatMessage {
    /// Message ID.
    #[serde(rename = "_id")]
    pub id: String,
    /// Channel ID.
    pub channel: String,
    /// Author user ID.
    pub author: String,
    /// Optional bundled user object.
    #[serde(default)]
    pub user: Option<StoatUser>,
    /// Optional bundled member object.
    #[serde(default)]
    pub member: Option<StoatMember>,
    /// Message text content.
    #[serde(default)]
    pub content: Option<String>,
    /// Optional system-message payload.
    #[serde(default)]
    pub system: Option<serde_json::Value>,
    /// Optional attachments.
    #[serde(default)]
    pub attachments: Option<Vec<StoatFile>>,
    /// Edit timestamp if message was edited.
    #[serde(default)]
    pub edited: Option<String>,
    /// Reply target IDs.
    #[serde(default)]
    pub replies: Option<Vec<String>>,
    /// Reaction map of emoji to reacting user IDs.
    #[serde(default)]
    pub reactions: Option<HashMap<String, Vec<String>>>,
    /// Optional webhook metadata.
    #[serde(default)]
    pub webhook: Option<StoatMessageWebhook>,
}

impl StoatMessage {
    /// Build a Poly message from the raw Stoat payload.
    #[must_use]
    pub fn into_poly_message(
        self,
        bundled_users: &HashMap<String, StoatUser>,
        bundled_members: &HashMap<String, StoatMember>,
        current_user_id: Option<&str>,
        autumn_base_url: Option<&str>,
    ) -> Message {
        let author = self.resolve_author(bundled_users, bundled_members, autumn_base_url);
        let attachments: Vec<Attachment> = self
            .attachments
            .clone()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|file| file.into_poly_attachment(autumn_base_url))
            .collect();
        let text = self.display_text();
        let content = if attachments.is_empty() {
            MessageContent::Text(text)
        } else {
            MessageContent::WithAttachments {
                text,
                attachments: attachments.clone(),
            }
        };

        Message {
            id: self.id.clone(),
            author,
            content,
            timestamp: stoat_message_timestamp(&self.id),
            attachments,
            reactions: self
                .reactions
                .unwrap_or_default()
                .into_iter()
                .map(|(emoji, users)| Reaction {
                    emoji,
                    count: u32::try_from(users.len()).unwrap_or(u32::MAX),
                    me: current_user_id
                        .is_some_and(|current| users.iter().any(|user| user == current)),
                })
                .collect(),
            reply_to: None,
            edited: self.edited.is_some(),
            thread: None,
            preview_image_url: None,
        }
    }

    /// First reply target ID when the message replies to another message.
    #[must_use]
    pub fn primary_reply_id(&self) -> Option<&str> {
        self.replies
            .as_ref()
            .and_then(|replies| replies.first().map(String::as_str))
    }

    fn display_text(&self) -> String {
        self.content.clone().unwrap_or_else(|| {
            if self.system.is_some() {
                "[Stoat system message]".to_string()
            } else if self
                .attachments
                .as_ref()
                .is_some_and(|attachments| !attachments.is_empty())
            {
                String::new()
            } else {
                "[Unsupported Stoat message]".to_string()
            }
        })
    }

    fn resolve_author(
        &self,
        bundled_users: &HashMap<String, StoatUser>,
        bundled_members: &HashMap<String, StoatMember>,
        autumn_base_url: Option<&str>,
    ) -> User {
        if let Some(user) = self
            .user
            .clone()
            .or_else(|| bundled_users.get(&self.author).cloned())
        {
            let mut author = user.into_poly_user_with_autumn(autumn_base_url);
            if let Some(member) = self
                .member
                .clone()
                .or_else(|| bundled_members.get(&self.author).cloned())
            {
                if let Some(nickname) = member.nickname {
                    author.display_name = nickname;
                }
                if let Some(avatar_url) = member
                    .avatar
                    .and_then(|avatar| avatar.download_url(autumn_base_url))
                {
                    author.avatar_url = Some(avatar_url);
                }
            }
            return author;
        }

        if let Some(webhook) = &self.webhook {
            return User {
                id: self.author.clone(),
                display_name: webhook.name.clone(),
                avatar_url: webhook
                    .avatar
                    .clone()
                    .and_then(|avatar| avatar.download_url(autumn_base_url)),
                presence: PresenceStatus::Offline,
                backend: BackendType::from(crate::SLUG),
            };
        }

        User {
            id: self.author.clone(),
            display_name: self.author.clone(),
            avatar_url: None,
            presence: PresenceStatus::Offline,
            backend: BackendType::from(crate::SLUG),
        }
    }
}

/// Minimal bundled member payload.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StoatMember {
    /// Composite member key.
    #[serde(rename = "_id")]
    pub key: StoatMemberCompositeKey,
    /// Optional nickname override.
    #[serde(default)]
    pub nickname: Option<String>,
    /// Optional avatar override.
    #[serde(default)]
    pub avatar: Option<StoatFile>,
}

/// Composite member key used in bundled member payloads.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StoatMemberCompositeKey {
    /// Server ID.
    pub server: String,
    /// User ID.
    pub user: String,
}

/// Bundled webhook metadata attached to a message.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StoatMessageWebhook {
    /// Webhook display name.
    pub name: String,
    /// Optional webhook avatar.
    #[serde(default)]
    pub avatar: Option<StoatFile>,
}

/// Search request payload for `POST /channels/{channel_id}/search`.
///
/// Revolt exposes per-channel message search with optional author and a
/// maximum-result cap.  The `query` field is the free-text term.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoatSearchRequest {
    /// Free-text search query.
    pub query: String,
    /// Restrict results to a specific author user ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Maximum number of results (1-100, default 50 on server side).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    /// Sort order — `"Latest"` (default) or `"Oldest"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
}

/// Response shape for `POST /channels/{channel_id}/search`.
///
/// Revolt returns an expanded envelope mirroring the bulk-fetch shape.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StoatSearchResponse {
    /// Matching messages.
    pub messages: Vec<StoatMessage>,
    /// Bundled user objects for rapid avatar/display-name resolution.
    #[serde(default)]
    pub users: Vec<StoatUser>,
}

/// Invite creation response from `POST /channels/{channel_id}/invites`.
///
/// Only the invite code is needed to form a sharable link; remaining fields
/// are informational.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StoatCreateInviteResponse {
    /// Invite type tag (e.g. `"Server"`).
    #[serde(rename = "type")]
    pub kind: Option<String>,
    /// The generated invite code used to build the join URL.
    #[serde(rename = "_id")]
    pub code: String,
    /// Server ID the invite grants access to.
    #[serde(default)]
    pub server: Option<String>,
    /// Channel ID the invite links to.
    #[serde(default)]
    pub channel: Option<String>,
}

impl StoatUser {
    /// Convert the Stoat user model into Poly's backend-agnostic user shape.
    #[must_use]
    pub fn into_poly_user(self) -> User {
        self.into_poly_user_with_autumn(None)
    }

    /// Convert the Stoat user model into Poly's backend-agnostic user shape
    /// with optional Autumn avatar resolution.
    #[must_use]
    pub fn into_poly_user_with_autumn(self, autumn_base_url: Option<&str>) -> User {
        let presence = self
            .status
            .and_then(|status| status.presence)
            .map_or(if self.online {
                PresenceStatus::Online
            } else {
                PresenceStatus::Offline
            }, StoatPresence::into_poly_presence);

        User {
            id: self.id,
            display_name: self.display_name.unwrap_or(self.username),
            avatar_url: self
                .avatar
                .and_then(|avatar| avatar.download_url(autumn_base_url)),
            presence,
            backend: BackendType::from(crate::SLUG),
        }
    }
}

/// Minimal file reference used for avatars, banners, and attachments.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StoatFile {
    /// File ID.
    #[serde(rename = "_id")]
    pub id: String,
    /// Stoat/Autumn storage bucket tag.
    #[serde(default)]
    pub tag: Option<String>,
    /// Original filename.
    #[serde(default)]
    pub filename: Option<String>,
    /// MIME content type.
    #[serde(default)]
    pub content_type: Option<String>,
    /// File size in bytes.
    #[serde(default)]
    pub size: Option<u64>,
}

impl StoatFile {
    /// Best-effort public download URL for a Stoat Autumn file.
    #[must_use]
    pub fn download_url(&self, autumn_base_url: Option<&str>) -> Option<String> {
        let base = autumn_base_url?.trim_end_matches('/');
        let tag = self.tag.as_deref()?;
        Some(format!("{base}/{tag}/{}", self.id))
    }

    /// Convert a Stoat file into Poly's attachment shape.
    #[must_use]
    pub fn into_poly_attachment(self, autumn_base_url: Option<&str>) -> Option<Attachment> {
        let url = self.download_url(autumn_base_url)?;
        let id = self.id.clone();
        Some(Attachment::remote(
            id,
            self.filename?,
            self.content_type?,
            url,
            self.size?,
        ))
    }
}

/// Optional user custom status object.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StoatUserStatus {
    /// Optional rich presence state.
    #[serde(default)]
    pub presence: Option<StoatPresence>,
}

/// Stoat/Revolt presence values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum StoatPresence {
    /// User is online.
    Online,
    /// User is idle.
    Idle,
    /// User is in focus mode.
    Focus,
    /// User is busy.
    Busy,
    /// User appears offline.
    Invisible,
}

impl StoatPresence {
    /// Convert Stoat presence into Poly's shared presence enum.
    #[must_use]
    pub fn into_poly_presence(self) -> PresenceStatus {
        match self {
            Self::Online => PresenceStatus::Online,
            Self::Idle => PresenceStatus::Idle,
            Self::Focus | Self::Busy => PresenceStatus::DoNotDisturb,
            Self::Invisible => PresenceStatus::Invisible,
        }
    }
}

// ── Moderation API types (B-ST) ─────────────────────────────────────────────

/// Payload sent to `PUT /servers/{server_id}/bans/{user_id}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StoatBanCreate {
    /// Optional ban reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Number of seconds of recent messages to bulk-delete (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete_message_seconds: Option<u64>,
}

/// A single ban entry returned in `GET /servers/{server_id}/bans`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StoatBan {
    /// Ban composite key.
    #[serde(rename = "_id")]
    pub id: StoatBanId,
    /// Optional ban reason.
    #[serde(default)]
    pub reason: Option<String>,
}

/// Composite ban key.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StoatBanId {
    /// Server ID.
    pub server: String,
    /// Banned user ID.
    pub user: String,
}

/// The full bans-list response from `GET /servers/{server_id}/bans`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StoatBansResponse {
    /// Ban records.
    pub bans: Vec<StoatBan>,
    /// Matching user records.
    pub users: Vec<StoatUser>,
}

/// `DataMemberEdit` payload for `PATCH /servers/{server_id}/members/{member_id}`.
///
/// Both `timeout` and `remove` are optional; send only one at a time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct StoatMemberEdit {
    /// ISO8601 datetime until which the member is timed out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,
    /// Fields to clear (e.g. `["Timeout"]` to lift an active timeout).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remove: Option<Vec<String>>,
}

/// `DataEditChannel` payload for `PATCH /channels/{channel_id}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct StoatChannelEdit {
    /// New channel name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// New channel description / topic.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Slow-mode interval in seconds (0 = disabled, max 21600).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slowmode: Option<u32>,
    /// NSFW flag.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nsfw: Option<bool>,
}

/// `DataEditGroup` payload for `PATCH /channels/{channel_id}` on a Group channel.
///
/// Used by `edit_group_dm`. The `icon` field in the Revolt protocol requires an
/// Autumn file upload first; that path is left as a future increment (only `name`
/// is supported here).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct StoatGroupEdit {
    /// New group display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Fields to clear (e.g. `["Icon"]` to remove the group icon).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remove: Option<Vec<String>>,
}

/// Server member info returned by `GET /servers/{server_id}/members/@me`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct StoatServerMemberMe {
    /// Composite member key.
    #[serde(rename = "_id")]
    pub key: StoatMemberCompositeKey,
    /// Assigned role IDs.
    #[serde(default)]
    pub roles: Vec<String>,
    /// Timeout expiry (ISO8601) if the member is currently timed out.
    #[serde(default)]
    pub timeout: Option<String>,
}

/// Build a stable reply preview from an already mapped Poly message.
#[must_use]
pub fn reply_preview_from_message(message: &Message) -> MessageReplyPreview {
    MessageReplyPreview {
        message_id: message.id.clone(),
        author_id: message.author.id.clone(),
        author_display_name: message.author.display_name.clone(),
        author_avatar_url: message.author.avatar_url.clone(),
        snippet: match &message.content {
            MessageContent::Text(text) => preview_snippet(text, message.attachments.len()),
            MessageContent::WithAttachments { text, attachments } => {
                preview_snippet(text, attachments.len())
            }
        },
    }
}

fn preview_snippet(text: &str, attachment_count: usize) -> String {
    if !text.trim().is_empty() {
        text.chars().take(80).collect()
    } else if attachment_count > 0 {
        format!(
            "[{} attachment{}]",
            attachment_count,
            if attachment_count == 1 { "" } else { "s" }
        )
    } else {
        "[Message]".to_string()
    }
}

fn stoat_message_timestamp(message_id: &str) -> DateTime<Utc> {
    extract_ulid_timestamp_ms(message_id)
        .and_then(DateTime::<Utc>::from_timestamp_millis)
        .unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
}

fn extract_ulid_timestamp_ms(ulid: &str) -> Option<i64> {
    let mut value = 0_u64;
    for ch in ulid.chars().take(10) {
        value = (value << 5_u32) | u64::from(crockford_base32_value(ch)?);
    }
    i64::try_from(value & 0x0000_FFFF_FFFF_FFFF).ok()
}

fn crockford_base32_value(ch: char) -> Option<u8> {
    match ch.to_ascii_uppercase() {
        '0' => Some(0),
        '1' => Some(1),
        '2' => Some(2),
        '3' => Some(3),
        '4' => Some(4),
        '5' => Some(5),
        '6' => Some(6),
        '7' => Some(7),
        '8' => Some(8),
        '9' => Some(9),
        'A' => Some(10),
        'B' => Some(11),
        'C' => Some(12),
        'D' => Some(13),
        'E' => Some(14),
        'F' => Some(15),
        'G' => Some(16),
        'H' => Some(17),
        'J' => Some(18),
        'K' => Some(19),
        'M' => Some(20),
        'N' => Some(21),
        'P' => Some(22),
        'Q' => Some(23),
        'R' => Some(24),
        'S' => Some(25),
        'T' => Some(26),
        'V' => Some(27),
        'W' => Some(28),
        'X' => Some(29),
        'Y' => Some(30),
        'Z' => Some(31),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        StoatFile, StoatLoginResponse, StoatPresence, StoatRootConfig, StoatRootFeatures,
        StoatServiceFeature, StoatUser, StoatUserStatus, extract_ulid_timestamp_ms,
    };
    use poly_client::{ClientError, PresenceStatus};

    #[test]
    fn stoat_focus_presence_maps_to_dnd() {
        assert_eq!(
            StoatPresence::Focus.into_poly_presence(),
            PresenceStatus::DoNotDisturb
        );
    }

    #[test]
    fn stoat_user_falls_back_to_username_display_name() {
        let user = StoatUser {
            id: "user_1".to_string(),
            username: "stoaty".to_string(),
            discriminator: "0001".to_string(),
            display_name: None,
            avatar: None,
            relations: vec![],
            relationship: None,
            status: Some(StoatUserStatus { presence: None }),
            online: false,
        };

        assert_eq!(user.into_poly_user().display_name, "stoaty".to_string());
    }

    #[test]
    fn stoat_login_mfa_branch_becomes_auth_error() {
        let result = StoatLoginResponse::Mfa {
            ticket: "ticket-1".to_string(),
            allowed_methods: vec!["Password".to_string(), "Totp".to_string()],
        }
        .into_success();

        assert!(matches!(
            result,
            Err(ClientError::AuthFailed(message)) if message.contains("requires MFA")
        ));
    }

    #[test]
    fn stoat_root_config_exposes_autumn_service_url() {
        let config = StoatRootConfig {
            revolt: "0.11.5".to_string(),
            features: StoatRootFeatures {
                autumn: Some(StoatServiceFeature {
                    enabled: true,
                    url: "https://files.example.test".to_string(),
                }),
            },
            ws: "wss://ws.example.test".to_string(),
        };

        assert_eq!(config.autumn_base_url(), Some("https://files.example.test"));
    }

    #[test]
    fn stoat_file_download_url_uses_autumn_base_and_tag() {
        let file = StoatFile {
            id: "file_1".to_string(),
            tag: Some("attachments".to_string()),
            filename: Some("test.png".to_string()),
            content_type: Some("image/png".to_string()),
            size: Some(42),
        };

        assert_eq!(
            file.download_url(Some("https://files.example.test/")),
            Some("https://files.example.test/attachments/file_1".to_string())
        );
    }

    #[test]
    fn stoat_message_ids_decode_ulid_timestamp_prefix() {
        assert_eq!(
            extract_ulid_timestamp_ms("01ARZ3NDEKTSV4RRFFQ69G5FAV"),
            Some(1_469_922_850_259)
        );
    }
}
