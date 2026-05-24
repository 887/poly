//! WASM Component Model guest implementation for the demo messenger plugin.
//!
//! This module is only compiled when targeting `wasm32` (gated in `lib.rs`).
//! It implements the `messenger-client` export interface, delegating to the `data`
//! module for all demo content and converting `poly-client` types → WIT types.
//!
//! DECISION(D21): WASM Plugin Backends.

#![allow(unsafe_code)]

use std::cell::RefCell;

use poly_client as pc;

use crate::wit_bindings::{
    ClientComposerGuest, ClientMenusGuest, ClientSettingsGuest, ClientSidebarGuest,
    ClientViewsGuest, MessengerClientGuest, PluginMetadataGuest, export, wit,
};
use crate::wit_bindings::exports::poly::messenger::client_config::{
    Guest as ClientConfigGuest, Mechanism,
};

// ─── State Management ──────────────────────────────────────────────
// WASM components are single-threaded; use thread_local + RefCell.

/// Internal mutable state for the demo plugin.
struct DemoState {
    /// Whether `authenticate()` has been called successfully.
    authenticated: bool,
}

thread_local! {
    static STATE: RefCell<DemoState> = const { RefCell::new(DemoState {
        authenticated: false,
    }) };
}

// ─── Bridge: poly-client → WIT types (for return values) ──────────

fn to_wit_presence(ps: pc::PresenceStatus) -> wit::PresenceStatus {
    match ps {
        pc::PresenceStatus::Online => wit::PresenceStatus::Online,
        pc::PresenceStatus::Idle => wit::PresenceStatus::Idle,
        pc::PresenceStatus::DoNotDisturb => wit::PresenceStatus::DoNotDisturb,
        pc::PresenceStatus::Invisible => wit::PresenceStatus::Invisible,
        pc::PresenceStatus::Offline => wit::PresenceStatus::Offline,
        // WIT wire type has no Unknown variant; map to Offline (same as plugin-host bridge).
        pc::PresenceStatus::Unknown => wit::PresenceStatus::Offline,
    }
}

fn to_wit_channel_type(ct: pc::ChannelType) -> wit::ChannelType {
    match ct {
        pc::ChannelType::Text => wit::ChannelType::Text,
        pc::ChannelType::Voice => wit::ChannelType::Voice,
        pc::ChannelType::Video => wit::ChannelType::Video,
        pc::ChannelType::Forum => wit::ChannelType::Forum,
        pc::ChannelType::HackerNews => wit::ChannelType::HackerNews,
        pc::ChannelType::Code => wit::ChannelType::Code,
        pc::ChannelType::Thread => wit::ChannelType::Thread,
        pc::ChannelType::Announcement => wit::ChannelType::Announcement,
    }
}

fn to_wit_user(u: &pc::User) -> wit::User {
    wit::User {
        id: u.id.clone(),
        display_name: u.display_name.clone(),
        avatar_url: u.avatar_url.clone(),
        presence: to_wit_presence(u.presence),
        backend: u.backend.as_str().to_string(),
    }
}

fn to_wit_category(c: &pc::Category) -> wit::Category {
    wit::Category {
        id: c.id.clone(),
        name: c.name.clone(),
        channel_ids: c.channel_ids.clone(),
    }
}

fn to_wit_server(s: pc::Server) -> wit::Server {
    wit::Server {
        id: s.id,
        name: s.name,
        icon_url: s.icon_url,
        banner_url: s.banner_url,
        categories: s.categories.iter().map(to_wit_category).collect(),
        backend: s.backend.as_str().to_string(),
        unread_count: s.unread_count,
        mention_count: s.mention_count,
        account_id: s.account_id,
        account_display_name: s.account_display_name,
        default_channel_id: s.default_channel_id,
    }
}

fn to_wit_forum_tag(t: pc::ForumTag) -> wit::ForumTag {
    wit::ForumTag {
        id: t.id,
        name: t.name,
        emoji: t.emoji,
        moderated: t.moderated,
    }
}

fn to_wit_thread_info(t: pc::ThreadInfo) -> wit::ThreadInfo {
    wit::ThreadInfo {
        thread_id: t.thread_id,
        parent_channel_id: t.parent_channel_id,
        message_count: t.message_count,
        member_count: t.member_count,
    }
}

fn to_wit_thread_metadata(m: pc::ThreadMetadata) -> wit::ThreadMetadata {
    wit::ThreadMetadata {
        archived: m.archived,
        auto_archive_minutes: m.auto_archive_minutes,
        archived_at: m.archived_at.map(|dt| dt.to_rfc3339()),
        locked: m.locked,
        created_at: m.created_at.to_rfc3339(),
    }
}

fn to_wit_channel(c: pc::Channel) -> wit::Channel {
    wit::Channel {
        id: c.id,
        name: c.name,
        channel_type: to_wit_channel_type(c.channel_type),
        server_id: c.server_id,
        unread_count: c.unread_count,
        mention_count: c.mention_count,
        last_message_id: c.last_message_id,
        forum_tags: c
            .forum_tags
            .map(|tags| tags.into_iter().map(to_wit_forum_tag).collect()),
        parent_channel_id: c.parent_channel_id,
        thread_metadata: c.thread_metadata.map(to_wit_thread_metadata),
    }
}

fn to_wit_attachment(a: &pc::Attachment) -> wit::Attachment {
    wit::Attachment {
        id: a.id.clone(),
        filename: a.filename.clone(),
        content_type: a.content_type.clone(),
        url: a.url.clone(),
        size: a.size,
    }
}

fn to_wit_reaction(r: &pc::Reaction) -> wit::Reaction {
    wit::Reaction {
        emoji: r.emoji.clone(),
        count: r.count,
        me: r.me,
    }
}

fn to_wit_message_reply_preview(r: &pc::MessageReplyPreview) -> wit::MessageReplyPreview {
    wit::MessageReplyPreview {
        message_id: r.message_id.clone(),
        author_id: r.author_id.clone(),
        author_display_name: r.author_display_name.clone(),
        author_avatar_url: r.author_avatar_url.clone(),
        snippet: r.snippet.clone(),
    }
}

fn to_wit_custom_emoji(e: pc::CustomEmoji) -> wit::CustomEmoji {
    wit::CustomEmoji {
        id: e.id,
        shortcode: e.shortcode,
        image_url: e.image_url,
        unicode_fallback: e.unicode_fallback,
        animated: e.animated,
        server_id: e.server_id,
        source_name: e.source_name,
    }
}

fn to_wit_sticker_item(s: pc::StickerItem) -> wit::StickerItem {
    wit::StickerItem {
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

fn to_wit_message_content(mc: pc::MessageContent) -> wit::MessageContent {
    match mc {
        pc::MessageContent::Text(text) => wit::MessageContent::Text(text),
        pc::MessageContent::WithAttachments { text, attachments } => {
            wit::MessageContent::WithAttachments(wit::TextWithAttachments {
                text,
                attachments: attachments.iter().map(to_wit_attachment).collect(),
            })
        }
    }
}

fn to_wit_message(m: pc::Message) -> wit::Message {
    wit::Message {
        id: m.id,
        author: to_wit_user(&m.author),
        content: to_wit_message_content(m.content),
        timestamp: m.timestamp.to_rfc3339(),
        attachments: m.attachments.iter().map(to_wit_attachment).collect(),
        reactions: m.reactions.iter().map(to_wit_reaction).collect(),
        reply_to: m.reply_to.as_ref().map(to_wit_message_reply_preview),
        edited: m.edited,
        thread: m.thread.map(to_wit_thread_info),
    }
}

fn to_wit_message_search_hit(hit: pc::MessageSearchHit) -> wit::MessageSearchHit {
    wit::MessageSearchHit {
        channel_id: hit.channel_id,
        channel_name: hit.channel_name,
        server_id: hit.server_id,
        message: to_wit_message(hit.message),
    }
}

fn to_wit_session(s: pc::Session) -> wit::Session {
    wit::Session {
        id: s.id,
        user: to_wit_user(&s.user),
        token: s.token,
        backend: s.backend.as_str().to_string(),
        icon_emoji: s.icon_emoji,
        instance_id: s.instance_id,
        backend_url: s.backend_url,
    }
}

fn to_wit_group(g: pc::Group) -> wit::Group {
    wit::Group {
        id: g.id,
        members: g.members.iter().map(to_wit_user).collect(),
        name: g.name,
        last_message: g.last_message.map(to_wit_message),
        backend: g.backend.as_str().to_string(),
        account_id: g.account_id,
    }
}

fn to_wit_dm_channel(dm: pc::DmChannel) -> wit::DmChannel {
    wit::DmChannel {
        id: dm.id,
        user: to_wit_user(&dm.user),
        last_message: dm.last_message.map(to_wit_message),
        unread_count: dm.unread_count,
        backend: dm.backend.as_str().to_string(),
        account_id: dm.account_id,
    }
}

fn to_wit_notification_kind(nk: &pc::NotificationKind) -> wit::NotificationKind {
    match nk {
        pc::NotificationKind::Mention {
            channel_id,
            message_id,
        } => wit::NotificationKind::Mention(wit::MentionInfo {
            channel_id: channel_id.clone(),
            message_id: message_id.clone(),
        }),
        pc::NotificationKind::FriendRequest { from_user_id } => {
            wit::NotificationKind::FriendRequest(from_user_id.clone())
        }
        pc::NotificationKind::ServerInvite { server_id } => {
            wit::NotificationKind::ServerInvite(server_id.clone())
        }
        pc::NotificationKind::VoiceChannelInvite {
            server_id,
            channel_id,
            channel_name,
            inviter_user_id,
        } => wit::NotificationKind::VoiceChannelInvite(wit::VoiceInviteInfo {
            server_id: server_id.clone(),
            channel_id: channel_id.clone(),
            channel_name: channel_name.clone(),
            inviter_user_id: inviter_user_id.clone(),
        }),
        pc::NotificationKind::ReauthRequired { backend_slug } => {
            wit::NotificationKind::ReauthRequired(backend_slug.clone())
        }
        pc::NotificationKind::Other(desc) => wit::NotificationKind::Other(desc.clone()),
    }
}

fn to_wit_notification(n: pc::Notification) -> wit::Notification {
    wit::Notification {
        id: n.id,
        kind: to_wit_notification_kind(&n.kind),
        backend: n.backend.as_str().to_string(),
        account_id: n.account_id,
        timestamp: n.timestamp.to_rfc3339(),
        read: n.read,
        preview: n.preview,
    }
}

fn to_wit_voice_participant(vp: pc::VoiceParticipant) -> wit::VoiceParticipant {
    wit::VoiceParticipant {
        user: to_wit_user(&vp.user),
        is_muted: vp.is_muted,
        is_deafened: vp.is_deafened,
        is_streaming: vp.is_streaming,
        is_video_on: vp.is_video_on,
        is_speaking: vp.is_speaking,
    }
}

fn to_wit_client_error(e: pc::ClientError) -> wit::ClientError {
    match e {
        pc::ClientError::AuthFailed(msg) => wit::ClientError::AuthFailed(msg),
        pc::ClientError::Network(msg) => wit::ClientError::Network(msg),
        pc::ClientError::NotFound(msg) => wit::ClientError::NotFound(msg),
        pc::ClientError::RateLimited { retry_after_ms } => {
            wit::ClientError::RateLimited(retry_after_ms)
        }
        pc::ClientError::PermissionDenied(msg) => wit::ClientError::PermissionDenied(msg),
        pc::ClientError::Internal(msg) => wit::ClientError::Internal(msg),
        pc::ClientError::NotSupported(msg) => wit::ClientError::NotSupported(msg),
    }
}

// ─── Bridge: WIT types → poly-client (for input parameters) ───────

fn from_wit_message_content(mc: wit::MessageContent) -> pc::MessageContent {
    match mc {
        wit::MessageContent::Text(text) => pc::MessageContent::Text(text),
        wit::MessageContent::WithAttachments(ta) => pc::MessageContent::WithAttachments {
            text: ta.text,
            attachments: ta
                .attachments
                .into_iter()
                .map(|a| pc::Attachment::remote(a.id, a.filename, a.content_type, a.url, a.size))
                .collect(),
        },
    }
}

fn from_wit_message_query(query: wit::MessageQuery) -> pc::MessageQuery {
    pc::MessageQuery {
        before: query.before,
        after: query.after,
        around: query.around,
        limit: query.limit,
    }
}

fn from_wit_message_search_query(query: wit::MessageSearchQuery) -> pc::MessageSearchQuery {
    pc::MessageSearchQuery {
        text: query.text,
        channel_id: query.channel_id,
        server_id: query.server_id,
        author_id: query.author_id,
        has_link: query.has_link,
        mentions_user_id: query.mentions_user_id,
        limit: query.limit,
    }
}

// ─── Helper: convert ClientResult<T> → Result<WitT, WitError> ────

/// Wrap a `poly_client::ClientResult<T>` into the WIT error type using
/// a conversion closure for the success value.
fn convert_result<T, W>(
    result: pc::ClientResult<T>,
    f: impl FnOnce(T) -> W,
) -> Result<W, wit::ClientError> {
    match result {
        Ok(val) => Ok(f(val)),
        Err(e) => Err(to_wit_client_error(e)),
    }
}

// ─── Guest Trait Implementation ────────────────────────────────────

/// The demo plugin component type.
struct DemoPlugin;

impl MessengerClientGuest for DemoPlugin {
    fn authenticate(_credentials: wit::AuthCredentials) -> Result<wit::Session, wit::ClientError> {
        let session = crate::data::demo_session();
        STATE.with(|s| s.borrow_mut().authenticated = true);
        Ok(to_wit_session(session))
    }

    fn logout() -> Result<(), wit::ClientError> {
        STATE.with(|s| s.borrow_mut().authenticated = false);
        Ok(())
    }

    fn is_authenticated() -> bool {
        STATE.with(|s| s.borrow().authenticated)
    }

    fn get_servers() -> Result<Vec<wit::Server>, wit::ClientError> {
        Ok(crate::data::demo_servers()
            .into_iter()
            .map(to_wit_server)
            .collect())
    }

    fn get_server(id: String) -> Result<wit::Server, wit::ClientError> {
        convert_result(
            crate::data::demo_servers()
                .into_iter()
                .find(|s| s.id == id)
                .ok_or_else(|| pc::ClientError::NotFound(format!("Server {id}"))),
            to_wit_server,
        )
    }

    fn get_channels(server_id: String) -> Result<Vec<wit::Channel>, wit::ClientError> {
        Ok(crate::data::demo_channels(&server_id)
            .into_iter()
            .map(to_wit_channel)
            .collect())
    }

    fn get_channel(id: String) -> Result<wit::Channel, wit::ClientError> {
        for server in crate::data::demo_servers() {
            for channel in crate::data::demo_channels(&server.id) {
                if channel.id == id {
                    return Ok(to_wit_channel(channel));
                }
            }
        }
        Err(to_wit_client_error(pc::ClientError::NotFound(format!(
            "Channel {id}"
        ))))
    }

    fn send_message(
        channel_id: String,
        content: wit::MessageContent,
    ) -> Result<wit::Message, wit::ClientError> {
        let pc_content = from_wit_message_content(content);
        let msg = crate::data::demo_sent_message(&channel_id, pc_content);
        Ok(to_wit_message(msg))
    }

    fn send_reply_message(
        channel_id: String,
        reply_to_message_id: String,
        content: wit::MessageContent,
    ) -> Result<wit::Message, wit::ClientError> {
        let pc_content = from_wit_message_content(content);
        let msg =
            crate::data::demo_sent_reply_message(&channel_id, &reply_to_message_id, pc_content);
        Ok(to_wit_message(msg))
    }

    fn get_messages(
        channel_id: String,
        query: wit::MessageQuery,
    ) -> Result<Vec<wit::Message>, wit::ClientError> {
        let messages =
            crate::data::demo_messages_query(&channel_id, &from_wit_message_query(query));
        Ok(messages.into_iter().map(to_wit_message).collect())
    }

    fn search_messages(
        query: wit::MessageSearchQuery,
    ) -> Result<Vec<wit::MessageSearchHit>, wit::ClientError> {
        let hits = crate::data::demo_search_messages(&from_wit_message_search_query(query));
        Ok(hits.into_iter().map(to_wit_message_search_hit).collect())
    }

    fn get_pinned_messages(channel_id: String) -> Result<Vec<wit::Message>, wit::ClientError> {
        Ok(crate::data::demo_pinned_messages(&channel_id)
            .into_iter()
            .map(to_wit_message)
            .collect())
    }

    fn get_available_emojis(channel_id: String) -> Result<Vec<wit::CustomEmoji>, wit::ClientError> {
        Ok(crate::data::demo_available_emojis(&channel_id)
            .into_iter()
            .map(to_wit_custom_emoji)
            .collect())
    }

    fn get_available_stickers(
        channel_id: String,
    ) -> Result<Vec<wit::StickerItem>, wit::ClientError> {
        Ok(crate::data::demo_available_stickers(&channel_id)
            .into_iter()
            .map(to_wit_sticker_item)
            .collect())
    }

    fn set_message_pinned(
        _channel_id: String,
        _message_id: String,
        _pinned: bool,
    ) -> Result<(), wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "demo pin mutation not yet implemented".to_string(),
        ))
    }

    fn get_user(user_id: String) -> Result<wit::User, wit::ClientError> {
        convert_result(
            crate::data::demo_users()
                .into_iter()
                .find(|u| u.id == user_id)
                .ok_or_else(|| pc::ClientError::NotFound(format!("User {user_id}"))),
            |u| to_wit_user(&u),
        )
    }

    fn get_friends() -> Result<Vec<wit::User>, wit::ClientError> {
        Ok(crate::data::demo_users()
            .into_iter()
            .take(8)
            .map(|u| to_wit_user(&u))
            .collect())
    }

    fn get_channel_members(_channel_id: String) -> Result<Vec<wit::User>, wit::ClientError> {
        Ok(crate::data::demo_users()
            .into_iter()
            .map(|u| to_wit_user(&u))
            .collect())
    }

    fn get_groups() -> Result<Vec<wit::Group>, wit::ClientError> {
        Ok(crate::data::demo_groups_v2()
            .into_iter()
            .map(to_wit_group)
            .collect())
    }

    fn remove_group_member(_group_id: String, _user_id: String) -> Result<(), wit::ClientError> {
        Ok(())
    }

    fn add_group_member(_group_id: String, _user_id: String) -> Result<(), wit::ClientError> {
        Ok(())
    }

    fn get_dm_channels() -> Result<Vec<wit::DmChannel>, wit::ClientError> {
        Ok(crate::data::demo_dm_channels()
            .into_iter()
            .map(to_wit_dm_channel)
            .collect())
    }

    fn open_direct_message_channel(user_id: String) -> Result<wit::DmChannel, wit::ClientError> {
        convert_result(
            crate::data::demo_dm_channels()
                .into_iter()
                .find(|dm| dm.user.id == user_id)
                .map_or_else(
                    || {
                        crate::data::demo_empty_dm_channel_for_user(
                            &user_id,
                            crate::data::DEMO_ACCOUNT_ID,
                        )
                    },
                    Ok,
                ),
            to_wit_dm_channel,
        )
    }

    fn open_saved_messages_channel() -> Result<wit::DmChannel, wit::ClientError> {
        let session = crate::data::demo_session();
        Ok(to_wit_dm_channel(pc::DmChannel {
            id: "dm-demo-saved-self".to_string(),
            user: session.user,
            last_message: None,
            unread_count: 0,
            backend: pc::BackendType::from(crate::SLUG),
            account_id: crate::data::DEMO_ACCOUNT_ID.to_string(),
        }))
    }

    fn get_notifications() -> Result<Vec<wit::Notification>, wit::ClientError> {
        Ok(crate::data::demo_notifications()
            .into_iter()
            .map(to_wit_notification)
            .collect())
    }

    fn get_voice_participants(
        channel_id: String,
    ) -> Result<Vec<wit::VoiceParticipant>, wit::ClientError> {
        Ok(crate::data::demo_voice_participants(&channel_id)
            .into_iter()
            .map(to_wit_voice_participant)
            .collect())
    }

    // G.5 — voice-transport WIT stubs (demo backend: pseudo-only).
    fn join_voice_channel_transport(
        _server_id: String,
        _channel_id: String,
    ) -> Result<(), wit::ClientError> {
        Ok(())
    }

    fn start_dm_call_transport(
        _dm_channel_id: String,
    ) -> Result<(), wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "start_dm_call_transport (WIT stub)".into(),
        ))
    }

    fn set_voice_mute(
        _server_id: String,
        _channel_id: String,
        _self_mute: bool,
        _self_deaf: bool,
    ) -> Result<(), wit::ClientError> {
        Ok(())
    }

    fn get_presence(_user_id: String) -> Result<wit::PresenceStatus, wit::ClientError> {
        Ok(wit::PresenceStatus::Online)
    }

    fn set_presence(_status: wit::PresenceStatus) -> Result<(), wit::ClientError> {
        Ok(())
    }

    fn handle_ws_data(_handle: u64, _data: Vec<u8>) {
        // Demo plugin does not use WebSocket connections.
    }

    fn get_backend_type() -> String {
        "demo".to_string()
    }

    fn get_backend_name() -> String {
        "Demo".to_string()
    }

    fn get_backend_capabilities() -> wit::BackendCapabilities {
        wit::BackendCapabilities {
            supports_voice: true,
            supports_video: true,
            supports_dms: true,
            supports_groups: true,
            supports_send_messages: true,
            supports_presence: true,
            supports_search: true,
            supports_reactions: true,
            supports_typing_indicators: true,
            supports_file_upload: true,
            landing: wit::LandingPage::DirectMessages,
        }
    }

    fn list_files(_channel_id: String, _path: String) -> Result<Vec<wit::FileEntry>, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "demo plugin has no code channels".to_string(),
        ))
    }

    fn read_file(_channel_id: String, _path: String) -> Result<wit::FileContent, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "demo plugin has no code channels".to_string(),
        ))
    }

    fn get_forum_posts(
        _forum_channel_id: String,
        _sort: wit::ForumSortOrder,
        _limit: Option<u32>,
    ) -> Result<Vec<wit::ForumPost>, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "get_forum_posts not implemented".to_string(),
        ))
    }

    fn get_active_threads(
        _server_id: String,
    ) -> Result<Vec<wit::ThreadInfo>, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "get_active_threads not implemented".to_string(),
        ))
    }

    fn get_archived_threads(
        _parent_channel_id: String,
        _limit: Option<u32>,
    ) -> Result<Vec<wit::ThreadInfo>, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "get_archived_threads not implemented".to_string(),
        ))
    }

    fn create_forum_post(
        _forum_channel_id: String,
        _title: String,
        _body: String,
        _tags: Vec<String>,
    ) -> Result<wit::ForumPost, wit::ClientError> {
        Err(wit::ClientError::NotSupported(
            "create_forum_post not implemented".to_string(),
        ))
    }

    fn get_signup_method(
        _server_url: Option<String>,
    ) -> Result<wit::SignupMethod, wit::ClientError> {
        // Demo is a fake backend with no real signup flow.
        Ok(wit::SignupMethod::NotSupported)
    }
}

// ─── Plugin Metadata Implementation ───────────────────────────────

/// FTL translation source strings embedded at WASM compile time.
///
/// These are returned via `get-translations(locale)` and merged by the
/// host into its global Fluent bundle under the plugin namespace.
/// All message IDs are prefixed with `plugin-demo-`.
const FTL_EN: &str = include_str!("../locales/en/plugin.ftl");
const FTL_DE: &str = include_str!("../locales/de/plugin.ftl");
const FTL_FR: &str = include_str!("../locales/fr/plugin.ftl");
const FTL_ES: &str = include_str!("../locales/es/plugin.ftl");

impl PluginMetadataGuest for DemoPlugin {
    fn get_translations(locale: String) -> String {
        match locale.as_str() {
            "en" => FTL_EN.to_string(),
            "de" => FTL_DE.to_string(),
            "fr" => FTL_FR.to_string(),
            "es" => FTL_ES.to_string(),
            // Fall back to English for unknown locales
            _ => FTL_EN.to_string(),
        }
    }

    fn get_display_name_key() -> String {
        "plugin-demo-title".to_string()
    }

    fn get_icon() -> String {
        "🧪".to_string()
    }

    fn get_plugin_manifest() -> crate::wit_bindings::PluginManifest {
        crate::wit_bindings::PluginManifest {
            exec_programs: vec![],
            http_hosts: vec![],
            description: "Demo backend with hardcoded fixture data — no network or subprocess access.".to_string(),
            homepage: None,
        }
    }
}

// ─── New UI Surface Stubs ──────────────────────────────────────────

use crate::wit_bindings::exports::poly::messenger::client_menus as menus_exp;
use crate::wit_bindings::exports::poly::messenger::client_settings as settings_exp;
use crate::wit_bindings::exports::poly::messenger::client_sidebar as sidebar_exp;
use crate::wit_bindings::exports::poly::messenger::client_views as views_exp;
use crate::wit_bindings::exports::poly::messenger::client_composer as composer_exp;
use crate::wit_bindings::poly::messenger::host_api;

// ─── Settings KV helpers ───────────────────────────────────────────

fn scope_label(scope: settings_exp::SettingsScope) -> &'static str {
    match scope {
        settings_exp::SettingsScope::AccountGlobal => "account-global",
        settings_exp::SettingsScope::PerServer => "per-server",
        settings_exp::SettingsScope::PerChannel => "per-channel",
        settings_exp::SettingsScope::PerUser => "per-user",
    }
}

fn composite_key(scope: settings_exp::SettingsScope, scope_id: &str, key: &str) -> String {
    format!("settings:{}:{}:{}", scope_label(scope), scope_id, key)
}

impl ClientMenusGuest for DemoPlugin {
    fn get_context_menu_items(
        _target: menus_exp::MenuTargetKind,
        _target_id: String,
    ) -> Result<Vec<menus_exp::MenuItem>, menus_exp::ClientError> {
        Ok(Vec::new())
    }

    fn invoke_context_action(
        action_id: String,
        _target: menus_exp::MenuTargetKind,
        _target_id: String,
    ) -> Result<menus_exp::ActionOutcome, menus_exp::ClientError> {
        Err(menus_exp::ClientError::NotFound(action_id))
    }

    fn poll_action(
        _handle: menus_exp::PendingHandle,
    ) -> Result<menus_exp::ActionOutcome, menus_exp::ClientError> {
        Ok(menus_exp::ActionOutcome::Completed)
    }
}

impl ClientSettingsGuest for DemoPlugin {
    fn get_settings_sections(
    ) -> Result<Vec<settings_exp::SettingsSection>, settings_exp::ClientError> {
        Ok(Vec::new())
    }

    fn get_setting_value(
        scope: settings_exp::SettingsScope,
        scope_id: String,
        key: String,
    ) -> Result<String, settings_exp::ClientError> {
        let storage_key = composite_key(scope, &scope_id, &key);
        match host_api::storage_get(&storage_key) {
            Some(bytes) => Ok(String::from_utf8(bytes).unwrap_or_else(|_| "null".to_string())),
            None => Ok("null".to_string()),
        }
    }

    fn set_setting_value(
        scope: settings_exp::SettingsScope,
        scope_id: String,
        key: String,
        value: String,
    ) -> Result<(), settings_exp::ClientError> {
        let storage_key = composite_key(scope, &scope_id, &key);
        host_api::storage_set(&storage_key, value.as_bytes())
            .map_err(settings_exp::ClientError::Internal)
    }
}

impl ClientSidebarGuest for DemoPlugin {
    fn get_sidebar_declaration(
    ) -> Result<sidebar_exp::SidebarDeclaration, sidebar_exp::ClientError> {
        Ok(sidebar_exp::SidebarDeclaration {
            layout: sidebar_exp::SidebarLayoutKind::ChannelList,
            sections: vec![],
            header_block: None,
        })
    }

    fn invoke_sidebar_action(
        action_id: String,
    ) -> Result<sidebar_exp::ActionOutcome, sidebar_exp::ClientError> {
        Err(sidebar_exp::ClientError::NotFound(action_id))
    }
}

impl ClientViewsGuest for DemoPlugin {
    fn get_account_overview_view() -> Result<views_exp::ViewDescriptor, views_exp::ClientError> {
        // Demo backend has no account overview; not supported.
        Err(views_exp::ClientError::NotSupported(
            "demo plugin does not support account overview views".to_string(),
        ))
    }

    fn get_channel_view(
        _channel_id: String,
    ) -> Result<views_exp::ViewDescriptor, views_exp::ClientError> {
        Err(views_exp::ClientError::NotSupported(
            "demo plugin does not support non-chat views".to_string(),
        ))
    }

    fn get_view_rows(
        _channel_id: String,
        _cursor: Option<views_exp::Cursor>,
        _sort_id: Option<String>,
        _filter_id: Option<String>,
        _tab_id: Option<String>,
    ) -> Result<views_exp::ViewRowsPage, views_exp::ClientError> {
        Err(views_exp::ClientError::NotSupported(
            "demo plugin does not support non-chat views".to_string(),
        ))
    }

    fn get_view_detail(
        _channel_id: String,
        _row_id: String,
    ) -> Result<views_exp::ViewDetail, views_exp::ClientError> {
        Err(views_exp::ClientError::NotSupported(
            "demo plugin does not support non-chat views".to_string(),
        ))
    }
}

impl ClientComposerGuest for DemoPlugin {
    fn get_composer_buttons(
        _channel_id: String,
    ) -> Result<Vec<composer_exp::ComposerButton>, composer_exp::ClientError> {
        Ok(Vec::new())
    }

    fn get_message_actions(
        _channel_id: String,
        _message_id: String,
    ) -> Result<Vec<composer_exp::MenuItem>, composer_exp::ClientError> {
        Ok(Vec::new())
    }

    fn invoke_composer_action(
        action_id: String,
        _channel_id: String,
    ) -> Result<composer_exp::ActionOutcome, composer_exp::ClientError> {
        Err(composer_exp::ClientError::NotFound(action_id))
    }

    fn invoke_message_action(
        action_id: String,
        _channel_id: String,
        _message_id: String,
    ) -> Result<composer_exp::ActionOutcome, composer_exp::ClientError> {
        Err(composer_exp::ClientError::NotFound(action_id))
    }
}

// ─── ClientConfigGuest ─────────────────────────────────────────────

impl ClientConfigGuest for DemoPlugin {
    fn get_client_version() -> String {
        // Demo backend has no real version; return a stable placeholder.
        "poly-demo/0.0.0".to_string()
    }

    fn set_client_version_override(
        _version_override: Option<String>,
    ) -> Result<(), wit::ClientError> {
        // Demo backend ignores version overrides — there is no real client to configure.
        Ok(())
    }

    fn get_client_mechanisms() -> Result<Vec<Mechanism>, wit::ClientError> {
        // Demo backend has no configurable mechanisms.
        Ok(vec![])
    }

    fn set_client_mechanism(id: String, _enabled: bool) -> Result<(), wit::ClientError> {
        Err(wit::ClientError::NotFound(format!(
            "unknown mechanism: {id}"
        )))
    }
}

// Register the component export.
export!(DemoPlugin with_types_in crate::wit_bindings);
