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

// ── Domain sub-modules ───────────────────────────────────────────────────────
// Each sub-module owns the #[component] adapter functions for its domain.
// Route variants reference these functions by name; the `use` globs below
// bring them into scope so the Dioxus Routable derive can find them.

mod account;
mod agent;
mod dm;
mod forum;
mod server;
mod settings;

// Re-export all per-domain components into this module's namespace so
// #[derive(Routable)] can see them by the same short names as the variants.
use account::{
    ClientSignup, PageNotFound, ReauthAccount, Root, SignupPicker,
};
use agent::{AgentRoute, AgentSectionRoute, PersonasRoute, SearchRoute};
use dm::{
    ConversationSearchRoute, DmChat, DmIncomingCall, DmMediaViewerRoute, DmPendingAddCall,
    DmPendingAddVideoCall, DmPendingCall, DmPendingVideoCall, DmsHome, DmsLayout,
    NewConversationRoute,
};
use forum::{CreateForumPostRoute, ForumCommentsRoute, ForumPostRoute, ForumSearchRoute};
use server::{
    CreateChannelRoute, CreateServerRoute, DiscoverRoute, FriendsRoute, NotificationsRoute,
    SavedItemsRoute, ServerChat, ServerHome, ServerLayout, ServerMediaViewerRoute,
    ServerOverviewAgentsRoute, ServerOverviewMissedRoute, ServerOverviewRoute,
    ServerOverviewStatsRoute, ThreadView,
};
use settings::{
    AccountSettingsRoute, ChannelSettingsRoute, ServerSettingsRoute, ServerSettingsSectionRoute,
    SettingsRoute, SettingsSectionRoute,
};

use crate::ui::main_layout::MainLayout;
use crate::client_manager::ClientManager;
use crate::state::{NavState, SettingsSection, View};
use dioxus::prelude::*;
use poly_client::BackendType;

// `route_account_id` and `route_variant_name` are generated by
// `#[derive(Connected)]` as inherent methods on `Route`.  The derive inspects
// each variant's fields: variants with an `account_id` field yield
// `Some(account_id.as_str())`; those without (or annotated
// `#[connected(skip_account_id)]`) yield `None`.
// See `crates/ui-macros/src/route_introspect.rs` for the codegen.

/// Whether the current route targets an account that is not currently active.
pub fn route_targets_unknown_account(route: &Route, client_manager: &ClientManager) -> bool {
    let Some(account_id) = Route::route_account_id(route) else {
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

    // E1.5 — also defer when the account is in `expected_account_ids`:
    // it's persisted in storage but `restore_native_accounts` hasn't
    // landed it in `backends` / `sessions` yet. Without this, deep links
    // to a non-demo account on cold boot bounce to /settings before
    // async restoration completes.
    if client_manager.expected_account_ids.contains(account_id) {
        return false;
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
#[derive(Routable, Clone, PartialEq, Eq, Debug, poly_ui_macros::Connected)]
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

            /// D.3 — shown when a Discord DM call is ringing for the local user.
            /// Navigated to from the gateway CALL_CREATE handler.
            /// Shows accept / decline UI.
            #[connected(linked)]
            #[route("/:backend/:instance_id/:account_id/dms/:dm_id/incoming-call")]
            DmIncomingCall { backend: String, instance_id: String, account_id: String, dm_id: String },

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
        #[route("/agent/personas")]
        PersonasRoute,

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

        // ── Account-scoped: Discover Communities ─────────────────────
        #[connected(linked)]
        #[route("/:backend/:instance_id/:account_id/discover")]
        DiscoverRoute { backend: String, instance_id: String, account_id: String },

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
    //
    // `skip_account_id` instructs `#[derive(Connected)]` to emit `None` from
    // `route_account_id` even though this variant carries an `account_id` field.
    // `route_targets_unknown_account` must never treat a reauth URL as pointing
    // at a missing account — the reauth flow exists precisely to fix an account
    // whose backend may be gone.
    #[connected(linked, skip_account_id)]
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
// lint-allow-unused: per-route arms have intentionally separate bodies
// even when they look identical — each arm is the documented landing
// behaviour for its specific Route variant; merging would obscure intent
// and the arms are likely to diverge as new fields are added per-route.
#[allow(clippy::match_same_arms)]
// lint-allow-unused: long cohesive view/handler; splitting risks reactive bugs
#[allow(clippy::too_many_lines)]
pub fn sync_route_to_app_state(route: &Route, nav: BatchedSignal<NavState>, user_prefs: Option<BatchedSignal<crate::state::UserPrefs>>) {
    #[cfg(debug_assertions)]
    account::record_route_visit(route);

    // Compute the URL string before borrowing nav mutably.
    // Routable derives Display, so format!("{route}") gives the URL path.
    let route_url = format!("{route}");
    // Extract settings_section from the route before the nav.batch borrow.
    let settings_section_override: Option<SettingsSection> = match route {
        Route::SettingsSectionRoute { section } => Some(SettingsSection::from_slug(section)),
        _ => None,
    };
    nav.batch(|s| {
    match route {
        Route::DmsHome {
            backend,
            instance_id,
            account_id,
        } => {
            s.view.set(View::DmsFriends);
            s.active_backend.set(Some(BackendType::from_slug(backend)));
            s.active_instance_id.set(Some(instance_id.clone()));
            s.active_account_id.set(Some(account_id.clone()));
            s.selected_server.set(None);
            s.selected_channel.set(None);
            s.account_last_routes.insert(account_id.clone(), route_url);
        }
        Route::DmChat {
            backend,
            instance_id,
            account_id,
            dm_id,
        } => {
            s.view.set(View::DmsFriends);
            s.active_backend.set(Some(BackendType::from_slug(backend)));
            s.active_instance_id.set(Some(instance_id.clone()));
            s.active_account_id.set(Some(account_id.clone()));
            s.selected_server.set(None);
            s.selected_channel.set(Some(dm_id.clone()));
            s.account_last_routes.insert(account_id.clone(), route_url);
            s.account_last_dm_routes.insert(account_id.clone(), format!("{route}"));
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
        }
        // D.3 — incoming call route; same nav-state semantics as outgoing pending call.
        | Route::DmIncomingCall {
            backend,
            instance_id,
            account_id,
            dm_id,
        } => {
            s.view.set(View::DmsFriends);
            s.active_backend.set(Some(BackendType::from_slug(backend)));
            s.active_instance_id.set(Some(instance_id.clone()));
            s.active_account_id.set(Some(account_id.clone()));
            s.selected_server.set(None);
            s.selected_channel.set(Some(dm_id.clone()));
            let dm_route = format!(
                "{}",
                Route::DmChat {
                    backend: backend.clone(),
                    instance_id: instance_id.clone(),
                    account_id: account_id.clone(),
                    dm_id: dm_id.clone(),
                }
            );
            s.account_last_routes.insert(account_id.clone(), dm_route.clone());
            s.account_last_dm_routes.insert(account_id.clone(), dm_route);
        }
        Route::DmMediaViewerRoute {
            backend,
            instance_id,
            account_id,
            dm_id,
            ..
        } => {
            s.view.set(View::DmsFriends);
            s.active_backend.set(Some(BackendType::from_slug(backend)));
            s.active_instance_id.set(Some(instance_id.clone()));
            s.active_account_id.set(Some(account_id.clone()));
            s.selected_server.set(None);
            s.selected_channel.set(Some(dm_id.clone()));
            s.account_last_routes.insert(account_id.clone(), route_url);
        }
        Route::ServerMediaViewerRoute {
            backend,
            instance_id,
            account_id,
            server_id,
            channel_id,
            ..
        } => {
            s.view.set(View::Server);
            s.active_backend.set(Some(BackendType::from_slug(backend)));
            s.active_instance_id.set(Some(instance_id.clone()));
            s.active_account_id.set(Some(account_id.clone()));
            s.selected_server.set(Some(server_id.clone()));
            s.selected_channel.set(Some(channel_id.clone()));
            s.account_last_routes.insert(account_id.clone(), route_url);
        }
        Route::NewConversationRoute {
            backend,
            instance_id,
            account_id,
        } => {
            s.view.set(View::DmsFriends);
            s.active_backend.set(Some(BackendType::from_slug(backend)));
            s.active_instance_id.set(Some(instance_id.clone()));
            s.active_account_id.set(Some(account_id.clone()));
            s.selected_server.set(None);
            s.selected_channel.set(None);
        }
        Route::ConversationSearchRoute {
            backend,
            instance_id,
            account_id,
        } => {
            s.view.set(View::DmsFriends);
            s.active_backend.set(Some(BackendType::from_slug(backend)));
            s.active_instance_id.set(Some(instance_id.clone()));
            s.active_account_id.set(Some(account_id.clone()));
            s.selected_server.set(None);
            s.selected_channel.set(None);
        }
        Route::ServerHome {
            backend,
            instance_id,
            account_id,
            server_id,
        } => {
            s.view.set(View::Server);
            s.active_backend.set(Some(BackendType::from_slug(backend)));
            s.active_instance_id.set(Some(instance_id.clone()));
            s.active_account_id.set(Some(account_id.clone()));
            s.selected_server.set(Some(server_id.clone()));
            s.selected_channel.set(None);
            s.account_last_routes.insert(account_id.clone(), route_url);
        }
        Route::ServerChat {
            backend,
            instance_id,
            account_id,
            server_id,
            channel_id,
        } => {
            s.view.set(View::Server);
            s.active_backend.set(Some(BackendType::from_slug(backend)));
            s.active_instance_id.set(Some(instance_id.clone()));
            s.active_account_id.set(Some(account_id.clone()));
            s.selected_server.set(Some(server_id.clone()));
            s.selected_channel.set(Some(channel_id.clone()));
            s.account_last_routes.insert(account_id.clone(), route_url);
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
            s.view.set(View::Server);
            s.active_backend.set(Some(BackendType::from_slug(backend)));
            s.active_instance_id.set(Some(instance_id.clone()));
            s.active_account_id.set(Some(account_id.clone()));
            s.selected_server.set(Some(server_id.clone()));
            s.selected_channel.set(Some(channel_id.clone()));
            s.account_last_routes.insert(account_id.clone(), route_url);
        }
        Route::CreateForumPostRoute {
            backend,
            instance_id,
            account_id,
            server_id,
            channel_id,
        } => {
            s.view.set(View::Server);
            s.active_backend.set(Some(BackendType::from_slug(backend)));
            s.active_instance_id.set(Some(instance_id.clone()));
            s.active_account_id.set(Some(account_id.clone()));
            s.selected_server.set(Some(server_id.clone()));
            s.selected_channel.set(Some(channel_id.clone()));
            // Do NOT record in account_last_routes — create-post is transient
        }
        Route::ForumSearchRoute {
            backend,
            instance_id,
            account_id,
            server_id,
            channel_id,
        } => {
            s.view.set(View::Server);
            s.active_backend.set(Some(BackendType::from_slug(backend)));
            s.active_instance_id.set(Some(instance_id.clone()));
            s.active_account_id.set(Some(account_id.clone()));
            s.selected_server.set(Some(server_id.clone()));
            s.selected_channel.set(Some(channel_id.clone()));
            // Do NOT record in account_last_routes — search is transient
        }
        Route::ForumCommentsRoute {
            backend,
            instance_id,
            account_id,
            server_id,
            channel_id,
        } => {
            s.view.set(View::Server);
            s.active_backend.set(Some(BackendType::from_slug(backend)));
            s.active_instance_id.set(Some(instance_id.clone()));
            s.active_account_id.set(Some(account_id.clone()));
            s.selected_server.set(Some(server_id.clone()));
            s.selected_channel.set(Some(channel_id.clone()));
            s.account_last_routes.insert(account_id.clone(), route_url);
        }
        Route::FriendsRoute {
            backend,
            instance_id,
            account_id,
        } => {
            s.view.set(View::Friends);
            s.active_backend.set(Some(BackendType::from_slug(backend)));
            s.active_instance_id.set(Some(instance_id.clone()));
            s.active_account_id.set(Some(account_id.clone()));
            s.account_last_routes.insert(account_id.clone(), route_url);
        }
        Route::NotificationsRoute {
            backend,
            instance_id,
            account_id,
        } => {
            s.view.set(View::Notifications);
            s.active_backend.set(Some(BackendType::from_slug(backend)));
            s.active_instance_id.set(Some(instance_id.clone()));
            s.active_account_id.set(Some(account_id.clone()));
            s.selected_server.set(None);
            s.selected_channel.set(None);
            s.account_last_routes.insert(account_id.clone(), route_url);
        }
        Route::SavedItemsRoute {
            backend,
            instance_id,
            account_id,
        } => {
            s.view.set(View::DmsFriends);
            s.active_backend.set(Some(BackendType::from_slug(backend)));
            s.active_instance_id.set(Some(instance_id.clone()));
            s.active_account_id.set(Some(account_id.clone()));
            s.selected_server.set(None);
            s.selected_channel.set(None);
            s.account_last_routes.insert(account_id.clone(), route_url);
        }
        Route::SettingsRoute => {
            s.view.set(View::Settings);
            // App-level — clear account context so Bar 2 hides and no server stays "open"
            s.active_account_id.set(None);
            s.active_instance_id.set(None);
            s.active_backend.set(None);
            s.selected_server.set(None);
            s.selected_channel.set(None);
        }
        Route::SettingsSectionRoute { section: _ } => {
            s.view.set(View::Settings);
            s.active_account_id.set(None);
            s.active_instance_id.set(None);
            s.active_backend.set(None);
            s.selected_server.set(None);
            s.selected_channel.set(None);
        }
        Route::AgentRoute => {
            s.view.set(View::Agent);
            s.active_account_id.set(None);
            s.active_instance_id.set(None);
            s.active_backend.set(None);
            s.selected_server.set(None);
            s.selected_channel.set(None);
        }
        Route::AgentSectionRoute { .. } => {
            s.view.set(View::Agent);
            s.active_account_id.set(None);
            s.active_instance_id.set(None);
            s.active_backend.set(None);
            s.selected_server.set(None);
            s.selected_channel.set(None);
        }
        Route::PersonasRoute => {
            s.view.set(View::Agent);
            s.active_account_id.set(None);
            s.active_instance_id.set(None);
            s.active_backend.set(None);
            s.selected_server.set(None);
            s.selected_channel.set(None);
        }
        Route::SearchRoute => {
            s.view.set(View::Search);
            // App-level — clear account context so Bar 2 hides
            s.active_account_id.set(None);
            s.active_instance_id.set(None);
            s.active_backend.set(None);
            s.selected_server.set(None);
            s.selected_channel.set(None);
        }
        Route::AccountSettingsRoute {
            backend,
            instance_id,
            account_id,
        } => {
            s.view.set(View::Settings);
            s.active_backend.set(Some(BackendType::from_slug(backend)));
            s.active_instance_id.set(Some(instance_id.clone()));
            s.active_account_id.set(Some(account_id.clone()));
            s.selected_server.set(None);
            s.selected_channel.set(None);
            s.account_last_routes.insert(account_id.clone(), route_url);
        }
        Route::CreateServerRoute {
            backend,
            instance_id,
            account_id,
        } => {
            s.view.set(View::Settings); // Reuse Settings view — hides channel list
            s.active_backend.set(Some(BackendType::from_slug(backend)));
            s.active_instance_id.set(Some(instance_id.clone()));
            s.active_account_id.set(Some(account_id.clone()));
            s.selected_server.set(None);
            s.selected_channel.set(None);
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
            s.view.set(View::Server);
            s.active_backend.set(Some(BackendType::from_slug(backend)));
            s.active_instance_id.set(Some(instance_id.clone()));
            s.active_account_id.set(Some(account_id.clone()));
            s.selected_server.set(Some(server_id.clone()));
            s.selected_channel.set(None);
            // Do NOT record in account_last_routes — create-channel is transient
        }
        Route::ServerSettingsRoute {
            backend,
            instance_id,
            account_id,
            server_id,
        } => {
            s.view.set(View::Settings);
            s.active_backend.set(Some(BackendType::from_slug(backend)));
            s.active_instance_id.set(Some(instance_id.clone()));
            s.active_account_id.set(Some(account_id.clone()));
            s.selected_server.set(Some(server_id.clone()));
            s.selected_channel.set(None);
            s.account_last_routes.insert(account_id.clone(), route_url);
        }
        Route::ServerSettingsSectionRoute {
            backend,
            instance_id,
            account_id,
            server_id,
            ..
        } => {
            s.view.set(View::Settings);
            s.active_backend.set(Some(BackendType::from_slug(backend)));
            s.active_instance_id.set(Some(instance_id.clone()));
            s.active_account_id.set(Some(account_id.clone()));
            s.selected_server.set(Some(server_id.clone()));
            s.selected_channel.set(None);
            s.account_last_routes.insert(account_id.clone(), route_url);
        }
        Route::ChannelSettingsRoute {
            backend,
            instance_id,
            account_id,
            server_id,
            channel_id,
        } => {
            s.view.set(View::Settings);
            s.active_backend.set(Some(BackendType::from_slug(backend)));
            s.active_instance_id.set(Some(instance_id.clone()));
            s.active_account_id.set(Some(account_id.clone()));
            s.selected_server.set(Some(server_id.clone()));
            s.selected_channel.set(Some(channel_id.clone()));
            s.account_last_routes.insert(account_id.clone(), route_url);
        }
        Route::ServerOverviewRoute { backend, instance_id, account_id }
        | Route::ServerOverviewMissedRoute { backend, instance_id, account_id }
        | Route::ServerOverviewStatsRoute { backend, instance_id, account_id }
        | Route::ServerOverviewAgentsRoute { backend, instance_id, account_id } => {
            s.view.set(View::Overview);
            s.active_backend.set(Some(BackendType::from_slug(backend)));
            s.active_instance_id.set(Some(instance_id.clone()));
            s.active_account_id.set(Some(account_id.clone()));
            s.selected_server.set(None);
            s.selected_channel.set(None);
            s.account_last_routes.insert(account_id.clone(), route_url);
        }
        Route::ThreadView { backend, instance_id, account_id, .. } => {
            // Thread full-page view (mobile). Keep account/server context intact;
            // the thread_id is carried by nav.thread_panel_open (or just the URL).
            s.active_backend.set(Some(BackendType::from_slug(backend)));
            s.active_instance_id.set(Some(instance_id.clone()));
            s.active_account_id.set(Some(account_id.clone()));
            s.account_last_routes.insert(account_id.clone(), route_url);
        }
        Route::DiscoverRoute {
            backend,
            instance_id,
            account_id,
        } => {
            s.view.set(View::DiscoverCommunities);
            s.active_backend.set(Some(BackendType::from_slug(backend)));
            s.active_instance_id.set(Some(instance_id.clone()));
            s.active_account_id.set(Some(account_id.clone()));
            s.selected_server.set(None);
            s.selected_channel.set(None);
            s.account_last_routes.insert(account_id.clone(), route_url);
        }
        Route::Root | Route::PageNotFound { .. } => {
            // on_update will redirect — nothing to sync here
        }
        Route::SignupPicker | Route::ClientSignup { .. } => {
            // Signup routes are outside MainLayout — clear account context.
            s.view.set(View::Signup);
            s.active_account_id.set(None);
            s.active_instance_id.set(None);
            s.active_backend.set(None);
            s.selected_server.set(None);
            s.selected_channel.set(None);
        }
        Route::ReauthAccount { backend, instance_id, account_id } => {
            // Reauth is a full-page form like signup, but scoped to an
            // existing account — keep the account context so the page can
            // look it up in ClientManager / ChatData.
            s.view.set(View::Signup);
            s.active_backend.set(Some(BackendType::from_slug(backend)));
            s.active_instance_id.set(Some(instance_id.clone()));
            s.active_account_id.set(Some(account_id.clone()));
            s.selected_server.set(None);
            s.selected_channel.set(None);
        }
    }
    });
    // Apply settings_section override via user_prefs (field lives on UserPrefs, not NavState).
    if let (Some(section), Some(prefs)) = (settings_section_override, user_prefs) {
        prefs.batch(|p| p.settings_section = section);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::Route;

    /// Regression test: stoat `/channels/:server_id/:channel_id` must parse to
    /// `ServerChat` — not `ServerHome` — preserving the full channel segment.
    ///
    /// Before the fix, `GET /users/@me/servers` in the test-stoat server returned
    /// an empty list, so `server_account_map` never got `SRV001 → STOAT01`.
    /// `restore_server_channel` then returned `None` (backend not found for server),
    /// and `ServerChat::use_spawn_once` called `nav.replace(ServerHome)` — silently
    /// truncating the URL to the server-only form.
    ///
    /// This test verifies the URL shape the router accepts, independent of runtime
    /// backend availability.
    #[test]
    fn stoat_server_channel_url_parses_to_server_chat() {
        let url = "/stoat/localhost:9101/STOAT01/channels/SRV001/CHVOICE001";
        let route: Route = url.parse().expect("stoat channel URL must parse to a valid Route");
        match route {
            Route::ServerChat {
                ref backend,
                ref instance_id,
                ref account_id,
                ref server_id,
                ref channel_id,
            } => {
                assert_eq!(backend, "stoat");
                assert_eq!(instance_id, "localhost:9101");
                assert_eq!(account_id, "STOAT01");
                assert_eq!(server_id, "SRV001");
                assert_eq!(channel_id, "CHVOICE001", "channel_id must NOT be truncated");
            }
            other => panic!(
                "stoat channel URL parsed to wrong variant: {other:?}\n\
                 Expected Route::ServerChat — if this is Route::ServerHome the channel_id \
                 segment was dropped by the router."
            ),
        }
    }

    /// Mirror test: discord `/channels/:server_id/:channel_id` also parses to
    /// `ServerChat` with both segments intact.  The stoat and discord routes use
    /// the same generic `ServerChat` variant, so both must pass or neither will.
    #[test]
    fn discord_server_channel_url_parses_to_server_chat() {
        let url = "/discord/localhost:9102/1/channels/100/204";
        let route: Route = url.parse().expect("discord channel URL must parse to a valid Route");
        match route {
            Route::ServerChat {
                ref backend,
                ref server_id,
                ref channel_id,
                ..
            } => {
                assert_eq!(backend, "discord");
                assert_eq!(server_id, "100");
                assert_eq!(channel_id, "204", "discord channel_id must not be truncated");
            }
            other => panic!("discord channel URL parsed to wrong variant: {other:?}"),
        }
    }

    /// Verify the `Display` impl (used by `sync_route_to_app_state` for
    /// `account_last_routes`) round-trips through `parse` for the stoat channel route.
    #[test]
    fn stoat_server_chat_route_display_round_trips() {
        let original_url = "/stoat/localhost:9101/STOAT01/channels/SRV001/CHVOICE001";
        let route: Route = original_url.parse().unwrap();
        let displayed = format!("{route}");
        assert_eq!(
            displayed, original_url,
            "Route::Display must reproduce the original URL — if it drops the channel_id segment \
             then account_last_routes will store the truncated URL and future restores will land \
             on ServerHome instead of ServerChat."
        );
    }

    /// Verify that the server-only URL parses to `ServerHome` (NOT `ServerChat`).
    /// This is the URL shape that appears after the truncation bug fires — it
    /// must be a different variant, confirming the two routes are distinct.
    #[test]
    fn stoat_server_home_url_parses_to_server_home() {
        let url = "/stoat/localhost:9101/STOAT01/channels/SRV001";
        let route: Route = url.parse().expect("stoat server-home URL must parse");
        assert!(
            matches!(route, Route::ServerHome { .. }),
            "Server-only URL must parse to ServerHome, got: {route:?}"
        );
    }
}
