use dioxus::prelude::*;
use crate::client_manager::BackendHandleExt;
use crate::state::use_spawn_once;
use super::super::signals::ChatViewSignals;

pub(in super::super) fn use_command_preload_effect(signals: &ChatViewSignals, channel_id: &Option<String>) {
    let nav = signals.nav;
    let client_manager = signals.client_manager;
    let mut command_suggestions = signals.command_suggestions;
    let mut show_command_popup = signals.show_command_popup;
    let cmd_channel_id = channel_id.clone();

    use_spawn_once(cmd_channel_id, move |cmd_channel_id| async move {
        let Some(cid) = cmd_channel_id else {
            command_suggestions.set(Vec::new());
            show_command_popup.set(false);
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
            return;
        };
        let guard = match backend.read_with_timeout(std::time::Duration::from_secs(5)).await {
            Ok(g) => g,
            Err(_) => {
                tracing::warn!("chat_view: backend read timed out in get_channel_commands");
                return;
            }
        };
        match guard.get_channel_commands(&cid).await {
            Ok(cmds) => command_suggestions.set(cmds),
            Err(err) => tracing::warn!("get_channel_commands failed: {err}"),
        }
    });
}
