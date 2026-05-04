//! Favorites bar — account icons + favorited server icons (Bar 1).
//!
//! This is the **leftmost sidebar column** (Bar 1), always visible.
//!
//! Shows:
//! 1. Account icons (top) — one per active backend account, click to switch
//!    - Shows unread badge (total DMs + friend requests + mentions)
//! 2. Separator
//! 3. Favorited server icons from ALL accounts (cross-account)
//! 4. Spacer
//! 5. Global Search button
//! 6. App Settings button
//!
//! Demo lifecycle management (toggle, event streaming) has moved to
//! [`crate::ui::demo`]. The demo toggle button now lives in the dynamically-
//! registered [`crate::ui::settings::plugin_settings::DemoPluginSettings`] page.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.

use crate::state::BatchedSignal;
use super::routes::Route;
use crate::client_manager::{BackendHandleExt, ClientManager};
use crate::i18n::t;
use crate::state::chat_data::user_color;
use crate::state::{AccountContextMenuState, AppState, ChatData, ContextMenuState, DragSource, View};
use crate::ui::context_menu::menus::{account_entry_at, server_icon_entry_at};
use crate::ui::account::common::chat_history::{
    initial_message_query, remember_message_list_scroll_position,
    request_restore_scroll_position_or_bottom,
};
use crate::ui::actions::{ActionCx, UiAction};
use crate::ui::main_layout::{close_mobile_drawer, mobile_left_drawer_open};
use dioxus::prelude::*;
use poly_client::{AccountPresence, ConnectionStatus};
use poly_ui_macros::{context_menu, ui_action};

/// Actions for the favorites sidebar (Bar 1).
#[derive(Debug, Clone)]
pub enum FavoritesBarAction {
    /// User clicked an account icon to switch accounts.
    SwitchAccount(String),
    /// User clicked the global search button.
    OpenSearch,
    /// User clicked the app settings button.
    OpenSettings,
    /// User dropped a server onto the favorites bar.
    DropServer(String),
    /// User dragged a favorite server to reorder it.
    ReorderFavorite { drag_id: String, target_id: String },
    /// User reordered account icons via drag-and-drop.
    ReorderAccount { drag_id: String, target_id: String },
}

impl UiAction for FavoritesBarAction {
    fn apply(self, _cx: ActionCx<'_>) {
        todo!("phase-E: FavoritesBarAction requires Signal + async handles");
    }
}

/// Spacer that reserves room for the native back/forward nav-bar (desktop/mobile).
/// On web, the browser provides its own back/forward buttons so no space is needed.
#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
#[allow(non_snake_case)]
fn NavBarSpacer() -> Element {
    #[cfg(feature = "native-nav")]
    return rsx! {
        div { class: "nav-bar-spacer" }
    };
    #[cfg(not(feature = "native-nav"))]
    rsx! {}
}

/// Custom hover tooltip for sidebar icons. Opens to the right of the icon
/// (or left when `mirror_menu_layout` is enabled). Uses `position: fixed`
#[context_menu(inherit)]
/// so it escapes overflow-hidden scroll containers.
#[ui_action(inherit)]
#[component]
pub(crate) fn SidebarTooltip(
    /// First row: account name or server name
    line1: String,
    /// Optional second row: backend type (for accounts) or account name (for servers)
    line2: Option<String>,
    /// Optional third row: backend type (only for server icons)
    line3: Option<String>,
    /// Override the CSS class (e.g. to add `sidebar-tooltip-visible` for signal-driven visibility)
    extra_class: Option<String>,
) -> Element {
    let class = extra_class.as_deref().unwrap_or("sidebar-tooltip");
    rsx! {
        div { class: "{class}",
            span { class: "sidebar-tooltip-line sidebar-tooltip-name", "{line1}" }
            if let Some(ref l2) = line2 {
                span { class: "sidebar-tooltip-line sidebar-tooltip-type", "{l2}" }
            }
            if let Some(ref l3) = line3 {
                span { class: "sidebar-tooltip-line sidebar-tooltip-type", "{l3}" }
            }
        }
    }
}

/// Favorites Bar component — **Favorites Bar** (Bar 1).
///
/// Shows: Account icons, separator, favorited server icons with
/// source badge, spacer, Demo toggle, App Settings.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(FavoritesBarAction)]
#[component]
#[allow(non_snake_case)]
pub fn FavoritesBar() -> Element {
    let app_state: BatchedSignal<AppState> = use_context();
    let current_view = *app_state.read().nav.view;
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let chat_data: BatchedSignal<ChatData> = use_context();

    let servers = chat_data.read().servers.clone();
    let demo_active = client_manager.read().demo_active;
    let active_account = app_state.read().nav.active_account_id.cloned();
    let _active_backend_slug = app_state
        .read()
        .nav
        .active_backend
        .cloned()
        .map(|b| b.slug().to_string());
    let _active_instance_id = app_state.read().nav.active_instance_id.cloned();

    // Collect distinct active account IDs for account icons, applying the
    // user-saved order from `ChatData.account_order` (hydrated at startup
    // from `AppSettings.account_order`). Accounts not listed in the saved
    // order are appended by a priority fallback so the default install is
    // predictable and groups related accounts together:
    //   0. demo messenger accounts (Cat, Dog)
    //   1. demo forum accounts (Platypus)
    //   2. every other account (user-added: HN, poly, etc.), alphabetical
    // This ordering is what a brand-new user expects: play with the demo
    // messengers first, see the forum variant next, then their own accounts.
    let account_ids = {
        let live: Vec<String> = client_manager.read().active_account_ids();
        let saved_order = chat_data.read().account_order.clone();
        let live_set: std::collections::HashSet<_> = live.iter().cloned().collect();
        let mut ordered: Vec<String> = saved_order
            .iter()
            .filter(|id| live_set.contains(*id))
            .cloned()
            .collect();
        let placed: std::collections::HashSet<_> = ordered.iter().cloned().collect();
        let cd = chat_data.read();
        let priority = |id: &String| -> u8 {
            match cd.account_sessions.get(id) {
                Some(s) if s.backend == "demo" => 0,
                Some(s) if s.backend == "demo_forum" => 1,
                _ => 2,
            }
        };
        let mut rest: Vec<String> =
            live.into_iter().filter(|id| !placed.contains(id)).collect();
        rest.sort_by(|a, b| priority(a).cmp(&priority(b)).then_with(|| a.cmp(b)));
        ordered.extend(rest);
        ordered
    };

    // Only show servers that have been dragged into favorites.
    let favorited_ids = chat_data.read().favorited_server_ids.clone();
    // Preserve the order from favorited_ids list.
    let favorite_servers: Vec<_> = favorited_ids
        .iter()
        .filter_map(|id| servers.iter().find(|s| &s.id == id))
        .cloned()
        .collect();

    // Local signal for drop-zone highlight state.
    let mut drag_over = use_signal(|| false);

    // Global tooltip show/hide + positioning for ALL sidebar icons.
    // Uses document-level mouseover/mouseout (which bubble) so it works
    // for both Bar 1 (.server-sidebar) and Bar 2 (.account-server-bar),
    // including draggable server icons where CSS :hover is unreliable.
    use_effect(move || {
        let _ = dioxus::prelude::document::eval(r#"
            (function() {
                if (document._sidebarTooltipInit) return;
                document._sidebarTooltipInit = true;
                function portalTip(icon) {
                    var tip = icon._tooltipEl;
                    if (tip) return tip;
                    tip = icon.querySelector('.sidebar-tooltip');
                    if (!tip) return null;
                    document.body.appendChild(tip);
                    icon._tooltipEl = tip;
                    return tip;
                }
                document.addEventListener('mouseover', function(e) {
                    var icon = e.target.closest('.server-icon');
                    if (!icon) return;
                    var tip = portalTip(icon);
                    if (!tip) return;
                    var r = icon.getBoundingClientRect();
                    var mirrored = document.querySelector('.poly-app.poly-menu-mirrored') !== null;
                    tip.style.top = (r.top + r.height / 2) + 'px';
                    if (mirrored) {
                        tip.style.right = (window.innerWidth - r.left + 12) + 'px';
                        tip.style.left = 'auto';
                    } else {
                        tip.style.left = (r.right + 12) + 'px';
                        tip.style.right = 'auto';
                    }
                    tip.style.display = 'flex';
                });
                document.addEventListener('mouseout', function(e) {
                    var icon = e.target.closest('.server-icon');
                    if (!icon) return;
                    var related = e.relatedTarget;
                    if (related && icon.contains(related)) return;
                    var tip = icon._tooltipEl;
                    if (tip) tip.style.display = 'none';
                });
            })()
        "#);
    });

    rsx! {
        nav { class: "server-sidebar",
            // Scrollable content area (accounts, favorites, spacer)
            div { class: "sidebar-scroll-area",
                // Allow drops from Bar 2 server icons.
                ondragover: move |evt| {
                    evt.prevent_default();
                    drag_over.set(true);
                },
                ondragleave: move |_| drag_over.set(false),
                ondrop: move |evt| {
                    evt.prevent_default();
                    drag_over.set(false);
                    let new_favorites = chat_data.batch(|cd| {
                        let drag_id = cd.dragging_server_id.clone();
                        let drag_src = cd.drag_source.clone();
                        // Per-item ondrop handles positional drops via stop_propagation.
                        // This handler catches drops on the nav background (append to end).
                        if let Some(sid) = drag_id {
                            match drag_src {
                                DragSource::AccountServer | DragSource::FavoriteServer => {
                                    if !cd.favorited_server_ids.contains(&sid) {
                                        cd.favorited_server_ids.push(sid);
                                    }
                                }
                                DragSource::None | DragSource::AccountIcon => {}
                            }
                        }
                        cd.dragging_server_id = None;
                        cd.drag_source = DragSource::None;
                        cd.drag_over_id = None;
                        cd.favorited_server_ids.clone()
                    });
                    spawn(async move {
                        persist_favorites(new_favorites).await;
                    });
                },
                class: if drag_over() { "drag-over" } else { "" },
                
                NavBarSpacer {}

                // ── Account icons (one per active account) ────────────────
                for aid in &account_ids {
                    AccountIcon {
                        account_id: aid.clone(),
                        is_active: active_account.as_deref() == Some(aid.as_str()),
                    }
                }

                // Separator (between accounts and favorites)
                if !account_ids.is_empty() {
                    div { class: "sidebar-separator" }
                }

                // ── Favorited servers (dragged in from Bar 2) ─────────────
                for server in &favorite_servers {
                    {
                        let instance_id = chat_data
                            .read()
                            .account_sessions
                            .get(&server.account_id)
                            .map_or_else(|| "demo".to_string(), |s| s.instance_id.clone());
                        rsx! {
                            FavoriteServerIcon {
                                server_id: server.id.clone(),
                                server_name: server.name.clone(),
                                backend_slug: server.backend.slug().to_string(),
                                instance_id,
                                account_id: server.account_id.clone(),
                                account_display_name: server.account_display_name.clone(),
                                backend_name: server.backend.display_name().to_string(),
                                unread: if client_manager.peek().capabilities_for_slug(server.backend.as_str()).is_forum_layout() { 0 } else { server.unread_count },
                                mention: if client_manager.peek().capabilities_for_slug(server.backend.as_str()).is_forum_layout() { 0 } else { server.mention_count },
                                icon_url: server.icon_url.clone(),
                            }
                        }
                    }
                }

                // Drop hint — shown only when no favorites yet.
                if favorite_servers.is_empty() && demo_active {
                    div { class: "favorites-drop-hint",
                        span { "← Drag servers here" }
                    }
                }

                // Spacer (flex: 1 pushes footer to bottom)
                div { class: "sidebar-spacer" }
            }

            // Footer: search + agent + settings buttons float at bottom
            div { class: "sidebar-footer",
                // Global Search button — ALWAYS truly global, regardless of
                // which account the user came from. Per-account scoping
                // happens inside the search page via the AccountFilter
                // toggles. The previous context-aware nav routed to
                // AccountSearchRoute when on an account page, which created
                // a surprising "I clicked the global search but it's
                // pre-scoped" UX. The global magnifier should mean global.
                {
                    let is_search = current_view == View::Search;
                    rsx! {
                        div {
                            class: if is_search { "server-icon active" } else { "server-icon" },
                            onclick: move |_| {
                                close_mobile_drawer();
                                crate::nav!(Route::SearchRoute);
                            },
                            title: "{t(\"nav-search\")}",
                            div { class: "icon-search", "🔍" }
                        }
                    }
                }

                // Agent button — navigates to the Agent page (integrations + profile)
                {
                    let is_agent = current_view == View::Agent;
                    rsx! {
                        div {
                            class: if is_agent { "server-icon active" } else { "server-icon" },
                            onclick: move |_| {
                                close_mobile_drawer();
                                crate::nav!(Route::AgentRoute);
                            },
                            title: "{t(\"nav-agent\")}",
                            div { class: "icon-agent", "🤖" }
                        }
                    }
                }

                // App Settings button — only "active" for app-level settings (no account scoped)
                {
                    let is_app_settings = current_view == View::Settings && active_account.is_none();
                    rsx! {
                        div {
                            class: if is_app_settings { "server-icon active" } else { "server-icon" },
                            onclick: move |_| {
                                close_mobile_drawer();
                                crate::nav!(Route::SettingsRoute);
                            },
                            title: "{t(\"nav-settings\")}",
                            div { class: "icon-settings", "⚙" }
                        }
                    }
                }
            }
        }
    }
}

/// Single account icon in the favorites bar.
///
/// Shows a colored circle with the account's emoji icon (if set in its session)
/// or first character of the account ID as fallback. Clicking navigates to that
/// account's last visited page (or DMs home if no history exists).
#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn AccountIcon(account_id: String, is_active: bool) -> Element {
    let chat_data: BatchedSignal<ChatData> = use_context();
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let app_state: BatchedSignal<AppState> = use_context();

    // Read connection and presence statuses for this account.
    let conn_class: &'static str = client_manager
        .read()
        .connection_statuses
        .get(&account_id)
        .map_or("disconnected", ConnectionStatus::css_class);
    let presence_class: &'static str = client_manager
        .read()
        .presence_statuses
        .get(&account_id)
        .copied()
        .unwrap_or(AccountPresence::Online)
        .css_class();

    let is_forum_account = chat_data
        .read()
        .account_sessions
        .get(&account_id)
        .is_some_and(|s| client_manager.peek().capabilities_for_slug(s.backend.as_str()).is_forum_layout());

    let color = user_color(&account_id);

    // Determine avatar URL: real accounts use user.avatar_url; demo accounts
    // get locally bundled cat/dog images; others fall back to icon_emoji text.
    let avatar_url: Option<String> = chat_data
        .read()
        .account_sessions
        .get(&account_id)
        .and_then(|s| s.user.avatar_url.clone());

    // Display name shown in the tooltip when hovering the account icon.
    let display_name: String = chat_data
        .read()
        .account_sessions
        .get(&account_id)
        .map_or_else(|| account_id.clone(), |s| s.user.display_name.clone());

    let backend_name: String = chat_data
        .read()
        .account_sessions
        .get(&account_id)
        .map_or_else(|| "Unknown".to_string(), |s| s.backend.display_name().to_string());

    // Use icon_emoji from session if available, else fall back to the first
    // letter of the account's display name (NOT the account_id, which starts
    // with the backend slug — e.g. all Lemmy accounts would collapse to "L").
    let icon_label: String = chat_data
        .read()
        .account_sessions
        .get(&account_id)
        .and_then(|s| s.icon_emoji.clone())
        .unwrap_or_else(|| {
            display_name
                .chars()
                .next()
                .map(|c| c.to_uppercase().to_string())
                .unwrap_or_default()
        });

    // Show unread notification count only — matches the bell badge in account server bar.
    // DM unread counts are surfaced separately in Bar 2.
    let total_unreads = u32::try_from(
        chat_data
            .read()
            .notifications
            .iter()
            .filter(|n| n.account_id == account_id)
            .count(),
    )
    .unwrap_or(u32::MAX);

    // Resolve backend slug and instance_id for routing — read from the session.
    let aid_for_click = account_id.clone();

    // Connection icon emoji for connection status
    let conn_icon = match conn_class {
        "connected" => "⚡",
        "connecting" => "↺",
        "disconnected" => "—",
        "unauthenticated" => "🔑",
        _ => "⚠",
    };
    let needs_reauth_badge = conn_class == "unauthenticated";

    let is_drag_over_account = chat_data.read().drag_over_id.as_deref()
        == Some(account_id.as_str())
        && chat_data.read().drag_source == DragSource::AccountIcon;

    let drag_start_id = account_id.clone();
    let on_account_drag_start = move |_: Event<DragData>| {
        chat_data.batch(|cd| {
            cd.dragging_server_id = Some(drag_start_id.clone());
            cd.drag_source = DragSource::AccountIcon;
        });
    };

    let drag_over_id = account_id.clone();
    let on_account_drag_over = move |evt: Event<DragData>| {
        if chat_data.read().drag_source != DragSource::AccountIcon {
            return;
        }
        evt.prevent_default();
        evt.stop_propagation();
        chat_data.batch(|cd| cd.drag_over_id = Some(drag_over_id.clone()));
    };

    let drag_leave_id = account_id.clone();
    let on_account_drag_leave = move |_: Event<DragData>| {
        chat_data.batch(|cd| {
            if cd.drag_over_id.as_deref() == Some(drag_leave_id.as_str()) {
                cd.drag_over_id = None;
            }
        });
    };

    let drop_target_id = account_id.clone();
    let client_manager_for_drop = client_manager;
    let on_account_drop = move |evt: Event<DragData>| {
        if chat_data.read().drag_source != DragSource::AccountIcon {
            return;
        }
        evt.prevent_default();
        evt.stop_propagation();
        let snapshot = chat_data.batch(|cd| {
            let dragging = cd.dragging_server_id.clone();
            cd.drag_over_id = None;
            let Some(drag_id) = dragging else {
                cd.dragging_server_id = None;
                cd.drag_source = DragSource::None;
                return None;
            };
            if drag_id == drop_target_id {
                cd.dragging_server_id = None;
                cd.drag_source = DragSource::None;
                return None;
            }
            // Seed account_order from the live accounts if empty so we have
            // something to reorder against. Live list is sorted so the
            // baseline is deterministic.
            if cd.account_order.is_empty() {
                let mut live: Vec<String> = client_manager_for_drop
                    .read()
                    .active_account_ids();
                live.sort();
                cd.account_order = live;
            }
            // Ensure both dragged + target are present in the order vec.
            if !cd.account_order.contains(&drag_id) {
                cd.account_order.push(drag_id.clone());
            }
            if !cd.account_order.contains(&drop_target_id) {
                cd.account_order.push(drop_target_id.clone());
            }
            if let Some(from) = cd.account_order.iter().position(|x| x == &drag_id) {
                cd.account_order.remove(from);
                if let Some(to) =
                    cd.account_order.iter().position(|x| x == &drop_target_id)
                {
                    cd.account_order.insert(to, drag_id);
                } else {
                    cd.account_order.push(drag_id);
                }
            }
            cd.dragging_server_id = None;
            cd.drag_source = DragSource::None;
            Some(cd.account_order.clone())
        });
        if let Some(order) = snapshot {
            spawn(async move {
                persist_account_order(order).await;
            });
        }
    };

    let on_account_drag_end = move |_: Event<DragData>| {
        chat_data.batch(|cd| {
            cd.dragging_server_id = None;
            cd.drag_source = DragSource::None;
            cd.drag_over_id = None;
        });
    };

    let account_item_class = match (is_active, is_drag_over_account) {
        (true, true) => "server-icon account-icon active drag-over-target",
        (true, false) => "server-icon account-icon active",
        (false, true) => "server-icon account-icon drag-over-target",
        (false, false) => "server-icon account-icon",
    };

    let menu_aid = account_id.clone();
    let menu_display = display_name.clone();
    let menu_app_state = app_state;
    let menu_chat_data = chat_data;
    let on_account_contextmenu = move |evt: Event<MouseData>| {
        evt.prevent_default();
        evt.stop_propagation();
        let coords = evt.client_coordinates();
        let (slug, inst) = {
            let cd = menu_chat_data.read();
            cd.account_sessions
                .get(&menu_aid)
                .map_or_else(
                    || ("demo".to_string(), "demo".to_string()),
                    |s| (s.backend.slug().to_string(), s.instance_id.clone()),
                )
        };
        menu_app_state.batch(|st| {
            st.context_menu_stack.push(account_entry_at(
                AccountContextMenuState {
                    x: coords.x,
                    y: coords.y,
                    account_id: menu_aid.clone(),
                    display_name: menu_display.clone(),
                    backend_slug: slug,
                    instance_id: inst,
                },
                coords.x,
                coords.y,
            ));
        });
    };

    rsx! {
        div {
            class: "{account_item_class}",
            draggable: "true",
            oncontextmenu: on_account_contextmenu,
            ondragstart: on_account_drag_start,
            ondragover: on_account_drag_over,
            ondragleave: on_account_drag_leave,
            ondrop: on_account_drop,
            ondragend: on_account_drag_end,
            onclick: move |_| {
                let aid = aid_for_click.clone();
                let preserve_drawer_context = mobile_left_drawer_open();

                // If this account needs reauth, skip the normal route resolution —
                // sending it through NotificationsRoute / ServerHome would mount a
                // component that spins waiting on a backend that no longer exists,
                // freezing the UI. Route to the account-scoped reauth page so the
                // existing token is updated (or the account removed) in place.
                let needs_reauth = {
                    let cm = client_manager.read();
                    cm.connection_statuses
                        .get(&aid)
                        .is_some_and(ConnectionStatus::needs_reauth)
                };
                if needs_reauth {
                    let info = chat_data
                        .read()
                        .account_sessions
                        .get(&aid)
                        .map(|s| (s.backend.slug().to_string(), s.instance_id.clone()));
                    if let Some((slug, instance_id)) = info {
                        // Session.instance_id may be stored with scheme (e.g.
                        // "http://localhost:9106") when restored from the
                        // persisted AccountToken. Route path segments cannot
                        // contain `//`, so normalize here.
                        let instance_id = instance_id
                            .trim_start_matches("https://")
                            .trim_start_matches("http://")
                            .trim_end_matches('/')
                            .to_string();
                        crate::nav!(Route::ReauthAccount {
                            backend: slug,
                            instance_id,
                            account_id: aid.clone(),
                        });
                        return;
                    }
                }

                // Clear server/channel state — the target route will reload what's needed.
                // Batch into one write guard so only a single Dioxus re-render cascade
                // is scheduled (5 separate writes → 5 cascades → WASM scheduler freeze).
                chat_data.batch(|cd| {
                    cd.current_server = None;
                    cd.current_channel = None;
                    cd.channels.clear();
                    cd.messages.clear();
                    cd.members.clear();
                });

                // If we have a stored last route for this account, restore it.
                // This makes account-switching feel like a true tab switch.
                //
                // EXCEPT — if the stored route is a DMs/Friends/Notifications
                // route but the backend doesn't actually support that surface,
                // ignore it and fall through to the capability-based default.
                // Without this guard, a one-off visit to a non-supported page
                // (manual URL nav, deep-link, stale persisted state from an
                // earlier capability shape) leaves the account permanently
                // landing there — e.g. demo_forum opening on an empty Direct
                // Messages page even though demo_forum has has_dms == false.
                if !preserve_drawer_context {
                    let last_route_url = app_state
                        .read()
                        .nav
                        .account_last_routes
                        .get(&aid)
                        .cloned();
                    if let Some(url) = last_route_url
                        && let Ok(route) = url.parse::<Route>()
                    {
                        let backend_slug_for_route_check = chat_data
                            .read()
                            .account_sessions
                            .get(&aid)
                            .map(|s| s.backend.slug().to_string());
                        let route_is_compatible = if let Some(slug) = backend_slug_for_route_check {
                            let caps = client_manager.peek().capabilities_for_slug(&slug);
                            let path = url.as_str();
                            let dm_path = path.contains("/dms");
                            let friends_path = path.contains("/friends");
                            let notif_path = path.contains("/notifications");
                            let needs_dms = dm_path && !caps.should_show_dms();
                            let needs_friends = friends_path && !caps.should_show_friends();
                            let needs_notif = notif_path && !caps.should_show_notifications();
                            !(needs_dms || needs_friends || needs_notif)
                        } else {
                            true
                        };
                        if route_is_compatible {
                            navigator().push(route);
                            return;
                        }
                    }
                }

                // No stored route — pick a sensible fallback based on the
                // backend's capabilities. Forum/read-only backends have no
                // DMs, so routing them to DmsHome would land on an empty
                // placeholder. Instead, drop forum accounts on their first
                // server (community) so the user sees content immediately.
                // IMPORTANT: read the signal once and extract all needed data
                // before dropping the guard; nested read() calls while an outer
                // read guard is held cause a runtime borrow panic in WASM.
                let (backend_slug, instance_id, first_server_id) = {
                    let guard = chat_data.read();
                    let (slug, inst) = if let Some(session) =
                        guard.account_sessions.get(&aid)
                    {
                        (session.backend.slug().to_string(), session.instance_id.clone())
                    } else {
                        let slug = guard
                            .servers
                            .iter()
                            .find(|s| s.account_id == aid)
                            .map_or_else(|| "demo".to_string(), |s| s.backend.slug().to_string());
                        (slug, "demo".to_string())
                    };
                    let first_server = guard
                        .servers
                        .iter()
                        .find(|s| s.account_id == aid)
                        .map(|s| s.id.clone());
                    (slug, inst, first_server)
                };
                // Belt-and-suspenders: strip any URL scheme that may have been
                // persisted in session.instance_id before the fix landed
                // (stored as backend_url which includes "http://...").
                // Route path segments cannot contain "://" — a scheme-inclusive
                // instance_id parses as PageNotFound and triggers an infinite
                // redirect + app_state.write() cascade (CLAUDE.md hang class #1).
                let instance_id = instance_id
                    .trim_start_matches("https://")
                    .trim_start_matches("http://")
                    .trim_end_matches('/')
                    .to_string();
                let caps = client_manager.peek().capabilities_for_slug(&backend_slug);
                let fallback_route = match caps.landing {
                    poly_client::LandingPage::Overview => {
                        Route::ServerOverviewRoute {
                            backend: backend_slug,
                            instance_id,
                            account_id: aid,
                        }
                    }
                    poly_client::LandingPage::FirstServer => {
                        if let Some(server_id) = first_server_id {
                            Route::ServerHome {
                                backend: backend_slug,
                                instance_id,
                                account_id: aid,
                                server_id,
                            }
                        } else {
                            Route::NotificationsRoute {
                                backend: backend_slug,
                                instance_id,
                                account_id: aid,
                            }
                        }
                    }
                    poly_client::LandingPage::DirectMessages => {
                        Route::DmsHome {
                            backend: backend_slug,
                            instance_id,
                            account_id: aid,
                        }
                    }
                };
                navigator().push(fallback_route);
            },
            // Render image avatar if available (avatar_url is set by the client;
            // demo client sets it to the bundled cat/dog asset path).
            if let Some(url) = &avatar_url {
                img {
                    src: "{url}",
                    class: "server-icon-image",
                    alt: "{account_id}",
                }
            } else {
                div {
                    class: "server-icon-letter",
                    style: "background-color: {color};",
                    "{icon_label}"
                }
            }
            // Bottom-left: connection status emoji icon (not shown for forum accounts,
            // unless the account needs reauthentication — then always show).
            if !is_forum_account {
                span {
                    class: "account-conn-icon account-conn-icon--{conn_class}",
                    "{conn_icon}"
                }
            } else if needs_reauth_badge {
                span {
                    class: "account-conn-icon account-conn-icon--unauthenticated",
                    title: "Sign in again",
                    "🔑"
                }
            }
            // Top-left: notification count badge.
            // Hidden for forum accounts by default (too chatty), but ALWAYS shown
            // when a reauth prompt is waiting — otherwise the user has no in-sidebar
            // cue that the 🔑 icon maps to a clickable notification.
            if (!is_forum_account || needs_reauth_badge) && total_unreads > 0 {
                span {
                    class: "badge mention-count-badge",
                    "{total_unreads}"
                }
            }
            // Bottom-right: presence dot (not shown for forum accounts)
            if !is_forum_account {
                span {
                    class: "status-dot presence-dot {presence_class}",
                }
            }
            SidebarTooltip {
                line1: display_name.clone(),
                line2: Some(backend_name),
                line3: None,
            }
        }
    }
}

/// Single favorited server icon in the favorites bar.
///
/// Supports:
/// - Click to navigate to the server
/// - Right-click to open the server context menu
/// - Drag to reorder within Bar 1 or move back (drag is tracked via `DragSource::FavoriteServer`)
/// - Accept drops from Bar 2 (`DragSource::AccountServer`) for positional insertion
#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn FavoriteServerIcon(
    server_id: String,
    server_name: String,
    backend_slug: String,
    /// Federated homeserver instance ID (mirrors `:instance_id` URL segment).
    instance_id: String,
    account_id: String,
    account_display_name: String,
    backend_name: String,
    unread: u32,
    /// Number of @mention notifications (shown as red badge).
    mention: u32,
    /// Optional server icon URL. When `Some`, rendered as an `<img>`; when
    /// `None`, falls back to a colored first-letter placeholder.
    icon_url: Option<String>,
) -> Element {
    let app_state: BatchedSignal<AppState> = use_context();
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let chat_data: BatchedSignal<ChatData> = use_context();

    // Get account's connection and presence status
    let _account_conn_class: &'static str = client_manager
        .read()
        .connection_statuses
        .get(&account_id)
        .map_or("disconnected", ConnectionStatus::css_class);
    let account_presence_class: &'static str = client_manager
        .read()
        .presence_statuses
        .get(&account_id)
        .copied()
        .unwrap_or(AccountPresence::Online)
        .css_class();

    // Connection icon emoji for account connection status
    let conn_icon = match _account_conn_class {
        "connected" => "⚡",
        "connecting" => "↺",
        "disconnected" => "—",
        "unauthenticated" => "🔑",
        _ => "⚠",
    };
    let server_needs_reauth = _account_conn_class == "unauthenticated";

    let is_selected = app_state.read().nav.selected_server.as_deref() == Some(&server_id);
    let is_drag_over = chat_data.read().drag_over_id.as_deref() == Some(server_id.as_str());
    let first_letter: String = server_name
        .chars()
        .next()
        .map(|c| c.to_string())
        .unwrap_or_default();
    let icon_color = user_color(&server_id);

    // Determine source badge: account's avatar URL
    let account_avatar_url: Option<String> = chat_data
        .read()
        .account_sessions
        .get(&account_id)
        .and_then(|s| s.user.avatar_url.clone());

    let item_class = match (is_selected, is_drag_over) {
        (true, true) => "server-icon active drag-over-target",
        (true, false) => "server-icon active",
        (false, true) => "server-icon drag-over-target",
        (false, false) => "server-icon",
    };

    rsx! {
        div {
            class: "{item_class}",
            draggable: "true",
            // Click → navigate to server
            onclick: {
                let sid = server_id.clone();
                let bslug = backend_slug.clone();
                let iid = instance_id.clone();
                let aid = account_id.clone();
                move |_| {
                    let preserve_drawer_context = mobile_left_drawer_open();
                    if let Some(previous_channel_id) = app_state
                        .read()
                        .nav
                        .selected_channel
                        .cloned()
                    {
                        remember_message_list_scroll_position(&previous_channel_id);
                    }
                    if !preserve_drawer_context {
                        let sid2 = sid.clone();
                        spawn(async move {
                            load_server_data(sid2, app_state, client_manager, chat_data).await;
                        });
                    }
                    navigator()
                        .push(Route::ServerHome {
                            backend: bslug.clone(),
                            instance_id: iid.clone(),
                            account_id: aid.clone(),
                            server_id: sid.clone(),
                        });
                }
            },
            // Right-click → open context menu at cursor position
            oncontextmenu: {
                let sid = server_id.clone();
                let sname = server_name.clone();
                let aid = account_id.clone();
                let iid = instance_id.clone();
                let bslug = backend_slug.clone();
                move |evt: Event<MouseData>| {
                    evt.prevent_default();
                    evt.stop_propagation();
                    let coords = evt.client_coordinates();
                    app_state.batch(|st| {
                        st.context_menu_stack.push(server_icon_entry_at(
                            ContextMenuState {
                                x: coords.x,
                                y: coords.y,
                                server_id: sid.clone(),
                                server_name: sname.clone(),
                                account_id: aid.clone(),
                                instance_id: iid.clone(),
                                backend_slug: bslug.clone(),
                            },
                            coords.x,
                            coords.y,
                        ));
                    });
                }
            },
            // Drag start — mark as dragging from Bar 1
            ondragstart: {
                let sid = server_id.clone();
                move |_| {
                    chat_data.batch(|cd| {
                        cd.dragging_server_id = Some(sid.clone());
                        cd.drag_source = DragSource::FavoriteServer;
                    });
                }
            },
            // Drag over this item — highlight as drop target
            ondragover: {
                let sid = server_id.clone();
                move |evt: Event<DragData>| {
                    evt.prevent_default();
                    evt.stop_propagation();
                    chat_data.batch(|cd| cd.drag_over_id = Some(sid.clone()));
                }
            },
            // Drag leave — clear highlight if we are still the target
            ondragleave: {
                let sid = server_id.clone();
                move |_| {
                    let currently_us =
                        chat_data.read().drag_over_id.as_deref()
                        == Some(sid.as_str());
                    if currently_us {
                        chat_data.batch(|cd| cd.drag_over_id = None);
                    }
                }
            },
            // Drop on this item — reorder within Bar 1, or insert from Bar 2
            ondrop: {
                let tid = server_id.clone();
                move |evt: Event<DragData>| {
                    evt.prevent_default();
                    // Stop bubbling so the nav's ondrop doesn't double-handle
                    evt.stop_propagation();
                    let new_favorites = chat_data.batch(|cd| {
                        let dragging = cd.dragging_server_id.clone();
                        let src = cd.drag_source.clone();
                        cd.drag_over_id = None;
                        let Some(drag_id) = dragging else {
                            cd.dragging_server_id = None;
                            cd.drag_source = DragSource::None;
                            return None;
                        };
                        let target_id = tid.clone();
                        if drag_id == target_id {
                            cd.dragging_server_id = None;
                            cd.drag_source = DragSource::None;
                            return None;
                        }
                        match src {
                            DragSource::FavoriteServer => {
                                // Reorder within Bar 1: move drag_id before target_id
                                if let Some(from) = cd
                                    .favorited_server_ids
                                    .iter()
                                    .position(|x| *x == drag_id)
                                {
                                    cd.favorited_server_ids.remove(from);
                                    if let Some(to) = cd
                                        .favorited_server_ids
                                        .iter()
                                        .position(|x| *x == target_id)
                                    {
                                        cd.favorited_server_ids.insert(to, drag_id);
                                    } else {
                                        cd.favorited_server_ids.push(drag_id);
                                    }
                                }
                            }
                            DragSource::AccountServer => {
                                // Insert from Bar 2 before target position
                                if !cd.favorited_server_ids.contains(&drag_id) {
                                    if let Some(to) = cd
                                        .favorited_server_ids
                                        .iter()
                                        .position(|x| *x == target_id)
                                    {
                                        cd.favorited_server_ids.insert(to, drag_id);
                                    } else {
                                        cd.favorited_server_ids.push(drag_id);
                                    }
                                }
                            }
                            DragSource::None | DragSource::AccountIcon => {}
                        }
                        cd.dragging_server_id = None;
                        cd.drag_source = DragSource::None;
                        Some(cd.favorited_server_ids.clone())
                    });
                    if let Some(favs) = new_favorites {
                        spawn(async move {
                            persist_favorites(favs).await;
                        });
                    }
                }
            },
            // Drag end — always clean up regardless of drop target
            ondragend: move |_| {
                chat_data.batch(|cd| {
                    cd.dragging_server_id = None;
                    cd.drag_source = DragSource::None;
                    cd.drag_over_id = None;
                });
            },
            if let Some(ref url) = icon_url {
                img {
                    class: "server-icon-image",
                    src: "{url}",
                    alt: "{server_name}",
                }
            } else {
                div {
                    class: "server-icon-letter",
                    style: "background-color: {icon_color};",
                    "{first_letter}"
                }
            }
            // Source badge: show account avatar image (or fallback letter) — top-left overlay
            if let Some(url) = &account_avatar_url {
                img {
                    src: "{url}",
                    class: "source-badge-image",
                    alt: "{account_display_name}",
                }
            } else {
                span { class: "source-badge", "A" }
            }
            // Bottom-left: account connection status emoji icon (not for forum,
            // unless reauth is needed — forum accounts still surface the 🔑 badge).
            if !client_manager.peek().capabilities_for_slug(&backend_slug).is_forum_layout() {
                span {
                    class: "account-conn-icon account-conn-icon--{_account_conn_class}",
                    "{conn_icon}"
                }
            } else if server_needs_reauth {
                span {
                    class: "account-conn-icon account-conn-icon--unauthenticated",
                    title: "Sign in again",
                    "🔑"
                }
            }
            // Top-left: mention count badge
            if mention > 0 {
                span { class: "badge mention-count-badge", "@{mention}" }
            } else if unread > 0 {
                span { class: "badge mention-count-badge", "{unread}" }
            }
            // Bottom-right: presence dot (not for forum)
            if !client_manager.peek().capabilities_for_slug(&backend_slug).is_forum_layout() {
                span {
                    class: "status-dot presence-dot {account_presence_class}",
                }
            }
            SidebarTooltip {
                line1: server_name.clone(),
                line2: Some(account_display_name.clone()),
                line3: Some(backend_name.clone()),
            }
        }
    }
}

/// Load channels and select the first text channel for a server.
pub async fn load_server_data(
    server_id: String,
    app_state: BatchedSignal<AppState>,
    client_manager: BatchedSignal<ClientManager>,
    chat_data: BatchedSignal<ChatData>,
) {
    load_server_data_internal(server_id, app_state, client_manager, chat_data, true).await;
}

pub async fn load_server_shell_data(
    server_id: String,
    app_state: BatchedSignal<AppState>,
    client_manager: BatchedSignal<ClientManager>,
    chat_data: BatchedSignal<ChatData>,
) {
    load_server_data_internal(server_id, app_state, client_manager, chat_data, false).await;
}

async fn load_server_data_internal(
    server_id: String,
    app_state: BatchedSignal<AppState>,
    client_manager: BatchedSignal<ClientManager>,
    chat_data: BatchedSignal<ChatData>,
    auto_select_first_text_channel: bool,
) {
    // Show the spinner immediately so the content area doesn't render a
    // stale server while we fetch the new one. Keep this as its own
    // cascade — the UI needs to react before we start awaiting.
    chat_data.batch(|cd| cd.loading = true);

    // Find which backend owns this server
    let backend_info = client_manager.peek().get_backend_for_server(&server_id);
    let Some((_account_id, backend)) = backend_info else {
        chat_data.batch(|cd| cd.loading = false);
        return;
    };

    // Accumulate every async-fetched mutation into one terminal batch.
    let mut pending = chat_data.pending_update();

    // Load server details
    {
        let guard = match backend
            .read_with_timeout(std::time::Duration::from_secs(5))
            .await
        {
            Ok(g) => g,
            Err(_) => {
                tracing::warn!(server_id = %server_id, "load_server_data: backend read timed out");
                chat_data.batch(|cd| cd.loading = false);
                return;
            }
        };
        if let Ok(server) = guard.get_server(&server_id).await {
            pending.set(move |cd| cd.current_server = Some(server));
        }
    }

    // Load channels
    let channels = {
        let guard = match backend.read_with_timeout(std::time::Duration::from_secs(5)).await {
            Ok(g) => g,
            Err(_) => {
                tracing::warn!("favorites_sidebar: backend read timed out loading channels");
                chat_data.batch(|cd| cd.loading = false);
                return;
            }
        };
        guard.get_channels(&server_id).await.unwrap_or_default()
    };

    // Find the best channel to auto-select:
    // 1. Backend-designated default channel (Discord system_channel_id, etc.)
    // 2. First text/forum/HN channel
    let default_id = {
        let guard = chat_data.read();
        guard
            .current_server
            .as_ref()
            .and_then(|s| s.default_channel_id.clone())
    };
    let first_text_channel = default_id
        .and_then(|id| channels.iter().find(|c| c.id == id).cloned())
        .or_else(|| {
            channels
                .iter()
                .find(|c| {
                    c.channel_type == poly_client::ChannelType::Text
                        || c.channel_type == poly_client::ChannelType::Forum
                        || c.channel_type == poly_client::ChannelType::HackerNews
                })
                .cloned()
        });

    pending.set(move |cd| cd.channels = channels);

    // Only auto-open the first text channel when the user is already in the
    // content area workflow. When the mobile left drawer is open and the user
    // taps a favorites/account-server icon, we keep them at the server shell
    // so only an explicit channel tap opens content and closes the drawer.
    if auto_select_first_text_channel && let Some(ch) = first_text_channel {
        let ch_id_for_presync = ch.id.clone();
        app_state.batch(|st| {
            st.nav.selected_channel.unsafe_presync_override(
                Some(ch_id_for_presync),
                "favorites_sidebar::load_server_data_internal: auto-select first \
                 text channel when landing on /channels/{server}; the URL stays at \
                 ServerHome so no nav.push follows — we need current_channel set \
                 synchronously or ChatView renders blank between click and effect",
            );
        });
        let ch_for_current = ch.clone();
        pending.set(move |cd| cd.current_channel = Some(ch_for_current));

        // Load messages for first channel
        let guard = match backend.read_with_timeout(std::time::Duration::from_secs(5)).await {
            Ok(g) => g,
            Err(_) => {
                tracing::warn!("favorites_sidebar: backend read timed out loading first-channel messages");
                chat_data.batch(|cd| cd.loading = false);
                return;
            }
        };
        if let Ok(messages) = guard
            .get_messages(&ch.id, initial_message_query(ch.unread_count))
            .await
        {
            pending.set(move |cd| cd.messages = messages);
            request_restore_scroll_position_or_bottom(&ch.id);
        }
        // Load members
        if let Ok(members) = guard.get_channel_members(&ch.id).await {
            pending.set(move |cd| cd.members = members);
        }
    }
    if !auto_select_first_text_channel {
        app_state.batch(|st| {
            st.nav.selected_channel.unsafe_presync_override(
                None,
                "favorites_sidebar::load_server_data_internal: clear selected_channel \
                 when loading server shell only (mobile drawer context) — must be \
                 synchronous so ChatView doesn't briefly render a stale channel",
            );
        });
        pending.set(|cd| {
            cd.current_channel = None;
            cd.messages.clear();
            cd.members.clear();
        });
    }
    pending.set(|cd| cd.loading = false);
    // ONE terminal cascade for the whole fetch.
    pending.apply();
    // Apply any user-defined icon/banner overrides from storage.
    apply_server_icon_overrides(chat_data).await;
}

/// Apply user icon and banner overrides from `AppSettings` to all servers in
/// `chat_data`.
///
/// Called after every `load_server_data` and `restore_server_channel` so that
/// overrides entered in the server settings Overview panel survive across page
/// navigations and app restarts.
///
/// No-ops silently if storage is not yet initialised.
async fn apply_server_icon_overrides(chat_data: BatchedSignal<crate::state::ChatData>) {
    let Some(storage) = crate::STORAGE.get() else {
        return;
    };
    let Ok(settings) = storage.get_app_settings().await else {
        return;
    };
    if settings.server_icon_overrides.is_empty() && settings.server_banner_overrides.is_empty() {
        return;
    }
    chat_data.batch(|cd| {
        for server in &mut cd.servers {
            if let Some(url) = settings.server_icon_overrides.get(&server.id) {
                server.icon_url = Some(url.clone());
            }
            if let Some(url) = settings.server_banner_overrides.get(&server.id) {
                server.banner_url = Some(url.clone());
            }
        }
        if let Some(ref mut current) = cd.current_server {
            if let Some(url) = settings.server_icon_overrides.get(&current.id) {
                current.icon_url = Some(url.clone());
            }
            if let Some(url) = settings.server_banner_overrides.get(&current.id) {
                current.banner_url = Some(url.clone());
            }
        }
    });
}
///
/// Called after every mutation of `ChatData.favorited_server_ids` to survive
/// page reloads, app restarts, and offline periods.
/// No-ops silently if storage is not yet initialised.
/// Persist the Bar-1 account icon order to `AppSettings.account_order`.
///
/// Called after every drag-drop reorder on account icons so users get a
/// stable, restorable layout across page reloads and app restarts.
pub(crate) async fn persist_account_order(order: Vec<String>) {
    let Some(s) = crate::STORAGE.get() else {
        return;
    };
    match s.get_app_settings().await {
        Ok(mut settings) => {
            settings.account_order = order;
            if let Err(e) = s.set_app_settings(&settings).await {
                tracing::warn!("Failed to persist account_order: {e}");
            }
        }
        Err(e) => tracing::warn!("Failed to read app_settings for account_order persist: {e}"),
    }
}

pub(crate) async fn persist_favorites(ids: Vec<String>) {
    let Some(s) = crate::STORAGE.get() else {
        return;
    };
    match s.get_app_settings().await {
        Ok(mut settings) => {
            settings.favorited_server_ids = ids;
            if let Err(e) = s.set_app_settings(&settings).await {
                tracing::warn!("Failed to persist favorites: {e}");
            }
        }
        Err(e) => tracing::warn!("Failed to read app_settings for favorites persist: {e}"),
    }
}

/// Restore a specific server channel from a URL (F5 / deep-link navigation).
///
/// Unlike [`load_server_data`] which auto-selects the first text channel,
/// this function restores the exact `channel_id` encoded in the URL.
///
/// Called from the `ServerChat` route component's `use_effect` when
/// `chat_data` is empty (i.e. the page was hard-refreshed).
///
/// Returns the resolved channel id. If the URL `channel_id` doesn't exist on
/// the server (deleted, never existed, typo'd deep link), returns
/// `Some(fallback_id)` — the caller is expected to `nav.replace` to that
/// fallback so the URL matches reality. If the server itself is missing or
/// has no channels at all, returns `None` and the caller should redirect
/// somewhere sensible (e.g., ServerHome).
pub async fn restore_server_channel(
    server_id: String,
    channel_id: String,
    mut app_state: BatchedSignal<AppState>,
    client_manager: BatchedSignal<ClientManager>,
    chat_data: BatchedSignal<ChatData>,
) -> Option<String> {
    // One initial cascade so the UI can paint a loading state while we
    // fetch. Every other mutation is deferred to a single terminal write
    // at the end — individual per-field writes were starving the WASM
    // scheduler on Teams (CLAUDE.md hang #1, same class as AccountIcon
    // onclick batching fix in commit a761fe01).
    chat_data.batch(|cd| cd.loading = true);

    let backend_info = client_manager.peek().get_backend_for_server(&server_id);
    let Some((_account_id, backend)) = backend_info else {
        chat_data.batch(|cd| cd.loading = false);
        return None;
    };

    // Load server details
    let loaded_server = {
        let guard = match backend.read_with_timeout(std::time::Duration::from_secs(5)).await {
            Ok(g) => g,
            Err(_) => {
                tracing::warn!("favorites_sidebar: backend read timed out loading server details");
                chat_data.batch(|cd| cd.loading = false);
                return None;
            }
        };
        guard.get_server(&server_id).await.ok()
    };

    // Load all channels for the sidebar
    let channels = {
        let guard = match backend.read_with_timeout(std::time::Duration::from_secs(5)).await {
            Ok(g) => g,
            Err(_) => {
                tracing::warn!("favorites_sidebar: backend read timed out loading channels");
                chat_data.batch(|cd| cd.loading = false);
                return None;
            }
        };
        guard.get_channels(&server_id).await.unwrap_or_default()
    };

    // Locate the requested channel.
    let exact = channels.iter().find(|c| c.id == channel_id).cloned();

    // Fall back to first text/forum channel if URL channel missing — but ONLY
    // for content rendering, NOT for selected_channel mutation. Pre-mutating
    // selected_channel here doubled with the caller's nav.replace cascading
    // through every ChatView use_effect subscriber wedged the WASM scheduler
    // (same bug class as friend-card hang 2026-04-19). The caller's
    // nav.replace alone, observed via on_update, is the single source of
    // truth for the field.
    let fallback = if exact.is_none() {
        // 1. Backend-designated default channel (Discord system_channel_id, etc.)
        // Read from the just-loaded server, not from chat_data — chat_data
        // hasn't been updated yet in this batched flow.
        let default_id = loaded_server
            .as_ref()
            .and_then(|s| s.default_channel_id.clone());
        let by_default = default_id.and_then(|id| channels.iter().find(|c| c.id == id).cloned());
        // 2. First text/forum/hackernews channel
        by_default.or_else(|| {
            channels
                .iter()
                .find(|c| {
                    matches!(
                        c.channel_type,
                        poly_client::ChannelType::Text
                            | poly_client::ChannelType::Forum
                            | poly_client::ChannelType::HackerNews
                    )
                })
                .cloned()
        })
    } else {
        None
    };
    let _ = &mut app_state;

    // Use the exact match for content load if it exists, otherwise the
    // fallback. The caller still needs the resolved id to nav.replace.
    let target = exact.or(fallback);

    // Pre-fetch per-channel content WITHOUT touching chat_data. All mutations
    // batched into the single terminal write below.
    let mut loaded_messages: Option<Vec<poly_client::Message>> = None;
    let mut loaded_channel_load_error: Option<String> = None;
    let mut loaded_members: Option<Vec<poly_client::User>> = None;
    let mut loaded_voice: Option<(String, Vec<poly_client::VoiceParticipant>)> = None;
    let mut request_scroll_for: Option<String> = None;

    if let Some(ref ch) = target {
        if matches!(
            ch.channel_type,
            poly_client::ChannelType::Text
                | poly_client::ChannelType::Forum
                | poly_client::ChannelType::HackerNews
        ) {
            let guard = match backend.read_with_timeout(std::time::Duration::from_secs(5)).await {
                Ok(g) => g,
                Err(_) => {
                    tracing::warn!("favorites_sidebar: backend read timed out loading channel messages");
                    chat_data.batch(|cd| cd.loading = false);
                    return None;
                }
            };
            // F-DC-1: handle PermissionDenied explicitly so the UI can render
            // a styled empty state instead of silently showing 0 messages.
            match guard
                .get_messages(&ch.id, initial_message_query(ch.unread_count))
                .await
            {
                Ok(messages) => {
                    loaded_messages = Some(messages);
                    loaded_channel_load_error = None;
                    request_scroll_for = Some(ch.id.clone());
                }
                Err(poly_client::ClientError::PermissionDenied(msg)) => {
                    tracing::info!(
                        "get_messages permission denied for channel {}: {}",
                        ch.id,
                        msg
                    );
                    loaded_messages = Some(Vec::new());
                    loaded_channel_load_error = Some(msg);
                }
                Err(err) => {
                    tracing::warn!("get_messages failed for channel {}: {}", ch.id, err);
                    loaded_channel_load_error = None;
                }
            }
            if let Ok(members) = guard.get_channel_members(&ch.id).await {
                loaded_members = Some(members);
            }
        } else if matches!(
            ch.channel_type,
            poly_client::ChannelType::Voice | poly_client::ChannelType::Video
        ) {
            let guard = match backend.read_with_timeout(std::time::Duration::from_secs(5)).await {
                Ok(g) => g,
                Err(_) => {
                    tracing::warn!("favorites_sidebar: backend read timed out loading voice participants");
                    chat_data.batch(|cd| cd.loading = false);
                    return None;
                }
            };
            if let Ok(participants) = guard.get_voice_participants(&ch.id).await {
                loaded_voice = Some((ch.id.clone(), participants));
            }
        }
    }

    // Single terminal cascade — everything that was read above lands in one
    // write-guard drop, so subscribers see one consistent state update
    // instead of 5-7 intermediate ones.
    chat_data.batch(|cd| {
        if let Some(server) = loaded_server {
            cd.current_server = Some(server);
        }
        cd.channels = channels;
        if let Some(ref ch) = target {
            cd.current_channel = Some(ch.clone());
        }
        if let Some(messages) = loaded_messages {
            cd.messages = messages;
        }
        cd.channel_load_error = loaded_channel_load_error;
        if let Some(members) = loaded_members {
            cd.members = members;
        }
        if let Some((id, participants)) = loaded_voice {
            cd.voice_channel_participants.insert(id, participants);
        }
        cd.loading = false;
    });

    if let Some(ch_id) = request_scroll_for {
        request_restore_scroll_position_or_bottom(&ch_id);
    }

    // Apply any user-defined icon/banner overrides from storage.
    apply_server_icon_overrides(chat_data).await;

    target.map(|channel| channel.id)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn favorites_bar_action_variants_compile() {
        fn assert_ui_action<T: crate::ui::actions::UiAction>() {}
        assert_ui_action::<FavoritesBarAction>();
        let _ = FavoritesBarAction::SwitchAccount("acc".into());
        let _ = FavoritesBarAction::OpenSearch;
        let _ = FavoritesBarAction::OpenSettings;
        let _ = FavoritesBarAction::DropServer("srv".into());
        let _ = FavoritesBarAction::ReorderFavorite {
            drag_id: "a".into(),
            target_id: "b".into(),
        };
        let _ = FavoritesBarAction::ReorderAccount {
            drag_id: "a".into(),
            target_id: "b".into(),
        };
    }
}
