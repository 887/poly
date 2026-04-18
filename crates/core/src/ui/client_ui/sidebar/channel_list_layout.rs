//! `SidebarLayoutKind::ChannelList` — the classic Discord / Stoat / Teams /
//! demo / poly-native sidebar.
//!
//! This is a thin wrapper around [`crate::ui::account::common::ChannelList`].
//! The 59 KB of existing sidebar behaviour (categories, DMs, friends,
//! search filter, scroll persistence, voice-participant rows) stays put —
//! we just delegate so that stock-layout backends get the current UI
//! verbatim after their plugin declares `SidebarLayoutKind::ChannelList`.
//!
//! See `docs/plans/plan-client-ui-surface.md` §7 WP 4 — "make ClientSidebar
//! delegate to it when layout=ChannelList; DO NOT MOVE IT".

use crate::ui::account::common::ChannelList;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Proxy wrapper that re-uses the existing [`ChannelList`] component.
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub fn ChannelListLayout() -> Element {
    rsx! {
        // Delegate to the existing ChannelList (reads AppState / ChatData
        // from context). No props to proxy.
        ChannelList {}
    }
}
