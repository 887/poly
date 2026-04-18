//! `SidebarLayoutKind::SpacesRooms` — Matrix spaces containing rooms.
//!
//! Full tree-nested rendering is a WP 4 follow-up. For practical minimum we
//! render the account's existing servers/rooms as a flat list so Matrix
//! users see navigable content immediately.

use crate::client_manager::ClientManager;
use crate::state::AppState;
use crate::ui::account::common::ChannelList;
use dioxus::prelude::*;
use poly_client::{ClientError, Server};
use poly_ui_macros::{context_menu, ui_action};

/// Matrix-style spaces-and-rooms sidebar (skeleton).
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub fn SpacesRoomsLayout() -> Element {
    let app_state: Signal<AppState> = use_context();
    let client_manager: Signal<ClientManager> = use_context();

    let account_id = app_state.read().nav.active_account_id.clone();

    let servers_res = {
        let account_id = account_id.clone();
        use_resource(move || {
            let account_id = account_id.clone();
            async move {
                let Some(account_id) = account_id else {
                    return Ok::<Vec<Server>, ClientError>(Vec::new());
                };
                let Some(backend) = client_manager.read().get_backend(&account_id) else {
                    return Err(ClientError::NotFound(format!(
                        "no backend for account {account_id}"
                    )));
                };
                let guard = backend.read().await;
                guard.get_servers().await
            }
        })
    };

    rsx! {
        aside { class: "client-sidebar spaces-rooms-layout",
            div { class: "sidebar-placeholder-note",
                "Spaces and Rooms — Matrix-style sidebar (WP 4 follow-up)"
            }
            // Until tree nesting lands, fall back to the existing ChannelList
            // so Matrix users still see channels.
            ChannelList {}
            // Also show a flat list of spaces so the backend's declaration is
            // visible in snapshots.
            match &*servers_res.read_unchecked() {
                None => rsx! {
                    div { class: "spaces-rooms-loading", "Loading spaces…" }
                },
                Some(Err(err)) => {
                    tracing::warn!("SpacesRoomsLayout: get_servers failed: {err:?}");
                    rsx! {
                        div { class: "spaces-rooms-error",
                            "Failed to load spaces"
                        }
                    }
                }
                Some(Ok(servers)) => {
                    let servers = servers.clone();
                    rsx! {
                        ul { class: "spaces-rooms-space-list",
                            {servers.into_iter().map(|s| rsx! {
                                li {
                                    key: "{s.id}",
                                    class: "spaces-rooms-space",
                                    "{s.name}"
                                }
                            })}
                        }
                    }
                }
            }
        }
    }
}
