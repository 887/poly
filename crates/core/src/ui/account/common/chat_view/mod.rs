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
use crate::state::{AccountSessions, AppState, ChatLists, ChatViewState, UiOverlays, VoiceState, use_reactive_effect, use_spawn_once};
use crate::ui::split_shell::RightWingShell;
use dioxus::html::HasFileData;
use dioxus::prelude::*;
use poly_client::{
    Attachment, BackendType, Channel, ChatCommand, DmChannel, Message,
    MessageContent, MessageQuery, MessageReplyPreview, MessageSearchHit,
    MessagingBackend, PresenceStatus, User,
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
#[cfg(target_arch = "wasm32")]
const MOBILE_RIGHT_WING_OPEN_JS: &str = "window.__polySetMobileRightWingOpen?.(true);";
#[cfg(target_arch = "wasm32")]
const MOBILE_RIGHT_WING_CLOSE_JS: &str = "window.__polySetMobileRightWingOpen?.(false);";


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
        {
            drop(backend.mark_channel_read(&cid).await);
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

        if let Some(server_id) = current_server_id.as_deref() {
            if let Some(current_server) = cv.current_server.as_mut()
                && current_server.id == server_id
            {
                current_server.unread_count = current_server.unread_count.saturating_sub(unread_count);
                current_server.mention_count =
                    current_server.mention_count.saturating_sub(mention_count);
            }
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
    mut app_state: BatchedSignal<AppState>,
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
    _app_state: &mut BatchedSignal<AppState>,
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


#[cfg(target_arch = "wasm32")]
fn runtime_mobile_ui_active() -> bool {
    let Some(window) = web_sys::window() else {
        return false;
    };

    let viewport_width = window
        .inner_width()
        .ok()
        .and_then(|value| value.as_f64())
        .unwrap_or_default();
    let viewport_height = window
        .inner_height()
        .ok()
        .and_then(|value| value.as_f64())
        .unwrap_or_default();

    let classes = window
        .document()
        .and_then(|document| document.query_selector(".poly-app").ok().flatten())
        .and_then(|root| root.get_attribute("class"));

    // Early render/hydration fallback: if `.poly-app` isn't available yet,
    // mirror the real app-shell precedence: URL override -> persisted setting.
    let Some(classes) = classes else {
        let (configured_mode, legacy_force_mobile) =
            crate::ui::load_persisted_layout_mode_from_window(&window);
        let fallback_mode = crate::ui::layout_query_override().unwrap_or_else(|| {
            crate::ui::effective_layout_mode(configured_mode, legacy_force_mobile)
        });
        return crate::ui::layout_mode_is_mobile(fallback_mode);
    };

    classes
        .split_whitespace()
        .any(|class| class == "poly-layout-mode-force-mobile")
        || (classes
            .split_whitespace()
            .any(|class| class == "poly-layout-mode-auto-width")
            && viewport_width <= 640.0)
        || (classes
            .split_whitespace()
            .any(|class| class == "poly-layout-mode-auto-portrait")
            && viewport_height > viewport_width)
}

#[cfg(not(target_arch = "wasm32"))]
const fn runtime_mobile_ui_active() -> bool {
    false
}

#[cfg(target_arch = "wasm32")]
fn sync_mobile_side_column_open(open: bool) {
    if !runtime_mobile_ui_active() {
        let _ = document::eval(MOBILE_RIGHT_WING_CLOSE_JS);
        return;
    }

    let _ = document::eval(if open {
        MOBILE_RIGHT_WING_OPEN_JS
    } else {
        MOBILE_RIGHT_WING_CLOSE_JS
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn sync_mobile_side_column_open(_open: bool) {}






#[derive(Clone)]
struct MessageListScrollWorkCtx {
    loading: bool,
    history_state: BatchedSignal<ChatHistoryUiState>,
    scroll_work_in_flight: Arc<AtomicBool>,
    scroll_work_requested: Arc<AtomicBool>,
    messages_for_window: Vec<Message>,
    unread_marker_id: Option<String>,
    unread_count: u32,
    search_query_value: String,
    virtual_window: Signal<MessageVirtualWindowState>,
    app_state: BatchedSignal<AppState>,
    nav: BatchedSignal<crate::state::NavState>,
    client_manager: BatchedSignal<ClientManager>,
    chat_view_state: BatchedSignal<ChatViewState>,
    top_edge_armed: Arc<AtomicBool>,
    bottom_edge_armed: Arc<AtomicBool>,
    /// Signal updated by the scroll loop — true when the user is scrolled far
    /// enough from the live tail that "Jump to Present" should be shown.
    scrolled_from_bottom: Signal<bool>,
}

fn spawn_message_list_scroll_work(mut ctx: MessageListScrollWorkCtx) {
    ctx.scroll_work_requested.store(true, Ordering::Relaxed);
    if ctx.loading || ctx.scroll_work_in_flight.swap(true, Ordering::Acquire) {
        return;
    }

    spawn(async move {
        loop {
            ctx.scroll_work_requested.store(false, Ordering::Relaxed);

            let Some(metrics) = read_message_list_viewport_metrics().await else {
                break;
            };

            if should_virtualize_messages(ctx.messages_for_window.len(), &ctx.search_query_value) {
                set_message_virtual_window(
                    ctx.virtual_window,
                    &ctx.messages_for_window,
                    ctx.unread_marker_id.as_deref(),
                    ctx.unread_count,
                    metrics,
                );
            }

            let history_snapshot = ctx.history_state.read().clone();
            // column-reverse layout: Chrome scrollTop is ≤ 0.
            //   scrollTop = 0          → visual bottom (newest messages)
            //   scrollTop = -maxScroll → visual top (oldest messages)
            // dist_from_bottom = how far from newest end = -scrollTop  (always ≥ 0)
            // dist_from_top    = how far from oldest end = maxScroll - dist_from_bottom
            let dist_from_bottom = (-metrics.scroll_top).max(0.0);
            let max_scroll = (metrics.scroll_height - metrics.client_height).max(0.0);
            let dist_from_top = (max_scroll - dist_from_bottom).max(0.0);
            let top_spacer_boundary = history_snapshot.before_spacer_px.max(0.0);
            let bottom_spacer_boundary = history_snapshot.after_spacer_px.max(0.0);

            // "Scrolled from bottom" = user is not at the newest-message end.
            let is_scrolled_from_bottom =
                history_snapshot.has_more_after || dist_from_bottom > JUMP_TO_PRESENT_THRESHOLD_PX;
            if *ctx.scrolled_from_bottom.peek() != is_scrolled_from_bottom {
                ctx.scrolled_from_bottom.set(is_scrolled_from_bottom);
            }

            // near_top: approaching the older-content spacer (dist_from_top is small).
            let near_top = history_snapshot.has_more_before
                && dist_from_top <= top_spacer_boundary + MESSAGE_HISTORY_EDGE_THRESHOLD_PX;
            // near_bottom: approaching the newer-content spacer (dist_from_bottom is small).
            let near_bottom = history_snapshot.has_more_after
                && dist_from_bottom <= bottom_spacer_boundary + MESSAGE_HISTORY_EDGE_THRESHOLD_PX;

            if !near_top {
                let top_rearm_threshold = if history_snapshot.before_spacer_px > 0.0_f64 {
                    top_spacer_boundary + MESSAGE_HISTORY_EDGE_THRESHOLD_PX
                } else {
                    MESSAGE_HISTORY_EDGE_REARM_PX
                };
                ctx.top_edge_armed
                    .store(dist_from_top > top_rearm_threshold, Ordering::Relaxed);
            }

            if !near_bottom {
                let bottom_rearm_threshold = if history_snapshot.after_spacer_px > 0.0_f64 {
                    bottom_spacer_boundary + MESSAGE_HISTORY_EDGE_THRESHOLD_PX
                } else {
                    MESSAGE_HISTORY_EDGE_REARM_PX
                };
                ctx.bottom_edge_armed
                    .store(dist_from_bottom > bottom_rearm_threshold, Ordering::Relaxed);
            }

            if near_top
                && history_snapshot.has_more_before
                && !history_snapshot.loading_before
                && ctx.top_edge_armed.swap(false, Ordering::Relaxed)
            {
                ctx.history_state.batch(|h| h.loading_before = true);
                load_older_messages(
                    ctx.nav,
                    ctx.client_manager,
                    ctx.chat_view_state,
                    ctx.history_state,
                )
                .await;
            }

            if near_bottom
                && history_snapshot.has_more_after
                && !history_snapshot.loading_after
                && ctx.bottom_edge_armed.swap(false, Ordering::Relaxed)
            {
                ctx.history_state.batch(|h| h.loading_after = true);
                load_newer_messages(
                    ctx.nav,
                    ctx.client_manager,
                    ctx.chat_view_state,
                    ctx.history_state,
                )
                .await;
            }

            if !ctx.scroll_work_requested.load(Ordering::Relaxed) {
                break;
            }
        }

        ctx.scroll_work_in_flight.store(false, Ordering::Release);
    });
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
    let show_side_column = ctx.utility_panel.read().is_some()
        || ctx.member_list_visible
        || mobile_server_right_wing_active(&ctx);
    let mobile_layout = runtime_mobile_ui_active();

    rsx! {
        div { class: "chat-layout-shell",
            {render_chat_main_column(ctx.clone())}
            if mobile_layout && show_side_column {
                {render_chat_side_column(ctx)}
            }
        }
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

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
fn render_chat_header_info(ctx: ChatViewMarkupCtx) -> Element {
    let current_channel = ctx.current_channel.clone();
    let current_server = ctx.current_server.clone();
    let dm_user_avatar = ctx.dm_user_avatar.clone();
    let dm_user_presence = ctx.dm_user_presence;
    let is_dm_channel = ctx.is_dm_channel;
    let is_group_channel = ctx.is_group_channel;
    let group_count = ctx.group_members.len();
    let dm_presence_dot_class = match dm_user_presence {
        PresenceStatus::Online => "presence-dot online",
        PresenceStatus::Idle => "presence-dot idle",
        PresenceStatus::DoNotDisturb => "presence-dot dnd",
        PresenceStatus::Offline | PresenceStatus::Invisible | PresenceStatus::Unknown => "",
    };

    rsx! {
        if let Some(ref ch) = current_channel {
            if is_dm_channel {
                div { class: "dm-chat-header-info",
                    div { class: "dm-chat-avatar-wrap",
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
                        if !dm_presence_dot_class.is_empty() {
                            span { class: "{dm_presence_dot_class}" }
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
    let mobile_right_wing = runtime_mobile_ui_active();

    rsx! {
        div { class: "chat-header-right",
            if mobile_right_wing {
                {render_mobile_chat_header_right_toggle(ctx)}
            } else {
                ChatHeaderActions {
                    app_state: ctx.app_state,
                    utility_panel: ctx.utility_panel,
                    notifications_muted: ctx.notifications_muted,
                    show_search_filters: ctx.show_search_filters,
                    header_actions_menu_open: ctx.header_actions_menu_open,
                    header_actions_overflow: ctx.header_actions_overflow,
                    voice_state: ctx.voice_state,
                    client_manager: ctx.client_manager,
                    mobile_layout_resize_tick: ctx.mobile_layout_resize_tick,
                    is_group_channel: ctx.is_group_channel,
                    is_dm_channel: ctx.is_dm_channel,
                    dm_user: ctx.dm_user.clone(),
                    channel_id: ctx.channel_id.clone(),
                    member_list_visible: ctx.member_list_visible,
                }
            }
        }
    }
}

fn mobile_server_right_wing_active(ctx: &ChatViewMarkupCtx) -> bool {
    runtime_mobile_ui_active() && !ctx.is_dm_channel && !ctx.is_group_channel
}

fn close_chat_side_column_state(
    ui_layout: BatchedSignal<crate::state::UiLayout>,
    mut utility_panel: Signal<Option<ChatUtilityPanel>>,
    mut show_search_filters: Signal<bool>,
    is_group_channel: bool,
    is_dm_channel: bool,
) {
    show_search_filters.set(false);
    if utility_panel.read().is_some() {
        utility_panel.set(None);
        return;
    }

    // Close agent panel first if open
    if false {
        return;
    }

    // Collapse 2-3 writes into ONE batch — see CLAUDE.md § Common WASM-hang causes #1.
    ui_layout.batch(|l| {
        if is_group_channel || is_dm_channel {
            l.dm_right_sidebar_visible = false;
            l.mobile_dm_contact_detail_visible = false;
        } else {
            l.right_sidebar_visible = false;
        }
    });
}

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
fn render_mobile_chat_header_right_toggle(ctx: ChatViewMarkupCtx) -> Element {
    let nav_state = ctx.nav;
    let ui_overlays = ctx.ui_overlays;
    let ui_layout = ctx.ui_layout;
    let mut utility_panel = ctx.utility_panel;
    let mut show_search_filters = ctx.show_search_filters;
    let right_wing_open = ctx.member_list_visible || ctx.utility_panel.read().is_some();
    let current_server = ctx.current_server.clone();
    let current_channel = ctx.current_channel.clone();
    let dm_user = ctx.dm_user.clone();
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    let account_sessions: BatchedSignal<AccountSessions> = use_context();
    let voice_state = ctx.voice_state;
    let client_manager = ctx.client_manager;
    let is_dm_channel = ctx.is_dm_channel;
    let is_group_channel = ctx.is_group_channel;
    let active_dm_call = voice_state
        .read()
        .voice_connection
        .clone()
        .filter(|connection| connection.dm_id.as_deref() == ctx.channel_id.as_deref());
    // For DMs, don't use the avatar — always show "@" on mobile
    let toggle_icon_url = if is_dm_channel {
        None
    } else {
        current_server
            .as_ref()
            .and_then(|server| server.icon_url.clone())
    };

    let toggle_label = if is_dm_channel {
        current_channel
            .as_ref().map_or_else(|| t("chat-toggle-contact"), |channel| channel.name.clone())
    } else if is_group_channel {
        current_channel
            .as_ref().map_or_else(|| t("chat-toggle-members"), |channel| channel.name.clone())
    } else {
        current_server
            .as_ref().map_or_else(|| t("chat-toggle-members"), |server| server.name.clone())
    };
    let toggle_fallback = if is_dm_channel {
        // On mobile, DMs show "@" symbol instead of first character
        "@".to_string()
    } else if is_group_channel {
        "👥".to_string()
    } else {
        current_server
            .as_ref().map_or_else(|| "#".to_string(), |server| server.name.chars().next().unwrap_or('#').to_string())
    };

    rsx! {
        div { class: "chat-header-actions chat-header-actions-mobile",
            if is_dm_channel && active_dm_call.is_none() {
                if let Some(dm_target) = dm_user.clone() {
                    button {
                        class: "header-btn chat-header-btn-call",
                        title: t("user-profile-call"),
                        onclick: move |_| {
                            navigate_to_pending_direct_call_from_active_account(
                                DirectCallRequest {
                                    target_user: dm_target.clone(),
                                    start_video: false,
                                    allow_add_to_active_temporary: false,
                                },
                                nav_state,
                                ui_overlays,
                                chat_lists,
                                account_sessions,
                                client_manager,
                                navigator(),
                            );
                        },
                        "📞"
                    }
                }
                if let Some(dm_target) = dm_user {
                    button {
                        class: "header-btn chat-header-btn-video",
                        title: t("user-profile-video"),
                        onclick: move |_| {
                            navigate_to_pending_direct_call_from_active_account(
                                DirectCallRequest {
                                    target_user: dm_target.clone(),
                                    start_video: true,
                                    allow_add_to_active_temporary: false,
                                },
                                nav_state,
                                ui_overlays,
                                chat_lists,
                                account_sessions,
                                client_manager,
                                navigator(),
                            );
                        },
                        "🎥"
                    }
                }
            }
            button {
                class: if right_wing_open { "header-btn soft-active poly-mobile-right-wing-toggle mobile-server-icon-toggle" } else { "header-btn poly-mobile-right-wing-toggle mobile-server-icon-toggle" },
                title: if is_dm_channel { t("chat-toggle-contact") } else { t("chat-toggle-members") },
                aria_label: "{toggle_label}",
                onclick: move |_| {
                    let currently_open = if is_dm_channel || is_group_channel {
                        ui_layout.read().dm_right_sidebar_visible
                    } else {
                        ui_layout.read().right_sidebar_visible
                    };
                    let is_opening = !currently_open;

                    show_search_filters.set(false);
                    utility_panel.set(None);

                    if is_opening {
                        show_search_filters.set(false);
                        ui_layout.batch(|l| {
                            if is_dm_channel || is_group_channel {
                                l.dm_right_sidebar_visible = true;
                                l.mobile_dm_contact_detail_visible = false;
                            } else {
                                l.right_sidebar_visible = true;
                            }
                        });
                    } else {
                        close_chat_side_column_state(
                            ui_layout,
                            utility_panel,
                            show_search_filters,
                            is_group_channel,
                            is_dm_channel,
                        );
                    }
                    #[cfg(target_arch = "wasm32")]
                    {
                        let _ = document::eval(
                            if is_opening {
                                MOBILE_RIGHT_WING_OPEN_JS
                            } else {
                                MOBILE_RIGHT_WING_CLOSE_JS
                            },
                        );
                    }
                },
                if let Some(ref icon_url) = toggle_icon_url {
                    img {
                        class: "mobile-server-icon-image",
                        src: "{icon_url}",
                        alt: "{toggle_label}",
                    }
                } else {
                    span { class: "mobile-server-icon-fallback", "{toggle_fallback}" }
                }
            }
        }
    }
}

fn render_chat_body_shell(ctx: ChatViewMarkupCtx) -> Element {
    let show_side_column = ctx.utility_panel.read().is_some()
        || ctx.member_list_visible;
    let mobile_layout = runtime_mobile_ui_active();
    // 5.2 — Thread panel is visible when a thread_id is stored in nav state
    // and we are not in mobile layout (mobile uses the full-page ThreadView route).
    let thread_panel_open = ctx.ui_overlays.read().thread_panel_open.is_some();

    rsx! {
        div { class: "chat-body-shell",
            {render_chat_content_column(ctx.clone())}
            if !mobile_layout && thread_panel_open {
                ThreadPanel {}
            }
            if !mobile_layout && show_side_column {
                {render_chat_side_column(ctx)}
            }
        }
    }
}

fn render_chat_content_column(ctx: ChatViewMarkupCtx) -> Element {
    rsx! {
        div { class: "chat-content-column",
            // 5.4 — Active threads bar above the message list for text channels
            // that have active threads. Renders nothing if no threads exist.
            ActiveThreadsBar {}
            {render_message_list(ctx.clone())}
            {render_jump_to_present(ctx.clone())}
            TypingIndicator {}
            {render_message_input_area(ctx)}
        }
    }
}

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
fn render_message_list(ctx: ChatViewMarkupCtx) -> Element {
    let loading = ctx.loading;
    let app_state = ctx.app_state;
    let nav = ctx.nav;
    let client_manager = ctx.client_manager;
    let chat_view_state_for_scroll = ctx.chat_view_state;
    let history_state = ctx.history_state;
    let scroll_work_in_flight = use_hook(|| Arc::new(AtomicBool::new(false)));
    let scroll_work_requested = use_hook(|| Arc::new(AtomicBool::new(false)));
    let scroll_frame_pending = use_hook(|| Arc::new(AtomicBool::new(false)));
    let top_edge_armed = use_hook(|| Arc::new(AtomicBool::new(true)));
    let bottom_edge_armed = use_hook(|| Arc::new(AtomicBool::new(true)));
    let virtualize_messages =
        should_virtualize_messages(ctx.messages.len(), &ctx.search_query_value);
    // Lock the message list scroll during a history load so the user cannot drag
    // the scrollbar mid-load and so our JS position-restore sees a stable DOM.
    let is_loading_history = {
        let hs = ctx.history_state.read();
        hs.loading_before || hs.loading_after
    };
    let unread_count = ctx.history_state.read().unread_count;
    let unread_marker_id = ctx.unread_marker_id.clone();
    let messages_for_window = ctx.messages.clone();
    let scroll_messages_for_window = ctx.messages.clone();
    let scroll_unread_marker_id = ctx.unread_marker_id.clone();
    let scroll_search_query_value = ctx.search_query_value.clone();
    let mut virtual_window = ctx.virtual_window;
    let scrolled_from_bottom = ctx.scrolled_from_bottom;

    use_effect(move || { // poly-lint: allow stale-effect-capture — virtualize_messages is bool (Copy); virtual_window/scrolled_from_bottom are Signals
        if !virtualize_messages {
            if virtual_window.read().enabled {
                virtual_window.set(MessageVirtualWindowState::default());
            }
            return;
        }

        let messages_for_window = messages_for_window.clone();
        let unread_marker_id = unread_marker_id.clone();
        spawn(async move {
            if let Some(metrics) = read_message_list_viewport_metrics().await {
                set_message_virtual_window(
                    virtual_window,
                    &messages_for_window,
                    unread_marker_id.as_deref(),
                    unread_count,
                    metrics,
                );
            }
        });
    });

    rsx! {
        div {
            class: if is_loading_history { "message-list loading-history" } else { "message-list" },
            id: "message-list-scroll",
            onscroll: move |_| {
                if scroll_frame_pending.swap(true, Ordering::AcqRel) {
                    return;
                }

                let scroll_frame_pending = Arc::clone(&scroll_frame_pending);
                let scroll_work_in_flight = Arc::clone(&scroll_work_in_flight);
                let scroll_work_requested = Arc::clone(&scroll_work_requested);
                let scroll_top_edge_armed = Arc::clone(&top_edge_armed);
                let scroll_bottom_edge_armed = Arc::clone(&bottom_edge_armed);
                let scroll_messages_for_window = scroll_messages_for_window.clone();
                let scroll_unread_marker_id = scroll_unread_marker_id.clone();
                let scroll_search_query_value = scroll_search_query_value.clone();

                spawn(async move {
                    if !wait_for_next_animation_frame().await {
                        scroll_frame_pending.store(false, Ordering::Release);
                        return;
                    }

                    scroll_frame_pending.store(false, Ordering::Release);
                    spawn_message_list_scroll_work(MessageListScrollWorkCtx {
                        loading,
                        history_state,
                        scroll_work_in_flight,
                        scroll_work_requested,
                        messages_for_window: scroll_messages_for_window,
                        unread_marker_id: scroll_unread_marker_id,
                        unread_count,
                        search_query_value: scroll_search_query_value,
                        virtual_window,
                        app_state,
                        nav,
                        client_manager,
                        chat_view_state: chat_view_state_for_scroll,
                        top_edge_armed: scroll_top_edge_armed,
                        bottom_edge_armed: scroll_bottom_edge_armed,
                        scrolled_from_bottom,
                    });
                });
            },
            // column-reverse: content must be FIRST child so it sits at scrollTop=0 (visual bottom).
            // Overlays and banner come AFTER so they appear at the visual top when scrolled up.
            div { class: if is_loading_history { "message-list-content message-list-content-swapping" } else { "message-list-content" },
                {render_message_list_content(ctx.clone())}
            }
            {render_unread_banner(ctx.clone())}
            {render_message_list_loading_overlays(ctx.clone())}
        }
    }
}

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
fn render_message_list_loading_overlays(ctx: ChatViewMarkupCtx) -> Element {
    let history_snapshot = ctx.history_state.read().clone();

    rsx! {
        if history_snapshot.loading_before {
            div { class: "message-history-loader-overlay message-history-loader-overlay-top",
                "{t(\"chat-loading-earlier\")}"
            }
        }
        if history_snapshot.loading_after {
            div { class: "message-history-loader-overlay message-history-loader-overlay-bottom",
                "{t(\"chat-loading-earlier\")}"
            }
        }
    }
}

async fn load_older_messages(
    nav: BatchedSignal<crate::state::NavState>,
    client_manager: BatchedSignal<ClientManager>,
    chat_view_state: BatchedSignal<ChatViewState>,
    history_state: BatchedSignal<ChatHistoryUiState>,
) {
    let Some(active_channel_id) = nav.read().selected_channel.cloned() else {
        history_state.batch(|h| h.loading_before = false);
        return;
    };
    let Some(before_message_id) = chat_view_state
        .read()
        .messages
        .first()
        .map(|message| message.id.clone())
    else {
        history_state.batch(|h| {
            h.loading_before = false;
            h.has_more_before = false;
        });
        return;
    };
    let backend = if let Some(server_id) = nav.peek().selected_server.cloned() {
        client_manager
            .peek()
            .get_backend_for_server(&server_id)
            .map(|(_, handle)| handle)
    } else if let Some(account_id) = nav.peek().active_account_id.cloned() {
        client_manager.peek().get_backend(&account_id)
    } else {
        None
    };
    let Some(backend) = backend else {
        history_state.batch(|h| h.loading_before = false);
        return;
    };
    let anchor_snapshot = read_message_list_anchor().await;

    let older_messages = {
        let guard = match backend.read_with_timeout(std::time::Duration::from_secs(5)).await {
            Ok(g) => g,
            Err(_) => {
                tracing::warn!("chat_view: backend read timed out in load_older_messages");
                history_state.batch(|h| h.loading_before = false);
                return;
            }
        };
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
        // HIGH-severity cascade collapse: 3 fields in one batch guard.
        history_state.batch(|history| {
            history.loading_before = false;
            history.has_more_before = false;
            history.before_spacer_px = 0.0_f64;
        });
        return;
    }

    let has_more_before =
        u32::try_from(older_messages.len()).unwrap_or(0) >= OLDER_MESSAGES_PAGE_SIZE;

    {
        let (merged_messages, dropped_newer_messages) = chat_view_state.batch(|cv| {
            let existing_messages = std::mem::take(&mut cv.messages);
            let mut merged_messages = older_messages.clone();
            merged_messages.extend(existing_messages);
            let dropped_newer_messages = trim_message_window_from_bottom(&mut merged_messages);
            cv.set_messages(merged_messages.clone());
            (merged_messages, dropped_newer_messages)
        });

        // HIGH-severity cascade collapse: 3 field writes + recompute in one batch guard.
        history_state.batch(|history| {
            history.has_more_before = has_more_before;
            history.has_more_after = dropped_newer_messages || history.has_more_after;
            recompute_history_spacers(history, &merged_messages);
        });
    }
    // column-reverse layout: prepending older messages at the visual top does not disturb
    // scrollTop (browser measures from the visual bottom). No scroll correction needed.
    // Anchor restoration is still used if available for precise pinning.
    if let Some((anchor_element_id, anchor_offset_px)) = anchor_snapshot {
        request_preserve_message_anchor(&anchor_element_id, anchor_offset_px);
    }
    history_state.batch(|h| h.loading_before = false);
}

async fn load_newer_messages(
    nav: BatchedSignal<crate::state::NavState>,
    client_manager: BatchedSignal<ClientManager>,
    chat_view_state: BatchedSignal<ChatViewState>,
    history_state: BatchedSignal<ChatHistoryUiState>,
) {
    let Some(active_channel_id) = nav.read().selected_channel.cloned() else {
        history_state.batch(|h| h.loading_after = false);
        return;
    };
    let Some(after_message_id) = chat_view_state
        .read()
        .messages
        .last()
        .map(|message| message.id.clone())
    else {
        history_state.batch(|h| {
            h.loading_after = false;
            h.has_more_after = false;
        });
        return;
    };
    let backend = if let Some(server_id) = nav.peek().selected_server.cloned() {
        client_manager
            .peek()
            .get_backend_for_server(&server_id)
            .map(|(_, handle)| handle)
    } else if let Some(account_id) = nav.peek().active_account_id.cloned() {
        client_manager.peek().get_backend(&account_id)
    } else {
        None
    };
    let Some(backend) = backend else {
        history_state.batch(|h| h.loading_after = false);
        return;
    };
    let anchor_snapshot = read_message_list_anchor().await;

    let (newer_messages, reached_latest_message) = {
        let guard = match backend
            .read_with_timeout(std::time::Duration::from_secs(30))
            .await
        {
            Ok(g) => g,
            Err(_) => {
                tracing::warn!("load_newer_messages: chain-load backend read timed out");
                history_state.batch(|h| h.loading_after = false);
                return;
            }
        };
        let mut collected_messages = Vec::new();
        let mut next_after_message_id = after_message_id;
        let mut reached_latest_message = false;

        for _ in 0..MAX_CHAINED_NEWER_HISTORY_PAGES {
            let batch = guard
                .get_messages(
                    &active_channel_id,
                    MessageQuery {
                        after: Some(next_after_message_id.clone()),
                        limit: Some(OLDER_MESSAGES_PAGE_SIZE),
                        ..Default::default()
                    },
                )
                .await
                .unwrap_or_default();

            if batch.is_empty() {
                reached_latest_message = true;
                break;
            }

            let batch_len = batch.len();
            let last_batch_message_id = batch.last().map(|message| message.id.clone());
            collected_messages.extend(batch);

            if u32::try_from(batch_len).unwrap_or(0) < OLDER_MESSAGES_PAGE_SIZE {
                reached_latest_message = true;
                break;
            }

            let Some(last_batch_message_id) = last_batch_message_id else {
                reached_latest_message = true;
                break;
            };
            next_after_message_id = last_batch_message_id;
        }

        (collected_messages, reached_latest_message)
    };
    if newer_messages.is_empty() {
        history_state.batch(|history| {
            history.loading_after = false;
            history.has_more_after = !reached_latest_message;
            history.after_spacer_px = 0.0_f64;
        });
        return;
    }

    let has_more_after = !reached_latest_message;

    {
        let (merged_messages, dropped_older_messages) = chat_view_state.batch(|cv| {
            let mut merged_messages = std::mem::take(&mut cv.messages);
            merged_messages.extend(newer_messages.clone());
            let dropped_older_messages = trim_message_window_from_top(&mut merged_messages);
            cv.set_messages(merged_messages.clone());
            (merged_messages, dropped_older_messages)
        });

        history_state.batch(|history| {
            history.has_more_before = dropped_older_messages || history.has_more_before;
            history.has_more_after = has_more_after;
            recompute_history_spacers(history, &merged_messages);
        });
    }
    // column-reverse: trimming older messages from the top does not disturb scrollTop.
    // Anchor restoration is still used for precise pinning when available.
    if let Some((anchor_element_id, anchor_offset_px)) = anchor_snapshot {
        request_preserve_message_anchor(&anchor_element_id, anchor_offset_px);
    }
    history_state.batch(|h| h.loading_after = false);
}

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
fn render_message_list_content(ctx: ChatViewMarkupCtx) -> Element {
    if ctx.loading {
        return rsx! {
            div { class: "message-loading", "{t(\"chat-loading\")}" }
        };
    }

    // F-DC-1: permission-denied empty state (e.g. Discord VIEW_CHANNEL missing)
    let channel_load_error = ctx.chat_view_state.read().channel_load_error.clone();
    if let Some(err_msg) = channel_load_error {
        return rsx! {
            div { class: "message-empty permission-denied-empty",
                div { class: "permission-denied-icon", "🔒" }
                h3 { class: "permission-denied-title", "No access to this channel" }
                p { class: "permission-denied-message", "{err_msg}" }
            }
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

    let virtual_window = ctx.virtual_window.read().clone();
    let render_start = if virtual_window.enabled {
        virtual_window.start_idx.min(ctx.messages.len())
    } else {
        0
    };
    let render_end = if virtual_window.enabled {
        virtual_window.end_idx.min(ctx.messages.len())
    } else {
        ctx.messages.len()
    };
    let history_snapshot = ctx.history_state.read().clone();
    let top_history_spacer_px = history_snapshot.before_spacer_px;
    let bottom_history_spacer_px = history_snapshot.after_spacer_px;
    let top_virtual_spacer_px = if virtual_window.enabled {
        virtual_window.top_spacer_px
    } else {
        0.0_f64
    };
    let bottom_virtual_spacer_px = if virtual_window.enabled {
        virtual_window.bottom_spacer_px
    } else {
        0.0_f64
    };
    let total_top_spacer_px = top_history_spacer_px + top_virtual_spacer_px;
    let total_bottom_spacer_px = bottom_history_spacer_px + bottom_virtual_spacer_px;

    rsx! {
        // column-reverse: "after" spacer (newer unloaded content) must be FIRST in DOM
        // so it appears at the visual bottom (scrollTop=0 end).
        if total_bottom_spacer_px > 0.0 {
            div {
                class: "message-history-spacer message-history-spacer-bottom",
                style: "height: {total_bottom_spacer_px}px;",
            }
        }
        for slot_idx in 0..MAX_LOADED_MESSAGES {
            {
                let actual_idx = render_start + slot_idx;
                if actual_idx < render_end {
                    if let Some(msg) = ctx.messages.get(actual_idx).cloned() {
                        let prev_msg = if actual_idx > 0 {
                            ctx.messages.get(actual_idx - 1).cloned()
                        } else {
                            None
                        };
                        rsx! {
                            div { key: "message-slot-{slot_idx}", class: "message-window-slot",
                                {render_message_row(ctx.clone(), msg, prev_msg)}
                            }
                        }
                    } else {
                        rsx! {
                            div {
                                key: "message-slot-{slot_idx}",
                                class: "message-window-slot message-window-slot-empty",
                            }
                        }
                    }
                } else {
                    rsx! {
                        div {
                            key: "message-slot-{slot_idx}",
                            class: "message-window-slot message-window-slot-empty",
                        }
                    }
                }
            }
        }
        // column-reverse: "before" spacer (older unloaded content) must be LAST in DOM
        // so it appears at the visual top (high scrollTop end).
        if total_top_spacer_px > 0.0 {
            div {
                class: "message-history-spacer message-history-spacer-top",
                style: "height: {total_top_spacer_px}px;",
            }
        }
    }
}

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
fn render_jump_to_present(ctx: ChatViewMarkupCtx) -> Element {
    let is_scrolled = *ctx.scrolled_from_bottom.read();
    let has_more_after = ctx.history_state.read().has_more_after;
    if !is_scrolled && !has_more_after {
        return rsx! {};
    }

    let new_count = *ctx.new_messages_while_scrolled_up.read();
    let mut new_messages_while_scrolled_up = ctx.new_messages_while_scrolled_up;
    let nav = ctx.nav;
    let client_manager = ctx.client_manager;
    let chat_view_state = ctx.chat_view_state;
    let history_state = ctx.history_state;

    rsx! {
        div { class: "chat-jump-to-present-wrap",
            button {
                class: "chat-jump-to-present",
                onclick: move |_| {
                    new_messages_while_scrolled_up.set(0);
                    if history_state.read().has_more_after && !history_state.read().loading_after {
                        history_state.batch(|h| h.loading_after = true);
                        spawn(async move {
                            load_newer_messages(nav, client_manager, chat_view_state, history_state).await;
                            // RAF-deferred so it runs after Dioxus applies the new messages to the DOM.
                            request_scroll_to_bottom_deferred();
                        });
                    } else {
                        request_scroll_to_bottom();
                    }
                },
                if new_count > 0 {
                    span { class: "chat-jump-to-present-badge", "{new_count}" }
                }
                span { class: "chat-jump-to-present-arrow", "↓" }
                span { class: "chat-jump-to-present-label",
                    "{t(\"chat-jump-to-present\")}"
                    if has_more_after {
                        span { class: "chat-jump-to-present-subtitle", "{t(\"chat-viewing-older-messages\")}" }
                    }
                }
            }
        }
    }
}

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
fn render_unread_banner(ctx: ChatViewMarkupCtx) -> Element {
    // Only show the banner if there are unread messages AND the unread marker is not visible on screen
    if !ctx.unread_banner_visible || *ctx.unread_marker_on_screen.read() {
        return rsx! {};
    }

    let chat_lists = ctx.chat_lists;
    let chat_view_state = ctx.chat_view_state;
    let history_state = ctx.history_state;
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
                        let _ = mark_channel_as_read(chat_lists, chat_view_state, &active_channel_id);
                        // Clear unread count (hides the banner) but preserve
                        // unread_divider_visible so the red line stays (Discord behaviour).
                        history_state.batch(|h| h.unread_count = 0);
                    }
                },
                "{t(\"notifications-mark-read\")}"
            }
        }
    }
}

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
fn render_message_row(ctx: ChatViewMarkupCtx, msg: Message, prev_msg: Option<Message>) -> Element {
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
                span { class: "message-author", style: "color: {color};", "{msg.author.display_name}" }
                span { class: "message-timestamp", "{time_str}" }
            }
            {render_message_content_stack(ctx, msg, is_editing)}
        }
    }
}

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
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

fn render_message_input_area(ctx: ChatViewMarkupCtx) -> Element {
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
        {
            if let Some(mb) = backend.as_messaging() {
                drop(mb.send_typing(&channel_id).await);
            }
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
                        {
                            if let Some(mb) = backend.as_messaging() {
                                drop(mb.send_typing(&channel_id).await);
                            }
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
        r#"
            let input = document.getElementById('poly-file-input');
            if (input) { input.click(); }
        "#,
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

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
fn render_input_emoji_picker(ctx: ChatViewMarkupCtx) -> Element {
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
            on_close: move |_| show_input_emoji.set(false),
            markdown_enabled,
            custom_emojis: custom_emojis.read().clone(),
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
    let mobile_tools = runtime_mobile_ui_active();

    rsx! {
        RightWingShell {
            panel_class: String::new(),
            content: rsx! {
                if mobile_tools {
                    {render_chat_tools_panel(ctx.clone())}
                }
                if let Some(panel) = panel {
                    {render_chat_utility_rail(ctx, panel, current_channel_name)}
                } else if ctx.is_dm_channel {
                    DmContactListPanel { channel_id: ctx.channel_id.clone().unwrap_or_default() }
                } else if ctx.is_group_channel {
                    DmUserSidebar {}
                } else {
                    UserSidebar {}
                }
            },
        }
    }
}

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
fn render_chat_tools_panel(ctx: ChatViewMarkupCtx) -> Element {
    let app_state = ctx.app_state;
    let ui_layout = ctx.ui_layout;
    let mut utility_panel = ctx.utility_panel;
    let notifications_muted = ctx.notifications_muted;
    let mut show_search_filters = ctx.show_search_filters;
    let member_sidebar_active = ctx.member_list_visible;
    let is_group_channel = ctx.is_group_channel;
    let is_dm_channel = ctx.is_dm_channel;
    let threads_active = *utility_panel.read() == Some(ChatUtilityPanel::Threads);
    let pinned_active = *utility_panel.read() == Some(ChatUtilityPanel::Pinned);
    let settings_active = *utility_panel.read() == Some(ChatUtilityPanel::Settings);
    rsx! {
        div { class: "chat-tools-panel",
            div { class: "chat-tools-topbar",
                button {
                    class: "header-btn chat-tools-close poly-mobile-right-wing-close-state",
                    title: t("action-close"),
                    onclick: move |_| {
                        close_chat_side_column_state(
                            ui_layout,
                            utility_panel,
                            show_search_filters,
                            is_group_channel,
                            is_dm_channel,
                        );
                        #[cfg(target_arch = "wasm32")]
                        {
                            let _ = document::eval(MOBILE_RIGHT_WING_CLOSE_JS);
                        }
                    },
                    "✕"
                }
                div { class: "chat-tools-actions",
                    button {
                        class: if settings_active { "header-btn active chat-header-btn-settings" } else { "header-btn chat-header-btn-settings" },
                        title: t("chat-settings"),
                        onclick: move |_| {
                            show_search_filters.set(false);
                            let next = if *utility_panel.read() == Some(ChatUtilityPanel::Settings) {
                                None
                            } else {
                                Some(ChatUtilityPanel::Settings)
                            };
                            utility_panel.set(next);
                            ui_layout.batch(|l| l.right_sidebar_visible = false);
                        },
                        span { class: "chat-settings-btn-icon",
                            span { class: "chat-settings-btn-icon-cog", "⚙️" }
                            if *notifications_muted.read() {
                                span { class: "chat-settings-btn-muted-dot" }
                            }
                        }
                    }
                    button {
                        class: if threads_active { "header-btn active chat-header-btn-threads" } else { "header-btn chat-header-btn-threads" },
                        title: t("threads"),
                        onclick: move |_| {
                            show_search_filters.set(false);
                            let next = if *utility_panel.read() == Some(ChatUtilityPanel::Threads) {
                                None
                            } else {
                                Some(ChatUtilityPanel::Threads)
                            };
                            utility_panel.set(next);
                            ui_layout.batch(|l| l.right_sidebar_visible = false);
                        },
                        "🧵"
                    }
                    button {
                        class: if pinned_active { "header-btn active chat-header-btn-pinned" } else { "header-btn chat-header-btn-pinned" },
                        title: t("pinned-messages"),
                        onclick: move |_| {
                            show_search_filters.set(false);
                            let next = if *utility_panel.read() == Some(ChatUtilityPanel::Pinned) {
                                None
                            } else {
                                Some(ChatUtilityPanel::Pinned)
                            };
                            utility_panel.set(next);
                            ui_layout.batch(|l| l.right_sidebar_visible = false);
                        },
                        "📌"
                    }
                    // B.5 drafts toggle dropped — pending drafts now live
                    // inside the agent panel (per-chat).
                    {
                        render_search_tab_button(
                            utility_panel,
                            show_search_filters,
                            true,
                            is_group_channel,
                            is_dm_channel,
                            ui_layout,
                        )
                    }
                    {render_agent_toggle_button(app_state, utility_panel, show_search_filters, is_dm_channel, is_group_channel)}
                    button {
                        class: if member_sidebar_active && utility_panel.read().is_none() { "header-btn soft-active chat-members-toggle-btn chat-header-btn-members" } else { "header-btn chat-members-toggle-btn chat-header-btn-members" },
                        title: if is_dm_channel { t("chat-toggle-contact") } else { t("chat-toggle-members") },
                        onclick: move |_| {
                            utility_panel.set(None);
                            show_search_filters.set(false);
                            // Opening members: close agent panel — collapse 2 writes to 1 batch.
                            ui_layout.batch(|l| {
                                if is_dm_channel || is_group_channel {
                                    l.dm_right_sidebar_visible = true;
                                    l.mobile_dm_contact_detail_visible = false;
                                } else {
                                    let current = l.right_sidebar_visible;
                                    l.right_sidebar_visible = !current;
                                }
                            });
                        },
                        "👥"
                    }
                }
            }
        }
    }
}

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
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
    let nav_state_for_search = ctx.nav;
    let nav_state_for_pinned = ctx.nav;
    let client_manager = ctx.client_manager;
    let chat_view_state = ctx.chat_view_state;
    let app_state = ctx.app_state;
    let notifications_muted = ctx.notifications_muted;
    let pinned_filter_open = ctx.pinned_filter_open;
    let pinned_filter_query = ctx.pinned_filter_query;
    let threads_filter_open = ctx.threads_filter_open;
    let threads_filter_query = ctx.threads_filter_query;
    let search_ui = render_chat_header_search(ctx.clone());

    rsx! {
        ChatUtilityRail {
            panel,
            search_ui,
            search_query,
            search_hits,
            search_terms,
            pinned_messages,
            current_channel_name,
            notifications_muted,
            pinned_filter_open,
            pinned_filter_query,
            threads_filter_open,
            threads_filter_query,
            on_open_search_hit: move |hit: MessageSearchHit| {
                let current_channel_id = search_hit_channel_id.clone();
                let current_server_id = search_hit_server
                    .as_ref()
                    .map(|server| server.id.clone());
                let nav = nav_for_search;
                let nav_state = nav_state_for_search;
                spawn(async move {
                    if let Some((route, message_id)) = open_message_hit(
                            hit,
                            current_channel_id,
                            current_server_id,
                            client_manager,
                            chat_view_state,
                            app_state,
                            nav_state,
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
                let nav_state = nav_state_for_pinned;
                spawn(async move {
                    if let Some((route, message_id)) = open_message_hit(
                            hit,
                            Some(active_channel_id),
                            current_server_id,
                            client_manager,
                            chat_view_state,
                            app_state,
                            nav_state,
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

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
fn render_chat_overlays(ctx: ChatViewMarkupCtx) -> Element {
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
                on_close: move |_| reaction_picker_msg.set(None),
            }
        }
        if msg_context_menu.read().is_some() {
            MsgContextMenuOverlay { msg_context_menu }
        }
    }
}

/// Build the "Copy last N messages" clipboard payload from the most
/// recent `limit` messages in `chat_data`. Plain-text format
/// (`- Author: body`) suitable for pasting into Claude Desktop or any
/// other LLM that reads recent context. Used by the AgentPanel
/// "Catch me up" button (see crates/core/src/ui/account/common/agent_panel.rs).
pub(crate) fn catch_up_clipboard_text(chat_view_state: &crate::state::ChatViewState, channel_name: &str, limit: usize) -> String {
    let recent: Vec<&Message> = chat_view_state.messages.iter().rev().take(limit).collect();
    let total = recent.len();
    let body = recent
        .iter()
        .rev()
        .map(|m| {
            let body = match &m.content {
                poly_client::MessageContent::Text(s) => s.clone(),
                poly_client::MessageContent::WithAttachments { text, .. } => text.clone(),
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
/// allow_default so rendered `<a>` anchors inside `.message-markdown` get the
/// OS "Open link / Copy link / Save link as" native context menu.
#[rustfmt::skip]
#[ui_action(None)]
#[context_menu(allow_default)]
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
fn AttachmentsView(
    attachments: Vec<poly_client::Attachment>,
    message_id: String,
    msg_context_menu: Signal<Option<MsgContextMenu>>,
    message_text: String,
    is_own: bool,
) -> Element {
    let nav_state: BatchedSignal<crate::state::NavState> = use_context();
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
fn ReactionsView(reactions: Vec<poly_client::Reaction>, message_id: String) -> Element {
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

/// Typing indicator shown above the message input when users are typing.
#[rustfmt::skip]
#[ui_action(None)]
#[context_menu(none)]
#[component]
fn TypingIndicator() -> Element {
    let chat_view_state: BatchedSignal<ChatViewState> = use_context();
    let typing = chat_view_state.read().typing_users.clone();

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
            request_scroll_to_bottom();
        }
        Err(e) => {
            tracing::error!("Failed to send message: {e}");
        }
    }
}

/// Apply an inline edit to a message in the chat data.
///
/// Sets `edited = true` on the message and replaces its content with the new text.
fn apply_edit(chat_view_state: BatchedSignal<ChatViewState>, message_id: &str, new_text: String) {
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
fn MessageInlineEdit(
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
fn MsgContextMenuOverlay(
    msg_context_menu: Signal<Option<MsgContextMenu>>,
) -> Element {
    let Some(menu) = msg_context_menu.read().clone() else {
        return rsx! {};
    };

    let nav_state: BatchedSignal<crate::state::NavState> = use_context();
    let user_prefs: BatchedSignal<crate::state::UserPrefs> = use_context();
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
#[ui_action(inherit)]
#[context_menu(inherit)]
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
#[ui_action(inherit)]
#[context_menu(inherit)]
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

#[rustfmt::skip]
#[ui_action(None)]
#[context_menu(none)]
#[component]
fn DmContactListPanel(channel_id: String) -> Element {
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    let account_sessions: BatchedSignal<crate::state::AccountSessions> = use_context();
    let nav_state: BatchedSignal<crate::state::NavState> = use_context();

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
