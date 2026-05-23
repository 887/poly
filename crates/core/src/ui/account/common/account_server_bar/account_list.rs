//! Account-list sub-concern for the account server bar.
//!
//! Owns: [`AccountBarOverviewButton`], [`AccountBarNotifsButton`],
//! [`AccountBarDiscoverButton`], [`CreateServerButton`].
//! These are account-wide navigation buttons that are not DM/friends-specific
//! and not server-icon-specific (ISP — separate from dm_bar and server_list).

use crate::state::BatchedSignal;
use super::super::super::super::routes::Route;
use crate::state::{ChatAction, ChatLists, ChatViewState, NavState, View};
use crate::i18n::t;
use crate::ui::favorites_sidebar::SidebarTooltip;
use crate::ui::main_layout::close_mobile_drawer;
use dioxus::prelude::*;
use poly_ui_macros::{context_menu, ui_action};

/// Per-account Overview button — first item in the AccountServerBar.
///
/// Routes to `Route::ServerOverviewRoute` which renders the plugin-supplied
/// `get_account_overview_view` ViewDescriptor inside the standard layout
/// (channel sidebar always present).
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn AccountBarOverviewButton(
    current_view: View,
    backend_slug: String,
    instance_id: String,
    account_id: String,
) -> Element {
    let chat_view_state: BatchedSignal<ChatViewState> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();

    rsx! {
        div {
            class: if current_view == View::Overview { "server-icon active" } else { "server-icon" },
            onclick: move |_| {
                if current_view == View::Overview {
                    return;
                }
                chat_view_state.batch(|cv| cv.apply(ChatAction::ClearChannelContext));
                chat_lists.batch(|cl| cl.set_channels(Vec::new()));
                navigator()
                    .push(Route::ServerOverviewRoute {
                        backend: backend_slug.clone(),
                        instance_id: instance_id.clone(),
                        account_id: account_id.clone(),
                    });
            },
            div { class: "icon-overview", "🏠" }
            SidebarTooltip {
                line1: t("account-bar-overview-tooltip"),
                line2: None,
                line3: None,
            }
        }
    }
}

/// Notifications button — account-scoped, with unread badge.
///
/// Only rendered when `caps.should_show_notifications()` is true.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn AccountBarNotifsButton(current_view: View, notif_count: usize) -> Element {
    let nav: BatchedSignal<NavState> = use_context();
    let chat_view_state: BatchedSignal<ChatViewState> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    // Collapse three consecutive nav reads into one scoped guard — avoids three
    // separate subscriptions on NavState from this component (hang class #7).
    let (backend_slug, instance_id, account_id_notif) = {
        let nav_guard = nav.read(); // poly-lint: allow render-time-read — intentional: re-render on nav change to push correct route
        (
            nav_guard.active_backend.cloned().map_or_else(|| "demo".to_string(), |b| b.slug().to_string()),
            nav_guard.active_instance_id.cloned().unwrap_or_else(|| "demo".to_string()),
            nav_guard.active_account_id.cloned().unwrap_or_default(),
        )
    };

    rsx! {
        div {
            class: if current_view == View::Notifications { "server-icon active" } else { "server-icon" },
            onclick: move |_| {
                if current_view == View::Notifications {
                    return;
                }
                chat_view_state.batch(|cv| cv.apply(ChatAction::ClearChannelContext));
                chat_lists.batch(|cl| cl.set_channels(Vec::new()));
                crate::nav!(Route::NotificationsRoute {
                    backend: backend_slug.clone(),
                    instance_id: instance_id.clone(),
                    account_id: account_id_notif.clone(),
                });
            },
            div { class: "icon-notifications", "🔔" }
            if notif_count > 0 {
                span { class: "badge", "{notif_count}" }
            }
            SidebarTooltip {
                line1: t("nav-notifications"),
                line2: None,
                line3: None,
            }
        }
    }
}

/// Discover Communities button — navigates to the Discover route.
///
/// Only rendered when `caps.should_show_discover()` is true (Lemmy, Reddit).
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn AccountBarDiscoverButton(
    current_view: View,
    backend_slug: String,
    instance_id: String,
    account_id: String,
) -> Element {
    let chat_view_state: BatchedSignal<ChatViewState> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    rsx! {
        div {
            class: if current_view == View::DiscoverCommunities { "server-icon active" } else { "server-icon" },
            onclick: move |_| {
                if current_view == View::DiscoverCommunities {
                    return;
                }
                chat_view_state.batch(|cv| cv.apply(ChatAction::ClearChannelContext));
                chat_lists.batch(|cl| cl.set_channels(Vec::new()));
                crate::nav!(Route::DiscoverRoute {
                    backend: backend_slug.clone(),
                    instance_id: instance_id.clone(),
                    account_id: account_id.clone(),
                });
            },
            div { class: "icon-discover", "🔍" }
            SidebarTooltip {
                line1: t("nav-discover"),
                line2: None,
                line3: None,
            }
        }
    }
}

/// "+" button that lets Poly accounts create a new server/guild.
///
/// Navigates to the full-page Create Server route where FavoritesBar and
/// AccountServerBar remain visible. The inline form was replaced by the
/// full-page route to match the Settings/Signup page pattern.
#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
pub fn CreateServerButton(account_id: String) -> Element {
    let nav: BatchedSignal<NavState> = use_context();
    // Collapse two nav reads into one scoped guard — avoids two separate subscriptions.
    let (backend_slug, instance_id) = {
        let nav_guard = nav.read(); // poly-lint: allow render-time-read — intentional: re-render on nav change to push correct route
        (
            nav_guard.active_backend.cloned().map_or_else(|| "poly".to_string(), |b| b.slug().to_string()),
            nav_guard.active_instance_id.cloned().unwrap_or_default(),
        )
    };

    rsx! {
        button {
            class: "create-server-pill",
            title: "{t(\"create-server-btn\")}",
            onclick: move |_| {
                crate::nav!(Route::CreateServerRoute {
                    backend:     backend_slug.clone(),
                    instance_id: instance_id.clone(),
                    account_id:  account_id.clone(),
                });
                close_mobile_drawer();
            },
            "+"
        }
    }
}
