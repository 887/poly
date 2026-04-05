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
    AccountSettingsPage, ChannelList, ChatView, ConversationSearchView, ForumView, ForumPostView,
    FriendsPanel, NewConversationView, NotificationsView, OutgoingDirectCallOverlay, SavedItemsView,
    ServerSettingsPage, VoiceChannelView,
};
use super::create_forum_post::CreateForumPostPage;
use super::main_layout::MainLayout;
use super::settings::SettingsPage;
use super::split_shell::SplitMenuShell;
use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::{AppState, ChatData, SettingsSection, View};
use crate::ui::account::common::VoiceAccountFooter;
use crate::ui::account::common::chat_history::initial_message_query;
use crate::ui::account::common::chat_history::request_restore_scroll_position_or_bottom;
use dioxus::prelude::*;
use poly_client::{BackendType, Channel, ChannelType};

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
        | Route::ServerSettingsRoute { account_id, .. }
        | Route::ServerSettingsSectionRoute { account_id, .. }
        | Route::CreateChannelRoute { account_id, .. }
        | Route::FriendsRoute { account_id, .. }
        | Route::NotificationsRoute { account_id, .. }
        | Route::SavedItemsRoute { account_id, .. }
        | Route::AccountSettingsRoute { account_id, .. }
        | Route::CreateServerRoute { account_id, .. }
        | Route::AccountSearchRoute { account_id, .. } => Some(account_id.as_str()),
        Route::Root
        | Route::SettingsRoute
        | Route::SettingsSectionRoute { .. }
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

            #[route("/:backend/:instance_id/:account_id/dms/search")]
            ConversationSearchRoute { backend: String, instance_id: String, account_id: String },

            #[route("/:backend/:instance_id/:account_id/dms/new")]
            NewConversationRoute { backend: String, instance_id: String, account_id: String },

            #[route("/:backend/:instance_id/:account_id/dms/:dm_id")]
            DmChat { backend: String, instance_id: String, account_id: String, dm_id: String },

            #[route("/:backend/:instance_id/:account_id/dms/:dm_id/call")]
            DmPendingCall { backend: String, instance_id: String, account_id: String, dm_id: String },

            #[route("/:backend/:instance_id/:account_id/dms/:dm_id/video-call")]
            DmPendingVideoCall { backend: String, instance_id: String, account_id: String, dm_id: String },

            #[route("/:backend/:instance_id/:account_id/dms/:dm_id/call/add")]
            DmPendingAddCall { backend: String, instance_id: String, account_id: String, dm_id: String },

            #[route("/:backend/:instance_id/:account_id/dms/:dm_id/video-call/add")]
            DmPendingAddVideoCall { backend: String, instance_id: String, account_id: String, dm_id: String },

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
            #[route("/:backend/:instance_id/:account_id/channels/:server_id/create-channel")]
            CreateChannelRoute {
                backend: String,
                instance_id: String,
                account_id: String,
                server_id: String,
            },

            #[route("/:backend/:instance_id/:account_id/channels/:server_id/:channel_id")]
            ServerChat {
                backend: String,
                instance_id: String,
                account_id: String,
                server_id: String,
                channel_id: String,
            },

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
            #[route("/:backend/:instance_id/:account_id/channels/:server_id/:channel_id/create-post")]
            CreateForumPostRoute {
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

        #[route("/:backend/:instance_id/:account_id/notifications")]
        NotificationsRoute { backend: String, instance_id: String, account_id: String },

        #[route("/:backend/:instance_id/:account_id/saved")]
        SavedItemsRoute { backend: String, instance_id: String, account_id: String },

        // ── App-level (not account-scoped) ───────────────────────────
        #[route("/settings")]
        SettingsRoute,

        #[route("/settings/:section")]
        SettingsSectionRoute { section: String },

        #[route("/search")]
        SearchRoute,

        /// Account-scoped search — shows the global search page but pre-filters to one account.
        #[route("/:backend/:instance_id/:account_id/search")]
        AccountSearchRoute { backend: String, instance_id: String, account_id: String },

        // ── Account-scoped settings ──────────────────────────────────
        #[route("/:backend/:instance_id/:account_id/settings")]
        AccountSettingsRoute { backend: String, instance_id: String, account_id: String },

        // ── Account-scoped: Create server (full-page form) ───────────
        #[route("/:backend/:instance_id/:account_id/create-server")]
        CreateServerRoute { backend: String, instance_id: String, account_id: String },

        // ── Account-scoped: Server settings ─────────────────────────
        #[route("/:backend/:instance_id/:account_id/servers/:server_id/settings")]
        ServerSettingsRoute {
            backend: String,
            instance_id: String,
            account_id: String,
            server_id: String,
        },

        #[route("/:backend/:instance_id/:account_id/servers/:server_id/settings/:section")]
        ServerSettingsSectionRoute {
            backend: String,
            instance_id: String,
            account_id: String,
            server_id: String,
            section: String,
        },

    #[end_layout]

    // ── Signup flow (full-page, no sidebar) ─────────────────────────────────
    // These routes render without MainLayout, giving a clean signup experience.
    #[route("/signup")]
    SignupPicker,

    #[route("/signup/:client")]
    ClientSignup { client: String },

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
#[rustfmt::skip]
#[component]
fn DmsLayout() -> Element {
    rsx! {
        SplitMenuShell {
            root_class: "account-view-main".to_string(),
            sidebar_class: "channel-list-wrapper".to_string(),
            content_class: String::new(),
            sidebar: rsx! {
                ChannelList {}
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
#[rustfmt::skip]
#[component]
fn ServerLayout() -> Element {
    rsx! {
        SplitMenuShell {
            root_class: "account-view-main".to_string(),
            sidebar_class: "channel-list-wrapper".to_string(),
            content_class: String::new(),
            sidebar: rsx! {
                ChannelList {}
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
#[rustfmt::skip]
#[component]
fn DmsHome(backend: String, instance_id: String, account_id: String) -> Element {
    let app_state: Signal<AppState> = use_context();
    let nav = navigator();
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
#[rustfmt::skip]
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

#[rustfmt::skip]
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

#[rustfmt::skip]
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

#[rustfmt::skip]
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

#[rustfmt::skip]
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

#[rustfmt::skip]
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

#[rustfmt::skip]
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

#[rustfmt::skip]
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

#[rustfmt::skip]
#[component]
fn NewConversationRoute(backend: String, instance_id: String, account_id: String) -> Element {
    rsx! {
        NewConversationView {}
    }
}

#[rustfmt::skip]
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
#[rustfmt::skip]
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
    let is_voice_channel = {
        let cd = chat_data.read();
        cd.current_server
            .as_ref()
            .is_some_and(|s| s.id == server_id)
            && cd
                .current_channel
                .as_ref()
                .is_some_and(|ch| matches!(ch.channel_type, ChannelType::Voice | ChannelType::Video))
    };

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
#[rustfmt::skip]
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

    let channel_type = chat_data
        .read()
        .current_channel
        .as_ref()
        .map(|ch| ch.channel_type.clone());

    let is_voice = matches!(channel_type, Some(ChannelType::Voice) | Some(ChannelType::Video));
    let is_forum = matches!(channel_type, Some(ChannelType::Forum));

    rsx! {
        if is_voice {
            VoiceChannelView {}
        } else if is_forum {
            ForumView {}
        } else {
            ChatView {}
        }
    }
}

/// Friends browser — tiled grid view.
#[rustfmt::skip]
#[component]
fn FriendsRoute(backend: String, instance_id: String, account_id: String) -> Element {
    rsx! {
        FriendsPanel { account_id }
    }
}

#[rustfmt::skip]
#[component]
fn SavedItemsRoute(backend: String, instance_id: String, account_id: String) -> Element {
    rsx! {
        SavedItemsView {}
    }
}

/// Notifications feed — account-scoped route that preserves Bar 2 context.
#[rustfmt::skip]
#[component]
fn NotificationsRoute(backend: String, instance_id: String, account_id: String) -> Element {
    let _route_identity = (backend, instance_id, account_id);
    rsx! {
        NotificationsView {}
    }
}

/// Settings page — app-level, not account-scoped.
#[rustfmt::skip]
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
#[rustfmt::skip]
#[component]
fn SettingsSectionRoute(section: String) -> Element {
    // Navigation was already handled by sync_route_to_app_state;
    // SettingsPage reads settings_section from AppState.
    let _ = section; // consumed by router; state already synced
    rsx! {
        SettingsPage {}
    }
}

/// Global search page — browse the full node tree of all accounts.
#[rustfmt::skip]
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
#[rustfmt::skip]
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
#[rustfmt::skip]
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
#[rustfmt::skip]
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
#[rustfmt::skip]
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

/// Root redirect — desktop memory history starts at "/".
///
/// Uses `use_effect` to navigate away on mount since the `on_update`
/// callback may not process its redirect return value on the very first
/// render in Dioxus memory-history mode.
#[rustfmt::skip]
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
#[rustfmt::skip]
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
#[rustfmt::skip]
#[component]
fn ClientSignup(client: String) -> Element {
    rsx! {
        super::signup::ClientSignupPage { client }
    }
}

/// Catch-all 404 — on_update redirects before render, but as a belt-and-suspenders
/// fallback this component also redirects to Root on mount.
#[rustfmt::skip]
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
#[rustfmt::skip]
#[component]
fn CreateServerRoute(backend: String, instance_id: String, account_id: String) -> Element {
    rsx! {
        super::create_server::CreateServerPage { backend, instance_id, account_id }
    }
}

/// Create Channel — `/:backend/:instance_id/:account_id/channels/:server_id/create-channel`.
///
/// Full-page form inside `ServerLayout` — the `ChannelList` sidebar (with all
/// existing channels) stays visible on the left while the form occupies the
/// main content area on the right.
#[rustfmt::skip]
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
#[rustfmt::skip]
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
#[rustfmt::skip]
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
