//! Per-message rendering — full message rows, grouped rows, message actions,
//! and the content-stack that stacks reply preview / content / attachments /
//! reactions / thread button.
//!
//! Single responsibility: turn a `Message` value + the shared `ChatViewMarkupCtx`
//! into RSX. No signal writes, no async, no effects — pure render helpers.

use dioxus::prelude::*;

use crate::i18n::t;
use crate::state::chat_data::user_color;
use poly_client::{Message, MessageReplyPreview, PresenceStatus};
use poly_ui_macros::{context_menu, ui_action};

use super::markup_ctx::ChatViewMarkupCtx;
use super::message_plain_text;
use super::MsgContextMenu;
use super::composer::reply_preview_snippet_pub;
use super::overlays::{
    AttachmentsView, MessageContentView, MessageInlineEdit, MessageReplyPreviewLine, ReactionsView,
};
use crate::ui::client_ui::MessageActions;
use super::super::thread_view::ViewThreadButton;

const GROUP_THRESHOLD_MINUTES: i64 = 7;

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
pub(super) fn render_message_row(
    ctx: ChatViewMarkupCtx,
    msg: Message,
    prev_msg: Option<Message>,
) -> Element {
    let show_date_sep = match prev_msg.as_ref() {
        Some(prev) => msg.timestamp.date_naive() != prev.timestamp.date_naive(),
        None => true,
    };
    let is_grouped = match prev_msg.as_ref() {
        Some(prev) => {
            prev.author.id == msg.author.id
                && !show_date_sep
                && msg.timestamp.signed_duration_since(prev.timestamp).num_minutes()
                    < GROUP_THRESHOLD_MINUTES
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
    let unread_divider_visible = ctx.history_state.read().unread_divider_visible;
    let unread_marker_id = ctx.unread_marker_id.clone();
    let msg_context_menu_signal = ctx.msg_context_menu;
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
        if unread_marker_id.as_deref() == Some(msg_id.as_str()) && unread_divider_visible {
            div { class: "message-unread-divider",
                div { class: "message-unread-divider-line" }
                span { class: "message-unread-divider-label", "{t(\"chat-unread-divider\")}" }
            }
        }
        div {
            id: "message-{msg_id}",
            "data-testid": "message-row-{msg_id}",
            class: {
                let base = if is_grouped { "message message-grouped" } else { "message message-full" };
                if is_editing { format!("{base} message-editing") } else { base.to_string() }
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
                                image_attachment: None,
                            }),
                        );
                }
            },

            {render_message_actions(ctx.clone(), msg_for_actions, is_own)}
            if is_grouped {
                {render_grouped_message_body(ctx, msg_for_grouped, time_str, is_editing)}
            } else {
                {render_full_message_body(ctx, msg, time_str, is_editing)}
            }
        }
    }
}

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
fn render_message_actions(
    ctx: ChatViewMarkupCtx,
    msg: Message,
    is_own: bool,
) -> Element {

    let mut reaction_picker_msg = ctx.reaction_picker_msg;
    let mut edit_draft = ctx.edit_draft;
    let mut editing_msg_id = ctx.editing_msg_id;
    let mut reply_target = ctx.reply_target;
    let chat_view_state = ctx.chat_view_state;
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
                        move |_| {
                            let mid_c = mid.clone();
                            chat_view_state.batch(move |cv| cv.messages.retain(|m| m.id != mid_c));
                        }
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
                        snippet: reply_preview_snippet_pub(&msg.content),
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
            // WP 6 — plan-client-ui-surface §7 WP 6 / §4.5. Plugin-declared
            // per-message actions render *after* host universal items so
            // host controls stay in stable positions.
            {
                let account_id = ctx.nav.read().active_account_id.cloned().unwrap_or_default();
                let channel_id = ctx.channel_id.clone().unwrap_or_default();
                if !account_id.is_empty() && !channel_id.is_empty() {
                    rsx! {
                        MessageActions {
                            account_id,
                            channel_id,
                            message_id: msg_id.clone(),
                        }
                    }
                } else {
                    rsx! {}
                }
            }
        }
    }
}

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
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
                span {
                    class: "message-author",
                    style: "color: {color};",
                    "{msg.author.display_name}"
                }
                span { class: "message-time", "{time_str}" }
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

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
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
                chat_view_state: ctx.chat_view_state,
            }
        } else {
            MessageContentView { content: msg.content.clone(), edited: msg.edited }
        }
        if !msg.attachments.is_empty() {
            AttachmentsView {
                attachments: msg.attachments.clone(),
                message_id: msg.id.clone(),
                msg_context_menu: ctx.msg_context_menu,
                message_text: message_plain_text(&msg.content),
                is_own: msg.author.id == ctx.self_user_id,
            }
        }
        if !msg.reactions.is_empty() {
            ReactionsView { reactions: msg.reactions.clone(), message_id: msg.id.clone() }
        }
        // 5.1 — "View Thread" button for messages that spawned a thread.
        if let Some(thread_info) = msg.thread.clone() {
            ViewThreadButton { thread: thread_info }
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
    } else if local.date_naive()
        == now
            .checked_sub_signed(chrono::Duration::days(1))
            .unwrap_or(now)
            .date_naive()
    {
        format!("Yesterday {}", local.format("%I:%M %p"))
    } else {
        local.format("%m/%d/%Y %I:%M %p").to_string()
    }
}
