//! URL-based routing for Poly — Discord-style URL structure.
//!
//! Uses Dioxus Router for browser-history–aware navigation:
//! - `/channels/@me` — DMs/Friends list
//! - `/channels/@me/:channel_id` — DM conversation
//! - `/channels/:server_id` — Server view (auto-selects first channel)
//! - `/channels/:server_id/:channel_id` — Server channel view
//! - `/friends` — Friends browser
//! - `/notifications` — Notifications feed
//! - `/settings` — Settings page
//!
//! On web the router integrates with browser history (back/forward buttons
//! work out of the box). On desktop it uses memory-based history with our
//! custom NavBar buttons calling `navigator().go_back()`/`go_forward()`.
//!
//! # AppState bridge
//! `on_update` callback syncs the current route into `AppState.nav` *before*
//! any component re-renders, so existing components that read from AppState
//! (ChannelList, ServerSidebar, …) continue to work unchanged.
// DECISION(DX-ROUTER-1): Dioxus Router replaces manual View enum matching.
// Browser back/forward works on web; desktop uses navigator() API.

use super::account_bar::AccountBar;
use super::account_switcher::AccountSwitcher;
use super::channel_list::ChannelList;
use super::chat_view::ChatView;
use super::friends_panel::FriendsPanel;
use super::main_layout::MainLayout;
use super::notifications::NotificationsView;
use super::settings::SettingsPage;
use super::user_sidebar::UserSidebar;
use super::voice_bar::VoiceBar;
use super::voice_view::VoiceChannelView;
use crate::i18n::t;
use crate::state::{AppState, ChatData, View};
use dioxus::prelude::*;
use poly_client::ChannelType;

// ── Route enum ──────────────────────────────────────────────────────────────

/// Application routes — Discord-style URL structure.
///
/// The Dioxus Router manages browser history (web) or memory-based history
/// (desktop), enabling back/forward navigation across all platforms.
#[derive(Routable, Clone, PartialEq, Debug)]
#[rustfmt::skip]
pub enum Route {
    #[layout(MainLayout)]
        // Root path — memory history starts here on desktop; immediately replaces
        // with DmsHome so there is no flash and no back-stack entry added.
        #[route("/")]
        Root,

        // ── DM views (shared ChannelList via DmsLayout) ─────────────
        #[layout(DmsLayout)]
            #[route("/channels/@me")]
            DmsHome,

            #[route("/channels/@me/:channel_id")]
            DmChat { channel_id: String },
        #[end_layout]

        // ── Server views (shared ChannelList via ServerLayout) ──────
        #[layout(ServerLayout)]
            #[route("/channels/:server_id/:channel_id")]
            ServerChat { server_id: String, channel_id: String },

            #[route("/channels/:server_id")]
            ServerHome { server_id: String },
        #[end_layout]

        // ── Standalone views ────────────────────────────────────────
        #[route("/friends")]
        FriendsRoute,

        #[route("/notifications")]
        NotificationsRoute,

        #[route("/settings")]
        SettingsRoute,
    #[end_layout]

    // Catch-all → redirected to DmsHome by on_update callback
    #[route("/:..segments")]
    PageNotFound { segments: Vec<String> },
}

// ── Route → AppState sync ───────────────────────────────────────────────────

/// Synchronize the current route into [`AppState::nav`] so that existing
/// components (ChannelList, ServerSidebar, etc.) that read from AppState
/// continue to work.
///
/// Called from [`RouterConfig::on_update`] *before* dependent components
/// re-render.
pub fn sync_route_to_app_state(route: &Route, mut app_state: Signal<AppState>) {
    let mut s = app_state.write();
    match route {
        Route::DmsHome => {
            s.nav.view = View::DmsFriends;
            s.nav.selected_server = None;
            s.nav.selected_channel = None;
        }
        Route::DmChat { channel_id } => {
            s.nav.view = View::DmsFriends;
            s.nav.selected_server = None;
            s.nav.selected_channel = Some(channel_id.clone());
        }
        Route::ServerHome { server_id } => {
            s.nav.view = View::Server;
            s.nav.selected_server = Some(server_id.clone());
            // Don't clear selected_channel — load_server_data sets it
        }
        Route::ServerChat {
            server_id,
            channel_id,
        } => {
            s.nav.view = View::Server;
            s.nav.selected_server = Some(server_id.clone());
            s.nav.selected_channel = Some(channel_id.clone());
        }
        Route::FriendsRoute => {
            s.nav.view = View::Friends;
        }
        Route::NotificationsRoute => {
            s.nav.view = View::Notifications;
        }
        Route::SettingsRoute => {
            s.nav.view = View::Settings;
        }
        Route::Root => {
            // Will be replaced by DmsHome immediately
            s.nav.view = View::DmsFriends;
            s.nav.selected_server = None;
            s.nav.selected_channel = None;
        }
        Route::PageNotFound { .. } => {
            // Will be redirected to DmsHome
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
fn DmsHome() -> Element {
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
fn DmChat(channel_id: String) -> Element {
    rsx! {
        ChatView {}
    }
}

/// Server home — auto-selects first channel, renders chat / voice view.
#[component]
fn ServerHome(server_id: String) -> Element {
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
fn ServerChat(server_id: String, channel_id: String) -> Element {
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
fn FriendsRoute() -> Element {
    rsx! {
        FriendsPanel {}
    }
}

/// Notifications feed.
#[component]
fn NotificationsRoute() -> Element {
    rsx! {
        NotificationsView {}
    }
}

/// Settings page.
#[component]
fn SettingsRoute() -> Element {
    rsx! {
        SettingsPage {}
    }
}

/// Root redirect — desktop memory history starts at "/"; on_update intercepts
/// this before the component renders and replaces with DmsHome.
/// This component body is never seen by the user.
#[component]
fn Root() -> Element {
    rsx! {}
}

/// Catch-all 404 — rendered briefly before on_update redirects to DmsHome.
#[component]
fn PageNotFound(segments: Vec<String>) -> Element {
    rsx! {}
}
