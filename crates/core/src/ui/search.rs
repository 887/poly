//! Global search — browse the full node tree of all accounts.
//!
//! Shows servers, channels, groups, and DMs across all active accounts
//! with per-account filtering checkboxes and a text search input.
//!
//! Each node in the tree shows which account it belongs to so users can
//! tell apart the same server joined on multiple accounts.
//!
//! # 150-line component rule
//! Each `#[component]` fn body MUST stay under 150 lines of RSX+logic.
//! Extract sub-components rather than growing this file.

use crate::client_manager::BackendHandleExt;
use crate::state::BatchedSignal;
use crate::i18n::{t, t_args};
use crate::state::{AccountSessions, AppState, ChatLists};
use crate::ui::actions::{ActionCx, UiAction};
use crate::ui::main_layout::close_mobile_drawer;
use crate::ui::routes::Route;
use crate::ui::split_shell::SplitMenuShell;
use chrono::{DateTime, Utc};
use dioxus::prelude::*;
use poly_client::BackendType;
use poly_ui_macros::{context_menu, ui_action};

/// Actions for the global search page.
#[derive(Debug, Clone)]
pub enum SearchPageAction {
    /// User typed a new search query.
    SetQuery(String),
    /// User toggled an account filter.
    ToggleAccount(String),
    /// User toggled a type filter (servers/dms/groups).
    ToggleType(String),
    /// User navigated to a result.
    NavigateTo(String),
}

impl UiAction for SearchPageAction {
    fn apply(self, _cx: ActionCx<'_>) {
        todo!("phase-E: SearchPageAction requires Signal handles");
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Derive a stable hex color from an account ID string.
fn user_color(account_id: &str) -> String {
    let hash: u32 = account_id.bytes().fold(5381_u32, |h, b| {
        h.wrapping_mul(33).wrapping_add(u32::from(b))
    });
    let hue = hash % 360;
    format!("hsl({hue}, 65%, 55%)")
}

/// Generic fallback icon for a backend in the search sidebar.
///
/// Pre-WP-7 this was a `match bt.as_str()` slug ladder. Per D27 (plan
/// `plan-client-ui-surface.md`), backend icons are plugin-declared and
/// rendered by the declarative UI surface. The global search UI does not
/// yet consume the plugin's `icon` field; until that wiring lands, every
/// account uses a single generic placeholder here.
///
/// DECISION(D27): do not re-introduce slug→emoji mapping in this file —
/// it belongs in the plugin's declaration.
fn backend_icon(_bt: &BackendType) -> &'static str {
    "📡"
}

/// Build a compact attribution string for a node: "Cat · Demo".
fn account_attribution(
    account_id: &str,
    account_sessions: &AccountSessions,
    client_manager: &crate::client_manager::ClientManager,
) -> String {
    let display_name = account_sessions
        .account_sessions
        .get(account_id).map_or_else(|| account_id.to_string(), |s| s.user.display_name.clone());
    let backend_name = client_manager
        .sessions
        .get(account_id)
        .map_or("", |s| s.backend.display_name());
    if backend_name.is_empty() {
        display_name
    } else {
        format!("{display_name} · {backend_name}")
    }
}

fn dm_last_incoming_timestamp(dm: &poly_client::DmChannel) -> Option<DateTime<Utc>> {
    dm.last_message
        .as_ref()
        .filter(|message| message.author.id == dm.user.id)
        .map(|message| message.timestamp)
}

fn group_last_incoming_timestamp(
    group: &poly_client::Group,
    active_user_id: Option<&str>,
) -> Option<DateTime<Utc>> {
    group
        .last_message
        .as_ref()
        .filter(|message| active_user_id.is_none_or(|user_id| message.author.id != user_id))
        .map(|message| message.timestamp)
}

// ── UI Components ─────────────────────────────────────────────────────────────

/// Search input bar for the global search page.
#[context_menu(allow_default)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn SearchInput(query: Signal<String>) -> Element {
    let current = query.read().clone();

    // Auto-focus the search input when this component is first mounted.
    // A short setTimeout ensures the DOM element is rendered before focusing.
    use_effect(|| {
        let _ = document::eval(
            "setTimeout(() => { \
                const el = document.querySelector('.search-page-input'); \
                if (el) el.focus(); \
            }, 50)"
        );
    });

    rsx! {
        div { class: "search-page-input-bar",
            input {
                r#type: "text",
                class: "search-page-input",
                placeholder: "{t(\"search-page-placeholder\")}",
                value: "{current}",
                oninput: move |e| query.set(e.value()),
            }
            if !current.is_empty() {
                button {
                    class: "search-page-clear",
                    onclick: move |_| query.set(String::new()),
                    "×"
                }
            }
        }
    }
}

/// Small circular avatar icon.
///
/// Renders an `<img>` if `url` is `Some`, otherwise a coloured bubble using
/// the first character of `label` as a fallback initial.
#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn AvatarIcon(url: Option<String>, label: String, color: String) -> Element {
    let initial = label
        .chars()
        .next().map_or_else(|| "?".to_string(), |c| c.to_uppercase().to_string());
    rsx! {
        if let Some(img_url) = url {
            img {
                class: "search-avatar-icon",
                src: "{img_url}",
                alt: "{label}",
            }
        } else {
            div {
                class: "search-avatar-icon search-avatar-icon-fallback",
                style: "background: {color};",
                "{initial}"
            }
        }
    }
}

/// Per-account toggle in the sidebar — shows avatar/icon, name, and backend.
///
/// Uses the same toggle-switch UI as the settings pages
/// (`role="switch"` + `.toggle-slider`) so the search account filter
/// looks consistent with the rest of the app instead of using raw browser
/// checkboxes.
#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn AccountFilter(
    account_id: String,
    display_name: String,
    avatar_url: Option<String>,
    icon_color: String,
    backend_name: String,
    backend_icon_str: String,
    enabled: bool,
    on_toggle: EventHandler<String>,
) -> Element {
    let aid = account_id.clone();
    rsx! {
        label { class: "search-account-filter",
            label { class: "toggle-switch",
                input {
                    r#type: "checkbox",
                    role: "switch",
                    "aria-checked": "{enabled}",
                    checked: enabled,
                    onchange: move |_| on_toggle.call(aid.clone()),
                }
                span { class: "toggle-slider" }
            }
            AvatarIcon {
                url: avatar_url,
                label: display_name.clone(),
                color: icon_color,
            }
            div { class: "search-account-filter-info",
                span { class: "search-account-filter-name", "{display_name}" }
                span { class: "search-account-filter-backend",
                    "{backend_icon_str} {backend_name}"
                }
            }
        }
    }
}

/// "All" master toggle — flips every per-account toggle below.
///
/// Tri-state visual: indeterminate when partially-enabled, on when all
/// accounts are enabled, off when none. Click toggles all on (when off
/// or partial) or all off (when fully on).
#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn AccountFilterAllToggle(
    enabled_count: usize,
    total_count: usize,
    on_toggle_all: EventHandler<bool>,
) -> Element {
    let all_on = enabled_count == total_count && total_count > 0;
    let partial = enabled_count > 0 && enabled_count < total_count;
    let label = if all_on { "All accounts" } else if partial { "Some accounts" } else { "No accounts" };
    rsx! {
        label { class: "search-account-filter search-account-filter-all",
            label { class: "toggle-switch",
                input {
                    r#type: "checkbox",
                    role: "switch",
                    "aria-checked": "{all_on}",
                    checked: all_on,
                    "data-indeterminate": "{partial}",
                    onchange: move |_| on_toggle_all.call(!all_on),
                }
                span { class: "toggle-slider" }
            }
            div { class: "search-account-filter-info",
                span { class: "search-account-filter-name", "{label}" }
                span { class: "search-account-filter-backend",
                    "{enabled_count} of {total_count}"
                }
            }
        }
    }
}

/// A single node row in the search tree.
#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn NodeRow(
    icon: String,
    label: String,
    highlight_terms: Vec<String>,
    sublabel: String,
    on_click: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        div { class: "search-node-row",
            onclick: move |evt| on_click.call(evt),
            span { class: "search-node-icon", "{icon}" }
            div { class: "search-node-info",
                HighlightedSearchText {
                    class_name: "search-node-label".to_string(),
                    text: label,
                    search_terms: highlight_terms,
                }
                if !sublabel.is_empty() {
                    span { class: "search-node-sublabel", "{sublabel}" }
                }
            }
        }
    }
}

/// A node row with an avatar icon — used for DM and group entries.
#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn AvatarNodeRow(
    avatar_url: Option<String>,
    avatar_label: String,
    avatar_color: String,
    label: String,
    highlight_terms: Vec<String>,
    sublabel: String,
    on_click: EventHandler<MouseEvent>,
) -> Element {
    rsx! {
        div { class: "search-node-row",
            onclick: move |evt| on_click.call(evt),
            AvatarIcon {
                url: avatar_url,
                label: avatar_label,
                color: avatar_color,
            }
            div { class: "search-node-info",
                HighlightedSearchText {
                    class_name: "search-node-label".to_string(),
                    text: label,
                    search_terms: highlight_terms,
                }
                if !sublabel.is_empty() {
                    span { class: "search-node-sublabel", "{sublabel}" }
                }
            }
        }
    }
}

fn build_highlight_terms(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .filter(|term| !term.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn HighlightedSearchText(class_name: String, text: String, search_terms: Vec<String>) -> Element {
    let lowercase_text = text.to_lowercase();
    let found_match = search_terms.into_iter().find_map(|term| {
        let lowercase_term = term.to_lowercase();
        lowercase_text
            .find(&lowercase_term)
            .map(|index| (index, index.saturating_add(lowercase_term.len())))
    });

    if let Some((start, end)) = found_match {
        let before = text.get(..start).unwrap_or_default().to_string();
        let matched = text.get(start..end).unwrap_or_default().to_string();
        let after = text.get(end..).unwrap_or_default().to_string();
        rsx! {
            span { class: "{class_name}",
                "{before}"
                mark { class: "search-result-match", "{matched}" }
                "{after}"
            }
        }
    } else {
        rsx! { span { class: "{class_name}", "{text}" } }
    }
}

/// Server section with its channels and account attribution in the header.
#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn ServerNode(
    server_id: String,
    server_name: String,
    icon_url: Option<String>,
    backend_slug: String,
    instance_id: String,
    account_id: String,
    account_attribution: String,
    query: String,
    highlight_terms: Vec<String>,
) -> Element {
    let client_manager: BatchedSignal<crate::client_manager::ClientManager> = use_context();
    let q_lower = query.to_lowercase();

    let sid = server_id.clone();
    let server_channels = use_resource(move || {
        let sid = sid.clone();
        let cm = client_manager;
        async move {
            let backend_info = cm.read().get_backend_for_server(&sid);
            if let Some((_aid, backend)) = backend_info {
                let guard = match backend.read_with_timeout(std::time::Duration::from_secs(5)).await {
                    Ok(g) => g,
                    Err(_) => {
                        tracing::warn!("search: backend read timed out fetching channels");
                        return Vec::new();
                    }
                };
                guard.get_channels(&sid).await.unwrap_or_default()
            } else {
                Vec::new()
            }
        }
    });

    let name_matches = q_lower.is_empty() || server_name.to_lowercase().contains(&q_lower);
    let server_color = user_color(&server_id);

    // Check if there are any matching channels before rendering the server
    if let Some(channels) = server_channels.read().as_ref() {
        let has_matching_channels = channels.iter().any(|ch| {
            let ch_name = ch.name.clone();
            q_lower.is_empty()
                || ch_name.to_lowercase().contains(&q_lower)
                || name_matches
        });

        if !has_matching_channels {
            return rsx! {};
        }
    }

    rsx! {
        div { class: "search-server-node",
            div { class: "search-server-header",
                AvatarIcon {
                    url: icon_url,
                    label: server_name.clone(),
                    color: server_color,
                }
                div { class: "search-server-header-info",
                    HighlightedSearchText {
                        class_name: "search-node-label".to_string(),
                        text: server_name,
                        search_terms: highlight_terms.clone(),
                    }
                    span { class: "search-node-account-badge", "{account_attribution}" }
                }
            }
            if let Some(channels) = server_channels.read().as_ref() {
                div { class: "search-server-channels",
                    for ch in channels.iter() {
                        {
                            let ch_name = ch.name.clone();
                            let ch_matches = q_lower.is_empty()
                                || ch_name.to_lowercase().contains(&q_lower)
                                || name_matches;
                            if ch_matches {
                                let icon = match ch.channel_type {
                                    poly_client::ChannelType::Text
                                    | poly_client::ChannelType::Thread
                                    | poly_client::ChannelType::Announcement => "#".to_string(),
                                    poly_client::ChannelType::Voice => "🔊".to_string(),
                                    poly_client::ChannelType::Video => "📹".to_string(),
                                    poly_client::ChannelType::Forum
                                    | poly_client::ChannelType::HackerNews => "📋".to_string(),
                                    poly_client::ChannelType::Code => "📁".to_string(),
                                };
                                let sid_c = server_id.clone();
                                let chid = ch.id.clone();
                                let bs = backend_slug.clone();
                                let iid = instance_id.clone();
                                let aid = account_id.clone();
                                rsx! {
                                    NodeRow {
                                        icon,
                                        label: ch_name,
                                        highlight_terms: highlight_terms.clone(),
                                        sublabel: String::new(),
                                        on_click: move |_| {
                                            close_mobile_drawer();
                                            crate::nav!(Route::ServerChat {
                                                backend: bs.clone(),
                                                instance_id: iid.clone(),
                                                account_id: aid.clone(),
                                                server_id: sid_c.clone(),
                                                channel_id: chid.clone(),
                                            });
                                        },
                                    }
                                }
                            } else {
                                rsx! {}
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Type filter checkboxes — Servers / DMs / Groups.
#[context_menu(inherit)]
#[rustfmt::skip]
#[ui_action(inherit)]
#[component]
fn TypeFilters(enabled_types: Signal<std::collections::HashSet<String>>) -> Element {
    let types: &[(&str, &str)] = &[
        ("servers", "search-type-servers"),
        ("dms",     "search-type-dms"),
        ("groups",  "search-type-groups"),
    ];
    rsx! {
        div { class: "search-type-filters",
            span { class: "search-type-filters-label", "{t(\"search-page-type-filter\")}" }
            for (type_key, i18n_key) in types {
                {
                    let tk = type_key.to_string();
                    let label = t(i18n_key);
                    let checked = enabled_types.read().contains(*type_key);
                    rsx! {
                        label { class: "search-type-filter-item",
                            label { class: "toggle-switch",
                                input {
                                    r#type: "checkbox",
                                    role: "switch",
                                    "aria-checked": "{checked}",
                                    checked,
                                    onchange: move |_| {
                                        let mut set = enabled_types.write();
                                        if set.contains(&tk) {
                                            set.remove(&tk);
                                        } else {
                                            set.insert(tk.clone());
                                        }
                                    },
                                }
                                span { class: "toggle-slider" }
                            }
                            span { "{label}" }
                        }
                    }
                }
            }
        }
    }
}

/// Global search page — sidebar with account filters + right tree of all nodes.
#[context_menu(None)]
#[rustfmt::skip]
#[ui_action(SearchPageAction)]
#[component]
pub fn SearchPage(
    /// When `Some(account_id)`, the search is initialised with only that account
    /// enabled (account-scoped entry from the favourites bar). The user can still
    /// toggle other accounts on manually to broaden the search.
    /// When `None`, all accounts start enabled (global search).
    locked_account_id: Option<String>,
) -> Element {
    let app_state: BatchedSignal<AppState> = use_context();
    let user_prefs: crate::state::BatchedSignal<crate::state::UserPrefs> = use_context();
    let chat_lists: BatchedSignal<ChatLists> = use_context();
    let account_sessions: BatchedSignal<AccountSessions> = use_context();
    let client_manager: BatchedSignal<crate::client_manager::ClientManager> = use_context();
    let query = use_signal(String::new);
    let initial_type_seed = user_prefs.read().search_type_seed.clone();
    let mut enabled_accounts: Signal<std::collections::HashSet<String>> = use_signal(|| {
        // If we have a locked account, start with only that account enabled.
        if let Some(ref aid) = locked_account_id {
            let mut set = std::collections::HashSet::new();
            set.insert(aid.clone());
            set
        } else {
            client_manager.read().active_account_ids().into_iter().collect()
        }
    });
    let mut enabled_types: Signal<std::collections::HashSet<String>> = use_signal(|| {
        initial_type_seed.clone().map_or_else(
            || {
                ["servers", "dms", "groups"]
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect::<std::collections::HashSet<_>>()
            },
            |seed| seed.into_iter().collect::<std::collections::HashSet<_>>(),
        )
    });
    // Infinite-scroll visible counts (incremented on scroll near bottom)
    let mut dm_visible: Signal<usize> = use_signal(|| 20_usize);
    let mut grp_visible: Signal<usize> = use_signal(|| 20_usize);

    use_effect(move || {
        let seed = user_prefs.read().search_type_seed.clone();
        if let Some(seed) = seed {
            enabled_types.set(seed.into_iter().collect());
            user_prefs.batch(|p| p.search_type_seed = None);
        }
    });

    let account_ids = client_manager.read().active_account_ids();
    let query_text = query.read().clone();
    let q_lower = query_text.to_lowercase();
    let highlight_terms = build_highlight_terms(&query_text);
    let servers = chat_lists.read().servers.clone(); // poly-lint: allow render-time-read — render snapshot; subscription intentional
    let mut dm_channels = chat_lists.read().dm_channels.clone(); // poly-lint: allow render-time-read — render snapshot; subscription intentional
    let mut groups = chat_lists.read().groups.clone(); // poly-lint: allow render-time-read — render snapshot; subscription intentional
    let account_user_ids: std::collections::HashMap<String, String> = account_sessions
        .read() // poly-lint: allow render-time-read — render snapshot; subscription intentional
        .account_sessions
        .iter()
        .map(|(account_id, session)| (account_id.clone(), session.user.id.clone()))
        .collect();

    dm_channels.sort_by(|a, b| {
        dm_last_incoming_timestamp(b)
            .cmp(&dm_last_incoming_timestamp(a))
            .then_with(|| b.last_message.as_ref().map(|m| m.timestamp).cmp(&a.last_message.as_ref().map(|m| m.timestamp)))
            .then_with(|| a.user.display_name.cmp(&b.user.display_name))
    });

    groups.sort_by(|a, b| {
        let a_active_user_id = account_user_ids.get(&a.account_id).map(String::as_str);
        let b_active_user_id = account_user_ids.get(&b.account_id).map(String::as_str);
        group_last_incoming_timestamp(b, b_active_user_id)
            .cmp(&group_last_incoming_timestamp(a, a_active_user_id))
            .then_with(|| b.last_message.as_ref().map(|m| m.timestamp).cmp(&a.last_message.as_ref().map(|m| m.timestamp)))
            .then_with(|| a.name.cmp(&b.name))
    });

    // Collect filtered DM/group lists to know total counts for the counter badge.
    let visible_dms: Vec<_> = dm_channels
        .iter()
        .filter(|dm| enabled_accounts.read().contains(&dm.account_id))
        .filter(|dm| {
            q_lower.is_empty() || dm.user.display_name.to_lowercase().contains(&q_lower)
        })
        .cloned()
        .collect();

    let visible_grps: Vec<_> = groups
        .iter()
        .filter(|g| enabled_accounts.read().contains(&g.account_id))
        .filter(|g| {
            let name = g.name.clone().unwrap_or_else(|| {
                g.members.iter().map(|m| m.display_name.clone()).collect::<Vec<_>>().join(", ")
            });
            q_lower.is_empty() || name.to_lowercase().contains(&q_lower)
        })
        .cloned()
        .collect();

    let dm_total = visible_dms.len();
    let grp_total = visible_grps.len();

    rsx! {
        SplitMenuShell {
            root_class: "search-page".to_string(),
            sidebar_class: "search-page-sidebar".to_string(),
            content_class: "search-page-results".to_string(),
            sidebar: rsx! {
                div { class: "search-page-filters",
                    h3 { "{t(\"search-page-accounts\")}" }
                    // Master toggle — flips every per-account toggle below.
                    {
                        let total_count = account_ids.len();
                        let enabled_count = enabled_accounts.read().len();
                        let account_ids_for_all = account_ids.clone();
                        rsx! {
                            AccountFilterAllToggle {
                                enabled_count,
                                total_count,
                                on_toggle_all: move |new_all_state: bool| {
                                    let mut set = enabled_accounts.write();
                                    if new_all_state {
                                        for aid in &account_ids_for_all {
                                            set.insert(aid.clone());
                                        }
                                    } else {
                                        set.clear();
                                    }
                                },
                            }
                        }
                    }
                    for aid in &account_ids {
                        {
                            let as_ = account_sessions.peek();
                            let cm = client_manager.read();
                            let session = as_.account_sessions.get(aid);
                            let display_name = session.map_or_else(|| aid.clone(), |s| s.user.display_name.clone());
                            let avatar_url = session
                                .and_then(|s| s.user.avatar_url.clone());
                            let icon_color = user_color(aid);
                            let bt = cm.sessions.get(aid).map_or(BackendType::from("demo"), |s| s.backend.clone());
                            let backend_name = bt.display_name().to_string();
                            let backend_icon_str = backend_icon(&bt).to_string();
                            let enabled = enabled_accounts.read().contains(aid);
                            drop(as_);
                            drop(cm);
                            rsx! {
                                AccountFilter {
                                    account_id: aid.clone(),
                                    display_name,
                                    avatar_url,
                                    icon_color,
                                    backend_name,
                                    backend_icon_str,
                                    enabled,
                                    on_toggle: move |id: String| {
                                        let mut set = enabled_accounts.write();
                                        if set.contains(&id) {
                                            set.remove(&id);
                                        } else {
                                            set.insert(id);
                                        }
                                    },
                                }
                            }
                        }
                    }
                }
            },

            // ── Results tree — scrollable, infinite-loads DMs+Groups ──
            content: rsx! {
                div { class: "search-page-header",
                    h2 { "{t(\"search-page-title\")}" }
                    SearchInput { query }
                    TypeFilters { enabled_types }
                }
                div { class: "search-page-results-tree",
                onscroll: move |_| {
                    // Spawn async to evaluate scroll position and load more if near bottom.
                    spawn(async move {
                        let js = "(() => { \
                            const el = document.querySelector('.search-page-results'); \
                            if (!el) return false; \
                            return el.scrollTop + el.clientHeight >= el.scrollHeight - 300; \
                        })()";
                        let near_bottom = dioxus::document::eval(js)
                            .await
                            .ok()
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        if near_bottom {
                            *dm_visible.write() += 20;
                            *grp_visible.write() += 20;
                        }
                    });
                },

                // Servers (always full-list; channels are nested and async-loaded)
                if enabled_types.read().contains("servers") {
                    for server in &servers {
                        {
                            if !enabled_accounts.read().contains(&server.account_id) {
                                return rsx! {};
                            }
                            let attribution = account_attribution(
                                &server.account_id,
                                &account_sessions.peek(),
                                &client_manager.read(),
                            );
                            let instance_id = account_sessions
                                .peek()
                                .account_sessions
                                .get(&server.account_id).map_or_else(|| "demo".to_string(), |s| s.instance_id.clone());
                            rsx! {
                                ServerNode {
                                    key: "{server.id}-{server.account_id}",
                                    server_id: server.id.clone(),
                                    server_name: server.name.clone(),
                                    icon_url: server.icon_url.clone(),
                                    backend_slug: server.backend.slug().to_string(),
                                    instance_id,
                                    account_id: server.account_id.clone(),
                                    account_attribution: attribution,
                                    query: q_lower.clone(),
                                    highlight_terms: highlight_terms.clone(),
                                }
                            }
                        }
                    }
                }

                // DMs — paginated via dm_visible
                if enabled_types.read().contains("dms") && !visible_dms.is_empty() {
                    div { class: "search-section-header",
                        span { "{t(\"search-page-dms\")}" }
                        if dm_total > 0 {
                            span { class: "search-section-count",
                                {
                                    let shown = dm_visible.cloned().min(dm_total);
                                    t_args("search-showing-of", &[
                                        ("count", &shown.to_string()),
                                        ("total", &dm_total.to_string()),
                                    ])
                                }
                            }
                        }
                    }
                    for dm in visible_dms.iter().take(*dm_visible.read()) {
                        {
                            let dm_id = dm.id.clone();
                            let bs = dm.backend.slug().to_string();
                            let attribution = account_attribution(
                                &dm.account_id,
                                &account_sessions.peek(),
                                &client_manager.read(),
                            );
                            let iid = account_sessions
                                .peek()
                                .account_sessions
                                .get(&dm.account_id).map_or_else(|| "demo".to_string(), |s| s.instance_id.clone());
                            let aid = dm.account_id.clone();
                            let dm_avatar_url = dm.user.avatar_url.clone();
                            let dm_display_name = dm.user.display_name.clone();
                            let dm_color = user_color(&dm.user.id);
                            let name = dm.user.display_name.clone();
                            rsx! {
                                AvatarNodeRow {
                                    key: "{dm.id}-{aid}",
                                    avatar_url: dm_avatar_url,
                                    avatar_label: dm_display_name,
                                    avatar_color: dm_color,
                                    label: name,
                                    highlight_terms: highlight_terms.clone(),
                                    sublabel: attribution,
                                    on_click: move |_| {
                                        close_mobile_drawer();
                                        crate::nav!(Route::DmChat {
                                            backend: bs.clone(),
                                            instance_id: iid.clone(),
                                            account_id: aid.clone(),
                                            dm_id: dm_id.clone(),
                                        });
                                    },
                                }
                            }
                        }
                    }
                    if *dm_visible.read() < dm_total {
                        div { class: "search-load-more-hint",
                            span { "{t(\"search-load-more\")}" }
                        }
                    }
                }

                // Groups — paginated via grp_visible
                if enabled_types.read().contains("groups") && !visible_grps.is_empty() {
                    div { class: "search-section-header",
                        span { "{t(\"search-page-groups\")}" }
                        if grp_total > 0 {
                            span { class: "search-section-count",
                                {
                                    let shown = grp_visible.cloned().min(grp_total);
                                    t_args("search-showing-of", &[
                                        ("count", &shown.to_string()),
                                        ("total", &grp_total.to_string()),
                                    ])
                                }
                            }
                        }
                    }
                    for group in visible_grps.iter().take(*grp_visible.read()) {
                        {
                            let name = group.name.clone().unwrap_or_else(|| {
                                group
                                    .members
                                    .iter()
                                    .map(|m| m.display_name.clone())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            });
                            let gid = group.id.clone();
                            let bs = group.backend.slug().to_string();
                            let attribution = account_attribution(
                                &group.account_id,
                                &account_sessions.peek(),
                                &client_manager.read(),
                            );
                            let iid = account_sessions
                                .peek()
                                .account_sessions
                                .get(&group.account_id).map_or_else(|| "demo".to_string(), |s| s.instance_id.clone());
                            let aid = group.account_id.clone();
                            let grp_avatar_url = group.members.first().and_then(|m| m.avatar_url.clone());
                            let grp_label = name.clone();
                            let grp_color = user_color(&group.id);
                            rsx! {
                                AvatarNodeRow {
                                    key: "{group.id}-{aid}",
                                    avatar_url: grp_avatar_url,
                                    avatar_label: grp_label,
                                    avatar_color: grp_color,
                                    label: name,
                                    highlight_terms: highlight_terms.clone(),
                                    sublabel: attribution,
                                    on_click: move |_| {
                                        close_mobile_drawer();
                                        crate::nav!(Route::DmChat {
                                            backend: bs.clone(),
                                            instance_id: iid.clone(),
                                            account_id: aid.clone(),
                                            dm_id: gid.clone(),
                                        });
                                    },
                                }
                            }
                        }
                    }
                    if *grp_visible.read() < grp_total {
                        div { class: "search-load-more-hint",
                            span { "{t(\"search-load-more\")}" }
                        }
                    }
                }
                }
            },
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn search_page_action_variants_compile() {
        fn assert_ui_action<T: crate::ui::actions::UiAction>() {}
        assert_ui_action::<SearchPageAction>();
        let _ = SearchPageAction::SetQuery("q".into());
        let _ = SearchPageAction::ToggleAccount("acc".into());
        let _ = SearchPageAction::ToggleType("servers".into());
        let _ = SearchPageAction::NavigateTo("dest".into());
    }
}
