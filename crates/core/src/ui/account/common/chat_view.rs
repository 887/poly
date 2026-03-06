//! Chat view — Discord-style message list and message input.
//!
//! Common implementation shared across all messenger backends.
//! Backend-specific chat view overrides (e.g., special message types)
//! will live in per-backend directories in future phases.
//!
//! Features:
//! - Message grouping (same author within 7 minutes)
//! - Date separators between different days
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.
//! - Inline image previews with size labels
//! - Non-image attachments as download links
//! - Reaction pills with emoji + count (clickable to toggle)
//! - Multi-line message rendering
//! - Edited indicator
//! - Auto-resize textarea input (Enter=send, Shift+Enter=newline)
//! - Channel header with source info + member count
//! - Hover action bar with add-reaction button
//! - Emoji/GIF/attachment buttons in the input toolbar
//! - File drag-and-drop overlay

// TODO(phase-2.5.6): Discord-style chat view rewrite

use super::emoji_picker::EmojiPicker;
use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::chat_data::{backend_badge, format_file_size, user_color};
use crate::state::{AppState, ChatData};
use dioxus::prelude::*;
use poly_client::MessageContent;

/// State for the message right-click context menu.
#[derive(Debug, Clone)]
struct MsgContextMenu {
    /// X pixel coordinate (client-relative).
    x: f64,
    /// Y pixel coordinate (client-relative).
    y: f64,
    /// ID of the message that was right-clicked.
    message_id: String,
    /// Plain text content of the message (for clipboard actions).
    message_text: String,
    /// Whether the right-clicked message belongs to the currently signed-in user.
    is_own: bool,
}

/// The maximum number of minutes between messages from the same author
/// to be considered a "group" (compact layout).
const GROUP_THRESHOLD_MINUTES: i64 = 7;

/// Chat view component.
///
/// Shows the channel header, scrollable message list with Discord-style
/// rendering, and textarea message input with emoji/GIF/attachment toolbar.
#[component]
pub fn ChatView() -> Element {
    let mut app_state: Signal<AppState> = use_context();
    let client_manager: Signal<ClientManager> = use_context();
    let mut chat_data: Signal<ChatData> = use_context();
    let mut message_input = use_signal(String::new);
    let mut show_input_emoji = use_signal(|| false);
    let mut reaction_picker_msg = use_signal(|| None::<String>);
    let mut drag_over = use_signal(|| false);
    let mut hovered_msg = use_signal(|| None::<String>);
    let mut editing_msg_id = use_signal(|| None::<String>);
    let mut edit_draft = use_signal(String::new);
    let mut msg_context_menu = use_signal(|| None::<MsgContextMenu>);

    let channel_id = app_state.read().nav.selected_channel.clone();
    let messages = chat_data.read().messages.clone();
    let current_channel = chat_data.read().current_channel.clone();
    let current_server = chat_data.read().current_server.clone();
    let members_count = chat_data.read().members.len();
    let loading = chat_data.read().loading;
    let reaction_picker_id = reaction_picker_msg.read().clone();
    let group_members = chat_data.read().active_group_members.clone();
    let is_dm_channel = channel_id.as_deref().unwrap_or_default().starts_with("dm-");
    let is_group_channel = channel_id
        .as_deref()
        .unwrap_or_default()
        .starts_with("group-");

    // Derive the signed-in user's ID for "is own message" checks.
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

    // Look up DM user avatar from dm_channels (Channel struct has no avatar_url)
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

    // Scroll message list to bottom when messages change
    let msg_count = messages.len();
    use_effect(move || {
        let _count = msg_count; // track dependency
        document::eval(
            r#"
            let el = document.getElementById('message-list-scroll');
            if (el) { el.scrollTop = el.scrollHeight; }
            "#,
        );
    });

    rsx! {
        main {
            class: if *drag_over.read() { "chat-view drag-over" } else { "chat-view" },
            // Drag-and-drop handlers
            ondragover: move |evt| {
                evt.prevent_default();
                drag_over.set(true);
            },
            ondragleave: move |_| {
                drag_over.set(false);
            },
            ondrop: move |evt| {
                evt.prevent_default();
                drag_over.set(false);
                // TODO(phase-3): parse dropped files into PendingFile attachments
                // The Dioxus DragEvent provides file data on web targets.
                // On desktop (Wry) the drop data arrives differently.
                tracing::debug!("File(s) dropped on chat view");
            },

            // Drag overlay
            if *drag_over.read() {
                div { class: "drag-overlay",
                    div { class: "drag-overlay-content",
                        span { class: "drag-icon", "📎" }
                        p { "{t(\"chat-drop-files\")}" }
                    }
                }
            }

            // ── Channel header ───────────────────────────────────────────
            div { class: "chat-header",
                if let Some(ref ch) = current_channel {
                    if is_dm_channel {
                        // DM header: avatar image or colored letter + display name
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
                                span { class: "chat-header-subtitle", "{t(\"dm-header-subtitle\")}" }
                            }
                        }
                    } else if is_group_channel {
                        // Group DM header: group name + member count
                        div { class: "dm-chat-header-info",
                            div { class: "group-chat-icon", "👥" }
                            div { class: "dm-chat-header-text",
                                span { class: "chat-channel-name", "{ch.name}" }
                                span { class: "chat-header-subtitle",
                                    "{group_members.len()} {t(\"group-members-title\")}"
                                }
                            }
                        }
                    } else {
                        // Server channel header (unchanged)
                        span { class: "chat-channel-name", "# {ch.name}" }
                        if let Some(ref server) = current_server {
                            span { class: "chat-source-badge",
                                "{backend_badge(&server.backend)} {server.backend.display_name()}"
                            }
                        }
                        span { class: "chat-member-count", "👥 {members_count}" }
                    }
                } else {
                    span { class: "chat-channel-name", "{t(\"chat-no-messages\")}" }
                }
                div { class: "chat-header-actions",
                    if is_group_channel {
                        button {
                            class: "header-btn",
                            title: "{t(\"chat-toggle-members\")}",
                            onclick: move |_| {
                                let current = app_state.read().nav.dm_right_sidebar_visible;
                                app_state.write().nav.dm_right_sidebar_visible = !current;
                            },
                            "👥"
                        }
                    } else if !is_dm_channel {
                        button {
                            class: "header-btn",
                            title: "{t(\"chat-toggle-members\")}",
                            onclick: move |_| {
                                let current = app_state.read().nav.right_sidebar_visible;
                                app_state.write().nav.right_sidebar_visible = !current;
                            },
                            "👥"
                        }
                    }
                }
            }

            // ── Message list ─────────────────────────────────────────────
            div {
                class: "message-list",
                id: "message-list-scroll",
                // Scroll-up pagination: detect near-top and trigger load more
                onscroll: move |_| {
                    spawn(async move {
                        let mut eval = document::eval(
                            r#"
                                                                                                                let el = document.getElementById('message-list-scroll');
                                                                                                                if (el && el.scrollTop < 100) { dioxus.send(true); }
                                                                                                                else { dioxus.send(false); }
                                                                                                                "#,
                        );
                        if let Ok(near_top) = eval.recv::<bool>().await
                            && near_top
                        {
                            // TODO(phase-3): call backend load_more_messages with before cursor
                            tracing::trace!("Scroll near top — would load more messages");
                        }
                    });
                },
                if loading {
                    div { class: "message-loading", "{t(\"chat-loading\")}" }
                } else if messages.is_empty() {
                    // Empty state
                    div { class: "message-empty",
                        div { class: "empty-wave", "👋" }
                        h3 { "{t(\"chat-no-messages\")}" }
                    }
                } else {
                    // Render messages with grouping and date separators
                    for (idx , msg) in messages.iter().enumerate() {
                        {
                            let prev_msg = if idx > 0 { messages.get(idx - 1) } else { None };

                            // Check if we need a date separator
                            let show_date_sep = match prev_msg {
                                Some(prev) => msg.timestamp.date_naive() != prev.timestamp.date_naive(),
                                None => true, // Always show date for the first message
                            };

                            // Check if this message is part of a group with the previous one
                            let is_grouped = match prev_msg {
                                Some(prev) => {
                                    prev.author.id == msg.author.id
                                        && !show_date_sep
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
                            };
                            let is_hovered = hovered_msg.read().as_deref() == Some(&msg_id);
                            let is_own = author.id == self_user_id;
                            let is_editing = editing_msg_id.read().as_deref() == Some(&msg_id);
                            let msg_id_hover = msg_id.clone();
                            let msg_id_reaction = msg_id.clone();
                            let msg_id_edit = msg_id.clone();
                            let msg_id_delete = msg_id.clone();
                            let msg_id_ctx = msg_id.clone();
                            let ctx_text = match &content {
                                MessageContent::Text(t) => t.clone(),
                                MessageContent::WithAttachments { text, .. } => text.clone(),
                            };
                            let edit_initial_text = ctx_text.clone();
                            rsx! {
                                // Date separator
                                if show_date_sep {
                                    div { class: "date-separator",
                                        span { class: "date-separator-text", "{date_str}" }
                                    }
                                }

                                // Message
                                div {
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

                                    // Hover action bar (hidden while inline-editing)
                                    if is_hovered && !is_editing {
                                        div { class: "message-actions", // Own message: Edit button
                                            // Reaction button — always shown
                                            button {
                                                class: "msg-action-btn",
                                                title: "{t(\"reaction-add\")}",
                                                onclick: {
                                                    let mid = msg_id_reaction.clone();
                                                    move |_| {
                                                        reaction_picker_msg.set(Some(mid.clone()));
                                                    }
                                                },
                                                "😀+"
                                            }
                                            if is_own {
                                                // Own message: Edit button
                                                button {
                                                    class: "msg-action-btn",
                                                    title: "{t(\"msg-edit\")}",
                                                    onclick: {
                                                        let mid = msg_id_edit.clone();
                                                        let txt = edit_initial_text.clone();
                                                        move |_| {
                                                            edit_draft.set(txt.clone());
                                                            editing_msg_id.set(Some(mid.clone()));
                                                        }
                                                    },
                                                    "✏️"
                                                } // Full message: avatar (image or fallback letter) + header // Full message: avatar (image or fallback letter) + header
                                                // Own message: Delete button
                                                button {
                                                    class: "msg-action-btn msg-action-btn-danger",
                                                    title: "{t(\"msg-delete\")}",
                                                    onclick: {
                                                        let mid = msg_id_delete.clone();
                                                        move |_| {
                                                            chat_data.write().messages.retain(|m| m.id != mid);
                                                        }
                                                    },
                                                    "🗑️"
                                                }
                                            } else {
                                                // Other's message: Reply + Forward
                                                button {
                                                    class: "msg-action-btn",
                                                    title: "{t(\"msg-reply\")}", // Reactions
                                                    onclick: move |_| {
                                                        tracing::debug!("Reply (stub)");
                                                    },
                                                    "↩️"
                                                }
                                                button { // Content (or inline edit UI) // Content (or inline edit UI)
                                                    class: "msg-action-btn",
                                                    title: "{t(\"msg-forward\")}",
                                                    onclick: move |_| {
                                                        tracing::debug!("Forward (stub)");
                                                    },
                                                    "➡️"
                                                }
                                            }
                                        }
                                    }

                                    if !is_grouped {
                                        // Full message: avatar (image or fallback letter) + header
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
                                            // Content (or inline edit UI)
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
                                            // Attachments
                                            if !attachments.is_empty() {
                                                AttachmentsView { attachments: attachments.clone() }
                                            }
                                            // Reactions
                                            if !reactions.is_empty() {
                                                ReactionsView { reactions: reactions.clone(), message_id: msg_id.clone() }
                                            }
                                        }
                                    } else {
                                        // Grouped message: just content, aligned with body
                                        div { class: "message-gutter",
                                            span { class: "message-hover-time", "{time_str}" }
                                        }
                                        div { class: "message-body",
                                            // Content (or inline edit UI)
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

            // ── Typing indicator ──────────────────────────────────────
            TypingIndicator {}

            // ── Reaction emoji picker (top-level to escape message-list overflow) ──
            if let Some(ref picker_msg_id) = reaction_picker_id {
                EmojiPicker {
                    on_select: {
                        let msg_id = picker_msg_id.clone();
                        move |emoji: String| {
                            toggle_reaction_on_message(&mut chat_data, &msg_id, &emoji);
                            reaction_picker_msg.set(None);
                        }
                    },
                    on_close: move |_| {
                        reaction_picker_msg.set(None);
                    },
                }
            }

            // ── Message right-click context menu ──────────────────────
            if msg_context_menu.read().is_some() {
                MsgContextMenuOverlay { msg_context_menu, chat_data }
            }

            // ── Message input with toolbar ───────────────────────────────
            div { class: "message-input-area",
                if channel_id.is_some() {
                    // Input toolbar row
                    div { class: "message-input-row",
                        textarea {
                            class: "message-input",
                            placeholder: "{t(\"chat-type-message\")}",
                            value: "{message_input}",
                            rows: "1",
                            oninput: move |evt| message_input.set(evt.value()),
                            onkeydown: {
                                let channel_id_send = channel_id.clone();
                                move |evt: KeyboardEvent| {
                                    if evt.key() == Key::Enter && !evt.modifiers().shift() {
                                        evt.prevent_default();
                                        let text = message_input.read().clone();
                                        if !text.is_empty() {
                                            message_input.set(String::new());
                                            if let Some(ref cid) = channel_id_send {
                                                let cid = cid.clone();
                                                let text = text.clone();
                                                spawn(async move {
                                                    send_message(cid, text, client_manager, chat_data, app_state)
                                                        .await;
                                                });
                                            }
                                        }
                                    }
                                }
                            },
                        }
                        // Input toolbar buttons (right side)
                        div { class: "input-toolbar",
                            button {
                                class: "toolbar-btn",
                                title: "{t(\"emoji-picker\")}",
                                onclick: move |_| {
                                    let current = *show_input_emoji.read();
                                    show_input_emoji.set(!current);
                                },
                                "😀"
                            }
                            button {
                                class: "toolbar-btn gif-btn",
                                title: "{t(\"gif-picker\")}",
                                "GIF"
                            }
                            button {
                                class: "toolbar-btn",
                                title: "{t(\"chat-attach-file\")}",
                                onclick: move |_| {
                                    // Trigger hidden file input via JS
                                    document::eval(
                                        r#"
                                                                                                                                                                                                                                            let input = document.getElementById('poly-file-input');
                                                                                                                                                                                                                                            if (input) { input.click(); }
                                                                                                                                                                                                                                            "#,
                                    );
                                },
                                "📎"
                            }
                        }
                    }
                    // Hidden file input for the attach button
                    input {
                        r#type: "file",
                        id: "poly-file-input",
                        multiple: true,
                        style: "display:none;",
                        onchange: move |_evt| {
                            // TODO(phase-3): read selected files and create PendingFile attachments
                            tracing::debug!("File selected via attach button");
                        },
                    }
                    // Emoji picker for input
                    if *show_input_emoji.read() {
                        EmojiPicker {
                            on_select: move |emoji: String| {
                                let current = message_input.read().clone();
                                message_input.set(format!("{current}{emoji}"));
                                show_input_emoji.set(false);
                            },
                            on_close: move |_| {
                                show_input_emoji.set(false);
                            },
                        }
                    }
                    button {
                        class: "btn btn-send",
                        disabled: message_input.read().is_empty(),
                        onclick: {
                            let channel_id_btn = channel_id.clone();
                            move |_| {
                                let text = message_input.read().clone();
                                if !text.is_empty() {
                                    message_input.set(String::new());
                                    if let Some(ref cid) = channel_id_btn {
                                        let cid = cid.clone();
                                        let text = text.clone();
                                        spawn(async move {
                                            send_message(cid, text, client_manager, chat_data, app_state)
                                                .await;
                                        });
                                    }
                                }
                            }
                        },
                        "{t(\"chat-send\")}"
                    }
                } else {
                    div { class: "message-input-disabled", "Select a channel to start chatting" }
                }
            }
        }
    }
}

/// Render message text content, handling multi-line and edited indicator.
#[component]
fn MessageContentView(content: MessageContent, edited: bool) -> Element {
    let text = match &content {
        MessageContent::Text(t) => t.clone(),
        MessageContent::WithAttachments { text, .. } => text.clone(),
    };

    rsx! {
        div { class: "message-text",
            // Split on newlines for multi-line rendering
            for line in text.split('\n') {
                if line.is_empty() {
                    br {}
                } else {
                    p { class: "message-line", "{line}" }
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
    client_manager: Signal<ClientManager>,
    mut chat_data: Signal<ChatData>,
    app_state: Signal<AppState>,
) {
    let server_id = app_state.read().nav.selected_server.clone();
    let Some(server_id) = server_id else { return };

    let backend_info = client_manager.read().get_backend_for_server(&server_id);
    let Some((_account_id, backend)) = backend_info else {
        return;
    };

    let guard = backend.read().await;
    match guard
        .send_message(&channel_id, MessageContent::Text(text))
        .await
    {
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
