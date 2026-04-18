//! Plugin-declared sidebar dispatcher.
//!
//! The `ClientSidebar` component reads the currently active account's
//! [`SidebarDeclaration`] (via `ClientBackend::get_sidebar_declaration`) and
//! dispatches to one of six layout sub-components:
//!
//! - [`ChannelListLayout`] — Discord / Stoat / Teams / poly-native / demo.
//!   A thin wrapper around the existing
//!   [`crate::ui::account::common::ChannelList`] so backends that declare
//!   `SidebarLayoutKind::ChannelList` keep their existing UI verbatim.
//! - [`SpacesRoomsLayout`] — Matrix-style spaces + rooms (WP 4 follow-up
//!   skeleton; currently renders the plugin's servers as a flat list).
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
pub mod spaces_rooms;

pub use channel_list_layout::ChannelListLayout;
pub use communities::CommunitiesLayout;
pub use custom::CustomSidebar;
pub use feed::FeedLayout;
pub use repo_tree::RepoTreeLayout;
pub use spaces_rooms::SpacesRoomsLayout;

use crate::client_manager::ClientManager;
use crate::state::AppState;
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
    let app_state: Signal<AppState> = use_context();
    let client_manager: Signal<ClientManager> = use_context();

    // Resolve the current account. `active_account_id` is populated by the
    // router's `on_update`. When we're on a DM / friends / app-level route
    // it may be None — in that case we render the ChannelListLayout which
    // itself delegates to the existing `ChannelList` that handles the
    // DMs-home empty state.
    let account_id = app_state.read().nav.active_account_id.clone();

    let decl_res = {
        let account_id = account_id.clone();
        use_resource(move || {
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
                let guard = backend.read().await;
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
            rsx! {
                aside { class: "client-sidebar client-sidebar-error",
                    // Fall back to the stock channel list so the user still
                    // has navigation even when the plugin errors.
                    ChannelListLayout {}
                }
            }
        }
        Some(Ok(decl)) => {
            let decl = decl.clone();
            match decl.layout {
                SidebarLayoutKind::ChannelList => rsx! { ChannelListLayout {} },
                SidebarLayoutKind::SpacesRooms => rsx! { SpacesRoomsLayout {} },
                SidebarLayoutKind::Communities => rsx! { CommunitiesLayout {} },
                SidebarLayoutKind::Feed => rsx! { FeedLayout {} },
                SidebarLayoutKind::RepoTree => rsx! { RepoTreeLayout {} },
                SidebarLayoutKind::Custom => rsx! { CustomSidebar { declaration: decl } },
            }
        }
    }
}
