//! Server-domain route adapter components.
//!
//! Covers server channels, media viewer, voice, server home, friends, saved items,
//! notifications, discover, create-server, create-channel, and the thread full view.

use crate::client_manager::ClientManager;
use crate::state::{
    AppState, BatchedSignal, ChatAction, ChatLists, ChatViewState, NavState, VoiceState,
    use_spawn_once,
};
use crate::ui::account::common::{FeatureUnsupportedPlaceholder, UnsupportedFeature};
use crate::ui::account::common::discover_communities::DiscoverCommunitiesView;
use crate::ui::account::common::VoiceAccountFooter;
use crate::ui::account::{
    ChatView, DiscordForumView, ForumView, FriendsPanel, NotificationsView, SavedItemsView,
    ThreadFullView, VoiceChannelView,
};
use crate::ui::client_ui::ClientSidebar;
use crate::ui::client_ui::view::AccountOverviewView;
use crate::ui::split_shell::SplitMenuShell;
use dioxus::prelude::*;
use poly_client::ChannelType;
use poly_ui_macros::{context_menu, ui_action};

use super::Route;

// ── Layout: Server ───────────────────────────────────────────────────────────

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
pub(super) fn ServerLayout() -> Element {
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

// ── Route pages ──────────────────────────────────────────────────────────────

/// Server home — auto-selects first channel, renders chat / voice view.
///
/// On URL-restore navigation (F5, deep link) the click handler that normally
/// calls `load_server_data` never ran, so data is missing. The `use_effect`
/// below detects missing data and calls `load_server_data` to reload it.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn ServerHome(
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
) -> Element {
    let nav_state: BatchedSignal<NavState> = use_context();
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    let chat_view_state: BatchedSignal<ChatViewState> = use_context();

    // Clear stale channel context on URL-navigation before is_voice_channel is
    // computed.  Without this, deep-link / F5 to a ServerHome URL while
    // `current_channel.channel_type == Voice` (left over from a previous
    // stoat voice visit) causes `VoiceChannelView` to render immediately —
    // before `load_server_data` runs — which triggers `getUserMedia` /
    // audio-device access and hard-crashes Chromium on Linux.
    //
    // The click-based path in `account_server_bar.rs:460` applies the same
    // fix but only fires on explicit icon click, not on URL navigation.
    // This block covers the URL-navigation / deep-link case.
    //
    // We use a component-local signal so the clear fires exactly once per
    // (account_id, server_id) key change, synchronously during the render
    // that sees the new key — guaranteeing the `is_voice_channel` check
    // below reads the fresh (empty) state, not the stale one.
    //
    // IMPORTANT: use ClearActiveChannel (not ClearChannelContext) so that
    // `current_server` is NOT nulled out here.  ClearChannelContext sets
    // `current_server = None`, which makes ChannelList fall through to the
    // "channel-empty" placeholder even when nav.view == View::Server — the
    // sidebar shows "Select a channel" instead of the server channel list.
    // ClearActiveChannel clears only `current_channel`, which is enough to
    // prevent the stale Voice channel from triggering VoiceChannelView
    // (the is_voice_channel guard also checks server_matches, so a mismatched
    // current_server can never activate VoiceChannelView for the wrong server).
    let mut cleared_key: Signal<String> = use_signal(|| String::new());
    let clear_key = format!("{account_id}|{server_id}");
    if *cleared_key.peek() != clear_key {
        cleared_key.set(clear_key.clone());
        chat_view_state.batch(|cv| cv.apply(ChatAction::ClearActiveChannel));
        chat_lists.batch(|cl| cl.set_channels(Vec::new()));
    }

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
    //
    // STARTUP RACE — backend not yet registered:
    // On a cold boot / deep-link / F5, `restore_native_accounts` runs
    // asynchronously after `is_setup_complete = true` causes the Router to
    // mount.  `ServerHome` can therefore render (and fire use_spawn_once)
    // BEFORE the stoat/matrix/discord backend is registered in ClientManager.
    // `load_server_data_internal` would then call `get_backend_for_server` →
    // None → early return → current_server stays None → sidebar shows the
    // fallback placeholder indefinitely.
    //
    // Fix: include a `backend_registered` flag in the spawn key.  The
    // `client_manager.read()` call subscribes ServerHome to ClientManager
    // changes, so when account_restore finishes and calls
    // `client_manager.batch(|cm| cm.commit_backend_account(...))`, ServerHome
    // re-renders with backend_registered=true, the key changes, and
    // use_spawn_once fires a second spawn that succeeds this time.
    //
    // The Teams‐style "team_id ∉ server_account_map" case is unaffected:
    // the Teams backend IS registered (backend_registered=true) so the key is
    // stable at "backend=true" after the first successful spawn; use_spawn_once
    // never re-fires for that key regardless of load_server_data returning early.
    let backend_registered = client_manager.read() // poly-lint: allow render-time-read — startup-race guard; subscription on client_manager intentional so ServerHome re-renders when the backend is registered
        .get_backend(account_id.as_str())
        .is_some();
    let spawn_key = format!("{account_id}|{server_id}|br={backend_registered}");
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
                crate::ui::favorites_sidebar::load_server_shell_data(
                    sid,
                    nav_state,
                    client_manager,
                    chat_lists,
                    chat_view_state,
                )
                .await;
            } else {
                crate::ui::favorites_sidebar::load_server_data(
                    sid,
                    nav_state,
                    client_manager,
                    chat_lists,
                    chat_view_state,
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
        let cv = chat_view_state.read(); // poly-lint: allow render-time-read — render snapshot; subscription intentional
        let cl = chat_lists.read(); // poly-lint: allow render-time-read — render snapshot; subscription intentional
        let server_matches = cv.current_server.as_ref().is_some_and(|s| s.id == server_id);
        let is_voice = server_matches
            && cv.current_channel.as_ref().is_some_and(|ch| matches!(ch.channel_type, ChannelType::Voice | ChannelType::Video));
        let is_forum = server_matches
            && cv.current_server.as_ref().is_some_and(|s| {
                // poly-lint: allow render-time-read — capability lookup on slug, not a signal subscription
                let slug = s.backend.as_str();
                client_manager.peek().capabilities_for_slug(slug).is_forum_layout()
            });
        // Empty server: server loaded but channels list is empty AND we're
        // not still in the initial loading window. Without this branch,
        // ChatView renders blank, which on a stale-deep-link redirect to
        // ServerHome (see ServerChat use_effect) leaves the user staring at
        // an empty pane with no explanation.
        let is_empty = server_matches && cl.channels.is_empty() && !cv.loading;
        (is_voice, is_forum, is_empty)
    };

    rsx! {
        if is_empty_server {
            div { class: "empty-state special-page-empty-state",
                h3 { "{crate::i18n::t(\"server-empty-title\")}" }
                p { "{crate::i18n::t(\"server-empty-body\")}" }
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
pub(super) fn ServerChat(
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
    channel_id: String,
) -> Element {
    let voice_state: BatchedSignal<VoiceState> = use_context();
    let app_state: BatchedSignal<AppState> = use_context();
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    let chat_view_state: BatchedSignal<ChatViewState> = use_context();
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
            let resolved_channel_id = crate::ui::favorites_sidebar::restore_server_channel(
                sid,
                cid.clone(),
                app_state,
                client_manager,
                voice_state,
                chat_lists,
                chat_view_state,
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
        let cl_snap = chat_lists.read(); // poly-lint: allow render-time-read — render snapshot; subscription intentional
        let cv_snap = chat_view_state.read(); // poly-lint: allow render-time-read — render snapshot; subscription intentional
        cl_snap
            .channels
            .iter()
            .find(|ch| ch.id == route_channel_id)
            .map(|ch| ch.channel_type)
            .or_else(|| cv_snap.current_channel.as_ref().map(|ch| ch.channel_type))
    };

    let is_forum_backend = chat_view_state.read().current_server.as_ref() // poly-lint: allow render-time-read — render snapshot; subscription intentional
        .is_some_and(|s| client_manager.peek().capabilities_for_slug(s.backend.as_str()).is_forum_layout());
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
            crate::ui::code_explorer::CodeExplorerView { route_channel_id: route_channel_id.clone() }
        } else if is_discord_forum {
            DiscordForumView {}
        } else if is_lemmy_forum {
            ForumView {}
        } else {
            ChatView {}
        }
    }
}

#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn ServerMediaViewerRoute(
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
    channel_id: String,
    message_id: String,
    attachment_index: usize,
) -> Element {
    let voice_state: BatchedSignal<VoiceState> = use_context();
    let app_state: BatchedSignal<AppState> = use_context();
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    let chat_view_state: BatchedSignal<ChatViewState> = use_context();
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
                let cv_snap = chat_view_state.peek();
                let cl_snap = chat_lists.peek();
                cv_snap
                    .current_server
                    .as_ref()
                    .is_some_and(|server| server.id == sid)
                    && cv_snap
                        .current_channel
                        .as_ref()
                        .is_some_and(|ch| ch.id == cid && ch.server_id == sid)
                    && cl_snap.channels.iter().any(|ch| ch.id == cid)
            };
            if already_loaded {
                return;
            }

            let resolved_channel_id = crate::ui::favorites_sidebar::restore_server_channel(
                sid,
                cid.clone(),
                app_state,
                client_manager,
                voice_state,
                chat_lists,
                chat_view_state,
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
        crate::ui::account::common::MessageMediaViewerOverlay {
            channel_id: overlay_channel_id,
            message_id: overlay_message_id,
            attachment_index,
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
pub(super) fn FriendsRoute(backend: String, instance_id: String, account_id: String) -> Element {
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let caps = client_manager.peek().capabilities_for_slug(&backend);
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
pub(super) fn SavedItemsRoute(backend: String, instance_id: String, account_id: String) -> Element {
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
pub(super) fn ServerOverviewRoute(backend: String, instance_id: String, account_id: String) -> Element {
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
pub(super) fn ServerOverviewMissedRoute(backend: String, instance_id: String, account_id: String) -> Element {
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
pub(super) fn ServerOverviewStatsRoute(backend: String, instance_id: String, account_id: String) -> Element {
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
pub(super) fn ServerOverviewAgentsRoute(backend: String, instance_id: String, account_id: String) -> Element {
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
pub(super) fn NotificationsRoute(backend: String, instance_id: String, account_id: String) -> Element {
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let caps = client_manager.peek().capabilities_for_slug(&backend);
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

/// Discover Communities — account-scoped route for searching communities.
///
/// Capability-gated: only shown when `backend_capabilities().community_search != None`.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn DiscoverRoute(backend: String, instance_id: String, account_id: String) -> Element {
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let caps = client_manager.peek().capabilities_for_slug(&backend);
    if matches!(caps.community_search, poly_client::CommunitySearchSupport::None) {
        return rsx! {
            FeatureUnsupportedPlaceholder {
                backend_slug: backend.clone(),
                feature: UnsupportedFeature::Discover,
            }
        };
    }
    rsx! {
        DiscoverCommunitiesView { account_id, instance_id, backend_slug: backend }
    }
}

/// Create Server — `/:backend/:instance_id/:account_id/create-server`.
///
/// Full-page form inside MainLayout (both FavoritesBar + AccountServerBar remain
/// visible on the left). Delegates to [`crate::ui::create_server::CreateServerPage`].
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
pub(super) fn CreateServerRoute(backend: String, instance_id: String, account_id: String) -> Element {
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
        crate::ui::create_server::CreateServerPage { backend, instance_id, account_id }
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
pub(super) fn CreateChannelRoute(
    backend: String,
    instance_id: String,
    account_id: String,
    server_id: String,
) -> Element {
    rsx! {
        crate::ui::create_channel::CreateChannelPage { backend, instance_id, account_id, server_id }
    }
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
pub(super) fn ThreadView(
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
