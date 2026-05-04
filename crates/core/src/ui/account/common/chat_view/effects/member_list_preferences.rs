use dioxus::prelude::*;
use crate::state::BatchedSignal;
use crate::state::UiLayout;
use super::super::persist_member_list_preferences;

pub(in super::super) fn use_member_list_preferences_effect(ui_layout: BatchedSignal<UiLayout>) {
    use_effect(move || { // poly-lint: allow stale-effect-capture — Signal-only; subscribes to ui_layout Signal
        let server_member_list_open = ui_layout.read().right_sidebar_visible;
        let dm_member_list_open = ui_layout.read().dm_right_sidebar_visible;
        spawn(async move {
            persist_member_list_preferences(server_member_list_open, dm_member_list_open).await;
        });
    });
}
