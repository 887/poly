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

    let channel_id = app_state.read().nav.selected_channel.clone();
    let messages = chat_data.read().messages.clone();
    let current_channel = chat_data.read().current_channel.clone();
    let current_server = chat_data.read().current_server.clone();
    let members_count = chat_data.read().members.len();
    let loading = chat_data.read().loading;
    let reaction_picker_id = reaction_picker_msg.read().clone();

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
                    span { class: "chat-channel-name", "# {ch.name}" }
                    if let Some(ref server) = current_server {
                        span { class: "chat-source-badge",
                            "{backend_badge(&server.backend)} {server.backend.display_name()}"
                        }
                    }
                    span { class: "chat-member-count", "👥 {members_count}" }
                } else {
                    span { class: "chat-channel-name", "{t(\"chat-no-messages\")}" }
                }
                div { class: "chat-header-actions",
                    button {
                        class: "header-btn",
                        title: "Toggle member list",
                        onclick: move |_| {
                            let current = app_state.read().nav.right_sidebar_visible;
                            app_state.write().nav.right_sidebar_visible = !current;
                        },
                        "👥"
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
                            let msg_id_hover = msg_id.clone();
                            let msg_id_reaction = msg_id.clone();
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

                                    // Hover action bar
                                    if is_hovered {
                                        div { class: "message-actions",
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
                                        }
                                    }

                                    if !is_grouped {
                                        // Full message: avatar + header
                                        div { class: "message-avatar", style: "background-color: {color};", "{first_char}" }
                                        div { class: "message-body",
                                            div { class: "message-header",
                                                span { class: "message-author", style: "color: {color};", "{author.display_name}" }
                                                span { class: "message-timestamp", "{time_str}" }
                                            }
                                            // Content
                                            MessageContentView { content: content.clone(), edited }
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
                                            MessageContentView { content: content.clone(), edited }
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
