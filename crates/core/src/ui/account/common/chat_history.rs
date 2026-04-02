//! Shared chat-history helpers for recent-first loading and scroll restoration.

use dioxus::prelude::document;
use poly_client::{Message, MessageQuery};
use serde_json::to_string as json_string;

/// Number of messages to load on first open when there is no unread context.
pub const INITIAL_MESSAGE_PAGE_SIZE: u32 = 36;
/// Older-history page size fetched when scrolling near the top.
pub const OLDER_MESSAGES_PAGE_SIZE: u32 = 50;
/// Maximum number of chat messages kept in memory for the active channel window.
pub const MAX_LOADED_MESSAGES: usize = 200;
/// Extra context messages to include above the unread boundary on first open.
pub const UNREAD_CONTEXT_MESSAGE_COUNT: u32 = 12;

/// Local UI state for the active chat history window.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChatHistoryUiState {
    /// Channel currently associated with this history state.
    pub channel_id: Option<String>,
    /// Whether older messages may still be available before the loaded window.
    pub has_more_before: bool,
    /// Whether an older-history request is currently in flight.
    pub loading_before: bool,
    /// Whether newer messages may still be available after the loaded window.
    pub has_more_after: bool,
    /// Whether a newer-history request is currently in flight.
    pub loading_after: bool,
    /// Estimated spacer height for unloaded older messages above the loaded DOM window.
    pub before_spacer_px: f64,
    /// Estimated spacer height for unloaded newer messages below the loaded DOM window.
    pub after_spacer_px: f64,
    /// Unread message count for the active channel.
    pub unread_count: u32,
    /// Message ID where the unread divider should be rendered.
    pub unread_marker_message_id: Option<String>,
    /// Whether the red unread-divider line should be shown.
    ///
    /// Distinct from `unread_count > 0`: the line persists after the user marks
    /// the channel as read (matching Discord behaviour — the line stays until the
    /// channel is switched, it just gets pushed up by new messages).
    pub unread_divider_visible: bool,
    /// Whether this state was initialized with actual messages (not during a loading race).
    ///
    /// Set to `true` only when `use_history_state_effect` runs with non-empty messages.
    /// Prevents the guard from treating a race-initialized state (empty messages at
    /// init time) as a valid initialization, so the effect will re-run once messages arrive.
    pub messages_loaded: bool,
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

/// Read the first visible message element and its pixel offset from the top of the
/// scroll container. This is used to preserve the exact visual anchor across fixed-
/// window page swaps.
pub async fn read_message_list_anchor() -> Option<(String, f64)> {
    let mut eval = document::eval(
        r#"
            const host = document.getElementById('message-list-scroll');
            if (!host) {
                dioxus.send('');
            } else {
                const hostRect = host.getBoundingClientRect();
                const rows = [...host.querySelectorAll('[id^="message-"]')];
                const anchor = rows.find((row) => {
                    const rect = row.getBoundingClientRect();
                    return rect.bottom > hostRect.top + 1 && rect.top < hostRect.bottom;
                });
                if (!anchor) {
                    dioxus.send('');
                } else {
                    const offset = anchor.getBoundingClientRect().top - hostRect.top;
                    dioxus.send(`${anchor.id}|${offset}`);
                }
            }
        "#,
    );

    let Ok(raw) = eval.recv::<String>().await else {
        return None;
    };
    let mut parts = raw.split('|');
    let element_id = parts.next()?.to_string();
    let offset = parts.next()?.parse::<f64>().ok()?;
    Some((element_id, offset))
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
        "window.polyRememberScrollPosition?.({encoded_channel_id})"
    ));
}

/// Scroll the message list to the bottom after the next render frame.
pub fn request_scroll_to_bottom() {
    document::eval("window.polyScrollToBottom?.()");
}

/// Restore a remembered scroll position for a channel, or fall back to bottom.
///
/// Also sets `window.__polyCurrentChannelId` so the auto-save scroll listener
/// can continuously track the position for this channel going forward.
pub fn request_restore_scroll_position_or_bottom(channel_id: &str) {
    let Some(encoded_channel_id) = encoded_channel_id(channel_id) else {
        request_scroll_to_bottom();
        return;
    };
    document::eval(&format!(
        "window.__polyCurrentChannelId = {encoded_channel_id}; window.polyRestoreScrollPosition?.({encoded_channel_id});"
    ));
}

/// Read the JS-side scrollend-saved view anchor for a channel.
///
/// Returns `(element_id, message_id, offset_px)` where:
/// - `element_id` is the DOM element ID (e.g. `"message-msg2-general-524"`) for `polyPreserveMessageAnchor`
/// - `message_id` is the raw message ID (e.g. `"msg2-general-524"`) for `MessageQuery::around`
/// - `offset_px` is pixels from the scroll container top (may be negative for partially-scrolled-off elements)
pub async fn read_channel_view_anchor(channel_id: &str) -> Option<(String, String, f64)> {
    let Some(encoded) = encoded_channel_id(channel_id) else {
        return None;
    };
    let mut eval = document::eval(&format!(
        r#"
        const a = (window.__polyChannelAnchors || {{}})[{encoded}];
        dioxus.send(a ? `${{a.elementId}}|${{a.messageId}}|${{a.offset}}` : '');
        "#
    ));
    let Ok(raw) = eval.recv::<String>().await else {
        return None;
    };
    if raw.is_empty() {
        return None;
    }
    let mut parts = raw.splitn(3, '|');
    let element_id = parts.next()?.to_string();
    let message_id = parts.next()?.to_string();
    let offset = parts.next()?.parse::<f64>().ok()?;
    Some((element_id, message_id, offset))
}

/// Restore the viewport so that `element_id` appears at `offset_px` from the scroll container top.
///
/// Also sets `__polyCurrentChannelId` to activate auto-save tracking.
/// Wraps `polyPreserveMessageAnchor` which is already RAF-deferred.
pub fn request_restore_to_anchor(channel_id: &str, element_id: &str, offset_px: f64) {
    let Some(encoded_channel_id) = encoded_channel_id(channel_id) else {
        return;
    };
    let msg_json = json_string(element_id).unwrap_or_default();
    document::eval(&format!(
        "window.__polyCurrentChannelId = {encoded_channel_id}; window.polyPreserveMessageAnchor?.({msg_json}, {offset_px});"
    ));
}

/// Adjust the message-list scrollTop by an explicit pixel delta after the next render.
///
/// This is used for fixed-window message swapping where the DOM can both prepend and
/// trim content in the same update. In that model, preserving by *net scroll height*
/// is wrong: to keep the same anchor message visible we must preserve by the exact
/// inserted/removed height near the active edge.
fn request_adjust_scroll_top_by(delta_px: f64, previous_scroll_top: f64) {
    document::eval(&format!(
        "window.polyPreserveScrollDelta?.({previous_scroll_top}, {delta_px})"
    ));
}

/// Preserve the user's viewport after prepending older messages.
///
/// `prepended_height_px` should be the estimated pixel height of the messages inserted
/// above the current viewport. We add that exact delta to scrollTop so the same anchor
/// message remains under the user's eyes even if another page is simultaneously trimmed
/// from the bottom of the fixed working set.
pub fn request_preserve_scroll_position(previous_scroll_top: f64, prepended_height_px: f64) {
    request_adjust_scroll_top_by(prepended_height_px, previous_scroll_top);
}

/// Preserve the user's viewport after appending newer messages while trimming older ones.
///
/// `trimmed_top_height_px` should be the estimated pixel height removed from the top of
/// the working set. We subtract that exact amount from scrollTop so the same anchor
/// message remains visible while newer content is swapped in below.
pub fn request_preserve_scroll_position_from_bottom(
    previous_scroll_top: f64,
    trimmed_top_height_px: f64,
) {
    request_adjust_scroll_top_by(-trimmed_top_height_px, previous_scroll_top);
}

/// Preserve the exact visible anchor message at the same pixel offset after the next render.
pub fn request_preserve_message_anchor(anchor_element_id: &str, offset_px: f64) {
    let Ok(encoded_anchor_id) = json_string(anchor_element_id) else {
        return;
    };
    document::eval(&format!(
        "window.polyPreserveMessageAnchor?.({encoded_anchor_id}, {offset_px})"
    ));
}
