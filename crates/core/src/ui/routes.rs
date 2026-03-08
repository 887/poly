//! URL-based routing for Poly — multi-account, multi-backend, multi-instance URL structure.
//!
//! Every account-scoped view encodes three pieces of identity in its URL:
//! - `:backend`     — one of `demo | stoat | matrix | discord | teams`
//! - `:instance_id` — the federated homeserver / instance (e.g. `demo`, `matrix.org`, `my.poly.server`)
//! - `:account_id`  — the account key used in `ClientManager` (unique within that instance)
//!
//! This lets Poly deep-link into any account on any federated instance and express
//! per-backend visual variations. App-level views (`/notifications`, `/settings`)
//! are not scoped to any account.
//!
//! # URL scheme
//! ```text
//! /                                                                                 → root redirect
//! /:backend/:instance_id/:account_id/dms                                           → DM home
//! /:backend/:instance_id/:account_id/dms/:dm_id                                    → DM conversation
//! /:backend/:instance_id/:account_id/friends                                       → Friends list
//! /:backend/:instance_id/:account_id/channels/:server_id                           → Server home
//! /:backend/:instance_id/:account_id/channels/:server_id/:channel_id              → Server channel
//! /notifications                                                                    → Aggregated feed
//! /settings                                                                         → App settings
//! /:backend/:instance_id/:account_id/settings                                      → Account settings
//! /:backend/:instance_id/:account_id/servers/:server_id/settings                  → Server settings
//! ```
//!
//! # URL example (demo accounts)
//! ```text
//! /demo/demo/demo-cat/dms                     → cat account DM home
//! /demo/demo/demo-dog/channels/s789/ch456     → dog account server channel
//! ```
//!
//! # URL example (federated real accounts)
//! ```text
//! /matrix/matrix.org/alice/channels/!room:matrix.org/!msg:matrix.org
//! /matrix/my.homeserver.org/bob/dms/!dm:my.homeserver.org
//! /stoat/stoat.chat/carol/channels/sid123/cid456
//! ```
//!
//! # AppState bridge
//! `on_update` syncs the current route into `AppState.nav` *before* any
//! component re-renders so components reading from AppState continue to work.
//!
//! # Demo accounts
//! Cat demo: backend=`demo`, instance=`demo`, account=`demo-cat`
//! Dog demo: backend=`demo`, instance=`demo`, account=`demo-dog`
// DECISION(DX-ROUTER-2): Multi-account routing replaces Discord-style single-account URLs.
// DECISION(DX-ROUTER-3): Added instance_id segment for federated multi-homeserver support.

use super::account::{
    AccountBar, AccountSettingsPage, ChannelList, ChatView, FriendsPanel, NotificationsView,
    ServerSettingsPage, VoiceBar, VoiceChannelView,
};
use super::main_layout::MainLayout;
use super::settings::SettingsPage;
use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::{AppState, ChatData, View};
use crate::ui::account::common::chat_history::initial_message_query;
use crate::ui::account::common::chat_history::request_restore_scroll_position_or_bottom;
use dioxus::prelude::*;
use poly_client::{BackendType, Channel, ChannelType};

/// Return the account id encoded by an account-scoped route, if any.
pub fn route_account_id(route: &Route) -> Option<&str> {
    match route {
        Route::DmsHome { account_id, .. }
        | Route::DmChat { account_id, .. }
        | Route::ServerHome { account_id, .. }
        | Route::ServerChat { account_id, .. }
        | Route::ServerSettingsRoute { account_id, .. }
        | Route::FriendsRoute { account_id, .. }
        | Route::AccountSettingsRoute { account_id, .. } => Some(account_id.as_str()),
        Route::Root
        | Route::SettingsRoute
        | Route::NotificationsRoute
        | Route::PageNotFound { .. } => None,
    }
}

/// Whether the current route targets an account that is not currently active.
pub fn route_targets_unknown_account(route: &Route, client_manager: &ClientManager) -> bool {
    let Some(account_id) = route_account_id(route) else {
        return false;
    };

    !client_manager
        .active_account_ids()
        .into_iter()
        .any(|id| id == account_id)
}

// ── Route enum ──────────────────────────────────────────────────────────────

/// Application routes — multi-account, multi-backend URL structure.
///
/// Account-scoped routes carry `:backend` and `:account_id` URL segments so
/// that:
/// - Any URL can be deep-linked to the correct account
/// - Backend-specific UI rendering can be keyed on `:backend`
/// - Browser back/forward work correctly across account switches
///
/// See module-level docs for the full URL scheme.
#[derive(Routable, Clone, PartialEq, Debug)]
#[rustfmt::skip]
pub enum Route {
    #[layout(MainLayout)]

        // Root redirect — memory history starts here on desktop; on_update
        // replaces immediately with the best active account DMs route.
        #[route("/")]
        Root,

        // ── Account-scoped: DMs ─────────────────────────────────────
        #[layout(DmsLayout)]
            #[route("/:backend/:instance_id/:account_id/dms")]
            DmsHome { backend: String, instance_id: String, account_id: String },

            #[route("/:backend/:instance_id/:account_id/dms/:dm_id")]
            DmChat { backend: String, instance_id: String, account_id: String, dm_id: String },
        #[end_layout]

        // ── Account-scoped: Server channels ─────────────────────────
        #[layout(ServerLayout)]
            #[route("/:backend/:instance_id/:account_id/channels/:server_id/:channel_id")]
            ServerChat {
                backend: String,
                instance_id: String,
                account_id: String,
                server_id: String,
                channel_id: String,
            },

            #[route("/:backend/:instance_id/:account_id/channels/:server_id")]
            ServerHome {
                backend: String,
                instance_id: String,
                account_id: String,
                server_id: String,
            },
        #[end_layout]

        // ── Account-scoped: Friends ──────────────────────────────────
        #[route("/:backend/:instance_id/:account_id/friends")]
        FriendsRoute { backend: String, instance_id: String, account_id: String },

        // ── App-level (not account-scoped) ───────────────────────────
        #[route("/notifications")]
        NotificationsRoute,

        #[route("/settings")]
        SettingsRoute,

        // ── Account-scoped settings ──────────────────────────────────
        #[route("/:backend/:instance_id/:account_id/settings")]
        AccountSettingsRoute { backend: String, instance_id: String, account_id: String },

        // ── Account-scoped: Server settings ─────────────────────────
        #[route("/:backend/:instance_id/:account_id/servers/:server_id/settings")]
        ServerSettingsRoute {
            backend: String,
            instance_id: String,
            account_id: String,
            server_id: String,
        },

    #[end_layout]

    // Catch-all → redirected by on_update to the best active route
    #[route("/:..segments")]
    PageNotFound { segments: Vec<String> },
}

// ── Route → AppState sync ───────────────────────────────────────────────────

/// Synchronize the current route into [`AppState::nav`] so existing components
/// (ChannelList, FavoritesBar, …) that read AppState continue to work.
///
/// Also extracts the `:backend` slug into [`BackendType`] and writes it to
/// `nav.active_backend`, the `:instance_id` to `nav.active_instance_id`, and
/// `:account_id` to `nav.active_account_id`.
///
/// Called from [`RouterConfig::on_update`] *before* dependent components
/// re-render.
pub fn sync_route_to_app_state(route: &Route, mut app_state: Signal<AppState>) {
    // Compute the URL string before borrowing app_state mutably.
    // Routable derives Display, so format!("{route}") gives the URL path.
    let route_url = format!("{route}");
    let mut s = app_state.write();
    match route {
        Route::DmsHome {
            backend,
            instance_id,
            account_id,
        } => {
            s.nav.view = View::DmsFriends;
            s.nav.active_backend = BackendType::from_slug(backend);
            s.nav.active_instance_id = Some(instance_id.clone());
            s.nav.active_account_id = Some(account_id.clone());
            s.nav.selected_server = None;
            s.nav.selected_channel = None;
            s.nav
                .account_last_routes
                .insert(account_id.clone(), route_url);
        }
        Route::DmChat {
            backend,
            instance_id,
            account_id,
            dm_id,
        } => {
            s.nav.view = View::DmsFriends;
            s.nav.active_backend = BackendType::from_slug(backend);
            s.nav.active_instance_id = Some(instance_id.clone());
            s.nav.active_account_id = Some(account_id.clone());
            s.nav.selected_server = None;
            s.nav.selected_channel = Some(dm_id.clone());
            s.nav
                .account_last_routes
                .insert(account_id.clone(), route_url);
        }
        Route::ServerHome {
            backend,
            instance_id,
            account_id,
            server_id,
        } => {
            s.nav.view = View::Server;
            s.nav.active_backend = BackendType::from_slug(backend);
            s.nav.active_instance_id = Some(instance_id.clone());
            s.nav.active_account_id = Some(account_id.clone());
            s.nav.selected_server = Some(server_id.clone());
            // Don't clear selected_channel — load_server_data sets it
            s.nav
                .account_last_routes
                .insert(account_id.clone(), route_url);
        }
        Route::ServerChat {
            backend,
            instance_id,
            account_id,
            server_id,
            channel_id,
        } => {
            s.nav.view = View::Server;
            s.nav.active_backend = BackendType::from_slug(backend);
            s.nav.active_instance_id = Some(instance_id.clone());
            s.nav.active_account_id = Some(account_id.clone());
            s.nav.selected_server = Some(server_id.clone());
            s.nav.selected_channel = Some(channel_id.clone());
            s.nav
                .account_last_routes
                .insert(account_id.clone(), route_url);
        }
        Route::FriendsRoute {
            backend,
            instance_id,
            account_id,
        } => {
            s.nav.view = View::Friends;
            s.nav.active_backend = BackendType::from_slug(backend);
            s.nav.active_instance_id = Some(instance_id.clone());
            s.nav.active_account_id = Some(account_id.clone());
            s.nav
                .account_last_routes
                .insert(account_id.clone(), route_url);
        }
        Route::NotificationsRoute => {
            s.nav.view = View::Notifications;
            // App-level — don't change active_account_id / active_backend
        }
        Route::SettingsRoute => {
            s.nav.view = View::Settings;
            // App-level — clear account context so Bar 2 hides and no server stays "open"
            s.nav.active_account_id = None;
            s.nav.active_instance_id = None;
            s.nav.active_backend = None;
            s.nav.selected_server = None;
            s.nav.selected_channel = None;
        }
        Route::AccountSettingsRoute {
            backend,
            instance_id,
            account_id,
        } => {
            s.nav.view = View::Settings;
            s.nav.active_backend = BackendType::from_slug(backend);
            s.nav.active_instance_id = Some(instance_id.clone());
            s.nav.active_account_id = Some(account_id.clone());
            s.nav.selected_server = None;
            s.nav.selected_channel = None;
            s.nav
                .account_last_routes
                .insert(account_id.clone(), route_url);
        }
        Route::ServerSettingsRoute {
            backend,
            instance_id,
            account_id,
            server_id,
        } => {
            s.nav.view = View::Settings;
            s.nav.active_backend = BackendType::from_slug(backend);
            s.nav.active_instance_id = Some(instance_id.clone());
            s.nav.active_account_id = Some(account_id.clone());
            s.nav.selected_server = Some(server_id.clone());
            s.nav.selected_channel = None;
            s.nav
                .account_last_routes
                .insert(account_id.clone(), route_url);
        }
        Route::Root | Route::PageNotFound { .. } => {
            // on_update will redirect — nothing to sync here
        }
    }
}

fn restore_dm_chat(
    dm_id: String,
    account_id: String,
    mut chat_data: Signal<ChatData>,
    client_manager: Signal<ClientManager>,
) {
    let already_set = chat_data
        .read()
        .current_channel
        .as_ref()
        .is_some_and(|ch| ch.id == dm_id);
    if already_set {
        return;
    }

    let channel = {
        let data = chat_data.read();
        data.dm_channels
            .iter()
            .find(|dm| dm.id == dm_id && dm.account_id == account_id)
            .map(|dm| Channel {
                id: dm.id.clone(),
                name: dm.user.display_name.clone(),
                channel_type: ChannelType::Text,
                server_id: String::new(),
                unread_count: dm.unread_count,
                last_message_id: None,
            })
            .or_else(|| {
                data.groups
                    .iter()
                    .find(|g| g.id == dm_id && g.account_id == account_id)
                    .map(|g| {
                        let name = g.name.clone().unwrap_or_else(|| {
                            g.members
                                .iter()
                                .map(|m| m.display_name.clone())
                                .collect::<Vec<_>>()
                                .join(", ")
                        });
                        Channel {
                            id: g.id.clone(),
                            name,
                            channel_type: ChannelType::Text,
                            server_id: String::new(),
                            unread_count: 0,
                            last_message_id: None,
                        }
                    })
            })
    };

    if let Some(ch) = channel {
        chat_data.write().current_channel = Some(ch);
        chat_data.write().current_server = None;
    }

    spawn(async move {
        chat_data.write().loading = true;
        chat_data.write().messages = Vec::new();
        chat_data.write().members = Vec::new();

        let unread_count = chat_data
            .read()
            .current_channel
            .as_ref()
            .filter(|channel| channel.id == dm_id)
            .map_or(0, |channel| channel.unread_count);

        let backend_arc = client_manager.read().get_backend(&account_id);
        let Some(backend_arc) = backend_arc else {
            chat_data.write().loading = false;
            return;
        };

        let guard = backend_arc.read().await;
        if let Ok(messages) = guard
            .get_messages(&dm_id, initial_message_query(unread_count))
            .await
        {
            chat_data.write().messages = messages;
            request_restore_scroll_position_or_bottom(&dm_id);
        }
        if let Ok(members) = guard.get_channel_members(&dm_id).await {
            chat_data.write().members = members;
        }
        chat_data.write().loading = false;
    });
}

// ── Layout: DMs ─────────────────────────────────────────────────────────────

/// Layout wrapper for DM views — provides the channel list panel.
///
/// Persists ChannelList state (search filter, scroll position) across
/// DmsHome ↔ DmChat navigation since the layout stays mounted.
///
/// VoiceBar and AccountBar share a `voice-account-footer` wrapper that inherits
/// the same `margin-left: -72px` trick as the old account-bar standalone, so
/// both panels extend to cover the favourites sidebar column.
// DECISION(V-1): VoiceBar + AccountBar share voice-account-footer for correct alignment.
#[component]
fn DmsLayout() -> Element {
    rsx! {
        div { class: "channel-list-wrapper",
            ChannelList {}
            div { class: "voice-account-footer",
                VoiceBar {}
                AccountBar {}
            }
        }
        Outlet::<Route> {}
    }
}

// ── Layout: Server ──────────────────────────────────────────────────────────

/// Layout wrapper for server views — channel list + optional user sidebar.
///
/// Reads `selected_server` from AppState (set by `on_update` before render).
/// Persists ChannelList across channel-switching within the same server.
///
/// VoiceBar and AccountBar share a `voice-account-footer` wrapper that inherits
/// the same `margin-left: -72px` trick as the old account-bar standalone, so
/// both panels extend to cover the favourites sidebar column.
// DECISION(V-1): VoiceBar + AccountBar share voice-account-footer for correct alignment.
#[component]
fn ServerLayout() -> Element {
    rsx! {
        div { class: "channel-list-wrapper",
            ChannelList {}
            div { class: "voice-account-footer",
                VoiceBar {}
                AccountBar {}
            }
        }
        Outlet::<Route> {}
    }
}

// ── Route pages ─────────────────────────────────────────────────────────────

/// DM home — placeholder when no conversation is selected.
#[component]
fn DmsHome(backend: String, instance_id: String, account_id: String) -> Element {
    rsx! {
        main { class: "chat-view",
            div { class: "chat-header",
                span { class: "chat-channel-name", "{t(\"nav-dms\")}" }
            }
            div { class: "message-list",
                div { class: "message-empty",
                    div { class: "empty-wave", "💬" }
                    h3 { "{t(\"chat-select-conversation\")}" }
                }
            }
            div { class: "message-input-area",
                div { class: "message-input-disabled", "{t(\"chat-select-conversation\")}" }
            }
        }
    }
}

/// DM chat — renders a conversation with an individual or group.
///
/// Handles both click navigation (DMChannelItem sets up data before routing)
/// and URL-restore navigation (account switch, page reload) by loading data
/// in a `use_effect` when `current_channel` doesn't already match `dm_id`.
#[component]
fn DmChat(backend: String, instance_id: String, account_id: String, dm_id: String) -> Element {
    let chat_data: Signal<ChatData> = use_context();
    let client_manager: Signal<ClientManager> = use_context();

    use_effect(move || {
        restore_dm_chat(dm_id.clone(), account_id.clone(), chat_data, client_manager);
    });

    rsx! {
        ChatView {}
    }
}

/// Server home — auto-selects first channel, renders chat / voice view.
///
/// On URL-restore navigation (F5, deep link) the click handler that normally
/// calls `load_server_data` never ran, so data is missing. The `use_effect`
/// here detects that case and loads the server data before rendering.
#[component]
fn ServerHome(
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
) -> Element {
    let chat_data: Signal<ChatData> = use_context();
    let app_state: Signal<AppState> = use_context();
    let client_manager: Signal<ClientManager> = use_context();

    // URL-restore: server data is absent after a hard reload. Load it now.
    use_effect(move || {
        let sid = server_id.clone();
        let server_already_loaded = chat_data
            .read()
            .current_server
            .as_ref()
            .is_some_and(|s| s.id == sid);
        if server_already_loaded {
            return;
        }
        spawn(async move {
            super::favorites_sidebar::load_server_data(sid, app_state, client_manager, chat_data)
                .await;
        });
    });

    let is_voice_channel = chat_data
        .read()
        .current_channel
        .as_ref()
        .is_some_and(|ch| matches!(ch.channel_type, ChannelType::Voice | ChannelType::Video));

    rsx! {
        if is_voice_channel {
            VoiceChannelView {}
        } else {
            ChatView {}
        }
    }
}

/// Server channel — specific channel view within a server.
///
/// On URL-restore navigation (F5, deep link), the click handlers that
/// normally set up `chat_data` never ran. The `use_effect` here detects
/// missing data and calls `restore_server_channel` to reload it, preserving
/// the exact channel from the URL rather than defaulting to the first one.
#[component]
fn ServerChat(
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
    channel_id: String,
) -> Element {
    let chat_data: Signal<ChatData> = use_context();
    let app_state: Signal<AppState> = use_context();
    let client_manager: Signal<ClientManager> = use_context();
    use_effect(move || {
        let cid = channel_id.clone();
        let sid = server_id.clone();

        // Skip if click-navigation already prepared this specific server +
        // channel. An empty text channel is still a valid loaded state, so we
        // must not use `messages.is_empty()` as the readiness check here.
        let snapshot = chat_data.read();
        let already_loaded = snapshot
            .current_server
            .as_ref()
            .is_some_and(|server| server.id == sid)
            && snapshot
                .current_channel
                .as_ref()
                .is_some_and(|ch| ch.id == cid && ch.server_id == sid)
            && snapshot.channels.iter().any(|ch| ch.id == cid);
        if already_loaded {
            return;
        }

        spawn(async move {
            super::favorites_sidebar::restore_server_channel(
                sid,
                cid,
                app_state,
                client_manager,
                chat_data,
            )
            .await;
        });
    });

    let is_voice_channel = chat_data
        .read()
        .current_channel
        .as_ref()
        .is_some_and(|ch| matches!(ch.channel_type, ChannelType::Voice | ChannelType::Video));

    rsx! {
        if is_voice_channel {
            VoiceChannelView {}
        } else {
            ChatView {}
        }
    }
}

/// Friends browser — tiled grid view.
#[component]
fn FriendsRoute(backend: String, instance_id: String, account_id: String) -> Element {
    rsx! {
        FriendsPanel {}
    }
}

/// Notifications feed — aggregated across all accounts.
#[component]
fn NotificationsRoute() -> Element {
    rsx! {
        NotificationsView {}
    }
}

/// Settings page — app-level, not account-scoped.
#[component]
fn SettingsRoute() -> Element {
    rsx! {
        SettingsPage {}
    }
}

/// Account settings — scoped to a specific backend account.
///
/// Passes the account context to AccountSettingsPage so it shows only
/// account-relevant settings (notifications). Global settings (theme,
/// identity, backup) remain in the app-level SettingsRoute.
///
/// AccountSettingsPage renders its own channel-list-wrapper (with settings nav
/// + AccountBar) and settings-content sibling, matching the normal layout.
#[component]
fn AccountSettingsRoute(backend: String, instance_id: String, account_id: String) -> Element {
    rsx! {
        AccountSettingsPage { backend, account_id }
    }
}

/// Server settings — notifications, profile, and general for a specific server.
///
/// Routes to the server-scoped settings page which provides notification levels,
/// per-server profile (nickname/avatar), and general options including leave server.
///
/// ServerSettingsPage renders its own channel-list-wrapper (with settings nav
/// + AccountBar) and settings-content sibling, matching the normal layout.
#[component]
fn ServerSettingsRoute(
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
) -> Element {
    rsx! {
        ServerSettingsPage {
            backend,
            instance_id,
            account_id,
            server_id,
        }
    }
}

/// Root redirect — desktop memory history starts at "/".
///
/// Uses `use_effect` to navigate away on mount since the `on_update`
/// callback may not process its redirect return value on the very first
/// render in Dioxus memory-history mode.
#[component]
fn Root() -> Element {
    let client_manager: Signal<crate::client_manager::ClientManager> = use_context();
    use_effect(move || {
        let cm = client_manager.read();
        if cm.demo_active {
            navigator().replace(Route::DmsHome {
                backend: "demo".to_string(),
                instance_id: "demo".to_string(),
                account_id: "demo-cat".to_string(),
            });
        } else {
            navigator().replace(Route::SettingsRoute);
        }
    });
    rsx! {}
}

/// Catch-all 404 — redirected by on_update before being seen.
#[component]
fn PageNotFound(segments: Vec<String>) -> Element {
    rsx! {}
}
