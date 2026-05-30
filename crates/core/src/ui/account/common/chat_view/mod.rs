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
//!
//! # Sub-module layout
//!
//! | Module | Responsibility |
//! |--------|---------------|
//! | `drag` | Drag-over file-drop overlay |
//! | `scroll` | Scroll loop, history paging, message list DOM |
//! | `message_row` | Per-message row rendering (full + grouped) |
//! | `composer` | Composer input, send, typing mode |
//! | `overlays` | Components: MessageContentView, AttachmentsView, reactions, context menu |
//! | `layout` | Two-column shell, header, side column, utility rail |
//! | `effects` | Dioxus hooks wired up in `use_chat_view_effects` |
//! | `signals` | `ChatViewSignals` — signal handle bundle |
//! | `markup_ctx` | `ChatViewMarkupCtx` — render-time snapshot bundle |
//! | `virtualization` | Virtual-window windowing for large message lists |
//! | `search_filter` | Search filter option types + rendering |
//! | `composer_helpers` | Low-level composer utilities (attachment, slash cmd) |
//! | `header` | `ChatHeaderActions` component + header buttons |
//! | `utility_rail` | `ChatUtilityRail` — search/pinned/threads side panel |

mod virtualization;
use virtualization::*;
mod header;
use header::{ChatHeaderActions, render_search_tab_button, render_agent_toggle_button};
mod utility_rail;
use utility_rail::ChatUtilityRail;
mod composer_helpers;
mod effects;
mod signals;
mod markup_ctx;
use composer_helpers::{
    PendingAttachmentPreview,
    append_attachment_previews, apply_builtin_command,
    contextual_compose_placeholder, filtered_slash_commands,
    pending_attachment_to_attachment, reply_preview_snippet, slash_command_query,
};
mod search_filter;
use search_filter::{
    SearchFilterOption,
    apply_search_filter_completion,
    build_search_filter_options, build_search_query,
    contextual_search_placeholder, filter_search_filter_options,
    message_search_terms,
    render_chat_header_search,
};
use signals::{ChatViewSignals, use_chat_view_signals};
use markup_ctx::{ChatViewMarkupCtx, build_chat_view_markup_ctx};

// New SOLID-split sub-modules (Phase C.1)
mod drag;
mod scroll;
mod message_row;
mod overlays;
mod composer;
mod layout;

use crate::state::BatchedSignal;
use super::super::super::routes::Route;
use super::chat_history::{
    ChatHistoryUiState, MAX_LOADED_MESSAGES, OLDER_MESSAGES_PAGE_SIZE, read_message_list_anchor,
    remember_message_list_scroll_position, request_preserve_message_anchor,
    request_preserve_scroll_position, request_preserve_scroll_position_from_bottom,
    request_scroll_to_bottom, request_scroll_to_bottom_deferred, unread_marker_message_id,
};
use super::direct_call::{DirectCallRequest, navigate_to_pending_direct_call_from_active_account};
use super::agent_panel::AgentPanel;
use super::dm_user_sidebar::DmUserSidebar;
use super::emoji_picker::EmojiPicker;
use super::media_picker::MediaPickerPopup;
use super::draft_banner::{DraftBanner, DraftsSidebar};
use super::thread_view::{ActiveThreadsBar, ThreadPanel, ViewThreadButton};
use super::user_profile_modal::open_user_profile;
use super::user_sidebar::UserSidebar;
use crate::client_manager::{BackendHandleExt, ClientManager};
use crate::ui::client_ui::{ComposerHooks, MessageActions};
use poly_client::ComposerSlot;
use crate::i18n::{t, t_args};
use crate::state::chat_data::{backend_badge, format_file_size, user_color};
use crate::state::{AccountSessions, ChatLists, ChatViewState, UiOverlays, VoiceState, use_reactive_effect, use_spawn_once};
use crate::ui::split_shell::RightWingShell;
use dioxus::html::HasFileData;
use dioxus::prelude::*;
use poly_client::{
    Attachment, BackendType, Channel, ChatCommand, DmChannel, Message,
    MessageContent, MessageQuery, MessageReplyPreview, MessageSearchHit,
    MessagingBackend, PresenceStatus, ServerAdminBackend, User,
};
use poly_ui_macros::{context_menu, ui_action};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// Treat history paging as an exact-edge action: only trigger when the scroll
/// position has actually reached the top/bottom sentinel boundary (with a tiny
/// epsilon for browser rounding noise).
const MESSAGE_HISTORY_EDGE_THRESHOLD_PX: f64 = 1.0;
const MESSAGE_HISTORY_EDGE_REARM_PX: f64 = 48.0;
/// When the user re-enters the bottom sentinel, fetch multiple newer pages in a
/// single async burst and swap the final 200-message working set only once.
/// This avoids visibly "attaching" rows page-by-page and clears the bottom
/// spacer once the real latest message has been reached.
const MAX_CHAINED_NEWER_HISTORY_PAGES: usize = 20;
/// Distance from the scroll bottom (in pixels) beyond which the "Jump to Present"
/// button appears. Matches roughly one viewport height of buffer.
const JUMP_TO_PRESENT_THRESHOLD_PX: f64 = 200.0;

#[cfg(target_arch = "wasm32")]
const MOBILE_RIGHT_WING_OPEN_JS: &str = "window.__polySetMobileRightWingOpen?.(true);";
#[cfg(target_arch = "wasm32")]
const MOBILE_RIGHT_WING_CLOSE_JS: &str = "window.__polySetMobileRightWingOpen?.(false);";

#[derive(Debug, Clone)]
struct MsgContextMenu {
    x: f64,
    y: f64,
    message_id: String,
    message_text: String,
    is_own: bool,
    /// Set when the right-click landed on a specific image attachment.
    /// `(url, filename)`. The MsgContextMenuOverlay appends the four
    /// Discord-parity image actions (Copy / Save / Copy Link / Open Link)
    /// for this attachment when present.
    image_attachment: Option<(String, String)>,
}

const GROUP_THRESHOLD_MINUTES: i64 = 7;


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ChatUtilityPanel {
    Search,
    Pinned,
    Threads,
    Settings,
    /// B.5 — pending agent drafts across all chats for the active account.
    Drafts,
    /// Per-chat agent panel — memory + drafts + style + access toggle.
    /// Lives in the same right wing as Search/Pinned/Threads so the user
    /// can switch between tabs without losing the agent context.
    Agent,
}


pub(super) fn message_plain_text(content: &MessageContent) -> String {
    match content {
        MessageContent::Text(text) | MessageContent::WithAttachments { text, .. } => text.clone(),
    }
}



pub(crate) fn highlight_message(message_id: &str) {
    let dom_id = format!("message-{message_id}");
    document::eval(&format!(
        "setTimeout(() => {{ const el = document.getElementById('{dom_id}'); if (el) {{ el.scrollIntoView({{behavior: 'smooth', block: 'center'}}); el.classList.add('message-search-hit'); setTimeout(() => el.classList.remove('message-search-hit'), 1400); }} }}, 80);"
    ));
}

fn current_channel_unread_count(
    channel_id: Option<&str>,
    current_channel: Option<&Channel>,
    chat_lists: &ChatLists,
) -> u32 {
    let Some(channel_id) = channel_id else {
        return 0;
    };

    if let Some(dm) = chat_lists.dm_channel_by_id(channel_id) {
        return dm.unread_count;
    }

    current_channel
        .filter(|channel| channel.id == channel_id)
        .map_or(0, |channel| channel.unread_count)
}

pub(crate) fn mark_channel_as_read_with_backend(
    chat_lists: BatchedSignal<ChatLists>,
    chat_view_state: BatchedSignal<ChatViewState>,
    client_manager: BatchedSignal<crate::client_manager::ClientManager>,
    account_id: Option<String>,
    server_id: Option<String>,
    channel_id: &str,
) -> u32 {
    let cleared = mark_channel_as_read(chat_lists, chat_view_state, channel_id);
    // Fire-and-forget: tell the backend so the next get_channels refetch
    // returns unread_count=0 for this channel.
    let cid = channel_id.to_string();
    spawn(async move {
        let handle = if let Some(sid) = server_id {
            client_manager.peek().get_backend_for_server(&sid).map(|(_, b)| b)
        } else if let Some(aid) = account_id {
            client_manager.peek().get_backend(&aid)
        } else {
            None
        };
        if let Some(handle) = handle
            && let Ok(backend) = handle
                .read_with_timeout(std::time::Duration::from_secs(5))
                .await
            && let Some(sa) = backend.as_server_admin() {
                drop(sa.mark_channel_read(&cid).await);
            }
    });
    cleared
}

pub(crate) fn mark_channel_as_read(
    chat_lists: BatchedSignal<ChatLists>,
    chat_view_state: BatchedSignal<ChatViewState>,
    channel_id: &str,
) -> u32 {
    let (unread_count, mention_count, current_server_id) = {
        let cl = chat_lists.read();
        let cv = chat_view_state.read();
        let (unread_count, mention_count) = cl
            .dm_channels
            .iter()
            .find(|dm| dm.id == channel_id)
            .map(|dm| (dm.unread_count, 0u32))
            .or_else(|| {
                cl.channels
                    .iter()
                    .find(|channel| channel.id == channel_id)
                    .map(|channel| (channel.unread_count, channel.mention_count))
            })
            .or_else(|| {
                cv.current_channel
                    .as_ref()
                    .filter(|channel| channel.id == channel_id)
                    .map(|channel| (channel.unread_count, channel.mention_count))
            })
            .unwrap_or((0, 0));
        let current_server_id = cv.current_server.as_ref().map(|server| server.id.clone());
        (unread_count, mention_count, current_server_id)
    };

    if unread_count == 0 && mention_count == 0 {
        return 0;
    }

    chat_view_state.batch(|cv| {
        if let Some(current_channel) = cv.current_channel.as_mut()
            && current_channel.id == channel_id
        {
            current_channel.unread_count = 0;
            current_channel.mention_count = 0;
        }

        if let Some(server_id) = current_server_id.as_deref()
            && let Some(current_server) = cv.current_server.as_mut()
                && current_server.id == server_id
            {
                current_server.unread_count = current_server.unread_count.saturating_sub(unread_count);
                current_server.mention_count =
                    current_server.mention_count.saturating_sub(mention_count);
            }
    });

    chat_lists.batch(|cl| {
        for channel in &mut cl.channels {
            if channel.id == channel_id {
                channel.unread_count = 0;
                channel.mention_count = 0;
                break;
            }
        }

        for dm in &mut cl.dm_channels {
            if dm.id == channel_id {
                dm.unread_count = 0;
                break;
            }
        }

        if let Some(ref server_id) = current_server_id {
            for server in &mut cl.servers {
                if server.id == *server_id {
                    server.unread_count = server.unread_count.saturating_sub(unread_count);
                    server.mention_count = server.mention_count.saturating_sub(mention_count);
                    break;
                }
            }
        }
    });

    unread_count
}

pub(crate) async fn open_message_hit(
    hit: MessageSearchHit,
    current_channel_id: Option<String>,
    current_server_id: Option<String>,
    client_manager: BatchedSignal<ClientManager>,
    chat_view_state: BatchedSignal<ChatViewState>,
    nav: BatchedSignal<crate::state::NavState>,
) -> Option<(Route, String)> {
    let target_message_id = hit.message.id.clone();
    let target_channel_id = hit.channel_id.clone();

    if let Some(ref previous_channel_id) = current_channel_id
        && previous_channel_id != &target_channel_id
    {
        remember_message_list_scroll_position(previous_channel_id);
    }

    if message_hit_already_rendered(
        &chat_view_state,
        current_channel_id.as_deref(),
        &target_channel_id,
        &target_message_id,
    ) {
        highlight_message(&target_message_id);
        return None;
    }

    let target_server_id = hit.server_id.clone().or(current_server_id);
    let active_account_id = nav.read().active_account_id.cloned();
    let active_instance_id = nav.read().active_instance_id.cloned();

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
                    nav.read().active_backend.cloned(),
                )
            })
    } else {
        None
    };
    let (account_id, backend, fallback_backend) = backend_info?;

    let guard = match backend
        .read_with_timeout(std::time::Duration::from_secs(5))
        .await
    {
        Ok(g) => g,
        Err(_) => {
            tracing::warn!("permalink hit: backend read timed out, bailing out");
            chat_view_state.batch(|cv| cv.loading = false);
            return None;
        }
    };
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
        .map(|server| server.backend.clone())
        .or(fallback_backend)
        .unwrap_or(BackendType::from("demo"));
    drop(guard);

    // Batch 5 cascades into one — separate writes each schedule a full
    // Dioxus reactive pass and starve the WASM scheduler (CLAUDE.md hang #1).
    {
        let target_channel_for_batch = target_channel.clone();
        let target_server_for_batch = target_server.clone();
        chat_view_state.batch(move |cv| {
            cv.loading = false;
            cv.set_messages(target_messages);
            cv.members = target_members;
            cv.current_channel = target_channel_for_batch;
            cv.current_server = target_server_for_batch;
        });
    }

    Some(build_message_hit_route(
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
    chat_view_state: &BatchedSignal<ChatViewState>,
    current_channel_id: Option<&str>,
    target_channel_id: &str,
    target_message_id: &str,
) -> bool {
    current_channel_id == Some(target_channel_id)
        && chat_view_state
            .read()
            .messages
            .iter()
            .any(|message| message.id == target_message_id)
}

struct MessageHitRouteCtx {
    client_manager: BatchedSignal<ClientManager>,
    active_instance_id: Option<String>,
    account_id: String,
    target_server_id: Option<String>,
    target_channel_id: String,
    backend_type: BackendType,
    target_message_id: String,
}

fn build_message_hit_route(
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

pub(super) async fn persist_member_list_display_settings(
    grouping: crate::state::MemberListGrouping,
    sort_order: crate::state::MemberListSortOrder,
    show_offline: bool,
) {
    let Some(storage) = crate::STORAGE.get() else {
        return;
    };
    let Ok(mut settings) = storage.get_app_settings().await else {
        return;
    };
    settings.member_list_grouping = grouping;
    settings.member_list_sort_order = sort_order;
    settings.member_list_show_offline = show_offline;
    if let Err(err) = storage.set_app_settings(&settings).await {
        tracing::warn!("Failed to persist member list display settings: {err}");
    }
}

/// Toggle a reaction on a message (add or remove).
///
/// If the reaction already exists and we've reacted, remove our reaction.
/// If it exists but we haven't reacted, add ours. Otherwise create a new reaction.
/// `pub(crate)` so `ReactionContextMenu` can call it without duplicating logic.
pub(crate) fn toggle_reaction_on_message(chat_view_state: BatchedSignal<ChatViewState>, message_id: &str, emoji: &str) {
    chat_view_state.batch(|cv| {
        if let Some(msg) = cv.messages.iter_mut().find(|m| m.id == message_id) {
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
                    reaction.count = reaction.count.saturating_add(1);
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
    });
}

/// Build the "Copy last N messages" clipboard payload from the most
/// recent `limit` messages in `chat_data`. Plain-text format
/// (`- Author: body`) suitable for pasting into Claude Desktop or any
/// other LLM that reads recent context. Used by the AgentPanel
/// "Catch me up" button (see crates/core/src/ui/account/common/agent_panel.rs).
pub(crate) fn catch_up_clipboard_text(chat_view_state: &crate::state::ChatViewState, channel_name: &str, limit: usize) -> String {
    let messages = &chat_view_state.messages;
    let total = messages.len().min(limit);
    let body = messages
        .iter()
        .rev()
        .take(limit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|m| {
            let body = match &m.content {
                MessageContent::Text(t) => t.clone(),
                MessageContent::WithAttachments { text, .. } => text.clone(),
            };
            format!("- {}: {}", m.author.display_name, body)
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Summarize the recent conversation in #{channel_name} (last {total} messages). \
         Pull out decisions, open questions, and action items.\n\n{body}"
    )
}

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
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

fn use_chat_view_effects(signals: &ChatViewSignals, ctx: &ChatViewMarkupCtx) {
    effects::use_member_list_effect(signals);
    effects::use_search_messages_effect(signals, ctx);
    effects::use_pinned_messages_effect(signals);
    effects::use_history_state_effect(signals);
    effects::use_member_list_preferences_effect(signals.ui_layout);
    effects::use_mobile_layout_resize_rerender_effect(signals.mobile_layout_resize_tick);
    effects::use_mobile_side_column_effect(signals, ctx);
    effects::use_command_preload_effect(signals, &ctx.channel_id);
    effects::use_unread_marker_visibility_effect(signals);
    effects::use_auto_dismiss_divider_effect(signals);
    effects::use_composer_focus_effect(signals);
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

            {drag::render_drag_overlay(is_drag_over)}
            {layout::render_chat_layout_shell(ctx.clone())}
            {composer::render_chat_overlays(ctx)}
        }
    }
}
