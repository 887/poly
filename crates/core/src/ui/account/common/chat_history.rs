//! Shared chat-history helpers for recent-first loading and scroll restoration.

use dioxus::prelude::document;
use poly_client::{Message, MessageQuery};
use serde_json::to_string as json_string;

/// Number of messages to load on first open when there is no unread context.
pub const INITIAL_MESSAGE_PAGE_SIZE: u32 = 36;
/// Older-history page size fetched when scrolling near the top.
pub const OLDER_MESSAGES_PAGE_SIZE: u32 = 48;
/// Extra context messages to include above the unread boundary on first open.
pub const UNREAD_CONTEXT_MESSAGE_COUNT: u32 = 12;

/// Local UI state for the active chat history window.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChatHistoryUiState {
    /// Channel currently associated with this history state.
    pub channel_id: Option<String>,
    /// Whether older messages may still be available before the loaded window.
    pub has_more_before: bool,
    /// Whether an older-history request is currently in flight.
    pub loading_before: bool,
    /// Unread message count for the active channel.
    pub unread_count: u32,
    /// Message ID where the unread divider should be rendered.
    pub unread_marker_message_id: Option<String>,
}

/// Build the initial message query for a channel based on unread count.
pub fn initial_message_query(unread_count: u32) -> MessageQuery {
    let limit =
        INITIAL_MESSAGE_PAGE_SIZE.max(unread_count.saturating_add(UNREAD_CONTEXT_MESSAGE_COUNT));

    MessageQuery {
        limit: Some(limit),
        ..Default::default()
    }
}

/// Return the message ID where the unread divider should appear.
pub fn unread_marker_message_id(messages: &[Message], unread_count: u32) -> Option<String> {
    if unread_count == 0 || messages.is_empty() {
        return None;
    }

    let unread_count = usize::try_from(unread_count).unwrap_or(messages.len());
    let marker_index = messages.len().saturating_sub(unread_count);
    messages
        .get(marker_index)
        .or_else(|| messages.first())
        .map(|message| message.id.clone())
}

/// Read the current message-list scroll metrics `(scroll_top, scroll_height)`.
pub async fn read_message_list_scroll_metrics() -> Option<(f64, f64)> {
    let mut eval = document::eval(
        r#"
            const el = document.getElementById('message-list-scroll');
            if (!el) {
                dioxus.send('');
            } else {
                dioxus.send(`${el.scrollTop}|${el.scrollHeight}`);
            }
        "#,
    );

    let Ok(raw) = eval.recv::<String>().await else {
        return None;
    };
    let mut parts = raw.split('|');
    let scroll_top = parts.next()?.parse::<f64>().ok()?;
    let scroll_height = parts.next()?.parse::<f64>().ok()?;
    Some((scroll_top, scroll_height))
}

fn encoded_channel_id(channel_id: &str) -> Option<String> {
    json_string(channel_id).ok()
}

/// Remember the current message-list scroll position for a channel.
pub fn remember_message_list_scroll_position(channel_id: &str) {
    let Some(encoded_channel_id) = encoded_channel_id(channel_id) else {
        return;
    };

    document::eval(&format!(
        r#"
            window.__polyMessageScrollPositions ??= Object.create(null);
            const el = document.getElementById('message-list-scroll');
            if (el) {{
                window.__polyMessageScrollPositions[{encoded_channel_id}] = el.scrollTop;
            }}
        "#,
    ));
}

fn request_scroll_top(scroll_script: &str) {
    document::eval(&format!(
        r#"
            window.__polyScrollRequestSeq = (window.__polyScrollRequestSeq ?? 0) + 1;
            const seq = window.__polyScrollRequestSeq;
            if (window.__polyScrollRafId) {{
                cancelAnimationFrame(window.__polyScrollRafId);
            }}
            if (Array.isArray(window.__polyScrollTimeoutIds)) {{
                for (const timeoutId of window.__polyScrollTimeoutIds) {{
                    clearTimeout(timeoutId);
                }}
            }}
            window.__polyScrollTimeoutIds = [];
            const applyScroll = () => {{
                if (window.__polyScrollRequestSeq !== seq) {{
                    return;
                }}
                const el = document.getElementById('message-list-scroll');
                if (el) {{
                    {scroll_script}
                }}
            }};
            window.__polyScrollRafId = requestAnimationFrame(() => {{
                applyScroll();
                window.__polyScrollTimeoutIds = [32, 90, 240, 480].map(
                    (delay) => setTimeout(applyScroll, delay),
                );
            }});
        "#,
    ));
}

/// Scroll the message list to the bottom after the next render frame.
pub fn request_scroll_to_bottom() {
    request_scroll_top("el.scrollTop = el.scrollHeight;");
}

/// Restore a remembered scroll position for a channel, or fall back to bottom.
pub fn request_restore_scroll_position_or_bottom(channel_id: &str) {
    let Some(encoded_channel_id) = encoded_channel_id(channel_id) else {
        request_scroll_to_bottom();
        return;
    };

    request_scroll_top(&format!(
        r#"
            window.__polyMessageScrollPositions ??= Object.create(null);
            const saved = window.__polyMessageScrollPositions[{encoded_channel_id}];
            if (Number.isFinite(saved)) {{
                el.scrollTop = saved;
            }} else {{
                el.scrollTop = el.scrollHeight;
            }}
        "#,
    ));
}

/// Preserve the user's viewport after prepending older messages.
pub fn request_preserve_scroll_position(previous_scroll_top: f64, previous_scroll_height: f64) {
    document::eval(&format!(
        r#"
            const el = document.getElementById('message-list-scroll');
            if (el) {{
                requestAnimationFrame(() => {{
                    const nextTop = (el.scrollHeight - {previous_scroll_height}) + {previous_scroll_top};
                    el.scrollTop = Math.max(0, nextTop);
                }});
            }}
        "#,
    ));
}
