//! Message-list scroll machinery, history pagination, and message-list rendering.
//!
//! Single responsibility: everything that deals with the scroll position and
//! paging older/newer messages in and out of the working set.
//!
//! Key types:
//! - [`MessageListScrollWorkCtx`] — immutable snapshot passed to the scroll loop.
//! - [`spawn_message_list_scroll_work`] — debounced scroll-event driver.
//! - [`load_older_messages`] / [`load_newer_messages`] — history page loaders.
//! - `render_message_list` / `render_message_list_content` — DOM side.

use std::sync::{Arc, atomic::{AtomicBool, Ordering}};

use dioxus::prelude::*;

use crate::client_manager::{BackendHandleExt, ClientManager};
use crate::i18n::t;
use crate::state::BatchedSignal;
use crate::state::{ChatLists, ChatViewState};
use super::super::super::super::routes::Route;
use super::super::chat_history::{
    ChatHistoryUiState, MAX_LOADED_MESSAGES, OLDER_MESSAGES_PAGE_SIZE,
    read_message_list_anchor,
    remember_message_list_scroll_position,
    request_preserve_message_anchor,
    request_scroll_to_bottom,
    request_scroll_to_bottom_deferred,
};
use super::markup_ctx::ChatViewMarkupCtx;
use super::mark_channel_as_read;
use super::message_row::render_message_row;
use super::virtualization::{
    MessageVirtualWindowState, read_message_list_viewport_metrics,
    recompute_history_spacers, set_message_virtual_window,
    should_virtualize_messages, trim_message_window_from_bottom,
    trim_message_window_from_top, wait_for_next_animation_frame,
};
use poly_client::{Message, MessageQuery};

/// Immutable snapshot of all signals needed by the scroll event loop.
///
/// Clone-cheap because signals are handles (Arc-backed internally).
#[derive(Clone)]
pub(super) struct MessageListScrollWorkCtx {
    pub(super) loading: bool,
    pub(super) history_state: BatchedSignal<ChatHistoryUiState>,
    pub(super) scroll_work_in_flight: Arc<AtomicBool>,
    pub(super) scroll_work_requested: Arc<AtomicBool>,
    pub(super) messages_for_window: Vec<Message>,
    pub(super) unread_marker_id: Option<String>,
    pub(super) unread_count: u32,
    pub(super) search_query_value: String,
    pub(super) virtual_window: Signal<MessageVirtualWindowState>,
    pub(super) nav: BatchedSignal<crate::state::NavState>,
    pub(super) client_manager: BatchedSignal<ClientManager>,
    pub(super) chat_view_state: BatchedSignal<ChatViewState>,
    pub(super) top_edge_armed: Arc<AtomicBool>,
    pub(super) bottom_edge_armed: Arc<AtomicBool>,
    /// Signal updated by the scroll loop — true when the user is scrolled far
    /// enough from the live tail that "Jump to Present" should be shown.
    pub(super) scrolled_from_bottom: Signal<bool>,
}

/// Threshold constants (duplicated from mod.rs pub(super) constants).
const MESSAGE_HISTORY_EDGE_THRESHOLD_PX: f64 = 1.0;
const MESSAGE_HISTORY_EDGE_REARM_PX: f64 = 48.0;
const MAX_CHAINED_NEWER_HISTORY_PAGES: usize = 20;
const JUMP_TO_PRESENT_THRESHOLD_PX: f64 = 200.0;

/// Debounced scroll loop driver.
///
/// Called on every animation frame after a scroll event. Starts an async loop
/// that reads the DOM metrics and pages history when the user approaches a
/// spacer boundary. A `swap(true, Acquire)` on `scroll_work_in_flight` ensures
/// only one loop runs at a time; `scroll_work_requested` acts as a repeat flag.
pub(super) fn spawn_message_list_scroll_work(mut ctx: MessageListScrollWorkCtx) {
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

/// Load one page of older messages above the current working set.
pub(super) async fn load_older_messages(
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

/// Load one or more pages of newer messages below the current working set.
///
/// Chains up to `MAX_CHAINED_NEWER_HISTORY_PAGES` pages in a single backend
/// lock to minimise visible "page-attach" flicker and clear the bottom spacer
/// in one DOM update.
// lint-allow-unused: long cohesive view/handler; splitting risks reactive bugs
#[allow(clippy::too_many_lines)]
pub(super) async fn load_newer_messages(
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
pub(super) fn render_message_list(ctx: ChatViewMarkupCtx) -> Element {
    let loading = ctx.loading;
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
pub(super) fn render_message_list_loading_overlays(ctx: ChatViewMarkupCtx) -> Element {
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

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
pub(super) fn render_message_list_content(ctx: ChatViewMarkupCtx) -> Element {
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
pub(super) fn render_jump_to_present(ctx: ChatViewMarkupCtx) -> Element {
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
pub(super) fn render_unread_banner(ctx: ChatViewMarkupCtx) -> Element {
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
                "{crate::i18n::t_args(\"chat-unread-banner\", &[(\"count\", unread_banner_count.as_str()), (\"time\", unread_banner_time.as_str()), (\"date\", unread_banner_date.as_str())])}"
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
