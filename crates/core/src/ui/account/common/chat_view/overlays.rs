//! Chat overlay components — message content rendering, context menu, inline
//! editing, reaction pills, typing indicator, DM contact panel, and markdown.
//!
//! Single responsibility: all Dioxus `#[component]` definitions that render
//! *on top of* or *inside* message rows, plus the per-message context menu.
//! No scroll logic, no layout, no composer code lives here.

use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

use crate::client_manager::BackendHandleExt;
use crate::i18n::{t, t_args};
use crate::state::chat_data::{format_file_size, user_color};
use crate::state::BatchedSignal;
use crate::state::{AccountSessions, ChatLists, ChatViewState, NavState, UiOverlays, UserPrefs};
use poly_client::{
    ChatCommand, DmChannel, Message, MessageContent, MessageReplyPreview,
    PresenceStatus, User,
};

use crate::ui::routes::Route;
use super::MsgContextMenu;
use super::toggle_reaction_on_message;
use super::super::user_profile_modal::open_user_profile;

/// Apply an inline edit to a message in the chat data.
///
/// Sets `edited = true` on the message and replaces its content with the new text.
pub(super) fn apply_edit(
    chat_view_state: BatchedSignal<ChatViewState>,
    message_id: &str,
    new_text: String,
) {
    chat_view_state.batch(|cv| {
        if let Some(msg) = cv.messages.iter_mut().find(|m| m.id == message_id) {
            msg.content = MessageContent::Text(new_text);
            msg.edited = true;
        }
    });
}

/// Inline edit UI rendered in place of the message content while editing.
///
/// Shows a textarea pre-filled with the current message text, a Cancel button,
/// and a Save button. Enter (without Shift) saves; Escape cancels.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(allow_default)]
#[component]
pub(super) fn MessageInlineEdit(
    message_id: String,
    editing_msg_id: Signal<Option<String>>,
    edit_draft: Signal<String>,
    chat_view_state: BatchedSignal<ChatViewState>,
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
                            apply_edit(chat_view_state, &mid, new_text);
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
                                apply_edit(chat_view_state, &mid, new_text);
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
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub(super) fn MsgContextMenuOverlay(
    msg_context_menu: Signal<Option<MsgContextMenu>>,
) -> Element {
    let Some(menu) = msg_context_menu.read().clone() else {
        return rsx! {};
    };

    let nav_state: BatchedSignal<NavState> = use_context();
    let user_prefs: BatchedSignal<UserPrefs> = use_context();
    let last_known_perms = user_prefs.read().last_known_perms.clone();
    let client_manager: BatchedSignal<crate::client_manager::ClientManager> = use_context();
    let chat_view_state: BatchedSignal<ChatViewState> = use_context();

    // Resolve account_id and channel_id for the delete_message backend call.
    let account_id_for_delete = nav_state.read().active_account_id
        .as_deref()
        .unwrap_or("")
        .to_string();
    let channel_id_for_delete = chat_view_state
        .read()
        .current_channel
        .as_ref()
        .map(|c| c.id.clone())
        .unwrap_or_default();

    let x = menu.x;
    let y = menu.y;
    let is_own = menu.is_own;
    let mid_delete = menu.message_id.clone();
    let mid_copy_id = menu.message_id.clone();
    let txt_copy = menu.message_text.clone();
    let image_att = menu.image_attachment.clone();

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
                    chat_view_state,
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

            {render_context_menu_image_items(msg_context_menu, image_att)}

            div { class: "context-menu-separator" }

            {render_context_menu_danger_item(is_own, last_known_perms, msg_context_menu, chat_view_state, mid_delete, channel_id_for_delete, account_id_for_delete, client_manager)}
            {render_context_menu_copy_id_item(msg_context_menu, mid_copy_id)}
        }
    }
}

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
fn render_context_menu_quick_reactions(
    message_id: String,
    mut msg_context_menu: Signal<Option<MsgContextMenu>>,
    chat_view_state: BatchedSignal<ChatViewState>,
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
                                toggle_reaction_on_message(chat_view_state, &mid, &e);
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

// lint-allow-unused: signal/text helper-style render fn called inline from rsx!; 8 args is fewer than the alternative struct-of-signals
#[allow(clippy::needless_pass_by_value, clippy::too_many_arguments)]
fn render_context_menu_danger_item(
    is_own: bool,
    last_known_perms: Option<poly_client::MemberPermissions>,
    mut msg_context_menu: Signal<Option<MsgContextMenu>>,
    chat_view_state: BatchedSignal<ChatViewState>,
    mid_delete: String,
    channel_id: String,
    account_id: String,
    client_manager: BatchedSignal<crate::client_manager::ClientManager>,
) -> Element {
    // Show the delete action if the user owns the message OR has manage_messages.
    let can_delete = is_own
        || last_known_perms
            .as_ref()
            .is_some_and(|p| p.manage_messages);

    if !can_delete {
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
            label: t("mod-action-delete-message"),
            danger: true,
            onclick: move |_| {
                // Optimistic local removal.
                {
                    let mid_c = mid_delete.clone();
                    chat_view_state.batch(move |cv| cv.messages.retain(|message| message.id != mid_c));
                }
                msg_context_menu.set(None);
                // Fire backend delete_message (best-effort; local removal already applied).
                if !channel_id.is_empty() && !account_id.is_empty() {
                    let cid = channel_id.clone();
                    let mid = mid_delete.clone();
                    let aid = account_id.clone();
                    spawn(async move {
                        if let Err(e) = client_manager.peek().with_backend(&aid, async |b| {
                            match b.as_moderation() {
                                Some(m) => m.delete_message(&cid, &mid).await,
                                None => Err(poly_client::ClientError::NotSupported("delete_message".to_string())),
                            }
                        }).await {
                            tracing::warn!("delete_message failed: {e}");
                        }
                    });
                }
            },
        }
    }
}

/// Append the four Discord-parity image actions when the right-click landed
/// on an image attachment. Renders nothing for text-only messages.
fn render_context_menu_image_items(
    mut msg_context_menu: Signal<Option<MsgContextMenu>>,
    image_att: Option<(String, String)>,
) -> Element {
    let Some((url, filename)) = image_att else {
        return rsx! {};
    };
    let url_for_copy = url.clone();
    let url_for_save = url.clone();
    let url_for_link_copy = url.clone();
    let url_for_link_open = url.clone();
    let name_for_save = filename.clone();
    rsx! {
        div { class: "context-menu-separator" }
        ContextMenuItemSimple {
            label: t("attachment-menu-copy-image"),
            onclick: move |_| {
                let js = format!(
                    "(async () => {{ try {{ const r = await fetch({u}); const b = await r.blob(); await navigator.clipboard.write([new ClipboardItem({{[b.type]: b}})]); }} catch (e) {{ console.warn('copy image failed:', e); }} }})();",
                    u = serde_json::to_string(&url_for_copy).unwrap_or_default(),
                );
                document::eval(&js);
                msg_context_menu.set(None);
            },
        }
        ContextMenuItemSimple {
            label: t("attachment-menu-save-image"),
            onclick: move |_| {
                let js = format!(
                    "(() => {{ const a = document.createElement('a'); a.href = {u}; a.download = {n}; a.target = '_blank'; a.rel = 'noopener noreferrer'; document.body.appendChild(a); a.click(); a.remove(); }})();",
                    u = serde_json::to_string(&url_for_save).unwrap_or_default(),
                    n = serde_json::to_string(&name_for_save).unwrap_or_default(),
                );
                document::eval(&js);
                msg_context_menu.set(None);
            },
        }
        ContextMenuItemSimple {
            label: t("attachment-menu-copy-link"),
            onclick: move |_| {
                let js = format!(
                    "navigator.clipboard.writeText({u}).catch((e) => console.warn('copy link failed:', e));",
                    u = serde_json::to_string(&url_for_link_copy).unwrap_or_default(),
                );
                document::eval(&js);
                msg_context_menu.set(None);
            },
        }
        ContextMenuItemSimple {
            label: t("attachment-menu-open-link"),
            onclick: move |_| {
                let js = format!(
                    "window.open({u}, '_blank', 'noopener,noreferrer');",
                    u = serde_json::to_string(&url_for_link_open).unwrap_or_default(),
                );
                document::eval(&js);
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
#[ui_action(inherit)]
#[context_menu(inherit)]
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
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub(super) fn MessageReplyPreviewLine(reply: MessageReplyPreview) -> Element {
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
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub(super) fn ReplyComposerBar(reply: MessageReplyPreview, on_cancel: EventHandler<MouseEvent>) -> Element {
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
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub(super) fn SlashCommandPopup(
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

/// Render message text content, handling multi-line and edited indicator.
/// allow_default so rendered `<a>` anchors inside `.message-markdown` get the
/// OS "Open link / Copy link / Save link as" native context menu.
#[rustfmt::skip]
#[ui_action(None)]
#[context_menu(allow_default)]
#[component]
pub(super) fn MessageContentView(content: MessageContent, edited: bool) -> Element {
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

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests for the markdown render pipeline (F12 regression)
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod markdown_tests {
    use super::render_markdown_html;

    /// F12 regression: em-dash (U+2014) must survive the
    /// pulldown_cmark → ammonia → String pipeline intact.
    /// Previously `strip_data_href_on_anchors` in the `sanitize_html`
    /// path cast `bytes[i] as char`, turning 0xE2 into 'â' etc.
    /// The markdown path never called that function, but we pin the
    /// invariant here so any future refactor that routes markdown through
    /// `sanitize_html` will still pass.
    #[test]
    fn em_dash_preserved() {
        // Real em-dash in the input, not the escape sequence
        let out = render_markdown_html("hello\u{2014}world");
        assert!(
            out.contains("\u{2014}"),
            "em-dash mangled in markdown render; got: {out:?}"
        );
        assert!(
            !out.contains('\u{00E2}'),
            "mojibake 'â' in markdown render; got: {out:?}"
        );
    }

    #[test]
    fn multibyte_chars_preserved() {
        // Accented, CJK, em-dash, emoji all in one message
        let input = "caf\u{00E9}\u{2014}日本語\u{2014}\u{00F1}\u{2014}\u{1F389}";
        let out = render_markdown_html(input);
        assert!(out.contains('\u{00E9}'), "é lost; got: {out:?}");
        assert!(out.contains('\u{2014}'), "em-dash lost; got: {out:?}");
        assert!(out.contains("日本語"), "CJK lost; got: {out:?}");
        assert!(out.contains('\u{00F1}'), "ñ lost; got: {out:?}");
        assert!(out.contains('\u{1F389}'), "🎉 lost; got: {out:?}");
    }

    #[test]
    fn em_dash_in_markdown_bold() {
        // em-dash adjacent to bold formatting (ensures pulldown_cmark
        // emits it correctly when surrounding markdown is parsed)
        let out = render_markdown_html("**hello**\u{2014}world");
        assert!(out.contains('\u{2014}'), "em-dash lost next to bold; got: {out:?}");
        assert!(out.contains("<strong>"), "bold lost; got: {out:?}");
    }
}

/// Render attachments (images inline, non-images as links).
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub(super) fn AttachmentsView(
    attachments: Vec<poly_client::Attachment>,
    message_id: String,
    msg_context_menu: Signal<Option<MsgContextMenu>>,
    message_text: String,
    is_own: bool,
) -> Element {
    let nav_state: BatchedSignal<NavState> = use_context();
    let router_nav = navigator();

    rsx! {
        div { class: "message-attachments",
            for (attachment_index, att) in attachments.iter().enumerate() {
                {
                    let is_image = att.content_type.starts_with("image/");
                    let filename = att.filename.clone();
                    let size_str = format_file_size(att.size);
                    let url = att.url.clone();
                    let msg_id = message_id.clone();
                    let idx = attachment_index;

                    if is_image {
                        let cm_url = url.clone();
                        let cm_filename = filename.clone();
                        let cm_msg_id = msg_id.clone();
                        let cm_text = message_text.clone();
                        let mut msg_context_menu = msg_context_menu;
                        rsx! {
                            div {
                                class: "attachment-image",
                                // Right-click on an image opens the regular message context menu
                                // (reactions, reply, forward, copy text, …) AND appends the four
                                // image actions (Copy / Save / Copy Link / Open Link) keyed to
                                // THIS specific attachment via `image_attachment`.
                                oncontextmenu: move |evt: MouseEvent| {
                                    evt.prevent_default();
                                    evt.stop_propagation();
                                    let coords = evt.client_coordinates();
                                    msg_context_menu.set(Some(MsgContextMenu {
                                        x: coords.x,
                                        y: coords.y,
                                        message_id: cm_msg_id.clone(),
                                        message_text: cm_text.clone(),
                                        is_own,
                                        image_attachment: Some((cm_url.clone(), cm_filename.clone())),
                                    }));
                                },
                                onclick: move |_| {
                                    let nav_snap = nav_state.read();
                                    let Some(backend) = nav_snap.active_backend.cloned() else {
                                        return;
                                    };
                                    let Some(instance_id) = nav_snap.active_instance_id.cloned() else {
                                        return;
                                    };
                                    let Some(account_id) = nav_snap.active_account_id.cloned() else {
                                        return;
                                    };
                                    let Some(channel_id) = nav_snap.selected_channel.cloned() else {
                                        return;
                                    };

                                    if let Some(server_id) = nav_snap.selected_server.cloned() {
                                        router_nav.push(Route::ServerMediaViewerRoute {
                                            backend: backend.slug().to_string(),
                                            instance_id,
                                            account_id,
                                            server_id,
                                            channel_id,
                                            message_id: msg_id.clone(),
                                            attachment_index: idx,
                                        });
                                    } else {
                                        router_nav.push(Route::DmMediaViewerRoute {
                                            backend: backend.slug().to_string(),
                                            instance_id,
                                            account_id,
                                            dm_id: channel_id,
                                            message_id: msg_id.clone(),
                                            attachment_index: idx,
                                        });
                                    }
                                },
                                img {
                                    src: "{url}",
                                    alt: "{filename}",
                                    loading: "lazy",
                                }
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
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub(super) fn ReactionsView(reactions: Vec<poly_client::Reaction>, message_id: String) -> Element {
    let chat_view_state: BatchedSignal<ChatViewState> = use_context();
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
                                toggle_reaction_on_message(chat_view_state, &mid, &emoji_click);
                            },
                            "{emoji} {count}"
                        }
                    }
                }
            }
        }
    }
}

/// Typing indicator shown above the message input when users are typing.
#[rustfmt::skip]
#[ui_action(None)]
#[context_menu(none)]
#[component]
pub(super) fn TypingIndicator() -> Element {
    let chat_view_state: BatchedSignal<ChatViewState> = use_context();
    let typing = chat_view_state.read().typing_users.clone();

    if typing.is_empty() {
        return rsx! {};
    }

    let text = match typing.len() {
        1 => t_args(
            "chat-typing",
            &[("user", typing.first().map_or("", |s| s.as_str()))],
        ),
        n => {
            let count_str = n.to_string();
            t_args("chat-typing-multiple", &[("count", count_str.as_str())])
        }
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

#[rustfmt::skip]
#[ui_action(None)]
#[context_menu(none)]
#[component]
pub(super) fn DmContactListPanel(channel_id: String) -> Element {
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    let account_sessions: BatchedSignal<AccountSessions> = use_context();
    let nav_state: BatchedSignal<NavState> = use_context();

    let active_account_id = nav_state.read().active_account_id.cloned().unwrap_or_default();

    // The other person in this 1:1 DM
    let dm: Option<DmChannel> = chat_lists
        .read()
        .dm_channels
        .iter()
        .find(|dm| dm.id == channel_id)
        .cloned();

    // The current user ("you") — from the active session
    let self_user: Option<User> = account_sessions
        .read()
        .account_sessions
        .get(&active_account_id)
        .map(|s| s.user.clone());

    rsx! {
        aside { class: "user-sidebar dm-contact-list-panel",
            div { class: "chat-utility-header user-sidebar-header",
                h3 { class: "chat-utility-title user-sidebar-title", {t("user-members")} }
            }
            div { class: "chat-utility-body user-sidebar-body",
                if let Some(ref dm) = dm {
                    DmContactRow { user: dm.user.clone() }
                } else {
                    div { class: "user-sidebar-empty", {t("user-no-members")} }
                }
                if let Some(self_u) = self_user {
                    DmContactRow { user: self_u }
                }
            }
        }
    }
}

/// A single contact row in the 1:1 DM contact panel.
///
/// Uses the `user-avatar-wrap` + explicit `span.presence-dot` pattern so the dot
/// is never clipped by `overflow: hidden` on `.user-avatar`.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(UserRowContextMenu)]
#[component]
fn DmContactRow(user: User) -> Element {
    let ui_overlays: BatchedSignal<UiOverlays> = use_context();
    let color = user_color(&user.id);
    let first_char: String = user
        .display_name
        .chars()
        .next()
        .map(|c| c.to_string())
        .unwrap_or_default();
    let dot_class: &'static str = match user.presence {
        PresenceStatus::Online => "presence-dot online",
        PresenceStatus::Idle => "presence-dot idle",
        PresenceStatus::DoNotDisturb => "presence-dot dnd",
        PresenceStatus::Offline | PresenceStatus::Invisible | PresenceStatus::Unknown => "",
    };
    let entry_class = if matches!(user.presence, PresenceStatus::Offline | PresenceStatus::Invisible) {
        "user-entry offline"
    } else {
        "user-entry"
    };
    let name = user.display_name.clone();
    let avatar_url = user.avatar_url.clone();
    let user_clone = user.clone();

    rsx! {
        div {
            class: "{entry_class}",
            onclick: move |_| open_user_profile(ui_overlays, user_clone.clone()),
            div { class: "user-avatar-wrap",
                div { class: "user-avatar",
                    if let Some(ref url) = avatar_url {
                        img { class: "user-avatar-image", src: "{url}", alt: "{name}" }
                    } else {
                        div {
                            class: "user-avatar-fallback",
                            style: "background-color: {color};",
                            "{first_char}"
                        }
                    }
                }
                if !dot_class.is_empty() {
                    span { class: "{dot_class}" }
                }
            }
            span { class: "user-name", "{name}" }
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

pub(super) fn render_markdown_html(text: &str) -> String {
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
