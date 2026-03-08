//! Shared chat-history helpers for recent-first loading and scroll restoration.

use dioxus::prelude::document;
use poly_client::{Message, MessageQuery};

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

/// Scroll the message list to the bottom after the next render frame.
pub fn request_scroll_to_bottom() {
    document::eval(
        r#"
            const snapToBottom = () => {
                const el = document.getElementById('message-list-scroll');
                if (el) {
                    el.scrollTop = el.scrollHeight;
                }
            };
            requestAnimationFrame(() => {
                snapToBottom();
                setTimeout(snapToBottom, 32);
                setTimeout(snapToBottom, 90);
                setTimeout(snapToBottom, 240);
                setTimeout(snapToBottom, 480);
            });
        "#,
    );
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
