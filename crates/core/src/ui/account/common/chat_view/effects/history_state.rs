use dioxus::prelude::*;
use super::super::ChatViewSignals;
use super::super::super::chat_history::{ChatHistoryUiState, unread_marker_message_id};
use super::super::virtualization::recompute_history_spacers;
use super::super::current_channel_unread_count;
use super::super::mark_channel_as_read_with_backend;

pub(in super::super) fn use_history_state_effect(signals: &ChatViewSignals) {
    let app_state = signals.app_state;
    let chat_data = signals.chat_data;
    let client_manager = signals.client_manager;
    let history_state = signals.history_state;
    let mut scrolled_from_bottom = signals.scrolled_from_bottom;
    let mut new_messages_while_scrolled_up = signals.new_messages_while_scrolled_up;

    use_effect(move || { // poly-lint: allow stale-effect-capture — Signal-only; subscribes to app_state, chat_data, history_state Signals
        let Some(active_channel_id) = app_state.read().nav.selected_channel.cloned() else {
            // hang #8 countermeasure: skip the write when already default,
            // otherwise the unconditional write re-fires the effect.
            history_state.set_if_changed(ChatHistoryUiState::default());
            return;
        };
        let chat_snapshot = chat_data.read().clone();
        if chat_snapshot.loading {
            return;
        }
        let prev_channel_id = history_state.read().channel_id.clone();
        let prev_messages_loaded = history_state.read().messages_loaded;
        let is_channel_switch = prev_channel_id.as_deref() != Some(&active_channel_id);

        if !is_channel_switch && prev_messages_loaded {
            return;
        }

        if is_channel_switch {
            scrolled_from_bottom.set(false);
            new_messages_while_scrolled_up.set(0);
        }
        let messages = chat_snapshot.messages.clone();
        let unread_count = current_channel_unread_count(
            Some(&active_channel_id),
            chat_snapshot.current_channel.as_ref(),
            &chat_snapshot.dm_channels,
        );
        let messages_loaded = !messages.is_empty();
        let has_more_after = messages_loaded && chat_snapshot.messages_loaded_via_anchor;
        let active_channel_id_for_mark = active_channel_id.clone();
        let mut next_history = ChatHistoryUiState {
            channel_id: Some(active_channel_id),
            has_more_before: messages_loaded,
            loading_before: false,
            has_more_after,
            loading_after: false,
            before_spacer_px: 0.0,
            after_spacer_px: 0.0,
            unread_count,
            unread_marker_message_id: unread_marker_message_id(&messages, unread_count),
            // Show the unread divider on channel open when there are unread messages.
            // The divider persists until the channel is switched — we capture the
            // marker position now and (below) flush chat_data.unread_count to 0 so
            // re-entering the channel later (without new messages) shows no divider.
            unread_divider_visible: unread_count > 0,
            messages_loaded,
        };
        recompute_history_spacers(&mut next_history, &messages);
        // `set_if_changed` (hang class #8 countermeasure) — without the
        // equality check, an empty channel (`messages_loaded` stays
        // `false` because `messages.is_empty()`) re-runs the
        // early-return-disabled branch every render, writes
        // `history_state` every time, re-fires every `history_state`
        // subscriber (this effect included), and pegs the WASM scheduler.
        // Witnessed 2026-04-25 on Teams T001/CH002 (3162 ChatView
        // re-renders for one load_server_data call).
        history_state.set_if_changed(next_history);

        // Mark the channel as read in chat_data the moment the user actually
        // sees it with messages loaded. Discord-style: the in-channel divider
        // stays visible for this visit (preserved in history_state above), but
        // the sidebar bolding + server unread badge clear immediately, and the
        // next time the user opens this channel they get a clean view (unless
        // new messages arrived). Two entry conditions cover both code paths:
        //   - sync: channel switch + messages already in chat_data
        //   - async: channel switch with empty messages, then messages load
        // The early-return guard above prevents the resulting chat_data write
        // from re-running the marker computation and erasing the divider.
        let entered_with_messages =
            messages_loaded && unread_count > 0 && (is_channel_switch || !prev_messages_loaded);
        if entered_with_messages {
            let server_id = app_state.read().nav.selected_server.cloned();
            let account_id = app_state.read().nav.active_account_id.cloned();
            mark_channel_as_read_with_backend(
                chat_data,
                client_manager,
                account_id,
                server_id,
                &active_channel_id_for_mark,
            );
        }
    });
}
