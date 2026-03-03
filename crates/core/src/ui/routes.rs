//! URL-based routing for Poly — multi-account, multi-backend URL structure.
//!
//! Every account-scoped view encodes two pieces of identity in its URL:
//! - `:backend` — one of `demo | stoat | matrix | discord | teams`
//! - `:account_id` — the account key used in `ClientManager`
//!
//! This lets Poly deep-link into any account and express per-backend visual
//! variations. App-level views (`/notifications`, `/settings`) are not scoped
//! to any account.
//!
//! # URL scheme
//! ```text
//! /                                                   → root redirect
//! /:backend/:account_id/dms                           → DM home
//! /:backend/:account_id/dms/:channel_id               → DM conversation
//! /:backend/:account_id/friends                       → Friends list
//! /:backend/:account_id/channels/:server_id           → Server home
//! /:backend/:account_id/channels/:server_id/:channel_id → Server channel
//! /notifications                                      → Aggregated feed
//! /settings                                           → App settings
//! ```
//!
//! # AppState bridge
//! `on_update` syncs the current route into `AppState.nav` *before* any
//! component re-renders so components reading from AppState continue to work.
//!
//! # Demo account
//! The demo account always uses backend=`demo`, account_id=`demo`.
// DECISION(DX-ROUTER-2): Multi-account routing replaces Discord-style single-account URLs.
// Backend slug + account_id in URL enables per-backend rendering and deep linking.

use super::account::{
    AccountBar, AccountSettingsPage, AccountSwitcher, ChannelList, ChatView, FriendsPanel,
    NotificationsView, UserSidebar, VoiceBar, VoiceChannelView,
};
use super::main_layout::MainLayout;
use super::settings::SettingsPage;
use crate::i18n::t;
use crate::state::{AppState, ChatData, View};
use dioxus::prelude::*;
use poly_client::{BackendType, ChannelType};

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
            #[route("/:backend/:account_id/dms")]
            DmsHome { backend: String, account_id: String },

            #[route("/:backend/:account_id/dms/:channel_id")]
            DmChat { backend: String, account_id: String, channel_id: String },
        #[end_layout]

        // ── Account-scoped: Server channels ─────────────────────────
        #[layout(ServerLayout)]
            #[route("/:backend/:account_id/channels/:server_id/:channel_id")]
            ServerChat {
                backend: String,
                account_id: String,
                server_id: String,
                channel_id: String,
            },

            #[route("/:backend/:account_id/channels/:server_id")]
            ServerHome {
                backend: String,
                account_id: String,
                server_id: String,
            },
        #[end_layout]

        // ── Account-scoped: Friends ──────────────────────────────────
        #[route("/:backend/:account_id/friends")]
        FriendsRoute { backend: String, account_id: String },

        // ── App-level (not account-scoped) ───────────────────────────
        #[route("/notifications")]
        NotificationsRoute,

        #[route("/settings")]
        SettingsRoute,

        // ── Account-scoped settings ──────────────────────────────────
        #[route("/:backend/:account_id/settings")]
        AccountSettingsRoute { backend: String, account_id: String },

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
/// `nav.active_backend`, and writes `:account_id` to `nav.active_account_id`.
///
/// Called from [`RouterConfig::on_update`] *before* dependent components
/// re-render.
pub fn sync_route_to_app_state(route: &Route, mut app_state: Signal<AppState>) {
    let mut s = app_state.write();
    match route {
        Route::DmsHome {
            backend,
            account_id,
        } => {
            s.nav.view = View::DmsFriends;
            s.nav.active_backend = BackendType::from_slug(backend);
            s.nav.active_account_id = Some(account_id.clone());
            s.nav.selected_server = None;
            s.nav.selected_channel = None;
        }
        Route::DmChat {
            backend,
            account_id,
            channel_id,
        } => {
            s.nav.view = View::DmsFriends;
            s.nav.active_backend = BackendType::from_slug(backend);
            s.nav.active_account_id = Some(account_id.clone());
            s.nav.selected_server = None;
            s.nav.selected_channel = Some(channel_id.clone());
        }
        Route::ServerHome {
            backend,
            account_id,
            server_id,
        } => {
            s.nav.view = View::Server;
            s.nav.active_backend = BackendType::from_slug(backend);
            s.nav.active_account_id = Some(account_id.clone());
            s.nav.selected_server = Some(server_id.clone());
            // Don't clear selected_channel — load_server_data sets it
        }
        Route::ServerChat {
            backend,
            account_id,
            server_id,
            channel_id,
        } => {
            s.nav.view = View::Server;
            s.nav.active_backend = BackendType::from_slug(backend);
            s.nav.active_account_id = Some(account_id.clone());
            s.nav.selected_server = Some(server_id.clone());
            s.nav.selected_channel = Some(channel_id.clone());
        }
        Route::FriendsRoute {
            backend,
            account_id,
        } => {
            s.nav.view = View::Friends;
            s.nav.active_backend = BackendType::from_slug(backend);
            s.nav.active_account_id = Some(account_id.clone());
        }
        Route::NotificationsRoute => {
            s.nav.view = View::Notifications;
            // App-level — don't change active_account_id / active_backend
        }
        Route::SettingsRoute => {
            s.nav.view = View::Settings;
            // App-level — clear account context so Bar 2 hides and no server stays "open"
            s.nav.active_account_id = None;
            s.nav.active_backend = None;
            s.nav.selected_server = None;
            s.nav.selected_channel = None;
        }
        Route::AccountSettingsRoute {
            backend,
            account_id,
        } => {
            s.nav.view = View::Settings;
            s.nav.active_backend = BackendType::from_slug(backend);
            s.nav.active_account_id = Some(account_id.clone());
            s.nav.selected_server = None;
            s.nav.selected_channel = None;
        }
        Route::Root | Route::PageNotFound { .. } => {
            // on_update will redirect — nothing to sync here
        }
    }
}

// ── Layout: DMs ─────────────────────────────────────────────────────────────

/// Layout wrapper for DM views — provides the channel list panel.
///
/// Persists ChannelList state (search filter, scroll position) across
/// DmsHome ↔ DmChat navigation since the layout stays mounted.
#[component]
fn DmsLayout() -> Element {
    rsx! {
        div { class: "channel-list-wrapper",
            ChannelList {}
            VoiceBar {}
            AccountSwitcher {}
        }
        Outlet::<Route> {}
    }
}

// ── Layout: Server ──────────────────────────────────────────────────────────

/// Layout wrapper for server views — channel list + optional user sidebar.
///
/// Reads `selected_server` from AppState (set by `on_update` before render).
/// Persists ChannelList across channel-switching within the same server.
#[component]
fn ServerLayout() -> Element {
    let app_state: Signal<AppState> = use_context();
    let chat_data: Signal<ChatData> = use_context();

    let show_right = app_state.read().nav.right_sidebar_visible;
    let is_voice_channel = chat_data
        .read()
        .current_channel
        .as_ref()
        .is_some_and(|ch| matches!(ch.channel_type, ChannelType::Voice | ChannelType::Video));

    rsx! {
        div { class: "channel-list-wrapper",
            ChannelList {}
            VoiceBar {}
            AccountBar {}
        }
        Outlet::<Route> {}
        if show_right && !is_voice_channel {
            UserSidebar {}
        }
    }
}

// ── Route pages ─────────────────────────────────────────────────────────────

/// DM home — placeholder when no conversation is selected.
#[component]
fn DmsHome(backend: String, account_id: String) -> Element {
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
#[component]
fn DmChat(backend: String, account_id: String, channel_id: String) -> Element {
    rsx! {
        ChatView {}
    }
}

/// Server home — auto-selects first channel, renders chat / voice view.
#[component]
fn ServerHome(backend: String, account_id: String, server_id: String) -> Element {
    let chat_data: Signal<ChatData> = use_context();

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
#[component]
fn ServerChat(
    backend: String,
    account_id: String,
    server_id: String,
    channel_id: String,
) -> Element {
    let chat_data: Signal<ChatData> = use_context();

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
fn FriendsRoute(backend: String, account_id: String) -> Element {
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
#[component]
fn AccountSettingsRoute(backend: String, account_id: String) -> Element {
    rsx! {
        AccountSettingsPage { backend, account_id }
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
                account_id: "demo".to_string(),
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
