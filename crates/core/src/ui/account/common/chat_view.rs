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
    Attachment, BackendType, ChatCommand, CommandScope, DmChannel, Message, MessageContent,
    MessageQuery, MessageSearchHit, MessageSearchQuery, PresenceStatus,
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

    let already_rendered = current_channel_id.as_deref() == Some(&target_channel_id)
        && chat_data
            .read()
            .messages
            .iter()
            .any(|message| message.id == target_message_id);
    if already_rendered {
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
        Some((
            Route::ServerChat {
                backend: backend_type.slug().to_string(),
                instance_id,
                account_id,
                server_id,
                channel_id: target_channel_id,
            },
            target_message_id,
        ))
    } else {
        app_state.write().nav.selected_server = None;
        Some((
            Route::DmChat {
                backend: backend_type.slug().to_string(),
                instance_id,
                account_id,
                dm_id: target_channel_id,
            },
            target_message_id,
        ))
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

#[component]
pub fn ChatView() -> Element {
    let mut app_state: Signal<AppState> = use_context();
    let client_manager: Signal<ClientManager> = use_context();
    let mut chat_data: Signal<ChatData> = use_context();
    let nav = navigator();
    let mut message_input = use_signal(String::new);
    let mut show_input_emoji = use_signal(|| false);
    let mut reaction_picker_msg = use_signal(|| None::<String>);
    let mut drag_over = use_signal(|| false);
    let mut hovered_msg = use_signal(|| None::<String>);
    let mut editing_msg_id = use_signal(|| None::<String>);
    let mut edit_draft = use_signal(String::new);
    let mut msg_context_menu = use_signal(|| None::<MsgContextMenu>);
    let mut utility_panel = use_signal(|| None::<ChatUtilityPanel>);
    let mut search_query = use_signal(String::new);
    let mut search_hits = use_signal(Vec::<MessageSearchHit>::new);
    let mut pinned_messages = use_signal(Vec::<Message>::new);
    let mut notifications_muted = use_signal(|| false);
    let mut show_search_filters = use_signal(|| false);
    let mut pending_attachments = use_signal(Vec::<PendingAttachmentPreview>::new);
    // Slash command popup state
    let mut command_suggestions = use_signal(Vec::<ChatCommand>::new);
    let mut active_command_idx = use_signal(|| 0_usize);
    let mut show_command_popup = use_signal(|| false);

    let channel_id = app_state.read().nav.selected_channel.clone();
    let messages = chat_data.read().messages.clone();
    let current_channel = chat_data.read().current_channel.clone();
    let current_server = chat_data.read().current_server.clone();
    let loading = chat_data.read().loading;
    let reaction_picker_id = reaction_picker_msg.read().clone();
    let group_members = chat_data.read().active_group_members.clone();
    let search_query_value = search_query.read().trim().to_string();
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
    let search_terms = message_search_terms(&search_query_value);
    let search_placeholder =
        contextual_search_placeholder(current_channel.as_ref(), is_dm_channel, is_group_channel);
    let compose_placeholder =
        contextual_compose_placeholder(current_channel.as_ref(), is_dm_channel, is_group_channel);

    let self_user_id: String = {
        let state = app_state.read();
        let cm = client_manager.read();
        state
            .nav
            .active_account_id
            .as_ref()
            .and_then(|aid| cm.sessions.get(aid))
            .map(|s| s.user.id.clone())
            .unwrap_or_default()
    };

    let dm_user_avatar: Option<String> = if is_dm_channel {
        let cid = channel_id.clone().unwrap_or_default();
        chat_data
            .read()
            .dm_channels
            .iter()
            .find(|dm| dm.id == cid)
            .and_then(|dm| dm.user.avatar_url.clone())
    } else {
        None
    };

    let search_effect_channel = current_channel.clone();
    let search_effect_server = current_server.clone();
    let search_effect_self_user_id = self_user_id.clone();
    let pinned_effect_channel_id = channel_id.clone();
    let search_hit_channel_id = channel_id.clone();
    let pinned_hit_channel_id = channel_id.clone();
    let search_hit_server = current_server.clone();
    let pinned_hit_server = current_server.clone();
    let pinned_hit_channel = current_channel.clone();
    let nav_for_search = nav;
    let nav_for_pinned = nav;

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
            search_effect_channel.clone(),
            search_effect_server.clone(),
            search_effect_self_user_id.clone(),
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

    use_effect(move || {
        if *utility_panel.read() != Some(ChatUtilityPanel::Pinned) {
            return;
        }
        let Some(target_channel_id) = pinned_effect_channel_id.clone() else {
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

    use_effect(move || {
        let count = chat_data.read().messages.len();
        if count == 0 {
            return;
        }
        document::eval(
            r#"
            let el = document.getElementById('message-list-scroll');
            if (el) {
                requestAnimationFrame(function() {
                    el.scrollTop = el.scrollHeight;
                });
            }
            "#,
        );
    });

    use_effect(move || {
        let _text = message_input.read().clone();
        document::eval(
            r#"
            let el = document.getElementById('poly-message-composer');
            if (el) {
                el.style.height = '0px';
                el.style.height = Math.min(el.scrollHeight, window.innerHeight * 0.5) + 'px';
            }
            "#,
        );
    });

    use_effect(move || {
        let server_member_list_open = app_state.read().nav.right_sidebar_visible;
        let dm_member_list_open = app_state.read().nav.dm_right_sidebar_visible;
        spawn(async move {
            persist_member_list_preferences(server_member_list_open, dm_member_list_open).await;
        });
    });

    // Pre-load slash commands whenever the channel changes so the popup can display instantly.
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
                    .map(|(_, h)| h)
            } else if let Some(account_id) = active_account_id {
                client_manager.read().get_backend(&account_id)
            } else {
                None
            };
            let Some(backend) = backend else { return };
            let guard = backend.read().await;
            match guard.get_channel_commands(&cid).await {
                Ok(cmds) => command_suggestions.set(cmds),
                Err(e) => tracing::warn!("get_channel_commands failed: {e}"),
            }
        });
    });

    rsx! {
        main {
            class: if *drag_over.read() { "chat-view drag-over" } else { "chat-view" },
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

            if *drag_over.read() {
                div { class: "drag-overlay",
                    div { class: "drag-overlay-content",
                        span { class: "drag-icon", "📎" }
                        p { "{t(\"chat-drop-files\")}" }
                    }
                }
            }

            div { class: "chat-layout-shell",
                div { class: "chat-main-column",
                    div { class: "chat-header",
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
                                        span { class: "chat-header-subtitle",
                                            {t("dm-header-subtitle")}
                                        }
                                    }
                                }
                            } else if is_group_channel {
                                div { class: "dm-chat-header-info",
                                    div { class: "group-chat-icon", "👥" }
                                    div { class: "dm-chat-header-text",
                                        span { class: "chat-channel-name", "{ch.name}" }
                                        span { class: "chat-header-subtitle",
                                            {format!("{} {}", group_members.len(), t("group-members-title"))}
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

                        div { class: "chat-header-right",
                            div { class: "chat-header-search-inline",
                                span { class: "chat-header-search-icon", "🔎" }
                                input {
                                    class: "chat-header-search-input",
                                    r#type: "text",
                                    placeholder: "{search_placeholder}",
                                    value: "{search_query_value}",
                                    onfocus: move |_| {
                                        let empty = search_query.read().trim().is_empty();
                                        show_search_filters.set(empty);
                                        if !empty {
                                            utility_panel.set(Some(ChatUtilityPanel::Search));
                                        }
                                    },
                                    oninput: move |evt| {
                                        let next_value = evt.value();
                                        let is_empty = next_value.trim().is_empty();
                                        search_query.set(next_value);
                                        show_search_filters.set(is_empty);
                                        if is_empty {
                                            utility_panel.set(None);
                                            search_hits.set(Vec::new());
                                        } else {
                                            utility_panel.set(Some(ChatUtilityPanel::Search));
                                        }
                                    },
                                }
                                if !search_query_value.is_empty() {
                                    button {
                                        class: "chat-header-search-clear",
                                        title: t("action-close"),
                                        onclick: move |_| {
                                            search_query.set(String::new());
                                            search_hits.set(Vec::new());
                                            utility_panel.set(None);
                                            show_search_filters.set(true);
                                        },
                                        "✕"
                                    }
                                }
                                if *show_search_filters.read() {
                                    SearchFilterPopup {
                                        current_channel_name: current_channel.as_ref().map(|channel| channel.name.clone()).unwrap_or_default(),
                                        on_append_filter: move |token: String| {
                                            let existing = search_query.read().trim().to_string();
                                            if existing.is_empty() {
                                                search_query.set(token);
                                            } else {
                                                search_query.set(format!("{existing} {token}"));
                                            }
                                            show_search_filters.set(false);
                                            utility_panel.set(Some(ChatUtilityPanel::Search));
                                        },
                                        on_close: move |_| show_search_filters.set(false),
                                    }
                                }
                            }

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
                                if is_group_channel {
                                    button {
                                        class: if app_state.read().nav.dm_right_sidebar_visible { "header-btn active" } else { "header-btn" },
                                        title: t("chat-toggle-members"),
                                        onclick: move |_| {
                                            let current = app_state.read().nav.dm_right_sidebar_visible;
                                            app_state.write().nav.dm_right_sidebar_visible = !current;
                                            utility_panel.set(None);
                                            show_search_filters.set(false);
                                        },
                                        "👥"
                                    }
                                } else if is_dm_channel {
                                    button {
                                        class: if app_state.read().nav.dm_right_sidebar_visible { "header-btn active" } else { "header-btn" },
                                        title: t("chat-toggle-contact"),
                                        onclick: move |_| {
                                            let current = app_state.read().nav.dm_right_sidebar_visible;
                                            app_state.write().nav.dm_right_sidebar_visible = !current;
                                            utility_panel.set(None);
                                            show_search_filters.set(false);
                                        },
                                        "👤"
                                    }
                                } else {
                                    button {
                                        class: if app_state.read().nav.right_sidebar_visible { "header-btn active" } else { "header-btn" },
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
                        }
                    }

                    div {
                        class: "message-list",
                        id: "message-list-scroll",
                        onscroll: move |_| {
                            spawn(async move {
                                let mut eval = document::eval(
                                    r#"
                                                            let el = document.getElementById('message-list-scroll');
                                                            if (el && el.scrollTop < 100) { dioxus.send(true); }
                                                            else { dioxus.send(false); }
                                                        "#,
                                );
                                if let Ok(near_top) = eval.recv::<bool>().await && near_top {
                                    tracing::trace!("Scroll near top — would load more messages");
                                }
                            });
                        },
                        if loading {
                            div { class: "message-loading", "{t(\"chat-loading\")}" }
                        } else if messages.is_empty() {
                            div { class: "message-empty",
                                div { class: "empty-wave", "👋" }
                                h3 { "{t(\"chat-no-messages\")}" }
                            }
                        } else {
                            for (idx , msg) in messages.iter().enumerate() {
                                {
                                    let prev_msg = if idx > 0 { messages.get(idx - 1) } else { None };
                                    let show_date_sep = match prev_msg {
                                        Some(prev) => msg.timestamp.date_naive() != prev.timestamp.date_naive(),
                                        None => true,
                                    };
                                    let is_grouped = match prev_msg {
                                        Some(prev) => {
                                            prev.author.id == msg.author.id && !show_date_sep
                                                && (msg.timestamp - prev.timestamp).num_minutes()
                                                    < GROUP_THRESHOLD_MINUTES
                                        }
                                        None => false,
                                    };
                                    let msg_id = msg.id.clone();
                                    let author = msg.author.clone();
                                    let content = msg.content.clone();
                                    let timestamp = msg.timestamp;
                                    let attachments = msg.attachments.clone();
                                    let reactions = msg.reactions.clone();
                                    let edited = msg.edited;
                                    let color = user_color(&author.id);
                                    let author_avatar = author.avatar_url.clone();
                                    let first_char: String = author
                                        .display_name
                                        .chars()
                                        .next()
                                        .map(|c| c.to_string())
                                        .unwrap_or_default();
                                    let time_str = format_timestamp(timestamp);
                                    let date_str = if show_date_sep {
                                        timestamp.format("%B %d, %Y").to_string()
                                    } else {
                                        String::new()
                                    }
                                    },
                                    let is_hovered = hovered_msg.read().as_deref() == Some(&msg_id);
                                    let is_own = author.id == self_user_id;
                                    let is_editing = editing_msg_id.read().as_deref() == Some(&msg_id);
                                    let msg_id_hover = msg_id.clone();
                                    let msg_id_reaction = msg_id.clone();
                                    let msg_id_edit = msg_id.clone();
                                    let msg_id_delete = msg_id.clone();
                                    let msg_id_ctx = msg_id.clone();
                                    let ctx_text = message_plain_text(&content);
                                    let edit_initial_text = ctx_text.clone();
                                    rsx! {
                                        if show_date_sep {
                                            div { class: "date-separator",
                                                span { class: "date-separator-text", "{date_str}" }
                                            }
                                        }
                                        div {
                                            id: "message-{msg_id}",
                                            class: if is_grouped { "message message-grouped" } else { "message message-full" },
                                            onmouseenter: {
                                                let mid = msg_id_hover.clone();
                                                move |_| hovered_msg.set(Some(mid.clone()))
                                            },
                                            onmouseleave: move |_| hovered_msg.set(None),
                                            oncontextmenu: {
                                                let mid = msg_id_ctx.clone();
                                                let txt = ctx_text.clone();
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

                                            if is_hovered && !is_editing {
                                                div { class: "message-actions",
                                                    button {
                                                        class: "msg-action-btn",
                                                        title: t("reaction-add"),
                                                        onclick: {
                                                            let mid = msg_id_reaction.clone();
                                                            move |_| reaction_picker_msg.set(Some(mid.clone()))
                                                        },
                                                        "😀+"
                                                    }
                                                    if is_own {
                                                        button {
                                                            class: "msg-action-btn",
                                                            title: t("msg-edit"),
                                                            onclick: {
                                                                let mid = msg_id_edit.clone();
                                                                let txt = edit_initial_text.clone();
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
                                                                let mid = msg_id_delete.clone();
                                                                move |_| chat_data.write().messages.retain(|m| m.id != mid)
                                                            },
                                                            "🗑️"
                                                        }
                                                    } else {
                                                        button {
                                                            class: "msg-action-btn",
                                                            title: t("msg-reply"),
                                                            onclick: move |_| tracing::debug!("Reply (stub)"),
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

                                            if !is_grouped {
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
                                                        span { class: "message-author", style: "color: {color};", "{author.display_name}" }
                                                        span { class: "message-timestamp", "{time_str}" }
                                                    }
                                                    if is_editing {
                                                        MessageInlineEdit {
                                                            message_id: msg_id.clone(),
                                                            editing_msg_id,
                                                            edit_draft,
                                                            chat_data,
                                                        }
                                                    } else {
                                                        MessageContentView { content: content.clone(), edited }
                                                    }
                                                    if !attachments.is_empty() {
                                                        AttachmentsView { attachments: attachments.clone() }
                                                    }
                                                    if !reactions.is_empty() {
                                                        ReactionsView { reactions: reactions.clone(), message_id: msg_id.clone() }
                                                    }
                                                }
                                            } else {
                                                div { class: "message-gutter",
                                                    span { class: "message-hover-time", "{time_str}" }
                                                }
                                                div { class: "message-body",
                                                    if is_editing {
                                                        MessageInlineEdit {
                                                            message_id: msg_id.clone(),
                                                            editing_msg_id,
                                                            edit_draft,
                                                            chat_data,
                                                        }
                                                    } else {
                                                        MessageContentView { content: content.clone(), edited }
                                                    }
                                                    if !attachments.is_empty() {
                                                        AttachmentsView { attachments: attachments.clone() }
                                                    }
                                                    if !reactions.is_empty() {
                                                        ReactionsView { reactions: reactions.clone(), message_id: msg_id.clone() }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    TypingIndicator {}

                    div { class: "message-input-area",
                        if channel_id.is_some() {
                            if !pending_attachments.read().is_empty() {
                                div { class: "attachment-preview-strip",
                                    for preview in pending_attachments.read().iter() {
                                        div { class: "attachment-preview-card",
                                            if let Some(ref preview_url) = preview.preview_url {
                                                img {
                                                    class: "attachment-preview-image",
                                                    src: "{preview_url}",
                                                    alt: "{preview.filename}",
                                                }
                                            } else {
                                                div { class: "attachment-preview-icon",
                                                    "📎"
                                                }
                                            }
                                            div { class: "attachment-preview-meta",
                                                span { class: "attachment-preview-name",
                                                    "{preview.filename}"
                                                }
                                                span { class: "attachment-preview-size",
                                                    "{format_file_size(preview.size)}"
                                                }
                                            }
                                            button {
                                                class: "attachment-preview-remove",
                                                title: t("action-close"),
                                                onclick: {
                                                    let preview_id = preview.id.clone();
                                                    move |_| {
                                                        pending_attachments.write().retain(|item| item.id != preview_id);
                                                    }
                                                },
                                                "✕"
                                            }
                                        }
                                    }
                                }
                            }
                            {
                                let all_cmds = command_suggestions.read().clone();
                                let text = message_input.read().clone();
                                let query = slash_command_query(&text);
                                let matches = if *show_command_popup.read() {
                                    filtered_slash_commands(query, &all_cmds)
                                } else {
                                    Vec::new()
                                };
                                let active_idx = *active_command_idx.read();
                                rsx! {
                                    if !matches.is_empty() {
                                        SlashCommandPopup {
                                            commands: matches,
                                            active_idx,
                                            on_select: move |filled: String| {
                                                message_input.set(filled);
                                                show_command_popup.set(false);
                                            },
                                        }
                                    }
                                }
                            }
                            div { class: "message-input-row",
                                div { class: "message-input-shell",
                                    button {
                                        class: "toolbar-btn composer-upload-btn",
                                        title: t("chat-attach-file"),
                                        onclick: move |_| {
                                            document::eval(
                                                r#"
                                                                                        let input = document.getElementById('poly-file-input');
                                                                                        if (input) { input.click(); }
                                                                                    "#,
                                            );
                                        },
                                        "➕"
                                    }
                                    div { class: "message-input-text-area",
                                        textarea {
                                            class: "message-input",
                                            id: "poly-message-composer",
                                            placeholder: "{compose_placeholder}",
                                            value: "{message_input}",
                                            rows: "1",
                                            oninput: move |evt| {
                                                let value = evt.value();
                                                message_input.set(value.clone());
                                                // Show slash command popup when user types /command (no space yet)
                                                let trimmed = value.trim_start();
                                                if trimmed.starts_with('/') && !trimmed.contains('\n')
                                                    && !trimmed[1..].contains(' ')
                                                {
                                                    let query = &trimmed[1..];
                                                    let all_cmds = command_suggestions.read().clone();
                                                    let matches = filtered_slash_commands(query, &all_cmds);
                                                    if !matches.is_empty() {
                                                        show_command_popup.set(true);
                                                    } else {
                                                        show_command_popup.set(false);
                                                    }
                                                    active_command_idx.set(0);
                                                } else {
                                                    show_command_popup.set(false);
                                                }
                                            },
                                            onkeydown: {
                                                let channel_id_send = channel_id.clone();
                                                move |evt: KeyboardEvent| {
                                                    // Slash command popup navigation
                                                    if *show_command_popup.read() {
                                                        match evt.key() {
                                                            Key::ArrowUp => {
                                                                evt.prevent_default();
                                                                let cur = *active_command_idx.read();
                                                                if cur > 0 {
                                                                    active_command_idx.set(cur - 1);
                                                                }
                                                                return;
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
                                                                return;
                                                            }
                                                            Key::Escape => {
                                                                evt.prevent_default();
                                                                show_command_popup.set(false);
                                                                return;
                                                            }
                                                            Key::Tab => {
                                                                evt.prevent_default();
                                                                let all_cmds = command_suggestions.read().clone();
                                                                let text = message_input.read().clone();
                                                                let query = slash_command_query(&text);
                                                                let matches = filtered_slash_commands(query, &all_cmds);
                                                                let idx = *active_command_idx.read();
                                                                if let Some(cmd) = matches.get(idx) {
                                                                    message_input.set(format!("/{} ", cmd.name));
                                                                    show_command_popup.set(false);
                                                                }
                                                                return;
                                                            }
                                                            Key::Enter if !evt.modifiers().shift() => {
                                                                evt.prevent_default();
                                                                let all_cmds = command_suggestions.read().clone();
                                                                let text = message_input.read().clone();
                                                                let query = slash_command_query(&text);
                                                                let matches = filtered_slash_commands(query, &all_cmds);
                                                                let idx = *active_command_idx.read();
                                                                if let Some(cmd) = matches.get(idx) {
                                                                    message_input.set(format!("/{} ", cmd.name));
                                                                    show_command_popup.set(false);
                                                                }
                                                                return;
                                                            }
                                                            _ => {}
                                                        }
                                                    }
                                                    // Normal Enter: send message
                                                    if evt.key() == Key::Enter && !evt.modifiers().shift() {
                                                        evt.prevent_default();
                                                        let raw_text = message_input.read().clone();
                                                        let text = apply_builtin_command(raw_text.trim())
                                                            .unwrap_or(raw_text);
                                                        let attachments = pending_attachments.read().clone();
                                                        if !text.is_empty() || !attachments.is_empty() {
                                                            message_input.set(String::new());
                                                            pending_attachments.set(Vec::new());
                                                            if let Some(ref cid) = channel_id_send {
                                                                let cid = cid.clone();
                                                                let attachments = attachments
                                                                    .iter()
                                                                    .map(pending_attachment_to_attachment)
                                                                    .collect::<Vec<_>>();
                                                                spawn(async move {
                                                                    send_message(
                                                                            cid,
                                                                            text,
                                                                            attachments,
                                                                            client_manager,
                                                                            chat_data,
                                                                            app_state,
                                                                        )
                                                                        .await;
                                                                });
                                                            }
                                                        }
                                                    }
                                                }
                                            },
                                        }
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
                                            button {
                                                class: "toolbar-btn gif-btn",
                                                title: t("gif-picker"),
                                                "GIF"
                                            }
                                            button {
                                                class: "toolbar-btn",
                                                title: t("chat-markdown-formatting"),
                                                "⌘"
                                            }
                                        }
                                    }
                                }
                                button {
                                    class: "btn btn-send chat-send-inline",
                                    disabled: message_input.read().is_empty() && pending_attachments.read().is_empty(),
                                    onclick: {
                                        let channel_id_btn = channel_id.clone();
                                        move |_| {
                                            let text = message_input.read().clone();
                                            let attachments = pending_attachments.read().clone();
                                            if !text.is_empty() || !attachments.is_empty() {
                                                message_input.set(String::new());
                                                pending_attachments.set(Vec::new());
                                                if let Some(ref cid) = channel_id_btn {
                                                    let cid = cid.clone();
                                                    let text = text.clone();
                                                    let attachments = attachments
                                                        .iter()
                                                        .map(pending_attachment_to_attachment)
                                                        .collect::<Vec<_>>();
                                                    spawn(async move {
                                                        send_message(
                                                                cid,
                                                                text,
                                                                attachments,
                                                                client_manager,
                                                                chat_data,
                                                                app_state,
                                                            )
                                                            .await;
                                                    });
                                                }
                                            }
                                        }
                                    },
                                    {t("chat-send")}
                                }
                            }
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
                            if *show_input_emoji.read() {
                                EmojiPicker {
                                    on_select: move |emoji: String| {
                                        let current = message_input.read().clone();
                                        message_input.set(format!("{current}{emoji}"));
                                        show_input_emoji.set(false);
                                    },
                                    on_close: move |_| show_input_emoji.set(false),
                                }
                            }
                        } else {
                            div { class: "message-input-disabled", {t("chat-select-channel")} }
                        }
                    }
                }

                if utility_panel.read().is_some() || member_list_visible {
                    div { class: "chat-side-column",
                        if let Some(panel) = *utility_panel.read() {
                            ChatUtilityRail {
                                panel,
                                search_query: search_query_value.clone(),
                                search_hits: search_hits.read().clone(),
                                search_terms: search_terms.clone(),
                                pinned_messages: pinned_messages.read().clone(),
                                current_channel_name: current_channel.as_ref().map(|c| c.name.clone()).unwrap_or_default(),
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
                        } else if is_dm_channel {
                            DmContactPanel { channel_id: channel_id.clone().unwrap_or_default() }
                        } else if is_group_channel {
                            DmUserSidebar {}
                        } else {
                            UserSidebar {}
                        }
                    }
                }
            }

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
}

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

#[component]
fn SearchFilterPopup(
    current_channel_name: String,
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
                for suggestion in SEARCH_FILTER_SUGGESTIONS {
                    SearchFilterRow {
                        icon: suggestion.icon,
                        title: t(suggestion.title_key),
                        subtitle: t(suggestion.subtitle_key),
                        token: if suggestion.token == "in:#current" { format!("in:#{}", current_channel_name) } else { suggestion.token.to_string() },
                        on_click: move |token| on_append_filter.call(token),
                    }
                }
            }
        }
    }
}

#[component]
fn SearchFilterRow(
    icon: &'static str,
    title: String,
    subtitle: String,
    token: String,
    on_click: EventHandler<String>,
) -> Element {
    rsx! {
        button {
            class: "search-filter-row",
            onclick: move |_| on_click.call(token.clone()),
            span { class: "search-filter-icon", "{icon}" }
            div { class: "search-filter-copy",
                div { class: "search-filter-title", "{title}" }
                div { class: "search-filter-subtitle", "{subtitle}" }
            }
        }
    }
}

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

/// Send a message via the backend and prepend it to the message list.
async fn send_message(
    channel_id: String,
    text: String,
    attachments: Vec<Attachment>,
    client_manager: Signal<ClientManager>,
    mut chat_data: Signal<ChatData>,
    app_state: Signal<AppState>,
) {
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
    match guard.send_message(&channel_id, content).await {
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
        // Transparent backdrop — closes menu on click
        div {
            class: "context-menu-backdrop",
            onclick: move |_| msg_context_menu.set(None),
            oncontextmenu: move |evt| {
                evt.prevent_default();
            },
        }

        // Floating context menu
        div {
            class: "context-menu msg-context-menu",
            style: "left: {x}px; top: {y}px;",
            onclick: move |evt| evt.stop_propagation(),

            // Quick reactions row
            div { class: "msg-context-quick-reactions",
                for emoji in QUICK_REACTIONS {
                    {
                        let e = emoji.to_string();
                        let mid = menu.message_id.clone();
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

            div { class: "context-menu-separator" }

            // Add Reaction
            ContextMenuItemSimple {
                label: t("reaction-add"),
                has_arrow: true,
                onclick: move |_| msg_context_menu.set(None),
            }

            // Reply
            ContextMenuItemSimple {
                label: t("msg-reply"),
                icon: "↩",
                onclick: move |_| {
                    tracing::debug!("Reply (stub)");
                    msg_context_menu.set(None);
                },
            }

            // Forward
            ContextMenuItemSimple {
                label: t("msg-forward"),
                icon: "➡",
                onclick: move |_| {
                    tracing::debug!("Forward (stub)");
                    msg_context_menu.set(None);
                },
            }

            // Copy Text
            ContextMenuItemSimple {
                label: t("msg-copy-text"),
                onclick: {
                    let txt = txt_copy.clone();
                    move |_| {
                        let js = format!(
                            "navigator.clipboard.writeText({}).catch(()=>{{}})",
                            serde_json::to_string(&txt).unwrap_or_default(),
                        );
                        document::eval(&js);
                        msg_context_menu.set(None);
                    }
                },
            }

            // Apps
            ContextMenuItemSimple {
                label: t("msg-apps"),
                has_arrow: true,
                onclick: move |_| msg_context_menu.set(None),
            }

            // Mark Unread
            ContextMenuItemSimple {
                label: t("msg-mark-unread"),
                onclick: move |_| {
                    tracing::debug!("Mark unread (stub)");
                    msg_context_menu.set(None);
                },
            }

            // Copy Message Link
            ContextMenuItemSimple {
                label: t("msg-copy-link"),
                onclick: move |_| {
                    tracing::debug!("Copy link (stub)");
                    msg_context_menu.set(None);
                },
            }

            // Speak Message
            ContextMenuItemSimple {
                label: t("msg-speak"),
                onclick: move |_| {
                    tracing::debug!("Speak (stub)");
                    msg_context_menu.set(None);
                },
            }

            div { class: "context-menu-separator" }

            // Report Message — only for others' messages
            if !is_own {
                ContextMenuItemSimple {
                    label: t("msg-report"),
                    danger: true,
                    onclick: move |_| {
                        tracing::debug!("Report (stub)");
                        msg_context_menu.set(None);
                    },
                }
            }

            // Delete — only for own messages
            if is_own {
                ContextMenuItemSimple {
                    label: t("msg-delete"),
                    danger: true,
                    onclick: move |_| {
                        let mid = mid_delete.clone();
                        chat_data.write().messages.retain(|m| m.id != mid);
                        msg_context_menu.set(None);
                    },
                }
            }

            // Copy Message ID
            ContextMenuItemSimple {
                label: t("msg-copy-id"),
                onclick: {
                    let mid = mid_copy_id.clone();
                    move |_| {
                        let js = format!(
                            "navigator.clipboard.writeText({}).catch(()=>{{}})",
                            serde_json::to_string(&mid).unwrap_or_default(),
                        );
                        document::eval(&js);
                        msg_context_menu.set(None);
                    }
                },
            }
        }
    }
}

/// Simple context menu item button.
///
/// Renders a full-width button with optional right arrow, danger styling,
/// and a leading icon glyph.
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

/// Slash command autocomplete popup rendered above the message input.
///
/// Shows filtered commands with provider badges. Highlighted item is driven by `active_idx`.
/// Clicking a command calls `on_select` with the filled command text (e.g. `"/play "`).
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
