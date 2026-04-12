//! Type bridge between WIT-generated types and `poly-client` types.
//!
//! The `wasmtime::component::bindgen!` macro generates its own Rust types
//! from the WIT definitions. These are structurally identical to the
//! `poly-client` types but are distinct Rust types. This module provides
//! zero-cost (or near-zero-cost) conversion between the two.
//!
//! Convention: `from_wit_*` converts WIT→poly-client, `to_wit_*` converts
//! poly-client→WIT.

use super::engine::exports::poly::messenger::plugin_metadata as wit_meta;
use super::engine::poly::messenger::types as wit;
use poly_client::{self as pc};

// ─── BackendType ───────────────────────────────────────────────────

/// Convert WIT `BackendType` → poly-client `BackendType`.
pub fn from_wit_backend_type(bt: wit::BackendType) -> pc::BackendType {
    pc::BackendType::from(match bt {
        wit::BackendType::Stoat => "stoat",
        wit::BackendType::Matrix => "matrix",
        wit::BackendType::Discord => "discord",
        wit::BackendType::Teams => "teams",
        wit::BackendType::Demo => "demo",
        wit::BackendType::Poly => "poly",
    })
}

/// Convert poly-client `BackendType` → WIT `BackendType`.
pub fn to_wit_backend_type(bt: &pc::BackendType) -> wit::BackendType {
    match bt.as_str() {
        "stoat" => wit::BackendType::Stoat,
        "matrix" => wit::BackendType::Matrix,
        "discord" => wit::BackendType::Discord,
        "teams" => wit::BackendType::Teams,
        "demo" => wit::BackendType::Demo,
        _ => wit::BackendType::Poly,
    }
}

// ─── PresenceStatus ────────────────────────────────────────────────

/// Convert WIT `PresenceStatus` → poly-client `PresenceStatus`.
pub fn from_wit_presence(ps: wit::PresenceStatus) -> pc::PresenceStatus {
    match ps {
        wit::PresenceStatus::Online => pc::PresenceStatus::Online,
        wit::PresenceStatus::Idle => pc::PresenceStatus::Idle,
        wit::PresenceStatus::DoNotDisturb => pc::PresenceStatus::DoNotDisturb,
        wit::PresenceStatus::Invisible => pc::PresenceStatus::Invisible,
        wit::PresenceStatus::Offline => pc::PresenceStatus::Offline,
    }
}

/// Convert poly-client `PresenceStatus` → WIT `PresenceStatus`.
pub fn to_wit_presence(ps: pc::PresenceStatus) -> wit::PresenceStatus {
    match ps {
        pc::PresenceStatus::Online => wit::PresenceStatus::Online,
        pc::PresenceStatus::Idle => wit::PresenceStatus::Idle,
        pc::PresenceStatus::DoNotDisturb => wit::PresenceStatus::DoNotDisturb,
        pc::PresenceStatus::Invisible => wit::PresenceStatus::Invisible,
        pc::PresenceStatus::Offline => wit::PresenceStatus::Offline,
    }
}

// ─── ChannelType ───────────────────────────────────────────────────

/// Convert WIT `ChannelType` → poly-client `ChannelType`.
pub fn from_wit_channel_type(ct: wit::ChannelType) -> pc::ChannelType {
    match ct {
        wit::ChannelType::Text => pc::ChannelType::Text,
        wit::ChannelType::Voice => pc::ChannelType::Voice,
        wit::ChannelType::Video => pc::ChannelType::Video,
        wit::ChannelType::Forum => pc::ChannelType::Forum,
        wit::ChannelType::HackerNews => pc::ChannelType::HackerNews,
        wit::ChannelType::Code => pc::ChannelType::Code,
    }
}

// ─── File / code-explorer types ────────────────────────────────────

/// Convert WIT `FileKind` → poly-client `FileKind`.
pub fn from_wit_file_kind(k: wit::FileKind) -> pc::FileKind {
    match k {
        wit::FileKind::File => pc::FileKind::File,
        wit::FileKind::Directory => pc::FileKind::Directory,
        wit::FileKind::Symlink => pc::FileKind::Symlink,
        wit::FileKind::Submodule => pc::FileKind::Submodule,
    }
}

/// Convert WIT `FileEntry` → poly-client `FileEntry`.
pub fn from_wit_file_entry(e: wit::FileEntry) -> pc::FileEntry {
    pc::FileEntry {
        path: e.path,
        name: e.name,
        kind: from_wit_file_kind(e.kind),
        size: e.size,
    }
}

/// Convert WIT `FileContent` → poly-client `FileContent`.
pub fn from_wit_file_content(c: wit::FileContent) -> pc::FileContent {
    pc::FileContent {
        path: c.path,
        bytes: c.bytes,
        truncated: c.truncated,
    }
}

// ─── BackendCapabilities ───────────────────────────────────────────

/// Convert WIT `BackendCapabilities` → poly-client `BackendCapabilities`.
///
/// The WIT interface still uses the legacy flat-bool shape. We project
/// those bools onto the richer enum-based Rust shape conservatively:
/// unknown axes (friends, notifications) default to `None`.
pub fn from_wit_backend_capabilities(c: wit::BackendCapabilities) -> pc::BackendCapabilities {
    pc::BackendCapabilities {
        messaging: if c.supports_send_messages {
            pc::MessagingModel::Full
        } else {
            pc::MessagingModel::ReadOnly
        },
        dms: if c.supports_dms {
            pc::DmSupport::User
        } else {
            pc::DmSupport::None
        },
        friends: pc::FriendModel::None,
        notifications: pc::NotificationSupport::None,
        voice: if c.supports_voice {
            pc::VoiceSupport::Full
        } else {
            pc::VoiceSupport::None
        },
        presence: c.supports_presence,
        typing_indicators: c.supports_typing_indicators,
        reactions: c.supports_reactions,
        search_messages: c.supports_search,
        attachments: c.supports_file_upload,
        create_server: c.supports_groups,
        create_channel: c.supports_groups,
        landing: match c.landing {
            wit::LandingPage::DirectMessages => pc::LandingPage::DirectMessages,
            wit::LandingPage::FirstServer => pc::LandingPage::FirstServer,
            wit::LandingPage::ServerOverview => pc::LandingPage::ServerOverview,
        },
    }
}

// ─── PluginManifest ────────────────────────────────────────────────

/// Convert WIT `PluginManifest` → poly-client `PluginManifest`.
pub fn from_wit_plugin_manifest(m: wit_meta::PluginManifest) -> pc::PluginManifest {
    pc::PluginManifest {
        exec_programs: m.exec_programs,
        http_hosts: m.http_hosts,
        description: m.description,
        homepage: m.homepage,
    }
}

// ─── User ──────────────────────────────────────────────────────────

/// Convert WIT `User` → poly-client `User`.
pub fn from_wit_user(u: wit::User) -> pc::User {
    pc::User {
        id: u.id,
        display_name: u.display_name,
        avatar_url: u.avatar_url,
        presence: from_wit_presence(u.presence),
        backend: from_wit_backend_type(u.backend),
    }
}

/// Convert poly-client `User` → WIT `User`.
pub fn to_wit_user(u: &pc::User) -> wit::User {
    wit::User {
        id: u.id.clone(),
        display_name: u.display_name.clone(),
        avatar_url: u.avatar_url.clone(),
        presence: to_wit_presence(u.presence),
        backend: to_wit_backend_type(&u.backend),
    }
}

// ─── Category ──────────────────────────────────────────────────────

/// Convert WIT `Category` → poly-client `Category`.
pub fn from_wit_category(c: wit::Category) -> pc::Category {
    pc::Category {
        id: c.id,
        name: c.name,
        channel_ids: c.channel_ids,
    }
}

// ─── Server ────────────────────────────────────────────────────────

/// Convert WIT `Server` → poly-client `Server`.
pub fn from_wit_server(s: wit::Server) -> pc::Server {
    pc::Server {
        id: s.id,
        name: s.name,
        icon_url: s.icon_url,
        banner_url: s.banner_url,
        categories: s.categories.into_iter().map(from_wit_category).collect(),
        backend: from_wit_backend_type(s.backend),
        unread_count: s.unread_count,
        mention_count: s.mention_count,
        account_id: s.account_id,
        account_display_name: s.account_display_name,
    }
}

// ─── Channel ───────────────────────────────────────────────────────

/// Convert WIT `Channel` → poly-client `Channel`.
pub fn from_wit_channel(c: wit::Channel) -> pc::Channel {
    pc::Channel {
        id: c.id,
        name: c.name,
        channel_type: from_wit_channel_type(c.channel_type),
        server_id: c.server_id,
        unread_count: c.unread_count,
        mention_count: c.mention_count,
        last_message_id: c.last_message_id,
    }
}

// ─── Attachment ────────────────────────────────────────────────────

/// Convert WIT `Attachment` → poly-client `Attachment`.
pub fn from_wit_attachment(a: wit::Attachment) -> pc::Attachment {
    pc::Attachment::remote(a.id, a.filename, a.content_type, a.url, a.size)
}

/// Convert WIT `MessageReplyPreview` → poly-client `MessageReplyPreview`.
pub fn from_wit_message_reply_preview(r: wit::MessageReplyPreview) -> pc::MessageReplyPreview {
    pc::MessageReplyPreview {
        message_id: r.message_id,
        author_id: r.author_id,
        author_display_name: r.author_display_name,
        author_avatar_url: r.author_avatar_url,
        snippet: r.snippet,
    }
}

/// Convert WIT `CustomEmoji` → poly-client `CustomEmoji`.
pub fn from_wit_custom_emoji(e: wit::CustomEmoji) -> pc::CustomEmoji {
    pc::CustomEmoji {
        id: e.id,
        shortcode: e.shortcode,
        image_url: e.image_url,
        unicode_fallback: e.unicode_fallback,
        animated: e.animated,
        server_id: e.server_id,
        source_name: e.source_name,
    }
}

/// Convert WIT `StickerItem` → poly-client `StickerItem`.
pub fn from_wit_sticker_item(s: wit::StickerItem) -> pc::StickerItem {
    pc::StickerItem {
        id: s.id,
        name: s.name,
        image_url: s.image_url,
        pack_name: s.pack_name,
        description: s.description,
        server_id: s.server_id,
        source_name: s.source_name,
        format: s.format,
    }
}

// ─── Reaction ──────────────────────────────────────────────────────

/// Convert WIT `Reaction` → poly-client `Reaction`.
pub fn from_wit_reaction(r: wit::Reaction) -> pc::Reaction {
    pc::Reaction {
        emoji: r.emoji,
        count: r.count,
        me: r.me,
    }
}

// ─── MessageContent ────────────────────────────────────────────────

/// Convert WIT `MessageContent` → poly-client `MessageContent`.
pub fn from_wit_message_content(mc: wit::MessageContent) -> pc::MessageContent {
    match mc {
        wit::MessageContent::Text(text) => pc::MessageContent::Text(text),
        wit::MessageContent::WithAttachments(ta) => pc::MessageContent::WithAttachments {
            text: ta.text,
            attachments: ta
                .attachments
                .into_iter()
                .map(from_wit_attachment)
                .collect(),
        },
    }
}

/// Convert poly-client `MessageContent` → WIT `MessageContent`.
pub fn to_wit_message_content(mc: pc::MessageContent) -> wit::MessageContent {
    match mc {
        pc::MessageContent::Text(text) => wit::MessageContent::Text(text),
        pc::MessageContent::WithAttachments { text, attachments } => {
            wit::MessageContent::WithAttachments(wit::TextWithAttachments {
                text,
                attachments: attachments
                    .into_iter()
                    .map(|a| wit::Attachment {
                        id: a.id,
                        filename: a.filename,
                        content_type: a.content_type,
                        url: a.url,
                        size: a.size,
                    })
                    .collect(),
            })
        }
    }
}

// ─── Message ───────────────────────────────────────────────────────

/// Convert WIT `Message` → poly-client `Message`.
pub fn from_wit_message(m: wit::Message) -> pc::Message {
    pc::Message {
        id: m.id,
        author: from_wit_user(m.author),
        content: from_wit_message_content(m.content),
        timestamp: chrono::DateTime::parse_from_rfc3339(&m.timestamp)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
        attachments: m.attachments.into_iter().map(from_wit_attachment).collect(),
        reactions: m.reactions.into_iter().map(from_wit_reaction).collect(),
        reply_to: m.reply_to.map(from_wit_message_reply_preview),
        edited: m.edited,
    }
}

// ─── Session ───────────────────────────────────────────────────────

/// Convert WIT `Session` → poly-client `Session`.
pub fn from_wit_session(s: wit::Session) -> pc::Session {
    pc::Session {
        id: s.id,
        user: from_wit_user(s.user),
        token: s.token,
        backend: from_wit_backend_type(s.backend),
        icon_emoji: s.icon_emoji,
        instance_id: s.instance_id,
        backend_url: s.backend_url,
    }
}

// ─── AuthCredentials ───────────────────────────────────────────────

/// Convert poly-client `AuthCredentials` → WIT `AuthCredentials`.
pub fn to_wit_auth_credentials(creds: pc::AuthCredentials) -> wit::AuthCredentials {
    match creds {
        pc::AuthCredentials::Token(token) => wit::AuthCredentials::Token(token),
        pc::AuthCredentials::EmailPassword { email, password } => {
            wit::AuthCredentials::EmailPassword(wit::EmailPasswordCreds { email, password })
        }
        pc::AuthCredentials::OAuth { token } => wit::AuthCredentials::Oauth(token),
        pc::AuthCredentials::DeviceCode { code } => wit::AuthCredentials::DeviceCode(code),
        pc::AuthCredentials::PolyServer {
            server_url,
            private_key_bytes,
            username,
            email,
            display_name,
            selected_user_id,
            is_signup,
        } => wit::AuthCredentials::PolyServer(wit::PolyServerCreds {
            server_url,
            private_key_bytes,
            username,
            email,
            display_name,
            selected_user_id,
            is_signup,
        }),
    }
}

// ─── MessageQuery ──────────────────────────────────────────────────

/// Convert poly-client `MessageQuery` → WIT `MessageQuery`.
pub fn to_wit_message_query(q: pc::MessageQuery) -> wit::MessageQuery {
    wit::MessageQuery {
        before: q.before,
        after: q.after,
        around: q.around,
        limit: q.limit,
    }
}

/// Convert poly-client `MessageSearchQuery` → WIT `MessageSearchQuery`.
pub fn to_wit_message_search_query(q: pc::MessageSearchQuery) -> wit::MessageSearchQuery {
    wit::MessageSearchQuery {
        text: q.text,
        channel_id: q.channel_id,
        server_id: q.server_id,
        author_id: q.author_id,
        has_link: q.has_link,
        mentions_user_id: q.mentions_user_id,
        limit: q.limit,
    }
}

/// Convert WIT `MessageSearchHit` → poly-client `MessageSearchHit`.
pub fn from_wit_message_search_hit(hit: wit::MessageSearchHit) -> pc::MessageSearchHit {
    pc::MessageSearchHit {
        channel_id: hit.channel_id,
        channel_name: hit.channel_name,
        server_id: hit.server_id,
        message: from_wit_message(hit.message),
    }
}

// ─── ClientError ───────────────────────────────────────────────────

/// Convert WIT `ClientError` → poly-client `ClientError`.
pub fn from_wit_client_error(e: wit::ClientError) -> pc::ClientError {
    match e {
        wit::ClientError::AuthFailed(msg) => pc::ClientError::AuthFailed(msg),
        wit::ClientError::Network(msg) => pc::ClientError::Network(msg),
        wit::ClientError::NotFound(msg) => pc::ClientError::NotFound(msg),
        wit::ClientError::RateLimited(ms) => pc::ClientError::RateLimited { retry_after_ms: ms },
        wit::ClientError::PermissionDenied(msg) => pc::ClientError::PermissionDenied(msg),
        wit::ClientError::Internal(msg) => pc::ClientError::Internal(msg),
        wit::ClientError::NotSupported(msg) => pc::ClientError::NotSupported(msg),
    }
}

// ─── Group ─────────────────────────────────────────────────────────

/// Convert WIT `Group` → poly-client `Group`.
pub fn from_wit_group(g: wit::Group) -> pc::Group {
    pc::Group {
        id: g.id,
        members: g.members.into_iter().map(from_wit_user).collect(),
        name: g.name,
        last_message: g.last_message.map(from_wit_message),
        backend: from_wit_backend_type(g.backend),
        account_id: g.account_id,
    }
}

// ─── DmChannel ─────────────────────────────────────────────────────

/// Convert WIT `DmChannel` → poly-client `DmChannel`.
pub fn from_wit_dm_channel(dm: wit::DmChannel) -> pc::DmChannel {
    pc::DmChannel {
        id: dm.id,
        user: from_wit_user(dm.user),
        last_message: dm.last_message.map(from_wit_message),
        unread_count: dm.unread_count,
        backend: from_wit_backend_type(dm.backend),
        account_id: dm.account_id,
    }
}

// ─── Notification ──────────────────────────────────────────────────

/// Convert WIT `NotificationKind` → poly-client `NotificationKind`.
pub fn from_wit_notification_kind(nk: wit::NotificationKind) -> pc::NotificationKind {
    match nk {
        wit::NotificationKind::Mention(info) => pc::NotificationKind::Mention {
            channel_id: info.channel_id,
            message_id: info.message_id,
        },
        wit::NotificationKind::FriendRequest(user_id) => pc::NotificationKind::FriendRequest {
            from_user_id: user_id,
        },
        wit::NotificationKind::ServerInvite(server_id) => {
            pc::NotificationKind::ServerInvite { server_id }
        }
        wit::NotificationKind::VoiceChannelInvite(info) => {
            pc::NotificationKind::VoiceChannelInvite {
                server_id: info.server_id,
                channel_id: info.channel_id,
                channel_name: info.channel_name,
                inviter_user_id: info.inviter_user_id,
            }
        }
        wit::NotificationKind::Other(desc) => pc::NotificationKind::Other(desc),
    }
}

/// Convert WIT `Notification` → poly-client `Notification`.
pub fn from_wit_notification(n: wit::Notification) -> pc::Notification {
    pc::Notification {
        id: n.id,
        kind: from_wit_notification_kind(n.kind),
        backend: from_wit_backend_type(n.backend),
        account_id: n.account_id,
        timestamp: chrono::DateTime::parse_from_rfc3339(&n.timestamp)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
        read: n.read,
        preview: n.preview,
    }
}

// ─── VoiceParticipant ──────────────────────────────────────────────

/// Convert WIT `VoiceParticipant` → poly-client `VoiceParticipant`.
pub fn from_wit_voice_participant(vp: wit::VoiceParticipant) -> pc::VoiceParticipant {
    pc::VoiceParticipant {
        user: from_wit_user(vp.user),
        is_muted: vp.is_muted,
        is_deafened: vp.is_deafened,
        is_streaming: vp.is_streaming,
        is_video_on: vp.is_video_on,
        is_speaking: vp.is_speaking,
    }
}

// ─── ClientEvent ───────────────────────────────────────────────────

/// Convert WIT `ClientEvent` → poly-client `ClientEvent`.
pub fn from_wit_client_event(ev: wit::ClientEvent) -> pc::ClientEvent {
    match ev {
        wit::ClientEvent::MessageReceived(e) => pc::ClientEvent::MessageReceived {
            channel_id: e.channel_id,
            message: from_wit_message(e.message),
        },
        wit::ClientEvent::MessageEdited(e) => pc::ClientEvent::MessageEdited {
            channel_id: e.channel_id,
            message: from_wit_message(e.message),
        },
        wit::ClientEvent::MessageDeleted(e) => pc::ClientEvent::MessageDeleted {
            channel_id: e.channel_id,
            message_id: e.message_id,
        },
        wit::ClientEvent::PresenceChanged(e) => pc::ClientEvent::PresenceChanged {
            user_id: e.user_id,
            status: from_wit_presence(e.status),
        },
        wit::ClientEvent::NotificationReceived(n) => {
            pc::ClientEvent::NotificationReceived(from_wit_notification(n))
        }
        wit::ClientEvent::TypingStarted(e) => pc::ClientEvent::TypingStarted {
            channel_id: e.channel_id,
            user_id: e.user_id,
            timestamp: chrono::DateTime::parse_from_rfc3339(&e.timestamp)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|_| chrono::Utc::now()),
        },
        wit::ClientEvent::ChannelUpdated(c) => pc::ClientEvent::ChannelUpdated(from_wit_channel(c)),
        wit::ClientEvent::ServerUpdated(s) => pc::ClientEvent::ServerUpdated(from_wit_server(s)),
        wit::ClientEvent::FriendRequestReceived(u) => pc::ClientEvent::FriendRequestReceived {
            from_user: from_wit_user(u),
        },
        wit::ClientEvent::ConnectionStateChanged(e) => pc::ClientEvent::ConnectionStateChanged {
            backend: from_wit_backend_type(e.backend),
            connected: e.connected,
        },
        wit::ClientEvent::VoiceUserJoined(e) => pc::ClientEvent::VoiceUserJoined {
            channel_id: e.channel_id,
            participant: from_wit_voice_participant(e.participant),
        },
        wit::ClientEvent::VoiceUserLeft(e) => pc::ClientEvent::VoiceUserLeft {
            channel_id: e.channel_id,
            user_id: e.user_id,
        },
        wit::ClientEvent::VoiceStateUpdated(e) => pc::ClientEvent::VoiceStateUpdated {
            channel_id: e.channel_id,
            participant: from_wit_voice_participant(e.participant),
        },
    }
}
