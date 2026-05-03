//! Plugin-declared sidebar dispatcher.
//!
//! The `ClientSidebar` component reads the currently active account's
//! [`SidebarDeclaration`] (via `ClientBackend::get_sidebar_declaration`) and
//! dispatches to one of five layout sub-components:
//!
//! - [`ChannelListLayout`] — Discord / Stoat / Teams / Matrix / poly-native / demo.
//!   A thin wrapper around the existing
//!   [`crate::ui::account::common::ChannelList`] so backends that declare
//!   `SidebarLayoutKind::ChannelList` keep their existing UI verbatim.
//!   `SpacesRooms` and `Custom` also fall through here (Matrix used `Custom`
//!   for a spaces-tree experiment that broke the multi-column shell layout;
//!   routing both to `ChannelListLayout` restores the standard column shape).
//! - [`CommunitiesLayout`] — Lemmy-style subscribed communities.
//! - [`FeedLayout`] — HN-style feed tabs (Top / New / Best / Ask / Show / Jobs).
//! - [`RepoTreeLayout`] — GitHub / Forgejo repo list with Issues / PRs /
//!   Discussions sub-items.
//! - [`CustomSidebar`] — renders a plugin-declared `sections` tree when
//!   `layout == SidebarLayoutKind::Custom`.
//!
//! When the active account cannot be resolved (e.g. DM view pre-selection),
//! we fall back to [`ChannelListLayout`] which internally handles the DM +
//! friends empty state via the existing `ChannelList` component.

pub mod channel_list_layout;
pub mod communities;
pub mod custom;
pub mod feed;
pub mod repo_tree;
pub mod sort_modes;

pub use channel_list_layout::ChannelListLayout;
pub use communities::CommunitiesLayout;
pub use custom::CustomSidebar;
pub use feed::FeedLayout;
pub use repo_tree::RepoTreeLayout;
pub use sort_modes::SortModesLayout;

use crate::client_manager::{BackendHandleExt, ClientManager};
use crate::i18n::t;
use crate::state::{AppState, BatchedSignal};
use dioxus::prelude::*;
use poly_client::{ClientError, SidebarDeclaration, SidebarLayoutKind};
use poly_ui_macros::{context_menu, ui_action};

/// Dispatcher — reads the active account's declared sidebar layout and
/// delegates to the matching layout sub-component.
///
/// Props-free: the active account comes from [`AppState::nav`] (the same
/// context source the existing `ChannelList` uses), so existing call sites
/// only have to swap `ChannelList {}` for `ClientSidebar {}`.
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub fn ClientSidebar() -> Element {
    let app_state: BatchedSignal<AppState> = use_context();
    let client_manager: BatchedSignal<ClientManager> = use_context();

    // Resolve the current account. `active_account_id` is populated by the
    // router's `on_update`. When we're on a DM / friends / app-level route
    // it may be None — in that case we render the ChannelListLayout which
    // itself delegates to the existing `ChannelList` that handles the
    // DMs-home empty state.
    let account_id = app_state.read().nav.active_account_id.cloned();
    // When a repo backend (RepoTree) has a specific server selected, the inner
    // sidebar must show only that repo's channels — not the full repo list.
    // Read reactively so the layout switches when the user navigates into or
    // out of a repo.
    let has_selected_server = app_state.read().nav.selected_server.is_some();

    let decl_res = {
        let account_id = account_id.clone();
        use_resource(move || {
            // P28 — subscribe to sidebar_invalidated_tick so plugin-emitted
            // `ClientEvent::SidebarInvalidated` events force a refetch.
            let _tick = app_state.read().sidebar_invalidated_tick;
            // E6 — subscribe to client_manager so the resource re-runs when a
            // backend is committed after first account activation (Discord and
            // other native backends may be committed after the route sets
            // `active_account_id`, causing a transient NotFound error). The
            // reactive subscription here means that when `commit_backend_account`
            // writes to `client_manager`, this resource re-runs and finds the
            // backend on the second attempt.
            let _ = client_manager.read();
            let account_id = account_id.clone();
            async move {
                let Some(account_id) = account_id else {
                    // Synthesize a ChannelList declaration for the
                    // account-less case — the layout wrapper ignores the
                    // declaration's `sections` / `header_block` anyway.
                    return Ok::<SidebarDeclaration, ClientError>(SidebarDeclaration {
                        layout: SidebarLayoutKind::ChannelList,
                        sections: Vec::new(),
                        header_block: None,
                    });
                };
                let Some(backend) = client_manager.read().get_backend(&account_id) else {
                    return Err(ClientError::NotFound(format!(
                        "no backend for account {account_id}"
                    )));
                };
                let guard = match backend.read_with_timeout(std::time::Duration::from_secs(5)).await {
                    Ok(g) => g,
                    Err(_) => {
                        tracing::warn!("sidebar: backend read timed out loading sidebar declaration");
                        return Err(ClientError::Internal("backend read timed out".into()));
                    }
                };
                guard.get_sidebar_declaration().await
            }
        })
    };

    match &*decl_res.read_unchecked() {
        None => rsx! {
            aside { class: "client-sidebar client-sidebar-loading",
                // Loading state: still show the ChannelList so returning
                // users don't flash an empty panel. The actual layout may
                // differ once the declaration resolves.
                ChannelListLayout {}
            }
        },
        Some(Err(err)) => {
            tracing::warn!("ClientSidebar: get_sidebar_declaration failed: {err:?}");
            let msg = t("ui-sidebar-plugin-error");
            rsx! {
                aside { class: "client-sidebar client-sidebar-error",
                    // P29 — discrete error badge above the fallback layout so
                    // users know the plugin's sidebar declaration failed but
                    // still have working channel navigation.
                    div {
                        class: "client-sidebar-error-badge",
                        role: "status",
                        aria_live: "polite",
                        "{msg}"
                    }
                    // Fall back to the stock channel list so the user still
                    // has navigation even when the plugin errors.
                    ChannelListLayout {}
                }
            }
        }
        Some(Ok(decl)) => {
            let decl = decl.clone();
            match decl.layout {
                // Standard chat-style column layout (Discord / Stoat / Teams / Matrix / demo).
                // SpacesRooms and Custom both fall through here: the Matrix
                // plugin previously declared Custom for an experimental
                // spaces-tree that took over the whole column and broke the
                // multi-column shell. Routing both to ChannelListLayout
                // restores the standard AccountServerBar + ChannelList split.
                SidebarLayoutKind::ChannelList
                | SidebarLayoutKind::SpacesRooms
                | SidebarLayoutKind::Custom => rsx! { ChannelListLayout {} },
                SidebarLayoutKind::Communities => rsx! { CommunitiesLayout {} },
                SidebarLayoutKind::Feed => rsx! { FeedLayout {} },
                SidebarLayoutKind::SortModes => rsx! { SortModesLayout { decl: decl.clone() } },
                // RepoTree backends (GitHub, Forgejo) expose a tree of all
                // repos at the account level. Once the user drills into a
                // specific repo server, the sidebar must show only that
                // repo's channels (Issues / PRs / Discussions), not the
                // full repo list. ChannelListLayout reads from the selected
                // server's channels — exactly what we want inside a repo.
                SidebarLayoutKind::RepoTree if has_selected_server => {
                    rsx! { ChannelListLayout {} }
                }
                SidebarLayoutKind::RepoTree => rsx! { RepoTreeLayout {} },
            }
        }
    }
}

/// P28 — pure helper: compute the new value of
/// `AppState::sidebar_invalidated_tick` after receipt of a
/// `ClientEvent::SidebarInvalidated` event. Extracted so unit tests can
/// pin the dependency wiring (tick increment → `use_resource` re-run)
/// without spinning up a Dioxus virtual DOM.
pub(crate) fn bump_sidebar_tick(current: u32) -> u32 {
    current.wrapping_add(1)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::state::AppState;

    #[test]
    fn bump_sidebar_tick_increments() {
        assert_eq!(bump_sidebar_tick(0), 1);
        assert_eq!(bump_sidebar_tick(5), 6);
    }

    #[test]
    fn bump_sidebar_tick_wraps_on_overflow() {
        // Ensures the wrapping add doesn't panic in debug builds when the
        // tick counter rolls over after u32::MAX events — extremely rare
        // in practice but part of the contract.
        assert_eq!(bump_sidebar_tick(u32::MAX), 0);
    }

    #[test]
    fn app_state_default_has_zero_sidebar_tick() {
        let s = AppState::default();
        assert_eq!(s.sidebar_invalidated_tick, 0);
    }

    #[test]
    fn sidebar_tick_dependency_reads_latest_value() {
        // Simulates the dependency wiring: if the tick is captured into
        // the `use_resource` closure, each increment produces a distinct
        // dep value → use_resource re-runs. Model that with a Vec that
        // records the tick observed on each "fetch".
        let mut s = AppState::default();
        let tick0 = s.sidebar_invalidated_tick;
        let _fetch_a = tick0; // first observation
        s.sidebar_invalidated_tick = bump_sidebar_tick(s.sidebar_invalidated_tick);
        let tick1 = s.sidebar_invalidated_tick;
        assert_ne!(
            tick0, tick1,
            "tick increment must change the observed value so use_resource re-runs"
        );
    }
}
