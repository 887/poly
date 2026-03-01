//! Chat view — Discord-style message list and message input.
//!
//! Features:
//! - Message grouping (same author within 7 minutes)
//! - Date separators between different days
//! - Inline image previews with size labels
//! - Non-image attachments as download links
//! - Reaction pills with emoji + count
//! - Multi-line message rendering
//! - Edited indicator
//! - Auto-resize textarea input (Enter=send, Shift+Enter=newline)
//! - Channel header with source info + member count
// TODO(phase-2.5.6): Discord-style chat view rewrite

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
/// rendering, and textarea message input.
#[component]
pub fn ChatView() -> Element {
    let mut app_state: Signal<AppState> = use_context();
    let client_manager: Signal<ClientManager> = use_context();
    let chat_data: Signal<ChatData> = use_context();
    let mut message_input = use_signal(String::new);

    let channel_id = app_state.read().nav.selected_channel.clone();
    let messages = chat_data.read().messages.clone();
    let current_channel = chat_data.read().current_channel.clone();
    let current_server = chat_data.read().current_server.clone();
    let members_count = chat_data.read().members.len();
    let loading = chat_data.read().loading;

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
        main { class: "chat-view",
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
            div { class: "message-list", id: "message-list-scroll",
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
                            let _msg_id = msg.id.clone();
                            let author = msg.author.clone();
                            let content = msg.content.clone();
                            let timestamp = msg.timestamp;
                            let attachments = msg.attachments.clone();
                            let reactions = msg.reactions.clone();
                            let edited = msg.edited; // Date separator
                            let color = user_color(&author.id);
                            let first_char: String = author
                                .display_name
                                .chars()
                                .next() // Full message: avatar + header
                                .map(|c| c.to_string())
                                .unwrap_or_default();
                            let time_str = format_timestamp(timestamp);
                            let date_str = if show_date_sep {
                                timestamp.format("%B %d, %Y").to_string()
                            } else {
                                String::new()
                            }; // Attachments
                            rsx! {
                                // Date separator
                                if show_date_sep { // Reactions
                                    div { class: "date-separator",
                                        span { class: "date-separator-text", "{date_str}" }
                                    }
                                }

                                // Message
                                div { class: if is_grouped { "message message-grouped" } else { "message message-full" },

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
                                                ReactionsView { reactions: reactions.clone() }
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
                                                ReactionsView { reactions: reactions.clone() }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ── Message input ────────────────────────────────────────────
            div { class: "message-input-area",
                if channel_id.is_some() {
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

/// Render reaction pills.
#[component]
fn ReactionsView(reactions: Vec<poly_client::Reaction>) -> Element {
    rsx! {
        div { class: "message-reactions",
            for reaction in &reactions {
                {
                    let emoji = reaction.emoji.clone();
                    let count = reaction.count;
                    let me_class = if reaction.me { "reaction-pill me" } else { "reaction-pill" };

                    rsx! {
                        span { class: "{me_class}", "{emoji} {count}" }
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
