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
//!
//! # Module layout (SOLID — C.3 split)
//! - `favorites_sidebar.rs` — rendering components (this file)
//! - `favorites_sidebar/persist.rs` — pure-async persistence helpers
//!   (`persist_favorites`, `persist_account_order`, `apply_server_icon_overrides`)

// ── Submodules ────────────────────────────────────────────────────────────

pub mod persist;
pub(crate) use persist::{apply_server_icon_overrides, persist_account_order, persist_favorites};

// ── Imports ───────────────────────────────────────────────────────────────

use crate::state::BatchedSignal;
use super::routes::Route;
use crate::client_manager::{BackendHandleExt, ClientManager};
use crate::i18n::t;
use crate::state::chat_data::user_color;
use crate::state::{AccountContextMenuState, AccountSessions, ChatAction, ChatLists, ChatViewState, ContextMenuState, DragSource, DragState, NavState, UiOverlays, View, VoiceState};
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

// ── Action enum ───────────────────────────────────────────────────────────

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

// ── Snapshot structs (B.3 — single .with() per component) ────────────────

/// All data derived from `account_sessions` + `client_manager` that
/// `FavoritesBar` needs in its render body. Collapsed into one `.with()`
/// per signal so each signal subscription fires exactly once per render.
struct FavoritesBarSnapshot {
    /// Ordered list of active account IDs (user-defined order or priority fallback).
    account_ids: Vec<String>,
    /// Server IDs the user has dragged into Bar 1 (in display order).
    favorited_ids: Vec<String>,
    /// Whether the demo backend is active (controls the drop-hint).
    demo_active: bool,
}

/// All `account_sessions` data that `AccountIcon` needs, extracted in one
/// `.with()` call to avoid N separate subscriptions (B.7).
struct AccountIconSnapshot {
    is_forum_account: bool,
    avatar_url: Option<String>,
    display_name: String,
    backend_name: String,
    icon_label: String,
}

/// All `account_sessions` data that `FavoriteServerIcon` needs, extracted
/// in one `.with()` call (B.7).
struct FavoriteServerIconSnapshot {
    account_avatar_url: Option<String>,
}

// ── Shared sub-components (B.1 split) ────────────────────────────────────

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

/// Avatar block for `FavoriteServerIcon` (B.1 — avatar/badge split).
///
/// Renders the server icon image (or first-letter fallback) plus the
/// source-account badge overlay. Pure display — no signal reads.
#[context_menu(inherit)]
#[ui_action(inherit)]
#[component]
fn FavoriteServerAvatarBlock(
    icon_url: Option<String>,
    icon_color: String,
    first_letter: String,
    server_name: String,
    account_avatar_url: Option<String>,
    account_display_name: String,
) -> Element {
    rsx! {
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
    }
}

/// Status badge block for `FavoriteServerIcon` (B.1 — badge split).
///
/// Renders connection status icon, mention/unread count badge, and
/// presence dot. Pure display — no signal reads.
#[context_menu(inherit)]
#[ui_action(inherit)]
#[component]
fn FavoriteServerBadgeBlock(
    is_forum: bool,
    conn_class: String,
    conn_icon: String,
    server_needs_reauth: bool,
    mention: u32,
    unread: u32,
    presence_class: String,
) -> Element {
    rsx! {
        // Bottom-left: account connection status emoji icon (not for forum,
        // unless reauth is needed — forum accounts still surface the 🔑 badge).
        if !is_forum {
            span {
                class: "account-conn-icon account-conn-icon--{conn_class}",
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
        if !is_forum {
            span {
                class: "status-dot presence-dot {presence_class}",
            }
        }
    }
}

// ── FavoritesBar (B.3 — state derivation lifted + split rendering) ────────

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
    let nav_state: BatchedSignal<NavState> = use_context();
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let account_sessions: BatchedSignal<AccountSessions> = use_context();
    let drag_state: BatchedSignal<DragState> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();

    // B.3 + B.7: Collapse all state derivation into ONE .with() per signal.
    // Kills the N separate .read() subscriptions that were allowlisted on
    // lines 146, 154, 170 (account_sessions) and the client_manager reads.
    let snap = {
        let live: Vec<String> = client_manager.with(|cm| cm.active_account_ids());
        let (account_ids, favorited_ids, demo_active) = account_sessions.with(|as_| {
            // Collect distinct active account IDs applying user-saved order.
            // Accounts not in saved order appended by priority fallback:
            //   0. demo messenger accounts (Cat, Dog)
            //   1. demo forum accounts (Platypus)
            //   2. every other account (user-added: HN, poly, etc.), alphabetical
            let saved_order = &as_.account_order;
            let live_set: std::collections::HashSet<_> = live.iter().cloned().collect();
            let mut ordered: Vec<String> = saved_order
                .iter()
                .filter(|id| live_set.contains(*id))
                .cloned()
                .collect();
            let placed: std::collections::HashSet<_> = ordered.iter().cloned().collect();
            let priority = |id: &String| -> u8 {
                match as_.account_sessions.get(id) {
                    Some(s) if s.backend == "demo" => 0,
                    Some(s) if s.backend == "demo_forum" => 1,
                    _ => 2,
                }
            };
            let mut rest: Vec<String> =
                live.into_iter().filter(|id| !placed.contains(id)).collect();
            rest.sort_by(|a, b| priority(a).cmp(&priority(b)).then_with(|| a.cmp(b)));
            ordered.extend(rest);
            let favorited_ids = as_.favorited_server_ids.clone();
            (ordered, favorited_ids, false) // demo_active derived below
        });
        let demo_active = client_manager.with(|cm| cm.demo_active);
        FavoritesBarSnapshot { account_ids, favorited_ids, demo_active }
    };

    // Derive the current view and active account from nav_state (intentional subscription).
    let current_view = nav_state.with(|n| *n.view);
    let active_account = nav_state.with(|n| n.active_account_id.cloned());

    // Expand favorited_ids to one icon per (account, server.id) pair so that
    // shared guilds render two icons — one per account.
    let favorite_servers: Vec<poly_client::Server> = {
        let snap_cl = chat_lists.peek();
        snap.favorited_ids
            .iter()
            .flat_map(|id| {
                snap_cl
                    .servers
                    .iter()
                    .filter(|s| s.id == *id)
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .collect()
    };

    // Local signal for drop-zone highlight state.
    let mut drag_over = use_signal(|| false);

    // Global tooltip show/hide + positioning for ALL sidebar icons.
    use_effect(move || { // poly-lint: allow stale-effect-capture — one-shot DOM initialiser; no non-Signal captures
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
            div { class: "sidebar-scroll-area",
                ondragover: move |evt| {
                    evt.prevent_default();
                    drag_over.set(true);
                },
                ondragleave: move |_| drag_over.set(false),
                ondrop: move |evt| {
                    evt.prevent_default();
                    drag_over.set(false);
                    let (drag_id, drag_src) = drag_state.batch(|d| {
                        let id = d.dragging_server_id.clone();
                        let src = d.drag_source.clone();
                        d.dragging_server_id = None;
                        d.drag_source = DragSource::None;
                        d.drag_over_id = None;
                        (id, src)
                    });
                    let new_favorites = account_sessions.batch(|as_| {
                        if let Some(sid) = drag_id {
                            match drag_src {
                                DragSource::AccountServer | DragSource::FavoriteServer => {
                                    if !as_.favorited_server_ids.contains(&sid) {
                                        as_.favorited_server_ids.push(sid);
                                    }
                                }
                                DragSource::None | DragSource::AccountIcon => {}
                            }
                        }
                        as_.favorited_server_ids.clone()
                    });
                    spawn(async move {
                        persist_favorites(new_favorites).await;
                    });
                },
                class: if drag_over() { "drag-over" } else { "" },

                NavBarSpacer {}

                // ── Account icons (B.3 — FavoritesBarLeft concern) ───────────
                FavoritesBarAccountList {
                    account_ids: snap.account_ids.clone(),
                    active_account: active_account.clone(),
                }

                // Separator (between accounts and favorites)
                if !snap.account_ids.is_empty() {
                    div { class: "sidebar-separator" }
                }

                // ── Favorited servers (B.3 — FavoritesBarMain concern) ───────
                for server in &favorite_servers {
                    {
                        let instance_id = account_sessions
                            .peek()
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
                if favorite_servers.is_empty() && snap.demo_active {
                    div { class: "favorites-drop-hint",
                        span { "← Drag servers here" }
                    }
                }

                // Spacer (flex: 1 pushes footer to bottom)
                div { class: "sidebar-spacer" }
            }

            // Footer: search + agent + settings buttons float at bottom
            FavoritesBarFooter {
                current_view,
                active_account,
            }
        }
    }
}

/// Account icon list sub-component (B.3 — FavoritesBarLeft).
///
/// Single responsibility: render the ordered list of account icons.
/// All state derivation (ordering, active account) is done by `FavoritesBar`.
#[context_menu(inherit)]
#[ui_action(inherit)]
#[component]
fn FavoritesBarAccountList(
    account_ids: Vec<String>,
    active_account: Option<String>,
) -> Element {
    rsx! {
        for aid in &account_ids {
            AccountIcon {
                account_id: aid.clone(),
                is_active: active_account.as_deref() == Some(aid.as_str()),
            }
        }
    }
}

/// Footer sub-component (B.3 — FavoritesBarMain footer concern).
///
/// Renders search, agent, and settings footer buttons.
/// Receives `current_view` and `active_account` as props — no signal reads.
#[context_menu(inherit)]
#[ui_action(inherit)]
#[component]
fn FavoritesBarFooter(
    current_view: View,
    active_account: Option<String>,
) -> Element {
    let is_search = current_view == View::Search;
    let is_agent = current_view == View::Agent;
    let is_app_settings = current_view == View::Settings && active_account.is_none();
    rsx! {
        div { class: "sidebar-footer",
            // Global Search button
            div {
                class: if is_search { "server-icon active" } else { "server-icon" },
                onclick: move |_| {
                    close_mobile_drawer();
                    crate::nav!(Route::SearchRoute);
                },
                title: "{t(\"nav-search\")}",
                div { class: "icon-search", "🔍" }
            }
            // Agent button
            div {
                class: if is_agent { "server-icon active" } else { "server-icon" },
                onclick: move |_| {
                    close_mobile_drawer();
                    crate::nav!(Route::AgentRoute);
                },
                title: "{t(\"nav-agent\")}",
                div { class: "icon-agent", "🤖" }
            }
            // App Settings button — only "active" for app-level settings (no account scoped)
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

// ── AccountIcon ───────────────────────────────────────────────────────────

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
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let nav_state: BatchedSignal<NavState> = use_context();
    let ui_overlays: BatchedSignal<UiOverlays> = use_context();
    let drag_state: BatchedSignal<DragState> = use_context();
    let account_sessions: BatchedSignal<AccountSessions> = use_context();
    let chat_view_state: BatchedSignal<ChatViewState> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();

    // B.7: Collapse 2 separate client_manager.read() calls into ONE .with()
    // (kills the per-signal subscription duplication; subscription intentional).
    let (conn_class, presence_class): (&'static str, &'static str) =
        client_manager.with(|cm| {
            let conn = cm
                .connection_statuses
                .get(&account_id)
                .map_or("disconnected", ConnectionStatus::css_class);
            let pres = cm
                .presence_statuses
                .get(&account_id)
                .copied()
                .unwrap_or(AccountPresence::Online)
                .css_class();
            (conn, pres)
        });

    // B.7: Collapse 5 separate account_sessions.read() calls (lines 427, 437,
    // 444, 450, 459) into ONE .with() block — one subscription, all fields.
    let as_snap: AccountIconSnapshot = account_sessions.with(|as_| {
        let session = as_.account_sessions.get(&account_id);
        let is_forum_account = session
            .is_some_and(|s| client_manager.peek().capabilities_for_slug(s.backend.as_str()).is_forum_layout());
        let avatar_url = session.and_then(|s| s.user.avatar_url.clone());
        let display_name = session
            .map_or_else(|| account_id.clone(), |s| s.user.display_name.clone());
        let backend_name = session
            .map_or_else(|| "Unknown".to_string(), |s| s.backend.display_name().to_string());
        // Use icon_emoji if available, else first letter of display_name.
        let icon_label = session
            .and_then(|s| s.icon_emoji.clone())
            .unwrap_or_else(|| {
                display_name
                    .chars()
                    .next()
                    .map(|c| c.to_uppercase().to_string())
                    .unwrap_or_default()
            });
        AccountIconSnapshot { is_forum_account, avatar_url, display_name, backend_name, icon_label }
    });

    // B.7: Collapse the single chat_lists.read() (line 475) into .with().
    let total_unreads = u32::try_from(
        chat_lists.with(|cl| {
            cl.notifications
                .iter()
                .filter(|n| n.account_id == account_id)
                .count()
        }),
    )
    .unwrap_or(u32::MAX);

    let color = user_color(&account_id);

    // Connection icon emoji for connection status
    let conn_icon = match conn_class {
        "connected" => "⚡",
        "connecting" => "↺",
        "disconnected" => "—",
        "unauthenticated" => "🔑",
        _ => "⚠",
    };
    let needs_reauth_badge = conn_class == "unauthenticated";

    let is_drag_over_account = drag_state.with(|d| {
        d.drag_over_id.as_deref() == Some(account_id.as_str())
            && d.drag_source == DragSource::AccountIcon
    });

    let drag_start_id = account_id.clone();
    let on_account_drag_start = move |_: Event<DragData>| {
        drag_state.batch(|d| {
            d.dragging_server_id = Some(drag_start_id.clone());
            d.drag_source = DragSource::AccountIcon;
        });
    };

    let drag_over_id = account_id.clone();
    let on_account_drag_over = move |evt: Event<DragData>| {
        if drag_state.with(|d| d.drag_source != DragSource::AccountIcon) {
            return;
        }
        evt.prevent_default();
        evt.stop_propagation();
        drag_state.batch(|d| d.drag_over_id = Some(drag_over_id.clone()));
    };

    let drag_leave_id = account_id.clone();
    let on_account_drag_leave = move |_: Event<DragData>| {
        drag_state.batch(|d| {
            if d.drag_over_id.as_deref() == Some(drag_leave_id.as_str()) {
                d.drag_over_id = None;
            }
        });
    };

    let drop_target_id = account_id.clone();
    let client_manager_for_drop = client_manager;
    let on_account_drop = move |evt: Event<DragData>| {
        if drag_state.with(|d| d.drag_source != DragSource::AccountIcon) {
            return;
        }
        evt.prevent_default();
        evt.stop_propagation();
        let (dragging, drag_id_valid) = drag_state.batch(|d| {
            let dragging = d.dragging_server_id.clone();
            d.drag_over_id = None;
            let valid = dragging.as_deref() != Some(drop_target_id.as_str()) && dragging.is_some();
            if !valid {
                d.dragging_server_id = None;
                d.drag_source = DragSource::None;
            }
            (dragging, valid)
        });
        let Some(drag_id) = dragging else {
            return;
        };
        if !drag_id_valid {
            return;
        }
        let snapshot = account_sessions.batch(|as_| {
            if as_.account_order.is_empty() {
                // peek() inside a batch closure — not a render-time read, no reactive subscription.
                let mut live: Vec<String> = client_manager_for_drop
                    .peek()
                    .active_account_ids();
                live.sort();
                as_.account_order = live;
            }
            if !as_.account_order.contains(&drag_id) {
                as_.account_order.push(drag_id.clone());
            }
            if !as_.account_order.contains(&drop_target_id) {
                as_.account_order.push(drop_target_id.clone());
            }
            if let Some(from) = as_.account_order.iter().position(|x| x == &drag_id) {
                as_.account_order.remove(from);
                if let Some(to) =
                    as_.account_order.iter().position(|x| x == &drop_target_id)
                {
                    as_.account_order.insert(to, drag_id);
                } else {
                    as_.account_order.push(drag_id);
                }
            }
            Some(as_.account_order.clone())
        });
        drag_state.batch(|d| {
            d.dragging_server_id = None;
            d.drag_source = DragSource::None;
        });
        if let Some(order) = snapshot {
            spawn(async move {
                persist_account_order(order).await;
            });
        }
    };

    let on_account_drag_end = move |_: Event<DragData>| {
        drag_state.batch(|d| {
            *d = DragState::default();
        });
    };

    let account_item_class = match (is_active, is_drag_over_account) {
        (true, true) => "server-icon account-icon active drag-over-target",
        (true, false) => "server-icon account-icon active",
        (false, true) => "server-icon account-icon drag-over-target",
        (false, false) => "server-icon account-icon",
    };

    let menu_aid = account_id.clone();
    let menu_display = as_snap.display_name.clone();
    let menu_ui_overlays = ui_overlays;
    let menu_account_sessions = account_sessions;
    let on_account_contextmenu = move |evt: Event<MouseData>| {
        evt.prevent_default();
        evt.stop_propagation();
        let coords = evt.client_coordinates();
        let (slug, inst) = {
            let as_ = menu_account_sessions.peek();
            as_.account_sessions
                .get(&menu_aid)
                .map_or_else(
                    || ("demo".to_string(), "demo".to_string()),
                    |s| (s.backend.slug().to_string(), s.instance_id.clone()),
                )
        };
        menu_ui_overlays.batch(|o| {
            o.context_menu_stack.push(account_entry_at(
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

    let aid_for_click = account_id.clone();
    let display_name = as_snap.display_name.clone();
    let backend_name = as_snap.backend_name.clone();

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

                let needs_reauth = {
                    let cm = client_manager.peek();
                    cm.connection_statuses
                        .get(&aid)
                        .is_some_and(ConnectionStatus::needs_reauth)
                };
                if needs_reauth {
                    let info = account_sessions
                        .peek()
                        .account_sessions
                        .get(&aid)
                        .map(|s| (s.backend.slug().to_string(), s.instance_id.clone()));
                    if let Some((slug, instance_id)) = info {
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

                chat_view_state.batch(|cv| cv.apply(ChatAction::ClearChannelContext));
                chat_lists.batch(|cl| cl.set_channels(Vec::new()));

                if !preserve_drawer_context {
                    let last_route_url = nav_state
                        .peek()
                        .account_last_routes
                        .get(&aid)
                        .cloned();
                    if let Some(url) = last_route_url
                        && let Ok(route) = url.parse::<Route>()
                    {
                        let backend_slug_for_route_check = account_sessions
                            .peek()
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

                let (backend_slug, instance_id, first_server_id) = {
                    let (slug, inst) = if let Some(session) =
                        account_sessions.peek().account_sessions.get(&aid).cloned()
                    {
                        (session.backend.slug().to_string(), session.instance_id.clone())
                    } else {
                        let slug = chat_lists
                            .peek()
                            .servers
                            .iter()
                            .find(|s| s.account_id == aid)
                            .map_or_else(|| "demo".to_string(), |s| s.backend.slug().to_string());
                        (slug, "demo".to_string())
                    };
                    let first_server = chat_lists
                        .peek()
                        .servers
                        .iter()
                        .find(|s| s.account_id == aid)
                        .map(|s| s.id.clone());
                    (slug, inst, first_server)
                };
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
            // Render image avatar if available.
            if let Some(url) = &as_snap.avatar_url {
                img {
                    src: "{url}",
                    class: "server-icon-image",
                    alt: "{account_id}",
                }
            } else {
                div {
                    class: "server-icon-letter",
                    style: "background-color: {color};",
                    "{as_snap.icon_label}"
                }
            }
            // Bottom-left: connection status emoji icon (not shown for forum accounts,
            // unless the account needs reauthentication — then always show).
            if !as_snap.is_forum_account {
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
            if (!as_snap.is_forum_account || needs_reauth_badge) && total_unreads > 0 {
                span {
                    class: "badge mention-count-badge",
                    "{total_unreads}"
                }
            }
            // Bottom-right: presence dot (not shown for forum accounts)
            if !as_snap.is_forum_account {
                span {
                    class: "status-dot presence-dot {presence_class}",
                }
            }
            SidebarTooltip {
                line1: display_name,
                line2: Some(backend_name),
                line3: None,
            }
        }
    }
}

// ── FavoriteServerIcon (B.1 — split into sub-components) ─────────────────

/// Single favorited server icon in the favorites bar.
///
/// Supports:
/// - Click to navigate to the server
/// - Right-click to open the server context menu
/// - Drag to reorder within Bar 1 or move back (drag is tracked via `DragSource::FavoriteServer`)
/// - Accept drops from Bar 2 (`DragSource::AccountServer`) for positional insertion
///
/// Rendering is split into `FavoriteServerAvatarBlock` and `FavoriteServerBadgeBlock`
/// sub-components (B.1 — single responsibility per block).
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
    let nav_state: BatchedSignal<NavState> = use_context();
    let ui_overlays: BatchedSignal<UiOverlays> = use_context();
    let client_manager: BatchedSignal<ClientManager> = use_context();
    let drag_state: BatchedSignal<DragState> = use_context();
    let account_sessions: BatchedSignal<AccountSessions> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    let chat_view_state: BatchedSignal<ChatViewState> = use_context();

    // Single render-time read of client_manager — avoid two separate .read()
    // calls (hang-class #1/#2). Subscription intentional.
    let (account_conn_class, account_presence_class): (&'static str, &'static str) =
        client_manager.with(|cm| {
            let conn = cm
                .connection_statuses
                .get(&account_id)
                .map_or("disconnected", ConnectionStatus::css_class);
            let pres = cm
                .presence_statuses
                .get(&account_id)
                .copied()
                .unwrap_or(AccountPresence::Online)
                .css_class();
            (conn, pres)
        });

    let conn_icon: &'static str = match account_conn_class {
        "connected" => "⚡",
        "connecting" => "↺",
        "disconnected" => "—",
        "unauthenticated" => "🔑",
        _ => "⚠",
    };
    let server_needs_reauth = account_conn_class == "unauthenticated";

    // B.7 + B.1: Collapse account_sessions.read() (line 940) into .with()
    // — one subscription, one field extracted.
    let fs_snap: FavoriteServerIconSnapshot = account_sessions.with(|as_| {
        let account_avatar_url = as_
            .account_sessions
            .get(&account_id)
            .and_then(|s| s.user.avatar_url.clone());
        FavoriteServerIconSnapshot { account_avatar_url }
    });

    let is_selected = nav_state.with(|n| n.selected_server.as_deref() == Some(&server_id));
    let is_drag_over = drag_state.with(|d| d.drag_over_id.as_deref() == Some(server_id.as_str()));

    // Derived display values — no signal reads.
    let first_letter: String = server_name
        .chars()
        .next()
        .map(|c| c.to_string())
        .unwrap_or_default();
    let icon_color = user_color(&server_id);

    // Capability check — peek only (no reactive subscription needed).
    let is_forum = client_manager.peek().capabilities_for_slug(&backend_slug).is_forum_layout();

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
                    if let Some(previous_channel_id) = nav_state
                        .peek()
                        .selected_channel
                        .cloned()
                    {
                        remember_message_list_scroll_position(&previous_channel_id);
                    }
                    if !preserve_drawer_context {
                        let sid2 = sid.clone();
                        spawn(async move {
                            load_server_data(sid2, nav_state, client_manager, chat_lists, chat_view_state).await;
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
                    ui_overlays.batch(|o| {
                        o.context_menu_stack.push(server_icon_entry_at(
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
                    drag_state.batch(|d| {
                        d.dragging_server_id = Some(sid.clone());
                        d.drag_source = DragSource::FavoriteServer;
                    });
                }
            },
            // Drag over this item — highlight as drop target
            ondragover: {
                let sid = server_id.clone();
                move |evt: Event<DragData>| {
                    evt.prevent_default();
                    evt.stop_propagation();
                    drag_state.batch(|d| d.drag_over_id = Some(sid.clone()));
                }
            },
            // Drag leave — clear highlight if we are still the target
            ondragleave: {
                let sid = server_id.clone();
                move |_| {
                    drag_state.batch(|d| {
                        if d.drag_over_id.as_deref() == Some(sid.as_str()) {
                            d.drag_over_id = None;
                        }
                    });
                }
            },
            // Drop on this item — reorder within Bar 1, or insert from Bar 2
            ondrop: {
                let tid = server_id.clone();
                move |evt: Event<DragData>| {
                    evt.prevent_default();
                    evt.stop_propagation();
                    let (dragging, src) = drag_state.batch(|d| {
                        let dragging = d.dragging_server_id.clone();
                        let src = d.drag_source.clone();
                        *d = DragState::default();
                        (dragging, src)
                    });
                    let Some(drag_id) = dragging else {
                        return;
                    };
                    let target_id = tid.clone();
                    if drag_id == target_id {
                        return;
                    }
                    let new_favorites = account_sessions.batch(|as_| {
                        match src {
                            DragSource::FavoriteServer => {
                                if let Some(from) = as_
                                    .favorited_server_ids
                                    .iter()
                                    .position(|x| *x == drag_id)
                                {
                                    as_.favorited_server_ids.remove(from);
                                    if let Some(to) = as_
                                        .favorited_server_ids
                                        .iter()
                                        .position(|x| *x == target_id)
                                    {
                                        as_.favorited_server_ids.insert(to, drag_id);
                                    } else {
                                        as_.favorited_server_ids.push(drag_id);
                                    }
                                }
                            }
                            DragSource::AccountServer => {
                                if !as_.favorited_server_ids.contains(&drag_id) {
                                    if let Some(to) = as_
                                        .favorited_server_ids
                                        .iter()
                                        .position(|x| *x == target_id)
                                    {
                                        as_.favorited_server_ids.insert(to, drag_id);
                                    } else {
                                        as_.favorited_server_ids.push(drag_id);
                                    }
                                }
                            }
                            DragSource::None | DragSource::AccountIcon => {}
                        }
                        Some(as_.favorited_server_ids.clone())
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
                drag_state.batch(|d| { *d = DragState::default(); });
            },

            // B.1: Avatar/badge rendering delegated to focused sub-components.
            FavoriteServerAvatarBlock {
                icon_url: icon_url.clone(),
                icon_color,
                first_letter,
                server_name: server_name.clone(),
                account_avatar_url: fs_snap.account_avatar_url,
                account_display_name: account_display_name.clone(),
            }
            FavoriteServerBadgeBlock {
                is_forum,
                conn_class: account_conn_class.to_string(),
                conn_icon: conn_icon.to_string(),
                server_needs_reauth,
                mention,
                unread,
                presence_class: account_presence_class.to_string(),
            }
            SidebarTooltip {
                line1: server_name.clone(),
                line2: Some(account_display_name.clone()),
                line3: Some(backend_name.clone()),
            }
        }
    }
}

// ── Async server data loaders ─────────────────────────────────────────────

/// Load channels and select the first text channel for a server.
pub async fn load_server_data(
    server_id: String,
    nav_state: BatchedSignal<NavState>,
    client_manager: BatchedSignal<ClientManager>,
    chat_lists: BatchedSignal<ChatLists>,
    chat_view_state: BatchedSignal<ChatViewState>,
) {
    load_server_data_internal(server_id, nav_state, client_manager, chat_lists, chat_view_state, true).await;
}

pub async fn load_server_shell_data(
    server_id: String,
    nav_state: BatchedSignal<NavState>,
    client_manager: BatchedSignal<ClientManager>,
    chat_lists: BatchedSignal<ChatLists>,
    chat_view_state: BatchedSignal<ChatViewState>,
) {
    load_server_data_internal(server_id, nav_state, client_manager, chat_lists, chat_view_state, false).await;
}

async fn load_server_data_internal(
    server_id: String,
    nav_state: BatchedSignal<NavState>,
    client_manager: BatchedSignal<ClientManager>,
    chat_lists: BatchedSignal<ChatLists>,
    chat_view_state: BatchedSignal<ChatViewState>,
    auto_select_first_text_channel: bool,
) {
    // Show the spinner immediately so the content area doesn't render a
    // stale server while we fetch the new one. Keep this as its own
    // cascade — the UI needs to react before we start awaiting.
    chat_view_state.batch(|cv| cv.loading = true);

    // Find which backend owns this server
    let backend_info = client_manager.peek().get_backend_for_server(&server_id);
    let Some((_account_id, backend)) = backend_info else {
        chat_view_state.batch(|cv| cv.loading = false);
        return;
    };

    let mut pending_cv = chat_view_state.pending_update();
    let mut pending_cl = chat_lists.pending_update();

    // Load server details
    {
        let guard = match backend
            .read_with_timeout(std::time::Duration::from_secs(5))
            .await
        {
            Ok(g) => g,
            Err(_) => {
                tracing::warn!(server_id = %server_id, "load_server_data: backend read timed out");
                pending_cv.discard();
                pending_cl.discard();
                chat_view_state.batch(|cv| cv.loading = false);
                return;
            }
        };
        if let Ok(server) = guard.get_server(&server_id).await {
            pending_cv.set(move |cv| cv.current_server = Some(server));
        }
    }

    // Load channels
    let channels = {
        let guard = match backend.read_with_timeout(std::time::Duration::from_secs(5)).await {
            Ok(g) => g,
            Err(_) => {
                tracing::warn!("favorites_sidebar: backend read timed out loading channels");
                pending_cv.discard();
                pending_cl.discard();
                chat_view_state.batch(|cv| cv.loading = false);
                return;
            }
        };
        guard.get_channels(&server_id).await.unwrap_or_default()
    };

    let default_id = chat_view_state.peek().current_server.as_ref().and_then(|s| s.default_channel_id.clone());
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

    pending_cl.set(move |cl| cl.set_channels(channels));

    if auto_select_first_text_channel && let Some(ch) = first_text_channel {
        let ch_id_for_presync = ch.id.clone();
        nav_state.batch(|n| {
            n.selected_channel.unsafe_presync_override(
                Some(ch_id_for_presync),
                "favorites_sidebar::load_server_data_internal: auto-select first \
                 text channel when landing on /channels/{server}; the URL stays at \
                 ServerHome so no nav.push follows — we need current_channel set \
                 synchronously or ChatView renders blank between click and effect",
            );
        });
        let ch_for_current = ch.clone();
        pending_cv.set(move |cv| cv.current_channel = Some(ch_for_current));

        // Load messages for first channel
        let guard = match backend.read_with_timeout(std::time::Duration::from_secs(5)).await {
            Ok(g) => g,
            Err(_) => {
                tracing::warn!("favorites_sidebar: backend read timed out loading first-channel messages");
                pending_cv.discard();
                pending_cl.discard();
                chat_view_state.batch(|cv| cv.loading = false);
                return;
            }
        };
        if let Ok(messages) = guard
            .get_messages(&ch.id, initial_message_query(ch.unread_count))
            .await
        {
            pending_cv.set(move |cv| cv.set_messages(messages));
            request_restore_scroll_position_or_bottom(&ch.id);
        }
        if let Ok(members) = guard.get_channel_members(&ch.id).await {
            pending_cv.set(move |cv| cv.members = members);
        }
    }
    if !auto_select_first_text_channel {
        nav_state.batch(|n| {
            n.selected_channel.unsafe_presync_override(
                None,
                "favorites_sidebar::load_server_data_internal: clear selected_channel \
                 when loading server shell only (mobile drawer context) — must be \
                 synchronous so ChatView doesn't briefly render a stale channel",
            );
        });
        pending_cv.set(|cv| cv.apply(ChatAction::ClearActiveChannel));
    }
    pending_cv.set(|cv| cv.loading = false);
    pending_cl.apply();
    pending_cv.apply();
    apply_server_icon_overrides(chat_lists, chat_view_state).await;
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
    client_manager: BatchedSignal<ClientManager>,
    voice_state: BatchedSignal<VoiceState>,
    chat_lists: BatchedSignal<ChatLists>,
    chat_view_state: BatchedSignal<ChatViewState>,
) -> Option<String> {
    chat_view_state.batch(|cv| cv.loading = true);

    let backend_info = client_manager.peek().get_backend_for_server(&server_id);
    let Some((_account_id, backend)) = backend_info else {
        chat_view_state.batch(|cv| cv.loading = false);
        return None;
    };

    // Load server details
    let loaded_server = {
        let guard = match backend.read_with_timeout(std::time::Duration::from_secs(5)).await {
            Ok(g) => g,
            Err(_) => {
                tracing::warn!("favorites_sidebar: backend read timed out loading server details");
                chat_view_state.batch(|cv| cv.loading = false);
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
                chat_view_state.batch(|cv| cv.loading = false);
                return None;
            }
        };
        guard.get_channels(&server_id).await.unwrap_or_default()
    };

    let exact = channels.iter().find(|c| c.id == channel_id).cloned();

    let fallback = if exact.is_none() {
        let default_id = loaded_server
            .as_ref()
            .and_then(|s| s.default_channel_id.clone());
        let by_default = default_id.and_then(|id| channels.iter().find(|c| c.id == id).cloned());
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

    let target = exact.or(fallback);

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
                    chat_view_state.batch(|cv| cv.loading = false);
                    return None;
                }
            };
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
                    chat_view_state.batch(|cv| cv.loading = false);
                    return None;
                }
            };
            if let Ok(participants) = guard.get_voice_participants(&ch.id).await {
                loaded_voice = Some((ch.id.clone(), participants));
            }
        }
    }

    chat_lists.batch(|cl| cl.set_channels(channels));
    chat_view_state.batch(|cv| {
        if let Some(server) = loaded_server {
            cv.current_server = Some(server);
        }
        if let Some(ref ch) = target {
            cv.current_channel = Some(ch.clone());
        }
        if let Some(messages) = loaded_messages {
            cv.set_messages(messages);
        }
        cv.channel_load_error = loaded_channel_load_error;
        if let Some(members) = loaded_members {
            cv.members = members;
        }
        cv.loading = false;
    });
    if let Some((id, participants)) = loaded_voice {
        voice_state.batch(move |v| {
            v.voice_channel_participants.insert(id, participants);
        });
    }

    if let Some(ch_id) = request_scroll_for {
        request_restore_scroll_position_or_bottom(&ch_id);
    }

    apply_server_icon_overrides(chat_lists, chat_view_state).await;

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
