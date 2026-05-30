//! Message composer — input area, attachment previews, slash-command popup,
//! send-message logic, typing-mode heartbeat, and hidden file input.
//!
//! Single responsibility: everything that lets the user *write* a message.
//! No message-list rendering, no history paging, no layout code lives here.

use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

use crate::client_manager::{BackendHandleExt, ClientManager};
use crate::i18n::t;
use crate::state::BatchedSignal;
use crate::state::{ChatViewState, use_reactive_effect};
use crate::state::chat_data::format_file_size;
use poly_client::{
    Attachment, ChatCommand, ComposerSlot, Message, MessageContent, MessageReplyPreview,
};
use crate::ui::client_ui::{ComposerHooks, MessageActions};

use super::composer_helpers::{
    PendingAttachmentPreview,
    append_attachment_previews, apply_builtin_command,
    contextual_compose_placeholder, filtered_slash_commands,
    pending_attachment_to_attachment, reply_preview_snippet, slash_command_query,
};
use super::markup_ctx::ChatViewMarkupCtx;
use super::overlays::{ReplyComposerBar, SlashCommandPopup};
use super::super::draft_banner::DraftBanner;
use super::super::emoji_picker::EmojiPicker;
use super::super::media_picker::MediaPickerPopup;
use super::toggle_reaction_on_message;

/// Public re-export so message_row.rs can build the reply preview without
/// duplicating the snippet-extraction logic.
pub(super) fn reply_preview_snippet_pub(content: &MessageContent) -> String {
    reply_preview_snippet(content)
}

/// Bundled parameters for [`send_message`] to avoid the too-many-arguments lint.
struct SendMessageCtx {
    channel_id: String,
    text: String,
    attachments: Vec<Attachment>,
    reply_to_message_id: Option<String>,
    client_manager: BatchedSignal<ClientManager>,
    chat_view_state: BatchedSignal<ChatViewState>,
    nav: BatchedSignal<crate::state::NavState>,
    /// Reset to 0 after sending so the Jump to Present badge clears.
    new_messages_while_scrolled_up: Signal<u32>,
}

/// Send a message via the backend and prepend it to the message list.
async fn send_message(ctx: SendMessageCtx) {
    let SendMessageCtx {
        channel_id,
        text,
        attachments,
        reply_to_message_id,
        client_manager,
        chat_view_state,
        nav,
        mut new_messages_while_scrolled_up,
    } = ctx;
    // Resolve the backend: server channels use server_id lookup; DM channels fall back to
    // active_account_id so messages still send when no server is selected.
    let backend = {
        let state = nav.peek();
        if let Some(ref server_id) = *state.selected_server {
            client_manager
                .peek()
                .get_backend_for_server(server_id)
                .map(|(_id, b)| b)
        } else if let Some(ref account_id) = *state.active_account_id {
            client_manager.peek().get_backend(account_id)
        } else {
            None
        }
    };

    let Some(backend) = backend else {
        tracing::warn!("send_message: no backend found for channel {channel_id}");
        return;
    };

    let guard = match backend.read_with_timeout(std::time::Duration::from_secs(5)).await {
        Ok(g) => g,
        Err(_) => {
            tracing::warn!("chat_view: backend read timed out in send_message");
            return;
        }
    };
    let content = if attachments.is_empty() {
        MessageContent::Text(text)
    } else {
        MessageContent::WithAttachments { text, attachments }
    };
    let result = if let Some(reply_id) = reply_to_message_id {
        if let Some(mb) = guard.as_messaging() {
            mb.send_reply_message(&channel_id, &reply_id, content).await
        } else {
            guard.send_message(&channel_id, content).await
        }
    } else {
        guard.send_message(&channel_id, content).await
    };
    match result {
        Ok(msg) => {
            chat_view_state.batch(move |cv| cv.push_message(msg));
            // Always scroll to bottom when the user sends a message.
            new_messages_while_scrolled_up.set(0);
            super::super::chat_history::request_scroll_to_bottom();
        }
        Err(e) => {
            tracing::error!("Failed to send message: {e}");
        }
    }
}

pub(super) fn render_message_input_area(ctx: ChatViewMarkupCtx) -> Element {
    // Pack F (P59) — composer gating on read-only backends. HN / GitHub
    // declare `MessagingModel::ReadOnly`; replace the textarea+send with a
    // static notice so users don't type into a control that silently no-ops.
    let backend_slug = ctx.nav.read().active_backend.cloned()
        .map_or_else(|| "demo".to_string(), |b| b.slug().to_string());
    let composer_writable =
        ctx.client_manager.peek().capabilities_for_slug(&backend_slug).composer_writable();

    rsx! {
        div { class: "message-input-area",
            if !composer_writable {
                div {
                    class: "message-input-disabled message-input-readonly",
                    "{t(\"chat-readonly-notice\")}"
                }
            } else if ctx.channel_id.is_some() {
                {render_message_input_enabled(ctx)}
            } else {
                div { class: "message-input-disabled", {t("chat-select-channel")} }
            }
        }
    }
}

fn render_message_input_enabled(ctx: ChatViewMarkupCtx) -> Element {
    // B.4 — DraftBanner: read the active account + channel from app_state/nav.
    let (active_account_id, active_chat_id) = {
        let account_id = ctx.nav.read().active_account_id
            .as_deref()
            .unwrap_or("")
            .to_string();
        let chat_id = ctx.channel_id.clone().unwrap_or_default();
        (account_id, chat_id)
    };

    rsx! {
        // B.4 — Show pending agent drafts above the reply bar and composer.
        if !active_account_id.is_empty() && !active_chat_id.is_empty() {
            DraftBanner {
                account_id: active_account_id,
                chat_id: active_chat_id,
            }
        }
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

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
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

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
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
    let chat_view_state_for_composer = ctx.chat_view_state;
    let nav = ctx.nav;
    // Typing-mode persists per chat-view mount (i.e. across channel switches
    // within the same session). Owned here so the textarea oninput and the
    // toolbar button can both read it.
    let typing_mode = use_signal(|| TypingMode::Off);
    // Debounce flag for Real-mode typing dispatch: while true, no further
    // send_typing fires. Cleared 5s after each send.
    let typing_send_in_flight = use_signal(|| false);
    let composer_runtime = ComposerRuntimeCtx {
        message_input,
        command_suggestions,
        active_command_idx,
        show_command_popup,
        pending_attachments,
        reply_target,
        client_manager,
        chat_view_state: chat_view_state_for_composer,
        nav,
        new_messages_while_scrolled_up: ctx.new_messages_while_scrolled_up,
    };

    // WP 6 — plan-client-ui-surface §7 WP 6 / §4.5. Plugin-contributed
    // composer buttons are mounted per-slot around the input. The chat view
    // itself stays untouched (D8 preservation) — only these three
    // `ComposerHooks` instances hook into the plugin surface.
    let active_account_id = ctx.nav.read().active_account_id.cloned().unwrap_or_default();
    let channel_for_hooks = channel_id.clone().unwrap_or_default();
    let has_channel =
        !active_account_id.is_empty() && !channel_for_hooks.is_empty();

    rsx! {
        if has_channel {
            ComposerHooks {
                account_id: active_account_id.clone(),
                channel_id: channel_for_hooks.clone(),
                slot: ComposerSlot::AboveInput,
            }
        }
        div { class: "message-input-row",
            div { class: "message-input-shell",
                button {
                    class: "toolbar-btn composer-upload-btn",
                    title: t("chat-attach-file"),
                    onclick: move |_| open_composer_file_picker(),
                    "➕"
                }
                if has_channel {
                    ComposerHooks {
                        account_id: active_account_id.clone(),
                        channel_id: channel_for_hooks.clone(),
                        slot: ComposerSlot::LeftOfInput,
                    }
                }
                div { class: "message-input-text-area",
                    textarea {
                        class: "message-input",
                        id: "poly-message-composer",
                        placeholder: "{compose_placeholder}",
                        value: "{message_input}",
                        rows: "1",
                        oninput: {
                            let real_typing_channel = channel_id.clone();
                            move |evt| {
                                handle_composer_input(
                                    &evt.value(),
                                    message_input,
                                    command_suggestions,
                                    show_command_popup,
                                    active_command_idx,
                                );
                                if *typing_mode.peek() == TypingMode::Real {
                                    maybe_send_real_typing(
                                        real_typing_channel.clone(),
                                        typing_send_in_flight,
                                        nav,
                                        client_manager,
                                    );
                                }
                            }
                        },
                        onkeydown: {
                            let channel_id_send = channel_id.clone();
                            move |evt: KeyboardEvent| {
                                handle_composer_keydown(&evt, channel_id_send.clone(), composer_runtime);
                            }
                        },
                    }
                }
                {render_composer_toolbar(show_input_emoji, typing_mode)}
                if has_channel {
                    ComposerHooks {
                        account_id: active_account_id.clone(),
                        channel_id: channel_for_hooks.clone(),
                        slot: ComposerSlot::RightOfInput,
                    }
                }
                {render_send_button(ctx)}
            }
        }
    }
}

fn render_composer_toolbar(
    mut show_input_emoji: Signal<bool>,
    typing_mode: Signal<TypingMode>,
) -> Element {
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
            TypingModeButton { typing_mode }
        }
    }
}

/// Real-mode typing dispatch with a 5s debounce. The `in_flight` signal is
/// the rate-limit gate: while it's true, no further send_typing is fired.
/// Once a send completes, a 5s timer clears the flag.
fn maybe_send_real_typing(
    channel_id: Option<String>,
    mut in_flight: Signal<bool>,
    nav: BatchedSignal<crate::state::NavState>,
    client_manager: BatchedSignal<ClientManager>,
) {
    if *in_flight.peek() { return; }
    let Some(channel_id) = channel_id else { return };
    in_flight.set(true);
    let account_id = nav.read().active_account_id.cloned();
    let server_id = nav.read().selected_server.cloned();
    spawn(async move {
        let handle = if let Some(ref sid) = server_id {
            client_manager.peek().get_backend_for_server(sid).map(|(_, b)| b)
        } else if let Some(ref aid) = account_id {
            client_manager.peek().get_backend(aid)
        } else {
            None
        };
        if let Some(handle) = handle
            && let Ok(backend) = handle
                .read_with_timeout(std::time::Duration::from_secs(2))
                .await
            && let Some(mb) = backend.as_messaging() {
                drop(mb.send_typing(&channel_id).await);
            }
        #[cfg(target_arch = "wasm32")]
        gloo_timers::future::TimeoutFuture::new(5_000).await;
        #[cfg(not(target_arch = "wasm32"))]
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        in_flight.set(false);
    });
}

/// Three-state typing-indicator mode for the composer.
///
/// - `Off`: no typing indicators are sent (privacy default).
/// - `Real`: when the user types in the composer, fire `send_typing`
///   debounced to once per 5s. Mirrors the standard messenger UX.
/// - `Simulator`: one-click manual trigger that fires `send_typing`
///   every 5s for ~60s to signal "I'm watching this chat" without
///   actually composing — the original simulator the user wanted.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum TypingMode {
    #[default]
    Off,
    Real,
    Simulator,
}

impl TypingMode {
    fn next(self) -> Self {
        match self {
            Self::Off       => Self::Real,
            Self::Real      => Self::Simulator,
            Self::Simulator => Self::Off,
        }
    }
}

/// Cycles `Off → Real → Simulator → Off`. In Simulator state, also drives
/// the 60s typing-indicator heartbeat that mirrors chat-mcp's
/// `start_typing_simulation`. In Real state, the textarea oninput handler
/// is responsible for firing `send_typing` (debounced); this button only
/// owns the mode signal.
#[ui_action(inherit)]
#[context_menu(none)]
#[component]
fn TypingModeButton(mut typing_mode: Signal<TypingMode>) -> Element {
    let nav: BatchedSignal<crate::state::NavState> = use_context();
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let mode = *typing_mode.read();

    let (icon, label_key, class_extra) = match mode {
        TypingMode::Off       => ("🔕", "composer-typing-off",       ""),
        TypingMode::Real      => ("⌨️",  "composer-typing-real",      "typing-sim-btn-active"),
        TypingMode::Simulator => ("🎭", "composer-typing-simulator", "typing-sim-btn-active"),
    };
    let class = format!("toolbar-btn typing-sim-btn {class_extra}");
    let title = t(label_key);

    rsx! {
        button {
            class: "{class}",
            title: "{title}",
            onclick: move |_| {
                let next = typing_mode.peek().next();
                typing_mode.set(next);
                if next != TypingMode::Simulator { return; }

                // Simulator mode: fire-and-forget 60s heartbeat. Loop bails
                // out as soon as the user clicks again to leave Simulator.
                let channel_id = nav.read().selected_channel.cloned();
                let account_id = nav.read().active_account_id.cloned();
                let server_id = nav.read().selected_server.cloned();
                let Some(channel_id) = channel_id else { return };
                let mode_signal = typing_mode;
                spawn(async move {
                    for _ in 0_i32..12_i32 {
                        if *mode_signal.peek() != TypingMode::Simulator { break; }
                        let handle = if let Some(ref sid) = server_id {
                            client_manager.peek().get_backend_for_server(sid).map(|(_, b)| b)
                        } else if let Some(ref aid) = account_id {
                            client_manager.peek().get_backend(aid)
                        } else {
                            None
                        };
                        if let Some(handle) = handle
                            && let Ok(backend) = handle
                                .read_with_timeout(std::time::Duration::from_secs(2))
                                .await
                            && let Some(mb) = backend.as_messaging() {
                                drop(mb.send_typing(&channel_id).await);
                            }
                        #[cfg(target_arch = "wasm32")]
                        gloo_timers::future::TimeoutFuture::new(5_000).await;
                        #[cfg(not(target_arch = "wasm32"))]
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    }
                });
            },
            "{icon}"
        }
    }
}

fn open_composer_file_picker() {
    document::eval(
        r"
            let input = document.getElementById('poly-file-input');
            if (input) { input.click(); }
        ",
    );
}

fn handle_composer_input(
    value: &str,
    mut message_input: Signal<String>,
    command_suggestions: Signal<Vec<ChatCommand>>,
    mut show_command_popup: Signal<bool>,
    mut active_command_idx: Signal<usize>,
) {
    message_input.set(value.to_string());
    let trimmed = value.trim_start();
    if trimmed.starts_with('/') {
        let after_slash = trimmed.get(1..).unwrap_or("");
        if !after_slash.contains(' ') {
            let all_cmds = command_suggestions.read().clone();
            let matches = filtered_slash_commands(after_slash, &all_cmds);
            show_command_popup.set(!matches.is_empty());
            active_command_idx.set(0);
            return;
        }
    }
    show_command_popup.set(false);
}

#[derive(Clone, Copy)]
struct ComposerRuntimeCtx {
    message_input: Signal<String>,
    command_suggestions: Signal<Vec<ChatCommand>>,
    active_command_idx: Signal<usize>,
    show_command_popup: Signal<bool>,
    pending_attachments: Signal<Vec<PendingAttachmentPreview>>,
    reply_target: Signal<Option<MessageReplyPreview>>,
    client_manager: BatchedSignal<ClientManager>,
    chat_view_state: BatchedSignal<ChatViewState>,
    nav: BatchedSignal<crate::state::NavState>,
    new_messages_while_scrolled_up: Signal<u32>,
}

fn handle_composer_keydown(
    evt: &KeyboardEvent,
    channel_id_send: Option<String>,
    ctx: ComposerRuntimeCtx,
) {
    let mut message_input = ctx.message_input;
    let mut pending_attachments = ctx.pending_attachments;
    let mut reply_target = ctx.reply_target;
    let show_command_popup = ctx.show_command_popup;

    if *show_command_popup.read() && handle_slash_popup_navigation(evt, ctx) {
        return;
    }

    if evt.key() != Key::Enter {
        return;
    }

    // Shift+Enter → insert newline into the composer, don't send.
    if evt.modifiers().shift() {
        evt.prevent_default();
        let current = message_input.read().clone();
        message_input.set(format!("{current}\n"));
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
                chat_view_state: ctx.chat_view_state,
                nav: ctx.nav,
                new_messages_while_scrolled_up: ctx.new_messages_while_scrolled_up,
            })
            .await;
        });
    }
}

// lint-allow-unused: Dioxus Key has too many variants to enumerate; explicit Arrow/Esc/Tab/Enter handling intentional
#[allow(clippy::wildcard_enum_match_arm)]
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
                active_command_idx.set(cur.saturating_sub(1));
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
            if cur.saturating_add(1) < matches.len() {
                active_command_idx.set(cur.saturating_add(1));
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

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
fn render_send_button(ctx: ChatViewMarkupCtx) -> Element {
    let channel_id = ctx.channel_id.clone();
    let mut message_input = ctx.message_input;
    let mut pending_attachments = ctx.pending_attachments;
    let mut reply_target = ctx.reply_target;
    let client_manager = ctx.client_manager;
    let chat_view_state = ctx.chat_view_state;
    let nav = ctx.nav;
    let new_messages_while_scrolled_up = ctx.new_messages_while_scrolled_up;

    let has_content = !message_input.read().is_empty() || !pending_attachments.read().is_empty();
    rsx! {
        button {
            class: if has_content { "toolbar-btn chat-send-btn chat-send-btn-active" } else { "toolbar-btn chat-send-btn" },
            disabled: !has_content,
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
                                chat_view_state,
                                nav,
                                new_messages_while_scrolled_up,
                            })
                            .await;
                    });
                }
            },
            "➤"
        }
    }
}

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
pub(super) fn render_hidden_file_input(ctx: ChatViewMarkupCtx) -> Element {
    // Only read from inside the wasm32-gated onchange handler below; on native
    // the file input renders without a handler attached.
    #[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
    let pending_attachments = ctx.pending_attachments;
    rsx! {
        input {
            r#type: "file",
            id: "poly-file-input",
            multiple: true,
            style: "display:none;",
            onchange: move |_evt| {
                #[cfg(target_arch = "wasm32")]
                {
                    use dioxus::html::HasFileData;
                    let files = _evt.files();
                    if !files.is_empty() {
                        spawn(async move {
                            append_attachment_previews(pending_attachments, files).await;
                        });
                    }
                }
            },
        }
    }
}

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
pub(super) fn render_input_emoji_picker(ctx: ChatViewMarkupCtx) -> Element {
    let mut message_input = ctx.message_input;
    let mut show_input_emoji = ctx.show_input_emoji;
    let markdown_enabled = ctx.markdown_enabled;
    let channel_id = ctx.channel_id.clone();
    let nav = ctx.nav;
    let client_manager = ctx.client_manager;

    // Hooks must be called before any early return for stable hook ordering.
    // Load custom emojis for the current channel on mount.
    let custom_emojis = use_signal(Vec::<poly_client::CustomEmoji>::new);
    use_reactive_effect(channel_id.clone(), move |channel_id| {
        let mut custom_emojis = custom_emojis;
        let client_manager = client_manager;
        spawn(async move {
            let Some(ref cid) = channel_id else { return };
            let Some(account_id) = nav.peek().active_account_id.cloned() else { return };
            if let Ok(emojis) = client_manager.peek().with_backend(&account_id, async |b| {
                match b.as_messaging() {
                    Some(mb) => mb.get_available_emojis(cid).await,
                    None => Ok(Vec::new()),
                }
            }).await {
                custom_emojis.set(emojis);
            }
        });
    });

    if !*show_input_emoji.read() {
        return rsx! {};
    }

    rsx! {
        MediaPickerPopup {
            on_emoji_select: move |emoji: String| {
                let current = message_input.read().clone();
                message_input.set(format!("{current}{emoji}"));
                show_input_emoji.set(false);
            },
            on_close: move |()| show_input_emoji.set(false),
            markdown_enabled,
            custom_emojis: custom_emojis.read().clone(),
        }
    }
}

/// Render the chat overlays layer: emoji reaction picker and message context menu.
// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
pub(super) fn render_chat_overlays(ctx: ChatViewMarkupCtx) -> Element {
    let reaction_picker_id = ctx.reaction_picker_id.clone();
    let mut reaction_picker_msg = ctx.reaction_picker_msg;
    let msg_context_menu = ctx.msg_context_menu;
    let chat_view_state = ctx.chat_view_state;

    rsx! {
        if let Some(ref picker_msg_id) = reaction_picker_id {
            EmojiPicker {
                on_select: {
                    let msg_id = picker_msg_id.clone();
                    move |emoji: String| {
                        toggle_reaction_on_message(chat_view_state, &msg_id, &emoji);
                        reaction_picker_msg.set(None);
                    }
                },
                on_close: move |()| reaction_picker_msg.set(None),
            }
        }
        if msg_context_menu.read().is_some() {
            super::overlays::MsgContextMenuOverlay { msg_context_menu }
        }
    }
}
