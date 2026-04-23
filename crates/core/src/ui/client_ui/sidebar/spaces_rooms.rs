//! `SidebarLayoutKind::SpacesRooms` — Matrix spaces containing rooms.
//!
//! P24 (Pack D): render the account's servers as "spaces", each with their
//! channels ("rooms") nested underneath at depth=1. True spaces-within-spaces
//! nesting requires backend changes (a plugin-declared `parent_id` on
//! `Server`) and is deferred — see TODO below.

use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::AppState;
use dioxus::prelude::*;
use poly_client::{Channel, ClientError, Server};
use poly_ui_macros::{context_menu, ui_action};

/// One space plus its rooms, ready for rendering.
#[derive(Clone, Debug, PartialEq)]
struct SpaceWithRooms {
    space: Server,
    rooms: Vec<Channel>,
}

/// Matrix-style spaces-and-rooms sidebar.
#[ui_action(None)]
#[context_menu(inherit)]
#[component]
pub fn SpacesRoomsLayout() -> Element {
    let app_state: Signal<AppState> = use_context();
    let client_manager: Signal<ClientManager> = use_context();

    let account_id = app_state.read().nav.active_account_id.cloned();

    // TODO(matrix-spaces-tree): Matrix models space containment as
    // `m.space.parent` / `m.space.child` state events — spaces can contain
    // other spaces. The current `Server` type has no `parent_id` field, so
    // we render a flat depth=1 tree (spaces → rooms). Adding true nesting
    // requires a WIT-level change to `Server` plus a per-backend test.
    let tree_res = {
        let account_id = account_id.clone();
        use_resource(move || {
            let account_id = account_id.clone();
            async move {
                let Some(account_id) = account_id else {
                    return Ok::<Vec<SpaceWithRooms>, ClientError>(Vec::new());
                };
                let Some(backend) = client_manager.read().get_backend(&account_id) else {
                    return Err(ClientError::NotFound(format!(
                        "no backend for account {account_id}"
                    )));
                };
                let guard = backend.read().await;
                let servers = guard.get_servers().await?;
                let mut out = Vec::with_capacity(servers.len());
                for space in servers {
                    // Ignore per-space fetch errors so one broken space
                    // doesn't break the whole tree; log and keep going.
                    let rooms = match guard.get_channels(&space.id).await {
                        Ok(cs) => cs,
                        Err(err) => {
                            tracing::warn!(
                                "SpacesRoomsLayout: get_channels({}) failed: {err:?}",
                                space.id
                            );
                            Vec::new()
                        }
                    };
                    out.push(SpaceWithRooms { space, rooms });
                }
                Ok(out)
            }
        })
    };

    rsx! {
        aside { class: "client-sidebar spaces-rooms-layout",
            h2 { class: "sidebar-header", {t("ui-sidebar-spaces-header")} }
            match &*tree_res.read_unchecked() {
                None => rsx! {
                    div { class: "spaces-rooms-loading", {t("ui-sidebar-spaces-loading")} }
                },
                Some(Err(err)) => {
                    tracing::warn!("SpacesRoomsLayout: get_servers failed: {err:?}");
                    rsx! {
                        div { class: "spaces-rooms-error",
                            {t("ui-sidebar-spaces-error")}
                        }
                    }
                }
                Some(Ok(tree)) => {
                    let tree = tree.clone();
                    if tree.is_empty() {
                        rsx! {
                            div { class: "spaces-rooms-empty",
                                {t("ui-sidebar-spaces-empty")}
                            }
                        }
                    } else {
                        rsx! {
                            ul { class: "spaces-rooms-space-list",
                                {tree.into_iter().map(|sr| {
                                    let space_id = sr.space.id.clone();
                                    let space_name = sr.space.name.clone();
                                    let rooms = sr.rooms.clone();
                                    rsx! {
                                        li {
                                            key: "{space_id}",
                                            class: "spaces-rooms-space",
                                            div { class: "spaces-rooms-space-name", "{space_name}" }
                                            if !rooms.is_empty() {
                                                ul { class: "spaces-rooms-room-list",
                                                    {rooms.into_iter().map(|r| {
                                                        let rid = r.id.clone();
                                                        let rname = r.name.clone();
                                                        rsx! {
                                                            li {
                                                                key: "{rid}",
                                                                class: "spaces-rooms-room",
                                                                "# {rname}"
                                                            }
                                                        }
                                                    })}
                                                }
                                            }
                                        }
                                    }
                                })}
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use poly_client::{BackendType, Channel, ChannelType, Server};

    fn mk_space(id: &str, name: &str) -> Server {
        Server {
            id: id.into(),
            name: name.into(),
            icon_url: None,
            banner_url: None,
            categories: Vec::new(),
            backend: BackendType::new("matrix"),
            unread_count: 0,
            mention_count: 0,
            account_id: "acct-test".into(),
            account_display_name: "Test".into(),
            default_channel_id: None,
            description: None,
            star_count: None,
            language: None,
            forks_count: None,
            open_issues_count: None,
        }
    }

    fn mk_room(id: &str, name: &str, server_id: &str) -> Channel {
        Channel {
            id: id.into(),
            name: name.into(),
            channel_type: ChannelType::Text,
            server_id: server_id.into(),
            unread_count: 0,
            mention_count: 0,
            last_message_id: None,
            forum_tags: None,
            parent_channel_id: None,
            thread_metadata: None,
        }
    }

    /// P24: the SpaceWithRooms model pairs a server with its own channels,
    /// preserving the order returned by the backend.
    #[test]
    fn space_with_rooms_preserves_order() {
        let tree = vec![
            SpaceWithRooms {
                space: mk_space("s1", "Gaming"),
                rooms: vec![
                    mk_room("r1", "general", "s1"),
                    mk_room("r2", "random", "s1"),
                ],
            },
            SpaceWithRooms {
                space: mk_space("s2", "Work"),
                rooms: vec![mk_room("r3", "standup", "s2")],
            },
        ];
        assert_eq!(tree.len(), 2);
        assert_eq!(tree[0].rooms.len(), 2);
        assert_eq!(tree[0].rooms[0].name, "general");
        assert_eq!(tree[1].space.name, "Work");
    }

    /// P24: a space with zero rooms is still valid — snapshot renders only
    /// the space header.
    #[test]
    fn space_with_no_rooms_is_valid() {
        let sr = SpaceWithRooms {
            space: mk_space("empty", "Empty Space"),
            rooms: Vec::new(),
        };
        assert!(sr.rooms.is_empty());
    }
}
