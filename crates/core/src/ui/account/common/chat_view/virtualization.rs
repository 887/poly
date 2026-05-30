//! Virtualization engine for the chat message list.
//!
//! Pure scroll-window math: viewport size estimation, message height
//! estimation, virtual window computation, and message-window trimming.
//! No reactive state (Signal reads/writes) lives here — only plain functions
//! that operate on slices of data and return new values.

use dioxus::prelude::*;
use poly_client::Message;
use super::super::chat_history::{ChatHistoryUiState, MAX_LOADED_MESSAGES};

// ---------------------------------------------------------------------------
// Virtualization constants
// ---------------------------------------------------------------------------

pub(super) const MESSAGE_VIRTUALIZATION_THRESHOLD: usize = 10_000;
pub(super) const MESSAGE_VIRTUALIZATION_OVERSCAN_PX: f64 = 1200.0;
pub(super) const MESSAGE_VIRTUALIZATION_MIN_RENDERED: usize = 96;
/// History sentinels only exist so the browser can observe edge entry.
/// Keep them tiny so the native scrollbar mostly reflects the real 200-row
/// working set instead of a synthetic fake scroll range.
pub(super) const MESSAGE_HISTORY_SENTINEL_PX: f64 = 8.0;

// ---------------------------------------------------------------------------
// Per-message height estimates (used for virtual window computation)
// ---------------------------------------------------------------------------

pub(super) const ESTIMATED_FULL_MESSAGE_HEIGHT: f64 = 92.0;
pub(super) const ESTIMATED_GROUPED_MESSAGE_HEIGHT: f64 = 34.0;
pub(super) const ESTIMATED_DATE_SEPARATOR_HEIGHT: f64 = 28.0;
pub(super) const ESTIMATED_UNREAD_DIVIDER_HEIGHT: f64 = 20.0;
pub(super) const ESTIMATED_REPLY_PREVIEW_HEIGHT: f64 = 22.0;
pub(super) const ESTIMATED_REACTION_BAR_HEIGHT: f64 = 28.0;
pub(super) const ESTIMATED_IMAGE_ATTACHMENT_HEIGHT: f64 = 180.0;
pub(super) const ESTIMATED_FILE_ATTACHMENT_HEIGHT: f64 = 52.0;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct MessageVirtualWindowState {
    pub(super) enabled: bool,
    pub(super) start_idx: usize,
    pub(super) end_idx: usize,
    pub(super) top_spacer_px: f64,
    pub(super) bottom_spacer_px: f64,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct MessageListViewportMetrics {
    pub(super) scroll_top: f64,
    pub(super) client_height: f64,
    pub(super) scroll_height: f64,
}

// ---------------------------------------------------------------------------
// Pure virtualization functions
// ---------------------------------------------------------------------------

pub(super) fn should_virtualize_messages(message_count: usize, search_query_value: &str) -> bool {
    message_count > MESSAGE_VIRTUALIZATION_THRESHOLD && search_query_value.is_empty()
}

pub(super) fn estimate_message_row_height(
    messages: &[Message],
    idx: usize,
    unread_marker_id: Option<&str>,
    unread_count: u32,
) -> f64 {
    let Some(msg) = messages.get(idx) else {
        return ESTIMATED_FULL_MESSAGE_HEIGHT;
    };
    let show_date_sep = if idx == 0 {
        true
    } else {
        messages
            .get(idx.saturating_sub(1))
            .is_none_or(|prev| msg.timestamp.date_naive() != prev.timestamp.date_naive())
    };
    let is_grouped = if idx == 0 {
        false
    } else {
        messages.get(idx.saturating_sub(1)).is_some_and(|prev| {
            prev.author.id == msg.author.id
                && !show_date_sep
                && msg
                    .timestamp
                    .signed_duration_since(prev.timestamp)
                    .num_minutes()
                    < super::GROUP_THRESHOLD_MINUTES
        })
    };

    let mut height = if is_grouped {
        ESTIMATED_GROUPED_MESSAGE_HEIGHT
    } else {
        ESTIMATED_FULL_MESSAGE_HEIGHT
    };

    if show_date_sep {
        height += ESTIMATED_DATE_SEPARATOR_HEIGHT;
    }
    if unread_count > 0 && unread_marker_id == Some(msg.id.as_str()) {
        height += ESTIMATED_UNREAD_DIVIDER_HEIGHT;
    }
    if msg.reply_to.is_some() {
        height += ESTIMATED_REPLY_PREVIEW_HEIGHT;
    }
    if !msg.reactions.is_empty() {
        height += ESTIMATED_REACTION_BAR_HEIGHT;
    }

    for attachment in &msg.attachments {
        let attachment_height = if attachment.content_type.starts_with("image/") {
            ESTIMATED_IMAGE_ATTACHMENT_HEIGHT
        } else {
            ESTIMATED_FILE_ATTACHMENT_HEIGHT
        };
        height += attachment_height;
    }

    height
}

pub(super) fn estimate_message_block_height(
    messages: &[Message],
    start_idx: usize,
    end_idx: usize,
    unread_marker_id: Option<&str>,
    unread_count: u32,
) -> f64 {
    if start_idx >= end_idx || start_idx >= messages.len() {
        return 0.0;
    }

    let capped_end = end_idx.min(messages.len());
    let mut total = 0.0_f64;
    for idx in start_idx..capped_end {
        total += estimate_message_row_height(messages, idx, unread_marker_id, unread_count);
    }
    total
}

pub(super) fn recompute_history_spacers(history: &mut ChatHistoryUiState, _messages: &[Message]) {
    history.before_spacer_px = if history.has_more_before {
        MESSAGE_HISTORY_SENTINEL_PX
    } else {
        0.0_f64
    };
    history.after_spacer_px = if history.has_more_after {
        MESSAGE_HISTORY_SENTINEL_PX
    } else {
        0.0_f64
    };
}

pub(super) fn compute_message_virtual_window(
    messages: &[Message],
    unread_marker_id: Option<&str>,
    unread_count: u32,
    metrics: MessageListViewportMetrics,
) -> MessageVirtualWindowState {
    if messages.is_empty() {
        return MessageVirtualWindowState::default();
    }

    let mut prefix_heights = Vec::with_capacity(messages.len().saturating_add(1));
    prefix_heights.push(0.0_f64);
    for idx in 0..messages.len() {
        let prev = prefix_heights.last().copied().unwrap_or(0.0_f64);
        let next = prev + estimate_message_row_height(messages, idx, unread_marker_id, unread_count);
        prefix_heights.push(next);
    }

    let viewport_start = (metrics.scroll_top - MESSAGE_VIRTUALIZATION_OVERSCAN_PX).max(0.0_f64);
    let viewport_end =
        metrics.scroll_top + metrics.client_height + MESSAGE_VIRTUALIZATION_OVERSCAN_PX;

    let mut start_idx = 0_usize;
    while start_idx < messages.len()
        && prefix_heights
            .get(start_idx.saturating_add(1))
            .copied()
            .is_some_and(|height| height < viewport_start)
    {
        start_idx = start_idx.saturating_add(1);
    }

    let mut end_idx = start_idx;
    while end_idx < messages.len()
        && prefix_heights
            .get(end_idx)
            .copied()
            .is_some_and(|height| height <= viewport_end)
    {
        end_idx = end_idx.saturating_add(1);
    }

    if end_idx.saturating_sub(start_idx) < MESSAGE_VIRTUALIZATION_MIN_RENDERED {
        let extra = MESSAGE_VIRTUALIZATION_MIN_RENDERED.saturating_sub(end_idx.saturating_sub(start_idx));
        // lint-allow-unused: floor division — splits leftover slack evenly above/below viewport
        #[allow(clippy::integer_division)]
        let extra_before = extra / 2;
        start_idx = start_idx.saturating_sub(extra_before);
        end_idx = start_idx.saturating_add(MESSAGE_VIRTUALIZATION_MIN_RENDERED).min(messages.len());
        start_idx = end_idx.saturating_sub(MESSAGE_VIRTUALIZATION_MIN_RENDERED);
    }

    let total_height = prefix_heights.last().copied().unwrap_or(0.0_f64);
    let top_spacer_px = prefix_heights.get(start_idx).copied().unwrap_or(0.0_f64);
    let bottom_spacer_px =
        total_height - prefix_heights.get(end_idx).copied().unwrap_or(total_height);

    MessageVirtualWindowState {
        enabled: true,
        start_idx,
        end_idx,
        top_spacer_px,
        bottom_spacer_px,
    }
}

pub(super) async fn read_message_list_viewport_metrics() -> Option<MessageListViewportMetrics> {
    let mut eval = document::eval(
        r"
            const el = document.getElementById('message-list-scroll');
            if (!el) {
                dioxus.send('');
            } else {
                dioxus.send(`${el.scrollTop}|${el.clientHeight}|${el.scrollHeight}`);
            }
        ",
    );

    let Ok(raw) = eval.recv::<String>().await else {
        return None;
    };
    let mut parts = raw.split('|');
    let scroll_top = parts.next()?.parse::<f64>().ok()?;
    let client_height = parts.next()?.parse::<f64>().ok()?;
    let scroll_height = parts.next()?.parse::<f64>().ok()?;
    Some(MessageListViewportMetrics {
        scroll_top,
        client_height,
        scroll_height,
    })
}

pub(super) fn trim_message_window_from_bottom(messages: &mut Vec<Message>) -> bool {
    if messages.len() <= MAX_LOADED_MESSAGES {
        return false;
    }
    messages.truncate(MAX_LOADED_MESSAGES);
    true
}

pub(super) fn trim_message_window_from_top(messages: &mut Vec<Message>) -> bool {
    if messages.len() <= MAX_LOADED_MESSAGES {
        return false;
    }
    let overflow = messages.len().saturating_sub(MAX_LOADED_MESSAGES);
    messages.drain(0..overflow);
    true
}

pub(super) fn set_message_virtual_window(
    mut virtual_window: Signal<MessageVirtualWindowState>,
    messages: &[Message],
    unread_marker_id: Option<&str>,
    unread_count: u32,
    metrics: MessageListViewportMetrics,
) {
    let next = compute_message_virtual_window(messages, unread_marker_id, unread_count, metrics);
    if *virtual_window.read() != next {
        virtual_window.set(next);
    }
}

pub(super) async fn wait_for_next_animation_frame() -> bool {
    let mut eval = document::eval(
        r"
            requestAnimationFrame(() => {
                dioxus.send(true);
            });
        ",
    );

    eval.recv::<bool>().await.unwrap_or(false)
}
