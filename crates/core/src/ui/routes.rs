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

use super::account::common::direct_call::{
    DirectCallRequest, start_direct_call_from_active_account,
};
use super::account::{
    AccountSettingsPage, ChannelSettingsPage, ChatView, ConversationSearchView, DiscordForumView,
    ForumView, ForumPostView, FriendsPanel, NewConversationView, NotificationsView,
    OutgoingDirectCallOverlay, SavedItemsView, ServerSettingsPage, VoiceChannelView,
};
use super::client_ui::ClientSidebar;
use super::create_forum_post::{CreateForumPostPage, ForumSearchPage};
use super::main_layout::MainLayout;
use super::server_overview::ServerOverviewPage;
use super::agent::AgentPage;
use super::settings::SettingsPage;
use super::split_shell::SplitMenuShell;
use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::{AppState, ChatData, SettingsSection, View};
use crate::ui::account::common::VoiceAccountFooter;
use crate::ui::account::common::{FeatureUnsupportedPlaceholder, UnsupportedFeature};
use crate::ui::account::common::chat_history::initial_message_query;
use crate::ui::account::common::chat_history::request_restore_scroll_position_or_bottom;
use dioxus::prelude::*;
use poly_client::{BackendType, Channel, ChannelType};
use poly_ui_macros::{context_menu, ui_action};

/// Return the account id encoded by an account-scoped route, if any.
pub fn route_account_id(route: &Route) -> Option<&str> {
    match route {
        Route::DmsHome { account_id, .. }
        | Route::ConversationSearchRoute { account_id, .. }
        | Route::NewConversationRoute { account_id, .. }
        | Route::DmChat { account_id, .. }
        | Route::DmPendingCall { account_id, .. }
        | Route::DmPendingVideoCall { account_id, .. }
        | Route::DmPendingAddCall { account_id, .. }
        | Route::DmPendingAddVideoCall { account_id, .. }
        | Route::DmMediaViewerRoute { account_id, .. }
        | Route::ServerMediaViewerRoute { account_id, .. }
        | Route::ServerHome { account_id, .. }
        | Route::ServerChat { account_id, .. }
        | Route::ForumPostRoute { account_id, .. }
        | Route::CreateForumPostRoute { account_id, .. }
        | Route::ForumSearchRoute { account_id, .. }
        | Route::ForumCommentsRoute { account_id, .. }
        | Route::ServerSettingsRoute { account_id, .. }
        | Route::ServerSettingsSectionRoute { account_id, .. }
        | Route::ChannelSettingsRoute { account_id, .. }
        | Route::CreateChannelRoute { account_id, .. }
        | Route::FriendsRoute { account_id, .. }
        | Route::NotificationsRoute { account_id, .. }
        | Route::SavedItemsRoute { account_id, .. }
        | Route::AccountSettingsRoute { account_id, .. }
        | Route::CreateServerRoute { account_id, .. }
        | Route::AccountSearchRoute { account_id, .. }
        | Route::ServerOverviewRoute { account_id, .. } => Some(account_id.as_str()),
        // Reauth intentionally returns None so route_targets_unknown_account
        // never treats a reauth URL as pointing at a missing account — the
        // point of reauth is to fix an account whose backend may be gone.
        Route::ReauthAccount { .. } => None,
        Route::Root
        | Route::SettingsRoute
        | Route::SettingsSectionRoute { .. }
        | Route::AgentRoute
        | Route::AgentSectionRoute { .. }
        | Route::SearchRoute
        | Route::SignupPicker
        | Route::ClientSignup { .. }
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
#[derive(Routable, Clone, PartialEq, Debug, poly_ui_macros::Connected)]
#[rustfmt::skip]
pub enum Route {
    #[layout(MainLayout)]

        // Root redirect — memory history starts here on desktop; on_update
        // replaces immediately with the best active account DMs route.
        #[connected(entry_point)]
        #[route("/")]
        Root,

        // ── Account-scoped: DMs ─────────────────────────────────────
        #[layout(DmsLayout)]
            #[connected(linked)]
            #[route("/:backend/:instance_id/:account_id/dms")]
            DmsHome { backend: String, instance_id: String, account_id: String },

            // Reached programmatically: the DMs channel list and conversation
            // search view navigate here via navigator().push. No Link callsite
            // exists yet (the search button is rendered inside DMFriendsView).
            #[connected(linked, programmatic<ConversationSearchRouteProducer>)]
            #[route("/:backend/:instance_id/:account_id/dms/search")]
            ConversationSearchRoute { backend: String, instance_id: String, account_id: String },

            #[connected(linked)]
            #[route("/:backend/:instance_id/:account_id/dms/new")]
            NewConversationRoute { backend: String, instance_id: String, account_id: String },

            #[connected(linked)]
            #[route("/:backend/:instance_id/:account_id/dms/:dm_id")]
            DmChat { backend: String, instance_id: String, account_id: String, dm_id: String },

            #[connected(linked)]
            #[route("/:backend/:instance_id/:account_id/dms/:dm_id/call")]
            DmPendingCall { backend: String, instance_id: String, account_id: String, dm_id: String },

            #[connected(linked)]
            #[route("/:backend/:instance_id/:account_id/dms/:dm_id/video-call")]
            DmPendingVideoCall { backend: String, instance_id: String, account_id: String, dm_id: String },

            #[connected(linked)]
            #[route("/:backend/:instance_id/:account_id/dms/:dm_id/call/add")]
            DmPendingAddCall { backend: String, instance_id: String, account_id: String, dm_id: String },

            #[connected(linked)]
            #[route("/:backend/:instance_id/:account_id/dms/:dm_id/video-call/add")]
            DmPendingAddVideoCall { backend: String, instance_id: String, account_id: String, dm_id: String },

            #[connected(linked)]
            #[route("/:backend/:instance_id/:account_id/dms/:dm_id/media/:message_id/:attachment_index")]
            DmMediaViewerRoute {
                backend: String,
                instance_id: String,
                account_id: String,
                dm_id: String,
                message_id: String,
                attachment_index: usize,
            },
        #[end_layout]

        // ── Account-scoped: Server channels ─────────────────────────
        #[layout(ServerLayout)]
            // ── Create Channel (more specific — must come BEFORE ServerChat) ──
            // CreateChannelRoute uses a literal "/create-channel" suffix, while
            // ServerChat uses a wildcard "/:channel_id". If ServerChat were
            // listed first, Dioxus would match "/create-channel" as
            // channel_id = "create-channel" and try to load that as a real
            // channel, causing a crash. Always keep literal-suffix routes above
            // wildcard-segment routes of the same depth.
            #[connected(linked)]
            #[route("/:backend/:instance_id/:account_id/channels/:server_id/create-channel")]
            CreateChannelRoute {
                backend: String,
                instance_id: String,
                account_id: String,
                server_id: String,
            },

            #[connected(linked)]
            #[route("/:backend/:instance_id/:account_id/channels/:server_id/:channel_id")]
            ServerChat {
                backend: String,
                instance_id: String,
                account_id: String,
                server_id: String,
                channel_id: String,
            },

            #[connected(linked)]
            #[route("/:backend/:instance_id/:account_id/channels/:server_id/:channel_id/media/:message_id/:attachment_index")]
            ServerMediaViewerRoute {
                backend: String,
                instance_id: String,
                account_id: String,
                server_id: String,
                channel_id: String,
                message_id: String,
                attachment_index: usize,
            },

            // ── Forum post thread (deeper than ServerChat — listed after) ──
            #[connected(linked)]
            #[route("/:backend/:instance_id/:account_id/channels/:server_id/:channel_id/posts/:post_id")]
            ForumPostRoute {
                backend: String,
                instance_id: String,
                account_id: String,
                server_id: String,
                channel_id: String,
                post_id: String,
            },

            // ── Create forum post (literal "create-post" segment — listed after) ──
            #[connected(linked)]
            #[route("/:backend/:instance_id/:account_id/channels/:server_id/:channel_id/create-post")]
            CreateForumPostRoute {
                backend: String,
                instance_id: String,
                account_id: String,
                server_id: String,
                channel_id: String,
            },

            // ── Forum search ──
            // Reached programmatically: the forum sidebar navigates here to open
            // the community search page. No Link callsite exists yet.
            #[connected(linked, programmatic<ForumSearchRouteProducer>)]
            #[route("/:backend/:instance_id/:account_id/channels/:server_id/:channel_id/search")]
            ForumSearchRoute {
                backend: String,
                instance_id: String,
                account_id: String,
                server_id: String,
                channel_id: String,
            },

            // ── Forum comments feed ──
            #[connected(linked)]
            #[route("/:backend/:instance_id/:account_id/channels/:server_id/:channel_id/comments")]
            ForumCommentsRoute {
                backend: String,
                instance_id: String,
                account_id: String,
                server_id: String,
                channel_id: String,
            },

            #[connected(linked)]
            #[route("/:backend/:instance_id/:account_id/channels/:server_id")]
            ServerHome {
                backend: String,
                instance_id: String,
                account_id: String,
                server_id: String,
            },
        #[end_layout]

        // ── Account-scoped: Friends ──────────────────────────────────
        #[connected(linked)]
        #[route("/:backend/:instance_id/:account_id/friends")]
        FriendsRoute { backend: String, instance_id: String, account_id: String },

        #[connected(linked)]
        #[route("/:backend/:instance_id/:account_id/notifications")]
        NotificationsRoute { backend: String, instance_id: String, account_id: String },

        #[connected(linked)]
        #[route("/:backend/:instance_id/:account_id/saved")]
        SavedItemsRoute { backend: String, instance_id: String, account_id: String },

        // ── Account-scoped: Server/repo overview (forge backends) ────
        #[connected(linked)]
        #[route("/:backend/:instance_id/:account_id/overview")]
        ServerOverviewRoute { backend: String, instance_id: String, account_id: String },

        // ── App-level (not account-scoped) ───────────────────────────
        #[connected(linked)]
        #[route("/settings")]
        SettingsRoute,

        #[connected(linked)]
        #[route("/settings/:section")]
        SettingsSectionRoute { section: String },

        #[connected(linked)]
        #[route("/agent")]
        AgentRoute,

        #[connected(linked)]
        #[route("/agent/:section")]
        AgentSectionRoute { section: String },

        #[connected(linked)]
        #[route("/search")]
        SearchRoute,

        /// Account-scoped search — shows the global search page but pre-filters to one account.
        #[connected(linked)]
        #[route("/:backend/:instance_id/:account_id/search")]
        AccountSearchRoute { backend: String, instance_id: String, account_id: String },

        // ── Account-scoped settings ──────────────────────────────────
        #[connected(linked)]
        #[route("/:backend/:instance_id/:account_id/settings")]
        AccountSettingsRoute { backend: String, instance_id: String, account_id: String },

        // ── Account-scoped: Create server (full-page form) ───────────
        #[connected(linked)]
        #[route("/:backend/:instance_id/:account_id/create-server")]
        CreateServerRoute { backend: String, instance_id: String, account_id: String },

        // ── Account-scoped: Server settings ─────────────────────────
        #[connected(linked)]
        #[route("/:backend/:instance_id/:account_id/servers/:server_id/settings")]
        ServerSettingsRoute {
            backend: String,
            instance_id: String,
            account_id: String,
            server_id: String,
        },

        // Reached programmatically: ServerSettingsPage uses history.replaceState
        // to update the URL when scrolling between sections (see
        // crates/core/src/ui/account/server/settings/mod.rs). No typed
        // navigator().push callsite exists; the route is entered via the URL.
        #[connected(linked, programmatic<ServerSettingsSectionRouteProducer>)]
        #[route("/:backend/:instance_id/:account_id/servers/:server_id/settings/:section")]
        ServerSettingsSectionRoute {
            backend: String,
            instance_id: String,
            account_id: String,
            server_id: String,
            section: String,
        },

        // ── Account-scoped: Channel settings (Pack C.3 / P19) ───────
        // Must come after ServerChat's route position *in the router's match
        // order*; but because the path has a literal "/settings" suffix after
        // :channel_id, it is strictly more specific than "/:channel_id" and
        // wins the route match regardless of declaration order.
        #[connected(linked)]
        #[route("/:backend/:instance_id/:account_id/channels/:server_id/:channel_id/settings")]
        ChannelSettingsRoute {
            backend: String,
            instance_id: String,
            account_id: String,
            server_id: String,
            channel_id: String,
        },

    #[end_layout]

    // ── Signup flow (full-page, no sidebar) ─────────────────────────────────
    // These routes render without MainLayout, giving a clean signup experience.
    #[connected(linked)]
    #[route("/signup")]
    SignupPicker,

    #[connected(linked)]
    #[route("/signup/:client")]
    ClientSignup { client: String },

    // Per-account reauth page — rendered full-page outside MainLayout.
    // Reused for 401/unauthenticated accounts: updates the existing account's
    // token in place (or removes the account) rather than creating a new one.
    #[connected(linked)]
    #[route("/:backend/:instance_id/:account_id/reauth")]
    ReauthAccount { backend: String, instance_id: String, account_id: String },

    // Catch-all → redirected by on_update to the best active route
    #[connected(linked)]
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
    #[cfg(debug_assertions)]
    record_route_visit(route);

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
            s.nav.active_backend = Some(BackendType::from_slug(backend));
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
            s.nav.active_backend = Some(BackendType::from_slug(backend));
            s.nav.active_instance_id = Some(instance_id.clone());
            s.nav.active_account_id = Some(account_id.clone());
            s.nav.selected_server = None;
            s.nav.selected_channel = Some(dm_id.clone());
            s.nav
                .account_last_routes
                .insert(account_id.clone(), route_url);
            s.nav
                .account_last_dm_routes
                .insert(account_id.clone(), format!("{route}"));
        }
        Route::DmPendingCall {
            backend,
            instance_id,
            account_id,
            dm_id,
        }
        | Route::DmPendingVideoCall {
            backend,
            instance_id,
            account_id,
            dm_id,
        }
        | Route::DmPendingAddCall {
            backend,
            instance_id,
            account_id,
            dm_id,
        }
        | Route::DmPendingAddVideoCall {
            backend,
            instance_id,
            account_id,
            dm_id,
        } => {
            s.nav.view = View::DmsFriends;
            s.nav.active_backend = Some(BackendType::from_slug(backend));
            s.nav.active_instance_id = Some(instance_id.clone());
            s.nav.active_account_id = Some(account_id.clone());
            s.nav.selected_server = None;
            s.nav.selected_channel = Some(dm_id.clone());
            let dm_route = format!(
                "{}",
                Route::DmChat {
                    backend: backend.clone(),
                    instance_id: instance_id.clone(),
                    account_id: account_id.clone(),
                    dm_id: dm_id.clone(),
                }
            );
            s.nav
                .account_last_routes
                .insert(account_id.clone(), dm_route.clone());
            s.nav
                .account_last_dm_routes
                .insert(account_id.clone(), dm_route);
        }
        Route::DmMediaViewerRoute {
            backend,
            instance_id,
            account_id,
            dm_id,
            ..
        } => {
            s.nav.view = View::DmsFriends;
            s.nav.active_backend = Some(BackendType::from_slug(backend));
            s.nav.active_instance_id = Some(instance_id.clone());
            s.nav.active_account_id = Some(account_id.clone());
            s.nav.selected_server = None;
            s.nav.selected_channel = Some(dm_id.clone());
            s.nav
                .account_last_routes
                .insert(account_id.clone(), route_url);
        }
        Route::ServerMediaViewerRoute {
            backend,
            instance_id,
            account_id,
            server_id,
            channel_id,
            ..
        } => {
            s.nav.view = View::Server;
            s.nav.active_backend = Some(BackendType::from_slug(backend));
            s.nav.active_instance_id = Some(instance_id.clone());
            s.nav.active_account_id = Some(account_id.clone());
            s.nav.selected_server = Some(server_id.clone());
            s.nav.selected_channel = Some(channel_id.clone());
            s.nav
                .account_last_routes
                .insert(account_id.clone(), route_url);
        }
        Route::NewConversationRoute {
            backend,
            instance_id,
            account_id,
        } => {
            s.nav.view = View::DmsFriends;
            s.nav.active_backend = Some(BackendType::from_slug(backend));
            s.nav.active_instance_id = Some(instance_id.clone());
            s.nav.active_account_id = Some(account_id.clone());
            s.nav.selected_server = None;
            s.nav.selected_channel = None;
        }
        Route::ConversationSearchRoute {
            backend,
            instance_id,
            account_id,
        } => {
            s.nav.view = View::DmsFriends;
            s.nav.active_backend = Some(BackendType::from_slug(backend));
            s.nav.active_instance_id = Some(instance_id.clone());
            s.nav.active_account_id = Some(account_id.clone());
            s.nav.selected_server = None;
            s.nav.selected_channel = None;
        }
        Route::ServerHome {
            backend,
            instance_id,
            account_id,
            server_id,
        } => {
            s.nav.view = View::Server;
            s.nav.active_backend = Some(BackendType::from_slug(backend));
            s.nav.active_instance_id = Some(instance_id.clone());
            s.nav.active_account_id = Some(account_id.clone());
            s.nav.selected_server = Some(server_id.clone());
            s.nav.selected_channel = None;
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
            s.nav.active_backend = Some(BackendType::from_slug(backend));
            s.nav.active_instance_id = Some(instance_id.clone());
            s.nav.active_account_id = Some(account_id.clone());
            s.nav.selected_server = Some(server_id.clone());
            s.nav.selected_channel = Some(channel_id.clone());
            s.nav
                .account_last_routes
                .insert(account_id.clone(), route_url);
        }
        Route::ForumPostRoute {
            backend,
            instance_id,
            account_id,
            server_id,
            channel_id,
            post_id: _,
        } => {
            // Keep selected_channel = parent forum channel so sidebar stays highlighted.
            s.nav.view = View::Server;
            s.nav.active_backend = Some(BackendType::from_slug(backend));
            s.nav.active_instance_id = Some(instance_id.clone());
            s.nav.active_account_id = Some(account_id.clone());
            s.nav.selected_server = Some(server_id.clone());
            s.nav.selected_channel = Some(channel_id.clone());
            s.nav
                .account_last_routes
                .insert(account_id.clone(), route_url);
        }
        Route::CreateForumPostRoute {
            backend,
            instance_id,
            account_id,
            server_id,
            channel_id,
        } => {
            s.nav.view = View::Server;
            s.nav.active_backend = Some(BackendType::from_slug(backend));
            s.nav.active_instance_id = Some(instance_id.clone());
            s.nav.active_account_id = Some(account_id.clone());
            s.nav.selected_server = Some(server_id.clone());
            s.nav.selected_channel = Some(channel_id.clone());
            // Do NOT record in account_last_routes — create-post is transient
        }
        Route::ForumSearchRoute {
            backend,
            instance_id,
            account_id,
            server_id,
            channel_id,
        } => {
            s.nav.view = View::Server;
            s.nav.active_backend = Some(BackendType::from_slug(backend));
            s.nav.active_instance_id = Some(instance_id.clone());
            s.nav.active_account_id = Some(account_id.clone());
            s.nav.selected_server = Some(server_id.clone());
            s.nav.selected_channel = Some(channel_id.clone());
            // Do NOT record in account_last_routes — search is transient
        }
        Route::ForumCommentsRoute {
            backend,
            instance_id,
            account_id,
            server_id,
            channel_id,
        } => {
            s.nav.view = View::Server;
            s.nav.active_backend = Some(BackendType::from_slug(backend));
            s.nav.active_instance_id = Some(instance_id.clone());
            s.nav.active_account_id = Some(account_id.clone());
            s.nav.selected_server = Some(server_id.clone());
            s.nav.selected_channel = Some(channel_id.clone());
            s.nav.account_last_routes.insert(account_id.clone(), route_url);
        }
        Route::FriendsRoute {
            backend,
            instance_id,
            account_id,
        } => {
            s.nav.view = View::Friends;
            s.nav.active_backend = Some(BackendType::from_slug(backend));
            s.nav.active_instance_id = Some(instance_id.clone());
            s.nav.active_account_id = Some(account_id.clone());
            s.nav
                .account_last_routes
                .insert(account_id.clone(), route_url);
        }
        Route::NotificationsRoute {
            backend,
            instance_id,
            account_id,
        } => {
            s.nav.view = View::Notifications;
            s.nav.active_backend = Some(BackendType::from_slug(backend));
            s.nav.active_instance_id = Some(instance_id.clone());
            s.nav.active_account_id = Some(account_id.clone());
            s.nav.selected_server = None;
            s.nav.selected_channel = None;
            s.nav
                .account_last_routes
                .insert(account_id.clone(), route_url);
        }
        Route::SavedItemsRoute {
            backend,
            instance_id,
            account_id,
        } => {
            s.nav.view = View::DmsFriends;
            s.nav.active_backend = Some(BackendType::from_slug(backend));
            s.nav.active_instance_id = Some(instance_id.clone());
            s.nav.active_account_id = Some(account_id.clone());
            s.nav.selected_server = None;
            s.nav.selected_channel = None;
            s.nav
                .account_last_routes
                .insert(account_id.clone(), route_url);
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
        Route::SettingsSectionRoute { section } => {
            s.nav.view = View::Settings;
            s.settings_section = SettingsSection::from_slug(section);
            s.nav.active_account_id = None;
            s.nav.active_instance_id = None;
            s.nav.active_backend = None;
            s.nav.selected_server = None;
            s.nav.selected_channel = None;
        }
        Route::AgentRoute => {
            s.nav.view = View::Agent;
            s.nav.active_account_id = None;
            s.nav.active_instance_id = None;
            s.nav.active_backend = None;
            s.nav.selected_server = None;
            s.nav.selected_channel = None;
        }
        Route::AgentSectionRoute { .. } => {
            s.nav.view = View::Agent;
            s.nav.active_account_id = None;
            s.nav.active_instance_id = None;
            s.nav.active_backend = None;
            s.nav.selected_server = None;
            s.nav.selected_channel = None;
        }
        Route::SearchRoute => {
            s.nav.view = View::Search;
            // App-level — clear account context so Bar 2 hides
            s.nav.active_account_id = None;
            s.nav.active_instance_id = None;
            s.nav.active_backend = None;
            s.nav.selected_server = None;
            s.nav.selected_channel = None;
        }
        Route::AccountSearchRoute {
            backend,
            instance_id,
            account_id,
        } => {
            // Account-scoped search — keep account context so Bar 2 stays visible
            // and `SearchPage` can pre-filter to this account.
            s.nav.view = View::Search;
            s.nav.active_backend = Some(BackendType::from_slug(backend));
            s.nav.active_instance_id = Some(instance_id.clone());
            s.nav.active_account_id = Some(account_id.clone());
            s.nav.selected_server = None;
            s.nav.selected_channel = None;
        }
        Route::AccountSettingsRoute {
            backend,
            instance_id,
            account_id,
        } => {
            s.nav.view = View::Settings;
            s.nav.active_backend = Some(BackendType::from_slug(backend));
            s.nav.active_instance_id = Some(instance_id.clone());
            s.nav.active_account_id = Some(account_id.clone());
            s.nav.selected_server = None;
            s.nav.selected_channel = None;
            s.nav
                .account_last_routes
                .insert(account_id.clone(), route_url);
        }
        Route::CreateServerRoute {
            backend,
            instance_id,
            account_id,
        } => {
            s.nav.view = View::Settings; // Reuse Settings view — hides channel list
            s.nav.active_backend = Some(BackendType::from_slug(backend));
            s.nav.active_instance_id = Some(instance_id.clone());
            s.nav.active_account_id = Some(account_id.clone());
            s.nav.selected_server = None;
            s.nav.selected_channel = None;
            // Do NOT record in account_last_routes — create-server is transient
        }
        Route::CreateChannelRoute {
            backend,
            instance_id,
            account_id,
            server_id,
        } => {
            // Keep the ServerLayout visible (selected_server stays set so
            // ChannelList renders) but clear any selected channel so the
            // Outlet renders CreateChannelPage instead of a chat view.
            s.nav.view = View::Server;
            s.nav.active_backend = Some(BackendType::from_slug(backend));
            s.nav.active_instance_id = Some(instance_id.clone());
            s.nav.active_account_id = Some(account_id.clone());
            s.nav.selected_server = Some(server_id.clone());
            s.nav.selected_channel = None;
            // Do NOT record in account_last_routes — create-channel is transient
        }
        Route::ServerSettingsRoute {
            backend,
            instance_id,
            account_id,
            server_id,
        } => {
            s.nav.view = View::Settings;
            s.nav.active_backend = Some(BackendType::from_slug(backend));
            s.nav.active_instance_id = Some(instance_id.clone());
            s.nav.active_account_id = Some(account_id.clone());
            s.nav.selected_server = Some(server_id.clone());
            s.nav.selected_channel = None;
            s.nav
                .account_last_routes
                .insert(account_id.clone(), route_url);
        }
        Route::ServerSettingsSectionRoute {
            backend,
            instance_id,
            account_id,
            server_id,
            ..
        } => {
            s.nav.view = View::Settings;
            s.nav.active_backend = Some(BackendType::from_slug(backend));
            s.nav.active_instance_id = Some(instance_id.clone());
            s.nav.active_account_id = Some(account_id.clone());
            s.nav.selected_server = Some(server_id.clone());
            s.nav.selected_channel = None;
            s.nav
                .account_last_routes
                .insert(account_id.clone(), route_url);
        }
        Route::ChannelSettingsRoute {
            backend,
            instance_id,
            account_id,
            server_id,
            channel_id,
        } => {
            s.nav.view = View::Settings;
            s.nav.active_backend = Some(BackendType::from_slug(backend));
            s.nav.active_instance_id = Some(instance_id.clone());
            s.nav.active_account_id = Some(account_id.clone());
            s.nav.selected_server = Some(server_id.clone());
            s.nav.selected_channel = Some(channel_id.clone());
            s.nav
                .account_last_routes
                .insert(account_id.clone(), route_url);
        }
        Route::ServerOverviewRoute {
            backend,
            instance_id,
            account_id,
        } => {
            s.nav.view = View::DmsFriends; // reuse the top-level view
            s.nav.active_backend = Some(BackendType::from_slug(backend));
            s.nav.active_instance_id = Some(instance_id.clone());
            s.nav.active_account_id = Some(account_id.clone());
            s.nav.selected_server = None;
            s.nav.selected_channel = None;
            s.nav
                .account_last_routes
                .insert(account_id.clone(), route_url);
        }
        Route::Root | Route::PageNotFound { .. } => {
            // on_update will redirect — nothing to sync here
        }
        Route::SignupPicker | Route::ClientSignup { .. } => {
            // Signup routes are outside MainLayout — clear account context.
            s.nav.view = View::Signup;
            s.nav.active_account_id = None;
            s.nav.active_instance_id = None;
            s.nav.active_backend = None;
            s.nav.selected_server = None;
            s.nav.selected_channel = None;
        }
        Route::ReauthAccount { backend, instance_id, account_id } => {
            // Reauth is a full-page form like signup, but scoped to an
            // existing account — keep the account context so the page can
            // look it up in ClientManager / ChatData.
            s.nav.view = View::Signup;
            s.nav.active_backend = Some(BackendType::from_slug(backend));
            s.nav.active_instance_id = Some(instance_id.clone());
            s.nav.active_account_id = Some(account_id.clone());
            s.nav.selected_server = None;
            s.nav.selected_channel = None;
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
                mention_count: 0,
                last_message_id: None,
                forum_tags: None,
                parent_channel_id: None,
                thread_metadata: None,
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
                            mention_count: 0,
                            last_message_id: None,
                            forum_tags: None,
                            parent_channel_id: None,
                            thread_metadata: None,
                        }
                    })
            })
    };

    if let Some(ch) = channel {
        // Single write guard — batching current_channel + current_server into
        // one guard so Dioxus schedules one re-render, not two.
        let mut w = chat_data.write();
        w.current_channel = Some(ch);
        w.current_server = None;
    }

    spawn(async move {
        // Single write guard for the three reset fields — one re-render.
        {
            let mut w = chat_data.write();
            w.loading = true;
            w.messages = Vec::new();
            w.members = Vec::new();
        }

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

        // Acquire the backend read lock with a 5 s deadline so a stalled
        // writer cannot hang the UI indefinitely.
        let guard = match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            backend_arc.read(),
        )
        .await
        {
            Ok(g) => g,
            Err(_) => {
                tracing::warn!(
                    dm_id = %dm_id,
                    account_id = %account_id,
                    "restore_dm_chat: backend lock acquire timed out after 5 s"
                );
                chat_data.write().loading = false;
                return;
            }
        };
        let messages = guard
            .get_messages(&dm_id, initial_message_query(unread_count))
            .await
            .ok();
        let members = guard.get_channel_members(&dm_id).await.ok();
        drop(guard);

        // Single write guard for the final state update.
        let mut w = chat_data.write();
        if let Some(msgs) = messages {
            w.messages = msgs;
            request_restore_scroll_position_or_bottom(&dm_id);
        }
        if let Some(mbrs) = members {
            w.members = mbrs;
        }
        w.loading = false;
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
#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn DmsLayout() -> Element {
    rsx! {
        SplitMenuShell {
            root_class: "account-view-main".to_string(),
            sidebar_class: "channel-list-wrapper".to_string(),
            content_class: String::new(),
            sidebar: rsx! {
                ClientSidebar {}
                VoiceAccountFooter {}
            },
            content: rsx! {
                Outlet::<Route> {}
            },
        }
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
#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn ServerLayout() -> Element {
    rsx! {
        SplitMenuShell {
            root_class: "account-view-main".to_string(),
            sidebar_class: "channel-list-wrapper".to_string(),
            content_class: String::new(),
            sidebar: rsx! {
                ClientSidebar {}
                VoiceAccountFooter {}
            },
            content: rsx! {
                Outlet::<Route> {}
            },
        }
    }
}

// ── Route pages ─────────────────────────────────────────────────────────────

/// DM home — placeholder when no conversation is selected.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn DmsHome(backend: String, instance_id: String, account_id: String) -> Element {
    let app_state: Signal<AppState> = use_context();
    let nav = navigator();
    // Capability guard: backends without DMs (HN, Lemmy, GitHub) render an
    // unsupported-feature placeholder in place. We must NOT redirect here:
    // a use_effect → navigator().replace() chain in the guard causes a
    // cascade (DmsHome → Root → DmsHome) that deadlocks the WASM main thread
    // when combined with sync_route_to_app_state signal writes. The
    // favorites-sidebar click handler is responsible for picking the right
    // landing route for forum/non-DM accounts.
    let caps = poly_client::capabilities_for_slug(&backend);
    if matches!(caps.dms, poly_client::DmSupport::None) {
        return rsx! {
            FeatureUnsupportedPlaceholder {
                backend_slug: backend.clone(),
                feature: UnsupportedFeature::Dms,
            }
        };
    }
    let current_route = Route::DmsHome {
        backend: backend.clone(),
        instance_id: instance_id.clone(),
        account_id: account_id.clone(),
    };

    use_effect(move || {
        if crate::ui::main_layout::mobile_left_drawer_open() {
            return;
        }

        let Some(last_dm_url) = app_state
            .read()
            .nav
            .account_last_dm_routes
            .get(&account_id)
            .cloned()
        else {
            return;
        };

        if last_dm_url == format!("{current_route}") {
            return;
        }

        let Ok(route) = last_dm_url.parse::<Route>() else {
            return;
        };

        if let Route::DmChat {
            account_id: route_account_id,
            ..
        } = &route
            && route_account_id == &account_id
        {
            nav.replace(route);
        }
    });

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
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn DmChat(backend: String, instance_id: String, account_id: String, dm_id: String) -> Element {
    let mut app_state: Signal<AppState> = use_context();
    let chat_data: Signal<ChatData> = use_context();
    let client_manager: Signal<ClientManager> = use_context();
    let dm_id_for_pending = dm_id.clone();
    let account_id_for_pending = account_id.clone();

    use_effect(move || {
        restore_dm_chat(dm_id.clone(), account_id.clone(), chat_data, client_manager);
    });

    use_effect(move || {
        let pending = app_state.read().nav.pending_direct_call.clone();
        let Some(pending) = pending else {
            return;
        };
        if pending.account_id != account_id_for_pending || pending.dm_id != dm_id_for_pending {
            return;
        }
        spawn(async move {
            #[cfg(target_arch = "wasm32")]
            {
                let mut eval = document::eval(
                    "(function(){ \
                        const ready = !!window.__polyPendingDirectCallReady; \
                        if (ready) window.__polyPendingDirectCallReady = false; \
                        dioxus.send(ready ? 'ready' : 'wait'); \
                    })()",
                );
                let status = eval.recv::<String>().await.unwrap_or_default();
                if status != "ready" {
                    return;
                }
            }

            let Some(pending) = app_state.write().nav.pending_direct_call.take() else {
                return;
            };
            start_direct_call_from_active_account(
                DirectCallRequest {
                    target_user: pending.target_user,
                    start_video: pending.start_video,
                    allow_add_to_active_temporary: pending.allow_add_to_active_temporary,
                },
                app_state,
                chat_data,
                client_manager,
            );
        });
    });

    rsx! {
        ChatView {}
    }
}

#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn DmPendingCall(backend: String, instance_id: String, account_id: String, dm_id: String) -> Element {
    rsx! {
        DmPendingCallInner {
            backend,
            instance_id,
            account_id,
            dm_id,
            start_video: false,
            allow_add_to_active_temporary: false,
        }
    }
}

#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn DmPendingVideoCall(backend: String, instance_id: String, account_id: String, dm_id: String) -> Element {
    rsx! {
        DmPendingCallInner {
            backend,
            instance_id,
            account_id,
            dm_id,
            start_video: true,
            allow_add_to_active_temporary: false,
        }
    }
}

#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn DmPendingAddCall(backend: String, instance_id: String, account_id: String, dm_id: String) -> Element {
    rsx! {
        DmPendingCallInner {
            backend,
            instance_id,
            account_id,
            dm_id,
            start_video: false,
            allow_add_to_active_temporary: true,
        }
    }
}

#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn DmPendingAddVideoCall(backend: String, instance_id: String, account_id: String, dm_id: String) -> Element {
    rsx! {
        DmPendingCallInner {
            backend,
            instance_id,
            account_id,
            dm_id,
            start_video: true,
            allow_add_to_active_temporary: true,
        }
    }
}

#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn DmPendingCallInner(
    backend: String,
    instance_id: String,
    account_id: String,
    dm_id: String,
    start_video: bool,
    allow_add_to_active_temporary: bool,
) -> Element {
    let chat_data: Signal<ChatData> = use_context();
    let client_manager: Signal<ClientManager> = use_context();
    let dm_id_for_effect = dm_id.clone();
    let account_id_for_effect = account_id.clone();

    use_effect(move || {
        restore_dm_chat(
            dm_id_for_effect.clone(),
            account_id_for_effect.clone(),
            chat_data,
            client_manager,
        );
    });

    rsx! {
        ChatView {}
        OutgoingDirectCallOverlay {
            backend,
            instance_id,
            account_id,
            dm_id,
            start_video,
            allow_add_to_active_temporary,
        }
    }
}

#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn DmMediaViewerRoute(
    backend: String,
    instance_id: String,
    account_id: String,
    dm_id: String,
    message_id: String,
    attachment_index: usize,
) -> Element {
    let chat_data: Signal<ChatData> = use_context();
    let client_manager: Signal<ClientManager> = use_context();
    let overlay_channel_id = dm_id.clone();
    let overlay_message_id = message_id.clone();
    let dm_id_for_effect = dm_id.clone();
    let account_id_for_effect = account_id.clone();

    use_effect(move || {
        restore_dm_chat(
            dm_id_for_effect.clone(),
            account_id_for_effect.clone(),
            chat_data,
            client_manager,
        );
    });

    rsx! {
        ChatView {}
        super::account::common::MessageMediaViewerOverlay {
            channel_id: overlay_channel_id,
            message_id: overlay_message_id,
            attachment_index,
        }
    }
}

#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn ServerMediaViewerRoute(
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
    channel_id: String,
    message_id: String,
    attachment_index: usize,
) -> Element {
    let chat_data: Signal<ChatData> = use_context();
    let app_state: Signal<AppState> = use_context();
    let client_manager: Signal<ClientManager> = use_context();
    let nav = navigator();
    let overlay_channel_id = channel_id.clone();
    let overlay_message_id = message_id.clone();
    let backend_for_effect = backend.clone();
    let instance_for_effect = instance_id.clone();
    let account_for_effect = account_id.clone();
    let server_for_effect = server_id.clone();
    let channel_for_effect = channel_id.clone();
    let message_for_effect = message_id.clone();

    use_effect(move || {
        let backend_slug = backend_for_effect.clone();
        let instance = instance_for_effect.clone();
        let account = account_for_effect.clone();
        let route_server_id = server_for_effect.clone();
        let cid = channel_for_effect.clone();
        let sid = server_for_effect.clone();
        let route_message_id = message_for_effect.clone();

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
            let resolved_channel_id = super::favorites_sidebar::restore_server_channel(
                sid,
                cid.clone(),
                app_state,
                client_manager,
                chat_data,
            )
            .await;

            if let Some(resolved_channel_id) = resolved_channel_id
                && resolved_channel_id != cid
            {
                nav.replace(Route::ServerMediaViewerRoute {
                    backend: backend_slug,
                    instance_id: instance,
                    account_id: account,
                    server_id: route_server_id,
                    channel_id: resolved_channel_id,
                    message_id: route_message_id,
                    attachment_index,
                });
            }
        });
    });

    rsx! {
        ChatView {}
        super::account::common::MessageMediaViewerOverlay {
            channel_id: overlay_channel_id,
            message_id: overlay_message_id,
            attachment_index,
        }
    }
}

#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn NewConversationRoute(backend: String, instance_id: String, account_id: String) -> Element {
    rsx! {
        NewConversationView {}
    }
}

#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn ConversationSearchRoute(backend: String, instance_id: String, account_id: String) -> Element {
    rsx! {
        ConversationSearchView {}
    }
}

/// Server home — auto-selects first channel, renders chat / voice view.
///
/// On URL-restore navigation (F5, deep link) the click handler that normally
/// calls `load_server_data` never ran, so data is missing. The `use_effect`
/// here detects that case and loads the server data before rendering.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
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
    //
    // Guard against double-loading: if `current_server` already matches (click
    // navigation already ran `load_server_data`), or a load is already in flight
    // (`loading == true`), skip spawning a second concurrent load.
    let server_id_for_effect = server_id.clone();
    use_effect(move || {
        let sid = server_id_for_effect.clone();
        let snapshot = chat_data.read();
        let server_already_loaded = snapshot
            .current_server
            .as_ref()
            .is_some_and(|s| s.id == sid);
        // Prevent a second concurrent `load_server_data` while the click-handler's
        // spawn is still running (i.e. loading is already true).
        let already_loading = snapshot.loading;
        drop(snapshot);
        if server_already_loaded || already_loading {
            return;
        }
        let preserve_drawer_context = crate::ui::main_layout::mobile_left_drawer_open();
        spawn(async move {
            if preserve_drawer_context {
                super::favorites_sidebar::load_server_shell_data(
                    sid,
                    app_state,
                    client_manager,
                    chat_data,
                )
                .await;
            } else {
                super::favorites_sidebar::load_server_data(
                    sid,
                    app_state,
                    client_manager,
                    chat_data,
                )
                .await;
            }
        });
    });

    // Only consider a channel "voice" if the loaded server actually matches
    // the URL.  Without this guard, stale `current_channel` data left over
    // from demo browsing (or from a previously visited server) can cause
    // `VoiceChannelView` to render immediately — before `load_server_data`
    // runs — which triggers `getUserMedia` / audio-device access and can
    // hard-crash Chromium on Linux.
    let (is_voice_channel, is_forum_server) = {
        let cd = chat_data.read();
        let server_matches = cd.current_server.as_ref().is_some_and(|s| s.id == server_id);
        let is_voice = server_matches
            && cd.current_channel.as_ref().is_some_and(|ch| matches!(ch.channel_type, ChannelType::Voice | ChannelType::Video));
        let is_forum = server_matches
            && cd.current_server.as_ref().is_some_and(|s| s.backend.uses_forum_layout());
        (is_voice, is_forum)
    };

    rsx! {
        if is_voice_channel {
            VoiceChannelView {}
        } else if is_forum_server {
            ForumView {}
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
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
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
    let nav = navigator();
    let route_channel_id = channel_id.clone();
    use_effect(move || {
        let backend_slug = backend.clone();
        let instance = instance_id.clone();
        let account = account_id.clone();
        let route_server_id = server_id.clone();
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
            let resolved_channel_id = super::favorites_sidebar::restore_server_channel(
                sid,
                cid.clone(),
                app_state,
                client_manager,
                chat_data,
            )
            .await;

            if let Some(resolved_channel_id) = resolved_channel_id
                && resolved_channel_id != cid
            {
                nav.replace(Route::ServerChat {
                    backend: backend_slug,
                    instance_id: instance,
                    account_id: account,
                    server_id: route_server_id,
                    channel_id: resolved_channel_id,
                });
            }
        });
    });

    // Prefer the channel type from the channels list keyed by the route's
    // channel_id — this updates immediately on navigation.  Fall back to
    // current_channel (set asynchronously by restore_server_channel) so the
    // view stays correct while the async load is in flight.
    let channel_type = {
        let snapshot = chat_data.read();
        snapshot
            .channels
            .iter()
            .find(|ch| ch.id == route_channel_id)
            .map(|ch| ch.channel_type)
            .or_else(|| snapshot.current_channel.as_ref().map(|ch| ch.channel_type))
    };

    let is_forum_backend = chat_data.read().current_server.as_ref()
        .is_some_and(|s| s.backend.uses_forum_layout());
    let is_voice = matches!(channel_type, Some(ChannelType::Voice) | Some(ChannelType::Video));
    let is_forum_channel = matches!(channel_type, Some(ChannelType::Forum));
    // Forum-layout backends (Lemmy, demo_forum) use the Lemmy-style ForumView.
    // Non-forum-layout backends (Discord, generic) that carry individual Forum
    // channels use DiscordForumView, which calls get_forum_posts directly.
    let is_discord_forum = is_forum_channel && !is_forum_backend;
    let is_lemmy_forum = is_forum_backend;
    let is_code = matches!(channel_type, Some(ChannelType::Code));

    rsx! {
        if is_voice {
            VoiceChannelView {}
        } else if is_code {
            super::code_explorer::CodeExplorerView { route_channel_id: route_channel_id.clone() }
        } else if is_discord_forum {
            DiscordForumView {}
        } else if is_lemmy_forum {
            ForumView {}
        } else {
            ChatView {}
        }
    }
}

/// Friends browser — tiled grid view.
///
/// Capability-gated: backends without a friends list (HN, Lemmy, GitHub)
/// redirect to the account landing route instead of rendering an empty page.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn FriendsRoute(backend: String, instance_id: String, account_id: String) -> Element {
    let caps = poly_client::capabilities_for_slug(&backend);
    if matches!(caps.friends, poly_client::FriendModel::None) {
        return rsx! {
            FeatureUnsupportedPlaceholder {
                backend_slug: backend.clone(),
                feature: UnsupportedFeature::Friends,
            }
        };
    }
    let _ = (backend, instance_id);
    rsx! {
        FriendsPanel { account_id }
    }
}

#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn SavedItemsRoute(backend: String, instance_id: String, account_id: String) -> Element {
    rsx! {
        SavedItemsView {}
    }
}

/// Server/repo overview — landing page for forge backends (GitHub, Forgejo).
/// Shows a searchable list of all repos with open issues/PRs counts.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn ServerOverviewRoute(backend: String, instance_id: String, account_id: String) -> Element {
    rsx! {
        ServerOverviewPage { backend, instance_id, account_id }
    }
}

/// Notifications feed — account-scoped route that preserves Bar 2 context.
///
/// Capability-gated: HN has no notification surface, so the route redirects
/// to root rather than rendering an empty inbox.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn NotificationsRoute(backend: String, instance_id: String, account_id: String) -> Element {
    let caps = poly_client::capabilities_for_slug(&backend);
    if matches!(caps.notifications, poly_client::NotificationSupport::None) {
        return rsx! {
            FeatureUnsupportedPlaceholder {
                backend_slug: backend.clone(),
                feature: UnsupportedFeature::Notifications,
            }
        };
    }
    let _ = instance_id;
    rsx! {
        NotificationsView { account_id, backend_slug: backend }
    }
}

/// Settings page — app-level, not account-scoped.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn SettingsRoute() -> Element {
    rsx! {
        SettingsPage {}
    }
}

/// Settings page with a specific section pre-selected via URL.
///
/// `/settings/:section` deep-links directly into a settings section.
/// `sync_route_to_app_state` parses the `section` slug and writes it to
/// `AppState.settings_section`, so `SettingsPage` renders the correct content.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn SettingsSectionRoute(section: String) -> Element {
    // Navigation was already handled by sync_route_to_app_state;
    // SettingsPage reads settings_section from AppState.
    let _ = section; // consumed by router; state already synced
    rsx! {
        SettingsPage {}
    }
}

/// Agent page — app-level, not account-scoped.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn AgentRoute() -> Element {
    rsx! {
        AgentPage {}
    }
}

/// Agent page with a specific section pre-selected via URL.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn AgentSectionRoute(section: String) -> Element {
    let _ = section; // consumed by router; AgentPage reads section from its own state
    rsx! {
        AgentPage {}
    }
}

/// Global search page — browse the full node tree of all accounts.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn SearchRoute() -> Element {
    rsx! {
        super::search::SearchPage { locked_account_id: None }
    }
}

/// Account-scoped search — shows the global search but pre-filters to one account.
///
/// Navigated to from the 🔍 button in `FavoritesBar` when an account
/// context is active.  The account context stays in app-state nav so Bar 2
/// remains visible; `SearchPage` receives the `locked_account_id` prop and
/// initialises `enabled_accounts` to contain only that account.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn AccountSearchRoute(backend: String, instance_id: String, account_id: String) -> Element {
    let _ = (backend, instance_id); // consumed by router; state already synced
    rsx! {
        super::search::SearchPage { locked_account_id: Some(account_id) }
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
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
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
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
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
            section: "overview".to_string(),
        }
    }
}

/// Server settings for a specific section of one server.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn ServerSettingsSectionRoute(
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
    section: String,
) -> Element {
    rsx! {
        ServerSettingsPage {
            backend,
            instance_id,
            account_id,
            server_id,
            section,
        }
    }
}

/// Per-channel settings — Pack C.3 / P19.
///
/// Delegates to [`ChannelSettingsPage`] which renders the plugin-declared
/// `PerChannel` settings sections (empty-state message if the backend
/// declares none).
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn ChannelSettingsRoute(
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
    channel_id: String,
) -> Element {
    rsx! {
        ChannelSettingsPage {
            backend,
            instance_id,
            account_id,
            server_id,
            channel_id,
        }
    }
}

/// Root redirect — desktop memory history starts at "/".
///
/// Uses `use_effect` to navigate away on mount since the `on_update`
/// callback may not process its redirect return value on the very first
/// render in Dioxus memory-history mode.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
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

/// Backend picker — `/signup` — full-page, outside MainLayout.
///
/// Renders [`super::signup::SignupPickerPage`] which lists available backends
/// and navigates to `/signup/:client` on selection.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn SignupPicker() -> Element {
    rsx! {
        super::signup::SignupPickerPage {}
    }
}

/// Per-backend signup page — `/signup/:client` — full-page, outside MainLayout.
///
/// The `client` slug selects which backend signup page to render:
/// - `"poly"` → full Poly server signup/sign-in form
/// - all others → "coming soon" stub
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn ClientSignup(client: String) -> Element {
    rsx! {
        super::signup::ClientSignupPage { client }
    }
}

/// Per-account reauth page — `/:backend/:instance_id/:account_id/reauth`.
///
/// Full-page form (outside MainLayout) that lets the user update the existing
/// account's credentials in place, or remove the account entirely. Used when
/// a stored token has been rejected (401) and the app has marked the
/// connection status as [`ConnectionStatus::Unauthenticated`].
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn ReauthAccount(backend: String, instance_id: String, account_id: String) -> Element {
    rsx! {
        super::signup::ReauthAccountPage { backend, instance_id, account_id }
    }
}

/// Catch-all 404 — on_update redirects before render, but as a belt-and-suspenders
/// fallback this component also redirects to Root on mount.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn PageNotFound(segments: Vec<String>) -> Element {
    // `segments` is provided by the router for the unmatched path but we only
    // need it for the route match; discard it so the unused-variable lint stays clean.
    drop(segments);
    let nav = navigator();
    use_effect(move || {
        // Belt-and-suspenders: redirect in case on_update hasn't fired yet
        // (e.g. stale browser history URLs from old route formats).
        nav.replace(Route::Root);
    });
    // Brief loading while redirect fires — never visible in practice.
    rsx! { div { class: "storage-loading" } }
}

/// Create Server — `/:backend/:instance_id/:account_id/create-server`.
///
/// Full-page form inside MainLayout (both FavoritesBar + AccountServerBar remain
/// visible on the left). Delegates to [`super::create_server::CreateServerPage`].
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn CreateServerRoute(backend: String, instance_id: String, account_id: String) -> Element {
    // Backends that don't support creating a server (Matrix, Lemmy, HN…)
    // render an unsupported-feature placeholder instead of redirecting —
    // redirect chains from use_effect caused main-thread deadlocks.
    if !poly_client::slug_supports_creating_server(&backend) {
        return rsx! {
            FeatureUnsupportedPlaceholder {
                backend_slug: backend.clone(),
                feature: UnsupportedFeature::CreateServer,
            }
        };
    }
    rsx! {
        super::create_server::CreateServerPage { backend, instance_id, account_id }
    }
}

/// Create Channel — `/:backend/:instance_id/:account_id/channels/:server_id/create-channel`.
///
/// Full-page form inside `ServerLayout` — the `ChannelList` sidebar (with all
/// existing channels) stays visible on the left while the form occupies the
/// main content area on the right.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn CreateChannelRoute(
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
) -> Element {
    rsx! {
        super::create_channel::CreateChannelPage { backend, instance_id, account_id, server_id }
    }
}

/// Forum post thread view — `/:backend/:instance_id/:account_id/channels/:server_id/:channel_id/posts/:post_id`.
///
/// Renders inside `ServerLayout` (sidebar visible). The parent `channel_id` is synced into
/// `AppState.nav.selected_channel` by `sync_route_to_app_state` so the sidebar stays highlighted.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn ForumPostRoute(
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
    channel_id: String,
    post_id: String,
) -> Element {
    rsx! {
        ForumPostView { channel_id, post_id }
    }
}

/// Create forum post — `/:backend/:instance_id/:account_id/channels/:server_id/:channel_id/create-post`.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn CreateForumPostRoute(
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
    channel_id: String,
) -> Element {
    rsx! {
        CreateForumPostPage { backend, instance_id, account_id, server_id, channel_id }
    }
}

/// Forum search — `/:backend/:instance_id/:account_id/channels/:server_id/:channel_id/search`.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn ForumSearchRoute(
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
    channel_id: String,
) -> Element {
    rsx! {
        ForumSearchPage { backend, instance_id, account_id, server_id, channel_id }
    }
}

/// Forum comments feed — `/:backend/:instance_id/:account_id/channels/:server_id/:channel_id/comments`.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn ForumCommentsRoute(
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
    channel_id: String,
) -> Element {
    let _ = (backend, instance_id, account_id, server_id, channel_id);
    rsx! {
        div { class: "forum-view",
            div { class: "forum-empty",
                div { class: "forum-empty-icon", "💬" }
                p { "Comments feed — coming soon." }
            }
        }
    }
}

/// Runtime route-coverage counter (plan-connected-routes §7.4).
///
/// Debug-only. On each call, records the visited `Route` variant in a
/// process-wide set and logs the first observation at `debug` level via
/// `tracing`. Lets a dev session's visited set be diffed against the full
/// `Route` enum to find routes that were declared but never exercised.
#[cfg(debug_assertions)]
fn record_route_visit(route: &Route) {
    use std::collections::HashSet;
    use std::sync::{Mutex, OnceLock};

    static VISITED: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let set = VISITED.get_or_init(|| Mutex::new(HashSet::new()));
    let name = route_variant_name(route);
    if let Ok(mut guard) = set.lock() && guard.insert(name) {
        tracing::debug!(target: "poly::route_coverage", "visited route variant: {name}");
    }
}

/// Short discriminant name for a `Route` value. Used only by the
/// debug-assertions coverage counter; no runtime consumer in release builds.
#[cfg(debug_assertions)]
fn route_variant_name(route: &Route) -> &'static str {
    match route {
        Route::Root => "Root",
        Route::DmsHome { .. } => "DmsHome",
        Route::ConversationSearchRoute { .. } => "ConversationSearchRoute",
        Route::NewConversationRoute { .. } => "NewConversationRoute",
        Route::DmChat { .. } => "DmChat",
        Route::DmPendingCall { .. } => "DmPendingCall",
        Route::DmPendingVideoCall { .. } => "DmPendingVideoCall",
        Route::DmPendingAddCall { .. } => "DmPendingAddCall",
        Route::DmPendingAddVideoCall { .. } => "DmPendingAddVideoCall",
        Route::DmMediaViewerRoute { .. } => "DmMediaViewerRoute",
        Route::CreateChannelRoute { .. } => "CreateChannelRoute",
        Route::ServerChat { .. } => "ServerChat",
        Route::ServerMediaViewerRoute { .. } => "ServerMediaViewerRoute",
        Route::ForumPostRoute { .. } => "ForumPostRoute",
        Route::CreateForumPostRoute { .. } => "CreateForumPostRoute",
        Route::ForumSearchRoute { .. } => "ForumSearchRoute",
        Route::ForumCommentsRoute { .. } => "ForumCommentsRoute",
        Route::ServerHome { .. } => "ServerHome",
        Route::FriendsRoute { .. } => "FriendsRoute",
        Route::NotificationsRoute { .. } => "NotificationsRoute",
        Route::SavedItemsRoute { .. } => "SavedItemsRoute",
        Route::ServerOverviewRoute { .. } => "ServerOverviewRoute",
        Route::SettingsRoute => "SettingsRoute",
        Route::SettingsSectionRoute { .. } => "SettingsSectionRoute",
        Route::AgentRoute => "AgentRoute",
        Route::AgentSectionRoute { .. } => "AgentSectionRoute",
        Route::SearchRoute => "SearchRoute",
        Route::AccountSearchRoute { .. } => "AccountSearchRoute",
        Route::AccountSettingsRoute { .. } => "AccountSettingsRoute",
        Route::CreateServerRoute { .. } => "CreateServerRoute",
        Route::ServerSettingsRoute { .. } => "ServerSettingsRoute",
        Route::ServerSettingsSectionRoute { .. } => "ServerSettingsSectionRoute",
        Route::ChannelSettingsRoute { .. } => "ChannelSettingsRoute",
        Route::SignupPicker => "SignupPicker",
        Route::ClientSignup { .. } => "ClientSignup",
        Route::ReauthAccount { .. } => "ReauthAccount",
        Route::PageNotFound { .. } => "PageNotFound",
    }
}
