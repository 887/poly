use dioxus::prelude::*;
use crate::state::AppState;
use crate::state::BatchedSignal;
use super::super::persist_member_list_preferences;

pub(in super::super) fn use_member_list_preferences_effect(app_state: BatchedSignal<AppState>) {
    use_effect(move || { // poly-lint: allow stale-effect-capture — Signal-only; subscribes to app_state Signal
        let server_member_list_open = app_state.read().nav.right_sidebar_visible;
        let dm_member_list_open = app_state.read().nav.dm_right_sidebar_visible;
        spawn(async move {
            persist_member_list_preferences(server_member_list_open, dm_member_list_open).await;
        });
    });
}
