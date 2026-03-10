//! WASM Component Model guest implementation for the demo messenger plugin.
//!
//! This module is only compiled when targeting `wasm32` (gated in `lib.rs`).
//! It implements the `messenger-client` export interface, delegating to the `data`
//! module for all demo content and converting `poly-client` types → WIT types.
//!
//! DECISION(D21): WASM Plugin Backends.

use std::cell::RefCell;

use poly_client as pc;

use crate::wit_bindings::{
    MessengerClientGuest, PluginMetadataGuest, SettingDescriptor, SettingKind, wit,
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

fn to_wit_backend_type(bt: pc::BackendType) -> wit::BackendType {
    match bt {
        pc::BackendType::Stoat => wit::BackendType::Stoat,
        pc::BackendType::Matrix => wit::BackendType::Matrix,
        pc::BackendType::Discord => wit::BackendType::Discord,
        pc::BackendType::Teams => wit::BackendType::Teams,
        pc::BackendType::Demo => wit::BackendType::Demo,
        pc::BackendType::Poly => wit::BackendType::Poly,
    }
}

fn to_wit_presence(ps: pc::PresenceStatus) -> wit::PresenceStatus {
    match ps {
        pc::PresenceStatus::Online => wit::PresenceStatus::Online,
        pc::PresenceStatus::Idle => wit::PresenceStatus::Idle,
        pc::PresenceStatus::DoNotDisturb => wit::PresenceStatus::DoNotDisturb,
        pc::PresenceStatus::Invisible => wit::PresenceStatus::Invisible,
        pc::PresenceStatus::Offline => wit::PresenceStatus::Offline,
    }
}

fn to_wit_channel_type(ct: pc::ChannelType) -> wit::ChannelType {
    match ct {
        pc::ChannelType::Text => wit::ChannelType::Text,
        pc::ChannelType::Voice => wit::ChannelType::Voice,
        pc::ChannelType::Video => wit::ChannelType::Video,
    }
}

fn to_wit_user(u: &pc::User) -> wit::User {
    wit::User {
        id: u.id.clone(),
        display_name: u.display_name.clone(),
        avatar_url: u.avatar_url.clone(),
        presence: to_wit_presence(u.presence),
        backend: to_wit_backend_type(u.backend),
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
        categories: s.categories.iter().map(to_wit_category).collect(),
        backend: to_wit_backend_type(s.backend),
        unread_count: s.unread_count,
        account_id: s.account_id,
        account_display_name: s.account_display_name,
    }
}

fn to_wit_channel(c: pc::Channel) -> wit::Channel {
    wit::Channel {
        id: c.id,
        name: c.name,
        channel_type: to_wit_channel_type(c.channel_type),
        server_id: c.server_id,
        unread_count: c.unread_count,
        last_message_id: c.last_message_id,
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
        backend: to_wit_backend_type(s.backend),
        icon_emoji: s.icon_emoji,
        instance_id: s.instance_id,
    }
}

fn to_wit_group(g: pc::Group) -> wit::Group {
    wit::Group {
        id: g.id,
        members: g.members.iter().map(to_wit_user).collect(),
        name: g.name,
        last_message: g.last_message.map(to_wit_message),
        backend: to_wit_backend_type(g.backend),
        account_id: g.account_id,
    }
}

fn to_wit_dm_channel(dm: pc::DmChannel) -> wit::DmChannel {
    wit::DmChannel {
        id: dm.id,
        user: to_wit_user(&dm.user),
        last_message: dm.last_message.map(to_wit_message),
        unread_count: dm.unread_count,
        backend: to_wit_backend_type(dm.backend),
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
        pc::NotificationKind::Other(desc) => wit::NotificationKind::Other(desc.clone()),
    }
}

fn to_wit_notification(n: pc::Notification) -> wit::Notification {
    wit::Notification {
        id: n.id,
        kind: to_wit_notification_kind(&n.kind),
        backend: to_wit_backend_type(n.backend),
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
                .map(|a| pc::Attachment {
                    id: a.id,
                    filename: a.filename,
                    content_type: a.content_type,
                    url: a.url,
                    size: a.size,
                })
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

    fn get_dm_channels() -> Result<Vec<wit::DmChannel>, wit::ClientError> {
        Ok(crate::data::demo_dm_channels()
            .into_iter()
            .map(to_wit_dm_channel)
            .collect())
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

    fn get_presence(_user_id: String) -> Result<wit::PresenceStatus, wit::ClientError> {
        Ok(wit::PresenceStatus::Online)
    }

    fn set_presence(_status: wit::PresenceStatus) -> Result<(), wit::ClientError> {
        Ok(())
    }

    fn poll_event() -> Option<wit::ClientEvent> {
        // Demo WASM plugin does not emit live events.
        // The native `event_stream()` uses tokio timers which are not
        // available in WASI. A future iteration could use host time
        // to generate periodic events.
        None
    }

    fn get_backend_type() -> wit::BackendType {
        wit::BackendType::Demo
    }

    fn get_backend_name() -> String {
        "Demo".to_string()
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

    fn get_settings_schema() -> Vec<SettingDescriptor> {
        vec![SettingDescriptor {
            key: "enabled".to_string(),
            kind: SettingKind::Toggle,
            default_value: "false".to_string(),
            extra: String::new(),
        }]
    }

    fn get_display_name_key() -> String {
        "plugin-demo-title".to_string()
    }

    fn get_icon() -> String {
        "🧪".to_string()
    }
}

// Register the component export.
// EXCEPTION: unsafe_code is allowed here only because the export!() macro
// produces unsafe FFI stubs. This is unavoidable for WIT component registration.
#[allow(unsafe_code)]
export!(DemoPlugin);
