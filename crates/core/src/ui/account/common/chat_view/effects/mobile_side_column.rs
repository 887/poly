use dioxus::prelude::*;
use super::super::ChatViewSignals;
use super::super::ChatViewMarkupCtx;
use super::super::sync_mobile_side_column_open;

pub(in super::super) fn use_mobile_side_column_effect(signals: &ChatViewSignals, ctx: &ChatViewMarkupCtx) {
    let app_state = signals.app_state;
    let utility_panel = signals.utility_panel;
    let is_dm_channel = ctx.is_dm_channel;
    let is_group_channel = ctx.is_group_channel;

    use_effect(move || { // poly-lint: allow stale-effect-capture — is_dm_channel/is_group_channel are bool (Copy); app_state/utility_panel are Signals
        let member_list_open = if is_dm_channel || is_group_channel {
            app_state.read().nav.dm_right_sidebar_visible
        } else {
            app_state.read().nav.right_sidebar_visible
        };
        let agent_panel_open = false;
        let right_wing_open = member_list_open || utility_panel.read().is_some() || agent_panel_open;
        sync_mobile_side_column_open(right_wing_open);
    });
}
