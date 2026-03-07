//! WASM Component Model guest implementation for the demo messenger plugin.
//!
//! This module is only compiled when targeting `wasm32` (gated in `lib.rs`).
//! It implements the `messenger-client` export interface, delegating to the `data`
//! module for all demo content and converting `poly-client` types → WIT types.
//!
//! DECISION(D21): WASM Plugin Backends.

use std::cell::RefCell;

use poly_client as pc;

use crate::wit_bindings::{Guest, wit};

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
        edited: m.edited,
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

impl Guest for DemoPlugin {
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

    fn get_messages(
        channel_id: String,
        _query: wit::MessageQuery,
    ) -> Result<Vec<wit::Message>, wit::ClientError> {
        let messages = if channel_id.starts_with("dm-") {
            crate::data::demo_dm_messages(&channel_id)
        } else if channel_id.starts_with("group-") {
            crate::data::demo_group_messages(&channel_id)
        } else {
            let rich = crate::data::demo2_messages_rich(&channel_id);
            if rich.is_empty() {
                crate::data::demo_messages(&channel_id)
            } else {
                rich
            }
        };
        Ok(messages.into_iter().map(to_wit_message).collect())
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

// Register the component export.
// EXCEPTION: unsafe_code is allowed here only because the export!() macro
// produces unsafe FFI stubs. This is unavoidable for WIT component registration.
#[allow(unsafe_code)]
export!(DemoPlugin);
