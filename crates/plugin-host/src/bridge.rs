//! Type bridge between WIT-generated types and `poly-client` types.
//!
//! The `wasmtime::component::bindgen!` macro generates its own Rust types
//! from the WIT definitions. These are structurally identical to the
//! `poly-client` types but are distinct Rust types. This module provides
//! zero-cost (or near-zero-cost) conversion between the two.
//!
//! Convention: `from_wit_*` converts WIT→poly-client, `to_wit_*` converts
//! poly-client→WIT.

use super::engine::exports::poly::messenger::client_composer as wit_composer;
use super::engine::exports::poly::messenger::client_menus as wit_menus;
use super::engine::exports::poly::messenger::client_settings as wit_settings;
use super::engine::exports::poly::messenger::client_sidebar as wit_sidebar;
use super::engine::exports::poly::messenger::client_views as wit_views;
use super::engine::exports::poly::messenger::plugin_metadata as wit_meta;
use super::engine::exports::poly::messenger::client_ui_common as wit_ui_common;
use super::engine::poly::messenger::types as wit;
use poly_client::{self as pc};

// ─── BackendType ───────────────────────────────────────────────────
//
// D17 — the WIT `backend-type` enum has been removed; backend fields
// are now plain `string` (slug). Callers convert with
// `pc::BackendType::from_slug(&s)`.

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
        wit::ChannelType::Thread => pc::ChannelType::Thread,
        wit::ChannelType::Announcement => pc::ChannelType::Announcement,
    }
}

/// Convert WIT `ForumTag` → poly-client `ForumTag`.
pub fn from_wit_forum_tag(t: wit::ForumTag) -> pc::ForumTag {
    pc::ForumTag {
        id: t.id,
        name: t.name,
        emoji: t.emoji,
        moderated: t.moderated,
    }
}

/// Convert WIT `ThreadMetadata` → poly-client `ThreadMetadata`.
pub fn from_wit_thread_metadata(m: wit::ThreadMetadata) -> pc::ThreadMetadata {
    pc::ThreadMetadata {
        archived: m.archived,
        auto_archive_minutes: m.auto_archive_minutes,
        archived_at: m.archived_at.and_then(|s| {
            chrono::DateTime::parse_from_rfc3339(&s)
                .ok()
                .map(|dt| dt.with_timezone(&chrono::Utc))
        }),
        locked: m.locked,
        created_at: chrono::DateTime::parse_from_rfc3339(&m.created_at)
            .map(|dt| dt.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now()),
    }
}

/// Convert WIT `ThreadInfo` → poly-client `ThreadInfo`.
pub fn from_wit_thread_info(t: wit::ThreadInfo) -> pc::ThreadInfo {
    pc::ThreadInfo {
        thread_id: t.thread_id,
        parent_channel_id: t.parent_channel_id,
        message_count: t.message_count,
        member_count: t.member_count,
    }
}

/// Convert WIT `ForumPost` → poly-client `ForumPost`.
pub fn from_wit_forum_post(p: wit::ForumPost) -> pc::ForumPost {
    pc::ForumPost {
        thread: from_wit_thread_info(p.thread),
        applied_tags: p.applied_tags,
        starter_message_id: p.starter_message_id,
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
        backend: pc::BackendType::from_slug(&u.backend),
    }
}

/// Convert poly-client `User` → WIT `User`.
pub fn to_wit_user(u: &pc::User) -> wit::User {
    wit::User {
        id: u.id.clone(),
        display_name: u.display_name.clone(),
        avatar_url: u.avatar_url.clone(),
        presence: to_wit_presence(u.presence),
        backend: u.backend.as_str().to_string(),
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
        backend: pc::BackendType::from_slug(&s.backend),
        unread_count: s.unread_count,
        mention_count: s.mention_count,
        account_id: s.account_id,
        account_display_name: s.account_display_name,
        default_channel_id: s.default_channel_id,
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
        forum_tags: c
            .forum_tags
            .map(|tags| tags.into_iter().map(from_wit_forum_tag).collect()),
        parent_channel_id: c.parent_channel_id,
        thread_metadata: c.thread_metadata.map(from_wit_thread_metadata),
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
        thread: m.thread.map(from_wit_thread_info),
    }
}

// ─── Session ───────────────────────────────────────────────────────

/// Convert WIT `Session` → poly-client `Session`.
pub fn from_wit_session(s: wit::Session) -> pc::Session {
    pc::Session {
        id: s.id,
        user: from_wit_user(s.user),
        token: s.token,
        backend: pc::BackendType::from_slug(&s.backend),
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
        backend: pc::BackendType::from_slug(&g.backend),
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
        backend: pc::BackendType::from_slug(&dm.backend),
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
        wit::NotificationKind::ReauthRequired(backend_slug) => {
            pc::NotificationKind::ReauthRequired { backend_slug }
        }
        wit::NotificationKind::Other(desc) => pc::NotificationKind::Other(desc),
    }
}

/// Convert WIT `Notification` → poly-client `Notification`.
pub fn from_wit_notification(n: wit::Notification) -> pc::Notification {
    pc::Notification {
        id: n.id,
        kind: from_wit_notification_kind(n.kind),
        backend: pc::BackendType::from_slug(&n.backend),
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
            backend: pc::BackendType::from_slug(&e.backend),
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
        // D19 — forwarded to the host so it re-fetches the sidebar
        // declaration via `ClientBackend::get_sidebar_declaration`.
        wit::ClientEvent::SidebarInvalidated => pc::ClientEvent::SidebarInvalidated,
    }
}

// ─── Client UI surface (WP 1.C) ────────────────────────────────────
//
// Each pair of helpers bridges one WIT type in
// `exports::poly::messenger::{client_menus, client_settings,
// client_sidebar, client_views, client_composer}` to the corresponding
// poly-client type in `clients/client/src/ui_surface.rs`.

// ─── Common (custom-block / icon-source / cursor) ──────────────────

pub fn from_wit_custom_block(b: wit_ui_common::CustomBlock) -> pc::CustomBlock {
    pc::CustomBlock {
        sanitized_html: b.sanitized_html,
        stylesheet: b.stylesheet,
        max_height_px: b.max_height_px,
    }
}

pub fn from_wit_icon_source(i: wit_ui_common::IconSource) -> pc::IconSource {
    match i {
        wit_ui_common::IconSource::Emoji(s) => pc::IconSource::Emoji(s),
        wit_ui_common::IconSource::Svg(s) => pc::IconSource::Svg(s),
    }
}

pub fn from_wit_cursor_kind(k: wit_ui_common::CursorKind) -> pc::CursorKind {
    match k {
        wit_ui_common::CursorKind::Offset => pc::CursorKind::Offset,
        wit_ui_common::CursorKind::Timestamp => pc::CursorKind::Timestamp,
        wit_ui_common::CursorKind::Id => pc::CursorKind::Id,
        wit_ui_common::CursorKind::Opaque => pc::CursorKind::Opaque,
    }
}

pub fn to_wit_cursor_kind(k: pc::CursorKind) -> wit_ui_common::CursorKind {
    match k {
        pc::CursorKind::Offset => wit_ui_common::CursorKind::Offset,
        pc::CursorKind::Timestamp => wit_ui_common::CursorKind::Timestamp,
        pc::CursorKind::Id => wit_ui_common::CursorKind::Id,
        pc::CursorKind::Opaque => wit_ui_common::CursorKind::Opaque,
    }
}

pub fn from_wit_cursor(c: wit_ui_common::Cursor) -> pc::Cursor {
    pc::Cursor {
        kind: from_wit_cursor_kind(c.kind),
        value: c.value,
    }
}

pub fn to_wit_cursor(c: pc::Cursor) -> wit_ui_common::Cursor {
    wit_ui_common::Cursor {
        kind: to_wit_cursor_kind(c.kind),
        value: c.value,
    }
}

// ─── Menus ─────────────────────────────────────────────────────────

pub fn from_wit_menu_target_kind(t: wit_menus::MenuTargetKind) -> pc::MenuTargetKind {
    match t {
        wit_menus::MenuTargetKind::Category => pc::MenuTargetKind::Category,
        wit_menus::MenuTargetKind::Channel => pc::MenuTargetKind::Channel,
        wit_menus::MenuTargetKind::Dm => pc::MenuTargetKind::Dm,
        wit_menus::MenuTargetKind::Message => pc::MenuTargetKind::Message,
        wit_menus::MenuTargetKind::Server => pc::MenuTargetKind::Server,
        wit_menus::MenuTargetKind::User => pc::MenuTargetKind::User,
    }
}

pub fn to_wit_menu_target_kind(t: pc::MenuTargetKind) -> wit_menus::MenuTargetKind {
    match t {
        pc::MenuTargetKind::Category => wit_menus::MenuTargetKind::Category,
        pc::MenuTargetKind::Channel => wit_menus::MenuTargetKind::Channel,
        pc::MenuTargetKind::Dm => wit_menus::MenuTargetKind::Dm,
        pc::MenuTargetKind::Message => wit_menus::MenuTargetKind::Message,
        pc::MenuTargetKind::Server => wit_menus::MenuTargetKind::Server,
        pc::MenuTargetKind::User => wit_menus::MenuTargetKind::User,
    }
}

pub fn from_wit_menu_slot(s: wit_menus::MenuSlot) -> pc::MenuSlot {
    match s {
        wit_menus::MenuSlot::Top => pc::MenuSlot::Top,
        wit_menus::MenuSlot::AfterFavorites => pc::MenuSlot::AfterFavorites,
        wit_menus::MenuSlot::BeforeLeave => pc::MenuSlot::BeforeLeave,
        wit_menus::MenuSlot::Bottom => pc::MenuSlot::Bottom,
    }
}

pub fn from_wit_menu_item_variant(v: wit_menus::MenuItemVariant) -> pc::MenuItemVariant {
    match v {
        wit_menus::MenuItemVariant::Normal => pc::MenuItemVariant::Normal,
        wit_menus::MenuItemVariant::Destructive => pc::MenuItemVariant::Destructive,
        wit_menus::MenuItemVariant::SubmenuHeader => pc::MenuItemVariant::SubmenuHeader,
        wit_menus::MenuItemVariant::InfoBlock => pc::MenuItemVariant::InfoBlock,
    }
}

// `icon-source` / `custom-block` are declared in `client-ui-common`
// but `client-menus` re-uses them via `use client-ui-common.{…}`.
// wasmtime's bindgen re-exports those types under the client-menus
// module, so the conversion helpers above (from_wit_icon_source,
// from_wit_custom_block) accept the shared WIT types directly.
//
// Each interface that uses `custom-block` / `icon-source` gets its own
// local alias for the type (they're the same at the component-model
// level). We normalise by converting through `wit_ui_common`.

pub fn from_wit_menu_item(item: wit_menus::MenuItem) -> pc::MenuItem {
    pc::MenuItem {
        id: item.id,
        parent_id: item.parent_id,
        slot: from_wit_menu_slot(item.slot),
        label_key: item.label_key,
        icon: item.icon.map(|i| match i {
            wit_menus::IconSource::Emoji(s) => pc::IconSource::Emoji(s),
            wit_menus::IconSource::Svg(s) => pc::IconSource::Svg(s),
        }),
        item_variant: from_wit_menu_item_variant(item.item_variant),
        shortcut: item.shortcut,
        block: item.block.map(|b| pc::CustomBlock {
            sanitized_html: b.sanitized_html,
            stylesheet: b.stylesheet,
            max_height_px: b.max_height_px,
        }),
    }
}

pub fn from_wit_toast_tone(t: wit_menus::ToastTone) -> pc::ToastTone {
    match t {
        wit_menus::ToastTone::Info => pc::ToastTone::Info,
        wit_menus::ToastTone::Success => pc::ToastTone::Success,
        wit_menus::ToastTone::Warning => pc::ToastTone::Warning,
        wit_menus::ToastTone::Error => pc::ToastTone::Error,
    }
}

pub fn from_wit_toast_payload(p: wit_menus::ToastPayload) -> pc::ToastPayload {
    pc::ToastPayload {
        label_key: p.label_key,
        tone: from_wit_toast_tone(p.tone),
    }
}

pub fn from_wit_settings_anchor(a: wit_menus::SettingsAnchor) -> pc::SettingsAnchor {
    pc::SettingsAnchor {
        scope: a.scope,
        scope_id: a.scope_id,
        section_key: a.section_key,
    }
}

pub fn from_wit_modal_ref(m: wit_menus::ModalRef) -> pc::ModalRef {
    pc::ModalRef {
        modal_id: m.modal_id,
        context: m.context,
    }
}

pub fn from_wit_pending_handle(h: wit_menus::PendingHandle) -> pc::PendingHandle {
    pc::PendingHandle {
        action_ref: h.action_ref,
        progress_hint: h.progress_hint,
    }
}

pub fn to_wit_pending_handle(h: pc::PendingHandle) -> wit_menus::PendingHandle {
    wit_menus::PendingHandle {
        action_ref: h.action_ref,
        progress_hint: h.progress_hint,
    }
}

pub fn from_wit_action_outcome(o: wit_menus::ActionOutcome) -> pc::ActionOutcome {
    match o {
        wit_menus::ActionOutcome::Noop => pc::ActionOutcome::Noop,
        wit_menus::ActionOutcome::Pending(h) => pc::ActionOutcome::Pending(from_wit_pending_handle(h)),
        wit_menus::ActionOutcome::Completed => pc::ActionOutcome::Completed,
        wit_menus::ActionOutcome::RefreshTarget => pc::ActionOutcome::RefreshTarget,
        wit_menus::ActionOutcome::RefreshSidebar => pc::ActionOutcome::RefreshSidebar,
        wit_menus::ActionOutcome::Navigate(s) => pc::ActionOutcome::Navigate(s),
        wit_menus::ActionOutcome::Toast(p) => pc::ActionOutcome::Toast(from_wit_toast_payload(p)),
        wit_menus::ActionOutcome::OpenSettings(a) => pc::ActionOutcome::OpenSettings(from_wit_settings_anchor(a)),
        wit_menus::ActionOutcome::OpenModal(m) => pc::ActionOutcome::OpenModal(from_wit_modal_ref(m)),
    }
}

// ─── Settings ──────────────────────────────────────────────────────

pub fn from_wit_setting_kind(k: wit_settings::SettingKind) -> pc::SettingKind {
    match k {
        wit_settings::SettingKind::Toggle => pc::SettingKind::Toggle,
        wit_settings::SettingKind::TextInput => pc::SettingKind::TextInput,
        wit_settings::SettingKind::Select => pc::SettingKind::Select,
        wit_settings::SettingKind::Slider => pc::SettingKind::Slider,
        wit_settings::SettingKind::InfoLabel => pc::SettingKind::InfoLabel,
    }
}

pub fn from_wit_setting_descriptor(d: wit_settings::SettingDescriptor) -> pc::SettingDescriptor {
    pc::SettingDescriptor {
        key: d.key,
        kind: from_wit_setting_kind(d.kind),
        default_value: d.default_value,
        extra: d.extra,
    }
}

pub fn from_wit_settings_scope(s: wit_settings::SettingsScope) -> pc::SettingsScope {
    match s {
        wit_settings::SettingsScope::AccountGlobal => pc::SettingsScope::AccountGlobal,
        wit_settings::SettingsScope::PerServer => pc::SettingsScope::PerServer,
        wit_settings::SettingsScope::PerChannel => pc::SettingsScope::PerChannel,
        wit_settings::SettingsScope::PerUser => pc::SettingsScope::PerUser,
    }
}

pub fn to_wit_settings_scope(s: pc::SettingsScope) -> wit_settings::SettingsScope {
    match s {
        pc::SettingsScope::AccountGlobal => wit_settings::SettingsScope::AccountGlobal,
        pc::SettingsScope::PerServer => wit_settings::SettingsScope::PerServer,
        pc::SettingsScope::PerChannel => wit_settings::SettingsScope::PerChannel,
        pc::SettingsScope::PerUser => wit_settings::SettingsScope::PerUser,
    }
}

pub fn from_wit_settings_section(s: wit_settings::SettingsSection) -> pc::SettingsSection {
    pc::SettingsSection {
        scope: from_wit_settings_scope(s.scope),
        section_key: s.section_key,
        icon: s.icon,
        fields: s.fields.into_iter().map(from_wit_setting_descriptor).collect(),
        info_block: s.info_block.map(|b| pc::CustomBlock {
            sanitized_html: b.sanitized_html,
            stylesheet: b.stylesheet,
            max_height_px: b.max_height_px,
        }),
    }
}

// ─── Sidebar ───────────────────────────────────────────────────────

pub fn from_wit_sidebar_layout_kind(k: wit_sidebar::SidebarLayoutKind) -> pc::SidebarLayoutKind {
    match k {
        wit_sidebar::SidebarLayoutKind::ChannelList => pc::SidebarLayoutKind::ChannelList,
        wit_sidebar::SidebarLayoutKind::SpacesRooms => pc::SidebarLayoutKind::SpacesRooms,
        wit_sidebar::SidebarLayoutKind::Communities => pc::SidebarLayoutKind::Communities,
        wit_sidebar::SidebarLayoutKind::Feed => pc::SidebarLayoutKind::Feed,
        wit_sidebar::SidebarLayoutKind::RepoTree => pc::SidebarLayoutKind::RepoTree,
        wit_sidebar::SidebarLayoutKind::Custom => pc::SidebarLayoutKind::Custom,
    }
}

pub fn from_wit_sidebar_route_kind(k: wit_sidebar::SidebarRouteKind) -> pc::SidebarRouteKind {
    match k {
        wit_sidebar::SidebarRouteKind::Channel => pc::SidebarRouteKind::Channel,
        wit_sidebar::SidebarRouteKind::Forum => pc::SidebarRouteKind::Forum,
        wit_sidebar::SidebarRouteKind::Feed => pc::SidebarRouteKind::Feed,
        wit_sidebar::SidebarRouteKind::Code => pc::SidebarRouteKind::Code,
        wit_sidebar::SidebarRouteKind::IssueTracker => pc::SidebarRouteKind::IssueTracker,
        wit_sidebar::SidebarRouteKind::Modal => pc::SidebarRouteKind::Modal,
        wit_sidebar::SidebarRouteKind::External => pc::SidebarRouteKind::External,
        wit_sidebar::SidebarRouteKind::CustomView => pc::SidebarRouteKind::CustomView,
    }
}

pub fn from_wit_sidebar_item(it: wit_sidebar::SidebarItem) -> pc::SidebarItem {
    pc::SidebarItem {
        id: it.id,
        parent_id: it.parent_id,
        label_key: it.label_key,
        icon: it.icon.map(|i| match i {
            wit_sidebar::IconSource::Emoji(s) => pc::IconSource::Emoji(s),
            wit_sidebar::IconSource::Svg(s) => pc::IconSource::Svg(s),
        }),
        badge: it.badge,
        route_kind: from_wit_sidebar_route_kind(it.route_kind),
    }
}

pub fn from_wit_sidebar_section(s: wit_sidebar::SidebarSection) -> pc::SidebarSection {
    pc::SidebarSection {
        header_key: s.header_key,
        collapsible: s.collapsible,
        default_collapsed: s.default_collapsed,
        items: s.items.into_iter().map(from_wit_sidebar_item).collect(),
    }
}

pub fn from_wit_sidebar_declaration(d: wit_sidebar::SidebarDeclaration) -> pc::SidebarDeclaration {
    pc::SidebarDeclaration {
        layout: from_wit_sidebar_layout_kind(d.layout),
        sections: d.sections.into_iter().map(from_wit_sidebar_section).collect(),
        header_block: d.header_block.map(|b| pc::CustomBlock {
            sanitized_html: b.sanitized_html,
            stylesheet: b.stylesheet,
            max_height_px: b.max_height_px,
        }),
    }
}

// ─── Views ─────────────────────────────────────────────────────────

pub fn from_wit_view_kind(k: wit_views::ViewKind) -> pc::ViewKind {
    match k {
        wit_views::ViewKind::FlatList => pc::ViewKind::FlatList,
        wit_views::ViewKind::CardGrid => pc::ViewKind::CardGrid,
        wit_views::ViewKind::Tree => pc::ViewKind::Tree,
        wit_views::ViewKind::Split => pc::ViewKind::Split,
    }
}

pub fn from_wit_toolbar_option(o: wit_views::ToolbarOption) -> pc::ToolbarOption {
    pc::ToolbarOption {
        id: o.id,
        label_key: o.label_key,
        icon: o.icon,
        default_selected: o.default_selected,
    }
}

pub fn from_wit_row_template(r: wit_views::RowTemplate) -> pc::RowTemplate {
    pc::RowTemplate {
        primary_field: r.primary_field,
        secondary_field: r.secondary_field,
        meta_field: r.meta_field,
        icon_field: r.icon_field,
    }
}

pub fn from_wit_list_spec(l: wit_views::ListSpec) -> pc::ListSpec {
    pc::ListSpec {
        row_template: from_wit_row_template(l.row_template),
        page_size: l.page_size,
    }
}

pub fn from_wit_card_spec(c: wit_views::CardSpec) -> pc::CardSpec {
    pc::CardSpec { primary_field: c.primary_field }
}

pub fn from_wit_tree_spec(t: wit_views::TreeSpec) -> pc::TreeSpec {
    pc::TreeSpec {
        root_page_size: t.root_page_size,
        max_depth: t.max_depth,
    }
}

pub fn from_wit_split_spec(s: wit_views::SplitSpec) -> pc::SplitSpec {
    pc::SplitSpec {
        list_side: from_wit_list_spec(s.list_side),
        detail_view_kind: from_wit_view_kind(s.detail_view_kind),
    }
}

pub fn from_wit_view_body(b: wit_views::ViewBody) -> pc::ViewBody {
    match b {
        wit_views::ViewBody::ListBody(l) => pc::ViewBody::ListBody(from_wit_list_spec(l)),
        wit_views::ViewBody::CardBody(c) => pc::ViewBody::CardBody(from_wit_card_spec(c)),
        wit_views::ViewBody::TreeBody(t) => pc::ViewBody::TreeBody(from_wit_tree_spec(t)),
        wit_views::ViewBody::SplitBody(s) => pc::ViewBody::SplitBody(from_wit_split_spec(s)),
    }
}

pub fn from_wit_view_header(h: wit_views::ViewHeader) -> pc::ViewHeader {
    pc::ViewHeader {
        title_key: h.title_key,
        subtitle_key: h.subtitle_key,
        info_block: h.info_block.map(|b| pc::CustomBlock {
            sanitized_html: b.sanitized_html,
            stylesheet: b.stylesheet,
            max_height_px: b.max_height_px,
        }),
    }
}

pub fn from_wit_view_toolbar(t: wit_views::ViewToolbar) -> pc::ViewToolbar {
    pc::ViewToolbar {
        sort_options: t.sort_options.into_iter().map(from_wit_toolbar_option).collect(),
        filter_options: t.filter_options.into_iter().map(from_wit_toolbar_option).collect(),
        tabs: t.tabs.into_iter().map(from_wit_toolbar_option).collect(),
        action_items: t
            .action_items
            .into_iter()
            .map(from_wit_view_menu_item)
            .collect(),
    }
}

/// `client-views::view-toolbar.action-items` re-uses `menu-item` from
/// `client-menus` — bindgen keeps the *same* generated struct, so this
/// is just a thin forwarder to [`from_wit_menu_item`].
pub fn from_wit_view_menu_item(item: wit_menus::MenuItem) -> pc::MenuItem {
    from_wit_menu_item(item)
}

pub fn from_wit_view_descriptor(d: wit_views::ViewDescriptor) -> pc::ViewDescriptor {
    pc::ViewDescriptor {
        kind: from_wit_view_kind(d.kind),
        header: d.header.map(from_wit_view_header),
        toolbar: d.toolbar.map(from_wit_view_toolbar),
        body: from_wit_view_body(d.body),
    }
}

/// Forwarder: `client-views` re-uses `menu-target-kind` from
/// `client-menus`. Bindgen preserves identity so this is a pass-through.
pub fn from_wit_view_menu_target_kind(k: wit_menus::MenuTargetKind) -> pc::MenuTargetKind {
    from_wit_menu_target_kind(k)
}

pub fn from_wit_view_row(r: wit_views::ViewRow) -> pc::ViewRow {
    pc::ViewRow {
        id: r.id,
        primary_text: r.primary_text,
        secondary_text: r.secondary_text,
        meta_text: r.meta_text,
        icon: r.icon,
        badge: r.badge,
        context_menu_target_kind: from_wit_view_menu_target_kind(r.context_menu_target_kind),
    }
}

/// `client-views::cursor` is re-used from `client-ui-common`; forward
/// through the shared helpers.
pub fn to_wit_view_cursor(c: pc::Cursor) -> wit_ui_common::Cursor {
    to_wit_cursor(c)
}

pub fn from_wit_view_cursor(c: wit_ui_common::Cursor) -> pc::Cursor {
    from_wit_cursor(c)
}

pub fn from_wit_view_rows_page(p: wit_views::ViewRowsPage) -> pc::ViewRowsPage {
    pc::ViewRowsPage {
        rows: p.rows.into_iter().map(from_wit_view_row).collect(),
        next_cursor: p.next_cursor.map(from_wit_view_cursor),
    }
}

pub fn from_wit_view_detail(d: wit_views::ViewDetail) -> pc::ViewDetail {
    pc::ViewDetail {
        body_block: pc::CustomBlock {
            sanitized_html: d.body_block.sanitized_html,
            stylesheet: d.body_block.stylesheet,
            max_height_px: d.body_block.max_height_px,
        },
        comments_section: d.comments_section.map(|t| pc::TreeSpec {
            root_page_size: t.root_page_size,
            max_depth: t.max_depth,
        }),
    }
}

// ─── Composer ──────────────────────────────────────────────────────

pub fn from_wit_composer_slot(s: wit_composer::ComposerSlot) -> pc::ComposerSlot {
    match s {
        wit_composer::ComposerSlot::LeftOfInput => pc::ComposerSlot::LeftOfInput,
        wit_composer::ComposerSlot::RightOfInput => pc::ComposerSlot::RightOfInput,
        wit_composer::ComposerSlot::AboveInput => pc::ComposerSlot::AboveInput,
    }
}

pub fn from_wit_composer_button(b: wit_composer::ComposerButton) -> pc::ComposerButton {
    pc::ComposerButton {
        id: b.id,
        label_key: b.label_key,
        icon: b.icon,
        position: from_wit_composer_slot(b.position),
    }
}

/// `client-composer::get-message-actions` returns `list<menu-item>` where
/// `menu-item` is re-used from `client-menus` (bindgen preserves identity).
pub fn from_wit_composer_menu_item(item: wit_menus::MenuItem) -> pc::MenuItem {
    from_wit_menu_item(item)
}

/// `client-composer::invoke-*-action` returns the same `action-outcome`
/// type declared in `client-menus`.
pub fn from_wit_composer_action_outcome(o: wit_menus::ActionOutcome) -> pc::ActionOutcome {
    from_wit_action_outcome(o)
}

/// `client-sidebar::invoke-sidebar-action` — same re-use pattern.
pub fn from_wit_sidebar_action_outcome(o: wit_menus::ActionOutcome) -> pc::ActionOutcome {
    from_wit_action_outcome(o)
}

// ─── ForumSortOrder ────────────────────────────────────────────────

/// Convert poly-client `ForumSortOrder` → WIT `ForumSortOrder`.
pub fn to_wit_forum_sort_order(s: pc::ForumSortOrder) -> wit::ForumSortOrder {
    match s {
        pc::ForumSortOrder::LatestActivity => wit::ForumSortOrder::LatestActivity,
        pc::ForumSortOrder::CreationDate => wit::ForumSortOrder::CreationDate,
    }
}
