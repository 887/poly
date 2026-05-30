use dioxus::prelude::*;
use crate::client_manager::BackendHandleExt;
use crate::state::use_spawn_once;
use super::super::signals::ChatViewSignals;

pub(in super::super) fn use_member_list_effect(signals: &ChatViewSignals) {
    let nav = signals.nav;
    let client_manager = signals.client_manager;
    let chat_view_state = signals.chat_view_state;

    // Key on Option<String> so channel-unset also has a stable key and we
    // only clear members once per transition. PartialEq on Option handles
    // both arms cleanly.
    //
    // **PEEK, not READ** — this is at the TOP of use_chat_view_effects'
    // setup body and runs on every ChatView render. A live `.read()` here
    // subscribes ChatView to every `app_state` write, so when
    // load_server_data's terminal pending.apply() writes app_state.nav.
    // selected_channel via unsafe_presync_override, ChatView re-renders,
    // this setup re-runs, the read fires the subscription again — perpetual
    // re-render loop that wedges the WASM scheduler. Found via SQLite-
    // persisted bisect trace: `load_server_data` ran 1×, ChatView setup
    // ran 1408×. peek() breaks the subscription; the use_spawn_once below
    // re-evaluates this key on every legitimate ChatView re-render anyway,
    // so channel switches still propagate.
    let active_channel_id = nav.peek().selected_channel.cloned();
    use_spawn_once(active_channel_id, move |active_channel_id| async move {
        let chat_view_state = chat_view_state;
        let Some(active_channel_id) = active_channel_id else {
            chat_view_state.batch(|cv| {
                cv.members = Vec::new();
                cv.active_group_members = Vec::new();
            });
            return;
        };

        let selected_server = nav.peek().selected_server.cloned();
        let active_account_id = nav.peek().active_account_id.cloned();
        let is_group = active_channel_id.starts_with("group-");

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
            chat_view_state.batch(|cv| {
                cv.members = Vec::new();
                cv.active_group_members = Vec::new();
            });
            return;
        };
        let Ok(guard) = backend.read_with_timeout(std::time::Duration::from_secs(5)).await else {
            tracing::warn!("chat_view: backend read timed out in get_channel_members");
            return;
        };
        match guard.get_channel_members(&active_channel_id).await {
            Ok(members) => {
                chat_view_state.batch(move |cv| {
                    cv.members = members.clone();
                    cv.active_group_members = if is_group { members } else { Vec::new() };
                });
            }
            Err(err) => {
                tracing::warn!(
                    "get_channel_members failed for channel {}: {}",
                    active_channel_id,
                    err
                );
                chat_view_state.batch(|cv| {
                    cv.members = Vec::new();
                    cv.active_group_members = Vec::new();
                });
            }
        }
    });
}
