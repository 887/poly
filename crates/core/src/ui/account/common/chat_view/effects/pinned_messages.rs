use dioxus::prelude::*;
use crate::client_manager::BackendHandleExt;
use crate::state::use_spawn_once;
use poly_client::MessagingBackend;
use super::super::signals::ChatViewSignals;
use super::super::ChatUtilityPanel;

pub(in super::super) fn use_pinned_messages_effect(signals: &ChatViewSignals) {
    let nav = signals.nav;
    let client_manager = signals.client_manager;
    let mut pinned_messages = signals.pinned_messages;
    let utility_panel = signals.utility_panel;

    // Key on (pinned-panel-open, channel_id) so opening the pinned panel
    // or switching channel while it's open re-spawns; other panel changes
    // don't.
    //
    // **PEEK, not READ** — both values are use_spawn_once keys. A live
    // .read() here subscribes ChatView to every write of utility_panel and
    // app_state.nav; when load_server_data writes selected_channel, ChatView
    // re-renders, this setup re-runs, the subscriptions re-fire — perpetual
    // loop (hang class #7, same as use_member_list_effect, commit 55f94246).
    let panel_is_pinned = *utility_panel.peek() == Some(ChatUtilityPanel::Pinned);
    let target_channel_id = nav.peek().selected_channel.cloned();
    use_spawn_once(
        (panel_is_pinned, target_channel_id),
        move |(panel_is_pinned, target_channel_id)| async move {
            if !panel_is_pinned {
                return;
            }
            let Some(target_channel_id) = target_channel_id else {
                pinned_messages.set(Vec::new());
                return;
            };
            let selected_server = nav.peek().selected_server.cloned();
            let active_account_id = nav.peek().active_account_id.cloned();
            let backend = if let Some(server_id) = selected_server {
                client_manager
                    .peek()
                    .get_backend_for_server(&server_id)
                    .map(|(_, handle)| handle)
            } else if let Some(account_id) = active_account_id {
                client_manager.peek().get_backend(&account_id)
            } else {
                None
            };
            let Some(backend) = backend else {
                pinned_messages.set(Vec::new());
                return;
            };
            let Ok(guard) = backend.read_with_timeout(std::time::Duration::from_secs(5)).await else {
                tracing::warn!("chat_view: backend read timed out in get_pinned_messages");
                pinned_messages.set(Vec::new());
                return;
            };
            let result = match guard.as_messaging() {
                Some(mb) => mb.get_pinned_messages(&target_channel_id).await,
                None => Ok(Vec::new()),
            };
            match result {
                Ok(messages) => pinned_messages.set(messages),
                Err(err) => {
                    tracing::warn!("get_pinned_messages failed: {err}");
                    pinned_messages.set(Vec::new());
                }
            }
        },
    );
}
