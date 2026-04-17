//! User sidebar — channel member list (right panel).
//!
//! Common implementation shared across all messenger backends.
//! Backend-specific user card decorations (roles, badges, verification)
//! will live in per-backend directories in future phases.
//!
//! ## Member filter
//! A collapsible filter row lives below the header. Click the 🔍 icon to
//! reveal the input; type to filter by display name (case-insensitive
//! substring). The matching substring is highlighted in each visible entry.
//!
//! Reads members from `Signal<ChatData>` and groups them by
//! presence status: Online, Idle, Do Not Disturb, Offline.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.
// TODO(phase-2.5.7): Wire user sidebar to backend data

use crate::i18n::t;
use crate::state::ChatData;
use crate::state::chat_data::user_color;
use crate::state::{AppState, MemberListGrouping, MemberListSortOrder};
use crate::ui::account::common::user_profile_modal::open_user_profile;
use dioxus::prelude::*;
use poly_client::{PresenceStatus, User};
use poly_ui_macros::context_menu;

/// Sort-rank for a presence status (lower = shown first).
fn presence_rank(p: PresenceStatus) -> u8 {
    match p {
        PresenceStatus::Online => 0,
        PresenceStatus::Idle => 1,
        PresenceStatus::DoNotDisturb => 2,
        PresenceStatus::Offline | PresenceStatus::Invisible => 3,
    }
}

/// Sort a user slice according to `sort_order`.
/// Does not reorder when `JoinOrder` is selected.
fn apply_sort(mut users: Vec<User>, sort_order: MemberListSortOrder) -> Vec<User> {
    match sort_order {
        MemberListSortOrder::JoinOrder => {}
        MemberListSortOrder::Alphabetical => {
            users.sort_by(|a, b| {
                a.display_name
                    .to_lowercase()
                    .cmp(&b.display_name.to_lowercase())
            });
        }
        MemberListSortOrder::OnlineFirst => {
            users.sort_by(|a, b| {
                presence_rank(a.presence)
                    .cmp(&presence_rank(b.presence))
                    .then_with(|| {
                        a.display_name
                            .to_lowercase()
                            .cmp(&b.display_name.to_lowercase())
                    })
            });
        }
    }
    users
}

/// Rendered member list body — switches between grouped and flat layout.
///
/// If `query` is empty or not found, renders the full name as plain text.
#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
fn HighlightedName(name: String, query: String) -> Element {
    if query.is_empty() {
        return rsx! { span { "{name}" } };
    }
    let lower_name = name.to_lowercase();
    let lower_q = query.to_lowercase();
    if let Some(pos) = lower_name.find(lower_q.as_str()) {
        let before = name[..pos].to_string();
        let matched = name[pos..pos + lower_q.len()].to_string();
        let after = name[pos + lower_q.len()..].to_string();
        rsx! {
            span {
                "{before}"
                mark { class: "member-name-highlight", "{matched}" }
                "{after}"
            }
        }
    } else {
        rsx! { span { "{name}" } }
    }
}

/// User sidebar component.
///
/// Shows channel members with layout controlled by the user's member-list
/// display preferences (`AppState.member_list_grouping`, `…sort_order`,
/// `…show_offline`).
///
/// A collapsible filter (🔍 icon → text input) lets users search by name.
#[context_menu(None)]
#[rustfmt::skip]
#[component]
pub fn UserSidebar() -> Element {
    let chat_data: Signal<ChatData> = use_context();
    let app_state: Signal<AppState> = use_context();
    let members = chat_data.read().members.clone();
    let mut filter_open = use_signal(|| false);
    let mut filter_text = use_signal(String::new);

    // Read display preferences from global AppState.
    let grouping   = app_state.read().member_list_grouping;
    let sort_order = app_state.read().member_list_sort_order;
    let show_offline = app_state.read().member_list_show_offline;

    let query   = filter_text.read().clone();
    let lower_q = query.to_lowercase();

    // 1. Name filter
    let after_filter: Vec<User> = if lower_q.is_empty() {
        members.clone()
    } else {
        members
            .iter()
            .filter(|u| u.display_name.to_lowercase().contains(&lower_q))
            .cloned()
            .collect()
    };

    // 2. Offline visibility filter
    let after_offline: Vec<User> = if show_offline {
        after_filter.clone()
    } else {
        after_filter
            .iter()
            .filter(|u| !matches!(u.presence, PresenceStatus::Offline | PresenceStatus::Invisible))
            .cloned()
            .collect()
    };

    rsx! {
        aside { class: "user-sidebar",
            div { class: "chat-utility-header user-sidebar-header",
                h3 { class: "chat-utility-title user-sidebar-title", "{t(\"user-members\")}" }
                button {
                    class: if *filter_open.read() {
                        "header-btn active chat-utility-filter-btn user-filter-btn user-filter-btn-active"
                    } else {
                        "header-btn chat-utility-filter-btn user-filter-btn"
                    },
                    title: "{t(\"member-filter-tooltip\")}",
                    onclick: move |_| {
                        let was_open = *filter_open.read();
                        filter_open.set(!was_open);
                        if was_open {
                            filter_text.set(String::new());
                        }
                    },
                    "🔍"
                }
            }

            div { class: "chat-utility-body user-sidebar-body",
                if *filter_open.read() {
                    div { class: "chat-utility-filter-row user-sidebar-filter-row",
                        input {
                            r#type: "text",
                            class: "chat-utility-filter-input member-filter-input",
                            placeholder: "{t(\"member-filter-placeholder\")}",
                            value: "{query}",
                            oninput: move |e| filter_text.set(e.value()),
                            autofocus: true,
                        }
                    }
                }

                if members.is_empty() {
                    div { class: "user-sidebar-empty", "{t(\"user-no-members\")}" }
                } else if after_offline.is_empty() {
                    div { class: "user-sidebar-empty",
                        if lower_q.is_empty() { {t("user-all-offline-hidden")} } else { {t("member-filter-no-results")} }
                    }
                } else if matches!(grouping, MemberListGrouping::NoGrouping) {
                    // ── Flat list with applied sort ────────────────────────
                    {
                        let sorted = apply_sort(after_offline.clone(), sort_order);
                        rsx! {
                            UserGroup {
                                label: format!("{} — {}", t("user-members"), sorted.len()),
                                users: sorted,
                                presence_class: "",
                                query: query.clone(),
                                on_click: move |u: User| open_user_profile(app_state, u),
                            }
                        }
                    }
                } else {
                    // ── Grouped by presence status ─────────────────────────
                    {
                        let mut online: Vec<User>  = after_offline.iter().filter(|u| u.presence == PresenceStatus::Online).cloned().collect();
                        let mut idle: Vec<User>    = after_offline.iter().filter(|u| u.presence == PresenceStatus::Idle).cloned().collect();
                        let mut dnd: Vec<User>     = after_offline.iter().filter(|u| u.presence == PresenceStatus::DoNotDisturb).cloned().collect();
                        let mut offline: Vec<User> = after_offline.iter().filter(|u| matches!(u.presence, PresenceStatus::Offline | PresenceStatus::Invisible)).cloned().collect();
                        if !matches!(sort_order, MemberListSortOrder::JoinOrder) {
                            for bucket in [&mut online, &mut idle, &mut dnd, &mut offline] {
                                bucket.sort_by(|a, b| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase()));
                            }
                        }
                        rsx! {
                            if !online.is_empty() {
                                UserGroup { label: format!("{} — {}", t("user-online"), online.len()), users: online, presence_class: "online", query: query.clone(), on_click: move |u: User| open_user_profile(app_state, u) }
                            }
                            if !idle.is_empty() {
                                UserGroup { label: format!("{} — {}", t("user-idle"), idle.len()), users: idle, presence_class: "idle", query: query.clone(), on_click: move |u: User| open_user_profile(app_state, u) }
                            }
                            if !dnd.is_empty() {
                                UserGroup { label: format!("{} — {}", t("user-dnd"), dnd.len()), users: dnd, presence_class: "dnd", query: query.clone(), on_click: move |u: User| open_user_profile(app_state, u) }
                            }
                            if !offline.is_empty() {
                                UserGroup { label: format!("{} — {}", t("user-offline"), offline.len()), users: offline, presence_class: "offline", query: query.clone(), on_click: move |u: User| open_user_profile(app_state, u) }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A group of users under a presence header.
#[context_menu(inherit)]
#[rustfmt::skip]
#[component]
fn UserGroup(
    label: String,
    users: Vec<User>,
    /// Presence class for the group header section (e.g. "offline" dims entries).
    /// Pass "" for flat/no-grouping lists — per-user presence is used for dots.
    presence_class: &'static str,
    /// Current filter query for substring highlighting (empty = no highlight).
    query: String,
    on_click: EventHandler<User>,
) -> Element {
    rsx! {
        div { class: "user-group",
            h5 { class: "user-group-title", "{label}" }
            for user in &users {
                {
                    let color = user_color(&user.id);
                    let first_char: String = user
                        .display_name
                        .chars()
                        .next()
                        .map(|c| c.to_string())
                        .unwrap_or_default();
                    let name = user.display_name.clone();
                    let u = user.clone();
                    let avatar_url = user.avatar_url.clone();
                    let q = query.clone();
                    // Use per-user presence for both the entry class and the dot.
                    let is_offline = matches!(user.presence, PresenceStatus::Offline | PresenceStatus::Invisible);
                    let entry_class = if is_offline || presence_class == "offline" {
                        "user-entry offline"
                    } else {
                        "user-entry"
                    };
                    let dot_class: &'static str = match user.presence {
                        PresenceStatus::Online => "presence-dot online",
                        PresenceStatus::Idle => "presence-dot idle",
                        PresenceStatus::DoNotDisturb => "presence-dot dnd",
                        PresenceStatus::Offline | PresenceStatus::Invisible => "",
                    };
                    rsx! {
                        div { class: "{entry_class}", onclick: move |_| on_click.call(u.clone()),
                            div { class: "user-avatar-wrap",
                                div { class: "user-avatar",
                                    if let Some(ref url) = avatar_url {
                                        img { class: "user-avatar-image", src: "{url}", alt: "{name}" }
                                    } else {
                                        div {
                                            class: "user-avatar-fallback",
                                            style: "background-color: {color};",
                                            "{first_char}"
                                        }
                                    }
                                }
                                if !dot_class.is_empty() {
                                    span { class: "{dot_class}" }
                                }
                            }
                            span { class: "user-name",
                                HighlightedName { name: name.clone(), query: q.clone() }
                            }
                        }
                    }
                }
            }
        }
    }
}
