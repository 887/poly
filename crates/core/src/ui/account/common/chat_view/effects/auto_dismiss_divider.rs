use dioxus::prelude::*;
use super::super::ChatViewSignals;
use super::super::mark_channel_as_read;

/// Auto-dismiss the unread divider (and mark the channel as read) the moment the
/// user reaches the live tail of a channel.
///
/// Fires whenever `scrolled_from_bottom` transitions to `false` (i.e. the user is
/// at the bottom and there are no newer unloaded messages).  At that point, if
/// `history_state.unread_count == 0`, all unread messages have been seen:
///   - clear `unread_divider_visible` so the red line disappears immediately, and
///   - call `mark_channel_as_read` so the channel name loses its bold styling and
///     the server badge counters are decremented.
pub(in super::super) fn use_auto_dismiss_divider_effect(signals: &ChatViewSignals) {
    let scrolled_from_bottom = signals.scrolled_from_bottom;
    let history_state = signals.history_state;
    let chat_data = signals.chat_data;

    use_effect(move || { // poly-lint: allow stale-effect-capture — Signal-only; subscribes to scrolled_from_bottom, history_state, chat_data Signals
        // Only act when the user is at the bottom (not scrolled away from live tail).
        if *scrolled_from_bottom.read() {
            return;
        }

        let (unread_count, divider_visible, channel_id) = {
            let hs = history_state.read();
            (
                hs.unread_count,
                hs.unread_divider_visible,
                hs.channel_id.clone(),
            )
        };

        // If divider is already gone or there are still unseen messages, nothing to do.
        if !divider_visible || unread_count != 0 {
            return;
        }

        // User is at the bottom, has seen everything — clear the divider.
        history_state.batch(|h| h.unread_divider_visible = false);

        // Also clear bold / server badge so they don't linger.
        if let Some(channel_id) = channel_id {
            mark_channel_as_read(chat_data, &channel_id);
        }
    });
}
