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

// Privileged write-access trait for `RouteSynced<T>`. Defined inline here so
// `pub(in crate::ui::routes)` satisfies Rust's ancestor-rule — the trait's
// defining module (this one) IS `crate::ui::routes`, which is its own
// ancestor trivially. A `use` of this trait outside `crate::ui::routes::*`
// does not compile, so the `.set(...)` path is compiler-enforced to live
// only here. See `crate::state::route_synced` for the full rationale (it
// prevents the friend-card WASM-scheduler-hang bug class).
mod internal {
    use crate::state::RouteSynced;
    pub(in crate::ui::routes) trait RouteSyncedWrite<T> {
        fn set(&mut self, value: T);
    }
    impl<T> RouteSyncedWrite<T> for RouteSynced<T> {
        #[inline]
        fn set(&mut self, value: T) {
            self.0 = value;
        }
    }
}
use crate::state::BatchedSignal;
use internal::RouteSyncedWrite as _;

use super::account::common::direct_call::{
    DirectCallRequest, start_direct_call_from_active_account,
};
use super::account::{
    AccountSettingsPage, ChannelSettingsPage, ChatView, ConversationSearchView, DiscordForumView,
    ForumView, ForumPostView, FriendsPanel, NewConversationView, NotificationsView,
    OutgoingDirectCallOverlay, SavedItemsView, ServerSettingsPage, ThreadFullView,
    ThreadPanel, VoiceChannelView,
};
use super::client_ui::ClientSidebar;
use super::create_forum_post::{CreateForumPostPage, ForumSearchPage};
use super::main_layout::MainLayout;
use super::client_ui::view::AccountOverviewView;
use super::agent::AgentPage;
use super::settings::SettingsPage;
use super::split_shell::SplitMenuShell;
use crate::client_manager::ClientManager;
use crate::i18n::t;
use crate::state::{AppState, ChatData, SettingsSection, View, use_spawn_once};
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
        | Route::ServerOverviewRoute { account_id, .. }
        | Route::ServerOverviewMissedRoute { account_id, .. }
        | Route::ServerOverviewStatsRoute { account_id, .. }
        | Route::ServerOverviewAgentsRoute { account_id, .. }
        | Route::ThreadView { account_id, .. } => Some(account_id.as_str()),
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

    let active = client_manager.active_account_ids();

    // E1 (extended): defer the verdict during the startup auto-signin burst.
    // Demo accounts sign in synchronously inside init_storage and land in
    // ClientManager BEFORE the Router mounts; non-demo test accounts sign in
    // asynchronously over ~1-2s. Without this guard, opening a deep link to
    // a non-demo account (e.g. /teams/.../U001/dms) checks early, finds demo
    // present + the target absent, and redirects to SettingsRoute permanently
    // — the route never re-evaluates after the target finally lands.
    //
    // Empty-active is the literal "still booting" case. The atomic flag
    // covers the partial-active case where the target's authenticate is
    // still in flight in `auto_signin_test_accounts`'s spawn.
    if active.is_empty() {
        return false;
    }
    #[cfg(debug_assertions)]
    {
        if !crate::ui::AUTO_SIGNIN_DONE.load(std::sync::atomic::Ordering::SeqCst) {
            return false;
        }
    }

    !active.into_iter().any(|id| id == account_id)
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

        #[connected(linked)]
        #[route("/:backend/:instance_id/:account_id/overview/missed")]
        ServerOverviewMissedRoute { backend: String, instance_id: String, account_id: String },

        #[connected(linked)]
        #[route("/:backend/:instance_id/:account_id/overview/stats")]
        ServerOverviewStatsRoute { backend: String, instance_id: String, account_id: String },

        #[connected(linked)]
        #[route("/:backend/:instance_id/:account_id/overview/agents")]
        ServerOverviewAgentsRoute { backend: String, instance_id: String, account_id: String },

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

        // ── Account-scoped settings ──────────────────────────────────
        #[connected(linked)]
        #[route("/:backend/:instance_id/:account_id/settings")]
        AccountSettingsRoute { backend: String, instance_id: String, account_id: String },

        // ── Account-scoped: Thread full-page view (mobile) ───────────
        #[connected(linked, programmatic<ThreadViewRouteProducer>)]
        #[route("/:backend/:instance_id/:account_id/threads/:thread_id")]
        ThreadView {
            backend: String,
            instance_id: String,
            account_id: String,
            thread_id: String,
        },

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
pub fn sync_route_to_app_state(route: &Route, app_state: BatchedSignal<AppState>) {
    #[cfg(debug_assertions)]
    record_route_visit(route);

    // Compute the URL string before borrowing app_state mutably.
    // Routable derives Display, so format!("{route}") gives the URL path.
    let route_url = format!("{route}");
    app_state.batch(|s| {
    match route {
        Route::DmsHome {
            backend,
            instance_id,
            account_id,
        } => {
            s.nav.view.set(View::DmsFriends);
            s.nav.active_backend.set(Some(BackendType::from_slug(backend)));
            s.nav.active_instance_id.set(Some(instance_id.clone()));
            s.nav.active_account_id.set(Some(account_id.clone()));
            s.nav.selected_server.set(None);
            s.nav.selected_channel.set(None);
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
            s.nav.view.set(View::DmsFriends);
            s.nav.active_backend.set(Some(BackendType::from_slug(backend)));
            s.nav.active_instance_id.set(Some(instance_id.clone()));
            s.nav.active_account_id.set(Some(account_id.clone()));
            s.nav.selected_server.set(None);
            s.nav.selected_channel.set(Some(dm_id.clone()));
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
            s.nav.view.set(View::DmsFriends);
            s.nav.active_backend.set(Some(BackendType::from_slug(backend)));
            s.nav.active_instance_id.set(Some(instance_id.clone()));
            s.nav.active_account_id.set(Some(account_id.clone()));
            s.nav.selected_server.set(None);
            s.nav.selected_channel.set(Some(dm_id.clone()));
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
            s.nav.view.set(View::DmsFriends);
            s.nav.active_backend.set(Some(BackendType::from_slug(backend)));
            s.nav.active_instance_id.set(Some(instance_id.clone()));
            s.nav.active_account_id.set(Some(account_id.clone()));
            s.nav.selected_server.set(None);
            s.nav.selected_channel.set(Some(dm_id.clone()));
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
            s.nav.view.set(View::Server);
            s.nav.active_backend.set(Some(BackendType::from_slug(backend)));
            s.nav.active_instance_id.set(Some(instance_id.clone()));
            s.nav.active_account_id.set(Some(account_id.clone()));
            s.nav.selected_server.set(Some(server_id.clone()));
            s.nav.selected_channel.set(Some(channel_id.clone()));
            s.nav
                .account_last_routes
                .insert(account_id.clone(), route_url);
        }
        Route::NewConversationRoute {
            backend,
            instance_id,
            account_id,
        } => {
            s.nav.view.set(View::DmsFriends);
            s.nav.active_backend.set(Some(BackendType::from_slug(backend)));
            s.nav.active_instance_id.set(Some(instance_id.clone()));
            s.nav.active_account_id.set(Some(account_id.clone()));
            s.nav.selected_server.set(None);
            s.nav.selected_channel.set(None);
        }
        Route::ConversationSearchRoute {
            backend,
            instance_id,
            account_id,
        } => {
            s.nav.view.set(View::DmsFriends);
            s.nav.active_backend.set(Some(BackendType::from_slug(backend)));
            s.nav.active_instance_id.set(Some(instance_id.clone()));
            s.nav.active_account_id.set(Some(account_id.clone()));
            s.nav.selected_server.set(None);
            s.nav.selected_channel.set(None);
        }
        Route::ServerHome {
            backend,
            instance_id,
            account_id,
            server_id,
        } => {
            s.nav.view.set(View::Server);
            s.nav.active_backend.set(Some(BackendType::from_slug(backend)));
            s.nav.active_instance_id.set(Some(instance_id.clone()));
            s.nav.active_account_id.set(Some(account_id.clone()));
            s.nav.selected_server.set(Some(server_id.clone()));
            s.nav.selected_channel.set(None);
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
            s.nav.view.set(View::Server);
            s.nav.active_backend.set(Some(BackendType::from_slug(backend)));
            s.nav.active_instance_id.set(Some(instance_id.clone()));
            s.nav.active_account_id.set(Some(account_id.clone()));
            s.nav.selected_server.set(Some(server_id.clone()));
            s.nav.selected_channel.set(Some(channel_id.clone()));
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
            s.nav.view.set(View::Server);
            s.nav.active_backend.set(Some(BackendType::from_slug(backend)));
            s.nav.active_instance_id.set(Some(instance_id.clone()));
            s.nav.active_account_id.set(Some(account_id.clone()));
            s.nav.selected_server.set(Some(server_id.clone()));
            s.nav.selected_channel.set(Some(channel_id.clone()));
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
            s.nav.view.set(View::Server);
            s.nav.active_backend.set(Some(BackendType::from_slug(backend)));
            s.nav.active_instance_id.set(Some(instance_id.clone()));
            s.nav.active_account_id.set(Some(account_id.clone()));
            s.nav.selected_server.set(Some(server_id.clone()));
            s.nav.selected_channel.set(Some(channel_id.clone()));
            // Do NOT record in account_last_routes — create-post is transient
        }
        Route::ForumSearchRoute {
            backend,
            instance_id,
            account_id,
            server_id,
            channel_id,
        } => {
            s.nav.view.set(View::Server);
            s.nav.active_backend.set(Some(BackendType::from_slug(backend)));
            s.nav.active_instance_id.set(Some(instance_id.clone()));
            s.nav.active_account_id.set(Some(account_id.clone()));
            s.nav.selected_server.set(Some(server_id.clone()));
            s.nav.selected_channel.set(Some(channel_id.clone()));
            // Do NOT record in account_last_routes — search is transient
        }
        Route::ForumCommentsRoute {
            backend,
            instance_id,
            account_id,
            server_id,
            channel_id,
        } => {
            s.nav.view.set(View::Server);
            s.nav.active_backend.set(Some(BackendType::from_slug(backend)));
            s.nav.active_instance_id.set(Some(instance_id.clone()));
            s.nav.active_account_id.set(Some(account_id.clone()));
            s.nav.selected_server.set(Some(server_id.clone()));
            s.nav.selected_channel.set(Some(channel_id.clone()));
            s.nav.account_last_routes.insert(account_id.clone(), route_url);
        }
        Route::FriendsRoute {
            backend,
            instance_id,
            account_id,
        } => {
            s.nav.view.set(View::Friends);
            s.nav.active_backend.set(Some(BackendType::from_slug(backend)));
            s.nav.active_instance_id.set(Some(instance_id.clone()));
            s.nav.active_account_id.set(Some(account_id.clone()));
            s.nav
                .account_last_routes
                .insert(account_id.clone(), route_url);
        }
        Route::NotificationsRoute {
            backend,
            instance_id,
            account_id,
        } => {
            s.nav.view.set(View::Notifications);
            s.nav.active_backend.set(Some(BackendType::from_slug(backend)));
            s.nav.active_instance_id.set(Some(instance_id.clone()));
            s.nav.active_account_id.set(Some(account_id.clone()));
            s.nav.selected_server.set(None);
            s.nav.selected_channel.set(None);
            s.nav
                .account_last_routes
                .insert(account_id.clone(), route_url);
        }
        Route::SavedItemsRoute {
            backend,
            instance_id,
            account_id,
        } => {
            s.nav.view.set(View::DmsFriends);
            s.nav.active_backend.set(Some(BackendType::from_slug(backend)));
            s.nav.active_instance_id.set(Some(instance_id.clone()));
            s.nav.active_account_id.set(Some(account_id.clone()));
            s.nav.selected_server.set(None);
            s.nav.selected_channel.set(None);
            s.nav
                .account_last_routes
                .insert(account_id.clone(), route_url);
        }
        Route::SettingsRoute => {
            s.nav.view.set(View::Settings);
            // App-level — clear account context so Bar 2 hides and no server stays "open"
            s.nav.active_account_id.set(None);
            s.nav.active_instance_id.set(None);
            s.nav.active_backend.set(None);
            s.nav.selected_server.set(None);
            s.nav.selected_channel.set(None);
        }
        Route::SettingsSectionRoute { section } => {
            s.nav.view.set(View::Settings);
            s.settings_section = SettingsSection::from_slug(section);
            s.nav.active_account_id.set(None);
            s.nav.active_instance_id.set(None);
            s.nav.active_backend.set(None);
            s.nav.selected_server.set(None);
            s.nav.selected_channel.set(None);
        }
        Route::AgentRoute => {
            s.nav.view.set(View::Agent);
            s.nav.active_account_id.set(None);
            s.nav.active_instance_id.set(None);
            s.nav.active_backend.set(None);
            s.nav.selected_server.set(None);
            s.nav.selected_channel.set(None);
        }
        Route::AgentSectionRoute { .. } => {
            s.nav.view.set(View::Agent);
            s.nav.active_account_id.set(None);
            s.nav.active_instance_id.set(None);
            s.nav.active_backend.set(None);
            s.nav.selected_server.set(None);
            s.nav.selected_channel.set(None);
        }
        Route::SearchRoute => {
            s.nav.view.set(View::Search);
            // App-level — clear account context so Bar 2 hides
            s.nav.active_account_id.set(None);
            s.nav.active_instance_id.set(None);
            s.nav.active_backend.set(None);
            s.nav.selected_server.set(None);
            s.nav.selected_channel.set(None);
        }
        Route::AccountSettingsRoute {
            backend,
            instance_id,
            account_id,
        } => {
            s.nav.view.set(View::Settings);
            s.nav.active_backend.set(Some(BackendType::from_slug(backend)));
            s.nav.active_instance_id.set(Some(instance_id.clone()));
            s.nav.active_account_id.set(Some(account_id.clone()));
            s.nav.selected_server.set(None);
            s.nav.selected_channel.set(None);
            s.nav
                .account_last_routes
                .insert(account_id.clone(), route_url);
        }
        Route::CreateServerRoute {
            backend,
            instance_id,
            account_id,
        } => {
            s.nav.view.set(View::Settings); // Reuse Settings view — hides channel list
            s.nav.active_backend.set(Some(BackendType::from_slug(backend)));
            s.nav.active_instance_id.set(Some(instance_id.clone()));
            s.nav.active_account_id.set(Some(account_id.clone()));
            s.nav.selected_server.set(None);
            s.nav.selected_channel.set(None);
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
            s.nav.view.set(View::Server);
            s.nav.active_backend.set(Some(BackendType::from_slug(backend)));
            s.nav.active_instance_id.set(Some(instance_id.clone()));
            s.nav.active_account_id.set(Some(account_id.clone()));
            s.nav.selected_server.set(Some(server_id.clone()));
            s.nav.selected_channel.set(None);
            // Do NOT record in account_last_routes — create-channel is transient
        }
        Route::ServerSettingsRoute {
            backend,
            instance_id,
            account_id,
            server_id,
        } => {
            s.nav.view.set(View::Settings);
            s.nav.active_backend.set(Some(BackendType::from_slug(backend)));
            s.nav.active_instance_id.set(Some(instance_id.clone()));
            s.nav.active_account_id.set(Some(account_id.clone()));
            s.nav.selected_server.set(Some(server_id.clone()));
            s.nav.selected_channel.set(None);
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
            s.nav.view.set(View::Settings);
            s.nav.active_backend.set(Some(BackendType::from_slug(backend)));
            s.nav.active_instance_id.set(Some(instance_id.clone()));
            s.nav.active_account_id.set(Some(account_id.clone()));
            s.nav.selected_server.set(Some(server_id.clone()));
            s.nav.selected_channel.set(None);
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
            s.nav.view.set(View::Settings);
            s.nav.active_backend.set(Some(BackendType::from_slug(backend)));
            s.nav.active_instance_id.set(Some(instance_id.clone()));
            s.nav.active_account_id.set(Some(account_id.clone()));
            s.nav.selected_server.set(Some(server_id.clone()));
            s.nav.selected_channel.set(Some(channel_id.clone()));
            s.nav
                .account_last_routes
                .insert(account_id.clone(), route_url);
        }
        Route::ServerOverviewRoute { backend, instance_id, account_id }
        | Route::ServerOverviewMissedRoute { backend, instance_id, account_id }
        | Route::ServerOverviewStatsRoute { backend, instance_id, account_id }
        | Route::ServerOverviewAgentsRoute { backend, instance_id, account_id } => {
            s.nav.view.set(View::Overview);
            s.nav.active_backend.set(Some(BackendType::from_slug(backend)));
            s.nav.active_instance_id.set(Some(instance_id.clone()));
            s.nav.active_account_id.set(Some(account_id.clone()));
            s.nav.selected_server.set(None);
            s.nav.selected_channel.set(None);
            s.nav
                .account_last_routes
                .insert(account_id.clone(), route_url);
        }
        Route::ThreadView { backend, instance_id, account_id, .. } => {
            // Thread full-page view (mobile). Keep account/server context intact;
            // the thread_id is carried by nav.thread_panel_open (or just the URL).
            s.nav.active_backend.set(Some(BackendType::from_slug(backend)));
            s.nav.active_instance_id.set(Some(instance_id.clone()));
            s.nav.active_account_id.set(Some(account_id.clone()));
            s.nav
                .account_last_routes
                .insert(account_id.clone(), route_url);
        }
        Route::Root | Route::PageNotFound { .. } => {
            // on_update will redirect — nothing to sync here
        }
        Route::SignupPicker | Route::ClientSignup { .. } => {
            // Signup routes are outside MainLayout — clear account context.
            s.nav.view.set(View::Signup);
            s.nav.active_account_id.set(None);
            s.nav.active_instance_id.set(None);
            s.nav.active_backend.set(None);
            s.nav.selected_server.set(None);
            s.nav.selected_channel.set(None);
        }
        Route::ReauthAccount { backend, instance_id, account_id } => {
            // Reauth is a full-page form like signup, but scoped to an
            // existing account — keep the account context so the page can
            // look it up in ClientManager / ChatData.
            s.nav.view.set(View::Signup);
            s.nav.active_backend.set(Some(BackendType::from_slug(backend)));
            s.nav.active_instance_id.set(Some(instance_id.clone()));
            s.nav.active_account_id.set(Some(account_id.clone()));
            s.nav.selected_server.set(None);
            s.nav.selected_channel.set(None);
        }
    }
    });
}

fn restore_dm_chat(
    dm_id: String,
    account_id: String,
    chat_data: BatchedSignal<ChatData>,
    client_manager: BatchedSignal<ClientManager>,
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
        chat_data.batch(move |cd| {
            cd.current_channel = Some(ch);
            cd.current_server = None;
        });
    }

    spawn(async move {
        // Fire an initial reset cascade so the UI paints a loading state
        // before we await the backend.
        chat_data.batch(|cd| {
            cd.loading = true;
            cd.messages = Vec::new();
            cd.members = Vec::new();
        });

        let unread_count = chat_data
            .read()
            .current_channel
            .as_ref()
            .filter(|channel| channel.id == dm_id)
            .map_or(0, |channel| channel.unread_count);

        let backend_arc = client_manager.read().get_backend(&account_id);
        let Some(backend_arc) = backend_arc else {
            chat_data.batch(|cd| cd.loading = false);
            return;
        };

        // tokio::time::timeout uses Instant::now() which panics on
        // wasm32-unknown-unknown ("time not implemented on this platform").
        // The executor is single-threaded on web so plain .await is fine.
        let guard = backend_arc.read().await;
        let messages = guard
            .get_messages(&dm_id, initial_message_query(unread_count))
            .await
            .ok();
        let members = guard.get_channel_members(&dm_id).await.ok();
        drop(guard);

        // ONE terminal cascade for the whole fetch.
        let mut pending = chat_data.pending_update();
        if let Some(msgs) = messages {
            pending.set(move |cd| cd.messages = msgs);
            request_restore_scroll_position_or_bottom(&dm_id);
        }
        if let Some(mbrs) = members {
            pending.set(move |cd| cd.members = mbrs);
        }
        pending.set(|cd| cd.loading = false);
        pending.apply();
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
    let app_state: BatchedSignal<AppState> = use_context();
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
    let app_state: BatchedSignal<AppState> = use_context();
    let chat_data: BatchedSignal<ChatData> = use_context();
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let dm_id_for_pending = dm_id.clone();
    let account_id_for_pending = account_id.clone();

    use_effect(move || {
        restore_dm_chat(dm_id.clone(), account_id.clone(), chat_data, client_manager);
    });

    // Key on the route's own (account_id, dm_id) — stable props that uniquely
    // identify this DmChat mount. The pending-call dispatch is a one-shot per
    // mount; `.take()` inside the async body consumes the pending option so
    // later renders become no-ops even if `use_spawn_once` weren't guarding us.
    use_spawn_once(
        (account_id_for_pending.clone(), dm_id_for_pending.clone()),
        move |(route_account_id, route_dm_id)| async move {
            let pending = app_state.peek().nav.pending_direct_call.clone();
            let Some(pending) = pending else {
                return;
            };
            if pending.account_id != route_account_id || pending.dm_id != route_dm_id {
                return;
            }
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

            let Some(pending) = app_state.batch(|st| st.nav.pending_direct_call.take()) else {
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
        },
    );

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
    let chat_data: BatchedSignal<ChatData> = use_context();
    let client_manager: BatchedSignal<ClientManager> = use_context();
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
    let chat_data: BatchedSignal<ChatData> = use_context();
    let client_manager: BatchedSignal<ClientManager> = use_context();
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
    let chat_data: BatchedSignal<ChatData> = use_context();
    let app_state: BatchedSignal<AppState> = use_context();
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let nav = navigator();
    let overlay_channel_id = channel_id.clone();
    let overlay_message_id = message_id.clone();
    let backend_for_effect = backend.clone();
    let instance_for_effect = instance_id.clone();
    let account_for_effect = account_id.clone();
    let server_for_effect = server_id.clone();
    let channel_for_effect = channel_id.clone();
    let message_for_effect = message_id.clone();

    // Key on the URL channel id — the stable identity across renders. The
    // "already loaded" fast-path moves inside the async body so the hook's
    // key-guard alone owns re-spawn prevention (the old `already_loaded`
    // check subscribed to chat_data writes and re-fired mid-restore, which
    // is the exact hang-class #3 shape — see plan-use-spawn-once §3).
    use_spawn_once(channel_for_effect.clone(), move |cid| {
        let backend_slug = backend_for_effect.clone();
        let instance = instance_for_effect.clone();
        let account = account_for_effect.clone();
        let route_server_id = server_for_effect.clone();
        let sid = server_for_effect.clone();
        let route_message_id = message_for_effect.clone();
        async move {
            // Cheap early return if click-navigation already populated the
            // channel + server. Peek so we don't subscribe.
            let already_loaded = {
                let snapshot = chat_data.peek();
                snapshot
                    .current_server
                    .as_ref()
                    .is_some_and(|server| server.id == sid)
                    && snapshot
                        .current_channel
                        .as_ref()
                        .is_some_and(|ch| ch.id == cid && ch.server_id == sid)
                    && snapshot.channels.iter().any(|ch| ch.id == cid)
            };
            if already_loaded {
                return;
            }

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
        }
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
    let chat_data: BatchedSignal<ChatData> = use_context();
    let app_state: BatchedSignal<AppState> = use_context();
    let client_manager: BatchedSignal<ClientManager> = use_context();

    // URL-restore: server data is absent after a hard reload. Load it now.
    //
    // `use_spawn_once` guards against RE-spawning after a previous load
    // finished without populating current_server (e.g. backend not found
    // for this server_id — Teams' team_id isn't a backend key). Without
    // this guard, `load_server_data_internal` toggles loading=true→false,
    // the effect re-fires, server_already_loaded is still false, and we
    // spawn forever. See Teams channels/T001 wedge, 2026-04-24, and
    // `docs/plans/plan-use-spawn-once.md`.
    // Key on (account_id, server_id) so switching accounts on the same
    // server URL forces a reload of that account's view of the server.
    let spawn_key = format!("{account_id}|{server_id}");
    let sid_for_async = server_id.clone();
    use_spawn_once(spawn_key, move |_key| {
        let sid = sid_for_async.clone();
        let preserve_drawer_context = crate::ui::main_layout::mobile_left_drawer_open();
        async move {
            // No `already_loaded` fast-path: the (account_id, server_id) key
            // already guarantees we only spawn once per account-switch, and
            // current_server can belong to a *different* account, so an id
            // match doesn't mean the cached data is for *this* account.
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
        }
    });

    // Only consider a channel "voice" if the loaded server actually matches
    // the URL.  Without this guard, stale `current_channel` data left over
    // from demo browsing (or from a previously visited server) can cause
    // `VoiceChannelView` to render immediately — before `load_server_data`
    // runs — which triggers `getUserMedia` / audio-device access and can
    // hard-crash Chromium on Linux.
    let (is_voice_channel, is_forum_server, is_empty_server) = {
        let cd = chat_data.read();
        let server_matches = cd.current_server.as_ref().is_some_and(|s| s.id == server_id);
        let is_voice = server_matches
            && cd.current_channel.as_ref().is_some_and(|ch| matches!(ch.channel_type, ChannelType::Voice | ChannelType::Video));
        let is_forum = server_matches
            && cd.current_server.as_ref().is_some_and(|s| s.backend.uses_forum_layout());
        // Empty server: server loaded but channels list is empty AND we're
        // not still in the initial loading window. Without this branch,
        // ChatView renders blank, which on a stale-deep-link redirect to
        // ServerHome (see ServerChat use_effect) leaves the user staring at
        // an empty pane with no explanation.
        let is_empty = server_matches && cd.channels.is_empty() && !cd.loading;
        (is_voice, is_forum, is_empty)
    };

    rsx! {
        if is_empty_server {
            div { class: "empty-state special-page-empty-state",
                h3 { "{t(\"server-empty-title\")}" }
                p { "{t(\"server-empty-body\")}" }
            }
        } else if is_voice_channel {
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
    let chat_data: BatchedSignal<ChatData> = use_context();
    let app_state: BatchedSignal<AppState> = use_context();
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let nav = navigator();
    let route_channel_id = channel_id.clone();
    // `use_spawn_once` keys on the URL channel id and refuses to re-spawn
    // for the same key. Without this guard, a stale-channel URL (e.g.
    // /channel/deleted-id) infinite-looped: restore_server_channel writes
    // chat_data 8 times, each write re-fired the use_effect, the
    // `already_loaded` check failed because the fallback channel.id never
    // equals the URL cid, and we spawned another restore — exponential
    // task growth → Chrome OOM (witnessed 2026-04-19).
    // Key on (account_id, channel_id) so switching accounts on the same
    // channel URL (e.g. demo-cat → demo-dog on cat-dog-arena/general) forces
    // a reload — without the account in the key, the readiness check passes
    // and the view stays stuck on the previous account's cached messages.
    let spawn_key = format!("{account_id}|{channel_id}");
    let cid_for_async = channel_id.clone();
    use_spawn_once(spawn_key, move |_key| {
        let backend_slug = backend.clone();
        let instance = instance_id.clone();
        let account = account_id.clone();
        let route_server_id = server_id.clone();
        let sid = server_id.clone();
        let cid = cid_for_async.clone();
        async move {
            let resolved_channel_id = super::favorites_sidebar::restore_server_channel(
                sid,
                cid.clone(),
                app_state,
                client_manager,
                chat_data,
            )
            .await;

            match resolved_channel_id {
                Some(resolved) if resolved != cid => {
                    nav.replace(Route::ServerChat {
                        backend: backend_slug,
                        instance_id: instance,
                        account_id: account,
                        server_id: route_server_id,
                        channel_id: resolved,
                    });
                }
                None => {
                    // Server has no channels at all (deleted or empty) —
                    // bounce to ServerHome so the user sees the server shell
                    // instead of an empty wedged page on a stale deep link.
                    nav.replace(Route::ServerHome {
                        backend: backend_slug,
                        instance_id: instance,
                        account_id: account,
                        server_id: route_server_id,
                    });
                }
                Some(_) => {}
            }
        }
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
    let _ = instance_id;
    rsx! {
        FriendsPanel { account_id, backend_slug: backend }
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

/// Per-account overview — default landing page for every backend.
///
/// Renders the plugin-supplied `get_account_overview_view()` ViewDescriptor
/// inside the standard account layout: Bar 1 (favorites) + Bar 2
/// (account-server-bar) + Channel sidebar column (the host's
/// `OverviewSidebar` with category toggles) + Content (cards). Same shape
/// as DMs / Friends / Server pages so the mobile pattern is uniform and
/// the account footer (Cat / etc.) stays at the bottom of column 3.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn ServerOverviewRoute(backend: String, instance_id: String, account_id: String) -> Element {
    let _ = (backend, instance_id);
    rsx! {
        SplitMenuShell {
            root_class: "account-view-main overview-shell".to_string(),
            sidebar_class: "special-page-sidebar overview-panel-sidebar".to_string(),
            content_class: "special-page-content overview-panel-content".to_string(),
            sidebar: rsx! {
                crate::ui::account::common::OverviewSidebar {}
                VoiceAccountFooter {}
            },
            content: rsx! {
                AccountOverviewView { account_id }
            },
        }
    }
}

/// Per-account overview — "Things you missed" sub-page.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn ServerOverviewMissedRoute(backend: String, instance_id: String, account_id: String) -> Element {
    let _ = (backend, instance_id);
    rsx! {
        SplitMenuShell {
            root_class: "account-view-main overview-shell".to_string(),
            sidebar_class: "special-page-sidebar overview-panel-sidebar".to_string(),
            content_class: "special-page-content overview-panel-content".to_string(),
            sidebar: rsx! {
                crate::ui::account::common::OverviewSidebar {}
                VoiceAccountFooter {}
            },
            content: rsx! {
                crate::ui::account::common::OverviewMissedView { account_id }
            },
        }
    }
}

/// Per-account overview — Statistics sub-page.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn ServerOverviewStatsRoute(backend: String, instance_id: String, account_id: String) -> Element {
    let _ = (backend, instance_id);
    rsx! {
        SplitMenuShell {
            root_class: "account-view-main overview-shell".to_string(),
            sidebar_class: "special-page-sidebar overview-panel-sidebar".to_string(),
            content_class: "special-page-content overview-panel-content".to_string(),
            sidebar: rsx! {
                crate::ui::account::common::OverviewSidebar {}
                VoiceAccountFooter {}
            },
            content: rsx! {
                crate::ui::account::common::OverviewStatsView { account_id }
            },
        }
    }
}

/// Per-account overview — Active Agents sub-page.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn ServerOverviewAgentsRoute(backend: String, instance_id: String, account_id: String) -> Element {
    let _ = (backend, instance_id);
    rsx! {
        SplitMenuShell {
            root_class: "account-view-main overview-shell".to_string(),
            sidebar_class: "special-page-sidebar overview-panel-sidebar".to_string(),
            content_class: "special-page-content overview-panel-content".to_string(),
            sidebar: rsx! {
                crate::ui::account::common::OverviewSidebar {}
                VoiceAccountFooter {}
            },
            content: rsx! {
                crate::ui::account::common::OverviewAgentsView { account_id }
            },
        }
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
    let client_manager: BatchedSignal<crate::client_manager::ClientManager> = use_context();
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

/// Thread full-page view — `/:backend/:instance_id/:account_id/threads/:thread_id`.
///
/// Mobile / narrow viewport: replaces the main content area with a full-page
/// thread view. On desktop the thread panel (side-panel alongside the channel)
/// is used instead, so users only land here from mobile navigation.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn ThreadView(
    backend: String,
    instance_id: String,
    account_id: String,
    thread_id: String,
) -> Element {
    let _ = (backend, instance_id, account_id);
    rsx! {
        ThreadFullView { thread_id }
    }
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
        // F-LE-2: Lemmy gets a "Browse Communities" CTA that opens the instance
        // communities page in a new tab — more useful than a generic error.
        if backend == "lemmy" {
            let communities_url = format!("https://{instance_id}/communities");
            return rsx! {
                div {
                    class: "special-page-content feature-unsupported",
                    "data-testid": "lemmy-browse-communities",
                    div { class: "feature-unsupported-inner",
                        p { class: "feature-unsupported-message",
                            "Browse and subscribe to communities on your Lemmy instance."
                        }
                        p { class: "feature-unsupported-hint",
                            "Subscribed communities will appear in the sidebar after you subscribe."
                        }
                        a {
                            class: "feature-unsupported-cta-link",
                            href: "{communities_url}",
                            target: "_blank",
                            rel: "noopener noreferrer",
                            "Browse Communities →"
                        }
                    }
                }
            };
        }
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
        Route::ServerOverviewMissedRoute { .. } => "ServerOverviewMissedRoute",
        Route::ServerOverviewStatsRoute { .. } => "ServerOverviewStatsRoute",
        Route::ServerOverviewAgentsRoute { .. } => "ServerOverviewAgentsRoute",
        Route::SettingsRoute => "SettingsRoute",
        Route::SettingsSectionRoute { .. } => "SettingsSectionRoute",
        Route::AgentRoute => "AgentRoute",
        Route::AgentSectionRoute { .. } => "AgentSectionRoute",
        Route::SearchRoute => "SearchRoute",
        Route::AccountSettingsRoute { .. } => "AccountSettingsRoute",
        Route::ThreadView { .. } => "ThreadView",
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
