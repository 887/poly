use dioxus::prelude::*;
use super::super::signals::ChatViewSignals;
use super::super::markup_ctx::ChatViewMarkupCtx;
use super::super::sync_mobile_side_column_open;

pub(in super::super) fn use_mobile_side_column_effect(signals: &ChatViewSignals, ctx: &ChatViewMarkupCtx) {
    let ui_layout = signals.ui_layout;
    let utility_panel = signals.utility_panel;
    let is_dm_channel = ctx.is_dm_channel;
    let is_group_channel = ctx.is_group_channel;

    use_effect(move || { // poly-lint: allow stale-effect-capture — is_dm_channel/is_group_channel are bool (Copy); app_state/utility_panel are Signals
        let member_list_open = if is_dm_channel || is_group_channel {
            ui_layout.read().dm_right_sidebar_visible
        } else {
            ui_layout.read().right_sidebar_visible
        };
        let agent_panel_open = false;
        let right_wing_open = member_list_open || utility_panel.read().is_some() || agent_panel_open;
        sync_mobile_side_column_open(right_wing_open);
    });
}
