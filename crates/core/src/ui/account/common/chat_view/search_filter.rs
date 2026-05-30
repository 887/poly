//! Search-filter helpers and UI components for the chat view search bar.
//!
//! Contains the `SearchFilterSuggestion` and `SearchFilterOption` types,
//! the static suggestion list, query parsing helpers, completion logic,
//! and the `SearchFilterPopup` / `SearchFilterRow` components and
//! `render_chat_header_search` / `render_search_clear_button` render functions.

use crate::i18n::{t, t_args};
use dioxus::prelude::*;
use poly_client::{MessageSearchHit, MessageSearchQuery};
use poly_ui_macros::{context_menu, ui_action};
use super::ChatUtilityPanel;

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
pub(super) struct SearchFilterSuggestion {
    pub(super) icon: &'static str,
    pub(super) title_key: &'static str,
    pub(super) subtitle_key: &'static str,
    pub(super) token: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SearchFilterOption {
    pub(super) icon: &'static str,
    pub(super) title: String,
    pub(super) subtitle: String,
    pub(super) token: String,
    pub(super) completion_token: String,
}

// ── Static suggestion list ────────────────────────────────────────────────────

pub(super) const SEARCH_FILTER_SUGGESTIONS: &[SearchFilterSuggestion] = &[
    SearchFilterSuggestion {
        icon: "👤",
        title_key: "search-filter-from-user",
        subtitle_key: "search-filter-from-user-subtitle",
        token: "from:alice",
    },
    SearchFilterSuggestion {
        icon: "#",
        title_key: "search-filter-in-channel",
        subtitle_key: "search-filter-in-channel-subtitle",
        token: "in:#current",
    },
    SearchFilterSuggestion {
        icon: "🔗",
        title_key: "search-filter-has-link",
        subtitle_key: "search-filter-has-link-subtitle",
        token: "has:link",
    },
    SearchFilterSuggestion {
        icon: "@",
        title_key: "search-filter-mentions",
        subtitle_key: "search-filter-mentions-subtitle",
        token: "mentions:me",
    },
    SearchFilterSuggestion {
        icon: "☷",
        title_key: "search-filter-more",
        subtitle_key: "search-filter-more-subtitle",
        token: "has:link from:alice",
    },
];

// ── Pure helper functions ─────────────────────────────────────────────────────

pub(super) fn completion_token_for_search_filter(token: &str) -> String {
    if token.starts_with("from:") {
        return "from:".to_string();
    }
    if token.starts_with("in:#") {
        return "in:#".to_string();
    }
    if token.starts_with("has:") {
        return "has:".to_string();
    }
    if token.starts_with("mentions:") {
        return "mentions:".to_string();
    }
    token.to_string()
}

pub(super) fn build_search_filter_options(current_channel_name: &str) -> Vec<SearchFilterOption> {
    SEARCH_FILTER_SUGGESTIONS
        .iter()
        .map(|suggestion| {
            let token = if suggestion.token == "in:#current" {
                format!("in:#{current_channel_name}")
            } else {
                suggestion.token.to_string()
            };

            SearchFilterOption {
                icon: suggestion.icon,
                title: t(suggestion.title_key),
                subtitle: t(suggestion.subtitle_key),
                completion_token: completion_token_for_search_filter(&token),
                token,
            }
        })
        .collect()
}

pub(super) fn active_search_filter_term(raw_query: &str) -> &str {
    raw_query
        .split_whitespace()
        .last()
        .map_or("", str::trim)
}

pub(super) fn filter_search_filter_options(
    options: &[SearchFilterOption],
    raw_query: &str,
) -> Vec<SearchFilterOption> {
    let term = active_search_filter_term(raw_query).to_ascii_lowercase();
    if term.is_empty() {
        return options.to_vec();
    }

    options
        .iter()
        .filter(|option| {
            option
                .completion_token
                .to_ascii_lowercase()
                .starts_with(&term)
                || option.token.to_ascii_lowercase().contains(&term)
                || option.title.to_ascii_lowercase().contains(&term)
                || option.subtitle.to_ascii_lowercase().contains(&term)
        })
        .cloned()
        .collect()
}

pub(super) fn apply_search_filter_completion(existing: &str, completion_token: &str) -> String {
    let mut parts = existing.split_whitespace().collect::<Vec<_>>();
    if parts.is_empty() {
        return format!("{completion_token} ");
    }

    parts.pop();
    parts.push(completion_token);

    format!("{} ", parts.join(" "))
}

/// Extract free-text search terms from a raw search query, filtering out
/// structured filter tokens like `from:alice` or `has:link`.
pub(super) fn message_search_terms(raw: &str) -> Vec<String> {
    raw.split_whitespace()
        .filter(|token| !token.contains(':'))
        .map(ToString::to_string)
        .filter(|token| !token.is_empty())
        .collect()
}

/// Return the contextual search placeholder for the given channel state.
pub(super) fn contextual_search_placeholder(
    current_channel: Option<&poly_client::Channel>,
    is_dm_channel: bool,
    is_group_channel: bool,
) -> String {
    if is_dm_channel {
        return t_args(
            "search-placeholder-user",
            &[(
                "user",
                current_channel.map_or("", |channel| channel.name.as_str()),
            )],
        );
    }
    if is_group_channel {
        return t_args(
            "search-placeholder-group",
            &[(
                "group",
                current_channel.map_or("", |channel| channel.name.as_str()),
            )],
        );
    }
    t_args(
        "search-placeholder-channel",
        &[(
            "channel",
            current_channel.map_or("", |channel| channel.name.as_str()),
        )],
    )
}

/// Parse a raw search query string into a `MessageSearchQuery`.
///
/// Structured tokens (`from:`, `in:#`, `has:link`, `mentions:me`) are
/// extracted into their corresponding query fields; the remaining
/// whitespace-separated tokens become the free-text `text` field.
pub(super) fn build_search_query(
    raw: &str,
    current_channel: Option<&poly_client::Channel>,
    current_server: Option<&poly_client::Server>,
    self_user_id: &str,
    is_dm_channel: bool,
    is_group_channel: bool,
) -> MessageSearchQuery {
    let mut query = MessageSearchQuery {
        text: String::new(),
        channel_id: if is_dm_channel || is_group_channel {
            current_channel.map(|channel| channel.id.clone())
        } else {
            None
        },
        server_id: if is_dm_channel || is_group_channel {
            None
        } else {
            current_server.map(|server| server.id.clone())
        },
        author_id: None,
        has_link: false,
        mentions_user_id: None,
        limit: Some(25),
    };
    let mut free_text = Vec::new();

    for token in raw.split_whitespace() {
        if let Some(author) = token.strip_prefix("from:") {
            if !author.is_empty() {
                query.author_id = Some(author.trim_start_matches('@').to_string());
            }
        } else if let Some(channel_name) = token.strip_prefix("in:") {
            if let Some(channel) = current_channel {
                let normalized = channel_name.trim_start_matches('#');
                if normalized.eq_ignore_ascii_case(&channel.name) {
                    query.channel_id = Some(channel.id.clone());
                }
            }
        } else if token.eq_ignore_ascii_case("has:link") {
            query.has_link = true;
        } else if token.eq_ignore_ascii_case("mentions:me") {
            query.mentions_user_id = Some(self_user_id.to_string());
        } else {
            free_text.push(token.to_string());
        }
    }

    query.text = free_text.join(" ");
    query
}

// ── Components ────────────────────────────────────────────────────────────────

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(none)]
#[component]
pub(super) fn SearchFilterPopup(
    suggestions: Vec<SearchFilterOption>,
    active_index: usize,
    on_append_filter: EventHandler<String>,
    on_close: EventHandler<()>,
) -> Element {
    rsx! {
        div { class: "search-filter-popup",
            div { class: "search-filter-popup-header",
                span { class: "search-filter-popup-title", {t("search-messages")} }
                button { class: "close-btn", onclick: move |_| on_close.call(()), "✕" }
            }
            div { class: "search-filter-list",
                for (index , suggestion) in suggestions.into_iter().enumerate() {
                    SearchFilterRow {
                        icon: suggestion.icon,
                        title: suggestion.title,
                        subtitle: suggestion.subtitle,
                        token: suggestion.completion_token,
                        selected: index == active_index,
                        on_click: move |token| on_append_filter.call(token),
                    }
                }
            }
        }
    }
}

#[rustfmt::skip]
#[ui_action(inherit)]
#[context_menu(inherit)]
#[component]
fn SearchFilterRow(
    icon: &'static str,
    title: String,
    subtitle: String,
    token: String,
    #[props(default)] selected: bool,
    on_click: EventHandler<String>,
) -> Element {
    rsx! {
        button {
            class: if selected { "search-filter-row selected" } else { "search-filter-row" },
            aria_selected: if selected { "true" } else { "false" },
            onclick: move |_| on_click.call(token.clone()),
            span { class: "search-filter-icon", "{icon}" }
            div { class: "search-filter-copy",
                div { class: "search-filter-title", "{title}" }
                div { class: "search-filter-subtitle", "{subtitle}" }
            }
        }
    }
}

// ── Render functions ──────────────────────────────────────────────────────────

// lint-allow-unused: Dioxus Key has too many variants to enumerate; explicit Escape/Arrow handling intentional
#[allow(clippy::wildcard_enum_match_arm)]
pub(super) fn handle_search_filter_keydown(
    evt: &KeyboardEvent,
    filtered_search_filter_options: &[SearchFilterOption],
    mut search_query: Signal<String>,
    mut active_search_filter_idx: Signal<usize>,
    mut show_search_filters: Signal<bool>,
    mut utility_panel: Signal<Option<ChatUtilityPanel>>,
) {
    if filtered_search_filter_options.is_empty() || !*show_search_filters.read() {
        if evt.key() == Key::Escape {
            show_search_filters.set(false);
        }
        return;
    }

    let item_count = filtered_search_filter_options.len();
    match evt.key() {
        Key::ArrowDown => {
            evt.prevent_default();
            let next = active_search_filter_idx.read().wrapping_add(1).checked_rem(item_count).unwrap_or(0);
            active_search_filter_idx.set(next);
        }
        Key::ArrowUp => {
            evt.prevent_default();
            let current = *active_search_filter_idx.read();
            let next = if current == 0 {
                item_count.saturating_sub(1)
            } else {
                current.saturating_sub(1)
            };
            active_search_filter_idx.set(next);
        }
        Key::Enter | Key::Tab => {
            evt.prevent_default();
            let current = (*active_search_filter_idx.read()).min(item_count.saturating_sub(1));
            if let Some(option) = filtered_search_filter_options.get(current) {
                let existing_query = search_query.read().clone();
                let next_query =
                    apply_search_filter_completion(&existing_query, &option.completion_token);
                search_query.set(next_query);
                active_search_filter_idx.set(0);
                show_search_filters.set(false);
                utility_panel.set(Some(ChatUtilityPanel::Search));
            }
        }
        Key::Escape => {
            evt.prevent_default();
            show_search_filters.set(false);
        }
        _ => {}
    }
}

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
pub(super) fn render_search_clear_button(
    search_query_value: String,
    mut search_query: Signal<String>,
    mut search_hits: Signal<Vec<MessageSearchHit>>,
    mut active_search_filter_idx: Signal<usize>,
    mut utility_panel: Signal<Option<ChatUtilityPanel>>,
    mut show_search_filters: Signal<bool>,
) -> Element {
    if search_query_value.is_empty() {
        return rsx! {};
    }

    rsx! {
        button {
            class: "chat-header-search-clear",
            title: t("action-clear"),
            onclick: move |_| {
                search_query.set(String::new());
                search_hits.set(Vec::new());
                active_search_filter_idx.set(0);
                utility_panel.set(Some(ChatUtilityPanel::Search));
                show_search_filters.set(false);
            },
            "✕"
        }
    }
}

// lint-allow-unused: by-value capture into rsx!/spawn closures (clone-into-spawn pattern)
#[allow(clippy::needless_pass_by_value)]
pub(super) fn render_chat_header_search(ctx: super::ChatViewMarkupCtx) -> Element {
    let search_placeholder = ctx.search_placeholder.clone();
    let search_query_input_value = ctx.search_query_input_value.clone();
    let search_query_value = ctx.search_query_value.clone();
    let filtered_search_filter_options = ctx.filtered_search_filter_options.clone();
    let search_filter_channel_name_onfocus = ctx.search_filter_channel_name_onfocus.clone();
    let search_filter_channel_name_oninput = ctx.search_filter_channel_name_oninput.clone();
    let mut search_query = ctx.search_query;
    let mut search_hits = ctx.search_hits;
    let mut active_search_filter_idx = ctx.active_search_filter_idx;
    let mut show_search_filters = ctx.show_search_filters;
    let mut utility_panel = ctx.utility_panel;

    rsx! {
        div { class: "chat-header-search-inline",
            input {
                class: "chat-header-search-input",
                r#type: "text",
                placeholder: "{search_placeholder}",
                value: "{search_query_input_value}",
                onfocus: move |_| {
                    let raw = search_query.read().clone();
                    let has_matches = !filter_search_filter_options(
                            &build_search_filter_options(&search_filter_channel_name_onfocus),
                            &raw,
                        )
                        .is_empty();
                    active_search_filter_idx.set(0);
                    show_search_filters.set(has_matches);
                    if !raw.trim().is_empty() {
                        utility_panel.set(Some(ChatUtilityPanel::Search));
                    }
                },
                oninput: move |evt| {
                    let next_value = evt.value();
                    let is_empty = next_value.trim().is_empty();
                    let has_matches = !filter_search_filter_options(
                            &build_search_filter_options(&search_filter_channel_name_oninput),
                            &next_value,
                        )
                        .is_empty();
                    search_query.set(next_value);
                    active_search_filter_idx.set(0);
                    show_search_filters.set(has_matches);
                    if is_empty {
                        search_hits.set(Vec::new());
                    }
                    utility_panel.set(Some(ChatUtilityPanel::Search));
                },
                onkeydown: move |evt: KeyboardEvent| {
                    handle_search_filter_keydown(
                        &evt,
                        &filtered_search_filter_options,
                        search_query,
                        active_search_filter_idx,
                        show_search_filters,
                        utility_panel,
                    );
                },
            }
            {
                render_search_clear_button(
                    search_query_value,
                    search_query,
                    search_hits,
                    active_search_filter_idx,
                    utility_panel,
                    show_search_filters,
                )
            }
            if *show_search_filters.read() && !filtered_search_filter_options.is_empty() {
                SearchFilterPopup {
                    suggestions: filtered_search_filter_options.clone(),
                    active_index: *active_search_filter_idx.read(),
                    on_append_filter: move |token: String| {
                        let next_value = apply_search_filter_completion(&search_query.read(), &token);
                        search_query.set(next_value);
                        active_search_filter_idx.set(0);
                        show_search_filters.set(false);
                        utility_panel.set(Some(ChatUtilityPanel::Search));
                    },
                    on_close: move |()| show_search_filters.set(false),
                }
            }
        }
    }
}
