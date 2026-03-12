//! Chat view — Discord-style message list and message input.
//!
//! Common implementation shared across all messenger backends.
//! Backend-specific chat view overrides (e.g., special message types)
//! will live in per-backend directories in future phases.
//!
//! Features:
//! - Message grouping (same author within 7 minutes)
//! - Date separators between different days
//! - Inline search, pinned messages, and threads rail
//! - Multi-line composer with toolbar controls
//! - Message reactions, editing, and context menu

use super::super::super::routes::Route;
use super::chat_history::{
    ChatHistoryUiState, OLDER_MESSAGES_PAGE_SIZE, read_message_list_scroll_metrics,
    remember_message_list_scroll_position, request_preserve_scroll_position,
    unread_marker_message_id,
};
use super::dm_user_sidebar::DmUserSidebar;
use super::emoji_picker::EmojiPicker;
use super::user_sidebar::UserSidebar;
use crate::client_manager::ClientManager;
use crate::i18n::{t, t_args};
use crate::state::chat_data::{backend_badge, format_file_size, user_color};
use crate::state::{AppState, ChatData};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use dioxus::html::HasFileData;
use dioxus::prelude::*;
use poly_client::{
    Attachment, BackendType, Channel, ChatCommand, CommandScope, DmChannel, Message,
    MessageContent, MessageQuery, MessageReplyPreview, MessageSearchHit, MessageSearchQuery,
    PresenceStatus,
};

#[derive(Debug, Clone)]
struct MsgContextMenu {
    x: f64,
    y: f64,
    message_id: String,
    message_text: String,
    is_own: bool,
}

const GROUP_THRESHOLD_MINUTES: i64 = 7;

/// Built-in slash commands always available in every channel.
///
/// Tuple fields: (name, description, usage_hint)
const BUILTIN_COMMANDS: &[(&str, &str, &str)] = &[
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
fn filtered_slash_commands(query: &str, backend_cmds: &[ChatCommand]) -> Vec<ChatCommand> {
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
fn apply_builtin_command(text: &str) -> Option<String> {
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
fn reply_preview_snippet(content: &MessageContent) -> String {
    let raw = match content {
        MessageContent::Text(text) => text.clone(),
        MessageContent::WithAttachments { text, .. } => text.clone(),
    };
    raw.chars().take(80).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChatUtilityPanel {
    Search,
    Pinned,
    Threads,
}

#[derive(Clone, Copy)]
struct SearchFilterSuggestion {
    icon: &'static str,
    title_key: &'static str,
    subtitle_key: &'static str,
    token: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SearchFilterOption {
    icon: &'static str,
    title: String,
    subtitle: String,
    token: String,
    completion_token: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingAttachmentPreview {
    id: String,
    filename: String,
    content_type: String,
    size: u64,
    preview_url: Option<String>,
}

const SEARCH_FILTER_SUGGESTIONS: &[SearchFilterSuggestion] = &[
    SearchFilterSuggestion {
        icon: "👤",
        title_key: "search-filter-from-user",
        subtitle_key: "search-filter-from-user-subtitle",
        token: "from:alice",
    },
    SearchFilterSuggestion {
        icon: "#",
        title_key: "search-filter-in-channel",
        subtitle_key: "search-filter-in-channel-subtitle",
        token: "in:#current",
    },
    SearchFilterSuggestion {
        icon: "🔗",
        title_key: "search-filter-has-link",
        subtitle_key: "search-filter-has-link-subtitle",
        token: "has:link",
    },
    SearchFilterSuggestion {
        icon: "@",
        title_key: "search-filter-mentions",
        subtitle_key: "search-filter-mentions-subtitle",
        token: "mentions:me",
    },
    SearchFilterSuggestion {
        icon: "☷",
        title_key: "search-filter-more",
        subtitle_key: "search-filter-more-subtitle",
        token: "has:link from:alice",
    },
];

fn completion_token_for_search_filter(token: &str) -> String {
    if token.starts_with("from:") {
        return "from:".to_string();
    }
    if token.starts_with("in:#") {
        return "in:#".to_string();
    }
    if token.starts_with("has:") {
        return "has:".to_string();
    }
    if token.starts_with("mentions:") {
        return "mentions:".to_string();
    }
    token.to_string()
}

fn build_search_filter_options(current_channel_name: &str) -> Vec<SearchFilterOption> {
    SEARCH_FILTER_SUGGESTIONS
        .iter()
        .map(|suggestion| {
            let token = if suggestion.token == "in:#current" {
                format!("in:#{}", current_channel_name)
            } else {
                suggestion.token.to_string()
            };

            SearchFilterOption {
                icon: suggestion.icon,
                title: t(suggestion.title_key),
                subtitle: t(suggestion.subtitle_key),
                completion_token: completion_token_for_search_filter(&token),
                token,
            }
        })
        .collect()
}

fn active_search_filter_term(raw_query: &str) -> &str {
    raw_query
        .split_whitespace()
        .last()
        .map(str::trim)
        .unwrap_or("")
}

fn filter_search_filter_options(
    options: &[SearchFilterOption],
    raw_query: &str,
) -> Vec<SearchFilterOption> {
    let term = active_search_filter_term(raw_query).to_ascii_lowercase();
    if term.is_empty() {
        return options.to_vec();
    }

    options
        .iter()
        .filter(|option| {
            option
                .completion_token
                .to_ascii_lowercase()
                .starts_with(&term)
                || option.token.to_ascii_lowercase().contains(&term)
                || option.title.to_ascii_lowercase().contains(&term)
                || option.subtitle.to_ascii_lowercase().contains(&term)
        })
        .cloned()
        .collect()
}

fn apply_search_filter_completion(existing: &str, completion_token: &str) -> String {
    let mut parts = existing.split_whitespace().collect::<Vec<_>>();
    if parts.is_empty() {
        return format!("{completion_token} ");
    }

    parts.pop();
    parts.push(completion_token);

    format!("{} ", parts.join(" "))
}

fn message_plain_text(content: &MessageContent) -> String {
    match content {
        MessageContent::Text(text) => text.clone(),
        MessageContent::WithAttachments { text, .. } => text.clone(),
    }
}

fn pending_attachment_to_attachment(preview: &PendingAttachmentPreview) -> Attachment {
    Attachment {
        id: preview.id.clone(),
        filename: preview.filename.clone(),
        content_type: preview.content_type.clone(),
        url: preview.preview_url.clone().unwrap_or_default(),
        size: preview.size,
    }
}

async fn build_attachment_previews(
    files: Vec<dioxus::html::FileData>,
) -> Vec<PendingAttachmentPreview> {
    let mut previews = Vec::new();

    for (index, file) in files.into_iter().enumerate() {
        let filename = file.name();
        let content_type = file
            .content_type()
            .unwrap_or_else(|| "application/octet-stream".to_string());
        let size = file.size();
        let preview_url = if content_type.starts_with("image/") && size <= 5_000_000 {
            match file.read_bytes().await {
                Ok(bytes) => Some(format!(
                    "data:{content_type};base64,{}",
                    BASE64_STANDARD.encode(bytes)
                )),
                Err(err) => {
                    tracing::warn!("failed to read attachment preview bytes: {err}");
                    None
                }
            }
        } else {
            None
        };

        previews.push(PendingAttachmentPreview {
            id: format!("pending-{}-{}-{}", file.last_modified(), index, filename),
            filename,
            content_type,
            size,
            preview_url,
        });
    }

    previews
}

async fn append_attachment_previews(
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

fn message_search_terms(raw: &str) -> Vec<String> {
    raw.split_whitespace()
        .filter(|token| !token.contains(':'))
        .map(ToString::to_string)
        .filter(|token| !token.is_empty())
        .collect()
}

fn slash_command_query(text: &str) -> &str {
    text.trim_start()
        .strip_prefix('/')
        .unwrap_or("")
        .split(' ')
        .next()
        .unwrap_or("")
}

fn contextual_search_placeholder(
    current_channel: Option<&poly_client::Channel>,
    is_dm_channel: bool,
    is_group_channel: bool,
) -> String {
    if is_dm_channel {
        return t_args(
            "search-placeholder-user",
            &[(
                "user",
                current_channel.map_or("", |channel| channel.name.as_str()),
            )],
        );
    }
    if is_group_channel {
        return t_args(
            "search-placeholder-group",
            &[(
                "group",
                current_channel.map_or("", |channel| channel.name.as_str()),
            )],
        );
    }
    t_args(
        "search-placeholder-channel",
        &[(
            "channel",
            current_channel.map_or("", |channel| channel.name.as_str()),
        )],
    )
}

fn contextual_compose_placeholder(
    current_channel: Option<&poly_client::Channel>,
    is_dm_channel: bool,
    is_group_channel: bool,
) -> String {
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

fn build_search_query(
    raw: String,
    current_channel: Option<poly_client::Channel>,
    current_server: Option<poly_client::Server>,
    self_user_id: String,
    is_dm_channel: bool,
    is_group_channel: bool,
) -> MessageSearchQuery {
    let mut query = MessageSearchQuery {
        text: String::new(),
        channel_id: if is_dm_channel || is_group_channel {
            current_channel.as_ref().map(|channel| channel.id.clone())
        } else {
            None
        },
        server_id: if is_dm_channel || is_group_channel {
            None
        } else {
            current_server.as_ref().map(|server| server.id.clone())
        },
        author_id: None,
        has_link: false,
        mentions_user_id: None,
        limit: Some(25),
    };
    let mut free_text = Vec::new();

    for token in raw.split_whitespace() {
        if let Some(author) = token.strip_prefix("from:") {
            if !author.is_empty() {
                query.author_id = Some(author.trim_start_matches('@').to_string());
            }
        } else if let Some(channel_name) = token.strip_prefix("in:") {
            if let Some(channel) = current_channel.as_ref() {
                let normalized = channel_name.trim_start_matches('#');
                if normalized.eq_ignore_ascii_case(&channel.name) {
                    query.channel_id = Some(channel.id.clone());
                }
            }
        } else if token.eq_ignore_ascii_case("has:link") {
            query.has_link = true;
        } else if token.eq_ignore_ascii_case("mentions:me") {
            query.mentions_user_id = Some(self_user_id.clone());
        } else {
            free_text.push(token.to_string());
        }
    }

    query.text = free_text.join(" ");
    query
}

fn highlight_message(message_id: &str) {
    let dom_id = format!("message-{message_id}");
    document::eval(&format!(
        "setTimeout(() => {{ const el = document.getElementById('{dom_id}'); if (el) {{ el.scrollIntoView({{behavior: 'smooth', block: 'center'}}); el.classList.add('message-search-hit'); setTimeout(() => el.classList.remove('message-search-hit'), 1400); }} }}, 80);"
    ));
}

fn current_channel_unread_count(
    channel_id: Option<&str>,
    current_channel: Option<&Channel>,
    dm_channels: &[DmChannel],
) -> u32 {
    let Some(channel_id) = channel_id else {
        return 0;
    };

    if let Some(dm) = dm_channels.iter().find(|dm| dm.id == channel_id) {
        return dm.unread_count;
    }

    current_channel
        .filter(|channel| channel.id == channel_id)
        .map_or(0, |channel| channel.unread_count)
}

fn unread_banner_timestamp<'a>(
    messages: &'a [Message],
    marker_message_id: Option<&str>,
) -> Option<&'a chrono::DateTime<chrono::Utc>> {
    let marker_message_id = marker_message_id?;
    messages
        .iter()
        .find(|message| message.id == marker_message_id)
        .map(|message| &message.timestamp)
}

fn display_unread_count(unread_count: u32) -> String {
    if unread_count > 9 {
        return format!("{unread_count}+");
    }

    unread_count.to_string()
}

fn mark_channel_as_read(chat_data: &mut Signal<ChatData>, channel_id: &str) -> u32 {
    let (unread_count, current_server_id) = {
        let data = chat_data.read();
        let unread_count = data
            .dm_channels
            .iter()
            .find(|dm| dm.id == channel_id)
            .map(|dm| dm.unread_count)
            .or_else(|| {
                data.channels
                    .iter()
                    .find(|channel| channel.id == channel_id)
                    .map(|channel| channel.unread_count)
            })
            .or_else(|| {
                data.current_channel
                    .as_ref()
                    .filter(|channel| channel.id == channel_id)
                    .map(|channel| channel.unread_count)
            })
            .unwrap_or(0);
        let current_server_id = data.current_server.as_ref().map(|server| server.id.clone());
        (unread_count, current_server_id)
    };

    if unread_count == 0 {
        return 0;
    }

    let mut data = chat_data.write();

    if let Some(current_channel) = data.current_channel.as_mut()
        && current_channel.id == channel_id
    {
        current_channel.unread_count = 0;
    }

    for channel in &mut data.channels {
        if channel.id == channel_id {
            channel.unread_count = 0;
            break;
        }
    }

    for dm in &mut data.dm_channels {
        if dm.id == channel_id {
            dm.unread_count = 0;
            break;
        }
    }

    if let Some(server_id) = current_server_id {
        if let Some(current_server) = data.current_server.as_mut()
            && current_server.id == server_id
        {
            current_server.unread_count = current_server.unread_count.saturating_sub(unread_count);
        }

        for server in &mut data.servers {
            if server.id == server_id {
                server.unread_count = server.unread_count.saturating_sub(unread_count);
                break;
            }
        }
    }

    unread_count
}

async fn open_message_hit(
    hit: MessageSearchHit,
    current_channel_id: Option<String>,
    current_server_id: Option<String>,
    client_manager: Signal<ClientManager>,
    mut chat_data: Signal<ChatData>,
    mut app_state: Signal<AppState>,
) -> Option<(Route, String)> {
    let target_message_id = hit.message.id.clone();
    let target_channel_id = hit.channel_id.clone();

    if let Some(ref previous_channel_id) = current_channel_id
        && previous_channel_id != &target_channel_id
    {
        remember_message_list_scroll_position(previous_channel_id);
    }

    if message_hit_already_rendered(
        &chat_data,
        current_channel_id.as_deref(),
        &target_channel_id,
        &target_message_id,
    ) {
        highlight_message(&target_message_id);
        return None;
    }

    let target_server_id = hit.server_id.clone().or(current_server_id);
    let active_account_id = app_state.read().nav.active_account_id.clone();
    let active_instance_id = app_state.read().nav.active_instance_id.clone();

    let backend_info = if let Some(ref server_id) = target_server_id {
        client_manager
            .read()
            .get_backend_for_server(server_id)
            .map(|(account_id, backend)| (account_id, backend, None::<BackendType>))
    } else if let Some(ref account_id) = active_account_id {
        client_manager
            .read()
            .get_backend(account_id)
            .map(|backend| {
                (
                    account_id.clone(),
                    backend,
                    app_state.read().nav.active_backend,
                )
            })
    } else {
        None
    };
    let (account_id, backend, fallback_backend) = backend_info?;

    let guard = backend.read().await;
    let target_channel = guard.get_channel(&target_channel_id).await.ok();
    let target_messages = guard
        .get_messages(
            &target_channel_id,
            MessageQuery {
                around: Some(target_message_id.clone()),
                limit: Some(64),
                ..Default::default()
            },
        )
        .await
        .unwrap_or_default();
    let target_members = guard
        .get_channel_members(&target_channel_id)
        .await
        .unwrap_or_default();
    let target_server = if let Some(ref server_id) = target_server_id {
        guard.get_server(server_id).await.ok()
    } else {
        None
    };
    let backend_type = target_server
        .as_ref()
        .map(|server| server.backend)
        .or(fallback_backend)
        .unwrap_or(BackendType::Demo);
    drop(guard);

    chat_data.write().loading = false;
    chat_data.write().messages = target_messages;
    chat_data.write().members = target_members;
    chat_data.write().current_channel = target_channel.clone();
    chat_data.write().current_server = target_server.clone();
    app_state.write().nav.selected_channel = Some(target_channel_id.clone());

    Some(build_message_hit_route(
        &mut app_state,
        MessageHitRouteCtx {
            client_manager,
            active_instance_id,
            account_id,
            target_server_id,
            target_channel_id,
            backend_type,
            target_message_id,
        },
    ))
}

fn message_hit_already_rendered(
    chat_data: &Signal<ChatData>,
    current_channel_id: Option<&str>,
    target_channel_id: &str,
    target_message_id: &str,
) -> bool {
    current_channel_id == Some(target_channel_id)
        && chat_data
            .read()
            .messages
            .iter()
            .any(|message| message.id == target_message_id)
}

struct MessageHitRouteCtx {
    client_manager: Signal<ClientManager>,
    active_instance_id: Option<String>,
    account_id: String,
    target_server_id: Option<String>,
    target_channel_id: String,
    backend_type: BackendType,
    target_message_id: String,
}

fn build_message_hit_route(
    app_state: &mut Signal<AppState>,
    ctx: MessageHitRouteCtx,
) -> (Route, String) {
    let MessageHitRouteCtx {
        client_manager,
        active_instance_id,
        account_id,
        target_server_id,
        target_channel_id,
        backend_type,
        target_message_id,
    } = ctx;

    let instance_id = active_instance_id.unwrap_or_else(|| {
        client_manager
            .read()
            .sessions
            .get(&account_id)
            .map(|session| session.instance_id.clone())
            .unwrap_or_default()
    });

    if let Some(server_id) = target_server_id {
        app_state.write().nav.selected_server = Some(server_id.clone());
        (
            Route::ServerChat {
                backend: backend_type.slug().to_string(),
                instance_id,
                account_id,
                server_id,
                channel_id: target_channel_id,
            },
            target_message_id,
        )
    } else {
        app_state.write().nav.selected_server = None;
        (
            Route::DmChat {
                backend: backend_type.slug().to_string(),
                instance_id,
                account_id,
                dm_id: target_channel_id,
            },
            target_message_id,
        )
    }
}

async fn persist_member_list_preferences(server_member_list_open: bool, dm_member_list_open: bool) {
    let Some(storage) = crate::STORAGE.get() else {
        return;
    };
    let Ok(mut settings) = storage.get_app_settings().await else {
        return;
    };
    if settings.server_member_list_open == server_member_list_open
        && settings.dm_member_list_open == dm_member_list_open
    {
        return;
    }
    settings.server_member_list_open = server_member_list_open;
    settings.dm_member_list_open = dm_member_list_open;
    if let Err(err) = storage.set_app_settings(&settings).await {
        tracing::warn!("Failed to persist member-list preferences: {err}");
    }
}

#[rustfmt::skip]
#[component]
pub fn ChatView() -> Element {
    render_chat_view()
}

fn render_chat_view() -> Element {
    let signals = use_chat_view_signals();
    let ctx = build_chat_view_markup_ctx(&signals);
    use_chat_view_effects(&signals, &ctx);
    render_chat_view_markup(ctx)
}

struct ChatViewSignals {
    app_state: Signal<AppState>,
    client_manager: Signal<ClientManager>,
    chat_data: Signal<ChatData>,
    message_input: Signal<String>,
    show_input_emoji: Signal<bool>,
    reaction_picker_msg: Signal<Option<String>>,
    drag_over: Signal<bool>,
    hovered_msg: Signal<Option<String>>,
    editing_msg_id: Signal<Option<String>>,
    edit_draft: Signal<String>,
    msg_context_menu: Signal<Option<MsgContextMenu>>,
    utility_panel: Signal<Option<ChatUtilityPanel>>,
    search_query: Signal<String>,
    search_hits: Signal<Vec<MessageSearchHit>>,
    pinned_messages: Signal<Vec<Message>>,
    notifications_muted: Signal<bool>,
    show_search_filters: Signal<bool>,
    active_search_filter_idx: Signal<usize>,
    pending_attachments: Signal<Vec<PendingAttachmentPreview>>,
    command_suggestions: Signal<Vec<ChatCommand>>,
    active_command_idx: Signal<usize>,
    show_command_popup: Signal<bool>,
    reply_target: Signal<Option<MessageReplyPreview>>,
    history_state: Signal<ChatHistoryUiState>,
    unread_marker_on_screen: Signal<bool>,
}

fn use_chat_view_signals() -> ChatViewSignals {
    ChatViewSignals {
        app_state: use_context(),
        client_manager: use_context(),
        chat_data: use_context(),
        message_input: use_signal(String::new),
        show_input_emoji: use_signal(|| false),
        reaction_picker_msg: use_signal(|| None::<String>),
        drag_over: use_signal(|| false),
        hovered_msg: use_signal(|| None::<String>),
        editing_msg_id: use_signal(|| None::<String>),
        edit_draft: use_signal(String::new),
        msg_context_menu: use_signal(|| None::<MsgContextMenu>),
        utility_panel: use_signal(|| None::<ChatUtilityPanel>),
        search_query: use_signal(String::new),
        search_hits: use_signal(Vec::<MessageSearchHit>::new),
        pinned_messages: use_signal(Vec::<Message>::new),
        notifications_muted: use_signal(|| false),
        show_search_filters: use_signal(|| false),
        active_search_filter_idx: use_signal(|| 0_usize),
        pending_attachments: use_signal(Vec::<PendingAttachmentPreview>::new),
        command_suggestions: use_signal(Vec::<ChatCommand>::new),
        active_command_idx: use_signal(|| 0_usize),
        show_command_popup: use_signal(|| false),
        reply_target: use_signal(|| None::<MessageReplyPreview>),
        history_state: use_signal(ChatHistoryUiState::default),
        unread_marker_on_screen: use_signal(|| false),
    }
}

fn build_chat_view_markup_ctx(signals: &ChatViewSignals) -> ChatViewMarkupCtx {
    let app_state = signals.app_state;
    let client_manager = signals.client_manager;
    let chat_data = signals.chat_data;
    let nav = navigator();
    let channel_id = app_state.read().nav.selected_channel.clone();
    let messages = chat_data.read().messages.clone();
    let current_channel = chat_data.read().current_channel.clone();
    let current_server = chat_data.read().current_server.clone();
    let loading = chat_data.read().loading;
    let reaction_picker_id = signals.reaction_picker_msg.read().clone();
    let group_members = chat_data.read().active_group_members.clone();
    let search_query_input_value = signals.search_query.read().clone();
    let search_query_value = search_query_input_value.trim().to_string();
    let current_channel_name = current_channel
        .as_ref()
        .map(|channel| channel.name.clone())
        .unwrap_or_default();
    let search_filter_options = build_search_filter_options(&current_channel_name);
    let filtered_search_filter_options =
        filter_search_filter_options(&search_filter_options, &search_query_input_value);
    let is_dm_channel = channel_id.as_deref().unwrap_or_default().starts_with("dm-");
    let is_group_channel = channel_id
        .as_deref()
        .unwrap_or_default()
        .starts_with("group-");
    let member_list_visible = if is_dm_channel || is_group_channel {
        app_state.read().nav.dm_right_sidebar_visible
    } else {
        app_state.read().nav.right_sidebar_visible
    };
    let (
        unread_marker_id,
        unread_banner_visible,
        unread_banner_count,
        unread_banner_time,
        unread_banner_date,
    ) = build_unread_banner_fields(signals.history_state, &messages);

    ChatViewMarkupCtx {
        app_state,
        client_manager,
        chat_data,
        channel_id: channel_id.clone(),
        messages,
        current_channel: current_channel.clone(),
        current_server: current_server.clone(),
        loading,
        reaction_picker_id,
        group_members,
        search_query_input_value,
        search_query_value: search_query_value.clone(),
        is_dm_channel,
        is_group_channel,
        member_list_visible,
        search_terms: message_search_terms(&search_query_value),
        search_placeholder: contextual_search_placeholder(
            current_channel.as_ref(),
            is_dm_channel,
            is_group_channel,
        ),
        compose_placeholder: contextual_compose_placeholder(
            current_channel.as_ref(),
            is_dm_channel,
            is_group_channel,
        ),
        search_filter_channel_name_onfocus: current_channel_name.clone(),
        search_filter_channel_name_oninput: current_channel_name,
        filtered_search_filter_options,
        unread_marker_id,
        unread_banner_visible,
        unread_banner_count,
        unread_banner_time,
        unread_banner_date,
        unread_banner_channel_id: channel_id.clone(),
        self_user_id: current_self_user_id(app_state, client_manager),
        dm_user_avatar: current_dm_user_avatar(chat_data, &channel_id, is_dm_channel),
        search_hit_channel_id: channel_id.clone(),
        pinned_hit_channel_id: channel_id,
        search_hit_server: current_server.clone(),
        pinned_hit_server: current_server.clone(),
        pinned_hit_channel: current_channel,
        nav_for_search: nav,
        nav_for_pinned: nav,
        message_input: signals.message_input,
        show_input_emoji: signals.show_input_emoji,
        reaction_picker_msg: signals.reaction_picker_msg,
        drag_over: signals.drag_over,
        hovered_msg: signals.hovered_msg,
        editing_msg_id: signals.editing_msg_id,
        edit_draft: signals.edit_draft,
        msg_context_menu: signals.msg_context_menu,
        utility_panel: signals.utility_panel,
        search_query: signals.search_query,
        search_hits: signals.search_hits,
        pinned_messages: signals.pinned_messages,
        notifications_muted: signals.notifications_muted,
        show_search_filters: signals.show_search_filters,
        active_search_filter_idx: signals.active_search_filter_idx,
        pending_attachments: signals.pending_attachments,
        command_suggestions: signals.command_suggestions,
        active_command_idx: signals.active_command_idx,
        show_command_popup: signals.show_command_popup,
        reply_target: signals.reply_target,
        history_state: signals.history_state,
        unread_marker_on_screen: signals.unread_marker_on_screen,
    }
}

fn build_unread_banner_fields(
    history_state: Signal<ChatHistoryUiState>,
    messages: &[Message],
) -> (Option<String>, bool, String, String, String) {
    let unread_marker_id = history_state.read().unread_marker_message_id.clone();
    let unread_count = history_state.read().unread_count;
    let unread_banner_time = unread_banner_timestamp(messages, unread_marker_id.as_deref())
        .map(|timestamp| timestamp.format("%H:%M").to_string())
        .unwrap_or_default();
    let unread_banner_date = unread_banner_timestamp(messages, unread_marker_id.as_deref())
        .map(|timestamp| timestamp.format("%-d %B %Y").to_string())
        .unwrap_or_default();

    (
        unread_marker_id,
        unread_count > 0,
        display_unread_count(unread_count),
        unread_banner_time,
        unread_banner_date,
    )
}

fn current_self_user_id(
    app_state: Signal<AppState>,
    client_manager: Signal<ClientManager>,
) -> String {
    let state = app_state.read();
    let cm = client_manager.read();
    state
        .nav
        .active_account_id
        .as_ref()
        .and_then(|aid| cm.sessions.get(aid))
        .map(|session| session.user.id.clone())
        .unwrap_or_default()
}

fn current_dm_user_avatar(
    chat_data: Signal<ChatData>,
    channel_id: &Option<String>,
    is_dm_channel: bool,
) -> Option<String> {
    if !is_dm_channel {
        return None;
    }

    let cid = channel_id.clone().unwrap_or_default();
    chat_data
        .read()
        .dm_channels
        .iter()
        .find(|dm| dm.id == cid)
        .and_then(|dm| dm.user.avatar_url.clone())
}

fn use_chat_view_effects(signals: &ChatViewSignals, ctx: &ChatViewMarkupCtx) {
    use_member_list_effect(signals);
    use_search_messages_effect(signals, ctx);
    use_pinned_messages_effect(signals);
    use_history_state_effect(signals);
    use_member_list_preferences_effect(signals.app_state);
    use_command_preload_effect(signals, &ctx.channel_id);
    use_unread_marker_visibility_effect(signals);
    use_composer_focus_effect(signals);
}

/// Auto-focus the message composer input whenever the selected channel or DM changes.
///
/// This gives the user immediate keyboard focus so they can start typing
/// right after clicking a channel or DM, matching Discord UX.
fn use_composer_focus_effect(signals: &ChatViewSignals) {
    let app_state = signals.app_state;
    use_effect(move || {
        // Depend on channel + active account so switching DMs also refocuses.
        let _channel = app_state.read().nav.selected_channel.clone();
        let _account = app_state.read().nav.active_account_id.clone();

        // Small delay so the composer DOM element is ready after route transition.
        let _ = document::eval(
            "setTimeout(() => { \
                const el = document.getElementById('poly-message-composer'); \
                if (el) el.focus(); \
            }, 80)"
        );
    });
}

fn use_member_list_effect(signals: &ChatViewSignals) {
    let app_state = signals.app_state;
    let client_manager = signals.client_manager;
    let mut chat_data = signals.chat_data;

    use_effect(move || {
        let active_channel_id = app_state.read().nav.selected_channel.clone();
        let Some(active_channel_id) = active_channel_id else {
            chat_data.write().members = Vec::new();
            chat_data.write().active_group_members = Vec::new();
            return;
        };

        let selected_server = app_state.read().nav.selected_server.clone();
        let active_account_id = app_state.read().nav.active_account_id.clone();
        let is_group = active_channel_id.starts_with("group-");
        spawn(async move {
            let backend = if let Some(server_id) = selected_server {
                client_manager
                    .read()
                    .get_backend_for_server(&server_id)
                    .map(|(_, handle)| handle)
            } else if let Some(account_id) = active_account_id {
                client_manager.read().get_backend(&account_id)
            } else {
                None
            };
            let Some(backend) = backend else {
                chat_data.write().members = Vec::new();
                chat_data.write().active_group_members = Vec::new();
                return;
            };
            let guard = backend.read().await;
            match guard.get_channel_members(&active_channel_id).await {
                Ok(members) => {
                    chat_data.write().members = members.clone();
                    chat_data.write().active_group_members =
                        if is_group { members } else { Vec::new() };
                }
                Err(err) => {
                    tracing::warn!(
                        "get_channel_members failed for channel {}: {}",
                        active_channel_id,
                        err
                    );
                    chat_data.write().members = Vec::new();
                    chat_data.write().active_group_members = Vec::new();
                }
            }
        });
    });
}

fn use_search_messages_effect(signals: &ChatViewSignals, ctx: &ChatViewMarkupCtx) {
    let app_state = signals.app_state;
    let client_manager = signals.client_manager;
    let mut search_hits = signals.search_hits;
    let utility_panel = signals.utility_panel;
    let search_query = signals.search_query;
    let current_channel = ctx.current_channel.clone();
    let current_server = ctx.current_server.clone();
    let self_user_id = ctx.self_user_id.clone();
    let is_dm_channel = ctx.is_dm_channel;
    let is_group_channel = ctx.is_group_channel;

    use_effect(move || {
        if *utility_panel.read() != Some(ChatUtilityPanel::Search) {
            return;
        }
        let raw_query = search_query.read().trim().to_string();
        if raw_query.is_empty() {
            search_hits.set(Vec::new());
            return;
        }
        let account_id = app_state.read().nav.active_account_id.clone();
        let Some(account_id) = account_id else {
            search_hits.set(Vec::new());
            return;
        };
        let parsed_query = build_search_query(
            raw_query,
            current_channel.clone(),
            current_server.clone(),
            self_user_id.clone(),
            is_dm_channel,
            is_group_channel,
        );
        spawn(async move {
            let Some(backend) = client_manager.read().get_backend(&account_id) else {
                search_hits.set(Vec::new());
                return;
            };
            let guard = backend.read().await;
            match guard.search_messages(parsed_query).await {
                Ok(hits) => search_hits.set(hits),
                Err(err) => {
                    tracing::warn!("search_messages failed: {err}");
                    search_hits.set(Vec::new());
                }
            }
        });
    });
}

fn use_pinned_messages_effect(signals: &ChatViewSignals) {
    let app_state = signals.app_state;
    let client_manager = signals.client_manager;
    let mut pinned_messages = signals.pinned_messages;
    let utility_panel = signals.utility_panel;

    use_effect(move || {
        if *utility_panel.read() != Some(ChatUtilityPanel::Pinned) {
            return;
        }
        let Some(target_channel_id) = app_state.read().nav.selected_channel.clone() else {
            pinned_messages.set(Vec::new());
            return;
        };
        let selected_server = app_state.read().nav.selected_server.clone();
        let active_account_id = app_state.read().nav.active_account_id.clone();
        spawn(async move {
            let backend = if let Some(server_id) = selected_server {
                client_manager
                    .read()
                    .get_backend_for_server(&server_id)
                    .map(|(_, handle)| handle)
            } else if let Some(account_id) = active_account_id {
                client_manager.read().get_backend(&account_id)
            } else {
                None
            };
            let Some(backend) = backend else {
                pinned_messages.set(Vec::new());
                return;
            };
            let guard = backend.read().await;
            match guard.get_pinned_messages(&target_channel_id).await {
                Ok(messages) => pinned_messages.set(messages),
                Err(err) => {
                    tracing::warn!("get_pinned_messages failed: {err}");
                    pinned_messages.set(Vec::new());
                }
            }
        });
    });
}

fn use_history_state_effect(signals: &ChatViewSignals) {
    let app_state = signals.app_state;
    let chat_data = signals.chat_data;
    let mut history_state = signals.history_state;

    use_effect(move || {
        let Some(active_channel_id) = app_state.read().nav.selected_channel.clone() else {
            history_state.set(ChatHistoryUiState::default());
            return;
        };
        let chat_snapshot = chat_data.read().clone();
        if chat_snapshot.loading {
            return;
        }
        if history_state.read().channel_id.as_deref() == Some(&active_channel_id) {
            return;
        }
        let messages = chat_snapshot.messages.clone();
        let unread_count = current_channel_unread_count(
            Some(&active_channel_id),
            chat_snapshot.current_channel.as_ref(),
            &chat_snapshot.dm_channels,
        );
        history_state.set(ChatHistoryUiState {
            channel_id: Some(active_channel_id),
            has_more_before: !messages.is_empty(),
            loading_before: false,
            unread_count,
            unread_marker_message_id: unread_marker_message_id(&messages, unread_count),
        });
    });
}

fn use_member_list_preferences_effect(app_state: Signal<AppState>) {
    use_effect(move || {
        let server_member_list_open = app_state.read().nav.right_sidebar_visible;
        let dm_member_list_open = app_state.read().nav.dm_right_sidebar_visible;
        spawn(async move {
            persist_member_list_preferences(server_member_list_open, dm_member_list_open).await;
        });
    });
}

fn use_command_preload_effect(signals: &ChatViewSignals, channel_id: &Option<String>) {
    let app_state = signals.app_state;
    let client_manager = signals.client_manager;
    let mut command_suggestions = signals.command_suggestions;
    let mut show_command_popup = signals.show_command_popup;
    let cmd_channel_id = channel_id.clone();

    use_effect(move || {
        let Some(cid) = cmd_channel_id.clone() else {
            command_suggestions.set(Vec::new());
            show_command_popup.set(false);
            return;
        };
        let selected_server = app_state.read().nav.selected_server.clone();
        let active_account_id = app_state.read().nav.active_account_id.clone();
        spawn(async move {
            let backend = if let Some(server_id) = selected_server {
                client_manager
                    .read()
                    .get_backend_for_server(&server_id)
                    .map(|(_, handle)| handle)
            } else if let Some(account_id) = active_account_id {
                client_manager.read().get_backend(&account_id)
            } else {
                None
            };
            let Some(backend) = backend else {
                return;
            };
            let guard = backend.read().await;
            match guard.get_channel_commands(&cid).await {
                Ok(cmds) => command_suggestions.set(cmds),
                Err(err) => tracing::warn!("get_channel_commands failed: {err}"),
            }
        });
    });
}

fn use_unread_marker_visibility_effect(signals: &ChatViewSignals) {
    let mut unread_marker_on_screen = signals.unread_marker_on_screen;
    let history_state = signals.history_state;

    use_effect(move || {
        let unread_marker_id = history_state.read().unread_marker_message_id.clone();
        let unread_count = history_state.read().unread_count;

        // If no unread marker or no unread count, marker is not visible
        if unread_marker_id.is_none() || unread_count == 0 {
            unread_marker_on_screen.set(false);
            return;
        }

        // Check if the unread marker message element is visible in the viewport
        let marker_id = unread_marker_id.unwrap_or_default();
        let dom_id = format!("message-{marker_id}");
        let js = format!(
            "(() => {{ \
                const el = document.getElementById('{dom_id}'); \
                if (!el) {{ dioxus.send(false); return; }} \
                const rect = el.getBoundingClientRect(); \
                const isVisible = rect.top >= 0 && rect.bottom <= window.innerHeight; \
                dioxus.send(isVisible); \
            }})()"
        );
        let mut eval = document::eval(&js);
        spawn(async move {
            if let Ok(visible) = eval.recv::<bool>().await {
                unread_marker_on_screen.set(visible);
            }
        });
    });
}

#[derive(Clone)]
struct ChatViewMarkupCtx {
    app_state: Signal<AppState>,
    client_manager: Signal<ClientManager>,
    chat_data: Signal<ChatData>,
    channel_id: Option<String>,
    messages: Vec<Message>,
    current_channel: Option<Channel>,
    current_server: Option<poly_client::Server>,
    loading: bool,
    reaction_picker_id: Option<String>,
    group_members: Vec<poly_client::User>,
    search_query_input_value: String,
    search_query_value: String,
    is_dm_channel: bool,
    is_group_channel: bool,
    member_list_visible: bool,
    search_terms: Vec<String>,
    search_placeholder: String,
    compose_placeholder: String,
    search_filter_channel_name_onfocus: String,
    search_filter_channel_name_oninput: String,
    filtered_search_filter_options: Vec<SearchFilterOption>,
    unread_marker_id: Option<String>,
    unread_banner_visible: bool,
    unread_banner_count: String,
    unread_banner_time: String,
    unread_banner_date: String,
    unread_banner_channel_id: Option<String>,
    self_user_id: String,
    dm_user_avatar: Option<String>,
    search_hit_channel_id: Option<String>,
    pinned_hit_channel_id: Option<String>,
    search_hit_server: Option<poly_client::Server>,
    pinned_hit_server: Option<poly_client::Server>,
    pinned_hit_channel: Option<Channel>,
    nav_for_search: crate::ui::dioxus_router::Navigator,
    nav_for_pinned: crate::ui::dioxus_router::Navigator,
    message_input: Signal<String>,
    show_input_emoji: Signal<bool>,
    reaction_picker_msg: Signal<Option<String>>,
    drag_over: Signal<bool>,
    hovered_msg: Signal<Option<String>>,
    editing_msg_id: Signal<Option<String>>,
    edit_draft: Signal<String>,
    msg_context_menu: Signal<Option<MsgContextMenu>>,
    utility_panel: Signal<Option<ChatUtilityPanel>>,
    search_query: Signal<String>,
    search_hits: Signal<Vec<MessageSearchHit>>,
    pinned_messages: Signal<Vec<Message>>,
    notifications_muted: Signal<bool>,
    show_search_filters: Signal<bool>,
    active_search_filter_idx: Signal<usize>,
    pending_attachments: Signal<Vec<PendingAttachmentPreview>>,
    command_suggestions: Signal<Vec<ChatCommand>>,
    active_command_idx: Signal<usize>,
    show_command_popup: Signal<bool>,
    reply_target: Signal<Option<MessageReplyPreview>>,
    history_state: Signal<ChatHistoryUiState>,
    unread_marker_on_screen: Signal<bool>,
}

fn render_chat_view_markup(ctx: ChatViewMarkupCtx) -> Element {
    let mut drag_over = ctx.drag_over;
    let pending_attachments = ctx.pending_attachments;
    let is_drag_over = *drag_over.read();

    rsx! {
        main {
            class: if is_drag_over { "chat-view drag-over" } else { "chat-view" },
            ondragover: move |evt| {
                evt.prevent_default();
                drag_over.set(true);
            },
            ondragleave: move |_| drag_over.set(false),
            ondrop: move |evt| {
                evt.prevent_default();
                drag_over.set(false);
                let files = evt.files();
                if !files.is_empty() {
                    spawn(async move {
                        append_attachment_previews(pending_attachments, files).await;
                    });
                }
            },

            {render_drag_overlay(is_drag_over)}
            {render_chat_layout_shell(ctx.clone())}
            {render_chat_overlays(ctx)}
        }
    }
}

fn render_drag_overlay(is_drag_over: bool) -> Element {
    if !is_drag_over {
        return rsx! {};
    }

    rsx! {
        div { class: "drag-overlay",
            div { class: "drag-overlay-content",
                span { class: "drag-icon", "📎" }
                p { "{t(\"chat-drop-files\")}" }
            }
        }
    }
}

fn render_chat_layout_shell(ctx: ChatViewMarkupCtx) -> Element {
    rsx! {
        div { class: "chat-layout-shell", {render_chat_main_column(ctx)} }
    }
}

fn render_chat_main_column(ctx: ChatViewMarkupCtx) -> Element {
    rsx! {
        div { class: "chat-main-column",
            {render_chat_header(ctx.clone())}
            {render_chat_body_shell(ctx)}
        }
    }
}

fn render_chat_header(ctx: ChatViewMarkupCtx) -> Element {
    rsx! {
        div { class: "chat-header",
            {render_chat_header_info(ctx.clone())}
            {render_chat_header_right(ctx)}
        }
    }
}

fn render_chat_header_info(ctx: ChatViewMarkupCtx) -> Element {
    let current_channel = ctx.current_channel.clone();
    let current_server = ctx.current_server.clone();
    let dm_user_avatar = ctx.dm_user_avatar.clone();
    let is_dm_channel = ctx.is_dm_channel;
    let is_group_channel = ctx.is_group_channel;
    let group_count = ctx.group_members.len();

    rsx! {
        if let Some(ref ch) = current_channel {
            if is_dm_channel {
                div { class: "dm-chat-header-info",
                    if let Some(ref avatar) = dm_user_avatar {
                        img {
                            class: "dm-chat-avatar",
                            src: "{avatar}",
                            alt: "{ch.name}",
                        }
                    } else {
                        div {
                            class: "dm-chat-avatar",
                            style: "background:{user_color(&ch.id)}",
                            "{ch.name.chars().next().unwrap_or('?')}"
                        }
                    }
                    div { class: "dm-chat-header-text",
                        span { class: "chat-channel-name", "{ch.name}" }
                        span { class: "chat-header-subtitle", {t("dm-header-subtitle")} }
                    }
                }
            } else if is_group_channel {
                div { class: "dm-chat-header-info",
                    div { class: "group-chat-icon", "👥" }
                    div { class: "dm-chat-header-text",
                        span { class: "chat-channel-name", "{ch.name}" }
                        span { class: "chat-header-subtitle",
                            {format!("{} {}", group_count, t("group-members-title"))}
                        }
                    }
                }
            } else {
                div { class: "server-chat-header-info",
                    span { class: "chat-channel-name", "# {ch.name}" }
                    if let Some(ref server) = current_server {
                        span { class: "chat-source-badge",
                            "{backend_badge(&server.backend)} {server.backend.display_name()}"
                        }
                    }
                }
            }
        } else {
            span { class: "chat-channel-name", {t("chat-no-messages")} }
        }
    }
}

fn render_chat_header_right(ctx: ChatViewMarkupCtx) -> Element {
    rsx! {
        div { class: "chat-header-right",
            {render_chat_header_actions(ctx.clone())}
            {render_chat_header_search(ctx)}
        }
    }
}

fn render_chat_header_actions(ctx: ChatViewMarkupCtx) -> Element {
    let app_state = ctx.app_state;
    let mut utility_panel = ctx.utility_panel;
    let mut notifications_muted = ctx.notifications_muted;
    let mut show_search_filters = ctx.show_search_filters;
    let is_group_channel = ctx.is_group_channel;
    let is_dm_channel = ctx.is_dm_channel;

    rsx! {
        div { class: "chat-header-actions",
            button {
                class: if *utility_panel.read() == Some(ChatUtilityPanel::Threads) { "header-btn active" } else { "header-btn" },
                title: t("threads"),
                onclick: move |_| {
                    show_search_filters.set(false);
                    let next = if *utility_panel.read() == Some(ChatUtilityPanel::Threads) {
                        None
                    } else {
                        Some(ChatUtilityPanel::Threads)
                    };
                    utility_panel.set(next);
                },
                "🧵"
            }
            button {
                class: if *utility_panel.read() == Some(ChatUtilityPanel::Pinned) { "header-btn active" } else { "header-btn" },
                title: t("pinned-messages"),
                onclick: move |_| {
                    show_search_filters.set(false);
                    let next = if *utility_panel.read() == Some(ChatUtilityPanel::Pinned) {
                        None
                    } else {
                        Some(ChatUtilityPanel::Pinned)
                    };
                    utility_panel.set(next);
                },
                "📌"
            }
            button {
                class: if *notifications_muted.read() { "header-btn active" } else { "header-btn" },
                title: if *notifications_muted.read() { t("unmute-notifications") } else { t("mute-notifications") },
                onclick: move |_| {
                    let current = *notifications_muted.read();
                    notifications_muted.set(!current);
                },
                "🔔"
            }
            {
                render_member_toggle_button(
                    app_state,
                    utility_panel,
                    show_search_filters,
                    is_group_channel,
                    is_dm_channel,
                )
            }
        }
    }
}

fn render_member_toggle_button(
    mut app_state: Signal<AppState>,
    mut utility_panel: Signal<Option<ChatUtilityPanel>>,
    mut show_search_filters: Signal<bool>,
    is_group_channel: bool,
    is_dm_channel: bool,
) -> Element {
    if is_group_channel {
        return rsx! {
            button {
                class: if app_state.read().nav.dm_right_sidebar_visible { "header-btn soft-active" } else { "header-btn" },
                title: t("chat-toggle-members"),
                onclick: move |_| {
                    let current = app_state.read().nav.dm_right_sidebar_visible;
                    app_state.write().nav.dm_right_sidebar_visible = !current;
                    utility_panel.set(None);
                    show_search_filters.set(false);
                },
                "👥"
            }
        };
    }

    if is_dm_channel {
        return rsx! {
            button {
                class: if app_state.read().nav.dm_right_sidebar_visible { "header-btn soft-active" } else { "header-btn" },
                title: t("chat-toggle-contact"),
                onclick: move |_| {
                    let current = app_state.read().nav.dm_right_sidebar_visible;
                    app_state.write().nav.dm_right_sidebar_visible = !current;
                    utility_panel.set(None);
                    show_search_filters.set(false);
                },
                "👤"
            }
        };
    }

    rsx! {
        button {
            class: if app_state.read().nav.right_sidebar_visible { "header-btn soft-active" } else { "header-btn" },
            title: t("chat-toggle-members"),
            onclick: move |_| {
                let current = app_state.read().nav.right_sidebar_visible;
                app_state.write().nav.right_sidebar_visible = !current;
                utility_panel.set(None);
                show_search_filters.set(false);
            },
            "👥"
        }
    }
}

fn render_chat_header_search(ctx: ChatViewMarkupCtx) -> Element {
    let search_placeholder = ctx.search_placeholder.clone();
    let search_query_input_value = ctx.search_query_input_value.clone();
    let search_query_value = ctx.search_query_value.clone();
    let filtered_search_filter_options = ctx.filtered_search_filter_options.clone();
    let search_filter_channel_name_onfocus = ctx.search_filter_channel_name_onfocus.clone();
    let search_filter_channel_name_oninput = ctx.search_filter_channel_name_oninput.clone();
    let mut search_query = ctx.search_query;
    let mut search_hits = ctx.search_hits;
    let mut active_search_filter_idx = ctx.active_search_filter_idx;
    let mut show_search_filters = ctx.show_search_filters;
    let mut utility_panel = ctx.utility_panel;

    rsx! {
        div { class: "chat-header-search-inline",
            span { class: "chat-header-search-icon", "🔎" }
            input {
                class: "chat-header-search-input",
                r#type: "text",
                placeholder: "{search_placeholder}",
                value: "{search_query_input_value}",
                onfocus: move |_| {
                    let raw = search_query.read().clone();
                    let has_matches = !filter_search_filter_options(
                            &build_search_filter_options(&search_filter_channel_name_onfocus),
                            &raw,
                        )
                        .is_empty();
                    active_search_filter_idx.set(0);
                    show_search_filters.set(has_matches);
                    if !raw.trim().is_empty() {
                        utility_panel.set(Some(ChatUtilityPanel::Search));
                    }
                },
                oninput: move |evt| {
                    let next_value = evt.value();
                    let is_empty = next_value.trim().is_empty();
                    let has_matches = !filter_search_filter_options(
                            &build_search_filter_options(&search_filter_channel_name_oninput),
                            &next_value,
                        )
                        .is_empty();
                    search_query.set(next_value);
                    active_search_filter_idx.set(0);
                    show_search_filters.set(has_matches);
                    if is_empty {
                        utility_panel.set(None);
                        search_hits.set(Vec::new());
                    } else {
                        utility_panel.set(Some(ChatUtilityPanel::Search));
                    }
                },
                onkeydown: move |evt: KeyboardEvent| {
                    handle_search_filter_keydown(
                        evt,
                        filtered_search_filter_options.clone(),
                        search_query,
                        active_search_filter_idx,
                        show_search_filters,
                        utility_panel,
                    );
                },
            }
            {
                render_search_clear_button(
                    search_query_value,
                    search_query,
                    search_hits,
                    active_search_filter_idx,
                    utility_panel,
                    show_search_filters,
                )
            }
            if *show_search_filters.read() && !filtered_search_filter_options.is_empty() {
                SearchFilterPopup {
                    suggestions: filtered_search_filter_options.clone(),
                    active_index: *active_search_filter_idx.read(),
                    on_append_filter: move |token: String| {
                        let next_value = apply_search_filter_completion(&search_query.read(), &token);
                        search_query.set(next_value);
                        active_search_filter_idx.set(0);
                        show_search_filters.set(false);
                        utility_panel.set(Some(ChatUtilityPanel::Search));
                    },
                    on_close: move |_| show_search_filters.set(false),
                }
            }
        }
    }
}

fn handle_search_filter_keydown(
    evt: KeyboardEvent,
    filtered_search_filter_options: Vec<SearchFilterOption>,
    mut search_query: Signal<String>,
    mut active_search_filter_idx: Signal<usize>,
    mut show_search_filters: Signal<bool>,
    mut utility_panel: Signal<Option<ChatUtilityPanel>>,
) {
    if filtered_search_filter_options.is_empty() || !*show_search_filters.read() {
        if evt.key() == Key::Escape {
            show_search_filters.set(false);
        }
        return;
    }

    let item_count = filtered_search_filter_options.len();
    match evt.key() {
        Key::ArrowDown => {
            evt.prevent_default();
            let next = (*active_search_filter_idx.read() + 1) % item_count;
            active_search_filter_idx.set(next);
        }
        Key::ArrowUp => {
            evt.prevent_default();
            let current = *active_search_filter_idx.read();
            let next = if current == 0 {
                item_count - 1
            } else {
                current - 1
            };
            active_search_filter_idx.set(next);
        }
        Key::Enter | Key::Tab => {
            evt.prevent_default();
            let current = (*active_search_filter_idx.read()).min(item_count - 1);
            if let Some(option) = filtered_search_filter_options.get(current) {
                let existing_query = search_query.read().clone();
                let next_query =
                    apply_search_filter_completion(&existing_query, &option.completion_token);
                search_query.set(next_query);
                active_search_filter_idx.set(0);
                show_search_filters.set(false);
                utility_panel.set(Some(ChatUtilityPanel::Search));
            }
        }
        Key::Escape => {
            evt.prevent_default();
            show_search_filters.set(false);
        }
        _ => {}
    }
}

fn render_search_clear_button(
    search_query_value: String,
    mut search_query: Signal<String>,
    mut search_hits: Signal<Vec<MessageSearchHit>>,
    mut active_search_filter_idx: Signal<usize>,
    mut utility_panel: Signal<Option<ChatUtilityPanel>>,
    mut show_search_filters: Signal<bool>,
) -> Element {
    if search_query_value.is_empty() {
        return rsx! {};
    }

    rsx! {
        button {
            class: "chat-header-search-clear",
            title: t("action-close"),
            onclick: move |_| {
                search_query.set(String::new());
                search_hits.set(Vec::new());
                active_search_filter_idx.set(0);
                utility_panel.set(None);
                show_search_filters.set(true);
            },
            "✕"
        }
    }
}

fn render_chat_body_shell(ctx: ChatViewMarkupCtx) -> Element {
    let show_side_column = ctx.utility_panel.read().is_some() || ctx.member_list_visible;

    rsx! {
        div { class: "chat-body-shell",
            {render_chat_content_column(ctx.clone())}
            if show_side_column {
                {render_chat_side_column(ctx)}
            }
        }
    }
}

fn render_chat_content_column(ctx: ChatViewMarkupCtx) -> Element {
    rsx! {
        div { class: "chat-content-column",
            {render_message_list(ctx.clone())}
            TypingIndicator {}
            {render_message_input_area(ctx)}
        }
    }
}

fn render_message_list(ctx: ChatViewMarkupCtx) -> Element {
    let loading = ctx.loading;
    let app_state = ctx.app_state;
    let client_manager = ctx.client_manager;
    let chat_data = ctx.chat_data;
    let mut history_state = ctx.history_state;

    rsx! {
        div {
            class: "message-list",
            id: "message-list-scroll",
            onscroll: move |_| {
                if loading || !history_state.read().has_more_before
                    || history_state.read().loading_before
                {
                    return;
                }
                history_state.write().loading_before = true;
                spawn(async move {
                    load_older_messages(app_state, client_manager, chat_data, history_state)
                        .await;
                });
            },
            {render_message_list_content(ctx)}
        }
    }
}

async fn load_older_messages(
    app_state: Signal<AppState>,
    client_manager: Signal<ClientManager>,
    mut chat_data: Signal<ChatData>,
    mut history_state: Signal<ChatHistoryUiState>,
) {
    let mut eval = document::eval(
        r#"
            let el = document.getElementById('message-list-scroll');
            if (el && el.scrollTop < 100) { dioxus.send(true); }
            else { dioxus.send(false); }
        "#,
    );
    let Ok(near_top) = eval.recv::<bool>().await else {
        history_state.write().loading_before = false;
        return;
    };
    if !near_top {
        history_state.write().loading_before = false;
        return;
    }

    let Some(active_channel_id) = app_state.read().nav.selected_channel.clone() else {
        history_state.write().loading_before = false;
        return;
    };
    let Some(before_message_id) = chat_data
        .read()
        .messages
        .first()
        .map(|message| message.id.clone())
    else {
        history_state.write().loading_before = false;
        history_state.write().has_more_before = false;
        return;
    };
    let Some((previous_scroll_top, previous_scroll_height)) =
        read_message_list_scroll_metrics().await
    else {
        history_state.write().loading_before = false;
        return;
    };
    let backend = if let Some(server_id) = app_state.read().nav.selected_server.clone() {
        client_manager
            .read()
            .get_backend_for_server(&server_id)
            .map(|(_, handle)| handle)
    } else if let Some(account_id) = app_state.read().nav.active_account_id.clone() {
        client_manager.read().get_backend(&account_id)
    } else {
        None
    };
    let Some(backend) = backend else {
        history_state.write().loading_before = false;
        return;
    };

    let older_messages = {
        let guard = backend.read().await;
        guard
            .get_messages(
                &active_channel_id,
                MessageQuery {
                    before: Some(before_message_id),
                    limit: Some(OLDER_MESSAGES_PAGE_SIZE),
                    ..Default::default()
                },
            )
            .await
            .unwrap_or_default()
    };
    if older_messages.is_empty() {
        history_state.write().loading_before = false;
        history_state.write().has_more_before = false;
        return;
    }

    let mut merged_messages = older_messages.clone();
    merged_messages.extend(chat_data.read().messages.clone());
    chat_data.write().messages = merged_messages;
    request_preserve_scroll_position(previous_scroll_top, previous_scroll_height);
    history_state.write().loading_before = false;
    history_state.write().has_more_before =
        u32::try_from(older_messages.len()).unwrap_or(0) >= OLDER_MESSAGES_PAGE_SIZE;
}

fn render_message_list_content(ctx: ChatViewMarkupCtx) -> Element {
    if ctx.loading {
        return rsx! {
            div { class: "message-loading", "{t(\"chat-loading\")}" }
        };
    }

    if ctx.messages.is_empty() {
        return rsx! {
            div { class: "message-empty",
                div { class: "empty-wave", "👋" }
                h3 { "{t(\"chat-no-messages\")}" }
            }
        };
    }

    rsx! {
        if ctx.history_state.read().loading_before {
            div { class: "message-history-loader", "{t(\"chat-loading-earlier\")}" }
        }
        {render_unread_banner(ctx.clone())}
        for (idx , msg) in ctx.messages.iter().enumerate() {
            {
                let prev_msg = if idx > 0 { ctx.messages.get(idx - 1).cloned() } else { None };
                render_message_row(ctx.clone(), msg.clone(), prev_msg)
            }
        }
    }
}

fn render_unread_banner(ctx: ChatViewMarkupCtx) -> Element {
    // Only show the banner if there are unread messages AND the unread marker is not visible on screen
    if !ctx.unread_banner_visible || *ctx.unread_marker_on_screen.read() {
        return rsx! {};
    }

    let mut chat_data = ctx.chat_data;
    let mut history_state = ctx.history_state;
    let unread_banner_channel_id = ctx.unread_banner_channel_id.clone();
    let unread_banner_count = ctx.unread_banner_count.clone();
    let unread_banner_time = ctx.unread_banner_time.clone();
    let unread_banner_date = ctx.unread_banner_date.clone();

    rsx! {
        div { class: "chat-unread-banner",
            div { class: "chat-unread-banner-text",
                "{t_args(\"chat-unread-banner\", &[(\"count\", unread_banner_count.as_str()), (\"time\", unread_banner_time.as_str()), (\"date\", unread_banner_date.as_str())])}"
            }
            button {
                class: "chat-unread-banner-action",
                onclick: move |_| {
                    if let Some(active_channel_id) = unread_banner_channel_id.clone() {
                        let _ = mark_channel_as_read(&mut chat_data, &active_channel_id);
                        history_state.write().unread_count = 0;
                        history_state.write().unread_marker_message_id = None;
                    }
                },
                "{t(\"notifications-mark-read\")}"
            }
        }
    }
}

fn render_message_row(ctx: ChatViewMarkupCtx, msg: Message, prev_msg: Option<Message>) -> Element {
    let show_date_sep = match prev_msg.as_ref() {
        Some(prev) => msg.timestamp.date_naive() != prev.timestamp.date_naive(),
        None => true,
    };
    let is_grouped = match prev_msg.as_ref() {
        Some(prev) => {
            prev.author.id == msg.author.id
                && !show_date_sep
                && (msg.timestamp - prev.timestamp).num_minutes() < GROUP_THRESHOLD_MINUTES
        }
        None => false,
    };

    let msg_id = msg.id.clone();
    let time_str = format_timestamp(msg.timestamp);
    let date_str = if show_date_sep {
        msg.timestamp.format("%B %d, %Y").to_string()
    } else {
        String::new()
    };
    let unread_count = ctx.history_state.read().unread_count;
    let unread_marker_id = ctx.unread_marker_id.clone();
    let hovered_msg_signal = ctx.hovered_msg;
    let hovered_msg_signal_leave = ctx.hovered_msg;
    let msg_context_menu_signal = ctx.msg_context_menu;
    let is_hovered = ctx.hovered_msg.read().as_deref() == Some(&msg_id);
    let is_own = msg.author.id == ctx.self_user_id;
    let is_editing = ctx.editing_msg_id.read().as_deref() == Some(&msg_id);
    let context_menu_text = message_plain_text(&msg.content);
    let msg_for_actions = msg.clone();
    let msg_for_grouped = msg.clone();

    rsx! {
        if show_date_sep {
            div { class: "date-separator",
                span { class: "date-separator-text", "{date_str}" }
            }
        }
        if unread_marker_id.as_deref() == Some(msg_id.as_str()) && unread_count > 0 {
            div { class: "message-unread-divider",
                div { class: "message-unread-divider-line" }
                span { class: "message-unread-divider-label", "{t(\"chat-unread-divider\")}" }
            }
        }
        div {
            id: "message-{msg_id}",
            class: if is_grouped { "message message-grouped" } else { "message message-full" },
            onmouseenter: {
                let mut hovered_msg = hovered_msg_signal;
                let mid = msg_id.clone();
                move |_| hovered_msg.set(Some(mid.clone()))
            },
            onmouseleave: {
                let mut hovered_msg = hovered_msg_signal_leave;
                move |_| hovered_msg.set(None)
            },
            oncontextmenu: {
                let mut msg_context_menu = msg_context_menu_signal;
                let mid = msg_id.clone();
                let txt = context_menu_text.clone();
                move |evt: MouseEvent| {
                    evt.prevent_default();
                    let coords = evt.client_coordinates();
                    msg_context_menu
                        .set(
                            Some(MsgContextMenu {
                                x: coords.x,
                                y: coords.y,
                                message_id: mid.clone(),
                                message_text: txt.clone(),
                                is_own,
                            }),
                        );
                }
            },

            {
                render_message_actions(
                    ctx.clone(),
                    msg_for_actions,
                    is_hovered,
                    is_editing,
                    is_own,
                )
            }
            if is_grouped {
                {render_grouped_message_body(ctx, msg_for_grouped, time_str, is_editing)}
            } else {
                {render_full_message_body(ctx, msg, time_str, is_editing)}
            }
        }
    }
}

fn render_message_actions(
    ctx: ChatViewMarkupCtx,
    msg: Message,
    is_hovered: bool,
    is_editing: bool,
    is_own: bool,
) -> Element {
    if !is_hovered || is_editing {
        return rsx! {};
    }

    let mut reaction_picker_msg = ctx.reaction_picker_msg;
    let mut edit_draft = ctx.edit_draft;
    let mut editing_msg_id = ctx.editing_msg_id;
    let mut reply_target = ctx.reply_target;
    let mut chat_data = ctx.chat_data;
    let msg_id = msg.id.clone();
    let ctx_text = message_plain_text(&msg.content);

    rsx! {
        div { class: "message-actions",
            button {
                class: "msg-action-btn",
                title: t("reaction-add"),
                onclick: {
                    let mid = msg_id.clone();
                    move |_| reaction_picker_msg.set(Some(mid.clone()))
                },
                "😀+"
            }
            if is_own {
                button {
                    class: "msg-action-btn",
                    title: t("msg-edit"),
                    onclick: {
                        let mid = msg_id.clone();
                        let txt = ctx_text.clone();
                        move |_| {
                            edit_draft.set(txt.clone());
                            editing_msg_id.set(Some(mid.clone()));
                        }
                    },
                    "✏️"
                }
                button {
                    class: "msg-action-btn msg-action-btn-danger",
                    title: t("msg-delete"),
                    onclick: {
                        let mid = msg_id.clone();
                        move |_| chat_data.write().messages.retain(|m| m.id != mid)
                    },
                    "🗑️"
                }
            }
            button {
                class: "msg-action-btn",
                title: t("msg-reply"),
                onclick: {
                    let preview = MessageReplyPreview {
                        message_id: msg.id.clone(),
                        author_id: msg.author.id.clone(),
                        author_display_name: msg.author.display_name.clone(),
                        author_avatar_url: msg.author.avatar_url.clone(),
                        snippet: reply_preview_snippet(&msg.content),
                    };
                    move |_| reply_target.set(Some(preview.clone()))
                },
                "↩️"
            }
            button {
                class: "msg-action-btn",
                title: t("msg-forward"),
                onclick: move |_| tracing::debug!("Forward (stub)"),
                "➡️"
            }
        }
    }
}

fn render_full_message_body(
    ctx: ChatViewMarkupCtx,
    msg: Message,
    time_str: String,
    is_editing: bool,
) -> Element {
    let color = user_color(&msg.author.id);
    let author_avatar = msg.author.avatar_url.clone();
    let first_char = msg
        .author
        .display_name
        .chars()
        .next()
        .map(|c| c.to_string())
        .unwrap_or_default();

    rsx! {
        if let Some(ref avatar) = author_avatar {
            img {
                class: "message-avatar message-avatar-img",
                src: "{avatar}",
                alt: "{first_char}",
            }
        } else {
            div { class: "message-avatar", style: "background-color: {color};", "{first_char}" }
        }
        div { class: "message-body",
            div { class: "message-header",
                span { class: "message-author", style: "color: {color};", "{msg.author.display_name}" }
                span { class: "message-timestamp", "{time_str}" }
            }
            {render_message_content_stack(ctx, msg, is_editing)}
        }
    }
}

fn render_grouped_message_body(
    ctx: ChatViewMarkupCtx,
    msg: Message,
    time_str: String,
    is_editing: bool,
) -> Element {
    rsx! {
        div { class: "message-gutter",
            span { class: "message-hover-time", "{time_str}" }
        }
        div { class: "message-body", {render_message_content_stack(ctx, msg, is_editing)} }
    }
}

fn render_message_content_stack(ctx: ChatViewMarkupCtx, msg: Message, is_editing: bool) -> Element {
    rsx! {
        if let Some(reply) = msg.reply_to.clone() {
            MessageReplyPreviewLine { reply }
        }
        if is_editing {
            MessageInlineEdit {
                message_id: msg.id.clone(),
                editing_msg_id: ctx.editing_msg_id,
                edit_draft: ctx.edit_draft,
                chat_data: ctx.chat_data,
            }
        } else {
            MessageContentView { content: msg.content.clone(), edited: msg.edited }
        }
        if !msg.attachments.is_empty() {
            AttachmentsView { attachments: msg.attachments.clone() }
        }
        if !msg.reactions.is_empty() {
            ReactionsView { reactions: msg.reactions.clone(), message_id: msg.id.clone() }
        }
    }
}

fn render_message_input_area(ctx: ChatViewMarkupCtx) -> Element {
    rsx! {
        div { class: "message-input-area",
            if ctx.channel_id.is_some() {
                {render_message_input_enabled(ctx)}
            } else {
                div { class: "message-input-disabled", {t("chat-select-channel")} }
            }
        }
    }
}

fn render_message_input_enabled(ctx: ChatViewMarkupCtx) -> Element {
    rsx! {
        if let Some(reply) = ctx.reply_target.read().clone() {
            ReplyComposerBar {
                reply,
                on_cancel: {
                    let mut reply_target = ctx.reply_target;
                    move |_| reply_target.set(None)
                },
            }
        }
        {render_attachment_preview_strip(ctx.clone())}
        {render_slash_command_popup(ctx.clone())}
        {render_message_input_row(ctx.clone())}
        {render_hidden_file_input(ctx.clone())}
        {render_input_emoji_picker(ctx)}
    }
}

fn render_attachment_preview_strip(ctx: ChatViewMarkupCtx) -> Element {
    let previews = ctx.pending_attachments.read().clone();
    if previews.is_empty() {
        return rsx! {};
    }

    let mut pending_attachments = ctx.pending_attachments;
    rsx! {
        div { class: "attachment-preview-strip",
            for preview in previews {
                div { class: "attachment-preview-card",
                    if let Some(ref preview_url) = preview.preview_url {
                        img {
                            class: "attachment-preview-image",
                            src: "{preview_url}",
                            alt: "{preview.filename}",
                        }
                    } else {
                        div { class: "attachment-preview-icon", "📎" }
                    }
                    div { class: "attachment-preview-meta",
                        span { class: "attachment-preview-name", "{preview.filename}" }
                        span { class: "attachment-preview-size", "{format_file_size(preview.size)}" }
                    }
                    button {
                        class: "attachment-preview-remove",
                        title: t("action-close"),
                        onclick: {
                            let preview_id = preview.id.clone();
                            move |_| pending_attachments.write().retain(|item| item.id != preview_id)
                        },
                        "✕"
                    }
                }
            }
        }
    }
}

fn render_slash_command_popup(ctx: ChatViewMarkupCtx) -> Element {
    let all_cmds = ctx.command_suggestions.read().clone();
    let text = ctx.message_input.read().clone();
    let query = slash_command_query(&text);
    let matches = if *ctx.show_command_popup.read() {
        filtered_slash_commands(query, &all_cmds)
    } else {
        Vec::new()
    };
    if matches.is_empty() {
        return rsx! {};
    }

    let mut message_input = ctx.message_input;
    let mut show_command_popup = ctx.show_command_popup;
    rsx! {
        SlashCommandPopup {
            commands: matches,
            active_idx: *ctx.active_command_idx.read(),
            on_select: move |filled: String| {
                message_input.set(filled);
                show_command_popup.set(false);
            },
        }
    }
}

fn render_message_input_row(ctx: ChatViewMarkupCtx) -> Element {
    let compose_placeholder = ctx.compose_placeholder.clone();
    let message_input = ctx.message_input;
    let show_input_emoji = ctx.show_input_emoji;
    let active_command_idx = ctx.active_command_idx;
    let show_command_popup = ctx.show_command_popup;
    let command_suggestions = ctx.command_suggestions;
    let pending_attachments = ctx.pending_attachments;
    let reply_target = ctx.reply_target;
    let channel_id = ctx.channel_id.clone();
    let client_manager = ctx.client_manager;
    let chat_data = ctx.chat_data;
    let app_state = ctx.app_state;
    let composer_runtime = ComposerRuntimeCtx {
        message_input,
        command_suggestions,
        active_command_idx,
        show_command_popup,
        pending_attachments,
        reply_target,
        client_manager,
        chat_data,
        app_state,
    };

    rsx! {
        div { class: "message-input-row",
            div { class: "message-input-shell",
                button {
                    class: "toolbar-btn composer-upload-btn",
                    title: t("chat-attach-file"),
                    onclick: move |_| open_composer_file_picker(),
                    "➕"
                }
                div { class: "message-input-text-area",
                    input {
                        class: "message-input",
                        id: "poly-message-composer",
                        r#type: "text",
                        placeholder: "{compose_placeholder}",
                        value: "{message_input}",
                        oninput: move |evt| {
                            handle_composer_input(
                                evt.value(),
                                message_input,
                                command_suggestions,
                                show_command_popup,
                                active_command_idx,
                            );
                        },
                        onkeydown: {
                            let channel_id_send = channel_id.clone();
                            move |evt: KeyboardEvent| {
                                handle_composer_keydown(evt, channel_id_send.clone(), composer_runtime);
                            }
                        },
                    }
                    {render_composer_toolbar(show_input_emoji)}
                }
            }
            {render_send_button(ctx)}
        }
    }
}

fn render_composer_toolbar(mut show_input_emoji: Signal<bool>) -> Element {
    rsx! {
        div { class: "input-toolbar input-toolbar-inline",
            button {
                class: "toolbar-btn",
                title: t("emoji-picker"),
                onclick: move |_| {
                    let current = *show_input_emoji.read();
                    show_input_emoji.set(!current);
                },
                "😀"
            }
            button { class: "toolbar-btn gif-btn", title: t("gif-picker"), "GIF" }
            button { class: "toolbar-btn", title: t("chat-markdown-formatting"), "⌘" }
        }
    }
}

fn open_composer_file_picker() {
    document::eval(
        r#"
            let input = document.getElementById('poly-file-input');
            if (input) { input.click(); }
        "#,
    );
}

fn handle_composer_input(
    value: String,
    mut message_input: Signal<String>,
    command_suggestions: Signal<Vec<ChatCommand>>,
    mut show_command_popup: Signal<bool>,
    mut active_command_idx: Signal<usize>,
) {
    message_input.set(value.clone());
    if value.trim_start().starts_with('/') && !value.trim_start()[1..].contains(' ') {
        let query = &value.trim_start()[1..];
        let all_cmds = command_suggestions.read().clone();
        let matches = filtered_slash_commands(query, &all_cmds);
        show_command_popup.set(!matches.is_empty());
        active_command_idx.set(0);
    } else {
        show_command_popup.set(false);
    }
}

#[derive(Clone, Copy)]
struct ComposerRuntimeCtx {
    message_input: Signal<String>,
    command_suggestions: Signal<Vec<ChatCommand>>,
    active_command_idx: Signal<usize>,
    show_command_popup: Signal<bool>,
    pending_attachments: Signal<Vec<PendingAttachmentPreview>>,
    reply_target: Signal<Option<MessageReplyPreview>>,
    client_manager: Signal<ClientManager>,
    chat_data: Signal<ChatData>,
    app_state: Signal<AppState>,
}

fn handle_composer_keydown(
    evt: KeyboardEvent,
    channel_id_send: Option<String>,
    ctx: ComposerRuntimeCtx,
) {
    let mut message_input = ctx.message_input;
    let mut pending_attachments = ctx.pending_attachments;
    let mut reply_target = ctx.reply_target;
    let show_command_popup = ctx.show_command_popup;

    if *show_command_popup.read() && handle_slash_popup_navigation(&evt, ctx) {
        return;
    }

    if evt.key() != Key::Enter || evt.modifiers().shift() {
        return;
    }
    evt.prevent_default();

    let raw_text = message_input.read().clone();
    let text = apply_builtin_command(raw_text.trim()).unwrap_or(raw_text);
    let attachments = pending_attachments.read().clone();
    let reply_to_message_id = reply_target
        .read()
        .as_ref()
        .map(|reply| reply.message_id.clone());
    if text.is_empty() && attachments.is_empty() {
        return;
    }

    message_input.set(String::new());
    pending_attachments.set(Vec::new());
    reply_target.set(None);
    if let Some(cid) = channel_id_send {
        spawn(async move {
            send_message(SendMessageCtx {
                channel_id: cid,
                text,
                attachments: attachments
                    .iter()
                    .map(pending_attachment_to_attachment)
                    .collect::<Vec<_>>(),
                reply_to_message_id,
                client_manager: ctx.client_manager,
                chat_data: ctx.chat_data,
                app_state: ctx.app_state,
            })
            .await;
        });
    }
}

fn handle_slash_popup_navigation(evt: &KeyboardEvent, ctx: ComposerRuntimeCtx) -> bool {
    let message_input = ctx.message_input;
    let command_suggestions = ctx.command_suggestions;
    let mut active_command_idx = ctx.active_command_idx;
    let mut show_command_popup = ctx.show_command_popup;

    match evt.key() {
        Key::ArrowUp => {
            evt.prevent_default();
            let cur = *active_command_idx.read();
            if cur > 0 {
                active_command_idx.set(cur - 1);
            }
            true
        }
        Key::ArrowDown => {
            evt.prevent_default();
            let all_cmds = command_suggestions.read().clone();
            let text = message_input.read().clone();
            let query = slash_command_query(&text);
            let matches = filtered_slash_commands(query, &all_cmds);
            let cur = *active_command_idx.read();
            if cur + 1 < matches.len() {
                active_command_idx.set(cur + 1);
            }
            true
        }
        Key::Escape => {
            evt.prevent_default();
            show_command_popup.set(false);
            true
        }
        Key::Tab | Key::Enter if !evt.modifiers().shift() => {
            evt.prevent_default();
            apply_selected_slash_command(ctx);
            true
        }
        _ => false,
    }
}

fn apply_selected_slash_command(ctx: ComposerRuntimeCtx) {
    let mut message_input = ctx.message_input;
    let command_suggestions = ctx.command_suggestions;
    let active_command_idx = ctx.active_command_idx;
    let mut show_command_popup = ctx.show_command_popup;
    let all_cmds = command_suggestions.read().clone();
    let text = message_input.read().clone();
    let query = slash_command_query(&text);
    let matches = filtered_slash_commands(query, &all_cmds);
    let idx = *active_command_idx.read();
    if let Some(cmd) = matches.get(idx) {
        message_input.set(format!("/{} ", cmd.name));
        show_command_popup.set(false);
    }
}

fn render_send_button(ctx: ChatViewMarkupCtx) -> Element {
    let channel_id = ctx.channel_id.clone();
    let mut message_input = ctx.message_input;
    let mut pending_attachments = ctx.pending_attachments;
    let mut reply_target = ctx.reply_target;
    let client_manager = ctx.client_manager;
    let chat_data = ctx.chat_data;
    let app_state = ctx.app_state;

    rsx! {
        button {
            class: "btn btn-send chat-send-inline",
            disabled: message_input.read().is_empty() && pending_attachments.read().is_empty(),
            onclick: move |_| {
                let text = message_input.read().clone();
                let attachments = pending_attachments.read().clone();
                let reply_to_message_id = reply_target
                    .read()
                    .as_ref()
                    .map(|reply| reply.message_id.clone());
                if text.is_empty() && attachments.is_empty() {
                    return;
                }
                message_input.set(String::new());
                pending_attachments.set(Vec::new());
                reply_target.set(None);
                if let Some(ref cid) = channel_id {
                    let cid = cid.clone();
                    let text = text.clone();
                    let attachments = attachments
                        .iter()
                        .map(pending_attachment_to_attachment)
                        .collect::<Vec<_>>();
                    spawn(async move {
                        send_message(SendMessageCtx {
                                channel_id: cid,
                                text,
                                attachments,
                                reply_to_message_id,
                                client_manager,
                                chat_data,
                                app_state,
                            })
                            .await;
                    });
                }
            },
            {t("chat-send")}
        }
    }
}

fn render_hidden_file_input(ctx: ChatViewMarkupCtx) -> Element {
    let pending_attachments = ctx.pending_attachments;
    rsx! {
        input {
            r#type: "file",
            id: "poly-file-input",
            multiple: true,
            style: "display:none;",
            onchange: move |_evt| {
                let files = _evt.files();
                if !files.is_empty() {
                    spawn(async move {
                        append_attachment_previews(pending_attachments, files).await;
                    });
                }
            },
        }
    }
}

fn render_input_emoji_picker(ctx: ChatViewMarkupCtx) -> Element {
    if !*ctx.show_input_emoji.read() {
        return rsx! {};
    }

    let mut message_input = ctx.message_input;
    let mut show_input_emoji = ctx.show_input_emoji;
    rsx! {
        EmojiPicker {
            on_select: move |emoji: String| {
                let current = message_input.read().clone();
                message_input.set(format!("{current}{emoji}"));
                show_input_emoji.set(false);
            },
            on_close: move |_| show_input_emoji.set(false),
        }
    }
}

fn render_chat_side_column(ctx: ChatViewMarkupCtx) -> Element {
    let current_channel_name = ctx
        .current_channel
        .as_ref()
        .map(|channel| channel.name.clone())
        .unwrap_or_default();
    let panel = *ctx.utility_panel.read();

    rsx! {
        div { class: "chat-side-column",
            if let Some(panel) = panel {
                {render_chat_utility_rail(ctx, panel, current_channel_name)}
            } else if ctx.is_dm_channel {
                DmContactPanel { channel_id: ctx.channel_id.clone().unwrap_or_default() }
            } else if ctx.is_group_channel {
                DmUserSidebar {}
            } else {
                UserSidebar {}
            }
        }
    }
}

fn render_chat_utility_rail(
    ctx: ChatViewMarkupCtx,
    panel: ChatUtilityPanel,
    current_channel_name: String,
) -> Element {
    let mut utility_panel = ctx.utility_panel;
    let search_query = ctx.search_query_value.clone();
    let search_terms = ctx.search_terms.clone();
    let search_hits = ctx.search_hits.read().clone();
    let pinned_messages = ctx.pinned_messages.read().clone();
    let search_hit_channel_id = ctx.search_hit_channel_id.clone();
    let search_hit_server = ctx.search_hit_server.clone();
    let pinned_hit_channel_id = ctx.pinned_hit_channel_id.clone();
    let pinned_hit_server = ctx.pinned_hit_server.clone();
    let pinned_hit_channel = ctx.pinned_hit_channel.clone();
    let nav_for_search = ctx.nav_for_search;
    let nav_for_pinned = ctx.nav_for_pinned;
    let client_manager = ctx.client_manager;
    let chat_data = ctx.chat_data;
    let app_state = ctx.app_state;

    rsx! {
        ChatUtilityRail {
            panel,
            search_query,
            search_hits,
            search_terms,
            pinned_messages,
            current_channel_name,
            on_open_search_hit: move |hit: MessageSearchHit| {
                let current_channel_id = search_hit_channel_id.clone();
                let current_server_id = search_hit_server
                    .as_ref()
                    .map(|server| server.id.clone());
                let nav = nav_for_search;
                spawn(async move {
                    if let Some((route, message_id)) = open_message_hit(
                            hit,
                            current_channel_id,
                            current_server_id,
                            client_manager,
                            chat_data,
                            app_state,
                        )
                        .await
                    {
                        nav.push(route);
                        highlight_message(&message_id);
                    }
                });
            },
            on_open_pinned: move |message: Message| {
                let Some(active_channel_id) = pinned_hit_channel_id.clone() else {
                    return;
                };
                let server_id = pinned_hit_server.as_ref().map(|server| server.id.clone());
                let hit = MessageSearchHit {
                    channel_id: active_channel_id.clone(),
                    channel_name: pinned_hit_channel
                        .as_ref()
                        .map(|channel| channel.name.clone()),
                    server_id,
                    message,
                };
                let current_server_id = pinned_hit_server
                    .as_ref()
                    .map(|server| server.id.clone());
                let nav = nav_for_pinned;
                spawn(async move {
                    if let Some((route, message_id)) = open_message_hit(
                            hit,
                            Some(active_channel_id),
                            current_server_id,
                            client_manager,
                            chat_data,
                            app_state,
                        )
                        .await
                    {
                        nav.push(route);
                        highlight_message(&message_id);
                    }
                });
            },
            on_close: move |_| utility_panel.set(None),
        }
    }
}

fn render_chat_overlays(ctx: ChatViewMarkupCtx) -> Element {
    let reaction_picker_id = ctx.reaction_picker_id.clone();
    let mut reaction_picker_msg = ctx.reaction_picker_msg;
    let msg_context_menu = ctx.msg_context_menu;
    let mut chat_data = ctx.chat_data;

    rsx! {
        if let Some(ref picker_msg_id) = reaction_picker_id {
            EmojiPicker {
                on_select: {
                    let msg_id = picker_msg_id.clone();
                    move |emoji: String| {
                        toggle_reaction_on_message(&mut chat_data, &msg_id, &emoji);
                        reaction_picker_msg.set(None);
                    }
                },
                on_close: move |_| reaction_picker_msg.set(None),
            }
        }
        if msg_context_menu.read().is_some() {
            MsgContextMenuOverlay { msg_context_menu, chat_data }
        }
    }
}

#[rustfmt::skip]
#[component]
fn ChatUtilityRail(
    panel: ChatUtilityPanel,
    search_query: String,
    search_hits: Vec<MessageSearchHit>,
    search_terms: Vec<String>,
    pinned_messages: Vec<Message>,
    current_channel_name: String,
    on_open_search_hit: EventHandler<MessageSearchHit>,
    on_open_pinned: EventHandler<Message>,
    on_close: EventHandler<()>,
) -> Element {
    let title = if panel == ChatUtilityPanel::Search {
        if search_query.is_empty() {
            t("search-messages")
        } else {
            format!("{} {}", search_hits.len(), t("search-results"))
        }
    } else if panel == ChatUtilityPanel::Pinned {
        t("pinned-messages")
    } else {
        t("threads")
    };
    let empty_label = if panel == ChatUtilityPanel::Pinned {
        format!("📌 {}", t("no-pinned-messages"))
    } else {
        format!("🧵 {}", t("no-threads"))
    };

    rsx! {
        aside { class: "chat-utility-rail",
            div { class: "chat-utility-header",
                h3 { "{title}" }
                button { class: "close-btn", onclick: move |_| on_close.call(()), "✕" }
            }
            if panel == ChatUtilityPanel::Search {
                div { class: "chat-utility-body",
                    if search_query.is_empty() || search_hits.is_empty() {
                        div { class: "utility-empty-state",
                            p { {t("search-no-results")} }
                        }
                    } else {
                        div { class: "search-results-list",
                            for hit in &search_hits {
                                SearchResultCard {
                                    hit: hit.clone(),
                                    search_terms: search_terms.clone(),
                                    on_open: move |hit| on_open_search_hit.call(hit),
                                }
                            }
                        }
                    }
                }
            } else if panel == ChatUtilityPanel::Pinned {
                div { class: "chat-utility-body",
                    if pinned_messages.is_empty() {
                        div { class: "utility-empty-state",
                            p { "{empty_label}" }
                        }
                    } else {
                        div { class: "search-results-list",
                            for message in &pinned_messages {
                                PinnedMessageCard {
                                    message: message.clone(),
                                    channel_name: current_channel_name.clone(),
                                    on_open: move |message| on_open_pinned.call(message),
                                }
                            }
                        }
                    }
                }
            } else {
                div { class: "chat-utility-body",
                    div { class: "utility-empty-state",
                        p { "{empty_label}" }
                    }
                }
            }
        }
    }
}

#[rustfmt::skip]
#[component]
fn SearchFilterPopup(
    suggestions: Vec<SearchFilterOption>,
    active_index: usize,
    on_append_filter: EventHandler<String>,
    on_close: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "search-filter-popup",
            div { class: "search-filter-popup-header",
                span { class: "search-filter-popup-title", {t("search-messages")} }
                button { class: "close-btn", onclick: move |_| on_close.call(()), "✕" }
            }
            div { class: "search-filter-list",
                for (index , suggestion) in suggestions.into_iter().enumerate() {
                    SearchFilterRow {
                        icon: suggestion.icon,
                        title: suggestion.title,
                        subtitle: suggestion.subtitle,
                        token: suggestion.completion_token,
                        selected: index == active_index,
                        on_click: move |token| on_append_filter.call(token),
                    }
                }
            }
        }
    }
}

#[rustfmt::skip]
#[component]
fn SearchFilterRow(
    icon: &'static str,
    title: String,
    subtitle: String,
    token: String,
    #[props(default)] selected: bool,
    on_click: EventHandler<String>,
) -> Element {
    rsx! {
        button {
            class: if selected { "search-filter-row selected" } else { "search-filter-row" },
            aria_selected: if selected { "true" } else { "false" },
            onclick: move |_| on_click.call(token.clone()),
            span { class: "search-filter-icon", "{icon}" }
            div { class: "search-filter-copy",
                div { class: "search-filter-title", "{title}" }
                div { class: "search-filter-subtitle", "{subtitle}" }
            }
        }
    }
}

#[rustfmt::skip]
#[component]
fn SearchResultCard(
    hit: MessageSearchHit,
    search_terms: Vec<String>,
    on_open: EventHandler<MessageSearchHit>,
) -> Element {
    let preview = message_plain_text(&hit.message.content);
    let preview_short = if preview.chars().count() > 140 {
        format!("{}…", preview.chars().take(140).collect::<String>())
    } else {
        preview
    };
    let timestamp = hit.message.timestamp.format("%d/%m/%Y, %H:%M").to_string();
    let avatar_url = hit.message.author.avatar_url.clone();
    let author_name = hit.message.author.display_name.clone();
    let fallback = author_name.chars().next().unwrap_or('?').to_string();
    let channel_label = hit
        .channel_name
        .clone()
        .unwrap_or_else(|| hit.channel_id.clone());

    rsx! {
        button {
            class: "search-result-card",
            onclick: move |_| on_open.call(hit.clone()),
            div { class: "search-result-channel", "# {channel_label}" }
            div { class: "search-result-content",
                div { class: "search-result-avatar",
                    if let Some(ref url) = avatar_url {
                        img {
                            class: "search-result-avatar-image",
                            src: "{url}",
                            alt: "{author_name}",
                        }
                    } else {
                        span { class: "search-result-avatar-fallback", "{fallback}" }
                    }
                }
                div { class: "search-result-copy",
                    div { class: "search-result-meta",
                        span { class: "search-result-author", "{author_name}" }
                        span { class: "search-result-time", "{timestamp}" }
                    }
                    div { class: "search-result-preview",
                        SearchPreviewText { text: preview_short, search_terms }
                    }
                }
            }
        }
    }
}

#[rustfmt::skip]
#[component]
fn PinnedMessageCard(
    message: Message,
    channel_name: String,
    on_open: EventHandler<Message>,
) -> Element {
    let preview = message_plain_text(&message.content);
    let preview_short = if preview.chars().count() > 140 {
        format!("{}…", preview.chars().take(140).collect::<String>())
    } else {
        preview
    };
    let timestamp = message.timestamp.format("%d/%m/%Y, %H:%M").to_string();
    let avatar_url = message.author.avatar_url.clone();
    let author_name = message.author.display_name.clone();
    let fallback = author_name.chars().next().unwrap_or('?').to_string();

    rsx! {
        button {
            class: "search-result-card",
            onclick: move |_| on_open.call(message.clone()),
            div { class: "search-result-channel", "# {channel_name}" }
            div { class: "search-result-content",
                div { class: "search-result-avatar",
                    if let Some(ref url) = avatar_url {
                        img {
                            class: "search-result-avatar-image",
                            src: "{url}",
                            alt: "{author_name}",
                        }
                    } else {
                        span { class: "search-result-avatar-fallback", "{fallback}" }
                    }
                }
                div { class: "search-result-copy",
                    div { class: "search-result-meta",
                        span { class: "search-result-author", "{author_name}" }
                        span { class: "search-result-time", "{timestamp}" }
                    }
                    div { class: "search-result-preview", "{preview_short}" }
                }
            }
        }
    }
}

#[rustfmt::skip]
#[component]
fn SearchPreviewText(text: String, search_terms: Vec<String>) -> Element {
    let lowercase_text = text.to_lowercase();
    let found_match = search_terms.into_iter().find_map(|term| {
        let lowercase_term = term.to_lowercase();
        lowercase_text
            .find(&lowercase_term)
            .map(|index| (index, index + lowercase_term.len()))
    });

    if let Some((start, end)) = found_match {
        let before = text.get(..start).unwrap_or_default().to_string();
        let matched = text.get(start..end).unwrap_or_default().to_string();
        let after = text.get(end..).unwrap_or_default().to_string();
        rsx! {
            span {
                "{before}"
                mark { class: "search-result-match", "{matched}" }
                "{after}"
            }
        }
    } else {
        rsx! {
            span { "{text}" }
        }
    }
}

fn looks_like_markdown(text: &str) -> bool {
    [
        "**", "__", "~~", "```", "# ", "- ", "* ", "> ", "|", "[", "](",
    ]
    .iter()
    .any(|marker| text.contains(marker))
}

fn render_markdown_html(text: &str) -> String {
    let mut options = pulldown_cmark::Options::empty();
    options.insert(pulldown_cmark::Options::ENABLE_STRIKETHROUGH);
    options.insert(pulldown_cmark::Options::ENABLE_TABLES);
    options.insert(pulldown_cmark::Options::ENABLE_TASKLISTS);
    options.insert(pulldown_cmark::Options::ENABLE_FOOTNOTES);
    options.insert(pulldown_cmark::Options::ENABLE_SMART_PUNCTUATION);

    let parser = pulldown_cmark::Parser::new_ext(text, options);
    let mut html_output = String::new();
    pulldown_cmark::html::push_html(&mut html_output, parser);

    let mut builder = ammonia::Builder::default();
    builder.add_tags([
        "table",
        "thead",
        "tbody",
        "tr",
        "th",
        "td",
        "pre",
        "code",
        "blockquote",
        "hr",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "ul",
        "ol",
        "li",
        "p",
        "em",
        "strong",
        "a",
    ]);
    builder.clean(&html_output).to_string()
}
/// Render message text content, handling multi-line and edited indicator.
#[rustfmt::skip]
#[component]
fn MessageContentView(content: MessageContent, edited: bool) -> Element {
    let text = match &content {
        MessageContent::Text(t) => t.clone(),
        MessageContent::WithAttachments { text, .. } => text.clone(),
    };
    let markdown_html = looks_like_markdown(&text).then(|| render_markdown_html(&text));

    rsx! {
        div { class: "message-text",
            if let Some(html) = markdown_html {
                div { class: "message-markdown", dangerous_inner_html: html }
            } else {
                for line in text.split('\n') {
                    if line.is_empty() {
                        br {}
                    } else {
                        p { class: "message-line", "{line}" }
                    }
                }
            }
            if edited {
                span { class: "message-edited", "{t(\"chat-edited\")}" }
            }
        }
    }
}

/// Render attachments (images inline, non-images as links).
#[rustfmt::skip]
#[component]
fn AttachmentsView(attachments: Vec<poly_client::Attachment>) -> Element {
    rsx! {
        div { class: "message-attachments",
            for att in &attachments {
                {
                    let is_image = att.content_type.starts_with("image/");
                    let filename = att.filename.clone();
                    let size_str = format_file_size(att.size);
                    let url = att.url.clone();

                    if is_image {
                        rsx! {
                            div { class: "attachment-image",
                                img { src: "{url}", alt: "{filename}", loading: "lazy" }
                                div { class: "attachment-info",
                                    span { class: "attachment-name", "{filename}" }
                                    span { class: "attachment-size", "— {size_str}" }
                                }
                            }
                        }
                    } else {
                        rsx! {
                            div { class: "attachment-file",
                                span { class: "attachment-file-icon", "📎" }
                                a { href: "{url}", target: "_blank", class: "attachment-file-link", "{filename}" }
                                span { class: "attachment-size", "— {size_str}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Render reaction pills (clickable to toggle).
#[rustfmt::skip]
#[component]
fn ReactionsView(reactions: Vec<poly_client::Reaction>, message_id: String) -> Element {
    let mut chat_data: Signal<ChatData> = use_context();
    rsx! {
        div { class: "message-reactions",
            for reaction in &reactions {
                {
                    let emoji = reaction.emoji.clone();
                    let count = reaction.count;
                    let me_class = if reaction.me { "reaction-pill me" } else { "reaction-pill" };
                    let emoji_click = emoji.clone();
                    let mid = message_id.clone();

                    rsx! {
                        button {
                            class: "{me_class}",
                            onclick: move |_| {
                                toggle_reaction_on_message(&mut chat_data, &mid, &emoji_click);
                            },
                            "{emoji} {count}"
                        }
                    }
                }
            }
        }
    }
}

/// Format a timestamp for display.
///
/// If today: "12:34 PM"
/// If yesterday: "Yesterday 12:34 PM"
/// Otherwise: "02/28/2026 12:34 PM"
fn format_timestamp(ts: chrono::DateTime<chrono::Utc>) -> String {
    let local = ts.with_timezone(&chrono::Local);
    let now = chrono::Local::now();

    if local.date_naive() == now.date_naive() {
        local.format("%I:%M %p").to_string()
    } else if local.date_naive() == (now - chrono::Duration::days(1)).date_naive() {
        format!("Yesterday {}", local.format("%I:%M %p"))
    } else {
        local.format("%m/%d/%Y %I:%M %p").to_string()
    }
}

/// Typing indicator shown above the message input when users are typing.
#[rustfmt::skip]
#[component]
fn TypingIndicator() -> Element {
    let chat_data: Signal<ChatData> = use_context();
    let typing = chat_data.read().typing_users.clone();

    if typing.is_empty() {
        return rsx! {};
    }

    let text = match typing.len() {
        1 => t("chat-typing").replace("{$user}", typing.first().map_or("", |s| s.as_str())),
        n => t("chat-typing-multiple").replace("{$count}", &n.to_string()),
    };

    rsx! {
        div { class: "typing-indicator",
            span { class: "typing-dots",
                span { class: "typing-dot" }
                span { class: "typing-dot" }
                span { class: "typing-dot" }
            }
            span { class: "typing-text", "{text}" }
        }
    }
}

/// Toggle a reaction on a message (add or remove).
///
/// If the reaction already exists and we've reacted, remove our reaction.
/// If it exists but we haven't reacted, add ours. Otherwise create a new reaction.
fn toggle_reaction_on_message(chat_data: &mut Signal<ChatData>, message_id: &str, emoji: &str) {
    let mut cd = chat_data.write();
    if let Some(msg) = cd.messages.iter_mut().find(|m| m.id == message_id) {
        if let Some(reaction) = msg.reactions.iter_mut().find(|r| r.emoji == emoji) {
            if reaction.me {
                // Remove our reaction
                reaction.count = reaction.count.saturating_sub(1);
                reaction.me = false;
                if reaction.count == 0 {
                    msg.reactions.retain(|r| r.emoji != emoji);
                }
            } else {
                // Add our reaction
                reaction.count += 1;
                reaction.me = true;
            }
        } else {
            // New reaction
            msg.reactions.push(poly_client::Reaction {
                emoji: emoji.to_string(),
                count: 1,
                me: true,
            });
        }
    }
}

/// Bundled parameters for [`send_message`] to avoid the too-many-arguments lint.
struct SendMessageCtx {
    channel_id: String,
    text: String,
    attachments: Vec<Attachment>,
    reply_to_message_id: Option<String>,
    client_manager: Signal<ClientManager>,
    chat_data: Signal<ChatData>,
    app_state: Signal<AppState>,
}

/// Send a message via the backend and prepend it to the message list.
async fn send_message(ctx: SendMessageCtx) {
    let SendMessageCtx {
        channel_id,
        text,
        attachments,
        reply_to_message_id,
        client_manager,
        mut chat_data,
        app_state,
    } = ctx;
    // Resolve the backend: server channels use server_id lookup; DM channels fall back to
    // active_account_id so messages still send when no server is selected.
    let backend = {
        let state = app_state.read();
        if let Some(ref server_id) = state.nav.selected_server {
            client_manager
                .read()
                .get_backend_for_server(server_id)
                .map(|(_id, b)| b)
        } else if let Some(ref account_id) = state.nav.active_account_id {
            client_manager.read().get_backend(account_id)
        } else {
            None
        }
    };

    let Some(backend) = backend else {
        tracing::warn!("send_message: no backend found for channel {channel_id}");
        return;
    };

    let guard = backend.read().await;
    let content = if attachments.is_empty() {
        MessageContent::Text(text)
    } else {
        MessageContent::WithAttachments { text, attachments }
    };
    let result = if let Some(reply_id) = reply_to_message_id {
        guard
            .send_reply_message(&channel_id, &reply_id, content)
            .await
    } else {
        guard.send_message(&channel_id, content).await
    };
    match result {
        Ok(msg) => {
            chat_data.write().messages.push(msg);
        }
        Err(e) => {
            tracing::error!("Failed to send message: {e}");
        }
    }
}

/// Apply an inline edit to a message in the chat data.
///
/// Sets `edited = true` on the message and replaces its content with the new text.
fn apply_edit(chat_data: &mut Signal<ChatData>, message_id: &str, new_text: String) {
    let mut cd = chat_data.write();
    if let Some(msg) = cd.messages.iter_mut().find(|m| m.id == message_id) {
        msg.content = MessageContent::Text(new_text);
        msg.edited = true;
    }
}

/// Inline edit UI rendered in place of the message content while editing.
///
/// Shows a textarea pre-filled with the current message text, a Cancel button,
/// and a Save button. Enter (without Shift) saves; Escape cancels.
#[rustfmt::skip]
#[component]
fn MessageInlineEdit(
    message_id: String,
    editing_msg_id: Signal<Option<String>>,
    edit_draft: Signal<String>,
    mut chat_data: Signal<ChatData>,
) -> Element {
    let mid_save = message_id.clone();
    rsx! {
        div { class: "message-inline-edit",
            textarea {
                class: "message-edit-input",
                value: "{edit_draft}",
                rows: "3",
                oninput: move |evt| edit_draft.set(evt.value()),
                onkeydown: {
                    let mid = mid_save.clone();
                    move |evt: KeyboardEvent| {
                        if evt.key() == Key::Enter && !evt.modifiers().shift() {
                            evt.prevent_default();
                            let new_text = edit_draft.read().clone();
                            apply_edit(&mut chat_data, &mid, new_text);
                            editing_msg_id.set(None);
                        } else if evt.key() == Key::Escape {
                            editing_msg_id.set(None);
                        }
                    }
                },
            }
            div { class: "message-edit-actions",
                span { class: "message-edit-hint",
                    "escape to "
                    button {
                        class: "message-edit-link-btn",
                        onclick: move |_| editing_msg_id.set(None),
                        "{t(\"msg-edit-cancel\")}"
                    }
                    " • enter to "
                    button {
                        class: "message-edit-link-btn message-edit-link-btn-save",
                        onclick: {
                            let mid = mid_save.clone();
                            move |_| {
                                let new_text = edit_draft.read().clone();
                                apply_edit(&mut chat_data, &mid, new_text);
                                editing_msg_id.set(None);
                            }
                        },
                        "{t(\"msg-edit-save\")}"
                    }
                }
            }
        }
    }
}

/// Quick-reaction emoji row shown at top of the message context menu.
const QUICK_REACTIONS: &[&str] = &["👍", "✅", "⚖️", "🔞"];

/// Right-click context menu overlay for messages.
///
/// Renders a transparent backdrop (closes on click) and a fixed-position
/// floating menu at the coordinates stored in `msg_context_menu`.
#[rustfmt::skip]
#[component]
fn MsgContextMenuOverlay(
    msg_context_menu: Signal<Option<MsgContextMenu>>,
    mut chat_data: Signal<ChatData>,
) -> Element {
    let Some(menu) = msg_context_menu.read().clone() else {
        return rsx! {};
    };

    let x = menu.x;
    let y = menu.y;
    let is_own = menu.is_own;
    let mid_delete = menu.message_id.clone();
    let mid_copy_id = menu.message_id.clone();
    let txt_copy = menu.message_text.clone();

    rsx! {
        div {
            class: "context-menu-backdrop",
            onclick: move |_| msg_context_menu.set(None),
            oncontextmenu: move |evt| evt.prevent_default(),
        }

        div {
            class: "context-menu msg-context-menu",
            style: "left: {x}px; top: {y}px;",
            onclick: move |evt| evt.stop_propagation(),

            {
                render_context_menu_quick_reactions(
                    menu.message_id.clone(),
                    msg_context_menu,
                    chat_data,
                )
            }
            div { class: "context-menu-separator" }
            ContextMenuItemSimple {
                label: t("reaction-add"),
                has_arrow: true,
                onclick: move |_| msg_context_menu.set(None),
            }

            {render_context_menu_stub_items(msg_context_menu)}
            {render_context_menu_copy_text_item(msg_context_menu, txt_copy)}
            div { class: "context-menu-separator" }

            {render_context_menu_danger_item(is_own, msg_context_menu, chat_data, mid_delete)}
            {render_context_menu_copy_id_item(msg_context_menu, mid_copy_id)}
        }
    }
}

fn render_context_menu_quick_reactions(
    message_id: String,
    mut msg_context_menu: Signal<Option<MsgContextMenu>>,
    mut chat_data: Signal<ChatData>,
) -> Element {
    rsx! {
        div { class: "msg-context-quick-reactions",
            for emoji in QUICK_REACTIONS {
                {
                    let e = emoji.to_string();
                    let mid = message_id.clone();
                    rsx! {
                        button {
                            class: "msg-context-quick-reaction-btn",
                            onclick: move |_| {
                                toggle_reaction_on_message(&mut chat_data, &mid, &e);
                                msg_context_menu.set(None);
                            },
                            "{emoji}"
                        }
                    }
                }
            }
        }
    }
}

fn render_context_menu_stub_items(mut msg_context_menu: Signal<Option<MsgContextMenu>>) -> Element {
    const STUB_ITEMS: &[(&str, &str)] = &[
        ("msg-reply", "↩"),
        ("msg-forward", "➡"),
        ("msg-apps", ""),
        ("msg-mark-unread", ""),
        ("msg-copy-link", ""),
        ("msg-speak", ""),
    ];

    rsx! {
        for (key , icon) in STUB_ITEMS {
            {
                let key = key.to_string();
                let icon_str = icon.to_string();
                rsx! {
                    ContextMenuItemSimple {
                        label: t(&key),
                        icon: icon_str,
                        onclick: move |_| {
                            tracing::debug!("{} (stub)", key);
                            msg_context_menu.set(None);
                        },
                    }
                }
            }
        }
    }
}

fn render_context_menu_copy_text_item(
    mut msg_context_menu: Signal<Option<MsgContextMenu>>,
    txt_copy: String,
) -> Element {
    rsx! {
        ContextMenuItemSimple {
            label: t("msg-copy-text"),
            onclick: move |_| {
                let js = format!(
                    "navigator.clipboard.writeText({}).catch(()=>{{}})",
                    serde_json::to_string(&txt_copy).unwrap_or_default(),
                );
                document::eval(&js);
                msg_context_menu.set(None);
            },
        }
    }
}

fn render_context_menu_danger_item(
    is_own: bool,
    mut msg_context_menu: Signal<Option<MsgContextMenu>>,
    mut chat_data: Signal<ChatData>,
    mid_delete: String,
) -> Element {
    if !is_own {
        return rsx! {
            ContextMenuItemSimple {
                label: t("msg-report"),
                danger: true,
                onclick: move |_| {
                    tracing::debug!("Report (stub)");
                    msg_context_menu.set(None);
                },
            }
        };
    }

    rsx! {
        ContextMenuItemSimple {
            label: t("msg-delete"),
            danger: true,
            onclick: move |_| {
                chat_data.write().messages.retain(|message| message.id != mid_delete);
                msg_context_menu.set(None);
            },
        }
    }
}

fn render_context_menu_copy_id_item(
    mut msg_context_menu: Signal<Option<MsgContextMenu>>,
    mid_copy_id: String,
) -> Element {
    rsx! {
        ContextMenuItemSimple {
            label: t("msg-copy-id"),
            onclick: move |_| {
                let js = format!(
                    "navigator.clipboard.writeText({}).catch(()=>{{}})",
                    serde_json::to_string(&mid_copy_id).unwrap_or_default(),
                );
                document::eval(&js);
                msg_context_menu.set(None);
            },
        }
    }
}

/// Simple context menu item button.
///
/// Renders a full-width button with optional right arrow, danger styling,
/// and a leading icon glyph.
#[rustfmt::skip]
#[component]
fn ContextMenuItemSimple(
    label: String,
    #[props(default)] icon: String,
    #[props(default)] has_arrow: bool,
    #[props(default)] danger: bool,
    onclick: EventHandler<MouseEvent>,
) -> Element {
    let class = if danger {
        "context-menu-item danger"
    } else {
        "context-menu-item"
    };
    rsx! {
        button { class: "{class}", onclick: move |evt| onclick.call(evt),
            if !icon.is_empty() {
                span { class: "context-menu-item-icon", "{icon}" }
            }
            span { class: "context-menu-item-label", "{label}" }
            if has_arrow {
                span { class: "context-menu-arrow", "›" }
            }
        }
    }
}

/// Small inline reply preview shown above a replied message.
#[rustfmt::skip]
#[component]
fn MessageReplyPreviewLine(reply: MessageReplyPreview) -> Element {
    rsx! {
        div { class: "message-reply-preview",
            span { class: "message-reply-arrow", "↪" }
            span { class: "message-reply-author", "{reply.author_display_name}" }
            span { class: "message-reply-snippet", "{reply.snippet}" }
        }
    }
}

/// Composer banner shown while replying to a message.
#[rustfmt::skip]
#[component]
fn ReplyComposerBar(reply: MessageReplyPreview, on_cancel: EventHandler<MouseEvent>) -> Element {
    rsx! {
        div { class: "reply-composer-bar",
            div { class: "reply-composer-main",
                div { class: "reply-composer-title",
                    {t_args("chat-replying-to", &[("name", reply.author_display_name.as_str())])}
                }
                div { class: "reply-composer-snippet", "{reply.snippet}" }
            }
            button {
                class: "reply-composer-close",
                title: t("action-close"),
                onclick: move |evt| on_cancel.call(evt),
                "✕"
            }
        }
    }
}

/// Slash command autocomplete popup rendered above the message input.
///
/// Shows filtered commands with provider badges. Highlighted item is driven by `active_idx`.
/// Clicking a command calls `on_select` with the filled command text (e.g. `"/play "`).
#[rustfmt::skip]
#[component]
fn SlashCommandPopup(
    commands: Vec<ChatCommand>,
    active_idx: usize,
    on_select: EventHandler<String>,
) -> Element {
    if commands.is_empty() {
        return rsx! {};
    }
    rsx! {
        div { class: "slash-command-popup",
            for (idx , cmd) in commands.iter().enumerate() {
                {
                    let cmd_name = cmd.name.clone();
                    let is_active = idx == active_idx;
                    let item_class = if is_active {
                        "slash-command-item selected"
                    } else {
                        "slash-command-item"
                    };
                    let provider_label = if cmd.is_builtin {
                        "Built-in".to_string()
                    } else {
                        cmd.provider.clone()
                    };
                    let usage_text = cmd.usage.clone().unwrap_or_default();
                    rsx! {
                        div {
                            class: "{item_class}",
                            id: if is_active { "slash-cmd-active" } else { "" },
                            onclick: move |_| on_select.call(format!("/{cmd_name} ")),
                            div { class: "slash-command-left",
                                span { class: "slash-command-name", "/{cmd.name}" }
                                if !usage_text.is_empty() {
                                    span { class: "slash-command-usage", " {usage_text}" }
                                }
                            }
                            div { class: "slash-command-right",
                                span { class: "slash-command-desc", "{cmd.description}" }
                                span { class: "slash-command-provider", "{provider_label}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Contact info panel shown on the right rail when a DM channel is active and the user
/// presses the 👤 header button.
///
/// Displays the remote user's avatar, display name, presence status, and backend badge.
#[rustfmt::skip]
#[component]
fn DmContactPanel(channel_id: String) -> Element {
    let chat_data: Signal<ChatData> = use_context();
    let mut app_state: Signal<AppState> = use_context();

    let dm: Option<DmChannel> = chat_data
        .read()
        .dm_channels
        .iter()
        .find(|dm| dm.id == channel_id)
        .cloned();

    let presence_class = dm.as_ref().map_or("status-dot presence-dot offline", |dm| {
        match dm.user.presence {
            PresenceStatus::Online => "status-dot presence-dot online",
            PresenceStatus::Idle => "status-dot presence-dot away",
            PresenceStatus::DoNotDisturb => "status-dot presence-dot dnd",
            PresenceStatus::Offline | PresenceStatus::Invisible => {
                "status-dot presence-dot offline"
            }
        }
    });

    rsx! {
        div { class: "dm-contact-panel",
            // Header with close button
            div { class: "dm-contact-panel-header",
                span { class: "dm-contact-panel-title", {t("dm-contact-panel-title")} }
                button {
                    class: "header-btn",
                    title: t("chat-toggle-contact"),
                    onclick: move |_| {
                        let current = app_state.read().nav.dm_right_sidebar_visible;
                        app_state.write().nav.dm_right_sidebar_visible = !current;
                    },
                    "✕"
                }
            }

            if let Some(ref dm) = dm {
                // Avatar section
                div { class: "dm-contact-avatar-section",
                    div { class: "dm-contact-avatar-wrap",
                        if let Some(ref url) = dm.user.avatar_url {
                            img {
                                class: "dm-contact-avatar",
                                src: "{url}",
                                alt: "{dm.user.display_name}",
                            }
                        } else {
                            div { class: "dm-contact-avatar dm-contact-avatar-fallback",
                                {dm.user.display_name.chars().next().unwrap_or('?').to_uppercase().to_string()}
                            }
                        }
                        span { class: "{presence_class}" }
                    }
                    div { class: "dm-contact-name", "{dm.user.display_name}" }
                    div { class: "dm-contact-presence",
                        match dm.user.presence {
                            PresenceStatus::Online => t("presence-online"),
                            PresenceStatus::Idle => t("presence-away"),
                            PresenceStatus::DoNotDisturb => t("presence-dnd"),
                            PresenceStatus::Offline | PresenceStatus::Invisible => t("presence-offline"),
                        }
                    }
                    // Backend badge
                    span { class: "account-backend-badge", {backend_badge(&dm.user.backend)} }
                }
            } else {
                div { class: "dm-contact-empty", {t("dm-contact-not-found")} }
            }
        }
    }
}
